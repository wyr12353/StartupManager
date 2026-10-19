pub mod action;
pub mod commands;
pub mod elevate;
pub mod error;
pub mod icons;
pub mod logging;
pub mod model;
pub mod paths;
pub mod scan;
pub mod signature;
pub mod snapshot;
pub mod startup_approved;
pub mod util;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    logging::init();

    // Development convenience: allow a second GUI instance while iterating on
    // the layout. The flag is ignored in release builds.
    let allow_multiple =
        cfg!(debug_assertions) && std::env::args().any(|arg| arg == "--allow-multiple");

    let mut builder = tauri::Builder::default();
    if !allow_multiple {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            use tauri::Manager;
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }));
    }

    builder
        .invoke_handler(tauri::generate_handler![
            commands::scan_fast,
            commands::scan_tasks,
            commands::scan_services,
            commands::set_item_enabled,
            commands::save_snapshot,
            commands::list_snapshots,
            commands::diff_snapshot,
            commands::delete_snapshot,
            commands::open_location,
            commands::load_icons,
            commands::get_app_state,
            commands::set_window_theme,
            commands::show_main_window,
        ])
        .setup(|app| {
            use tauri::Manager;

            // The window starts hidden so WebView2 cannot paint a white frame
            // before the themed app shell is ready. Show it anyway if the
            // frontend fails to call show_main_window.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(5));
                if let Some(window) = handle.get_webview_window("main")
                    && !window.is_visible().unwrap_or(false)
                {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
