use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};

use tauri::AppHandle;

use crate::action;
use crate::elevate;
use crate::error::AppError;
use crate::model::{AppState, ScanResult, SourceKind};
use crate::scan;
use crate::snapshot::{self, SnapshotDiff, SnapshotSummary};

/// Snapshot saves and diffs both run a full scan. Serialize them so repeated
/// UI clicks cannot stack multiple heavy scans.
static SNAPSHOT_SCAN_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn snapshot_scan_guard() -> std::sync::MutexGuard<'static, ()> {
    SNAPSHOT_SCAN_LOCK
        .lock()
        .unwrap_or_else(|err| err.into_inner())
}

#[tauri::command]
pub async fn scan_fast(app: AppHandle) -> Result<Vec<ScanResult>, AppError> {
    tauri::async_runtime::spawn_blocking(move || scan::scan_fast(Some(&app)))
        .await
        .map_err(|err| AppError::new("SCAN_FAILED", "扫描失败").with_detail(err.to_string()))
}

#[tauri::command]
pub async fn scan_tasks(app: AppHandle) -> Result<Vec<ScanResult>, AppError> {
    tauri::async_runtime::spawn_blocking(move || scan::scan_tasks(Some(&app)))
        .await
        .map_err(|err| {
            AppError::new("SCAN_FAILED", "扫描计划任务失败").with_detail(err.to_string())
        })
}

#[tauri::command]
pub async fn scan_services(app: AppHandle) -> Result<Vec<ScanResult>, AppError> {
    tauri::async_runtime::spawn_blocking(move || scan::scan_services(Some(&app)))
        .await
        .map_err(|err| AppError::new("SCAN_FAILED", "扫描服务失败").with_detail(err.to_string()))
}

#[tauri::command]
pub async fn set_item_enabled(
    id: String,
    source: SourceKind,
    enabled: bool,
) -> Result<(), AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let item = scan::find_item_in_source(&id, source, false)?;
        if item.readonly {
            return Err(AppError::unsupported("该启动项为只读，工具不会修改"));
        }
        if item.requires_elevation() && !elevate::is_elevated() {
            elevate::request_set_enabled(&item, enabled)?;
        } else {
            action::set_enabled(&item, enabled)?;
        }
        action::record_action(&item, enabled);
        Ok(())
    })
    .await
    .map_err(|err| AppError::new("ACTION_FAILED", "操作失败").with_detail(err.to_string()))?
}

#[tauri::command]
pub async fn save_snapshot() -> Result<SnapshotSummary, AppError> {
    tauri::async_runtime::spawn_blocking(|| {
        let _guard = snapshot_scan_guard();
        let results = scan::scan_all(None);
        ensure_complete_scan(&results, "保存快照失败")?;
        let items = results
            .into_iter()
            .flat_map(|result| result.items)
            .collect();
        snapshot::save(items)
    })
    .await
    .map_err(|err| AppError::new("SNAPSHOT_ERROR", "保存快照失败").with_detail(err.to_string()))?
}

fn ensure_complete_scan(
    results: &[crate::model::ScanResult],
    context: &str,
) -> Result<(), AppError> {
    let failures: Vec<String> = results
        .iter()
        .filter_map(|result| {
            result
                .error
                .as_ref()
                .map(|error| format!("{:?}: {error}", result.source))
        })
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(
            AppError::new("SNAPSHOT_INCOMPLETE", context).with_detail(format!(
                "以下来源读取失败，快照可能不完整：{}",
                failures.join("；")
            )),
        )
    }
}

#[tauri::command]
pub async fn list_snapshots() -> Result<Vec<SnapshotSummary>, AppError> {
    tauri::async_runtime::spawn_blocking(snapshot::list)
        .await
        .map_err(|err| {
            AppError::new("SNAPSHOT_ERROR", "读取快照失败").with_detail(err.to_string())
        })?
}

#[tauri::command]
pub async fn diff_snapshot(file: String) -> Result<SnapshotDiff, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = snapshot_scan_guard();
        snapshot::diff(&file)
    })
    .await
    .map_err(|err| AppError::new("SNAPSHOT_ERROR", "对比快照失败").with_detail(err.to_string()))?
}

