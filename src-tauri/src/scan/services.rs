use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY};

use crate::model::{ScanResult, Scope, SourceKind, StartupItem};
use crate::signature;
use crate::util::{decode_reg_string, expand_env, make_id, parse_executable};

const SERVICES_KEY: &str = r"SYSTEM\CurrentControlSet\Services";

pub fn scan() -> ScanResult {
    scan_with(true)
}

pub(crate) fn scan_with(metadata: bool) -> ScanResult {
    let mut items = Vec::new();
    let mut errors = Vec::new();

    let root = match winreg::RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(SERVICES_KEY, KEY_READ | KEY_WOW64_64KEY)
    {
        Ok(root) => root,
        Err(err) => {
            return ScanResult {
                source: SourceKind::Service,
                items,
                error: Some(format!("无法读取服务注册表: {err}")),
            };
        }
    };

    for entry in root.enum_keys() {
        let Ok(name) = entry else {
            continue;
        };
        // A skipped key must be reported: snapshot completeness relies on this
        // error field, and a silent gap would make a missing service look like
        // a "removed" item in the diff.
        let key = match root.open_subkey_with_flags(&name, KEY_READ | KEY_WOW64_64KEY) {
            Ok(key) => key,
            Err(err) => {
                errors.push(format!("{name}: 无法打开服务子键（{err}）"));
                continue;
            }
        };
        let start = match key.get_value::<u32, _>("Start") {
            Ok(start) => start,
            // Subkeys without Start are registry artifacts, not services.
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => {
                errors.push(format!("{name}: 无法读取启动类型（{err}）"));
                continue;
            }
        };
        if !matches!(start, 0..=2) {
            continue;
        }
        let service_type = key.get_value::<u32, _>("Type").unwrap_or(0x10);
        if service_type & 0x30 == 0 {
            continue;
        }

        let display_name = read_string(&key, "DisplayName")
            .map(|value| crate::util::shell_display_text(&value).unwrap_or(value))
            .unwrap_or_else(|| name.clone());
        let image_path = read_string(&key, "ImagePath")
            .map(|value| normalize_image_path(&value))
            .unwrap_or_default();
        let delayed = key
            .get_value::<u32, _>("DelayedAutoStart")
            .map(|value| value != 0)
            .unwrap_or(false);

        let exe_path = parse_executable(&image_path).map(|path| {
            if path.is_absolute() {
                path
            } else {
                // Non-fully-qualified ImagePath values are relative to the
                // Windows directory according to CreateService semantics.
                let relative = path.to_string_lossy();
                crate::util::windows_dir().join(relative.trim_start_matches('\\'))
            }
        });
        let (publisher, signature_status) = signature::describe(exe_path.as_deref(), metadata);

        let mut warnings = vec!["服务列表只读，暂不支持修改".to_string()];
        if delayed {
            warnings.push("延迟自动启动".to_string());
        }

        items.push(StartupItem {
            id: make_id(SourceKind::Service, Scope::AllUsers, None, &name),
            name: display_name,
            command: if image_path.is_empty() {
                None
            } else {
                Some(image_path)
            },
            exe_path,
            publisher,
            signature: signature_status,
            source: SourceKind::Service,
            scope: Scope::AllUsers,
            view: None,
            enabled: true,
            readonly: true,
            machine_wide: false,
            origin: name,
            warnings,
            action: None,
        });
    }

    items.sort_by_key(|a| a.name.to_lowercase());

    ScanResult {
        source: SourceKind::Service,
        items,
        error: if errors.is_empty() {
            None
        } else {
            Some(errors.join("；"))
        },
    }
}

fn normalize_image_path(raw: &str) -> String {
    let mut value = raw.trim().to_string();
    let lower = value.to_ascii_lowercase();
    if lower.starts_with(r"\systemroot\") {
        value = format!("%SystemRoot%\\{}", &value[12..]);
    } else if lower.starts_with(r"systemroot\") {
        value = format!("%SystemRoot%\\{}", &value[11..]);
    }
    if let Some(rest) = value.strip_prefix(r"\??\") {
        value = rest.to_string();
    }
    expand_env(&value)
}

fn read_string(key: &winreg::RegKey, name: &str) -> Option<String> {
    decode_reg_string(&key.get_raw_value(name).ok()?)
}
