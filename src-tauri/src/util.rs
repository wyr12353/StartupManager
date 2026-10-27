use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use winreg::RegValue;
use winreg::enums::{REG_EXPAND_SZ, REG_SZ};

use crate::model::{RegView, Scope, SourceKind};

pub fn make_id(kind: SourceKind, scope: Scope, view: Option<RegView>, locator: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{kind:?}|{scope:?}|{view:?}|{locator}"));
    let digest = hasher.finalize();
    let mut out = String::with_capacity(32);
    for byte in &digest[..16] {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

pub fn expand_env(input: &str) -> String {
    use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
    use windows::core::PCWSTR;

    let wide: Vec<u16> = input.encode_utf16().chain(std::iter::once(0)).collect();
    let needed = unsafe { ExpandEnvironmentStringsW(PCWSTR(wide.as_ptr()), None) };
    if needed == 0 {
        return input.to_string();
    }
    let mut buffer = vec![0u16; needed as usize];
    let written = unsafe { ExpandEnvironmentStringsW(PCWSTR(wide.as_ptr()), Some(&mut buffer)) };
    if written == 0 || written as usize > buffer.len() {
        return input.to_string();
    }
    let len = written.saturating_sub(1) as usize;
    String::from_utf16_lossy(&buffer[..len]).to_string()
}

/// Resolves a Windows known folder without trusting environment variables.
pub fn known_folder_path(id: &windows::core::GUID) -> Option<PathBuf> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{KF_FLAG_DEFAULT, SHGetKnownFolderPath};

    unsafe {
        let ptr = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
        let path = ptr.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(ptr.0 as *const core::ffi::c_void));
        path
    }
}

pub fn windows_dir() -> PathBuf {
    use windows::Win32::UI::Shell::FOLDERID_Windows;

    known_folder_path(&FOLDERID_Windows)
        .or_else(|| std::env::var_os("SystemRoot").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
}

/// Version queries and WinVerifyTrust can block for seconds on a network
/// share and leak NTLM credentials to an attacker-controlled host. Device
/// paths bypass normal Win32 path handling and are equally unsafe to probe.
pub fn is_remote_path(path: &Path) -> bool {
    let value = path
        .to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase();
    if is_remote_drive(&value)
        || value.starts_with(r"\\.\")
        || value.starts_with(r"\\?\globalroot\")
        || value.starts_with(r"\\?\volume")
    {
        return true;
    }
    match value.strip_prefix(r"\\?\") {
        Some(rest) => rest.starts_with("unc\\"),
        None => value.starts_with(r"\\"),
    }
}

fn is_remote_drive(value: &str) -> bool {
    use windows::Win32::Storage::FileSystem::GetDriveTypeW;
    use windows::Win32::System::WindowsProgramming::DRIVE_REMOTE;
    use windows::core::PCWSTR;

    let bytes = value.as_bytes();
    if bytes.len() < 2 || bytes[1] != b':' || !bytes[0].is_ascii_alphabetic() {
        return false;
    }
    let root = [bytes[0] as u16, ':' as u16, '\\' as u16, 0];
    unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) == DRIVE_REMOTE }
}

/// Decodes a REG_SZ / REG_EXPAND_SZ value. Returns `None` for other types and
/// for empty strings so callers can skip them uniformly.
pub fn decode_reg_string(value: &RegValue) -> Option<String> {
    if value.vtype != REG_SZ && value.vtype != REG_EXPAND_SZ {
        return None;
    }
    let units: Vec<u16> = value
        .bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();
    let text = String::from_utf16_lossy(&units)
        .trim_end_matches('\0')
        .to_string();
    if text.is_empty() { None } else { Some(text) }
}

/// Resolves a shell display string such as `@%SystemRoot%\system32\shell32.dll,-8506`.
/// `SHLoadIndirectString` can load the referenced module, so only modules under
/// the Windows directory or Program Files are accepted. Plain strings are
/// returned unchanged; an empty, `ms-resource:` or untrusted indirect string
/// yields `None`.
pub fn shell_display_text(raw: &str) -> Option<String> {
    let value = expand_env(raw.trim());
    if value.is_empty() || value.to_ascii_lowercase().contains("ms-resource:") {
        return None;
    }
    if !value.starts_with('@') {
        return Some(value);
    }
    let path = indirect_resource_path(&value)?;
    if !is_trusted_resource(&path) {
        return None;
    }
    resolve_indirect_string(&value)
}

/// Extracts the module path from an indirect string. `@{...}` forms resolve
/// through a package identity and are not handled here.
fn indirect_resource_path(value: &str) -> Option<PathBuf> {
    let rest = value.strip_prefix('@')?;
    if rest.starts_with('{') {
        return None;
    }
    let comma = rest.rfind(',')?;
    let raw = rest[..comma].trim().trim_matches('"');
    if raw.is_empty() {
        return None;
    }
    let expanded = PathBuf::from(expand_env(raw));
    if expanded.is_absolute() {
        Some(expanded)
    } else {
        let candidate = windows_dir().join("System32").join(expanded);
        candidate.is_file().then_some(candidate)
    }
}

fn is_trusted_resource(path: &Path) -> bool {
    // `C:\Windows\..\Users\evil\x.dll` still carries the trusted prefix.
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return false;
    }
    if !path.is_file() {
        return false;
    }
    let value = path
        .to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase();
    let mut roots = vec![windows_dir().to_string_lossy().to_string()];
    for name in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(root) = std::env::var_os(name) {
            roots.push(PathBuf::from(root).to_string_lossy().to_string());
        }
    }
    roots.iter().any(|root| {
        let root = root
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_ascii_lowercase();
        // Compare whole path components so `C:\WindowsApps` is not treated as
        // a child of `C:\Windows`.
        !root.is_empty() && (value == root || value.starts_with(&format!("{root}\\")))
    })
}

