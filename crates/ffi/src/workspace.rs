//! `nominal_workspace_*` — mirrors `nominal::core::workspace`.
//!
//! Lists the workspaces the token can access — the practical way to discover
//! the workspace RID to pass to `nominal_client_new` from LabVIEW, instead
//! of copying it out of the web app.

use std::os::raw::c_char;

use nominal::core::Workspace;

use crate::client::ClientHandle;
use crate::error::{fail, fail_sdk, guard, NominalErrorCode};
use crate::handles::{handle_registry, insert_handle_list, lookup_handle};
use crate::runtime::block_on;
use crate::strings::{write_opt_str_field, write_str_field};

handle_registry!(WorkspaceHandle, Workspace);

/// Lists the workspaces the authenticated user can access (sorted by display
/// name), returning a handle list.
///
/// On success `*out_list` is a handle list of `*out_count` workspace handles
/// — read them with `nominal_handle_list_get` and free the list with
/// `nominal_handle_list_free`. Each workspace handle stays valid until
/// passed to `nominal_workspace_free`, independent of the list.
#[no_mangle]
pub extern "C" fn nominal_workspace_list(
    client: i32,
    out_list: *mut i32,
    out_count: *mut u32,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        if out_list.is_null() || out_count.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_list and out_count must not be null",
            );
        }

        match block_on(client.workspaces().list_workspaces()) {
            Ok(workspaces) => {
                let handles: Vec<i32> = workspaces
                    .into_iter()
                    .map(WorkspaceHandle::insert)
                    .collect();
                let count = handles.len() as u32;
                // SAFETY: out pointers checked non-null above; caller owns them.
                unsafe {
                    *out_list = insert_handle_list(handles);
                    *out_count = count;
                }
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Frees a workspace handle. Freeing twice returns an error.
#[no_mangle]
pub extern "C" fn nominal_workspace_free(workspace: i32) -> i32 {
    guard(|| {
        if WorkspaceHandle::remove(workspace) {
            0
        } else {
            fail(
                NominalErrorCode::InvalidHandle,
                format!("invalid workspace handle: {workspace}"),
            )
        }
    })
}

/// Writes the workspace's RID into `buf`, storing the byte count needed in
/// `out_needed`. This is the value `nominal_client_new` takes as
/// `workspace_rid`.
#[no_mangle]
pub extern "C" fn nominal_workspace_rid(
    workspace: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let workspace = lookup_handle!(WorkspaceHandle, workspace);
        write_str_field(workspace.rid(), buf, cap, out_needed)
    })
}

/// Writes the workspace's display name into `buf`, and whether one is set
/// into `is_present` (an absent name reports 0 bytes needed).
#[no_mangle]
pub extern "C" fn nominal_workspace_display_name(
    workspace: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
    is_present: *mut bool,
) -> i32 {
    guard(|| {
        let workspace = lookup_handle!(WorkspaceHandle, workspace);
        write_opt_str_field(workspace.display_name(), buf, cap, out_needed, is_present)
    })
}
