use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use winreg::RegKey;
use winreg::enums::{KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY};

use crate::error::AppError;
use crate::model::{ActionRef, Hive, ScanResult, Scope, SignatureStatus, SourceKind, StartupItem};
use crate::signature;
use crate::util::{decode_reg_string, expand_env, make_id, parse_executable};

const CLASSES: &str = r"Software\Classes";
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Shell Extensions\Approved";
pub(crate) const BLOCKED: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked";
const FILE_TYPES: &str = "SystemFileAssociations";

/// Marks a verb that this tool hid. It lets us avoid deleting values written by
/// other context-menu managers when the user re-enables an item.
pub(crate) const VERB_MARKER: &str = "StartupManager.ContextMenu";
/// Value data used to mark CLSIDs blocked by this tool.
pub(crate) const BLOCKED_MARKER: &str = "StartupManager";

const STATIC_ROOTS: &[(&str, &str)] = &[
    ("*", "所有文件"),
    ("AllFilesystemObjects", "所有文件/文件夹"),
    ("Folder", "文件夹（Shell）"),
    ("Directory", "文件夹"),
    (r"Directory\Background", "文件夹空白处"),
    ("DesktopBackground", "桌面背景"),
    ("Drive", "驱动器"),
];

struct HandlerRegistration {
    clsid: String,
    hive: Hive,
    root: String,
    name: String,
}

pub fn scan() -> ScanResult {
    scan_with(true)
}

pub(crate) fn scan_with(metadata: bool) -> ScanResult {
    let mut items = Vec::new();
    let mut errors = Vec::new();
    let mut handlers = Vec::new();

    for root in collect_roots(&mut errors) {
        let hkcu_verbs = scan_root(
            Hive::Hkcu,
            &root,
            metadata,
            &mut items,
            &mut errors,
            &mut handlers,
            &HashSet::new(),
        );
        scan_root(
            Hive::Hklm,
            &root,
            metadata,
            &mut items,
            &mut errors,
            &mut handlers,
            &hkcu_verbs,
        );
    }

    items.extend(build_handlers(&handlers, metadata, &mut errors));
    // This page targets third-party entries. Unknown publishers are kept
    // because they may still be third-party items without version metadata.
    items.retain(|item| !is_microsoft_item(item));
    items.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.origin.cmp(&b.origin))
    });

    ScanResult {
        source: SourceKind::ContextMenu,
        items,
        error: if errors.is_empty() {
            None
        } else {
            Some(errors.join("；"))
        },
    }
}

fn collect_roots(errors: &mut Vec<String>) -> Vec<String> {
    let mut roots: Vec<String> = STATIC_ROOTS
        .iter()
        .map(|(path, _)| (*path).to_string())
        .collect();

    let mut file_types = BTreeSet::new();
    for hive in [Hive::Hkcu, Hive::Hklm] {
        match open_classes_key(hive, FILE_TYPES) {
            Ok(Some(key)) => {
                for entry in key.enum_keys() {
                    match entry {
                        Ok(name) => {
                            file_types.insert(name);
                        }
                        Err(err) => errors.push(format!(
                            "{} {}: 无法枚举文件类型（{err}）",
                            hive.label(),
                            FILE_TYPES
                        )),
                    }
                }
            }
            Ok(None) => {}
            Err(err) => errors.push(format!("{} {}: {}", hive.label(), FILE_TYPES, err.message)),
        }
    }
    roots.extend(
        file_types
            .into_iter()
            .map(|name| format!("{FILE_TYPES}\\{name}")),
    );
    roots
}

