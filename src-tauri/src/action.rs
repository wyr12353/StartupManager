use std::io::Write;

use serde::Serialize;
use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};

use crate::error::{AppError, AppResult};
use crate::model::{ActionRef, StartupItem};
use crate::paths;
use crate::scan::tasks;
use crate::scan::uwp::APP_MODEL;
use crate::startup_approved;

pub fn set_enabled(item: &StartupItem, enabled: bool) -> AppResult<()> {
    if item.readonly {
        return Err(AppError::unsupported("该启动项为只读，工具不会修改"));
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
