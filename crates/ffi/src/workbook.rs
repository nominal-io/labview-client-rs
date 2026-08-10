//! `nominal_workbook_*` — mirrors `nominal::core::workbook`.
//!
//! Workbooks are saved analysis views attached to assets or runs. Creation
//! always starts from a template (`nominal_template_get`), with the data
//! scope — which assets or runs the workbook visualizes — staged one RID at
//! a time (assets and runs are mutually exclusive, matching upstream).

use std::collections::HashMap;
use std::os::raw::c_char;
use std::sync::Mutex;

use nominal::core::{Workbook, WorkbookCreate, WorkbookDataScope, WorkbookQuery};

use crate::client::ClientHandle;
use crate::error::{fail, fail_sdk, guard, NominalErrorCode};
use crate::handles::{handle_registry, insert_handle_list, lookup_handle};
use crate::runtime::block_on;
use crate::strings::{read_optional_str, read_required_str, write_opt_str_field, write_str_field};
use crate::template::TemplateHandle;

handle_registry!(WorkbookHandle, Workbook);

/// Whether a workbook's data scope is a set of assets or a set of runs, as
/// reported by `nominal_workbook_scope_type`.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NominalWorkbookScopeType {
    Assets = 0,
    Runs = 1,
}

fn index_error(index: i32, len: usize) -> i32 {
    fail(
        NominalErrorCode::IndexOutOfRange,
        format!("index {index} out of range for collection of {len}"),
    )
}

/// Properties in sorted-key order, so `index` means the same entry across the
/// `_count` / `_key_at` / `_value_at` calls.
fn property_at(workbook: &Workbook, index: i32) -> Result<(&String, &String), i32> {
    let mut keys: Vec<&String> = workbook.properties().keys().collect();
    keys.sort();
    let key = keys.get(usize::try_from(index).map_err(|_| index_error(index, keys.len()))?);
    match key {
        Some(&key) => Ok((key, &workbook.properties()[key])),
        None => Err(index_error(index, keys.len())),
    }
}

fn scope_rids(workbook: &Workbook) -> &[String] {
    match workbook.data_scope() {
        WorkbookDataScope::Assets(rids) => rids,
        WorkbookDataScope::Runs(rids) => rids,
    }
}

