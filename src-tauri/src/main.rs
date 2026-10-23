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

    startupmanager_lib::run();
}

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .cloned()
}