fn scan_root(
    hive: Hive,
    root: &str,
    metadata: bool,
    items: &mut Vec<StartupItem>,
    errors: &mut Vec<String>,
    handlers: &mut Vec<HandlerRegistration>,
    skip_verbs: &HashSet<String>,
) -> HashSet<String> {
    let mut verb_names = HashSet::new();
    let shell_path = format!(r"{root}\shell");

    match open_classes_key(hive, &shell_path) {
        Ok(Some(shell)) => {
            for entry in shell.enum_keys() {
                let verb_name = match entry {
                    Ok(name) => name,
                    Err(err) => {
                        errors.push(format!(
                            "{} {}: 无法枚举菜单命令（{err}）",
                            hive.label(),
                            shell_path
                        ));
                        continue;
                    }
                };
                // Registry key names are case-insensitive; compare the
                // normalized form so `HKLM\...\shell\Open` is skipped when
                // HKCU already defines `open`.
                let normalized = verb_name.to_ascii_lowercase();
                verb_names.insert(normalized.clone());
                if skip_verbs.contains(&normalized) {
                    continue;
                }
                let key = match shell.open_subkey(&verb_name) {
                    Ok(key) => key,
                    Err(err) => {
                        errors.push(format!(
                            "{} {}\\{}: 无法读取菜单命令（{err}）",
                            hive.label(),
                            shell_path,
                            verb_name
                        ));
                        continue;
                    }
                };
                let subkey = format!(r"{root}\shell\{verb_name}");
                match build_verb_item(hive, &subkey, &verb_name, &key, metadata) {
                    Ok(item) => items.push(item),
                    Err(err) => errors.push(format!(
                        "{} {}\\{}: {}",
                        hive.label(),
                        shell_path,
                        verb_name,
                        err.message
                    )),
                }
            }
        }
        Ok(None) => {}
        Err(err) => errors.push(format!("{} {}: {}", hive.label(), shell_path, err.message)),
    }

    let handler_path = format!(r"{root}\shellex\ContextMenuHandlers");
    match open_classes_key(hive, &handler_path) {
        Ok(Some(key)) => {
            for entry in key.enum_keys() {
                let name = match entry {
                    Ok(name) => name,
                    Err(err) => {
                        errors.push(format!(
                            "{} {}: 无法枚举 COM 扩展（{err}）",
                            hive.label(),
                            handler_path
                        ));
                        continue;
                    }
                };
                let sub = match key.open_subkey(&name) {
                    Ok(sub) => sub,
                    Err(err) => {
                        errors.push(format!(
                            "{} {}\\{}: 无法读取 COM 扩展（{err}）",
                            hive.label(),
                            handler_path,
                            name
                        ));
                        continue;
                    }
                };
                let Some(clsid) = read_default(&sub).map(|value| normalize_clsid(&value)) else {
                    continue;
                };
                if !is_clsid(&clsid) {
                    continue;
                }
                handlers.push(HandlerRegistration {
                    clsid,
                    hive,
                    root: root.to_string(),
                    name,
                });
            }
        }
        Ok(None) => {}
        Err(err) => errors.push(format!(
            "{} {}: {}",
            hive.label(),
            handler_path,
            err.message
        )),
    }

    verb_names
}