/// Writes a collection size to a `*mut u32` out-parameter.
fn write_count(count: usize, out_count: *mut u32) -> i32 {
    if out_count.is_null() {
        return fail(NominalErrorCode::NullArgument, "out_count must not be null");
    }
    // SAFETY: out_count checked non-null above; caller owns it.
    unsafe { *out_count = count as u32 };
    0
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

/// Fetches the workbook with the given RID, writing its handle to
/// `out_workbook`. Free with `nominal_workbook_free`.
#[no_mangle]
pub extern "C" fn nominal_workbook_get(
    client: i32,
    rid: *const c_char,
    out_workbook: *mut i32,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let rid = match read_required_str(rid, "rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if out_workbook.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_workbook must not be null",
            );
        }

        match block_on(client.workbooks().get(&rid)) {
            Ok(workbook) => {
                // SAFETY: out_workbook checked non-null above; caller owns it.
                unsafe { *out_workbook = WorkbookHandle::insert(workbook) };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Lists all workbooks (search with no filters), returning a handle list.
///
/// On success `*out_list` is a handle list of `*out_count` workbook handles —
/// read them with `nominal_handle_list_get` and free the list with
/// `nominal_handle_list_free`. Each workbook handle stays valid until passed
/// to `nominal_workbook_free`, independent of the list.
#[no_mangle]
pub extern "C" fn nominal_workbook_list(
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

        match block_on(client.workbooks().search(WorkbookQuery::search_text(""))) {
            Ok(workbooks) => {
                let handles: Vec<i32> = workbooks.into_iter().map(WorkbookHandle::insert).collect();
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

/// Searches workbooks, returning a handle list (see `nominal_workbook_list`
/// for ownership).
///
/// Filters may each be null or empty ("no filter"); the ones provided are
/// combined with AND:
/// - `search_text`: fuzzy full-text match on title and description
/// - `label`: exact label match
/// - `property_key` + `property_value`: property match (both or neither)
/// - `asset_rid`: workbooks attached to this asset
/// - `run_rid`: workbooks attached to this run
#[no_mangle]
pub extern "C" fn nominal_workbook_search(
    client: i32,
    search_text: *const c_char,
    label: *const c_char,
    property_key: *const c_char,
    property_value: *const c_char,
    asset_rid: *const c_char,
    run_rid: *const c_char,
    out_list: *mut i32,
    out_count: *mut u32,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let search_text = match read_optional_str(search_text, "search_text") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let label = match read_optional_str(label, "label") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let property_key = match read_optional_str(property_key, "property_key") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let property_value = match read_optional_str(property_value, "property_value") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let asset_rid = match read_optional_str(asset_rid, "asset_rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let run_rid = match read_optional_str(run_rid, "run_rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if out_list.is_null() || out_count.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_list and out_count must not be null",
            );
        }

        let mut filters = Vec::new();
        if let Some(text) = search_text {
            filters.push(WorkbookQuery::search_text(text));
        }
        if let Some(label) = label {
            filters.push(WorkbookQuery::label(label));
        }
        match (property_key, property_value) {
            (Some(key), Some(value)) => filters.push(WorkbookQuery::property(key, value)),
            (None, None) => {}
            _ => {
                return fail(
                    NominalErrorCode::InvalidArgument,
                    "property_key and property_value must be provided together",
                )
            }
        }
        if let Some(rid) = asset_rid {
            filters.push(WorkbookQuery::asset_rid(rid));
        }
        if let Some(rid) = run_rid {
            filters.push(WorkbookQuery::run_rid(rid));
        }
        // No filters at all matches everything — same behavior as list.
        let query = WorkbookQuery::and(filters);

        match block_on(client.workbooks().search(query)) {
            Ok(workbooks) => {
                let handles: Vec<i32> = workbooks.into_iter().map(WorkbookHandle::insert).collect();
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

/// Archives a workbook (hidden from the UI, not deleted).
#[no_mangle]
pub extern "C" fn nominal_workbook_archive(client: i32, rid: *const c_char) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let rid = match read_required_str(rid, "rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        match block_on(client.workbooks().archive(&rid)) {
            Ok(()) => 0,
            Err(err) => fail_sdk(err),
        }
    })
}

/// Unarchives a workbook, restoring its visibility in the UI.
#[no_mangle]
pub extern "C" fn nominal_workbook_unarchive(client: i32, rid: *const c_char) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let rid = match read_required_str(rid, "rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        match block_on(client.workbooks().unarchive(&rid)) {
            Ok(()) => 0,
            Err(err) => fail_sdk(err),
        }
    })
}

/// Frees a workbook handle. Freeing twice returns an error.
#[no_mangle]
pub extern "C" fn nominal_workbook_free(workbook: i32) -> i32 {
    guard(|| {
        if WorkbookHandle::remove(workbook) {
            0
        } else {
            fail(
                NominalErrorCode::InvalidHandle,
                format!("invalid workbook handle: {workbook}"),
            )
        }
    })
}

// ---------------------------------------------------------------------------
// Staged create-from-template
// ---------------------------------------------------------------------------

/// Accumulated fields for a workbook-create request. Scope RIDs are assets
/// XOR runs, enforced at the add calls.
#[derive(Default)]
pub(crate) struct WorkbookCreateParams {
    title: Option<String>,
    description: Option<String>,
    labels: Vec<String>,
    properties: HashMap<String, String>,
    scope_assets: Vec<String>,
    scope_runs: Vec<String>,
}

handle_registry!(WorkbookCreateStagingHandle, Mutex<WorkbookCreateParams>);

/// Starts staging a workbook-create request. Add at least one scope RID
/// (`nominal_workbook_create_add_scope_asset` or `_add_scope_run` — not
/// both), optionally override title/description (they default to the
/// template's), then fire it with `nominal_workbook_create_commit`. Free
/// with `nominal_workbook_create_free` (commit does not free).
#[no_mangle]
pub extern "C" fn nominal_workbook_create_begin(out_staging: *mut i32) -> i32 {
    guard(|| {
        if out_staging.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_staging must not be null",
            );
        }
        // SAFETY: out_staging checked non-null above; caller owns it.
        unsafe {
            *out_staging =
                WorkbookCreateStagingHandle::insert(Mutex::new(WorkbookCreateParams::default()))
        };
        0
    })
}

