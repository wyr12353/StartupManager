use std::io::{self, Read, Write};
use std::os::windows::io::{AsHandle, AsRawHandle};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use interprocess::local_socket::{
    ConnectOptions, GenericNamespaced, ListenerNonblockingMode, ListenerOptions,
    Stream as LocalSocketStream, prelude::*,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Pipes::PeekNamedPipe;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;
use windows::core::{PCWSTR, PWSTR};

use crate::action;
use crate::error::{AppError, AppResult};
use crate::model::{ActionRef, Scope, SourceKind, StartupItem};
use crate::scan;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HelperRequest {
    action: String,
    id: String,
    source: SourceKind,
    enabled: bool,
    nonce: String,
    expected_action: ActionRef,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HelperResponse {
    ok: bool,
    id: String,
    nonce: String,
    code: Option<String>,
    message: Option<String>,
    detail: Option<String>,
}

impl HelperResponse {
    fn success(id: String, nonce: String) -> Self {
        Self {
            ok: true,
            id,
            nonce,
            code: None,
            message: None,
            detail: None,
        }
    }

    fn failure(id: String, nonce: String, err: AppError) -> Self {
        Self {
            ok: false,
            id,
            nonce,
            code: Some(err.code),
            message: Some(err.message),
            detail: err.detail,
        }
    }
}

const ELEVATION_TIMEOUT: Duration = Duration::from_secs(60);
const IPC_TIMEOUT: Duration = Duration::from_secs(30);
const IPC_POLL_INTERVAL: Duration = Duration::from_millis(5);
const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(20);

fn ipc_timeout(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, message)
}

fn write_all_deadline(
    stream: &mut LocalSocketStream,
    mut data: &[u8],
    deadline: Instant,
) -> io::Result<()> {
    while !data.is_empty() {
        match stream.write(data) {
            Ok(0) => {}
            Ok(n) => data = &data[n..],
            Err(err)
                if matches!(
                    err.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(err) => return Err(err),
        }
        if data.is_empty() {
            break;
        }
        if Instant::now() >= deadline {
            return Err(ipc_timeout("IPC 写入超时"));
        }
        std::thread::sleep(IPC_POLL_INTERVAL);
    }
    Ok(())
}

fn pipe_handle(stream: &LocalSocketStream) -> io::Result<HANDLE> {
    match stream {
        LocalSocketStream::NamedPipe(inner) => Ok(HANDLE(inner.as_handle().as_raw_handle())),
    }
}

/// Windows nonblocking named-pipe reads can report `Ok(0)` for an empty pipe,
/// which is indistinguishable from EOF. Peek first so a timeout and a closed
/// peer produce different errors.
fn read_line_deadline(stream: &mut LocalSocketStream, deadline: Instant) -> io::Result<String> {
    let handle = pipe_handle(stream)?;
    let mut data = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        let mut available = 0u32;
        if let Err(err) =
            unsafe { PeekNamedPipe(handle, None, 0, None, Some(&mut available), None) }
        {
            let code = err.code().0;
            if code == 109 || code == 233 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "提权进程已关闭连接",
                ));
            }
            return Err(io::Error::from_raw_os_error(code));
        }
        if available > 0 {
            let want = (available as usize).min(buffer.len());
            match stream.read(&mut buffer[..want]) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "提权进程已关闭连接",
                    ));
                }
                Ok(n) => {
                    data.extend_from_slice(&buffer[..n]);
                    if data.contains(&b'\n') {
                        return String::from_utf8(data)
                            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err));
                    }
                    continue;
                }
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => {}
                Err(err) => return Err(err),
            }
        }
        if Instant::now() >= deadline {
            return Err(ipc_timeout("IPC 读取超时"));
        }
        std::thread::sleep(IPC_POLL_INTERVAL);
    }
}

pub fn is_elevated() -> bool {
    use windows::Win32::Security::TOKEN_QUERY;
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let elevated = token_is_elevated(token);
        let _ = CloseHandle(token);
        elevated
    }
}

