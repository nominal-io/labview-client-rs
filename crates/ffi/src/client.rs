//! `nominal_client_*` — connect/disconnect and client introspection.
//!
//! Built via `NominalClient::builder(token)` with explicit arguments, never
//! the profile-file path. Multiple simultaneous clients are supported.

use std::os::raw::c_char;

use nominal::core::NominalClient;

use crate::error::{fail, fail_sdk, guard, guard_i64, NominalErrorCode};
use crate::handles::{handle_registry, lookup_handle};
use crate::strings::{read_optional_str, read_required_str, write_opt_str_out, write_str_out};

handle_registry!(ClientHandle, NominalClient);

/// Creates a Nominal API client and writes its handle to `out_client`.
///
/// `token` is required. `workspace_rid` and `base_url` may be null or empty:
/// an absent workspace uses the token's default workspace, and an absent base
/// URL uses the production API. No network traffic happens here — a bad token
/// fails on the first request, not at construction.
///
/// Free with `nominal_client_free`.
#[no_mangle]
pub extern "C" fn nominal_client_new(
    token: *const c_char,
    workspace_rid: *const c_char,
    base_url: *const c_char,
    out_client: *mut i64,
) -> i32 {
    guard(|| {
        let token = match read_required_str(token, "token") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let workspace_rid = match read_optional_str(workspace_rid, "workspace_rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let base_url = match read_optional_str(base_url, "base_url") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if out_client.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_client must not be null",
            );
        }

        let mut builder = NominalClient::builder(token).workspace_rid(workspace_rid);
        if let Some(url) = base_url {
            builder = builder.base_url(url);
        }

        match builder.build() {
            Ok(client) => {
                // SAFETY: out_client checked non-null above; caller owns it.
                unsafe { *out_client = ClientHandle::insert(client) };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Frees a client handle. The handle is invalid afterwards; freeing twice
/// returns an error. Asset (and other resource) handles obtained through this
/// client stay valid — they hold their own data.
#[no_mangle]
pub extern "C" fn nominal_client_free(client: i64) -> i32 {
    guard(|| {
        if ClientHandle::remove(client) {
            0
        } else {
            fail(
                NominalErrorCode::InvalidHandle,
                format!("invalid client handle: {client}"),
            )
        }
    })
}

/// Writes the client's API base URL into `buf`. String-getter convention:
/// returns bytes needed, or a negative error code.
#[no_mangle]
pub extern "C" fn nominal_client_base_url(client: i64, buf: *mut c_char, cap: usize) -> i64 {
    guard_i64(|| {
        let client = lookup_handle!(
            ClientHandle,
            client,
            -(NominalErrorCode::InvalidHandle as i64)
        );
        write_str_out(client.base_url(), buf, cap)
    })
}

/// Writes the client's workspace RID into `buf` and whether one is configured
/// into `is_present`. String-getter convention: returns bytes needed, or a
/// negative error code.
#[no_mangle]
pub extern "C" fn nominal_client_workspace_rid(
    client: i64,
    buf: *mut c_char,
    cap: usize,
    is_present: *mut bool,
) -> i64 {
    guard_i64(|| {
        let client = lookup_handle!(
            ClientHandle,
            client,
            -(NominalErrorCode::InvalidHandle as i64)
        );
        write_opt_str_out(client.workspace_rid(), buf, cap, is_present)
    })
}
