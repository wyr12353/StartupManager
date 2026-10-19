// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|arg| arg == "--elevated-helper") {
        let pipe = arg_value(&args, "--pipe").unwrap_or_default();
        let nonce = arg_value(&args, "--nonce").unwrap_or_default();
        std::process::exit(startupmanager_lib::elevate::run_helper(&pipe, &nonce));
    }

    if args.iter().any(|arg| arg == "--scan-json") {
        let results = startupmanager_lib::scan::scan_all(None);
        match serde_json::to_string_pretty(&results) {
            Ok(json) => println!("{json}"),
            Err(err) => eprintln!("无法序列化扫描结果: {err}"),
        }
        return;
    }

    if args.iter().any(|arg| arg == "--selftest") {
        std::process::exit(run_selftest());
    }

    startupmanager_lib::run();
}

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

fn run_selftest() -> i32 {
    use startupmanager_lib::startup_approved::{ApprovedState, parse_blob};

    let enabled = [2u8, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8];
    let disabled = [3u8, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8];
    let re_enabled = [6u8, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8];
    let re_disabled = [7u8, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8];
    let cases = [
        (enabled.as_slice(), ApprovedState::Enabled),
        (disabled.as_slice(), ApprovedState::Disabled),
        (re_enabled.as_slice(), ApprovedState::Enabled),
        (re_disabled.as_slice(), ApprovedState::Disabled),
        ([2u8, 0, 0].as_slice(), ApprovedState::Unknown),
        ([].as_slice(), ApprovedState::Unknown),
    ];

    for (blob, expected) in cases {
        if parse_blob(blob) != expected {
            eprintln!("StartupApproved 自检失败: {blob:?} != {expected:?}");
            return 1;
        }
    }
    println!("自检通过");
    0
}