fn build_verb_item(
    hive: Hive,
    subkey: &str,
    verb_name: &str,
    key: &RegKey,
    metadata: bool,
) -> Result<StartupItem, AppError> {
    let command_key = key.open_subkey("command").ok();
    let delegate = command_key
        .as_ref()
        .and_then(|key| read_string(key, "DelegateExecute"));
    let explorer_handler = read_string(key, "ExplorerCommandHandler");
    let drop_clsid = key
        .open_subkey("DropTarget")
        .ok()
        .and_then(|key| read_string(&key, "CLSID"));

    let mut command = command_key.as_ref().and_then(read_default);
    if command.is_none()
        && let Some(clsid) = delegate.or(explorer_handler).or(drop_clsid)
    {
        command = Some(format!("COM {}", normalize_clsid(&clsid)));
    }

    let com_clsid = command
        .as_deref()
        .and_then(|value| value.strip_prefix("COM "))
        .map(normalize_clsid);
    let exe_path = match com_clsid.as_deref() {
        Some(clsid) => resolve_clsid_dll(clsid).0,
        None => command.as_deref().and_then(resolve_command_target),
    };
    let (publisher, signature_status) = describe(exe_path.as_deref(), metadata);
    let raw_name = read_string(key, "MUIVerb").or_else(|| read_default(key));
    let name = raw_name
        .map(|value| resolve_shell_text(&value, verb_name))
        .unwrap_or_else(|| verb_name.to_string());

    let programmatic = has_value(key, "ProgrammaticAccessOnly");
    let legacy = has_value(key, "LegacyDisable");
    let marker = read_string(key, VERB_MARKER).as_deref() == Some("hidden");
    let hidden = programmatic || legacy;
    let external_hidden = hidden && !marker;
    let protected = is_protected_verb(verb_name);

    let mut warnings = Vec::new();
    if has_value(key, "Extended") {
        warnings.push("仅按住 Shift 时显示".to_string());
    }
    if has_value(key, "SubCommands") || has_value(key, "ExtendedSubCommandsKey") {
        warnings.push("级联菜单".to_string());
    }
    if let Some(applies_to) = read_string(key, "AppliesTo") {
        warnings.push(format!("仅对满足条件的文件显示（{applies_to}）"));
    }
    if external_hidden {
        warnings.push("由其他工具隐藏，本工具不会覆盖该状态".to_string());
    }
    if command.is_none() {
        warnings.push("未找到命令行或 COM 处理器".to_string());
    }
    if com_clsid.is_some() && exe_path.is_none() {
        warnings.push("未找到 COM 处理器 DLL".to_string());
    }
    if exe_path.as_deref().is_some_and(is_indirect_interpreter) {
        warnings.push("通过命令解释器间接调用，发布者可能不准确".to_string());
    }
    if protected {
        warnings.push("系统关键项，工具不会修改".to_string());
    }
    if hive == Hive::Hkcu {
        warnings.push("该项注册在当前用户，修改只影响当前用户".to_string());
    }

    let readonly = protected || external_hidden;
    let scope = if hive == Hive::Hkcu {
        Scope::CurrentUser
    } else {
        Scope::AllUsers
    };
    let origin = format!(r"{}\{CLASSES}\{}", hive.label(), subkey);
    let action = if readonly {
        None
    } else {
        Some(ActionRef::ContextMenuVerb {
            hive,
            subkey: subkey.to_string(),
        })
    };

    Ok(StartupItem {
        id: make_id(
            SourceKind::ContextMenu,
            scope,
            None,
            &format!("{}|{}", hive.label(), subkey),
        ),
        name,
        command,
        exe_path,
        publisher,
        signature: signature_status,
        source: SourceKind::ContextMenu,
        scope,
        view: None,
        enabled: !hidden,
        readonly,
        machine_wide: hive == Hive::Hklm,
        origin,
        warnings,
        action,
    })
}

fn build_handlers(
    registrations: &[HandlerRegistration],
    metadata: bool,
    errors: &mut Vec<String>,
) -> Vec<StartupItem> {
    let mut grouped: HashMap<String, Vec<&HandlerRegistration>> = HashMap::new();
    for registration in registrations {
        grouped
            .entry(registration.clsid.clone())
            .or_default()
            .push(registration);
    }

    let mut items = Vec::new();
    for (clsid, registrations) in grouped {
        let (dll, is_32_bit) = resolve_clsid_dll(&clsid);
        let (publisher, signature_status) = describe(dll.as_deref(), metadata);
        let mut warnings = Vec::new();
        if registrations.len() > 1 {
            warnings.push(format!(
                "在 {} 个位置注册，禁用会影响全部位置",
                registrations.len()
            ));
        }
        if dll.is_none() {
            warnings.push("未找到可加载的 DLL/EXE".to_string());
        }
        if is_32_bit {
            warnings.push("32 位扩展，64 位资源管理器通常不会加载".to_string());
        }

        let hklm_blocked = read_blocked(Hive::Hklm, &clsid);
        let hkcu_blocked = read_blocked(Hive::Hkcu, &clsid);
        let hklm_external = hklm_blocked
            .as_deref()
            .is_some_and(|data| data != BLOCKED_MARKER);
        if hklm_external {
            warnings.push("已被其他程序或策略在所有用户范围阻止".to_string());
        }
        if let Some(data) = hkcu_blocked.as_deref() {
            if data == BLOCKED_MARKER {
                warnings.push(
                    "当前用户还有本工具写入的 HKCU 阻止；本开关只管理所有用户范围".to_string(),
                );
            } else {
                warnings
                    .push("当前用户另有其他程序的 HKCU 阻止；本开关只管理所有用户范围".to_string());
            }
        }

        let enabled = hklm_blocked.is_none();
        // A handler whose module cannot be resolved is still blocked by CLSID,
        // so it stays manageable. Only a foreign machine-level block is a hard
        // stop.
        let readonly = hklm_external;

        let name = read_approved(Hive::Hkcu, &clsid)
            .or_else(|| read_approved(Hive::Hklm, &clsid))
            .or_else(|| read_clsid_name(&clsid))
            .unwrap_or_else(|| registrations[0].name.clone());

        let scope = Scope::AllUsers;

        let origin = unique_origins(&registrations);
        let action = if readonly {
            None
        } else {
            Some(ActionRef::ShellExtensionBlocked {
                clsid: clsid.clone(),
                hive: Hive::Hklm,
            })
        };

        items.push(StartupItem {
            id: make_id(
                SourceKind::ContextMenu,
                Scope::AllUsers,
                None,
                &format!("handler:{clsid}"),
            ),
            name,
            command: Some(match dll.as_ref() {
                Some(path) => format!("COM {clsid} → {}", path.display()),
                None => format!("COM {clsid}"),
            }),
            exe_path: dll,
            publisher,
            signature: signature_status,
            source: SourceKind::ContextMenu,
            scope,
            view: None,
            enabled,
            readonly,
            machine_wide: true,
            origin,
            warnings,
            action,
        });
    }

    if items.is_empty() && !registrations.is_empty() {
        errors.push("COM 扩展解析结果为空".to_string());
    }
    items
}

