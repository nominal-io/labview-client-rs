//! Shared helpers for strings crossing the FFI boundary.
//!
//! Outgoing strings use a caller-supplied buffer, Win32-style — no allocation
//! ever crosses the boundary, so there is no `_free` to mismatch. Every
//! function returns an `i32` status code; the byte count needed comes back
//! through a `*mut u64` out-parameter (never `usize`/`size_t` — its width
//! differs between the 32- and 64-bit DLLs, which LabVIEW's Import Shared
//! Library wizard can't express without manual preprocessor definitions).

use std::os::raw::c_char;

use crate::error::{fail, NominalErrorCode};

/// Writes `value` (plus a null terminator) into `buf` of capacity `cap`
/// bytes, and stores the byte count needed (excluding the terminator) in
/// `out_needed`. Returns:
///
/// - `Ok` — written, or `buf` was null (the supported size-query call).
/// - `BufferTooSmall` — `buf` is non-null but `cap` < needed + 1; nothing was
///   written. Retry with a buffer of at least (`*out_needed` + 1) bytes.
///
/// Does NOT touch the last-error message — `nominal_last_error` itself uses
/// this helper, and overwriting the message it is trying to report would lose
/// the original error. Getters wanting a message use [`write_str_field`].
pub(crate) fn write_str_out(value: &str, buf: *mut c_char, cap: u64, out_needed: *mut u64) -> i32 {
    let bytes = value.as_bytes();
    if !out_needed.is_null() {
        // SAFETY: out_needed is non-null and caller-owned.
        unsafe { *out_needed = bytes.len() as u64 };
    }
    if buf.is_null() {
        return NominalErrorCode::Ok as i32;
    }
    if bytes.len() as u64 >= cap {
        return NominalErrorCode::BufferTooSmall as i32;
    }
    // SAFETY: buf is non-null and the caller promises cap writable bytes;
    // bytes.len() + 1 <= cap was checked above.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf as *mut u8, bytes.len());
        *buf.add(bytes.len()) = 0;
    }
    NominalErrorCode::Ok as i32
}

/// [`write_str_out`] plus a last-error message on failure — what every string
/// getter uses.
pub(crate) fn write_str_field(
    value: &str,
    buf: *mut c_char,
    cap: u64,
    out_needed: *mut u64,
) -> i32 {
    let code = write_str_out(value, buf, cap, out_needed);
    if code == NominalErrorCode::BufferTooSmall as i32 {
        return fail(
            NominalErrorCode::BufferTooSmall,
            format!(
                "buffer too small: need {} bytes plus a null terminator, capacity is {cap}",
                value.len()
            ),
        );
    }
    code
}

/// [`write_str_field`] for optional strings: also sets `*is_present`. Absent
/// values report 0 bytes needed and write an empty string.
pub(crate) fn write_opt_str_field(
    value: Option<&str>,
    buf: *mut c_char,
    cap: u64,
    out_needed: *mut u64,
    is_present: *mut bool,
) -> i32 {
    if !is_present.is_null() {
        // SAFETY: is_present is non-null and caller-owned.
        unsafe { *is_present = value.is_some() };
    }
    write_str_field(value.unwrap_or(""), buf, cap, out_needed)
}

/// Reads a required incoming string argument. On failure records the error
/// (naming `arg`) and returns the `i32` code to bubble out.
pub(crate) fn read_required_str(ptr: *const c_char, arg: &str) -> Result<String, i32> {
    if ptr.is_null() {
        return Err(fail(
            NominalErrorCode::NullArgument,
            format!("argument '{arg}' must not be null"),
        ));
    }
    // SAFETY: ptr is non-null and the caller promises a null-terminated string.
    let c_str = unsafe { std::ffi::CStr::from_ptr(ptr) };
    match c_str.to_str() {
        Ok(s) => Ok(s.to_string()),
        Err(_) => Err(fail(
            NominalErrorCode::InvalidUtf8,
            format!("argument '{arg}' is not valid UTF-8"),
        )),
    }
}