fn token_is_elevated(token: HANDLE) -> bool {
    use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TokenElevation};

    unsafe {
        let mut elevation = TOKEN_ELEVATION::default();
        let mut length = 0u32;
        GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut length,
        )
        .is_ok()
            && elevation.TokenIsElevated != 0
    }
}

fn process_image_path(pid: u32) -> Option<PathBuf> {
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };

    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut buffer = [0u16; 1024];
    let mut length = buffer.len() as u32;
    let ok = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_FORMAT(0),
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
    };
    let _ = unsafe { CloseHandle(handle) };
    ok.ok()?;
    Some(PathBuf::from(String::from_utf16_lossy(
        &buffer[..length as usize],
    )))
}

fn process_is_elevated(pid: u32) -> Option<bool> {
    use windows::Win32::Security::TOKEN_QUERY;
    use windows::Win32::System::Threading::{
        OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut token = HANDLE::default();
    let opened = unsafe { OpenProcessToken(handle, TOKEN_QUERY, &mut token) }.is_ok();
    let elevated = opened.then(|| token_is_elevated(token));
    if opened && !token.is_invalid() {
        let _ = unsafe { CloseHandle(token) };
    }
    let _ = unsafe { CloseHandle(handle) };
    elevated
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    left.to_string_lossy()
        .eq_ignore_ascii_case(&right.to_string_lossy())
}

/// Positive check only: reject when the peer is clearly a different image.
/// If the process cannot be inspected, fall back to the random pipe name,
/// nonce and UAC consent rather than breaking elevation entirely.
fn peer_looks_like_self(pid: Option<u32>) -> bool {
    let Some(pid) = pid else {
        return true;
    };
    let Some(path) = process_image_path(pid) else {
        match process_is_elevated(pid) {
            Some(false) => {
                tracing::warn!("elevation IPC peer is not elevated");
                return false;
            }
            _ => {
                // Cross-account UAC may hide the image path. The random pipe
                // name, nonce and UAC consent remain the boundary.
                tracing::warn!("cannot inspect elevation IPC peer image");
                return true;
            }
        }
    };
    let Ok(current) = std::env::current_exe() else {
        return true;
    };
    if !paths_equal(&path, &current) {
        tracing::warn!(?path, "elevation IPC peer image mismatch");
        return false;
    }
    if process_is_elevated(pid) == Some(false) {
        tracing::warn!("elevation IPC peer is not elevated");
        return false;
    }
    true
}

pub fn request_set_enabled(item: &StartupItem, enabled: bool) -> AppResult<()> {
    if is_elevated() {
        action::set_enabled(item, enabled)?;
        return Ok(());
    }
    let expected_action = item
        .action
        .clone()
        .ok_or_else(|| AppError::unsupported("该启动项没有可用的修改方式"))?;
    let id = item.id.clone();

    tracing::info!(id = %id, enabled, "requesting elevation");
    let nonce = Uuid::new_v4().to_string();
    let pipe_name = format!("startupmanager-{}", Uuid::new_v4().simple());
    let ns_name = pipe_name
        .as_str()
        .to_ns_name::<GenericNamespaced>()
        .map_err(|e| AppError::new("IPC_ERROR", "无法创建提权通道").with_detail(e.to_string()))?;
    let listener = ListenerOptions::new()
        .name(ns_name)
        .nonblocking(ListenerNonblockingMode::Both)
        .create_sync()
        .map_err(|e| AppError::new("IPC_ERROR", "无法创建提权通道").with_detail(e.to_string()))?;

    let exe = std::env::current_exe()?;
    let params = format!("--elevated-helper --pipe {} --nonce {}", pipe_name, nonce);
    let verb: Vec<u16> = "runas\0".encode_utf16().collect();
    let file: Vec<u16> = exe
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let parameters: Vec<u16> = params.encode_utf16().chain(std::iter::once(0)).collect();

    let launch = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR(parameters.as_ptr()),
            None,
            SW_HIDE,
        )
    };
    let launch_code = launch.0 as isize;
    if launch_code <= 32 {
        return Err(AppError::elevation_cancelled()
            .with_detail(format!("ShellExecuteW 返回 {launch_code}")));
    }

    let accept_deadline = Instant::now() + ELEVATION_TIMEOUT;
    let mut connection = loop {
        match listener.accept() {
            Ok(connection) => break connection,
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                if Instant::now() >= accept_deadline {
                    return Err(AppError::new("ELEVATION_TIMEOUT", "等待管理员授权超时"));
                }
                std::thread::sleep(ACCEPT_POLL_INTERVAL);
            }
            Err(err) => {
                return Err(
                    AppError::new("IPC_ERROR", "提权进程连接失败").with_detail(err.to_string())
                );
            }
        }
    };

    let peer_pid = connection.peer_creds().ok().and_then(|creds| creds.pid());
    if !peer_looks_like_self(peer_pid) {
        return Err(AppError::new("IPC_REJECTED", "提权通道对端校验失败"));
    }

    let request = HelperRequest {
        action: "set_enabled".to_string(),
        id: id.clone(),
        source: item.source,
        enabled,
        nonce: nonce.clone(),
        expected_action,
    };
    let request_json = serde_json::to_string(&request)
        .map_err(|e| AppError::new("IPC_ERROR", "无法序列化提权请求").with_detail(e.to_string()))?;
    let request_deadline = Instant::now() + IPC_TIMEOUT;
    write_all_deadline(&mut connection, request_json.as_bytes(), request_deadline).map_err(
        |err| AppError::new("IPC_ERROR", "无法发送提权请求").with_detail(err.to_string()),
    )?;
    write_all_deadline(&mut connection, b"\n", request_deadline).map_err(|err| {
        AppError::new("IPC_ERROR", "无法发送提权请求").with_detail(err.to_string())
    })?;

    let line = read_line_deadline(&mut connection, request_deadline).map_err(|err| {
        if err.kind() == io::ErrorKind::TimedOut {
            AppError::new("IPC_TIMEOUT", "等待提权进程返回超时").with_detail(err.to_string())
        } else {
            AppError::new("IPC_ERROR", "提权进程返回失败").with_detail(err.to_string())
        }
    })?;
    let response: HelperResponse = serde_json::from_str(&line).map_err(|e| {
        AppError::new("IPC_ERROR", "提权进程返回了无法解析的结果").with_detail(e.to_string())
    })?;
    if response.id != id || response.nonce != nonce {
        return Err(AppError::new("IPC_REJECTED", "提权进程返回了不匹配的结果"));
    }

    if response.ok {
        Ok(())
    } else {
        Err(AppError {
            code: response
                .code
                .unwrap_or_else(|| "ELEVATION_FAILED".to_string()),
            message: response
                .message
                .unwrap_or_else(|| "提权操作失败".to_string()),
            detail: response.detail,
        })
    }
}

