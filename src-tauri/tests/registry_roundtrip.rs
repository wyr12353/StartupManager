#![cfg(windows)]

use startupmanager_lib::action;
use startupmanager_lib::model::{
    ActionRef, Hive, RegView, Scope, SignatureStatus, SourceKind, StartupItem,
};
use startupmanager_lib::startup_approved::{self, ApprovedState};
use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;

const STATE_ROOT: &str = r"Software\StartupManagerTestState";
const ACTION_ROOT: &str = r"Software\StartupManagerTestAction";
const FAMILY_REENABLED_ROOT: &str = r"Software\StartupManagerTestFamilyReenabled";
const FAMILY_BITS_ROOT: &str = r"Software\StartupManagerTestFamilyBits";

struct Cleanup(&'static str);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(self.0);
    }
}

fn clean_start(root: &'static str) -> Cleanup {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let _ = hkcu.delete_subkey_all(root);
    Cleanup(root)
}

/// Same semantics the old library helper had: a missing value reads as
/// "enabled" so Explorer's default holds.
fn read_state(hive: Hive, subkey: &str, value_name: &str) -> ApprovedState {
    startup_approved::try_read_state_opt(hive, subkey, value_name)
        .ok()
        .flatten()
        .unwrap_or(ApprovedState::Enabled)
}

#[test]
fn startup_approved_state_roundtrip() {
    let _cleanup = clean_start(STATE_ROOT);
    let subkey = format!(r"{STATE_ROOT}\StartupApproved\Run");

    assert_eq!(
        read_state(Hive::Hkcu, &subkey, "Roundtrip"),
        ApprovedState::Enabled,
        "missing value means enabled"
    );

    startup_approved::write_state(Hive::Hkcu, &subkey, "Roundtrip", false).expect("disable");
    assert_eq!(
        read_state(Hive::Hkcu, &subkey, "Roundtrip"),
        ApprovedState::Disabled
    );

    startup_approved::write_state(Hive::Hkcu, &subkey, "Roundtrip", true).expect("enable");
    assert_eq!(
        read_state(Hive::Hkcu, &subkey, "Roundtrip"),
        ApprovedState::Enabled
    );
}

#[test]
fn action_path_roundtrip() {
    let _cleanup = clean_start(ACTION_ROOT);
    let subkey = format!(r"{ACTION_ROOT}\StartupApproved\Run");

    let item = StartupItem {
        id: "test-action".to_string(),
        name: "Test Action".to_string(),
        command: Some(r"C:\Windows\System32\cmd.exe".to_string()),
        exe_path: None,
        publisher: None,
        signature: SignatureStatus::Unknown,
        source: SourceKind::RegistryRun,
        scope: Scope::CurrentUser,
        view: Some(RegView::X64),
        enabled: true,
        readonly: false,
        origin: "test".to_string(),
        warnings: Vec::new(),
        action: Some(ActionRef::StartupApproved {
            hive: Hive::Hkcu,
            subkey: subkey.clone(),
            value_name: "ActionTest".to_string(),
        }),
    };

    action::set_enabled(&item, false).expect("disable");
    assert_eq!(
        read_state(Hive::Hkcu, &subkey, "ActionTest"),
        ApprovedState::Disabled
    );

    action::set_enabled(&item, true).expect("enable");
    assert_eq!(
        read_state(Hive::Hkcu, &subkey, "ActionTest"),
        ApprovedState::Enabled
    );
}

#[test]
fn preserves_reenabled_family() {
    let _cleanup = clean_start(FAMILY_REENABLED_ROOT);
    let subkey = format!(r"{FAMILY_REENABLED_ROOT}\StartupApproved\Run");

    let key = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey(&subkey)
        .expect("create key")
        .0;
    let blob = [6u8, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8];
    key.set_raw_value(
        "FamilyTest",
        &winreg::RegValue {
            bytes: blob.to_vec().into(),
            vtype: winreg::enums::REG_BINARY,
        },
    )
    .expect("seed 0x06 blob");

    startup_approved::write_state(Hive::Hkcu, &subkey, "FamilyTest", false).expect("disable");
    let raw = key.get_raw_value("FamilyTest").expect("read blob");
    assert_eq!(raw.bytes[0], 0x07, "0x06 family must become 0x07");

    startup_approved::write_state(Hive::Hkcu, &subkey, "FamilyTest", true).expect("enable");
    let raw = key.get_raw_value("FamilyTest").expect("read blob");
    assert_eq!(raw.bytes[0], 0x06, "0x07 family must become 0x06");
}

#[test]
fn preserves_other_family_bits() {
    let _cleanup = clean_start(FAMILY_BITS_ROOT);
    let subkey = format!(r"{FAMILY_BITS_ROOT}\StartupApproved\Run");

    let key = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey(&subkey)
        .expect("create key")
        .0;
    let blob = [10u8, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8];
    key.set_raw_value(
        "FamilyBits",
        &winreg::RegValue {
            bytes: blob.to_vec().into(),
            vtype: winreg::enums::REG_BINARY,
        },
    )
    .expect("seed 0x0A blob");

    startup_approved::write_state(Hive::Hkcu, &subkey, "FamilyBits", false).expect("disable");
    let raw = key.get_raw_value("FamilyBits").expect("read blob");
    assert_eq!(raw.bytes[0], 0x0B, "0x0A family must become 0x0B");

    startup_approved::write_state(Hive::Hkcu, &subkey, "FamilyBits", true).expect("enable");
    let raw = key.get_raw_value("FamilyBits").expect("read blob");
    assert_eq!(raw.bytes[0], 0x0A, "0x0B family must become 0x0A");
}
