//! Shared helpers for strings crossing the FFI boundary.
//!
//! Outgoing strings use a caller-supplied buffer, Win32-style — no allocation
//! ever crosses the boundary, so there is no `_free` to mismatch.

use std::os::raw::c_char;

use crate::error::{fail, NominalErrorCode};

/// Writes `value` into `buf` (capacity `cap`, in bytes). Returns the number of
/// bytes needed (excluding null terminator). If the return value is >= `cap`,
/// the caller's buffer was too small — nothing was written — and the caller
/// should retry with a buffer of at least (return value + 1) bytes.
///
/// Calling with `buf` null / `cap` 0 is the supported way to query the size.
pub(crate) fn write_str_out(value: &str, buf: *mut c_char, cap: usize) -> i64 {
    let bytes = value.as_bytes();
    let needed = bytes.len() as i64;
    if buf.is_null() || bytes.len() >= cap {
        return needed;
    }
    // SAFETY: buf is non-null and the caller promises cap writable bytes;
    // bytes.len() + 1 <= cap was checked above.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf as *mut u8, bytes.len());
        *buf.add(bytes.len()) = 0;
    }
    needed
}

/// [`write_str_out`] for optional strings: sets `*is_present` and writes the
/// value (or nothing) into `buf`. Absent values report 0 bytes needed.
pub(crate) fn write_opt_str_out(
    value: Option<&str>,
    buf: *mut c_char,
    cap: usize,
    is_present: *mut bool,
) -> i64 {
    if !is_present.is_null() {
        // SAFETY: is_present is non-null and caller-owned.
        unsafe { *is_present = value.is_some() };
    }
    write_str_out(value.unwrap_or(""), buf, cap)
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

    fn write_to(value: &str, cap: usize) -> (i64, Vec<u8>) {
        let mut buf = vec![0xAAu8; cap];
        let needed = write_str_out(value, buf.as_mut_ptr() as *mut c_char, cap);
        (needed, buf)
    }

    #[test]
    fn writes_string_and_null_terminator() {
        let (needed, buf) = write_to("hello", 16);
        assert_eq!(needed, 5);
        assert_eq!(&buf[..5], b"hello");
        assert_eq!(buf[5], 0);
    }

    #[test]
    fn exact_fit_needs_room_for_terminator() {
        // cap == len: no room for the terminator, so nothing is written.
        let (needed, buf) = write_to("hello", 5);
        assert_eq!(needed, 5);
        assert_eq!(buf, vec![0xAAu8; 5]);

        // cap == len + 1 is the smallest buffer that fits.
        let (needed, buf) = write_to("hello", 6);
        assert_eq!(needed, 5);
        assert_eq!(&buf[..6], b"hello\0");
    }

    #[test]
    fn empty_string_writes_terminator_only() {
        let (needed, buf) = write_to("", 4);
        assert_eq!(needed, 0);
        assert_eq!(buf[0], 0);
    }

    #[test]
    fn multibyte_utf8_counts_bytes_not_chars() {
        let value = "héllo"; // 6 bytes, 5 chars
        let (needed, buf) = write_to(value, 16);
        assert_eq!(needed, 6);
        assert_eq!(&buf[..6], value.as_bytes());
        assert_eq!(buf[6], 0);
    }

    #[test]
    fn null_buffer_is_a_size_query() {
        assert_eq!(write_str_out("hello", std::ptr::null_mut(), 0), 5);
    }

    #[test]
    fn optional_present_and_absent() {
        let mut is_present = false;
        let mut buf = [0u8; 8];
        let needed = write_opt_str_out(
            Some("hi"),
            buf.as_mut_ptr() as *mut c_char,
            buf.len(),
            &mut is_present,
        );
        assert_eq!((needed, is_present), (2, true));
        assert_eq!(&buf[..3], b"hi\0");

        let needed = write_opt_str_out(
            None,
            buf.as_mut_ptr() as *mut c_char,
            buf.len(),
            &mut is_present,
        );
        assert_eq!((needed, is_present), (0, false));
    }

    #[test]
    fn read_required_rejects_null() {
        let err = read_required_str(std::ptr::null(), "token").unwrap_err();
        assert_eq!(err, NominalErrorCode::NullArgument as i32);
    }

    #[test]
    fn read_required_rejects_invalid_utf8() {
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
