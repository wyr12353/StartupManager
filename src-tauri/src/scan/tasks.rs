use windows::Win32::Foundation::VARIANT_BOOL;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::System::TaskScheduler::{
    IActionCollection, IExecAction, IRegisteredTask, IRegisteredTaskCollection, ITaskDefinition,
    ITaskFolder, ITaskService, TASK_ENUM_HIDDEN, TASK_TRIGGER_BOOT, TASK_TRIGGER_LOGON,
    TASK_TRIGGER_TYPE2,
};
use windows::Win32::System::Variant::VARIANT;
use windows::core::{BSTR, GUID, Interface};

use crate::error::{AppError, AppResult};
use crate::identity;
use crate::model::{ActionRef, ScanResult, Scope, SourceKind, StartupItem};
use crate::signature;
use crate::util::{make_id, parse_executable};

const CLSID_TASK_SERVICE: GUID = GUID::from_u128(0x0f87369f_a4e5_4cfc_bd3e_73e6154572dd);
const MAX_TASK_FOLDER_DEPTH: usize = 32;

fn normalize_principal(value: &str) -> String {
    value.trim().trim_start_matches(".\\").to_ascii_lowercase()
}

fn is_system_principal(user_id: &str) -> bool {
    let id = normalize_principal(user_id);
    id == "system" || id == "nt authority\\system" || id == "s-1-5-18" || id.ends_with("\\system")
}

fn is_current_user(user_id: &str) -> bool {
    let uid = normalize_principal(user_id);
    if uid.is_empty() || is_system_principal(&uid) {
        return false;
    }
    if let Some(current) = identity::current() {
        let sid = current.sid.to_ascii_lowercase();
        let account = current.name.to_ascii_lowercase();
        if uid == sid || (!account.is_empty() && uid == account) {
            return true;
        }
        if let Some((_, name)) = account.rsplit_once('\\')
            && !uid.contains('\\')
            && uid == name
        {
            return true;
        }
        return false;
    }
    let username = std::env::var("USERNAME")
        .unwrap_or_default()
        .to_ascii_lowercase();
    if username.is_empty() {
        return false;
    }
    let domain = std::env::var("USERDOMAIN")
        .unwrap_or_default()
        .to_ascii_lowercase();
    uid == username || (!domain.is_empty() && uid == format!("{domain}\\{username}"))
}