pub fn run_helper(pipe_name: &str, nonce: &str) -> i32 {
    crate::logging::init();
    if !is_elevated() {
        return 2;
    }
    let Ok(ns_name) = pipe_name.to_ns_name::<GenericNamespaced>() else {
        return 2;
    };
    let Ok(mut connection) = ConnectOptions::new()
        .name(ns_name)
        .nonblocking_stream(true)
        .connect_sync()
    else {
        return 2;
    };

    let deadline = Instant::now() + IPC_TIMEOUT;
    let Ok(line) = read_line_deadline(&mut connection, deadline) else {
        return 2;
    };
    let Ok(request) = serde_json::from_str::<HelperRequest>(&line) else {
        return 2;
    };

    let response = if request.nonce != nonce || request.action != "set_enabled" {
        HelperResponse::failure(
            request.id.clone(),
            nonce.to_string(),
            AppError::new("IPC_REJECTED", "提权请求校验失败"),
        )
    } else {
        match perform_helper_action(&request) {
            Ok(()) => HelperResponse::success(request.id.clone(), nonce.to_string()),
            Err(err) => HelperResponse::failure(request.id.clone(), nonce.to_string(), err),
        }
    };

    let Ok(json) = serde_json::to_string(&response) else {
        return 2;
    };
    if write_all_deadline(&mut connection, json.as_bytes(), deadline).is_err()
        || write_all_deadline(&mut connection, b"\n", deadline).is_err()
    {
        return 2;
    }
    if response.ok { 0 } else { 1 }
}

