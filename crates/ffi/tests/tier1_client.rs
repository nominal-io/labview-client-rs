//! Tier 1: client construction and handle lifecycle, no network involved
//! (`nominal_client_new` performs no I/O — it only validates its arguments).

mod common;

use common::{cstr, last_error, read_string};
use nominal_ffi::client::{
    nominal_client_base_url, nominal_client_free, nominal_client_new,
    nominal_client_new_from_profile, nominal_client_new_from_profile_env,
    nominal_client_workspace_rid,
};
use nominal_ffi::error::NominalErrorCode;

const WORKSPACE_RID: &str =
    "ri.security.cerulean-staging.workspace.00000000-0000-0000-0000-00000000aaaa";

fn new_client(workspace_rid: Option<&str>, base_url: Option<&str>) -> Result<i32, i32> {
    let token = cstr("test-token");
    let workspace = workspace_rid.map(cstr);
    let base = base_url.map(cstr);
    let mut handle = 0i32;
    let code = nominal_client_new(
        token.as_ptr(),
        workspace.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),
        base.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),
        &mut handle,
    );
    if code == 0 {
        Ok(handle)
    } else {
        Err(code)
    }
}

/// Size-queries the workspace RID getter, returning (code, needed, is_present).
fn query_workspace_rid(client: i32) -> (i32, u32, bool) {
    let mut needed = 0u32;
    let mut is_present = false;
    let code = nominal_client_workspace_rid(
        client,
        std::ptr::null_mut(),
        0,
        &mut needed,
        &mut is_present,
    );
    (code, needed, is_present)
}

#[test]
fn client_lifecycle_and_getters() {
    let _guard = common::message_lock();
    let client = new_client(Some(WORKSPACE_RID), Some("http://127.0.0.1:1/api")).unwrap();
    assert!(client > 0);

    let base_url =
        read_string(|buf, cap, needed| nominal_client_base_url(client, buf, cap, needed)).unwrap();
    assert_eq!(base_url, "http://127.0.0.1:1/api");

    let (code, needed, is_present) = query_workspace_rid(client);
    assert_eq!(code, 0);
    assert_eq!(needed, WORKSPACE_RID.len() as u32);
    assert!(is_present);

    assert_eq!(nominal_client_free(client), 0);
    assert_eq!(
        nominal_client_free(client),
        NominalErrorCode::InvalidHandle as i32,
        "double free must fail"
    );
    assert!(last_error().contains("invalid client handle"));
}

#[test]
fn absent_workspace_rid_reports_not_present() {
    let client = new_client(None, None).unwrap();
    let (code, needed, is_present) = query_workspace_rid(client);
    assert_eq!((code, needed, is_present), (0, 0, false));

    // Empty string means absent too — LabVIEW passes "" more easily than null.
    nominal_client_free(client);
    let client = new_client(Some(""), None).unwrap();
    let (code, needed, is_present) = query_workspace_rid(client);
    assert_eq!((code, needed, is_present), (0, 0, false));
    nominal_client_free(client);
}

#[test]
fn too_small_buffer_reports_needed_bytes() {
    let _guard = common::message_lock();
    let client = new_client(None, Some("http://127.0.0.1:1/api")).unwrap();

    let mut buf = [0xAAu8; 4];
    let mut needed = 0u32;
    let code = nominal_client_base_url(
        client,
        buf.as_mut_ptr() as *mut std::os::raw::c_char,
        buf.len() as u32,
        &mut needed,
    );
    assert_eq!(code, NominalErrorCode::BufferTooSmall as i32);
    assert_eq!(needed, "http://127.0.0.1:1/api".len() as u32);
    assert_eq!(buf, [0xAAu8; 4], "buffer must be untouched when too small");

    nominal_client_free(client);
}

#[test]
fn null_token_is_rejected() {
    let _guard = common::message_lock();
    let mut handle = 0i32;
    let code = nominal_client_new(
        std::ptr::null(),
        std::ptr::null(),
        std::ptr::null(),
        &mut handle,
    );
    assert_eq!(code, NominalErrorCode::NullArgument as i32);
    assert!(last_error().contains("token"), "got: {}", last_error());
}

#[test]
fn invalid_token_is_rejected() {
    let _guard = common::message_lock();
    // Bearer tokens must not contain spaces.
    let token = cstr("not a valid token");
    let mut handle = 0i32;
    let code = nominal_client_new(
        token.as_ptr(),
        std::ptr::null(),
        std::ptr::null(),
        &mut handle,
    );
    assert_eq!(code, NominalErrorCode::InvalidArgument as i32);
    assert!(!last_error().is_empty());
}

#[test]
fn invalid_base_url_is_rejected() {
    let _guard = common::message_lock();
    let err = new_client(None, Some("://not-a-url")).unwrap_err();
    assert_eq!(err, NominalErrorCode::InvalidArgument as i32);
}

#[test]
fn invalid_workspace_rid_is_rejected() {
    let _guard = common::message_lock();
    let err = new_client(Some("not-a-rid"), None).unwrap_err();
    assert_eq!(err, NominalErrorCode::InvalidArgument as i32);
    assert!(last_error().to_lowercase().contains("rid"));
}

#[test]
fn null_out_param_is_rejected() {
    let _guard = common::message_lock();
    let token = cstr("test-token");
    let code = nominal_client_new(
        token.as_ptr(),
        std::ptr::null(),
        std::ptr::null(),
        std::ptr::null_mut(),
    );
    assert_eq!(code, NominalErrorCode::NullArgument as i32);
}

// The profile-constructor happy paths need a real config file in the user's
// home directory (the upstream loader reads only the fixed default path, and
// planting one there from a test would clobber the developer's own config),
// so tier 1 covers the error paths and the happy path lives in the manual
// LabVIEW test plan.

#[test]
fn from_profile_with_unknown_profile_fails() {
    let _guard = common::message_lock();
    // Whether or not a config file exists, this profile name can't be in it.
    let name = cstr("ffi-test-profile-that-cannot-exist-4f9a2b");
    let mut handle = 0i32;
    let code = nominal_client_new_from_profile(name.as_ptr(), &mut handle);
    assert_ne!(code, 0);
    assert!(!last_error().is_empty());

    let empty = cstr("");
    assert_eq!(
        nominal_client_new_from_profile(empty.as_ptr(), &mut handle),
        NominalErrorCode::InvalidArgument as i32
    );
    assert_eq!(
        nominal_client_new_from_profile(name.as_ptr(), std::ptr::null_mut()),
        NominalErrorCode::NullArgument as i32
    );
}

#[test]
fn from_profile_env_without_env_var_fails() {
    let _guard = common::message_lock();
    // Only this test touches NOMINAL_PROFILE, and the message lock already
    // serializes it against the other error-path tests.
    std::env::remove_var("NOMINAL_PROFILE");
    let mut handle = 0i32;
    let code = nominal_client_new_from_profile_env(&mut handle);
    assert_ne!(code, 0);
    assert!(
        last_error().contains("NOMINAL_PROFILE"),
        "got: {}",
        last_error()
    );
}

#[test]
fn getters_reject_invalid_handles() {
    let _guard = common::message_lock();
    for handle in [0i32, -1, 999_999_999] {
        let err = read_string(|buf, cap, needed| nominal_client_base_url(handle, buf, cap, needed))
            .unwrap_err();
        assert_eq!(err, NominalErrorCode::InvalidHandle as i32);
    }
}
