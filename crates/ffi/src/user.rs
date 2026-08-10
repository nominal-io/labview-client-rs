//! `nominal_user_*` — mirrors `nominal::core::user`.
//!
//! One operation upstream: who-am-I for the authenticated token. Handy from
//! LabVIEW as a cheap "is my token valid" probe and for stamping operator
//! identity into test metadata.

use std::os::raw::c_char;

use nominal::core::User;

use crate::client::ClientHandle;
use crate::error::{fail, fail_sdk, guard, NominalErrorCode};
use crate::handles::{handle_registry, lookup_handle};
use crate::runtime::block_on;
use crate::strings::write_str_field;

handle_registry!(UserHandle, User);

/// Fetches the authenticated user (the owner of the client's token),
/// writing its handle to `out_user`. Free with `nominal_user_free`.
#[no_mangle]
pub extern "C" fn nominal_user_me(client: i32, out_user: *mut i32) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        if out_user.is_null() {
            return fail(NominalErrorCode::NullArgument, "out_user must not be null");
        }

        match block_on(client.users().who_am_i()) {
            Ok(user) => {
                // SAFETY: out_user checked non-null above; caller owns it.
                unsafe { *out_user = UserHandle::insert(user) };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Frees a user handle. Freeing twice returns an error.
#[no_mangle]
pub extern "C" fn nominal_user_free(user: i32) -> i32 {
    guard(|| {
        if UserHandle::remove(user) {
            0
        } else {
            fail(
                NominalErrorCode::InvalidHandle,
                format!("invalid user handle: {user}"),
            )
        }
    })
}

/// Writes the user's RID into `buf`, storing the byte count needed in
/// `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_user_rid(
    user: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let user = lookup_handle!(UserHandle, user);
        write_str_field(user.rid(), buf, cap, out_needed)
    })
}

/// Writes the RID of the user's organization into `buf`, storing the byte
/// count needed in `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_user_org_rid(
    user: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let user = lookup_handle!(UserHandle, user);
        write_str_field(user.org_rid(), buf, cap, out_needed)
    })
}

/// Writes the user's email into `buf`, storing the byte count needed in
/// `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_user_email(
    user: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let user = lookup_handle!(UserHandle, user);
        write_str_field(user.email(), buf, cap, out_needed)
    })
}

/// Writes the user's display name into `buf`, storing the byte count needed
/// in `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_user_display_name(
    user: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let user = lookup_handle!(UserHandle, user);
        write_str_field(user.display_name(), buf, cap, out_needed)
    })
}