fn principal_user_id(definition: &ITaskDefinition) -> Option<String> {
    let principal = unsafe { definition.Principal() }.ok()?;
    let mut user_id = BSTR::new();
    unsafe { principal.UserId(&mut user_id) }.ok()?;
    let value = user_id.to_string();
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub fn scan() -> ScanResult {
    scan_with(true)
}

pub(crate) fn scan_with(metadata: bool) -> ScanResult {
    let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    let result = scan_inner(metadata);
    if initialized {
        unsafe { CoUninitialize() };
    }
    result
}

fn scan_inner(metadata: bool) -> ScanResult {
    let mut items = Vec::new();
    let mut error = None;

    let service: ITaskService =
        match unsafe { CoCreateInstance(&CLSID_TASK_SERVICE, None, CLSCTX_INPROC_SERVER) } {
            Ok(service) => service,
            Err(err) => {
                return ScanResult {
                    source: SourceKind::ScheduledTask,
                    items,
                    error: Some(format!("无法连接任务计划服务: {err}")),
                };
            }
        };

    let empty = VARIANT::default();
    if let Err(err) = unsafe { service.Connect(&empty, &empty, &empty, &empty) } {
        return ScanResult {
            source: SourceKind::ScheduledTask,
            items,
            error: Some(format!("无法连接任务计划服务: {err}")),
        };
    }

    let root = match unsafe { service.GetFolder(&BSTR::from("\\")) } {
        Ok(folder) => folder,
        Err(err) => {
            return ScanResult {
                source: SourceKind::ScheduledTask,
                items,
                error: Some(format!("无法打开任务计划根目录: {err}")),
            };
        }
    };

    walk_folder(&root, &mut items, &mut error, metadata, 0);

    items.sort_by_key(|a| a.name.to_lowercase());

    ScanResult {
        source: SourceKind::ScheduledTask,
        items,
        error,
    }
}

fn note_error(error: &mut Option<String>, message: impl Into<String>) {
    if error.is_none() {
        *error = Some(message.into());
    }
}

fn walk_folder(
    folder: &ITaskFolder,
    items: &mut Vec<StartupItem>,
    error: &mut Option<String>,
    metadata: bool,
    depth: usize,
) {
    if depth > MAX_TASK_FOLDER_DEPTH {
        note_error(error, "计划任务文件夹层级过深，已停止扫描该分支");
        return;
    }
    match unsafe { folder.GetTasks(TASK_ENUM_HIDDEN.0) } {
        Ok(collection) => collect_tasks(&collection, items, error, metadata),
        Err(err) => note_error(error, format!("无法枚举计划任务: {err}")),
    }

    match unsafe { folder.GetFolders(0) } {
        Ok(folders) => match unsafe { folders.Count() } {
            Ok(count) => {
                for index in 1..=count {
                    let var = VARIANT::from(index);
                    let Ok(child) = (unsafe { folders.get_Item(&var) }) else {
                        continue;
                    };
                    walk_folder(&child, items, error, metadata, depth + 1);
                }
            }
            Err(err) => note_error(error, format!("无法读取任务文件夹数量: {err}")),
        },
        Err(err) => note_error(error, format!("无法枚举任务文件夹: {err}")),
    }
}

fn collect_tasks(
    collection: &IRegisteredTaskCollection,
    items: &mut Vec<StartupItem>,
    error: &mut Option<String>,
    metadata: bool,
) {
    let count = match unsafe { collection.Count() } {
        Ok(count) => count,
        Err(err) => {
            note_error(error, format!("无法读取计划任务数量: {err}"));
            return;
        }
    };
    for index in 1..=count {
        let var = VARIANT::from(index);
        let Ok(task) = (unsafe { collection.get_Item(&var) }) else {
            continue;
        };
        match task_to_item(&task, metadata) {
            Ok(Some(item)) => items.push(item),
            Ok(None) => {}
            Err(err) => note_error(error, err),
        }
    }
}

fn task_to_item(task: &IRegisteredTask, metadata: bool) -> Result<Option<StartupItem>, String> {
    let path = unsafe { task.Path() }
        .map(|value| value.to_string())
        .unwrap_or_default();
    if path.is_empty() {
        return Ok(None);
    }

    let definition = match unsafe { task.Definition() } {
        Ok(definition) => definition,
        Err(_) => return Ok(None),
    };

    let triggers = match unsafe { definition.Triggers() } {
        Ok(triggers) => triggers,
        Err(_) => return Ok(None),
    };
    let mut trigger_count = 0i32;
    let _ = unsafe { triggers.Count(&mut trigger_count) };
    let mut is_logon = false;
    for index in 1..=trigger_count {
        let Ok(trigger) = (unsafe { triggers.get_Item(index) }) else {
            continue;
        };
        let mut kind = TASK_TRIGGER_TYPE2(0);
        if unsafe { trigger.Type(&mut kind) }.is_err() {
            continue;
        }
        if kind == TASK_TRIGGER_BOOT || kind == TASK_TRIGGER_LOGON {
            is_logon = true;
            break;
        }
    }
    if !is_logon {
        return Ok(None);
    }

    let name = unsafe { task.Name() }
        .map(|value| value.to_string())
        .unwrap_or_else(|_| path.clone());
    let enabled = unsafe { task.Enabled() }
        .map(|value| value.as_bool())
        .unwrap_or(true);

    let mut command = None;
    let mut exe_path = None;
    if let Ok(actions) = unsafe { definition.Actions() }
        && let Some((exec_path, args)) = first_exec_action(&actions)
    {
        let combined = match args {
            Some(args) if !args.trim().is_empty() => format!("{exec_path} {args}"),
            _ => exec_path.clone(),
        };
        exe_path = parse_executable(&exec_path);
        command = Some(combined);
    }

    let user_id = principal_user_id(&definition);
    let system_task = path.to_ascii_lowercase().starts_with("\\microsoft\\")
        || user_id.as_deref().is_some_and(is_system_principal);
    let scope = if system_task || user_id.as_deref().is_none_or(|id| !is_current_user(id)) {
        Scope::AllUsers
    } else {
        Scope::CurrentUser
    };
    let mut warnings = Vec::new();
    if system_task {
        warnings.push("系统任务，工具不会修改".to_string());
    }

    let (publisher, signature_status) = signature::describe(exe_path.as_deref(), metadata);
    Ok(Some(StartupItem {
        // Keep the ID scope stable so existing snapshots still match; the
        // user-facing `scope` below reflects the real principal.
        id: make_id(SourceKind::ScheduledTask, Scope::AllUsers, None, &path),
        name,
        command,
        exe_path,
        publisher,
        signature: signature_status,
        source: SourceKind::ScheduledTask,
        scope,
        view: None,
        enabled,
        readonly: system_task,
        machine_wide: !system_task && scope == Scope::AllUsers,
        origin: path.clone(),
        warnings,
        action: if system_task {
            None
        } else {
            Some(ActionRef::ScheduledTask { path })
        },
    }))
}

fn first_exec_action(actions: &IActionCollection) -> Option<(String, Option<String>)> {
    let mut count = 0i32;
    unsafe { actions.Count(&mut count) }.ok()?;
    for index in 1..=count {
        let Ok(action) = (unsafe { actions.get_Item(index) }) else {
            continue;
        };
        let Ok(exec) = action.cast::<IExecAction>() else {
            continue;
        };
        let mut path = BSTR::new();
        if unsafe { exec.Path(&mut path) }.is_err() {
            continue;
        }
        let path = path.to_string();
        if path.is_empty() {
            continue;
        }
        let mut arguments = BSTR::new();
        let args = if unsafe { exec.Arguments(&mut arguments) }.is_ok() {
            let value = arguments.to_string();
            if value.is_empty() { None } else { Some(value) }
        } else {
            None
        };
        return Some((path, args));
    }
    None
}

pub fn set_enabled(path: &str, enabled: bool) -> AppResult<()> {
    let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    let result = set_enabled_inner(path, enabled);
    if initialized {
        unsafe { CoUninitialize() };
    }
    result
}

fn set_enabled_inner(path: &str, enabled: bool) -> AppResult<()> {
    let service: ITaskService =
        unsafe { CoCreateInstance(&CLSID_TASK_SERVICE, None, CLSCTX_INPROC_SERVER) }
            .map_err(|e| AppError::windows("无法连接任务计划服务", e))?;
    let empty = VARIANT::default();
    unsafe { service.Connect(&empty, &empty, &empty, &empty) }
        .map_err(|e| AppError::windows("无法连接任务计划服务", e))?;

    let (folder_path, task_name) = split_task_path(path);
    let folder = unsafe { service.GetFolder(&BSTR::from(folder_path.as_str())) }
        .map_err(|e| AppError::windows("无法打开任务所在文件夹", e))?;
    let task = unsafe { folder.GetTask(&BSTR::from(task_name.as_str())) }
        .map_err(|e| AppError::windows("无法打开计划任务", e))?;

    unsafe { task.SetEnabled(VARIANT_BOOL(if enabled { -1 } else { 0 })) }
        .map_err(|e| AppError::windows("无法修改计划任务状态", e))?;

    let current = unsafe { task.Enabled() }
        .map_err(|e| AppError::windows("无法校验计划任务状态", e))?
        .as_bool();
    if current != enabled {
        return Err(AppError::new(
            "STATE_VERIFY_FAILED",
            "计划任务状态写入后校验失败",
        ));
    }
    Ok(())
}

fn split_task_path(path: &str) -> (String, String) {
    let trimmed = path.trim_start_matches('\\');
    match trimmed.rsplit_once('\\') {
        Some((parent, name)) => (format!("\\{parent}"), name.to_string()),
        None => ("\\".to_string(), trimmed.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{is_current_user, is_system_principal};
    use crate::identity;

    #[test]
    fn recognizes_system_principals() {
        for value in ["SYSTEM", "NT AUTHORITY\\SYSTEM", "S-1-5-18", ".\\SYSTEM"] {
            assert!(is_system_principal(value), "principal {value}");
        }
        for value in ["Users", "NT AUTHORITY\\LOCAL SERVICE", "S-1-5-32-545"] {
            assert!(!is_system_principal(value), "principal {value}");
        }
    }

    #[test]
    fn recognizes_the_current_user() {
        let Some(current) = identity::current() else {
            return;
        };
        assert!(is_current_user(&current.sid));
        if !current.name.is_empty() {
            assert!(is_current_user(&current.name));
        }
        assert!(!is_current_user(""));
        assert!(!is_current_user("S-1-5-18"));
    }
}
