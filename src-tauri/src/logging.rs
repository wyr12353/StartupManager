use std::sync::OnceLock;

use tracing_subscriber::EnvFilter;

use crate::paths;

static GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();

pub fn init() {
    let _ = std::fs::create_dir_all(paths::logs_dir());

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("startupmanager=info,warn"));
    let file_writer = tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("startup-manager.log")
        .build(paths::logs_dir())
        .ok()
        .map(|appender| {
            let (writer, guard) = tracing_appender::non_blocking(appender);
            let _ = GUARD.set(guard);
            writer
        });

    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false);
    if let Some(writer) = file_writer {
        let _ = builder.with_writer(writer).try_init();
    } else {
        // A read-only profile, full disk or a file squatting on the log path
        // must not prevent the app from starting.
        let _ = builder.with_writer(std::io::stderr).try_init();
    }
}
