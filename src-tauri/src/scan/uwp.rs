use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;

use crate::model::{ActionRef, ScanResult, Scope, SignatureStatus, SourceKind, StartupItem};
use crate::util::make_id;

const APP_MODEL: &str = r"Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\SystemAppData";

pub fn scan() -> ScanResult {
    let mut items = Vec::new();
    let mut errors = Vec::new();

    let root = match RegKey::predef(HKEY_CURRENT_USER).open_subkey(APP_MODEL) {
        Ok(root) => root,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return ScanResult {
                source: SourceKind::Uwp,
                items,
                error: None,
            };
        }
        Err(err) => {
            return ScanResult {
                source: SourceKind::Uwp,
                items,
                error: Some(format!("无法读取 UWP 启动任务: {err}")),
            };
        }
    };

    for pfn_entry in root.enum_keys() {
        let Ok(pfn) = pfn_entry else {
            continue;
        };
        let Ok(pfn_key) = root.open_subkey(&pfn) else {
            continue;
        };
        for task_entry in pfn_key.enum_keys() {
            let Ok(task_id) = task_entry else {
                continue;
            };
            let Ok(task_key) = pfn_key.open_subkey(&task_id) else {
                continue;
            };
            let state = match task_key.get_value::<u32, _>("State") {
                Ok(state) => state,
                // Most subkeys under a package are not startup tasks.
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
                Err(err) => {
                    errors.push(format!("{pfn}\\{task_id}: 无法读取启动状态（{err}）"));
                    continue;
                }
            };

            let (enabled, readonly) = match state {
                0 | 1 | 3 => (false, state == 3),
                2 => (true, false),
                4 => (true, true),
                _ => (false, true),
            };

            let mut warnings = Vec::new();
            if state == 3 || state == 4 {
                warnings.push("由组策略控制，工具不会修改".to_string());
            }

            let display_name = pfn
                .split('_')
                .next()
                .filter(|name| !name.is_empty())
                .unwrap_or(&pfn)
                .to_string();
            let origin = format!(r"HKCU\{APP_MODEL}\{pfn}\{task_id}");

            items.push(StartupItem {
                id: make_id(
                    SourceKind::Uwp,
                    Scope::CurrentUser,
                    None,
                    &format!("{pfn}\\{task_id}"),
                ),
                name: format!("{display_name} / {task_id}"),
                command: None,
                exe_path: None,
                publisher: None,
                signature: SignatureStatus::Unknown,
                source: SourceKind::Uwp,
                scope: Scope::CurrentUser,
                view: None,
                enabled,
                readonly,
                origin,
                warnings,
                action: if readonly {
                    None
                } else {
                    Some(ActionRef::UwpStartupTask {
                        pfn: pfn.clone(),
                        task_id,
                    })
                },
            });
        }
    }

    items.sort_by_key(|a| a.name.to_lowercase());

    ScanResult {
        source: SourceKind::Uwp,
        items,
        error: if errors.is_empty() {
            None
        } else {
            Some(errors.join("；"))
        },
    }
}
