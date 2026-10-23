use std::time::{SystemTime, UNIX_EPOCH};

use winreg::RegValue;
use winreg::enums::{KEY_READ, KEY_WOW64_64KEY, KEY_WRITE, REG_BINARY};

use crate::error::{AppError, AppResult};
use crate::model::{Hive, RegView};

pub const APPROVED_RUN: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
pub const APPROVED_RUN32: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run32";
pub const APPROVED_STARTUP_FOLDER: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovedState {
    Enabled,
    Disabled,
    Unknown,
}

impl ApprovedState {
    pub fn is_enabled(self) -> bool {
        !matches!(self, ApprovedState::Disabled)
    }
}

pub fn overlay_subkey(view: RegView) -> &'static str {
    match view {
        RegView::X64 => APPROVED_RUN,
        RegView::X86 => APPROVED_RUN32,
    }
}

/// StartupApproved is always stored in the 64-bit view. Explorer keeps the
/// 32-bit Run overrides in `...\StartupApproved\Run32`, not in
/// `WOW6432Node\...\StartupApproved\Run32`.
/// Distinguishes "missing value" from a read error so callers can show a
/// warning instead of silently reporting "enabled".
pub fn try_read_state_opt(
    hive: Hive,
    subkey: &str,
    value_name: &str,
) -> AppResult<Option<ApprovedState>> {
    match read_blob(hive, subkey, value_name)? {
        Some(bytes) => Ok(Some(parse_blob(&bytes))),
        None => Ok(None),
    }
}

fn read_blob(hive: Hive, subkey: &str, value_name: &str) -> AppResult<Option<Vec<u8>>> {
    let key = match hive
        .key()
        .open_subkey_with_flags(subkey, KEY_READ | KEY_WOW64_64KEY)
    {
        Ok(key) => key,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(AppError::registry("无法读取启动项状态键", err)),
    };
    match key.get_raw_value(value_name) {
        Ok(value) => Ok(Some(value.bytes.to_vec())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(AppError::registry("无法读取启动项状态", err)),
    }
}

pub fn parse_blob(bytes: &[u8]) -> ApprovedState {
    if bytes.len() != 12 {
        return ApprovedState::Unknown;
    }
    if bytes[0] & 1 == 0 {
        ApprovedState::Enabled
    } else {
        ApprovedState::Disabled
    }
}

fn current_filetime() -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    (now.as_secs() + 11_644_473_600) * 10_000_000 + u64::from(now.subsec_nanos()) / 100
}

pub fn write_state(
    hive: Hive,
    subkey: &str,
    value_name: &str,
    enabled: bool,
) -> AppResult<ApprovedState> {
    let flags = KEY_READ | KEY_WRITE | KEY_WOW64_64KEY;
    let (key, _) = hive
        .key()
        .create_subkey_with_flags(subkey, flags)
        .map_err(|e| AppError::registry("无法打开启动项状态键", e))?;

    // Preserve the existing family byte (0x02/0x03, 0x06/0x07, 0x0A/0x0B, ...)
    // and only flip the low enable bit.
    let family = read_blob(hive, subkey, value_name)?
        .filter(|bytes| bytes.len() == 12)
        .map(|bytes| bytes[0] & !1)
        .unwrap_or(0x02);

    let state_byte = if enabled { family & !1 } else { family | 0x01 };
    let mut blob = [0u8; 12];
    blob[0] = state_byte;
    blob[4..12].copy_from_slice(&current_filetime().to_le_bytes());

    key.set_raw_value(
        value_name,
        &RegValue {
            bytes: blob.to_vec().into(),
            vtype: REG_BINARY,
        },
    )
    .map_err(|e| AppError::registry("无法写入启动项状态", e))?;

    // A failed read must not be treated as "enabled", otherwise a rejected
    // write can look successful.
    match read_blob(hive, subkey, value_name)? {
        Some(bytes) if bytes.len() == 12 && bytes[0] == state_byte => Ok(parse_blob(&bytes)),
        Some(bytes) => Err(
            AppError::new("STATE_VERIFY_FAILED", "启动项状态写入后校验失败")
                .with_detail(format!("期望 {state_byte:#04x}，读回 {:?}", bytes.first())),
        ),
        None => Err(AppError::new(
            "STATE_VERIFY_FAILED",
            "启动项状态写入后无法读回",
        )),
    }
}

pub fn pick_folder_value_name(
    has_exact: bool,
    has_stem: bool,
    value_name: &str,
    stem: &str,
) -> String {
    if has_exact {
        value_name.to_string()
    } else if has_stem {
        stem.to_string()
    } else {
        value_name.to_string()
    }
}

pub fn find_folder_value_name(hive: Hive, value_name: &str, stem: &str) -> String {
    let flags = KEY_READ | KEY_WOW64_64KEY;
    if let Ok(key) = hive
        .key()
        .open_subkey_with_flags(APPROVED_STARTUP_FOLDER, flags)
    {
        let has_exact = key.get_raw_value(value_name).is_ok();
        let has_stem = key.get_raw_value(stem).is_ok();
        return pick_folder_value_name(has_exact, has_stem, value_name, stem);
    }
    value_name.to_string()
}

#[cfg(test)]
mod tests {
    use super::{ApprovedState, parse_blob, pick_folder_value_name};

    #[test]
    fn parses_known_state_bytes() {
        let cases: [(&[u8], ApprovedState); 8] = [
            (
                &[2u8, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8],
                ApprovedState::Enabled,
            ),
            (
                &[6u8, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8],
                ApprovedState::Enabled,
            ),
            (
                &[3u8, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8],
                ApprovedState::Disabled,
            ),
            (
                &[7u8, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8],
                ApprovedState::Disabled,
            ),
            (
                &[10u8, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8],
                ApprovedState::Enabled,
            ),
            (
                &[11u8, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8],
                ApprovedState::Disabled,
            ),
            (&[2u8, 0, 0], ApprovedState::Unknown),
            (&[], ApprovedState::Unknown),
        ];
        for (blob, expected) in cases {
            assert_eq!(parse_blob(blob), expected, "blob {blob:?}");
        }
    }

    #[test]
    fn prefers_the_existing_folder_value_name() {
        assert_eq!(
            pick_folder_value_name(true, true, "App.lnk", "App"),
            "App.lnk"
        );
        assert_eq!(pick_folder_value_name(false, true, "App.lnk", "App"), "App");
        assert_eq!(
            pick_folder_value_name(false, false, "App.lnk", "App"),
            "App.lnk"
        );
    }
}
