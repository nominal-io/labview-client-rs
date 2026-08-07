//! Error codes, the last-error message, and the panic guard every exported
//! function is wrapped in.
//!
//! # Conventions used across this library
//!
//! **Return values.** Every FFI function returns `i32`: `0` = success,
//! non-zero = a [`NominalErrorCode`] value. All results — handles, strings,
//! counts, timestamps — come back through out-parameters. This uniformity is
//! deliberate: LabVIEW's Import Shared Library wizard can then apply its
//! "Function Returns Error Code/Status" mode to every function and auto-wire
//! the return value into the error cluster.
//!
//! **Strings out.** The caller supplies a byte buffer + `u64` capacity; the
//! function stores the byte count needed (excluding the null terminator) in a
//! `*mut u64` out-parameter. A null buffer is the supported size query (
//! returns success). A non-null buffer that is too small returns
//! `BufferTooSmall` and writes nothing — retry with (needed + 1) bytes.
//!
//! After any error, call [`nominal_last_error`] for a human-readable message
//! — same pattern as `GetLastError`/`errno` + `strerror`.
//!
//! **Sizes and counts** are always `u64`, never `usize`/`size_t` — `size_t`'s
//! width differs between the 32- and 64-bit DLLs, which the wizard can't
//! resolve without manual preprocessor definitions.
//!
//! **Timestamps.** Every timestamp crossing this FFI boundary is an `i64` in
//! Unix **milliseconds** (UTC). No other unit is ever used.
//!
//! **Handles.** Opaque objects are `i64` handles. Handle `0` is reserved,
//! never issued, and doubles as "null"/"absent".

use std::os::raw::c_char;
use std::sync::Mutex;

use once_cell::sync::Lazy;

/// Error codes returned by every function in this library.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NominalErrorCode {
    /// Success.
    Ok = 0,
    /// A Rust panic was caught at the FFI boundary. State may be suspect;
    /// report this as a bug in the FFI layer.
    Panic = 1,
    /// A required pointer argument was null.
    NullArgument = 2,
    /// A string argument was not valid UTF-8.
    InvalidUtf8 = 3,
    /// A handle was never issued or has already been freed.
    InvalidHandle = 4,
    /// An argument was rejected before any request was sent (bad RID, bad
    /// token, bad URL, conflicting filters, ...).
    InvalidArgument = 5,
    /// An index argument was out of range for the collection.
    IndexOutOfRange = 6,
    /// The requested resource does not exist.
    NotFound = 7,
    /// The Nominal API returned an error or an unreadable response.
    ApiError = 8,
    /// Any other error from the underlying `nominal` SDK.
    SdkError = 9,
    /// A caller-supplied output buffer was too small; nothing was written.
    /// The needed-bytes out-parameter says how much to allocate (+ 1 for the
    /// null terminator).
    BufferTooSmall = 10,
}

/// The most recent error message, global across all threads — LabVIEW may
/// fetch it from a different thread than the one the failing call ran on.
static LAST_ERROR: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::new()));

pub(crate) fn set_last_error(message: impl Into<String>) {
    *LAST_ERROR
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = message.into();
}

/// Record `message` and return `code` as the `i32` to bubble out.
pub(crate) fn fail(code: NominalErrorCode, message: impl Into<String>) -> i32 {
    set_last_error(message);
    code as i32
}

/// Record an error from the `nominal` SDK and return the matching `i32` code.
pub(crate) fn fail_sdk(err: nominal::Error) -> i32 {
    let code = classify(&err);
    set_last_error(err.to_string());
    code as i32
}

/// Map `nominal::Error` variants onto the flat code enum. `nominal::Error` is
/// `#[non_exhaustive]`, so unknown future variants fall through to `SdkError`.
fn classify(err: &nominal::Error) -> NominalErrorCode {
    use nominal::Error;
    match err {
        Error::NotFound { .. } => NominalErrorCode::NotFound,
        Error::Conjure { .. } => NominalErrorCode::ApiError,
        Error::Rid { .. } | Error::InvalidBearerToken { .. } | Error::InvalidServiceUrl { .. } => {
            NominalErrorCode::InvalidArgument
        }
        _ => NominalErrorCode::SdkError,
    }
}