/// Reads an optional incoming string argument: null or empty both mean
/// "absent" (LabVIEW passes empty strings far more naturally than nulls).
pub(crate) fn read_optional_str(ptr: *const c_char, arg: &str) -> Result<Option<String>, i32> {
    if ptr.is_null() {
        return Ok(None);
    }
    let value = read_required_str(ptr, arg)?;
    Ok(if value.is_empty() { None } else { Some(value) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_to(value: &str, cap: usize) -> (i32, u64, Vec<u8>) {
        let mut buf = vec![0xAAu8; cap];
        let mut needed = 0u64;
        let code = write_str_out(
            value,
            buf.as_mut_ptr() as *mut c_char,
            cap as u64,
            &mut needed,
        );
        (code, needed, buf)
    }

    #[test]
    fn writes_string_and_null_terminator() {
        let (code, needed, buf) = write_to("hello", 16);
        assert_eq!((code, needed), (0, 5));
        assert_eq!(&buf[..6], b"hello\0");
    }

    #[test]
    fn exact_fit_needs_room_for_terminator() {
        // cap == len: no room for the terminator, so nothing is written.
        let (code, needed, buf) = write_to("hello", 5);
        assert_eq!(code, NominalErrorCode::BufferTooSmall as i32);
        assert_eq!(needed, 5, "needed is reported even on failure");
        assert_eq!(buf, vec![0xAAu8; 5], "buffer must be untouched");

        // cap == len + 1 is the smallest buffer that fits.
        let (code, needed, buf) = write_to("hello", 6);
        assert_eq!((code, needed), (0, 5));
        assert_eq!(&buf[..6], b"hello\0");
    }

    #[test]
    fn empty_string_writes_terminator_only() {
        let (code, needed, buf) = write_to("", 4);
        assert_eq!((code, needed), (0, 0));
        assert_eq!(buf[0], 0);
    }

    #[test]
    fn multibyte_utf8_counts_bytes_not_chars() {
        let value = "héllo"; // 6 bytes, 5 chars
        let (code, needed, buf) = write_to(value, 16);
        assert_eq!((code, needed), (0, 6));
        assert_eq!(&buf[..6], value.as_bytes());
        assert_eq!(buf[6], 0);
    }

    #[test]
    fn null_buffer_is_a_size_query_not_an_error() {
        let mut needed = 0u64;
        let code = write_str_out("hello", std::ptr::null_mut(), 0, &mut needed);
        assert_eq!((code, needed), (0, 5));
    }

    #[test]
    fn buffer_too_small_sets_message_via_field_variant() {
        let _guard = crate::error::test_message_lock();
        let mut buf = [0u8; 2];
        let mut needed = 0u64;
        let code = write_str_field(
            "hello",
            buf.as_mut_ptr() as *mut c_char,
            buf.len() as u64,
            &mut needed,
        );
        assert_eq!(code, NominalErrorCode::BufferTooSmall as i32);
        assert_eq!(needed, 5);
    }

    #[test]
    fn optional_present_and_absent() {
        let mut is_present = false;
        let mut needed = 0u64;
        let mut buf = [0u8; 8];
        let code = write_opt_str_field(
            Some("hi"),
            buf.as_mut_ptr() as *mut c_char,
            buf.len() as u64,
            &mut needed,
            &mut is_present,
        );
        assert_eq!((code, needed, is_present), (0, 2, true));
        assert_eq!(&buf[..3], b"hi\0");

        let code = write_opt_str_field(
            None,
            buf.as_mut_ptr() as *mut c_char,
            buf.len() as u64,
            &mut needed,
            &mut is_present,
        );
        assert_eq!((code, needed, is_present), (0, 0, false));
    }

    #[test]
    fn read_required_rejects_null() {
        let _guard = crate::error::test_message_lock();
        let err = read_required_str(std::ptr::null(), "token").unwrap_err();
        assert_eq!(err, NominalErrorCode::NullArgument as i32);
    }

    #[test]
    fn read_required_rejects_invalid_utf8() {
        let _guard = crate::error::test_message_lock();
        let bad = [0xFFu8, 0xFE, 0x00];
        let err = read_required_str(bad.as_ptr() as *const c_char, "token").unwrap_err();
        assert_eq!(err, NominalErrorCode::InvalidUtf8 as i32);
    }

    #[test]
    fn read_optional_treats_null_and_empty_as_absent() {
        assert_eq!(read_optional_str(std::ptr::null(), "x").unwrap(), None);
        let empty = b"\0";
        assert_eq!(
            read_optional_str(empty.as_ptr() as *const c_char, "x").unwrap(),
            None
        );
        let value = b"abc\0";
        assert_eq!(
            read_optional_str(value.as_ptr() as *const c_char, "x").unwrap(),
            Some("abc".to_string())
        );
    }
}
