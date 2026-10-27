use std::io::Write;

use serde::Serialize;
use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WOW64_64KEY, KEY_WRITE};

use crate::error::{AppError, AppResult};
use crate::model::{ActionRef, Hive, StartupItem};
use crate::paths;
use crate::scan::context_menu::{BLOCKED, BLOCKED_MARKER, VERB_MARKER};
use crate::scan::tasks;
use crate::scan::uwp::APP_MODEL;
use crate::startup_approved;
use crate::util::decode_reg_string;

const PROGRAMMATIC_ACCESS_ONLY: &str = "ProgrammaticAccessOnly";
const LEGACY_DISABLE: &str = "LegacyDisable";

pub fn set_enabled(item: &StartupItem, enabled: bool) -> AppResult<()> {
    if item.readonly {
        return Err(AppError::unsupported("该项目为只读，工具不会修改"));
    }
    let Some(action) = item.action.as_ref() else {
        return Err(AppError::unsupported("该启动项没有可用的修改方式"));
    };

    match action {
        ActionRef::StartupApproved {
            hive,
            subkey,
            value_name,
        } => {
            startup_approved::write_state(*hive, subkey, value_name, enabled)?;
        }
        ActionRef::UwpStartupTask { pfn, task_id } => {
            set_uwp_state(pfn, task_id, enabled)?;
        }
        ActionRef::ScheduledTask { path } => {
            tasks::set_enabled(path, enabled)?;
        }
        ActionRef::ContextMenuVerb { hive, subkey } => {
            set_context_menu_verb(*hive, subkey, enabled)?;
        }
        ActionRef::ShellExtensionBlocked { clsid, hive } => {
            set_shell_ext_blocked(clsid, *hive, enabled)?;
        }
    }

    tracing::info!(
        id = %item.id,
        name = %item.name,
        source = ?item.source,
        enabled,
        "startup item updated"
    );
    Ok(())
}

fn set_uwp_state(pfn: &str, task_id: &str, enabled: bool) -> AppResult<()> {
    let path = format!(r"{APP_MODEL}\{pfn}\{task_id}");
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(&path, KEY_READ | KEY_WRITE)
        .map_err(|e| AppError::registry("无法打开 UWP 启动任务", e))?;

    let current: u32 = key
        .get_value("State")
        .map_err(|e| AppError::registry("无法读取 UWP 启动任务状态", e))?;
    if current == 3 || current == 4 {
        return Err(AppError::unsupported(
            "该 UWP 启动任务由组策略控制，不能在此修改",
        ));
    }

    let target: u32 = if enabled { 2 } else { 1 };
    key.set_value("State", &target)
        .map_err(|e| AppError::registry("无法修改 UWP 启动任务状态", e))?;

    let readback: u32 = key
        .get_value("State")
        .map_err(|e| AppError::registry("无法校验 UWP 启动任务状态", e))?;
    if readback != target {
        return Err(AppError::new(
            "STATE_VERIFY_FAILED",
            "UWP 启动任务状态写入后校验失败",
        ));
    }
    Ok(())
}

fn set_context_menu_verb(hive: Hive, subkey: &str, enabled: bool) -> AppResult<()> {
    let path = format!(r"Software\Classes\{subkey}");
    let flags = KEY_READ | KEY_WRITE | KEY_WOW64_64KEY;
    let key = hive
        .key()
        .open_subkey_with_flags(&path, flags)
        .map_err(|err| AppError::registry("无法打开右键菜单注册项", err))?;

    let has_programmatic = key.get_raw_value(PROGRAMMATIC_ACCESS_ONLY).is_ok();
    let has_legacy = key.get_raw_value(LEGACY_DISABLE).is_ok();
    let marker = read_value_string(&key, VERB_MARKER).as_deref() == Some("hidden");

    if enabled {
        if !marker {
            if has_programmatic || has_legacy {
                return Err(AppError::unsupported("该项由其他工具隐藏，本工具不会修改"));
            }
            return Ok(());
        }

        let _ = key.delete_value(PROGRAMMATIC_ACCESS_ONLY);
        let _ = key.delete_value(LEGACY_DISABLE);
        key.delete_value(VERB_MARKER)
            .map_err(|err| AppError::registry("无法清除右键菜单隐藏标记", err))?;

        if key.get_raw_value(PROGRAMMATIC_ACCESS_ONLY).is_ok()
            || key.get_raw_value(LEGACY_DISABLE).is_ok()
            || key.get_raw_value(VERB_MARKER).is_ok()
        {
            return Err(AppError::new(
                "STATE_VERIFY_FAILED",
                "右键菜单状态写入后校验失败",
            ));
        }
        return Ok(());
    }

    if (has_programmatic || has_legacy) && !marker {
        return Err(AppError::unsupported("该项由其他工具隐藏，本工具不会修改"));
    }

    key.set_value(PROGRAMMATIC_ACCESS_ONLY, &"")
        .map_err(|err| AppError::registry("无法隐藏右键菜单项", err))?;
    key.set_value(LEGACY_DISABLE, &"")
        .map_err(|err| AppError::registry("无法隐藏右键菜单项", err))?;
    key.set_value(VERB_MARKER, &"hidden")
        .map_err(|err| AppError::registry("无法写入右键菜单隐藏标记", err))?;

    if key.get_raw_value(PROGRAMMATIC_ACCESS_ONLY).is_err()
        || key.get_raw_value(LEGACY_DISABLE).is_err()
        || read_value_string(&key, VERB_MARKER).as_deref() != Some("hidden")
    {
        return Err(AppError::new(
            "STATE_VERIFY_FAILED",
            "右键菜单状态写入后校验失败",
        ));
    }
    Ok(())
}

