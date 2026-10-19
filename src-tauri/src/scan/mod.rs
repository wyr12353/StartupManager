pub mod registry_run;
pub mod services;
pub mod startup_folder;
pub mod tasks;
pub mod uwp;

use tauri::{AppHandle, Emitter};

use crate::error::{AppError, AppResult};
use crate::model::{ScanProgress, ScanResult, SourceKind, StartupItem};

fn run_source(
    app: Option<&AppHandle>,
    source: SourceKind,
    completed: usize,
    total: usize,
    scanner: impl FnOnce() -> ScanResult,
) -> ScanResult {
    let result = scanner();
    crate::icons::register_paths(&result.items);
    tracing::info!(
        source = ?result.source,
        count = result.items.len(),
        error = ?result.error,
        "scan completed"
    );
    emit_progress(app, source, completed, total);
    result
}

pub fn scan_fast(app: Option<&AppHandle>) -> Vec<ScanResult> {
    let scanners: [(SourceKind, fn() -> ScanResult); 3] = [
        (SourceKind::RegistryRun, registry_run::scan),
        (SourceKind::StartupFolder, startup_folder::scan),
        (SourceKind::Uwp, uwp::scan),
    ];
    let total = scanners.len();
    let mut results = Vec::with_capacity(total);
    for (index, (source, scanner)) in scanners.into_iter().enumerate() {
        results.push(run_source(app, source, index + 1, total, scanner));
    }
    results
}

pub fn scan_tasks(app: Option<&AppHandle>) -> Vec<ScanResult> {
    vec![run_source(
        app,
        SourceKind::ScheduledTask,
        1,
        1,
        tasks::scan,
    )]
}

pub fn scan_services(app: Option<&AppHandle>) -> Vec<ScanResult> {
    vec![run_source(app, SourceKind::Service, 1, 1, services::scan)]
}

pub fn scan_all(app: Option<&AppHandle>) -> Vec<ScanResult> {
    let mut results = scan_fast(app);
    results.extend(scan_tasks(app));
    results.extend(scan_services(app));
    results
}

/// Snapshot diffs only compare enabled/name/command, so skip the expensive
/// version and Authenticode queries while still resolving `.lnk` targets to
/// keep command strings comparable with saved snapshots.
pub fn scan_all_light() -> Vec<ScanResult> {
    vec![
        run_source(None, SourceKind::RegistryRun, 1, 5, || {
            registry_run::scan_with(false)
        }),
        run_source(None, SourceKind::StartupFolder, 2, 5, || {
            startup_folder::scan_with(false, true)
        }),
        run_source(None, SourceKind::Uwp, 3, 5, uwp::scan),
        run_source(None, SourceKind::ScheduledTask, 4, 5, || {
            tasks::scan_with(false)
        }),
        run_source(None, SourceKind::Service, 5, 5, || {
            services::scan_with(false)
        }),
    ]
}

/// Looks up a single item for an action. Metadata (publisher/signature) is
/// skipped: it is expensive and would touch user-controlled paths in the
/// elevated helper.
pub fn find_item(id: &str) -> AppResult<StartupItem> {
    find_item_with(id, true)
}

/// Action lookups do not need the `.lnk` target. Skipping link resolution
/// keeps the elevated helper away from user-writable shell link data.
pub fn find_item_for_action(id: &str) -> AppResult<StartupItem> {
    find_item_with(id, false)
}

/// Looks up a single item in one source. Falls back to the full scan when the
/// source does not contain the id, so a stale source hint cannot make the item
/// disappear.
pub fn find_item_in_source(
    id: &str,
    source: SourceKind,
    resolve_links: bool,
) -> AppResult<StartupItem> {
    let result = match source {
        SourceKind::RegistryRun => registry_run::scan_with(false),
        SourceKind::StartupFolder => startup_folder::scan_with(false, resolve_links),
        SourceKind::Uwp => uwp::scan(),
        SourceKind::ScheduledTask => tasks::scan_with(false),
        SourceKind::Service => services::scan_with(false),
    };
    if let Some(item) = result.items.into_iter().find(|item| item.id == id) {
        return Ok(item);
    }
    find_item_with(id, resolve_links)
}

fn find_item_with(id: &str, resolve_links: bool) -> AppResult<StartupItem> {
    let results = [
        registry_run::scan_with(false),
        startup_folder::scan_with(false, resolve_links),
        uwp::scan(),
        tasks::scan_with(false),
        services::scan_with(false),
    ];
    for result in results {
        if let Some(item) = result.items.into_iter().find(|item| item.id == id) {
            return Ok(item);
        }
    }
    Err(AppError::stale_item())
}

pub fn emit_progress(app: Option<&AppHandle>, source: SourceKind, completed: usize, total: usize) {
    if let Some(app) = app {
        let _ = app.emit(
            "scan://progress",
            ScanProgress {
                source,
                completed,
                total,
            },
        );
    }
}
