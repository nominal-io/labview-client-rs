//! `nominal_template_*` — mirrors `nominal::core::template`.
//!
//! Templates are versioned workbook definitions. The upstream client is
//! read-only (get by RID); a template handle's main job here is feeding
//! `nominal_workbook_create_commit`. The template's layout/content JSON
//! blobs are not exposed — their schema is tied to the Nominal frontend.

use std::os::raw::c_char;

use nominal::core::Template;

use crate::client::ClientHandle;
use crate::error::{fail, fail_sdk, guard, NominalErrorCode};
use crate::handles::{handle_registry, lookup_handle};
use crate::runtime::block_on;
use crate::strings::{read_required_str, write_opt_str_field, write_str_field};

handle_registry!(TemplateHandle, Template);

/// Fetches the template with the given RID (latest commit on the main
/// branch), writing its handle to `out_template`. Free with
/// `nominal_template_free`.
#[no_mangle]
pub extern "C" fn nominal_template_get(
    client: i32,
    rid: *const c_char,
    out_template: *mut i32,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let rid = match read_required_str(rid, "rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if out_template.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_template must not be null",
            );
        }

        match block_on(client.templates().get(&rid)) {
            Ok(template) => {
                // SAFETY: out_template checked non-null above; caller owns it.
                unsafe { *out_template = TemplateHandle::insert(template) };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Frees a template handle. Freeing twice returns an error.
#[no_mangle]
pub extern "C" fn nominal_template_free(template: i32) -> i32 {
    guard(|| {
        if TemplateHandle::remove(template) {
            0
        } else {
            fail(
                NominalErrorCode::InvalidHandle,
                format!("invalid template handle: {template}"),
            )
        }
    })
}

/// Writes the template's RID into `buf`, storing the byte count needed in
/// `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_template_rid(
    template: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let template = lookup_handle!(TemplateHandle, template);
        write_str_field(template.rid(), buf, cap, out_needed)
    })
}

/// Writes the template's title into `buf`, storing the byte count needed in
/// `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_template_title(
    template: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let template = lookup_handle!(TemplateHandle, template);
        write_str_field(template.title(), buf, cap, out_needed)
    })
}

/// Writes the template's description into `buf`, and whether one is set into
/// `is_present` (an absent description reports 0 bytes needed).
#[no_mangle]
pub extern "C" fn nominal_template_description(
    template: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
    is_present: *mut bool,
) -> i32 {
    guard(|| {
        let template = lookup_handle!(TemplateHandle, template);
        write_opt_str_field(template.description(), buf, cap, out_needed, is_present)
    })
}

/// Writes the commit ID identifying this template version into `buf`,
/// storing the byte count needed in `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_template_commit_id(
    template: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let template = lookup_handle!(TemplateHandle, template);
        write_str_field(template.commit_id(), buf, cap, out_needed)
    })
}

/// Writes the URL for viewing this template in the Nominal web app into
/// `buf`, storing the byte count needed in `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_template_url(
    template: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let template = lookup_handle!(TemplateHandle, template);
        write_str_field(&template.nominal_url(), buf, cap, out_needed)
    })
}

/// Writes the template's creation time to `out_millis` as `f64` Unix
/// milliseconds (UTC).
#[no_mangle]
pub extern "C" fn nominal_template_created_at(template: i32, out_millis: *mut f64) -> i32 {
    guard(|| {
        let template = lookup_handle!(TemplateHandle, template);
        if out_millis.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_millis must not be null",
            );
        }
        // SAFETY: out_millis checked non-null above; caller owns it.
        unsafe { *out_millis = template.created_at().timestamp_millis() as f64 };
        0
    })
}
