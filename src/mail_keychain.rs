//! File-based macOS Keychain items ignore per-query Data Protection UI flags.
//! Serialize every mail credential operation while changing the legacy,
//! process-wide interaction flag, including foreground authorization and writes.

use std::sync::Mutex;

static ACCESS: Mutex<()> = Mutex::new(());
// Test observations of the native global also exclude unrelated UI tests that
// call the public secrets API. Keep this separate from the production lock.
#[cfg(test)]
pub(super) static TEST_POLICY: Mutex<()> = Mutex::new(());

#[link(name = "Security", kind = "framework")]
extern "C" {
    fn SecKeychainGetUserInteractionAllowed(state: *mut u8) -> i32;
    fn SecKeychainSetUserInteractionAllowed(state: u8) -> i32;
}

#[derive(Debug)]
struct InteractionError(i32);

impl std::fmt::Display for InteractionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "系统钥匙串不可用（OSStatus {}）", self.0)
    }
}
impl std::error::Error for InteractionError {}

fn check(code: i32) -> Result<(), keyring::Error> {
    if code == 0 { Ok(()) } else { Err(keyring::Error::PlatformFailure(Box::new(InteractionError(code)))) }
}

struct RestoreInteraction(u8);
impl Drop for RestoreInteraction {
    fn drop(&mut self) {
        // This guard is dropped before ACCESS is released, including on panic.
        unsafe { SecKeychainSetUserInteractionAllowed(self.0); }
    }
}

pub(super) fn with_interaction<T>(allow: bool, operation: impl FnOnce() -> Result<T, keyring::Error>) -> Result<T, keyring::Error> {
    let _access = ACCESS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut previous = 0;
    unsafe {
        check(SecKeychainGetUserInteractionAllowed(&mut previous))?;
        check(SecKeychainSetUserInteractionAllowed(u8::from(allow)))?;
    }
    let _restore = RestoreInteraction(previous);
    operation()
}

fn helper_path() -> Option<std::path::PathBuf> {
    std::env::var_os("QINGYU_DEV_MAIL_HELPER").map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join("Library/Application Support/Qingyu Development/qingyu-mail-credentials")))
}

extern "C" {
    fn qingyu_mail_verify_dev_helper(path: *const std::ffi::c_char) -> i32;
    fn qingyu_mail_verify_dev_helper_process(pid: i32) -> i32;
}

/// A local installed build may reuse the persistent helper only when its
/// certificate matches. External/ad-hoc builds keep the standard Keychain path.
pub(super) fn uses_helper() -> bool {
    if cfg!(debug_assertions) { return true; }
    use std::os::unix::ffi::OsStrExt;
    helper_path().and_then(|path| std::ffi::CString::new(path.as_os_str().as_bytes()).ok())
        .is_some_and(|path| unsafe { qingyu_mail_verify_dev_helper(path.as_ptr()) } == 0)
}

pub(super) fn helper_request(action: u8, account: &str, password: &str) -> Result<Vec<u8>, keyring::Error> {
    use std::{ffi::CString, io::{Read, Write}, os::unix::ffi::OsStrExt, process::{Command, Stdio}};
    fn failure(error: impl std::error::Error + Send + Sync + 'static) -> keyring::Error {
        keyring::Error::PlatformFailure(Box::new(error))
    }
    let helper = helper_path()
        .ok_or_else(|| failure(std::io::Error::other("请通过 cargo run 启动开发版以准备安全凭据助手")))?;
    let path = CString::new(helper.as_os_str().as_bytes()).map_err(failure)?;
    // Verify both the file and the actual child before transmitting secrets.
    check(unsafe { qingyu_mail_verify_dev_helper(path.as_ptr()) })?;
    if account.is_empty() || account.len() > 4096 || password.len() > 65536 {
        return Err(keyring::Error::Invalid("credential".into(), "凭据长度无效".into()));
    }
    let mut child = Command::new(helper).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(failure)?;
    let result = (|| {
        check(unsafe { qingyu_mail_verify_dev_helper_process(child.id() as i32) })?;
        let mut input = child.stdin.take().ok_or_else(|| failure(std::io::Error::other("凭据助手输入不可用")))?;
        input.write_all(&[action]).map_err(failure)?;
        input.write_all(&(account.len() as u32).to_be_bytes()).map_err(failure)?;
        input.write_all(&(password.len() as u32).to_be_bytes()).map_err(failure)?;
        input.write_all(account.as_bytes()).map_err(failure)?;
        input.write_all(password.as_bytes()).map_err(failure)?;
        drop(input);
        let mut output = Vec::new();
        child.stdout.take().ok_or_else(|| failure(std::io::Error::other("凭据助手输出不可用")))?
            .take(65545).read_to_end(&mut output).map_err(failure)?;
        if output.len() < 8 || output.len() > 65544 {
            return Err(failure(std::io::Error::other("凭据助手未通过验证或返回了无效响应")));
        }
        let code = i32::from_be_bytes(output[..4].try_into().unwrap());
        let length = u32::from_be_bytes(output[4..8].try_into().unwrap()) as usize;
        if length != output.len() - 8 {
            return Err(failure(std::io::Error::other("凭据助手响应长度无效")));
        }
        if code == -25300 { return Err(keyring::Error::NoEntry); }
        check(code)?;
        Ok(output.split_off(8))
    })();
    if result.is_err() { let _ = child.kill(); }
    let status = child.wait().map_err(failure)?;
    if result.is_ok() && !status.success() {
        return Err(failure(std::io::Error::other("凭据助手已退出")));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowed() -> u8 {
        let mut state = 0;
        check(unsafe { SecKeychainGetUserInteractionAllowed(&mut state) }).unwrap();
        state
    }

    #[test]
    fn background_access_disables_ui_and_restores_the_previous_policy_on_error() {
        let _test = TEST_POLICY.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let before = allowed();
        let result: Result<(), _> = with_interaction(false, || {
            assert_eq!(allowed(), 0);
            Err(keyring::Error::NoEntry)
        });
        assert!(matches!(result, Err(keyring::Error::NoEntry)));
        assert_eq!(allowed(), before);
    }

    #[test]
    fn foreground_authorization_cannot_enable_ui_during_a_background_read() {
        let _test = TEST_POLICY.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let before = allowed();
        let (started, waiting) = std::sync::mpsc::channel();
        let worker = with_interaction(false, || {
            let worker = std::thread::spawn(move || {
                started.send(()).unwrap();
                with_interaction(true, || { assert_ne!(allowed(), 0); Ok(()) }).unwrap();
            });
            waiting.recv().unwrap();
            // The foreground call waits at ACCESS, so background migration,
            // reads and writes continue with interaction disabled throughout.
            assert_eq!(allowed(), 0);
            Ok(worker)
        }).unwrap();
        worker.join().unwrap();
        assert_eq!(allowed(), before);
    }

    #[test]
    fn panic_restores_the_policy_and_the_next_call_can_recover() {
        let _test = TEST_POLICY.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let before = allowed();
        let panic = std::panic::catch_unwind(|| {
            let _: Result<(), _> = with_interaction(false, || panic!("synthetic operation failure"));
        });
        assert!(panic.is_err());
        assert_eq!(allowed(), before);
        with_interaction(false, || { assert_eq!(allowed(), 0); Ok(()) }).unwrap();
        assert_eq!(allowed(), before);
    }
}