/// Wrap a function body so a Rust panic becomes an error code instead of
/// undefined behavior (unwinding across `extern "C"` is UB — every exported
/// function must go through this).
pub(crate) fn guard<F: FnOnce() -> i32>(body: F) -> i32 {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)) {
        Ok(code) => code,
        // `&*payload`, not `&payload`: the Box must deref so the downcasts in
        // panic_message see the payload, not the Box itself.
        Err(payload) => fail(NominalErrorCode::Panic, panic_message(&*payload)),
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        format!("panic in FFI layer: {s}")
    } else if let Some(s) = payload.downcast_ref::<String>() {
        format!("panic in FFI layer: {s}")
    } else {
        "panic in FFI layer (non-string payload)".to_string()
    }
}

/// LAST_ERROR is deliberately process-global, but the test harness runs unit
/// tests on parallel threads — any test that *sets* an error (even without
/// reading it) must hold this lock, or it races the tests that assert on the
/// message text.
#[cfg(test)]
pub(crate) fn test_message_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Writes the most recent error message into `buf` (capacity `cap`, in
/// bytes) and stores the byte count needed (excluding the null terminator)
/// in `out_needed`. Call after any function returns a non-zero code.
///
/// A null `buf` is the supported size query. If `buf` is non-null but too
/// small, returns `BufferTooSmall` WITHOUT overwriting the stored message —
/// retry with a buffer of at least (`*out_needed` + 1) bytes.
#[no_mangle]
pub extern "C" fn nominal_last_error(buf: *mut c_char, cap: u64, out_needed: *mut u64) -> i32 {
    guard(|| {
        let message = LAST_ERROR
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        // write_str_out, not write_str_field: must not clobber the message
        // this function exists to report.
        crate::strings::write_str_out(&message, buf, cap, out_needed)
    })
}

#[cfg(test)]
mod tests {
    use super::test_message_lock as message_lock;
    use super::*;

    fn fetch_last_error() -> String {
        let mut needed = 0u64;
        let code = nominal_last_error(std::ptr::null_mut(), 0, &mut needed);
        assert_eq!(code, 0, "size query must succeed");
        let mut buf = vec![0u8; needed as usize + 1];
        let code = nominal_last_error(
            buf.as_mut_ptr() as *mut c_char,
            buf.len() as u64,
            &mut needed,
        );
        assert_eq!(code, 0);
        String::from_utf8(buf[..needed as usize].to_vec()).unwrap()
    }

    #[test]
    fn fail_sets_message_and_returns_code() {
        let _guard = message_lock();
        let code = fail(NominalErrorCode::InvalidHandle, "no such handle");
        assert_eq!(code, NominalErrorCode::InvalidHandle as i32);
        assert_eq!(fetch_last_error(), "no such handle");
    }

    #[test]
    fn guard_converts_panic_to_error_code() {
        let _guard = message_lock();
        let code = guard(|| panic!("boom"));
        assert_eq!(code, NominalErrorCode::Panic as i32);
        assert!(
            fetch_last_error().contains("boom"),
            "got: {}",
            fetch_last_error()
        );
    }

    #[test]
    fn too_small_buffer_keeps_original_message() {
        let _guard = message_lock();
        set_last_error("the original error message");
        let mut buf = [0xAAu8; 4];
        let mut needed = 0u64;
        let code = nominal_last_error(
            buf.as_mut_ptr() as *mut c_char,
            buf.len() as u64,
            &mut needed,
        );
        assert_eq!(code, NominalErrorCode::BufferTooSmall as i32);
        assert_eq!(needed, "the original error message".len() as u64);
        assert_eq!(buf, [0xAAu8; 4], "buffer must be untouched when too small");
        // The stored message must survive the failed fetch.
        assert_eq!(fetch_last_error(), "the original error message");
    }
}
