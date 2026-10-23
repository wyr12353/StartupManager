use std::collections::HashMap;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use windows::Win32::Foundation::{
    CERT_E_CHAINING, CERT_E_EXPIRED, CERT_E_INVALID_NAME, CERT_E_REVOCATION_FAILURE,
    CERT_E_REVOKED, CERT_E_UNTRUSTEDROOT, CERT_E_WRONG_USAGE, HANDLE, HWND, TRUST_E_BAD_DIGEST,
    TRUST_E_EXPLICIT_DISTRUST, TRUST_E_NOSIGNATURE, TRUST_E_PROVIDER_UNKNOWN,
    TRUST_E_SUBJECT_FORM_UNKNOWN,
};
use windows::Win32::Security::WinTrust::{
    WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_DATA_UICONTEXT,
    WINTRUST_FILE_INFO, WTD_CHOICE_FILE, WTD_REVOKE_NONE, WTD_SAFER_FLAG, WTD_STATEACTION_CLOSE,
    WTD_STATEACTION_VERIFY, WTD_UI_NONE, WinVerifyTrustEx,
};
use windows::core::{PCWSTR, PWSTR};

use crate::model::SignatureStatus;
use crate::util::is_remote_path;

static PUBLISHER_CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
static SIGNATURE_CACHE: OnceLock<Mutex<HashMap<String, SignatureStatus>>> = OnceLock::new();

fn cache_key(path: &Path) -> String {
    let base = path.to_string_lossy().to_ascii_lowercase();
    match std::fs::metadata(path) {
        Ok(meta) => {
            let modified = meta
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_nanos())
                .unwrap_or(0);
            format!("{base}|{}|{modified}", meta.len())
        }
        Err(_) => base,
    }
}

/// Reads the CompanyName from the file's version resource. This is cheap and
/// runs during the initial scan, so rows can show a publisher immediately.
pub fn publisher_for(path: &Path) -> Option<String> {
    if is_remote_path(path) {
        return None;
    }
    let key = cache_key(path);
    let cache = PUBLISHER_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(cached) = cache.lock().ok().and_then(|map| map.get(&key).cloned()) {
        return cached;
    }
    let value = version_string(path, "CompanyName");
    if let Ok(mut map) = cache.lock() {
        if map.len() >= 2048 {
            map.clear();
        }
        map.insert(key, value.clone());
    }
    value
}

/// Publisher and signature status for a scanned executable. `metadata` is
/// false during snapshot re-scans, where only raw items are needed.
pub fn describe(exe_path: Option<&Path>, metadata: bool) -> (Option<String>, SignatureStatus) {
    if !metadata {
        return (None, SignatureStatus::Unknown);
    }
    (
        exe_path.and_then(publisher_for),
        exe_path.map(verify).unwrap_or(SignatureStatus::Unknown),
    )
}

fn version_string(path: &Path, key: &str) -> Option<String> {
    use windows::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let size = unsafe { GetFileVersionInfoSizeW(PCWSTR(wide.as_ptr()), None) };
    if size == 0 {
        return None;
    }
    let mut data = vec![0u8; size as usize];
    unsafe {
        GetFileVersionInfoW(PCWSTR(wide.as_ptr()), None, size, data.as_mut_ptr().cast()).ok()?;
    }

    let mut translations_ptr: *mut core::ffi::c_void = std::ptr::null_mut();
    let mut translations_len = 0u32;
    let translation_query: Vec<u16> = "\\VarFileInfo\\Translation\0".encode_utf16().collect();
    let ok = unsafe {
        VerQueryValueW(
            data.as_ptr().cast(),
            PCWSTR(translation_query.as_ptr()),
            &mut translations_ptr,
            &mut translations_len,
        )
    };
    if !ok.as_bool() || translations_ptr.is_null() || translations_len < 4 {
        return None;
    }

    let lang = unsafe { *(translations_ptr as *const u16) };
    let codepage = unsafe { *((translations_ptr as *const u16).add(1)) };
    let sub_block = format!("\\StringFileInfo\\{lang:04x}{codepage:04x}\\{key}\0");
    let sub_block: Vec<u16> = sub_block.encode_utf16().collect();
    let mut value_ptr: *mut core::ffi::c_void = std::ptr::null_mut();
    let mut value_len = 0u32;
    let ok = unsafe {
        VerQueryValueW(
            data.as_ptr().cast(),
            PCWSTR(sub_block.as_ptr()),
            &mut value_ptr,
            &mut value_len,
        )
    };
    if !ok.as_bool() || value_ptr.is_null() || value_len == 0 {
        return None;
    }
    let slice = unsafe { std::slice::from_raw_parts(value_ptr as *const u16, value_len as usize) };
    let trimmed = slice
        .iter()
        .position(|c| *c == 0)
        .map(|end| &slice[..end])
        .unwrap_or(slice);
    let text = String::from_utf16_lossy(trimmed).trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

/// Verifies the embedded Authenticode signature. A file without an embedded
/// signature is reported as Unknown, not Unsigned, because Windows system
/// files are commonly signed through a catalog instead.
pub fn verify(path: &Path) -> SignatureStatus {
    if is_remote_path(path) {
        return SignatureStatus::Unknown;
    }
    let key = cache_key(path);
    let cache = SIGNATURE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(cached) = cache.lock().ok().and_then(|map| map.get(&key).copied()) {
        return cached;
    }
    let status = verify_uncached(path);
    if let Ok(mut map) = cache.lock() {
        if map.len() >= 2048 {
            map.clear();
        }
        map.insert(key, status);
    }
    status
}

fn verify_uncached(path: &Path) -> SignatureStatus {
    if !path.exists() {
        return SignatureStatus::Unknown;
    }
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let mut file_info = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(wide.as_ptr()),
        hFile: HANDLE::default(),
        pgKnownSubject: std::ptr::null_mut(),
    };

    let mut data = WINTRUST_DATA {
        cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
        pPolicyCallbackData: std::ptr::null_mut(),
        pSIPClientData: std::ptr::null_mut(),
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 {
            pFile: &mut file_info,
        },
        dwStateAction: WTD_STATEACTION_VERIFY,
        hWVTStateData: HANDLE::default(),
        pwszURLReference: PWSTR::null(),
        dwProvFlags: WTD_SAFER_FLAG,
        dwUIContext: WINTRUST_DATA_UICONTEXT(0),
        pSignatureSettings: std::ptr::null_mut(),
    };

    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    let status = unsafe { WinVerifyTrustEx(HWND::default(), &mut action, &mut data) };

    data.dwStateAction = WTD_STATEACTION_CLOSE;
    let _ = unsafe { WinVerifyTrustEx(HWND::default(), &mut action, &mut data) };

    classify_verify_status(status)
}

