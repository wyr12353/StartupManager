use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::model::StartupItem;
use crate::paths;
use crate::scan;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotDocument {
    #[serde(default)]
    pub version: String,
    pub created_at: String,
    pub items: Vec<StartupItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotSummary {
    pub file: String,
    pub created_at: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemChange {
    pub id: String,
    pub before_name: String,
    pub name: String,
    pub before_enabled: bool,
    pub after_enabled: bool,
    pub before_command: Option<String>,
    pub after_command: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotDiff {
    pub added: Vec<StartupItem>,
    pub removed: Vec<StartupItem>,
    pub changed: Vec<ItemChange>,
}

pub fn save(items: Vec<StartupItem>) -> AppResult<SnapshotSummary> {
    paths::ensure_dirs();
    let now = chrono::Local::now();
    let created_at = now.to_rfc3339();
    let file_name = format!("snapshot-{}.json", now.format("%Y%m%d-%H%M%S%9f"));
    let document = SnapshotDocument {
        version: env!("CARGO_PKG_VERSION").to_string(),
        created_at: created_at.clone(),
        items,
    };
    let path = paths::snapshots_dir().join(&file_name);
    let json = serde_json::to_string_pretty(&document).map_err(|e| {
        AppError::new("SNAPSHOT_ERROR", "无法序列化快照").with_detail(e.to_string())
    })?;
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, json)?;
    if let Err(err) = std::fs::rename(&temp, &path) {
        let _ = std::fs::remove_file(&temp);
        return Err(AppError::io("无法写入快照文件", err));
    }
    Ok(SnapshotSummary {
        file: file_name,
        created_at,
        count: document.items.len(),
    })
}

pub fn list() -> AppResult<Vec<SnapshotSummary>> {
    paths::ensure_dirs();
    let mut summaries = Vec::new();
    let entries = match std::fs::read_dir(paths::snapshots_dir()) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(summaries),
        Err(err) => return Err(AppError::io("无法读取快照目录", err)),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();
        if !is_snapshot_file_name(&file_name) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            tracing::warn!(file = %file_name, "failed to read snapshot");
            continue;
        };
        let document = match serde_json::from_str::<SnapshotDocument>(&text) {
            Ok(document) => document,
            Err(err) => {
                tracing::warn!(file = %file_name, error = %err, "skipping invalid snapshot");
                continue;
            }
        };
        summaries.push(SnapshotSummary {
            file: file_name,
            created_at: document.created_at,
            count: document.items.len(),
        });
    }
    summaries.sort_by(|a, b| {
        match (
            chrono::DateTime::parse_from_rfc3339(&a.created_at),
            chrono::DateTime::parse_from_rfc3339(&b.created_at),
        ) {
            (Ok(left), Ok(right)) => right.cmp(&left),
            _ => b.created_at.cmp(&a.created_at),
        }
    });
    Ok(summaries)
}

pub fn diff(file: &str) -> AppResult<SnapshotDiff> {
    use std::collections::HashMap;

    let path = safe_snapshot_path(file)?;
    let text = std::fs::read_to_string(&path)?;
    let old = serde_json::from_str::<SnapshotDocument>(&text)
        .map_err(|e| AppError::new("SNAPSHOT_ERROR", "无法解析快照").with_detail(e.to_string()))?;

    let results = scan::scan_all_light();
    let failures: Vec<String> = results
        .iter()
        .filter_map(|result| {
            result
                .error
                .as_ref()
                .map(|error| format!("{:?}: {error}", result.source))
        })
        .collect();
    if !failures.is_empty() {
        return Err(
            AppError::new("SNAPSHOT_INCOMPLETE", "部分启动项读取失败，无法对比")
                .with_detail(failures.join("；")),
        );
    }
    let current: Vec<StartupItem> = results
        .into_iter()
        .flat_map(|result| result.items)
        .collect();

    let old_by_id: HashMap<&str, &StartupItem> = old
        .items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect();
    let current_by_id: HashMap<&str, &StartupItem> = current
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect();

    let mut added = Vec::new();
    let mut changed = Vec::new();
    for item in &current {
        match old_by_id.get(item.id.as_str()) {
            None => added.push(item.clone()),
            Some(old_item) => {
                if old_item.enabled != item.enabled
                    || old_item.command != item.command
                    || old_item.name != item.name
                {
                    changed.push(ItemChange {
                        id: item.id.clone(),
                        before_name: old_item.name.clone(),
                        name: item.name.clone(),
                        before_enabled: old_item.enabled,
                        after_enabled: item.enabled,
                        before_command: old_item.command.clone(),
                        after_command: item.command.clone(),
                    });
                }
            }
        }
    }
    let removed = old
        .items
        .iter()
        .filter(|old_item| !current_by_id.contains_key(old_item.id.as_str()))
        .cloned()
        .collect();

    Ok(SnapshotDiff {
        added,
        removed,
        changed,
    })
}

pub fn delete(file: &str) -> AppResult<()> {
    let path = safe_snapshot_path(file)?;
    std::fs::remove_file(&path).map_err(|err| AppError::io("删除快照失败", err))
}

fn is_snapshot_file_name(file: &str) -> bool {
    let lower = file.to_ascii_lowercase();
    lower.starts_with("snapshot-") && lower.ends_with(".json")
}

/// Windows reserves these device names regardless of extension. The
/// `snapshot-` prefix already prevents them, but reject them explicitly so a
/// future naming change cannot turn a file read into a device access.
fn is_reserved_windows_name(file: &str) -> bool {
    let stem = file
        .split('.')
        .next()
        .unwrap_or(file)
        .trim_end_matches(' ')
        .to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return true;
    }
    let bytes = stem.as_bytes();
    bytes.len() == 4
        && (bytes.starts_with(b"COM") || bytes.starts_with(b"LPT"))
        && (b'1'..=b'9').contains(&bytes[3])
}

fn safe_snapshot_path(file: &str) -> AppResult<PathBuf> {
    let candidate = Path::new(file);
    let is_plain_name = candidate.file_name().and_then(|name| name.to_str()) == Some(file);
    if !is_plain_name
        || file.contains("..")
        || file.contains(':')
        || !is_snapshot_file_name(file)
        || is_reserved_windows_name(file)
    {
        return Err(AppError::new("INVALID_SNAPSHOT", "快照文件名不合法"));
    }
    let path = paths::snapshots_dir().join(file);
    if !path.exists() {
        return Err(AppError::new("SNAPSHOT_NOT_FOUND", "找不到该快照"));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::{is_reserved_windows_name, is_snapshot_file_name};

    #[test]
    fn accepts_only_snapshot_json_names() {
        assert!(is_snapshot_file_name("snapshot-20260101-120000000.json"));
        assert!(is_snapshot_file_name("snapshot-X.JSON"));
        assert!(!is_snapshot_file_name("other.json"));
        assert!(!is_snapshot_file_name("snapshot-x.txt"));
        assert!(!is_snapshot_file_name("snapshot-x.json.tmp"));
    }

    #[test]
    fn rejects_reserved_device_names() {
        for name in [
            "CON.json",
            "con.json",
            "PRN.json",
            "AUX.json",
            "NUL.json",
            "COM1.json",
            "LPT9.json",
        ] {
            assert!(is_reserved_windows_name(name), "name {name}");
        }
        for name in [
            "COM0.json",
            "LPT0.json",
            "console.json",
            "snapshot-CON.json",
        ] {
            assert!(!is_reserved_windows_name(name), "name {name}");
        }
    }
}
