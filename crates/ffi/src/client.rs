//! `nominal_client_*` — connect/disconnect and client introspection.
//!
//! Built via `NominalClient::builder(token)` with explicit arguments as the
//! primary path; `nominal_client_new_from_profile`/`_from_profile_env` are
//! sanctioned secondary constructors for machines with a `nominal` CLI
//! config file. Multiple simultaneous clients are supported.

use std::os::raw::c_char;

use nominal::core::NominalClient;

use crate::error::{fail, fail_sdk, guard, NominalErrorCode};
use crate::handles::{handle_registry, lookup_handle};
use crate::strings::{read_optional_str, read_required_str, write_opt_str_field, write_str_field};

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
    out_client: *mut i32,
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

/// Creates a client from the named profile in the Nominal config file
/// (`~/.config/nominal/config.yml`, as written by the `nominal` CLI's auth
/// flow), writing its handle to `out_client`. Secondary constructor —
/// `nominal_client_new` with an explicit token is the primary path. Fails
/// if the config file is missing or the profile name isn't in it.
///
/// Free with `nominal_client_free`.
#[no_mangle]
pub extern "C" fn nominal_client_new_from_profile(
    name: *const c_char,
    out_client: *mut i32,
) -> i32 {
    guard(|| {
        let name = match read_required_str(name, "name") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if name.is_empty() {
            return fail(NominalErrorCode::InvalidArgument, "name must not be empty");
        }
        if out_client.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_client must not be null",
            );
        }

        match NominalClient::from_profile(&name) {
            Ok(client) => {
                // SAFETY: out_client checked non-null above; caller owns it.
                unsafe { *out_client = ClientHandle::insert(client) };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Creates a client from the profile named by the `NOMINAL_PROFILE`
/// environment variable — `nominal_client_new_from_profile` with the name
/// taken from the environment. Fails if the variable is unset.
///
/// Free with `nominal_client_free`.
#[no_mangle]
pub extern "C" fn nominal_client_new_from_profile_env(out_client: *mut i32) -> i32 {
    guard(|| {
        if out_client.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_client must not be null",
            );
        }

        match NominalClient::from_profile_env() {
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
pub extern "C" fn nominal_client_free(client: i32) -> i32 {
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

/// Writes the client's API base URL into `buf` (capacity `cap` bytes),
/// storing the byte count needed in `out_needed` — see the string convention
/// in the header preamble.
#[no_mangle]
pub extern "C" fn nominal_client_base_url(
    client: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        write_str_field(client.base_url(), buf, cap, out_needed)
    })
}

/// Writes the client's workspace RID into `buf` and whether one is configured
/// into `is_present` (an absent workspace reports 0 bytes needed).
#[no_mangle]
pub extern "C" fn nominal_client_workspace_rid(
    client: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
    is_present: *mut bool,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        write_opt_str_field(client.workspace_rid(), buf, cap, out_needed, is_present)
    })
}