/// Sets the workbook's title (overrides the template's).
#[no_mangle]
pub extern "C" fn nominal_workbook_create_set_title(staging: i32, title: *const c_char) -> i32 {
    guard(|| {
        let staging = lookup_handle!(WorkbookCreateStagingHandle, staging);
        let title = match read_required_str(title, "title") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if title.is_empty() {
            return fail(NominalErrorCode::InvalidArgument, "title must not be empty");
        }
        staging
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .title = Some(title);
        0
    })
}

/// Sets the workbook's description (overrides the template's).
#[no_mangle]
pub extern "C" fn nominal_workbook_create_set_description(
    staging: i32,
    description: *const c_char,
) -> i32 {
    guard(|| {
        let staging = lookup_handle!(WorkbookCreateStagingHandle, staging);
        let description = match read_required_str(description, "description") {
            Ok(value) => value,
            Err(code) => return code,
        };
        staging
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .description = Some(description);
        0
    })
}

/// Adds one label to the staged create.
#[no_mangle]
pub extern "C" fn nominal_workbook_create_add_label(staging: i32, label: *const c_char) -> i32 {
    guard(|| {
        let staging = lookup_handle!(WorkbookCreateStagingHandle, staging);
        let label = match read_required_str(label, "label") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if label.is_empty() {
            return fail(NominalErrorCode::InvalidArgument, "label must not be empty");
        }
        staging
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .labels
            .push(label);
        0
    })
}

/// Sets one property on the staged create (same key overwrites).
#[no_mangle]
pub extern "C" fn nominal_workbook_create_set_property(
    staging: i32,
    key: *const c_char,
    value: *const c_char,
) -> i32 {
    guard(|| {
        let staging = lookup_handle!(WorkbookCreateStagingHandle, staging);
        let key = match read_required_str(key, "key") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if key.is_empty() {
            return fail(NominalErrorCode::InvalidArgument, "key must not be empty");
        }
        let value = match read_required_str(value, "value") {
            Ok(value) => value,
            Err(code) => return code,
        };
        staging
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .properties
            .insert(key, value);
        0
    })
}

/// Adds an asset RID to the workbook's data scope. Mutually exclusive with
/// `nominal_workbook_create_add_scope_run`.
#[no_mangle]
pub extern "C" fn nominal_workbook_create_add_scope_asset(
    staging: i32,
    asset_rid: *const c_char,
) -> i32 {
    guard(|| {
        let staging = lookup_handle!(WorkbookCreateStagingHandle, staging);
        let asset_rid = match read_required_str(asset_rid, "asset_rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if asset_rid.is_empty() {
            return fail(
                NominalErrorCode::InvalidArgument,
                "asset_rid must not be empty",
            );
        }
        let mut params = staging
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !params.scope_runs.is_empty() {
            return fail(
                NominalErrorCode::InvalidArgument,
                "scope is already runs — a workbook scope is assets or runs, not both",
            );
        }
        params.scope_assets.push(asset_rid);
        0
    })
}

/// Adds a run RID to the workbook's data scope. Mutually exclusive with
/// `nominal_workbook_create_add_scope_asset`.
#[no_mangle]
pub extern "C" fn nominal_workbook_create_add_scope_run(
    staging: i32,
    run_rid: *const c_char,
) -> i32 {
    guard(|| {
        let staging = lookup_handle!(WorkbookCreateStagingHandle, staging);
        let run_rid = match read_required_str(run_rid, "run_rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if run_rid.is_empty() {
            return fail(
                NominalErrorCode::InvalidArgument,
                "run_rid must not be empty",
            );
        }
        let mut params = staging
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !params.scope_assets.is_empty() {
            return fail(
                NominalErrorCode::InvalidArgument,
                "scope is already assets — a workbook scope is assets or runs, not both",
            );
        }
        params.scope_runs.push(run_rid);
        0
    })
}

