//! Error codes, the last-error message, and the panic guard every exported
//! function is wrapped in.
//!
//! # Conventions used across this library
//!
//! **Return values.** Every FFI function returns one of two shapes:
//!
//! - *Action functions* (create, get, free, archive, ...) return `i32`:
//!   `0` = success, non-zero = a [`NominalErrorCode`] value. Results come back
//!   through out-parameters.
//! - *String and count getters* return `i64`, exactly like
//!   [`nominal_last_error`]: a value `>= 0` is the byte count needed (for
//!   strings, excluding the null terminator) or the element count; a negative
//!   value is a negated [`NominalErrorCode`] (e.g. `-4` = invalid handle).
//!
//! After any error, call [`nominal_last_error`] for a human-readable message
//! — same pattern as `GetLastError`/`errno` + `strerror`.
//!
//! **Timestamps.** Every timestamp crossing this FFI boundary is an `i64` in
//! Unix **milliseconds** (UTC). No other unit is ever used.
//!
//! **Handles.** Opaque objects are `i64` handles. Handle `0` is reserved,
//! never issued, and doubles as "null"/"absent".

use std::os::raw::c_char;
use std::sync::Mutex;

use once_cell::sync::Lazy;

/// Error codes returned by every function in this library (negated when the
/// function returns `i64`, see module docs).
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
}

/// The most recent error message, global across all threads — LabVIEW may
/// fetch it from a different thread than the one the failing call ran on.
static LAST_ERROR: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::new()));

pub(crate) fn set_last_error(message: impl Into<String>) {
    *LAST_ERROR
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = message.into();
}

/// Record `message` and return `code` as the `i32` an action function returns.
pub(crate) fn fail(code: NominalErrorCode, message: impl Into<String>) -> i32 {
    set_last_error(message);
    code as i32
}

/// Record `message` and return the negated code an `i64`-returning getter returns.
pub(crate) fn fail_i64(code: NominalErrorCode, message: impl Into<String>) -> i64 {
    set_last_error(message);
    -(code as i64)
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

/// Wrap an action-function body so a Rust panic becomes an error code instead
/// of undefined behavior (unwinding across `extern "C"` is UB — every exported
/// function must go through this or [`guard_i64`]).
pub(crate) fn guard<F: FnOnce() -> i32>(body: F) -> i32 {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)) {
        Ok(code) => code,
        // `&*payload`, not `&payload`: the Box must deref so the downcasts in
        // panic_message see the payload, not the Box itself.
        Err(payload) => fail(NominalErrorCode::Panic, panic_message(&*payload)),
    }
}

/// [`guard`] for `i64`-returning getters.
pub(crate) fn guard_i64<F: FnOnce() -> i64>(body: F) -> i64 {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)) {
        Ok(value) => value,
        Err(payload) => fail_i64(NominalErrorCode::Panic, panic_message(&*payload)),
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
/// bytes). Returns the number of bytes needed (excluding null terminator); if
/// the return value is >= `cap`, nothing was written — retry with a larger
/// buffer. Call after any function returns a non-zero / negative code.
#[no_mangle]
pub extern "C" fn nominal_last_error(buf: *mut c_char, cap: u64) -> i64 {
    guard_i64(|| {
        let message = LAST_ERROR
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        crate::strings::write_str_out(&message, buf, cap)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::test_message_lock as message_lock;

    #[test]
    fn fail_sets_message_and_returns_code() {
        let _guard = message_lock();
        let code = fail(NominalErrorCode::InvalidHandle, "no such handle");
        assert_eq!(code, NominalErrorCode::InvalidHandle as i32);

        let mut buf = [0u8; 64];
        let needed = nominal_last_error(buf.as_mut_ptr() as *mut c_char, buf.len() as u64);
        assert_eq!(needed, "no such handle".len() as i64);
        let written = &buf[..needed as usize];
        assert_eq!(written, b"no such handle");
        assert_eq!(buf[needed as usize], 0, "null terminator expected");
    }

    #[test]
    fn fail_i64_negates_code() {
        let _guard = message_lock();
        assert_eq!(
            fail_i64(NominalErrorCode::InvalidHandle, "x"),
            -(NominalErrorCode::InvalidHandle as i64)
        );
    }

    #[test]
    fn guard_converts_panic_to_error_code() {
        let _guard = message_lock();
        let code = guard(|| panic!("boom"));
        assert_eq!(code, NominalErrorCode::Panic as i32);

        let mut buf = [0u8; 128];
        let needed = nominal_last_error(buf.as_mut_ptr() as *mut c_char, buf.len() as u64);
        let message = std::str::from_utf8(&buf[..needed as usize]).unwrap();
        assert!(message.contains("boom"), "got: {message}");
    }

    #[test]
    fn guard_i64_converts_panic_to_negated_code() {
        let _guard = message_lock();
        let value = guard_i64(|| panic!("boom"));
        assert_eq!(value, -(NominalErrorCode::Panic as i64));
    }

    #[test]
    fn last_error_reports_needed_bytes_when_buffer_too_small() {
        let _guard = message_lock();
        set_last_error("a longer message than four bytes");
        let mut buf = [0xAAu8; 4];
        let needed = nominal_last_error(buf.as_mut_ptr() as *mut c_char, buf.len() as u64);
        assert_eq!(needed, "a longer message than four bytes".len() as i64);
        assert_eq!(buf, [0xAAu8; 4], "buffer must be untouched when too small");
    }
}
