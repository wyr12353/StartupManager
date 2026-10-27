#![cfg(windows)]

use startupmanager_lib::action;
use startupmanager_lib::model::{ActionRef, Hive, Scope, SignatureStatus, SourceKind, StartupItem};
use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WOW64_64KEY, KEY_WRITE};

const BLOCKED: &str = r"Software\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked";
const MARKER: &str = "StartupManager";

struct Cleanup(String);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(&self.0);
    }
}

fn clean_start(root: &str) -> Cleanup {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let _ = hkcu.delete_subkey_all(root);
    Cleanup(root.to_string())
}

struct BlockedCleanup {
    clsid: String,
}

impl Drop for BlockedCleanup {
    fn drop(&mut self) {
        if let Ok(key) = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(BLOCKED, KEY_READ | KEY_WRITE | KEY_WOW64_64KEY)
        {
            let _ = key.delete_value(&self.clsid);
        }
    }
}

fn verb_item(subkey: &str) -> StartupItem {
    StartupItem {
        id: "test-context-verb".to_string(),
        name: "Test verb".to_string(),
        command: Some(r"C:\Windows\System32\notepad.exe".to_string()),
        exe_path: None,
        publisher: None,
        signature: SignatureStatus::Unknown,
        source: SourceKind::ContextMenu,
        scope: Scope::CurrentUser,
        view: None,
        enabled: true,
        readonly: false,
        machine_wide: false,
        origin: format!(r"HKCU\Software\Classes\{subkey}"),
        warnings: Vec::new(),
        action: Some(ActionRef::ContextMenuVerb {
            hive: Hive::Hkcu,
            subkey: subkey.to_string(),
        }),
    }
}

fn blocked_item_with_hive(clsid: &str, hive: Hive) -> StartupItem {
    StartupItem {
        id: "test-context-handler".to_string(),
        name: "Test handler".to_string(),
        command: Some(format!("COM {clsid}")),
        exe_path: None,
        publisher: None,
        signature: SignatureStatus::Unknown,
        source: SourceKind::ContextMenu,
        scope: if hive == Hive::Hkcu {
            Scope::CurrentUser
        } else {
            Scope::AllUsers
        },
        view: None,
        enabled: true,
        readonly: false,
        machine_wide: hive == Hive::Hklm,
        origin: r"HKCU\Software\Classes\*\shellex\ContextMenuHandlers\Test".to_string(),
        warnings: Vec::new(),
        action: Some(ActionRef::ShellExtensionBlocked {
            clsid: clsid.to_string(),
            hive,
        }),
    }
}

fn blocked_item(clsid: &str) -> StartupItem {
    blocked_item_with_hive(clsid, Hive::Hkcu)
}

#[test]
fn hklm_handler_requires_elevation() {
    let clsid = "{00000000-0000-0000-0000-000000000000}";
    assert!(blocked_item_with_hive(clsid, Hive::Hklm).requires_elevation());
    assert!(!blocked_item_with_hive(clsid, Hive::Hkcu).requires_elevation());
}

fn create_verb_key(subkey: &str) -> RegKey {
    RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey(format!(r"Software\Classes\{subkey}"))
        .expect("create verb key")
        .0
}

#[test]
fn verb_hide_roundtrip() {
    let _cleanup = clean_start(r"Software\Classes\StartupManagerTestVerb");
    let subkey = r"StartupManagerTestVerb\shell\Test";
    let _key = create_verb_key(subkey);
    let item = verb_item(subkey);

    action::set_enabled(&item, false).expect("hide verb");
    {
        let key = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(format!(r"Software\Classes\{subkey}"), KEY_READ)
            .expect("open hidden verb");
        assert!(key.get_raw_value("ProgrammaticAccessOnly").is_ok());
        assert!(key.get_raw_value("LegacyDisable").is_ok());
        assert!(key.get_raw_value("StartupManager.ContextMenu").is_ok());
    }

    action::set_enabled(&item, true).expect("show verb");
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(format!(r"Software\Classes\{subkey}"), KEY_READ)
        .expect("open shown verb");
    assert!(key.get_raw_value("ProgrammaticAccessOnly").is_err());
    assert!(key.get_raw_value("LegacyDisable").is_err());
    assert!(key.get_raw_value("StartupManager.ContextMenu").is_err());
}

#[test]
fn verb_refuses_to_remove_foreign_hide_marker() {
    let _cleanup = clean_start(r"Software\Classes\StartupManagerTestVerbForeign");
    let subkey = r"StartupManagerTestVerbForeign\shell\Foreign";
    let key = create_verb_key(subkey);
    key.set_value("ProgrammaticAccessOnly", &"")
        .expect("seed hide");
    let item = verb_item(subkey);

    let error = action::set_enabled(&item, true).expect_err("must refuse to show");
    assert_eq!(error.code, "UNSUPPORTED");

    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(format!(r"Software\Classes\{subkey}"), KEY_READ)
        .expect("open foreign verb");
    assert!(key.get_raw_value("ProgrammaticAccessOnly").is_ok());
    assert!(key.get_raw_value("StartupManager.ContextMenu").is_err());
}

#[test]
fn shell_extension_block_roundtrip() {
    let clsid = format!("{{{}}}", uuid::Uuid::new_v4().to_string().to_uppercase());
    let _cleanup = BlockedCleanup {
        clsid: clsid.clone(),
    };
    let item = blocked_item(&clsid);

    action::set_enabled(&item, false).expect("block handler");
    {
        let key = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(BLOCKED, KEY_READ)
            .expect("open blocked list");
        let value: String = key.get_value(&clsid).expect("blocked value");
        assert_eq!(value, MARKER);
    }

    action::set_enabled(&item, true).expect("unblock handler");
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(BLOCKED, KEY_READ)
        .expect("open blocked list");
    assert!(key.get_raw_value(&clsid).is_err());
}

#[test]
fn machine_scope_toggle_clears_stale_user_block() {
    let clsid = format!("{{{}}}", uuid::Uuid::new_v4().to_string().to_uppercase());
    let _cleanup = BlockedCleanup {
        clsid: clsid.clone(),
    };
    let hkcu_item = blocked_item(&clsid);
    action::set_enabled(&hkcu_item, false).expect("seed user block");

    let hklm_item = blocked_item_with_hive(&clsid, Hive::Hklm);
    action::clear_stale_user_block(&hklm_item);

    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(BLOCKED, KEY_READ)
        .expect("open blocked list");
    assert!(key.get_raw_value(&clsid).is_err());
}

#[test]
fn shell_extension_refuses_to_remove_foreign_block() {
    let clsid = format!("{{{}}}", uuid::Uuid::new_v4().to_string().to_uppercase());
    let _cleanup = BlockedCleanup {
        clsid: clsid.clone(),
    };
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey_with_flags(BLOCKED, KEY_READ | KEY_WRITE | KEY_WOW64_64KEY)
        .expect("create blocked list");
    key.set_value(&clsid, &"OtherTool")
        .expect("seed foreign block");
    let item = blocked_item(&clsid);

    let error = action::set_enabled(&item, true).expect_err("must refuse to unblock");
    assert_eq!(error.code, "UNSUPPORTED");
    assert!(key.get_raw_value(&clsid).is_ok());
}