fn resolve_indirect_string(value: &str) -> Option<String> {
    use windows::Win32::UI::Shell::SHLoadIndirectString;
    use windows::core::PCWSTR;

    let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    let mut output = vec![0u16; 8192];
    let result = unsafe { SHLoadIndirectString(PCWSTR(wide.as_ptr()), &mut output, None) };
    if result.is_err() {
        return None;
    }
    let text = String::from_utf16_lossy(&output)
        .trim_end_matches('\0')
        .trim()
        .to_string();
    (!text.is_empty()).then_some(text)
}

pub fn parse_executable(command: &str) -> Option<PathBuf> {
    let expanded = expand_env(command);
    let trimmed = expanded.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(rest) = trimmed.strip_prefix('"') {
        let candidate = rest.split('"').next().unwrap_or(rest).trim();
        return (!candidate.is_empty()).then(|| PathBuf::from(candidate));
    }

    // Unquoted command lines may contain spaces (`C:\Program Files\...`).
    // Use the first boundary-valid executable extension so arguments that
    // contain another path are not swallowed;
    // otherwise fall back to the first whitespace-delimited token.
    let lower = trimmed.to_ascii_lowercase();
    let mut cut: Option<usize> = None;
    for ext in [".exe", ".com", ".bat", ".cmd", ".ps1", ".vbs", ".scr"] {
        let mut from = 0;
        while let Some(pos) = lower[from..].find(ext) {
            let end = from + pos + ext.len();
            let boundary = lower
                .as_bytes()
                .get(end)
                .is_none_or(|byte| matches!(byte, b' ' | b'\t' | b'"'));
            if boundary {
                cut = Some(cut.map_or(end, |current: usize| current.min(end)));
            }
            from = end;
        }
    }
    let candidate = match cut {
        Some(end) => &trimmed[..end],
        None => trimmed.split_whitespace().next().unwrap_or(trimmed),
    };

    let candidate = candidate.trim();
    if candidate.is_empty() {
        None
    } else {
        Some(PathBuf::from(candidate))
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_reg_string, expand_env, is_remote_path, parse_executable};
    use std::path::Path;
    use winreg::RegValue;
    use winreg::enums::{REG_BINARY, REG_SZ};

    #[test]
    fn resolves_only_safe_display_strings() {
        assert_eq!(
            super::shell_display_text("  Notepad  ").as_deref(),
            Some("Notepad")
        );
        assert_eq!(super::shell_display_text("   "), None);
        assert_eq!(super::shell_display_text("ms-resource://foo"), None);
        assert_eq!(
            super::shell_display_text(r"@C:\Users\Public\missing.dll,-1"),
            None
        );
    }

    #[test]
    fn accepts_only_resources_inside_trusted_roots() {
        let system32 = super::windows_dir().join("System32");
        let shell32 = system32.join("shell32.dll");
        if shell32.is_file() {
            assert!(super::is_trusted_resource(&shell32));
            assert!(
                !super::is_trusted_resource(
                    &super::windows_dir()
                        .join("..")
                        .join("Windows")
                        .join("System32")
                        .join("shell32.dll")
                ),
                "parent-directory traversal must be rejected"
            );
        }
        assert!(!super::is_trusted_resource(Path::new(
            r"C:\WindowsApps\missing.dll"
        )));
    }

    #[test]
    fn parses_quoted_and_unquoted_paths() {
        let cases = [
            (
                r#""C:\Program Files\App\a.exe" -x"#,
                r"C:\Program Files\App\a.exe",
            ),
            (
                r"C:\Program Files\App\a.exe -x",
                r"C:\Program Files\App\a.exe",
            ),
            (
                r"C:\Windows\System32\cmd.exe",
                r"C:\Windows\System32\cmd.exe",
            ),
            (r"C:\tools\run.bat /silent", r"C:\tools\run.bat"),
        ];
        for (input, expected) in cases {
            assert_eq!(
                parse_executable(input).as_deref(),
                Some(Path::new(expected)),
                "input {input}"
            );
        }
    }

    #[test]
    fn stops_at_the_first_executable() {
        let cases = [
            (
                r"C:\Tools\app.exe --run C:\Other\helper.exe",
                r"C:\Tools\app.exe",
            ),
            (
                r#"C:\Windows\System32\cmd.exe /c "C:\x\y.exe""#,
                r"C:\Windows\System32\cmd.exe",
            ),
            (
                r"C:\Windows\System32\rundll32.exe shell32.dll,Control_RunDLL",
                r"C:\Windows\System32\rundll32.exe",
            ),
            (
                r"C:\dir.with.exe\app.exe --flag",
                r"C:\dir.with.exe\app.exe",
            ),
            (r"C:\a.exe.bat /x", r"C:\a.exe.bat"),
        ];
        for (input, expected) in cases {
            assert_eq!(
                parse_executable(input).as_deref(),
                Some(Path::new(expected)),
                "input {input}"
            );
        }
    }

    #[test]
    fn expands_environment_variables() {
        let expanded = expand_env(r"%SystemRoot%\System32");
        assert!(!expanded.contains("%SystemRoot%"));
        assert!(expanded.to_ascii_lowercase().ends_with("\\system32"));
        assert_eq!(
            expand_env("%STARTUPMANAGER_NOT_SET%"),
            "%STARTUPMANAGER_NOT_SET%"
        );
    }

    #[test]
    fn detects_remote_and_device_paths() {
        for value in [
            r"\\server\share\a.exe",
            r"//server/share/a.exe",
            r"\\?\UNC\server\share\a.exe",
            r"\\.\PhysicalDrive0",
            r"\\?\GLOBALROOT\Device\HarddiskVolume1\a.exe",
            r"\\?\Volume{00000000-0000-0000-0000-000000000000}\a.exe",
        ] {
            assert!(is_remote_path(Path::new(value)), "path {value}");
        }
        for value in [
            r"C:\Windows\a.exe",
            r"\\?\C:\Windows\a.exe",
            r"C:/Windows/a.exe",
        ] {
            assert!(!is_remote_path(Path::new(value)), "path {value}");
        }
    }

    #[test]
    fn decodes_registry_strings() {
        let mut bytes: Vec<u8> = "hello".encode_utf16().flat_map(u16::to_le_bytes).collect();
        bytes.extend_from_slice(&[0, 0]);
        let value = RegValue {
            bytes: bytes.into(),
            vtype: REG_SZ,
        };
        assert_eq!(decode_reg_string(&value).as_deref(), Some("hello"));

        let empty = RegValue {
            bytes: Vec::new().into(),
            vtype: REG_SZ,
        };
        assert_eq!(decode_reg_string(&empty), None);

        let binary = RegValue {
            bytes: vec![1, 2, 3].into(),
            vtype: REG_BINARY,
        };
        assert_eq!(decode_reg_string(&binary), None);
    }
}
