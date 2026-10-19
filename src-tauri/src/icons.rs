use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use base64::Engine as _;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDIBits, GetObjectW, HBITMAP, HDC, HGDIOBJ,
};
use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL;
use windows::Win32::UI::Shell::{SHFILEINFOW, SHGFI_ICON, SHGetFileInfoW};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};
use windows::core::PCWSTR;

use crate::model::StartupItem;
use crate::util::is_remote_path;

static ICON_CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
static KNOWN_PATHS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn path_key(path: &Path) -> String {
    path.to_string_lossy().to_ascii_lowercase()
}

/// Registers executable paths seen by a scan. The UI may only request icons
/// for these paths, so a compromised renderer cannot probe arbitrary files.
pub fn register_paths(items: &[StartupItem]) {
    let known = KNOWN_PATHS.get_or_init(|| Mutex::new(HashSet::new()));
    let Ok(mut known) = known.lock() else {
        return;
    };
    if known.len() >= 4096 {
        known.clear();
    }
    for item in items {
        if let Some(path) = &item.exe_path {
            known.insert(path_key(path));
        }
    }
}

pub fn is_known_path(path: &Path) -> bool {
    KNOWN_PATHS
        .get_or_init(|| Mutex::new(HashSet::new()))
        .lock()
        .ok()
        .is_some_and(|known| known.contains(&path_key(path)))
}

/// Returns a PNG data URL for the file's shell icon. Results are cached per
/// path because many startup items share the same executable.
pub fn data_url_for(path: &Path) -> Option<String> {
    if is_remote_path(path) {
        return None;
    }
    let key = path.to_string_lossy().to_ascii_lowercase();
    let cache = ICON_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(cached) = cache.lock().ok().and_then(|map| map.get(&key).cloned()) {
        return cached;
    }

    let value = extract_icon_png(path).map(|png| {
        let encoded = base64::engine::general_purpose::STANDARD.encode(png);
        format!("data:image/png;base64,{encoded}")
    });
    // Only cache successes so a temporarily unavailable file is retried by
    // the next scan instead of staying blank until the process restarts.
    if let Some(ref icon) = value
        && let Ok(mut map) = cache.lock()
    {
        if map.len() >= 512 {
            map.clear();
        }
        map.insert(key, Some(icon.clone()));
    }
    value
}

struct GdiResources {
    icon: HICON,
    color: HBITMAP,
    mask: HBITMAP,
    dc: HDC,
}

impl Drop for GdiResources {
    fn drop(&mut self) {
        unsafe {
            if !self.color.0.is_null() {
                let _ = DeleteObject(self.color.into());
            }
            if !self.mask.0.is_null() {
                let _ = DeleteObject(self.mask.into());
            }
            if !self.icon.0.is_null() {
                let _ = DestroyIcon(self.icon);
            }
            if !self.dc.is_invalid() {
                let _ = DeleteDC(self.dc);
            }
        }
    }
}

fn extract_icon_png(path: &Path) -> Option<Vec<u8>> {
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let mut file_info = SHFILEINFOW::default();
    let result = unsafe {
        SHGetFileInfoW(
            PCWSTR(wide.as_ptr()),
            FILE_ATTRIBUTE_NORMAL,
            Some(&mut file_info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON,
        )
    };
    if result == 0 || file_info.hIcon.0.is_null() {
        return None;
    }

    let mut icon_info = ICONINFO::default();
    if unsafe { GetIconInfo(file_info.hIcon, &mut icon_info) }.is_err() {
        unsafe {
            let _ = DestroyIcon(file_info.hIcon);
        }
        return None;
    }

    let resources = GdiResources {
        icon: file_info.hIcon,
        color: icon_info.hbmColor,
        mask: icon_info.hbmMask,
        dc: unsafe { CreateCompatibleDC(None) },
    };
    if resources.dc.is_invalid() || resources.color.0.is_null() {
        return None;
    }

    let mut bitmap = BITMAP::default();
    let copied = unsafe {
        GetObjectW(
            HGDIOBJ(resources.color.0),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bitmap as *mut BITMAP as *mut c_void),
        )
    };
    if copied == 0 || bitmap.bmWidth <= 0 || bitmap.bmHeight <= 0 {
        return None;
    }

    let width = bitmap.bmWidth as u32;
    let height = bitmap.bmHeight as u32;
    let mut pixels = vec![0u8; width as usize * height as usize * 4];

    let mut color_info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let lines = unsafe {
        GetDIBits(
            resources.dc,
            resources.color,
            0,
            height,
            Some(pixels.as_mut_ptr().cast()),
            &mut color_info,
            DIB_RGB_COLORS,
        )
    };
    if lines == 0 {
        return None;
    }

    // GetDIBits returns BGRA. Keep the alpha channel when the icon has one;
    // otherwise derive opacity from the 1-bit mask.
    let mut mask_pixels = Vec::new();
    let mask_stride = width.div_ceil(32) * 4;
    if !resources.mask.0.is_null() {
        let mut mask_info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 1,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        mask_pixels.resize(mask_stride as usize * height as usize, 0);
        let mask_lines = unsafe {
            GetDIBits(
                resources.dc,
                resources.mask,
                0,
                height,
                Some(mask_pixels.as_mut_ptr().cast()),
                &mut mask_info,
                DIB_RGB_COLORS,
            )
        };
        if mask_lines == 0 {
            mask_pixels.clear();
        }
    }

    let has_alpha = pixels.as_chunks::<4>().0.iter().any(|pixel| pixel[3] != 0);
    for y in 0..height as usize {
        for x in 0..width as usize {
            let index = (y * width as usize + x) * 4;
            pixels.swap(index, index + 2);
            let transparent = if mask_pixels.is_empty() {
                false
            } else {
                let byte = mask_pixels[y * mask_stride as usize + x / 8];
                (byte >> (7 - (x % 8))) & 1 != 0
            };
            if transparent {
                pixels[index + 3] = 0;
            } else if !has_alpha {
                pixels[index + 3] = 255;
            }
        }
    }

    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&pixels).ok()?;
    }
    Some(png_bytes)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{is_known_path, register_paths};
    use crate::model::{Scope, SignatureStatus, SourceKind, StartupItem};

    #[test]
    fn allowlist_accepts_only_registered_paths() {
        let item = StartupItem {
            id: "test".to_string(),
            name: "Test".to_string(),
            command: None,
            exe_path: Some(PathBuf::from(r"C:\Windows\notepad.exe")),
            publisher: None,
            signature: SignatureStatus::Unknown,
            source: SourceKind::RegistryRun,
            scope: Scope::CurrentUser,
            view: None,
            enabled: true,
            readonly: false,
            origin: "test".to_string(),
            warnings: Vec::new(),
            action: None,
        };
        register_paths(&[item]);
        assert!(is_known_path(Path::new(r"C:\WINDOWS\NOTEPAD.EXE")));
        assert!(!is_known_path(Path::new(r"C:\Windows\other.exe")));
    }
}
