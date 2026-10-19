use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum SourceKind {
    RegistryRun,
    StartupFolder,
    Uwp,
    ScheduledTask,
    Service,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    CurrentUser,
    AllUsers,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum RegView {
    #[serde(rename = "64")]
    X64,
    #[serde(rename = "32")]
    X86,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SignatureStatus {
    Verified,
    Unsigned,
    Invalid,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Hive {
    Hkcu,
    Hklm,
}

impl Hive {
    pub fn label(self) -> &'static str {
        match self {
            Hive::Hkcu => "HKCU",
            Hive::Hklm => "HKLM",
        }
    }

    pub fn key(self) -> winreg::RegKey {
        match self {
            Hive::Hkcu => winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER),
            Hive::Hklm => winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE),
        }
    }
}

/// Backend-only information needed to perform a write. Never sent to the UI:
/// the UI only ever passes a stable id, and the backend re-enumerates to find
/// the matching item before acting.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ActionRef {
    StartupApproved {
        hive: Hive,
        subkey: String,
        value_name: String,
    },
    UwpStartupTask {
        pfn: String,
        task_id: String,
    },
    ScheduledTask {
        path: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupItem {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(
        default,
        serialize_with = "serialize_optional_path",
        deserialize_with = "deserialize_optional_path"
    )]
    pub exe_path: Option<PathBuf>,
    #[serde(default)]
    pub publisher: Option<String>,
    pub signature: SignatureStatus,
    pub source: SourceKind,
    pub scope: Scope,
    #[serde(default)]
    pub view: Option<RegView>,
    pub enabled: bool,
    pub readonly: bool,
    pub origin: String,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(skip)]
    pub action: Option<ActionRef>,
}

fn serialize_optional_path<S>(value: &Option<PathBuf>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match value {
        Some(path) => serializer.serialize_some(&path.to_string_lossy()),
        None => serializer.serialize_none(),
    }
}

fn deserialize_optional_path<'de, D>(deserializer: D) -> Result<Option<PathBuf>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    Ok(value.map(PathBuf::from))
}

impl StartupItem {
    pub fn requires_elevation(&self) -> bool {
        if self.readonly {
            return false;
        }
        match &self.action {
            Some(ActionRef::StartupApproved { hive, .. }) => *hive == Hive::Hklm,
            Some(ActionRef::ScheduledTask { .. }) => self.scope == Scope::AllUsers,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub source: SourceKind,
    pub items: Vec<StartupItem>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppState {
    pub is_elevated: bool,
    pub version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub source: SourceKind,
    pub completed: usize,
    pub total: usize,
}
