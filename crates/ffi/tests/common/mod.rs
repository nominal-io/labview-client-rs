//! Shared helpers for the integration test suites, which call the exported
//! FFI functions exactly the way LabVIEW would (pointers and buffers in,
//! codes out) — just from Rust.

use std::ffi::CString;
use std::os::raw::c_char;

use once_cell::sync::Lazy;

/// Runtime for driving wiremock setup. The FFI functions themselves must NOT
/// run inside it — they call `block_on` on the library's own global runtime,
/// which panics if the calling thread is already inside a runtime. Calling
/// them from the plain test thread is the point: that's what LabVIEW does.
#[allow(dead_code)] // used by the tier-2 suite only; `common` compiles into both.
pub static TEST_RT: Lazy<tokio::runtime::Runtime> =
    Lazy::new(|| tokio::runtime::Runtime::new().expect("failed to start test runtime"));

pub fn cstr(s: &str) -> CString {
    CString::new(s).expect("test string contained a NUL byte")
}

/// Drives a string getter through the documented two-call pattern: size query
/// with a null buffer, then the real call. Returns the negative error code on
/// failure.
pub fn read_string(mut getter: impl FnMut(*mut c_char, u64) -> i64) -> Result<String, i64> {
    let needed = getter(std::ptr::null_mut(), 0);
    if needed < 0 {
        return Err(needed);
    }
    let cap = needed as usize + 1;
    let mut buf = vec![0u8; cap];
    let second = getter(buf.as_mut_ptr() as *mut c_char, cap as u64);
    assert_eq!(second, needed, "size query and write disagreed");
    assert_eq!(buf[needed as usize], 0, "missing null terminator");
    Ok(String::from_utf8(buf[..needed as usize].to_vec()).expect("invalid UTF-8 from getter"))
}

/// The last-error message is deliberately process-global, but the test
/// harness runs tests on parallel threads — any test that asserts on the
/// message text must hold this lock across the failing call and the read.
#[allow(dead_code)]
pub fn message_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn last_error() -> String {
    read_string(|buf, cap| nominal_ffi::error::nominal_last_error(buf, cap))
        .expect("nominal_last_error itself failed")
}