/// Creates the staged workbook from the given template (see
/// `nominal_template_get`), writing the new workbook's handle to
/// `out_workbook` (free with `nominal_workbook_free`). At least one scope
/// RID must have been staged. The staging handle stays valid — free it with
/// `nominal_workbook_create_free`.
#[no_mangle]
pub extern "C" fn nominal_workbook_create_commit(
    client: i32,
    template: i32,
    staging: i32,
    out_workbook: *mut i32,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let template = lookup_handle!(TemplateHandle, template);
        let staging = lookup_handle!(WorkbookCreateStagingHandle, staging);
        if out_workbook.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_workbook must not be null",
            );
        }

        let (scope, create) = {
            let params = staging
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let scope = if !params.scope_assets.is_empty() {
                WorkbookDataScope::assets(params.scope_assets.clone())
            } else if !params.scope_runs.is_empty() {
                WorkbookDataScope::runs(params.scope_runs.clone())
            } else {
                return fail(
                    NominalErrorCode::InvalidArgument,
                    "no data scope staged: add at least one scope asset or run RID",
                );
            };
            let mut create = WorkbookCreate::new();
            if let Some(title) = &params.title {
                create = create.title(title.clone());
            }
            if let Some(description) = &params.description {
                create = create.description(description.clone());
            }
            if !params.labels.is_empty() {
                create = create.labels(params.labels.clone());
            }
            if !params.properties.is_empty() {
                create = create.properties(params.properties.clone());
            }
            (scope, create)
        };

        match block_on(
            client
                .workbooks()
                .create_from_template(&template, scope, create),
        ) {
            Ok(workbook) => {
                // SAFETY: out_workbook checked non-null above; caller owns it.
                unsafe { *out_workbook = WorkbookHandle::insert(workbook) };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Frees a workbook-create staging handle. Freeing twice returns an error.
#[no_mangle]
pub extern "C" fn nominal_workbook_create_free(staging: i32) -> i32 {
    guard(|| {
        if WorkbookCreateStagingHandle::remove(staging) {
            0
        } else {
            fail(
                NominalErrorCode::InvalidHandle,
                format!("invalid workbook-create staging handle: {staging}"),
            )
        }
    })
}

// ---------------------------------------------------------------------------
// Field getters
// ---------------------------------------------------------------------------

/// Writes the workbook's RID into `buf`, storing the byte count needed in
/// `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_workbook_rid(
    workbook: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let workbook = lookup_handle!(WorkbookHandle, workbook);
        write_str_field(workbook.rid(), buf, cap, out_needed)
    })
}

/// Writes the workbook's name into `buf`, storing the byte count needed in
/// `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_workbook_name(
    workbook: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let workbook = lookup_handle!(WorkbookHandle, workbook);
        write_str_field(workbook.name(), buf, cap, out_needed)
    })
}

/// Writes the workbook's description into `buf`, and whether one is set into
/// `is_present` (an absent description reports 0 bytes needed).
#[no_mangle]
pub extern "C" fn nominal_workbook_description(
    workbook: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
    is_present: *mut bool,
) -> i32 {
    guard(|| {
        let workbook = lookup_handle!(WorkbookHandle, workbook);
        write_opt_str_field(workbook.description(), buf, cap, out_needed, is_present)
    })
}

/// Writes the URL for viewing this workbook in the Nominal web app into
/// `buf`, storing the byte count needed in `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_workbook_url(
    workbook: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let workbook = lookup_handle!(WorkbookHandle, workbook);
        write_str_field(&workbook.nominal_url(), buf, cap, out_needed)
    })
}

/// Writes the workbook's creation time to `out_millis` as `f64` Unix
/// milliseconds (UTC).
#[no_mangle]
pub extern "C" fn nominal_workbook_created_at(workbook: i32, out_millis: *mut f64) -> i32 {
    guard(|| {
        let workbook = lookup_handle!(WorkbookHandle, workbook);
        if out_millis.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_millis must not be null",
            );
        }
        // SAFETY: out_millis checked non-null above; caller owns it.
        unsafe { *out_millis = workbook.created_at().timestamp_millis() as f64 };
        0
    })
}