#[tauri::command]
pub async fn delete_snapshot(file: String) -> Result<(), AppError> {
    tauri::async_runtime::spawn_blocking(move || snapshot::delete(&file))
        .await
        .map_err(|err| {
            AppError::new("SNAPSHOT_ERROR", "删除快照失败").with_detail(err.to_string())
        })?
}

#[tauri::command]
pub async fn open_location(id: String, source: SourceKind) -> Result<(), AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let item = scan::find_item_in_source(&id, source, true)?;
        let target: Option<PathBuf> = item
            .exe_path
            .clone()
            .filter(|path| path.is_absolute())
            .or_else(|| {
                if item.source == SourceKind::StartupFolder {
                    Some(PathBuf::from(&item.origin))
                } else {
                    None
                }
            });
        let Some(target) = target else {
            return Err(AppError::unsupported("该启动项没有可定位的文件"));
        };
        if !target.is_absolute() {
            return Err(AppError::new(
                "INVALID_PATH",
                "该启动项没有可定位的绝对路径",
            ));
        }
        // explorer.exe lives in the Windows directory, not System32. Resolve
        // it through the known folder API instead of trusting the environment.
        let explorer = crate::util::windows_dir().join("explorer.exe");
        let mut command = std::process::Command::new(explorer);
        if target.exists() {
            // Explorer parses `/select,` itself and does not understand the
            // CRT-style `\"` escaping that Command::arg adds, so the argument
            // would be treated as an unknown switch and Explorer would fall
            // back to the last folder. Paths reaching here exist and therefore
            // cannot contain a quote, so passing the literal argument is safe.
            command.raw_arg(format!("/select,\"{}\"", target.display()));
        } else if let Some(parent) = target.parent().filter(|path| path.exists()) {
            command.arg(parent);
        } else {
            return Err(AppError::new("FILE_NOT_FOUND", "找不到对应文件"));
        }
        command
            .spawn()
            .map_err(|err| AppError::io("无法打开文件位置", err))?;
        Ok(())
    })
    .await
    .map_err(|err| AppError::new("ACTION_FAILED", "打开位置失败").with_detail(err.to_string()))?
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IconEntry {
    pub path: String,
    pub icon: Option<String>,
}

const MAX_ICON_PATHS: usize = 512;

#[tauri::command]
pub async fn load_icons(paths: Vec<String>) -> Result<Vec<IconEntry>, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut seen = std::collections::HashSet::new();
        let mut unique = Vec::new();
        for path in paths {
            if path.trim().is_empty() || !seen.insert(path.to_ascii_lowercase()) {
                continue;
            }
            unique.push(path);
            if unique.len() >= MAX_ICON_PATHS {
                break;
            }
        }
        unique
            .into_iter()
            .map(|path| {
                let icon = if crate::icons::is_known_path(Path::new(&path)) {
                    crate::icons::data_url_for(Path::new(&path))
                } else {
                    None
                };
                IconEntry { path, icon }
            })
            .collect()
    })
    .await
    .map_err(|err| AppError::new("ICON_ERROR", "读取图标失败").with_detail(err.to_string()))
}

#[tauri::command]
pub fn get_app_state() -> AppState {
    AppState {
        is_elevated: elevate::is_elevated(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

#[tauri::command]
pub fn set_window_theme(window: tauri::Window, theme: String) -> Result<(), AppError> {
    let theme = match theme.as_str() {
        "light" => Some(tauri::Theme::Light),
        "dark" => Some(tauri::Theme::Dark),
        _ => None,
    };
    window
        .set_theme(theme)
        .map_err(|err| AppError::windows("无法同步窗口主题", err))
}

#[tauri::command]
pub fn show_main_window(window: tauri::WebviewWindow) -> Result<(), AppError> {
    window
        .show()
        .map_err(|err| AppError::windows("无法显示主窗口", err))?;
    window
        .set_focus()
        .map_err(|err| AppError::windows("无法聚焦主窗口", err))
}
