use winreg::enums::{KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY};

use crate::error::AppError;
use crate::model::{ActionRef, Hive, RegView, ScanResult, Scope, SourceKind, StartupItem};
use crate::signature;
use crate::startup_approved::{self, ApprovedState};
use crate::util::{decode_reg_string, make_id, parse_executable};

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_ONCE: &str = r"Software\Microsoft\Windows\CurrentVersion\RunOnce";

pub fn scan() -> ScanResult {
    scan_with(true)
}

pub(crate) fn scan_with(metadata: bool) -> ScanResult {
    let mut items = Vec::new();
    let mut errors = Vec::new();

    for hive in [Hive::Hkcu, Hive::Hklm] {
        // HKCU's Run key is shared by 32-bit and 64-bit applications; only
        // HKLM has a separate WOW6432Node view. Scanning HKCU twice would
        // produce duplicate rows with a bogus "32-bit" state.
        let views: &[RegView] = match hive {
            Hive::Hkcu => &[RegView::X64],
            Hive::Hklm => &[RegView::X64, RegView::X86],
        };
        for &view in views {
            for (subkey, once) in [(RUN, false), (RUN_ONCE, true)] {
                match scan_key(hive, view, subkey, once, metadata) {
                    Ok(found) => items.extend(found),
                    Err(err) => {
                        let mut message = format!("{} {}: {}", hive.label(), subkey, err.message);
                        if let Some(detail) = err.detail {
                            message.push_str(&format!("（{detail}）"));
                        }
                        errors.push(message);
                    }
                }
            }
        }
    }

    items.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.origin.cmp(&b.origin))
    });

    ScanResult {
        source: SourceKind::RegistryRun,
        items,
        error: if errors.is_empty() {
            None
        } else {
            Some(errors.join("；"))
        },
    }
}

fn scan_key(
    hive: Hive,
    view: RegView,
    subkey: &str,
    once: bool,
    metadata: bool,
) -> Result<Vec<StartupItem>, AppError> {
    let flags = KEY_READ
        | match view {
            RegView::X64 => KEY_WOW64_64KEY,
            RegView::X86 => KEY_WOW64_32KEY,
        };
    let key = match hive.key().open_subkey_with_flags(subkey, flags) {
        Ok(key) => key,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            return Err(AppError::registry("无法读取启动项注册表", err));
        }
    };

    let mut items = Vec::new();
    for entry in key.enum_values() {
        let Ok((raw_name, value)) = entry else {
            continue;
        };
        let name = if raw_name.is_empty() {
            "(默认)".to_string()
        } else {
            raw_name.clone()
        };
        let Some(command) = decode_reg_string(&value) else {
            continue;
        };
        if command.trim().is_empty() {
            continue;
        }

        let exe_path = parse_executable(&command);
        let overlay = startup_approved::overlay_subkey(view);
        let mut approved_hive = hive;
        let mut state = ApprovedState::Enabled;
        let mut warnings = Vec::new();

        if !once {
            match startup_approved::try_read_state_opt(hive, overlay, &raw_name) {
                Ok(Some(value)) => state = value,
                Ok(None) => {}
                Err(err) => warnings.push(format!("无法读取启动状态：{}", err.message)),
            }
        }

        // Explorer applies a per-user StartupApproved value on top of HKLM
        // entries. Reflect that effective state and write it back to HKCU so a
        // disable actually takes effect without touching the machine value.
        if hive == Hive::Hklm && !once {
            match startup_approved::try_read_state_opt(Hive::Hkcu, overlay, &raw_name) {
                Ok(Some(per_user)) => {
                    state = per_user;
                    approved_hive = Hive::Hkcu;
                    warnings.push("当前用户对此项有单独设置".to_string());
                }
                Ok(None) => {}
                Err(err) => {
                    warnings.push(format!("无法读取当前用户覆盖状态：{}", err.message));
                }
            }
        }
        if state == ApprovedState::Unknown {
            warnings.push("启动状态数据异常，按启用显示".to_string());
        }
        if once {
            warnings.push("RunOnce 仅执行一次，不支持禁用".to_string());
        }

        let scope = if hive == Hive::Hkcu {
            Scope::CurrentUser
        } else {
            Scope::AllUsers
        };
        let locator = format!("{}\\{}|{}", hive.label(), subkey, raw_name);
        let (publisher, signature_status) = signature::describe(exe_path.as_deref(), metadata);
        items.push(StartupItem {
            id: make_id(SourceKind::RegistryRun, scope, Some(view), &locator),
            name,
            command: Some(command),
            exe_path,
            publisher,
            signature: signature_status,
            source: SourceKind::RegistryRun,
            scope,
            view: (hive == Hive::Hklm).then_some(view),
            enabled: state.is_enabled(),
            readonly: once,
            origin: format!("{}\\{}", hive.label(), subkey),
            warnings,
            action: if once {
                None
            } else {
                Some(ActionRef::StartupApproved {
                    hive: approved_hive,
                    subkey: overlay.to_string(),
                    value_name: raw_name,
                })
            },
        });
    }
    Ok(items)
}