/// Stores whether the workbook's scope is assets (0) or runs (1) in
/// `out_type`, as a `NominalWorkbookScopeType` value.
#[no_mangle]
pub extern "C" fn nominal_workbook_scope_type(workbook: i32, out_type: *mut i32) -> i32 {
    guard(|| {
        let workbook = lookup_handle!(WorkbookHandle, workbook);
        if out_type.is_null() {
            return fail(NominalErrorCode::NullArgument, "out_type must not be null");
        }
        let scope_type = match workbook.data_scope() {
            WorkbookDataScope::Assets(_) => NominalWorkbookScopeType::Assets,
            WorkbookDataScope::Runs(_) => NominalWorkbookScopeType::Runs,
        };
        // SAFETY: out_type checked non-null above; caller owns it.
        unsafe { *out_type = scope_type as i32 };
        0
    })
}

/// Stores the number of RIDs in the workbook's data scope in `out_count`.
#[no_mangle]
pub extern "C" fn nominal_workbook_scope_rid_count(workbook: i32, out_count: *mut u32) -> i32 {
    guard(|| {
        let workbook = lookup_handle!(WorkbookHandle, workbook);
        write_count(scope_rids(&workbook).len(), out_count)
    })
}

/// Writes the scope RID at `index` (0-based) into `buf`, storing the byte
/// count needed in `out_needed`. Whether they are asset or run RIDs is
/// reported by `nominal_workbook_scope_type`.
#[no_mangle]
pub extern "C" fn nominal_workbook_scope_rid_at(
    workbook: i32,
    index: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let workbook = lookup_handle!(WorkbookHandle, workbook);
        let rids = scope_rids(&workbook);
        match usize::try_from(index).ok().and_then(|i| rids.get(i)) {
            Some(rid) => write_str_field(rid, buf, cap, out_needed),
            None => index_error(index, rids.len()),
        }
    })
}

/// Stores the number of properties on the workbook in `out_count`.
#[no_mangle]
pub extern "C" fn nominal_workbook_property_count(workbook: i32, out_count: *mut u32) -> i32 {
    guard(|| {
        let workbook = lookup_handle!(WorkbookHandle, workbook);
        write_count(workbook.properties().len(), out_count)
    })
}

/// Writes the key of the property at `index` (0-based, sorted-key order)
/// into `buf`, storing the byte count needed in `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_workbook_property_key_at(
    workbook: i32,
    index: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let workbook = lookup_handle!(WorkbookHandle, workbook);
        match property_at(&workbook, index) {
            Ok((key, _)) => write_str_field(key, buf, cap, out_needed),
            Err(code) => code,
        }
    })
}

/// Writes the value of the property at `index` (0-based, sorted-key order)
/// into `buf`, storing the byte count needed in `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_workbook_property_value_at(
    workbook: i32,
    index: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let workbook = lookup_handle!(WorkbookHandle, workbook);
        match property_at(&workbook, index) {
            Ok((_, value)) => write_str_field(value, buf, cap, out_needed),
            Err(code) => code,
        }
    })
}

/// Stores the number of labels on the workbook in `out_count`.
#[no_mangle]
pub extern "C" fn nominal_workbook_label_count(workbook: i32, out_count: *mut u32) -> i32 {
    guard(|| {
        let workbook = lookup_handle!(WorkbookHandle, workbook);
        write_count(workbook.labels().len(), out_count)
    })
}

/// Writes the label at `index` (0-based) into `buf`, storing the byte count
/// needed in `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_workbook_label_at(
    workbook: i32,
    index: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let workbook = lookup_handle!(WorkbookHandle, workbook);
        let labels = workbook.labels();
        match usize::try_from(index).ok().and_then(|i| labels.get(i)) {
            Some(label) => write_str_field(label, buf, cap, out_needed),
            None => index_error(index, labels.len()),
        }
    })
}
