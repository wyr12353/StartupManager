use std::path::{Path, PathBuf};

use crate::error::AppError;
use crate::model::{ActionRef, Hive, ScanResult, Scope, SignatureStatus, SourceKind, StartupItem};
use crate::signature;
use crate::startup_approved::{self, ApprovedState};
use crate::util::make_id;

pub fn scan() -> ScanResult {
    scan_with(true, true)
}

pub(crate) fn scan_with(metadata: bool, resolve_links: bool) -> ScanResult {
    let mut items = Vec::new();
    let mut errors = Vec::new();

    use windows::Win32::UI::Shell::{FOLDERID_ProgramData, FOLDERID_RoamingAppData};

    let startup = r"Microsoft\Windows\Start Menu\Programs\Startup";
    let user_dir = crate::util::known_folder_path(&FOLDERID_RoamingAppData)
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
        .map(|root| root.join(startup));
    let common_dir = crate::util::known_folder_path(&FOLDERID_ProgramData)
        .or_else(|| std::env::var_os("PROGRAMDATA").map(PathBuf::from))
        .map(|root| root.join(startup));

    for (dir, hive, scope) in [
        (user_dir, Hive::Hkcu, Scope::CurrentUser),
        (common_dir, Hive::Hklm, Scope::AllUsers),
    ] {
        let Some(dir) = dir else {
            continue;
        };
        match scan_dir(&dir, hive, scope, metadata, resolve_links) {
            Ok(mut found) => items.append(&mut found),
            Err(err) => errors.push(format!("{}: {}", dir.display(), err.message)),
        }
    }

    items.sort_by_key(|a| a.name.to_lowercase());

    ScanResult {
        source: SourceKind::StartupFolder,
        items,
        error: if errors.is_empty() {
            None
        } else {
            Some(errors.join("；"))
        },
    }
}

fn scan_dir(
    dir: &Path,
    hive: Hive,
    scope: Scope,
    metadata: bool,
    resolve_links: bool,
) -> Result<Vec<StartupItem>, AppError> {
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};

    let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
    let result = scan_dir_inner(dir, hive, scope, metadata, resolve_links);
    if initialized {
        unsafe { CoUninitialize() };
    }
    result
}

fn scan_dir_inner(
    dir: &Path,
    hive: Hive,
    scope: Scope,
    metadata: bool,
    resolve_links: bool,
) -> Result<Vec<StartupItem>, AppError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(AppError::io("无法读取启动文件夹", err)),
    };

    let mut items = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        // Do not follow links: a link in the user startup folder could point
        // at a network share and block the elevated helper.
        if !file_type.is_file() {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy().to_string();
        if file_name.eq_ignore_ascii_case("desktop.ini") {
            continue;
        }
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| file_name.clone());

        let value_name = startup_approved::find_folder_value_name(hive, &file_name, &stem);
        let mut approved_hive = hive;
        let mut approved_value_name = value_name.clone();
        let mut state = ApprovedState::Enabled;
        let mut warnings = Vec::new();

        match startup_approved::try_read_state_opt(
            hive,
            startup_approved::APPROVED_STARTUP_FOLDER,
            &value_name,
        ) {
            Ok(Some(value)) => state = value,
            Ok(None) => {}
            Err(err) => warnings.push(format!("无法读取启动状态：{}", err.message)),
        }

        // The per-user overlay also applies to the common startup folder.
        if scope == Scope::AllUsers {
            let per_user_name =
                startup_approved::find_folder_value_name(Hive::Hkcu, &file_name, &stem);
            match startup_approved::try_read_state_opt(
                Hive::Hkcu,
                startup_approved::APPROVED_STARTUP_FOLDER,
                &per_user_name,
            ) {
                Ok(Some(per_user)) => {
                    state = per_user;
                    approved_hive = Hive::Hkcu;
                    approved_value_name = per_user_name;
                    warnings.push("当前用户对此项有单独设置".to_string());
                }
                Ok(None) => warnings.push("公共启动文件夹的修改需要管理员权限".to_string()),
                Err(err) => {
                    warnings.push(format!("无法读取当前用户覆盖状态：{}", err.message));
                }
            }
        }
        if state == ApprovedState::Unknown {
            warnings.push("启动状态数据异常，按启用显示".to_string());
        }

        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase());
        let (target, arguments) = if resolve_links && extension.as_deref() == Some("lnk") {
            resolve_lnk(&path).unwrap_or((None, None))
        } else {
            (None, None)
        };
        let exe_path = match extension.as_deref() {
            Some("exe" | "bat" | "cmd" | "vbs" | "ps1") => Some(path.clone()),
            _ => target.as_ref().map(PathBuf::from),
        };
        let command = match target {
            Some(target) => match arguments.filter(|args| !args.trim().is_empty()) {
                Some(args) => format!("{target} {args}"),
                None => target,
            },
            None => path.to_string_lossy().to_string(),
        };

        let signature_status = if metadata {
            exe_path
                .as_deref()
                .map(signature::verify)
                .unwrap_or(SignatureStatus::Unknown)
        } else {
            SignatureStatus::Unknown
        };
        let publisher = if metadata {
            exe_path.as_deref().and_then(signature::publisher_for)
        } else {
            None
        };
        let locator = path.to_string_lossy().to_string();
        items.push(StartupItem {
            id: make_id(SourceKind::StartupFolder, scope, None, &locator),
            name: stem,
            command: Some(command),
            exe_path,
            publisher,
            signature: signature_status,
            source: SourceKind::StartupFolder,
            scope,
            view: None,
            enabled: state.is_enabled(),
            readonly: false,
            origin: locator,
            warnings,
            action: Some(ActionRef::StartupApproved {
                hive: approved_hive,
                subkey: startup_approved::APPROVED_STARTUP_FOLDER.to_string(),
                value_name: approved_value_name,
            }),
        });
    }
    Ok(items)
}

fn wide_to_string(buffer: &[u16]) -> String {
    let len = buffer
        .iter()
        .position(|ch| *ch == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..len])
}

fn resolve_lnk(path: &Path) -> Option<(Option<String>, Option<String>)> {
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile, STGM_READ,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    use windows::core::{HSTRING, Interface};

    let link: IShellLinkW =
        unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }.ok()?;
    let persist: IPersistFile = link.cast().ok()?;
    unsafe { persist.Load(&HSTRING::from(path.to_string_lossy().as_ref()), STGM_READ) }.ok()?;

    let mut buffer = vec![0u16; 32768];
    unsafe { link.GetPath(&mut buffer, std::ptr::null_mut(), 0) }.ok()?;
    let target = wide_to_string(&buffer);
    let target = if target.is_empty() {
        None
    } else {
        Some(target)
    };

    let mut args_buffer = vec![0u16; 32768];
    let arguments = unsafe { link.GetArguments(&mut args_buffer) }
        .ok()
        .map(|()| wide_to_string(&args_buffer))
        .filter(|args| !args.is_empty());

    if target.is_none() && arguments.is_none() {
        return None;
    }
    Some((target, arguments))
}