fn classify_verify_status(status: i32) -> SignatureStatus {
    if status == 0 {
        return SignatureStatus::Verified;
    }
    match status {
        // No embedded Authenticode signature. Windows system files are often
        // catalog-signed, so this is "unknown", not "unsigned".
        s if s == TRUST_E_NOSIGNATURE.0
            || s == TRUST_E_SUBJECT_FORM_UNKNOWN.0
            || s == TRUST_E_PROVIDER_UNKNOWN.0 =>
        {
            SignatureStatus::Unknown
        }
        // The signature exists but the digest, chain, validity or usage is
        // wrong. This must not be reported as "unsigned".
        s if s == TRUST_E_BAD_DIGEST.0
            || s == TRUST_E_EXPLICIT_DISTRUST.0
            || s == CERT_E_UNTRUSTEDROOT.0
            || s == CERT_E_CHAINING.0
            || s == CERT_E_EXPIRED.0
            || s == CERT_E_REVOKED.0
            || s == CERT_E_REVOCATION_FAILURE.0
            || s == CERT_E_WRONG_USAGE.0
            || s == CERT_E_INVALID_NAME.0 =>
        {
            SignatureStatus::Invalid
        }
        // Anything else (I/O failure, provider error, trust service failure)
        // is unknown; guessing "unsigned" would be misleading.
        _ => SignatureStatus::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::classify_verify_status;
    use crate::model::SignatureStatus;
    use windows::Win32::Foundation::{
        CERT_E_EXPIRED, CERT_E_UNTRUSTEDROOT, CERT_E_WRONG_USAGE, TRUST_E_BAD_DIGEST,
        TRUST_E_NOSIGNATURE, TRUST_E_SUBJECT_FORM_UNKNOWN,
    };

    #[test]
    fn classifies_verify_results() {
        assert_eq!(classify_verify_status(0), SignatureStatus::Verified);
        assert_eq!(
            classify_verify_status(TRUST_E_NOSIGNATURE.0),
            SignatureStatus::Unknown
        );
        assert_eq!(
            classify_verify_status(TRUST_E_SUBJECT_FORM_UNKNOWN.0),
            SignatureStatus::Unknown
        );
        assert_eq!(
            classify_verify_status(TRUST_E_BAD_DIGEST.0),
            SignatureStatus::Invalid
        );
        assert_eq!(
            classify_verify_status(CERT_E_UNTRUSTEDROOT.0),
            SignatureStatus::Invalid
        );
        assert_eq!(
            classify_verify_status(CERT_E_EXPIRED.0),
            SignatureStatus::Invalid
        );
        assert_eq!(
            classify_verify_status(CERT_E_WRONG_USAGE.0),
            SignatureStatus::Invalid
        );
        assert_eq!(
            classify_verify_status(0x8007_0005u32 as i32),
            SignatureStatus::Unknown
        );
    }
}
