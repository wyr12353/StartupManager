//! Windows account identity helpers.
//!
//! The GUI always starts elevated. When UAC is answered with a *different*
//! administrator account, the elevated process owns that administrator's
//! `HKCU`, so every current-user item would silently belong to the wrong
//! profile. The launching (unelevated) account's SID is forwarded across the
//! elevation boundary so the UI can warn about that situation.

use std::sync::OnceLock;

use windows::Win32::Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::{ConvertSidToStringSidW, ConvertStringSidToSidW};
use windows::Win32::Security::{
    GetTokenInformation, LookupAccountSidW, PSID, SID_NAME_USE, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::core::{PCWSTR, PWSTR};

#[derive(Debug, Clone)]
pub struct Account {
    pub sid: String,
    pub name: String,
}

static CURRENT: OnceLock<Option<Account>> = OnceLock::new();
static ORIGINAL_SID: OnceLock<Option<String>> = OnceLock::new();

/// The account that owns the current process token.
pub fn current() -> Option<&'static Account> {
    CURRENT.get_or_init(load_current).as_ref()
}

pub fn current_sid() -> Option<String> {
    current().map(|account| account.sid.clone())
}

/// Records the account that launched the process before it was elevated.
pub fn remember_original_sid(sid: Option<String>) {
    let _ = ORIGINAL_SID.set(sid);
}

#[derive(Debug, Clone)]
pub struct ElevationStatus {
    /// Account of the elevated token (the one whose HKCU is being read).
    pub current_user: Option<String>,
    /// Account that started the process before UAC.
    pub original_user: Option<String>,
    /// True when UAC was answered with a different account.
    pub mismatch: bool,
}

pub fn elevation_status() -> ElevationStatus {
    let current = current();
    let original_sid = ORIGINAL_SID.get().and_then(|value| value.as_deref());
    match (original_sid, current) {
        (Some(original), Some(current)) => ElevationStatus {
            current_user: Some(current.name.clone()),
            original_user: account_name_for_sid(original),
            mismatch: !original.eq_ignore_ascii_case(&current.sid),
        },
        (Some(original), None) => ElevationStatus {
            current_user: None,
            original_user: account_name_for_sid(original),
            mismatch: false,
        },
        (None, Some(current)) => ElevationStatus {
            current_user: Some(current.name.clone()),
            original_user: None,
            mismatch: false,
        },
        (None, None) => ElevationStatus {
            current_user: None,
            original_user: None,
            mismatch: false,
        },
    }
}

fn load_current() -> Option<Account> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).ok()?;
        let account = read_token_account(token);
        let _ = CloseHandle(token);
        account
    }
}

unsafe fn read_token_account(token: HANDLE) -> Option<Account> {
    let mut length = 0u32;
    let _ = unsafe { GetTokenInformation(token, TokenUser, None, 0, &mut length) };
    if length == 0 {
        return None;
    }
    let words = (length as usize).div_ceil(std::mem::size_of::<usize>());
    let mut buffer = vec![0usize; words];
    unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            Some(buffer.as_mut_ptr().cast()),
            length,
            &mut length,
        )
    }
    .ok()?;
    let sid = unsafe { (*(buffer.as_ptr() as *const TOKEN_USER)).User.Sid };

    let mut sid_ptr = PWSTR::null();
    unsafe { ConvertSidToStringSidW(sid, &mut sid_ptr) }.ok()?;
    let sid_string = unsafe { sid_ptr.to_string() }.ok()?;
    let _ = unsafe { LocalFree(Some(HLOCAL(sid_ptr.0.cast()))) };

    Some(Account {
        sid: sid_string,
        name: unsafe { lookup_account(sid) }.unwrap_or_default(),
    })
}

pub fn account_name_for_sid(sid: &str) -> Option<String> {
    let wide: Vec<u16> = sid.encode_utf16().chain(std::iter::once(0)).collect();
    let mut psid = PSID::default();
    unsafe { ConvertStringSidToSidW(PCWSTR(wide.as_ptr()), &mut psid) }.ok()?;
    let name = unsafe { lookup_account(psid) };
    let _ = unsafe { LocalFree(Some(HLOCAL(psid.0))) };
    name
}

unsafe fn lookup_account(sid: PSID) -> Option<String> {
    let mut name_len = 0u32;
    let mut domain_len = 0u32;
    let mut use_type = SID_NAME_USE(0);
    let _ = unsafe {
        LookupAccountSidW(
            PCWSTR::null(),
            sid,
            None,
            &mut name_len,
            None,
            &mut domain_len,
            &mut use_type,
        )
    };
    if name_len == 0 {
        return None;
    }

    let mut name_buf = vec![0u16; name_len as usize];
    let mut domain_buf = vec![0u16; domain_len as usize];
    let domain_arg = if domain_len == 0 {
        None
    } else {
        Some(PWSTR(domain_buf.as_mut_ptr()))
    };
    let looked_up = unsafe {
        LookupAccountSidW(
            PCWSTR::null(),
            sid,
            Some(PWSTR(name_buf.as_mut_ptr())),
            &mut name_len,
            domain_arg,
            &mut domain_len,
            &mut use_type,
        )
    };
    if looked_up.is_err() {
        return None;
    }

    let name = String::from_utf16_lossy(&name_buf[..name_len as usize])
        .trim_end_matches('\0')
        .to_string();
    let domain = String::from_utf16_lossy(&domain_buf[..domain_len as usize])
        .trim_end_matches('\0')
        .to_string();
    if domain.is_empty() {
        Some(name)
    } else {
        Some(format!("{domain}\\{name}"))
    }
}

#[cfg(test)]
mod tests {
    use super::account_name_for_sid;

    #[test]
    fn resolves_the_builtin_system_account() {
        let name = account_name_for_sid("S-1-5-18").expect("well-known system SID");
        assert!(name.to_ascii_uppercase().contains("SYSTEM"), "name: {name}");
        assert!(account_name_for_sid("not-a-sid").is_none());
    }

    #[test]
    fn current_account_has_a_sid() {
        if let Some(account) = super::current() {
            assert!(account.sid.starts_with("S-1-"));
            assert!(!account.name.is_empty());
        }
    }
}