fn unique_origins(registrations: &[&HandlerRegistration]) -> String {
    let mut seen = HashSet::new();
    registrations
        .iter()
        .filter_map(|registration| {
            let value = format!(
                r"{}\{CLASSES}\{}\shellex\ContextMenuHandlers\{}",
                registration.hive.label(),
                registration.root,
                registration.name
            );
            seen.insert(value.clone()).then_some(value)
        })
        .collect::<Vec<_>>()
        .join("；")
}

fn open_classes_key(hive: Hive, subkey: &str) -> Result<Option<RegKey>, AppError> {
    let path = format!(r"{CLASSES}\{subkey}");
    match hive
        .key()
        .open_subkey_with_flags(&path, KEY_READ | KEY_WOW64_64KEY)
    {
        Ok(key) => Ok(Some(key)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(AppError::registry("无法读取右键菜单注册表", err)),
    }
}

fn resolve_clsid_dll(clsid: &str) -> (Option<PathBuf>, bool) {
    for hive in [Hive::Hkcu, Hive::Hklm] {
        if let Some(path) = read_server_path(hive, clsid, KEY_WOW64_64KEY) {
            return (Some(path), false);
        }
    }
    for hive in [Hive::Hkcu, Hive::Hklm] {
        if let Some(path) = read_server_path(hive, clsid, KEY_WOW64_32KEY) {
            return (Some(path), true);
        }
    }
    (None, false)
}

fn read_server_path(hive: Hive, clsid: &str, flags: u32) -> Option<PathBuf> {
    let path = format!(r"{CLASSES}\CLSID\{clsid}");
    let key = hive
        .key()
        .open_subkey_with_flags(&path, KEY_READ | flags)
        .ok()?;
    for server in ["InprocServer32", "LocalServer32"] {
        let Ok(sub) = key.open_subkey(server) else {
            continue;
        };
        if let Some(codebase) = read_string(&sub, "CodeBase")
            && let Some(path) = normalize_codebase(&codebase)
        {
            return Some(path);
        }
        if let Some(default) = read_default(&sub) {
            // LocalServer32 stores a full command line; InprocServer32 stores
            // the DLL path itself.
            let path = if server == "LocalServer32" {
                parse_executable(&default)
            } else {
                Some(PathBuf::from(expand_env(default.trim_matches('"'))))
            };
            if let Some(path) = path.filter(|path| path.is_file()) {
                return Some(path);
            }
        }
    }
    None
}

fn normalize_codebase(value: &str) -> Option<PathBuf> {
    let trimmed = value.trim();
    let stripped = trimmed
        .strip_prefix("file:///")
        .or_else(|| trimmed.strip_prefix("file://"))
        .unwrap_or(trimmed);
    let path = PathBuf::from(expand_env(&stripped.replace('/', r"\")));
    path.is_file().then_some(path)
}

fn read_clsid_name(clsid: &str) -> Option<String> {
    for hive in [Hive::Hkcu, Hive::Hklm] {
        let path = format!(r"{CLASSES}\CLSID\{clsid}");
        if let Ok(key) = hive
            .key()
            .open_subkey_with_flags(&path, KEY_READ | KEY_WOW64_64KEY)
            && let Some(name) = read_default(&key)
        {
            return Some(name);
        }
    }
    None
}

fn read_approved(hive: Hive, clsid: &str) -> Option<String> {
    let key = hive
        .key()
        .open_subkey_with_flags(APPROVED, KEY_READ | KEY_WOW64_64KEY)
        .ok()?;
    read_string(&key, clsid)
}

fn read_blocked(hive: Hive, clsid: &str) -> Option<String> {
    let key = hive
        .key()
        .open_subkey_with_flags(BLOCKED, KEY_READ | KEY_WOW64_64KEY)
        .ok()?;
    let value = key.get_raw_value(clsid).ok()?;
    Some(decode_reg_string(&value).unwrap_or_default())
}

fn resolve_command_target(command: &str) -> Option<PathBuf> {
    let expanded = expand_env(command);
    let exe = parse_executable(&expanded)?;
    let file_name = exe.file_name()?.to_string_lossy().to_ascii_lowercase();
    if matches!(file_name.as_str(), "rundll32.exe" | "regsvr32.exe")
        && let Some(dll) = find_dll_argument(&expanded)
    {
        return Some(dll);
    }
    Some(exe)
}

fn find_dll_argument(command: &str) -> Option<PathBuf> {
    let lower = command.to_ascii_lowercase();
    let mut start = 0;
    while let Some(relative) = lower[start..].find(".dll") {
        let end = start + relative + 4;
        let mut token_start = start + relative;
        while token_start > 0 {
            let previous = command.as_bytes()[token_start - 1];
            if previous.is_ascii_whitespace() || previous == b'"' {
                break;
            }
            token_start -= 1;
        }
        let token = command[token_start..end].trim_matches('"').trim();
        if !token.is_empty() {
            let path = PathBuf::from(expand_env(token));
            if path.is_file() {
                return Some(path);
            }
        }
        start = end;
    }
    None
}

fn is_indirect_interpreter(path: &Path) -> bool {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    matches!(
        file_name.as_str(),
        "cmd.exe" | "powershell.exe" | "pwsh.exe" | "mshta.exe" | "wscript.exe" | "cscript.exe"
    )
}

const MICROSOFT_VERB_KEYS: &[&str] = &["updateencryptionsettings", "updateencryptionsettingswork"];
// Windows Media Player Play/Enqueue handlers do not expose a DLL through the
// classic CLSID keys, so identify them by CLSID.
const MICROSOFT_HANDLER_CLSIDS: &[&str] = &[
    "{ED1D0FDF-4414-470A-A56D-CFB68623FC58}",
    "{45597C98-80F6-4549-84FF-752CF55E2D29}",
];

fn is_microsoft_item(item: &StartupItem) -> bool {
    if is_microsoft_publisher(item.publisher.as_deref()) {
        return true;
    }
    if let Some(command) = item.command.as_deref() {
        let upper = command.to_ascii_uppercase();
        if MICROSOFT_HANDLER_CLSIDS
            .iter()
            .any(|clsid| upper.contains(clsid))
        {
            return true;
        }
    }
    let origin = item.origin.to_ascii_lowercase();
    if origin.contains(r"\shell\windows.") || origin.contains(r"\shell\.spotlight") {
        return true;
    }
    if let Some((_, verb)) = origin.rsplit_once(r"\shell\")
        && MICROSOFT_VERB_KEYS.contains(&verb)
    {
        return true;
    }
    false
}

fn is_microsoft_publisher(publisher: Option<&str>) -> bool {
    publisher.is_some_and(|value| value.to_ascii_lowercase().contains("microsoft"))
}

/// Publisher and signature for a context-menu entry. The Microsoft filter runs
/// on the publisher, so verifying the signature of an item that is about to be
/// dropped only costs a WinVerifyTrust round trip.
fn describe(exe_path: Option<&Path>, metadata: bool) -> (Option<String>, SignatureStatus) {
    if !metadata {
        return (None, SignatureStatus::Unknown);
    }
    let publisher = exe_path.and_then(signature::publisher_for);
    let status = if is_microsoft_publisher(publisher.as_deref()) {
        SignatureStatus::Unknown
    } else {
        exe_path
            .map(signature::verify)
            .unwrap_or(SignatureStatus::Unknown)
    };
    (publisher, status)
}

fn is_protected_verb(verb: &str) -> bool {
    let lower = verb.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "open" | "opennewwindow" | "print" | "printto" | "runas" | "pintohome"
    ) || lower.starts_with("windows.")
        || lower.starts_with(".spotlight")
}

fn resolve_shell_text(raw: &str, fallback: &str) -> String {
    crate::util::shell_display_text(raw).unwrap_or_else(|| fallback.to_string())
}

fn normalize_clsid(value: &str) -> String {
    let inner = value.trim().trim_start_matches('{').trim_end_matches('}');
    format!("{{{}}}", inner.to_ascii_uppercase())
}

fn is_clsid(value: &str) -> bool {
    let inner = value.trim_start_matches('{').trim_end_matches('}');
    inner.len() == 36
        && inner
            .chars()
            .all(|character| character.is_ascii_hexdigit() || character == '-')
}

fn has_value(key: &RegKey, name: &str) -> bool {
    key.get_raw_value(name).is_ok()
}

fn read_string(key: &RegKey, name: &str) -> Option<String> {
    key.get_raw_value(name)
        .ok()
        .and_then(|value| decode_reg_string(&value))
}

fn read_default(key: &RegKey) -> Option<String> {
    read_string(key, "")
}

#[cfg(test)]
mod tests {
    use super::*;
    use winreg::enums::HKEY_CURRENT_USER;

    struct Cleanup(String);

    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(&self.0);
        }
    }

    fn setup(prefix: &str) -> (Cleanup, RegKey, String) {
        let root = format!(r"Software\Classes\StartupManagerTestScan{prefix}");
        let subkey = format!("StartupManagerTestScan{prefix}\\shell\\Test");
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let _ = hkcu.delete_subkey_all(&root);
        let key = hkcu
            .create_subkey(format!(r"{root}\shell\Test"))
            .expect("create test verb")
            .0;
        (Cleanup(root), key, subkey)
    }

    #[test]
    fn parses_a_verb_and_builds_an_action() {
        let (_cleanup, key, subkey) = setup("A");
        key.set_value("MUIVerb", &"Test Menu").expect("set name");
        let command = key.create_subkey("command").expect("create command").0;
        command
            .set_value("", &r#""C:\Windows\System32\notepad.exe" "%1""#)
            .expect("set command");

        let item = build_verb_item(Hive::Hkcu, &subkey, "Test", &key, false).expect("build item");

        assert_eq!(item.name, "Test Menu");
        assert_eq!(
            item.command.as_deref(),
            Some(r#""C:\Windows\System32\notepad.exe" "%1""#)
        );
        assert!(item.enabled);
        assert!(!item.readonly);
        assert!(matches!(
            item.action,
            Some(ActionRef::ContextMenuVerb { .. })
        ));
    }

    #[test]
    fn detects_a_foreign_hide_marker() {
        let (_cleanup, key, subkey) = setup("B");
        key.set_value("ProgrammaticAccessOnly", &"")
            .expect("seed hide");

        let item = build_verb_item(Hive::Hkcu, &subkey, "Test", &key, false).expect("build item");

        assert!(!item.enabled);
        assert!(item.readonly);
        assert!(item.action.is_none());
        assert!(
            item.warnings
                .iter()
                .any(|warning| warning.contains("其他工具"))
        );
    }

    #[test]
    fn recognizes_our_own_hide_marker() {
        let (_cleanup, key, subkey) = setup("C");
        key.set_value("ProgrammaticAccessOnly", &"")
            .expect("seed hide");
        key.set_value(VERB_MARKER, &"hidden").expect("seed marker");

        let item = build_verb_item(Hive::Hkcu, &subkey, "Test", &key, false).expect("build item");

        assert!(!item.enabled);
        assert!(!item.readonly);
        assert!(matches!(
            item.action,
            Some(ActionRef::ContextMenuVerb { .. })
        ));
    }

    #[test]
    fn keeps_a_handler_without_a_module_manageable() {
        let clsid = format!("{{{}}}", uuid::Uuid::new_v4().to_string().to_uppercase());
        let registrations = vec![HandlerRegistration {
            clsid,
            hive: Hive::Hkcu,
            root: "*".to_string(),
            name: "Missing".to_string(),
        }];
        let mut errors = Vec::new();

        let items = build_handlers(&registrations, false, &mut errors);

        assert_eq!(items.len(), 1);
        assert!(
            !items[0].readonly,
            "a missing module must not make the switch read-only"
        );
        assert!(items[0].action.is_some());
        assert!(
            items[0]
                .warnings
                .iter()
                .any(|warning| warning.contains("DLL"))
        );
        assert!(items[0].machine_wide);
    }
}