fn set_shell_ext_blocked(clsid: &str, hive: Hive, enabled: bool) -> AppResult<()> {
    let flags = KEY_READ | KEY_WRITE | KEY_WOW64_64KEY;
    let (key, _) = hive
        .key()
        .create_subkey_with_flags(BLOCKED, flags)
        .map_err(|err| AppError::registry("无法打开右键菜单阻止列表", err))?;

    let existing = key
        .get_raw_value(clsid)
        .ok()
        .and_then(|value| decode_reg_string(&value));

    if enabled {
        match existing.as_deref() {
            None => Ok(()),
            Some(data) if data == BLOCKED_MARKER => {
                key.delete_value(clsid)
                    .map_err(|err| AppError::registry("无法启用右键菜单扩展", err))?;
                if key.get_raw_value(clsid).is_ok() {
                    return Err(AppError::new(
                        "STATE_VERIFY_FAILED",
                        "右键菜单扩展状态写入后校验失败",
                    ));
                }
                Ok(())
            }
            Some(_) => Err(AppError::unsupported(
                "该项由其他程序或策略阻止，本工具不会修改",
            )),
        }
    } else {
        match existing.as_deref() {
            Some(data) if data == BLOCKED_MARKER => Ok(()),
            Some(_) => Err(AppError::unsupported(
                "该项由其他程序或策略阻止，本工具不会修改",
            )),
            None => {
                key.set_value(clsid, &BLOCKED_MARKER)
                    .map_err(|err| AppError::registry("无法阻止右键菜单扩展", err))?;
                let readback = key
                    .get_raw_value(clsid)
                    .ok()
                    .and_then(|value| decode_reg_string(&value));
                if readback.as_deref() != Some(BLOCKED_MARKER) {
                    return Err(AppError::new(
                        "STATE_VERIFY_FAILED",
                        "右键菜单扩展状态写入后校验失败",
                    ));
                }
                Ok(())
            }
        }
    }
}

fn read_value_string(key: &RegKey, name: &str) -> Option<String> {
    key.get_raw_value(name)
        .ok()
        .and_then(|value| decode_reg_string(&value))
}

/// When a handler is toggled at machine scope, remove any marker this tool
/// previously wrote to the current user's HKCU blocked list. Values written by
/// other tools are left untouched.
pub fn clear_stale_user_block(item: &StartupItem) {
    let Some(ActionRef::ShellExtensionBlocked {
        clsid,
        hive: Hive::Hklm,
    }) = item.action.as_ref()
    else {
        return;
    };
    let Ok(key) = Hive::Hkcu
        .key()
        .open_subkey_with_flags(BLOCKED, KEY_READ | KEY_WRITE | KEY_WOW64_64KEY)
    else {
        return;
    };
    let data = key
        .get_raw_value(clsid)
        .ok()
        .and_then(|value| decode_reg_string(&value));
    if data.as_deref() == Some(BLOCKED_MARKER)
        && let Err(error) = key.delete_value(clsid)
    {
        tracing::warn!(error = %error, "failed to clear stale user context-menu block");
    }
}

#[derive(Serialize)]
struct ActionLog<'a> {
    at: String,
    id: &'a str,
    name: &'a str,
    source: crate::model::SourceKind,
    enabled: bool,
    origin: &'a str,
}

pub fn record_action(item: &StartupItem, enabled: bool) {
    let entry = ActionLog {
        at: chrono::Local::now().to_rfc3339(),
        id: &item.id,
        name: &item.name,
        source: item.source,
        enabled,
        origin: &item.origin,
    };
    let Ok(line) = serde_json::to_string(&entry) else {
        return;
    };
    paths::ensure_dirs();
    if let Err(err) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths::actions_file())
        .and_then(|mut file| writeln!(file, "{line}"))
    {
        tracing::warn!(error = %err, "failed to append action log");
    }
}