fn perform_helper_action(request: &HelperRequest) -> AppResult<()> {
    tracing::info!(id = %request.id, enabled = request.enabled, "elevated helper acting");
    let item = scan::find_item_in_source(&request.id, request.source, false)?;
    if item.readonly {
        return Err(AppError::unsupported("该启动项为只读"));
    }
    if item.scope != Scope::AllUsers {
        return Err(AppError::new("IPC_REJECTED", "提权进程只处理机器级启动项"));
    }
    // The helper re-enumerates with its own HKCU. Over-the-shoulder elevation
    // can therefore derive a different StartupApproved hive than the parent
    // did; never act on that silently.
    if item.action.as_ref() != Some(&request.expected_action) {
        return Err(AppError::new(
            "IPC_REJECTED",
            "提权目标与主进程不一致，请刷新后重试",
        ));
    }
    action::set_enabled(&item, request.enabled)
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::time::{Duration, Instant};

    use interprocess::local_socket::{
        ConnectOptions, GenericNamespaced, ListenerNonblockingMode, ListenerOptions, prelude::*,
    };
    use uuid::Uuid;

    use super::{HelperRequest, read_line_deadline, write_all_deadline};
    use crate::model::{ActionRef, Hive, SourceKind};

    fn pipe_pair() -> (LocalSocketStream, LocalSocketStream) {
        let name = format!("startupmanager-test-{}", Uuid::new_v4().simple());
        let ns_name = name
            .as_str()
            .to_ns_name::<GenericNamespaced>()
            .expect("pipe name");
        let listener = ListenerOptions::new()
            .name(ns_name)
            .nonblocking(ListenerNonblockingMode::Both)
            .create_sync()
            .expect("listener");
        let client_name = name.clone();
        let client = std::thread::spawn(move || {
            let ns_name = client_name
                .as_str()
                .to_ns_name::<GenericNamespaced>()
                .expect("pipe name");
            ConnectOptions::new()
                .name(ns_name)
                .nonblocking_stream(true)
                .connect_sync()
                .expect("client")
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        let server = loop {
            match listener.accept() {
                Ok(stream) => break stream,
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "accept timeout");
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(err) => panic!("accept failed: {err}"),
            }
        };
        (server, client.join().expect("client thread"))
    }

    #[test]
    fn ipc_read_write_roundtrip() {
        let (mut server, mut client) = pipe_pair();
        let deadline = Instant::now() + Duration::from_secs(5);
        write_all_deadline(&mut client, b"hello\n", deadline).expect("client write");
        assert_eq!(
            read_line_deadline(&mut server, deadline)
                .expect("server read")
                .trim_end(),
            "hello"
        );
        write_all_deadline(&mut server, b"world\n", deadline).expect("server write");
        assert_eq!(
            read_line_deadline(&mut client, deadline)
                .expect("client read")
                .trim_end(),
            "world"
        );
    }

    #[test]
    fn ipc_read_times_out() {
        let (mut server, _client) = pipe_pair();
        let err = read_line_deadline(&mut server, Instant::now() + Duration::from_millis(100))
            .expect_err("read should time out");
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
    }

    #[test]
    fn helper_request_roundtrips_expected_action() {
        let request = HelperRequest {
            action: "set_enabled".to_string(),
            id: "item".to_string(),
            source: SourceKind::RegistryRun,
            enabled: false,
            nonce: "nonce".to_string(),
            expected_action: ActionRef::StartupApproved {
                hive: Hive::Hklm,
                subkey: "Software\\Test\\StartupApproved\\Run".to_string(),
                value_name: "Value".to_string(),
            },
        };
        let json = serde_json::to_string(&request).expect("serialize");
        let parsed: HelperRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.expected_action, request.expected_action);
    }
}
