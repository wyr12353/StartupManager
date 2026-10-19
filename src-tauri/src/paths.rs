use std::path::PathBuf;

pub fn app_data_dir() -> PathBuf {
    use windows::Win32::UI::Shell::FOLDERID_LocalAppData;

    crate::util::known_folder_path(&FOLDERID_LocalAppData)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .unwrap_or_else(std::env::temp_dir)
        .join("StartupManager")
}

pub fn snapshots_dir() -> PathBuf {
    app_data_dir().join("snapshots")
}

pub fn logs_dir() -> PathBuf {
    app_data_dir().join("logs")
}

pub fn actions_file() -> PathBuf {
    app_data_dir().join("actions.jsonl")
}

pub fn ensure_dirs() {
    let _ = std::fs::create_dir_all(snapshots_dir());
    let _ = std::fs::create_dir_all(logs_dir());
}
