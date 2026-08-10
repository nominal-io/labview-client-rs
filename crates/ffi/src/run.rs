//! `nominal_run_*` — mirrors `nominal::core::run`.
//!
//! Same shape as `asset.rs`: actions return `i32` status codes with results
//! in out-parameters; collections are `_count` + `_at(index)` pairs;
//! create/update with labels, properties, or asset lists go through staging
//! handles (see `asset.rs` and the CLAUDE.md cookbook).
//!
//! Timestamps cross the boundary as `f64` Unix milliseconds (UTC), both
//! directions. Optional incoming timestamps pair the `f64` with a `bool`
//! "has_" flag — never NaN sentinels.

use std::collections::HashMap;
use std::os::raw::c_char;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use nominal::core::{DataSource, Run, RunCreate, RunQuery, RunUpdate};

use crate::asset::NominalDataSourceType;
use crate::client::ClientHandle;
use crate::error::{fail, fail_sdk, guard, NominalErrorCode};
use crate::handles::{handle_registry, insert_handle_list, lookup_handle};
use crate::runtime::block_on;
use crate::strings::{read_optional_str, read_required_str, write_str_field};

handle_registry!(RunHandle, Run);

/// Converts an incoming `f64` Unix-millisecond timestamp to the `DateTime`
/// the upstream builders take. Fractional milliseconds are rounded.
fn ms_to_datetime(ms: f64, arg: &str) -> Result<DateTime<Utc>, i32> {
    if !ms.is_finite() {
        return Err(fail(
            NominalErrorCode::InvalidArgument,
            format!("argument '{arg}' must be a finite Unix-millisecond timestamp, got {ms}"),
        ));
    }
    DateTime::from_timestamp_millis(ms.round() as i64).ok_or_else(|| {
        fail(
            NominalErrorCode::InvalidArgument,
            format!("argument '{arg}' is out of timestamp range: {ms}"),
        )
    })
}

fn index_error(index: i32, len: usize) -> i32 {
    fail(
        NominalErrorCode::IndexOutOfRange,
        format!("index {index} out of range for collection of {len}"),
    )
}

/// Properties in sorted-key order, so `index` means the same entry across the
/// `_count` / `_key_at` / `_value_at` calls.
fn property_at(run: &Run, index: i32) -> Result<(&String, &String), i32> {
    let mut keys: Vec<&String> = run.properties().keys().collect();
    keys.sort();
    let key = keys.get(usize::try_from(index).map_err(|_| index_error(index, keys.len()))?);
    match key {
        Some(&key) => Ok((key, &run.properties()[key])),
        None => Err(index_error(index, keys.len())),
    }
}

/// Data sources in sorted-ref-name order, so `index` means the same entry
/// across the `_count` / `_name_at` / `_rid_at` / `_type_at` calls.
fn data_source_at(run: &Run, index: i32) -> Result<(&String, &DataSource), i32> {
    let mut names: Vec<&String> = run.data_sources().keys().collect();
    names.sort();
    let name = names.get(usize::try_from(index).map_err(|_| index_error(index, names.len()))?);
    match name {
        Some(&name) => Ok((name, &run.data_sources()[name])),
        None => Err(index_error(index, names.len())),
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

/// Creates a run with a name, start time (`f64` Unix milliseconds), optional
/// description (null or empty = none), and optional end time (`has_end`
/// false = still running / no end). Writes the new run's handle to
/// `out_run`; free with `nominal_run_free`. For labels, properties, or asset
/// links use the `nominal_run_create_begin` staging flow instead.
#[no_mangle]
pub extern "C" fn nominal_run_create(
    client: i32,
    name: *const c_char,
    description: *const c_char,
    start_ms: f64,
    end_ms: f64,
    has_end: bool,
    out_run: *mut i32,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let name = match read_required_str(name, "name") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let description = match read_optional_str(description, "description") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let start = match ms_to_datetime(start_ms, "start_ms") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if out_run.is_null() {
            return fail(NominalErrorCode::NullArgument, "out_run must not be null");
        }

        let mut create = RunCreate::new(name, start);
        if let Some(description) = description {
            create = create.description(description);
        }
        if has_end {
            let end = match ms_to_datetime(end_ms, "end_ms") {
                Ok(value) => value,
                Err(code) => return code,
            };
            create = create.end(end);
        }

        match block_on(client.runs().create(create)) {
            Ok(run) => {
                // SAFETY: out_run checked non-null above; caller owns it.
                unsafe { *out_run = RunHandle::insert(run) };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Fetches the run with the given RID, writing its handle to `out_run`.
/// Free with `nominal_run_free`.
#[no_mangle]
pub extern "C" fn nominal_run_get(client: i32, rid: *const c_char, out_run: *mut i32) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let rid = match read_required_str(rid, "rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if out_run.is_null() {
            return fail(NominalErrorCode::NullArgument, "out_run must not be null");
        }

        match block_on(client.runs().get(&rid)) {
            Ok(run) => {
                // SAFETY: out_run checked non-null above; caller owns it.
                unsafe { *out_run = RunHandle::insert(run) };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Lists all runs (newest first), returning a handle list.
///
/// On success `*out_list` is a handle list of `*out_count` run handles —
/// read them with `nominal_handle_list_get` and free the list with
/// `nominal_handle_list_free`. Each run handle stays valid until passed to
/// `nominal_run_free`, independent of the list.
#[no_mangle]
pub extern "C" fn nominal_run_list(client: i32, out_list: *mut i32, out_count: *mut u32) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        if out_list.is_null() || out_count.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_list and out_count must not be null",
            );
        }

        match block_on(client.runs().list()) {
            Ok(runs) => {
                let handles: Vec<i32> = runs.into_iter().map(RunHandle::insert).collect();
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

/// Searches runs, returning a handle list (see `nominal_run_list` for
/// ownership).
///
/// Filters are optional and combined with AND:
/// - `search_text`, `label`, `property_key` + `property_value`: as in
///   `nominal_asset_search` (null or empty = no filter)
/// - `run_number`: 0 = no filter (real run numbers start at 1)
/// - `start_after_ms` (with `has_start_after`): only runs starting at or
///   after this Unix-millisecond timestamp
/// - `end_before_ms` (with `has_end_before`): only runs ending at or before
///   this Unix-millisecond timestamp
#[no_mangle]
pub extern "C" fn nominal_run_search(
    client: i32,
    search_text: *const c_char,
    label: *const c_char,
    property_key: *const c_char,
    property_value: *const c_char,
    run_number: u32,
    start_after_ms: f64,
    has_start_after: bool,
    end_before_ms: f64,
    has_end_before: bool,
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
        if out_list.is_null() || out_count.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_list and out_count must not be null",
            );
        }

        let mut filters = Vec::new();
        if let Some(text) = search_text {
            filters.push(RunQuery::search_text(text));
        }
        if let Some(label) = label {
            filters.push(RunQuery::label(label));
        }
        match (property_key, property_value) {
            (Some(key), Some(value)) => filters.push(RunQuery::property(key, value)),
            (None, None) => {}
            _ => {
                return fail(
                    NominalErrorCode::InvalidArgument,
                    "property_key and property_value must be provided together",
                )
            }
        }
        if run_number != 0 {
            filters.push(RunQuery::run_number(run_number));
        }
        if has_start_after {
            let start = match ms_to_datetime(start_after_ms, "start_after_ms") {
                Ok(value) => value,
                Err(code) => return code,
            };
            filters.push(RunQuery::start_time_inclusive(start));
        }
        if has_end_before {
            let end = match ms_to_datetime(end_before_ms, "end_before_ms") {
                Ok(value) => value,
                Err(code) => return code,
            };
            filters.push(RunQuery::end_time_inclusive(end));
        }
        // No filters at all matches everything — same behavior as list.
        let query = RunQuery::and(filters);

        match block_on(client.runs().search(query)) {
            Ok(runs) => {
                let handles: Vec<i32> = runs.into_iter().map(RunHandle::insert).collect();
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

/// Updates a run's name and/or description. Null or empty arguments leave
/// that field unchanged. Writes a handle to the updated run to `out_run`
/// (free with `nominal_run_free`). For labels, properties, or time changes
/// use the `nominal_run_update_begin` staging flow instead.
#[no_mangle]
pub extern "C" fn nominal_run_update(
    client: i32,
    rid: *const c_char,
    name: *const c_char,
    description: *const c_char,
    out_run: *mut i32,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let rid = match read_required_str(rid, "rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let name = match read_optional_str(name, "name") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let description = match read_optional_str(description, "description") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if out_run.is_null() {
            return fail(NominalErrorCode::NullArgument, "out_run must not be null");
        }
        if name.is_none() && description.is_none() {
            return fail(
                NominalErrorCode::InvalidArgument,
                "at least one of name or description must be provided",
            );
        }

        let mut update = RunUpdate::new();
        if let Some(name) = name {
            update = update.name(name);
        }
        if let Some(description) = description {
            update = update.description(description);
        }

        match block_on(client.runs().update(&rid, update)) {
            Ok(run) => {
                // SAFETY: out_run checked non-null above; caller owns it.
                unsafe { *out_run = RunHandle::insert(run) };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

// ---------------------------------------------------------------------------
// Data-source attach
//
// Each call attaches ONE data source under a ref name and returns the
// updated run. Ref names should be stable across runs of the same type
// (checklists and templates reference data sources by them). The server
// rejects a ref name the run already has. To attach several sources, call
// these repeatedly — the end state is identical to a batched attach.
// ---------------------------------------------------------------------------

/// Attaches the dataset with `dataset_rid` to the run with `rid` under
/// `ref_name`, writing a handle to the updated run to `out_run` (free with
/// `nominal_run_free`).
#[no_mangle]
pub extern "C" fn nominal_run_add_dataset(
    client: i32,
    rid: *const c_char,
    ref_name: *const c_char,
    dataset_rid: *const c_char,
    out_run: *mut i32,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let rid = match read_required_str(rid, "rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let ref_name = match read_required_str(ref_name, "ref_name") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let dataset_rid = match read_required_str(dataset_rid, "dataset_rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if out_run.is_null() {
            return fail(NominalErrorCode::NullArgument, "out_run must not be null");
        }

        match block_on(client.runs().add_dataset(&rid, &ref_name, &dataset_rid)) {
            Ok(run) => {
                // SAFETY: out_run checked non-null above; caller owns it.
                unsafe { *out_run = RunHandle::insert(run) };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Attaches the video with `video_rid` to the run with `rid` under
/// `ref_name`, writing a handle to the updated run to `out_run` (free with
/// `nominal_run_free`).
#[no_mangle]
pub extern "C" fn nominal_run_add_video(
    client: i32,
    rid: *const c_char,
    ref_name: *const c_char,
    video_rid: *const c_char,
    out_run: *mut i32,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let rid = match read_required_str(rid, "rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let ref_name = match read_required_str(ref_name, "ref_name") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let video_rid = match read_required_str(video_rid, "video_rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if out_run.is_null() {
            return fail(NominalErrorCode::NullArgument, "out_run must not be null");
        }

        match block_on(client.runs().add_video(&rid, &ref_name, &video_rid)) {
            Ok(run) => {
                // SAFETY: out_run checked non-null above; caller owns it.
                unsafe { *out_run = RunHandle::insert(run) };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Attaches the connection with `connection_rid` to the run with `rid` under
/// `ref_name`, writing a handle to the updated run to `out_run` (free with
/// `nominal_run_free`).
#[no_mangle]
pub extern "C" fn nominal_run_add_connection(
    client: i32,
    rid: *const c_char,
    ref_name: *const c_char,
    connection_rid: *const c_char,
    out_run: *mut i32,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let rid = match read_required_str(rid, "rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let ref_name = match read_required_str(ref_name, "ref_name") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let connection_rid = match read_required_str(connection_rid, "connection_rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if out_run.is_null() {
            return fail(NominalErrorCode::NullArgument, "out_run must not be null");
        }

        match block_on(
            client
                .runs()
                .add_connection(&rid, &ref_name, &connection_rid),
        ) {
            Ok(run) => {
                // SAFETY: out_run checked non-null above; caller owns it.
                unsafe { *out_run = RunHandle::insert(run) };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Adds an already-uploaded attachment (by RID) to the run with `rid`. Call
/// repeatedly to add several — each call is one API request reaching the
/// same end state as an upstream batch.
#[no_mangle]
pub extern "C" fn nominal_run_add_attachment(
    client: i32,
    rid: *const c_char,
    attachment_rid: *const c_char,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let rid = match read_required_str(rid, "rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let attachment_rid = match read_required_str(attachment_rid, "attachment_rid") {
            Ok(value) => value,
            Err(code) => return code,
        };

        match block_on(client.runs().add_attachments(&rid, [&attachment_rid])) {
            Ok(()) => 0,
            Err(err) => fail_sdk(err),
        }
    })
}

/// Removes an attachment (by RID) from the run with `rid`. The attachment
/// itself is not deleted from Nominal. Call repeatedly to remove several.
#[no_mangle]
pub extern "C" fn nominal_run_remove_attachment(
    client: i32,
    rid: *const c_char,
    attachment_rid: *const c_char,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let rid = match read_required_str(rid, "rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let attachment_rid = match read_required_str(attachment_rid, "attachment_rid") {
            Ok(value) => value,
            Err(code) => return code,
        };

        match block_on(client.runs().remove_attachments(&rid, [&attachment_rid])) {
            Ok(()) => 0,
            Err(err) => fail_sdk(err),
        }
    })
}

/// Archives a run (hidden from the UI, not deleted).
#[no_mangle]
pub extern "C" fn nominal_run_archive(client: i32, rid: *const c_char) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let rid = match read_required_str(rid, "rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        match block_on(client.runs().archive(&rid)) {
            Ok(()) => 0,
            Err(err) => fail_sdk(err),
        }
    })
}

/// Unarchives a run, restoring its visibility in the UI.
#[no_mangle]
pub extern "C" fn nominal_run_unarchive(client: i32, rid: *const c_char) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let rid = match read_required_str(rid, "rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        match block_on(client.runs().unarchive(&rid)) {
            Ok(()) => 0,
            Err(err) => fail_sdk(err),
        }
    })
}

/// Frees a run handle. Freeing twice returns an error.
#[no_mangle]
pub extern "C" fn nominal_run_free(run: i32) -> i32 {
    guard(|| {
        if RunHandle::remove(run) {
            0
        } else {
            fail(
                NominalErrorCode::InvalidHandle,
                format!("invalid run handle: {run}"),
            )
        }
    })
}

// ---------------------------------------------------------------------------
// Staging handles for create/update with collections and times
// ---------------------------------------------------------------------------

/// Accumulated fields for a run-create request.
pub(crate) struct RunCreateParams {
    name: String,
    start: DateTime<Utc>,
    description: Option<String>,
    end: Option<DateTime<Utc>>,
    labels: Vec<String>,
    properties: HashMap<String, String>,
    assets: Vec<String>,
}

handle_registry!(RunCreateStagingHandle, Mutex<RunCreateParams>);

/// Starts staging a run-create request with the required name and start time
/// (`f64` Unix milliseconds). Add optional fields with the
/// `nominal_run_create_set_*` / `_add_*` calls, then fire it with
/// `nominal_run_create_commit`. Free with `nominal_run_create_free` (commit
/// does not free).
#[no_mangle]
pub extern "C" fn nominal_run_create_begin(
    name: *const c_char,
    start_ms: f64,
    out_staging: *mut i32,
) -> i32 {
    guard(|| {
        let name = match read_required_str(name, "name") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if name.is_empty() {
            return fail(NominalErrorCode::InvalidArgument, "name must not be empty");
        }
        let start = match ms_to_datetime(start_ms, "start_ms") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if out_staging.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_staging must not be null",
            );
        }
        let params = RunCreateParams {
            name,
            start,
            description: None,
            end: None,
            labels: Vec::new(),
            properties: HashMap::new(),
            assets: Vec::new(),
        };
        // SAFETY: out_staging checked non-null above; caller owns it.
        unsafe { *out_staging = RunCreateStagingHandle::insert(Mutex::new(params)) };
        0
    })
}

/// Sets the description on a staged run create (empty clears it).
#[no_mangle]
pub extern "C" fn nominal_run_create_set_description(
    staging: i32,
    description: *const c_char,
) -> i32 {
    guard(|| {
        let staging = lookup_handle!(RunCreateStagingHandle, staging);
        let description = match read_optional_str(description, "description") {
            Ok(value) => value,
            Err(code) => return code,
        };
        staging
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .description = description;
        0
    })
}

/// Sets the end time (`f64` Unix milliseconds) on a staged run create.
#[no_mangle]
pub extern "C" fn nominal_run_create_set_end(staging: i32, end_ms: f64) -> i32 {
    guard(|| {
        let staging = lookup_handle!(RunCreateStagingHandle, staging);
        let end = match ms_to_datetime(end_ms, "end_ms") {
            Ok(value) => value,
            Err(code) => return code,
        };
        staging
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .end = Some(end);
        0
    })
}

/// Adds one label to a staged run create.
#[no_mangle]
pub extern "C" fn nominal_run_create_add_label(staging: i32, label: *const c_char) -> i32 {
    guard(|| {
        let staging = lookup_handle!(RunCreateStagingHandle, staging);
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

/// Sets one property on a staged run create (same key overwrites).
#[no_mangle]
pub extern "C" fn nominal_run_create_set_property(
    staging: i32,
    key: *const c_char,
    value: *const c_char,
) -> i32 {
    guard(|| {
        let staging = lookup_handle!(RunCreateStagingHandle, staging);
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

/// Links one asset (by RID) to a staged run create.
#[no_mangle]
pub extern "C" fn nominal_run_create_add_asset(staging: i32, asset_rid: *const c_char) -> i32 {
    guard(|| {
        let staging = lookup_handle!(RunCreateStagingHandle, staging);
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
        staging
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .assets
            .push(asset_rid);
        0
    })
}

/// Creates the staged run, writing the new run's handle to `out_run` (free
/// with `nominal_run_free`). The staging handle stays valid — free it with
/// `nominal_run_create_free`.
#[no_mangle]
pub extern "C" fn nominal_run_create_commit(client: i32, staging: i32, out_run: *mut i32) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let staging = lookup_handle!(RunCreateStagingHandle, staging);
        if out_run.is_null() {
            return fail(NominalErrorCode::NullArgument, "out_run must not be null");
        }

        let create = {
            let params = staging
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let mut create = RunCreate::new(params.name.clone(), params.start);
            if let Some(description) = &params.description {
                create = create.description(description.clone());
            }
            if let Some(end) = params.end {
                create = create.end(end);
            }
            if !params.labels.is_empty() {
                create = create.labels(params.labels.clone());
            }
            if !params.properties.is_empty() {
                create = create.properties(params.properties.clone());
            }
            if !params.assets.is_empty() {
                create = create.assets(params.assets.clone());
            }
            create
        };

        match block_on(client.runs().create(create)) {
            Ok(run) => {
                // SAFETY: out_run checked non-null above; caller owns it.
                unsafe { *out_run = RunHandle::insert(run) };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Frees a run-create staging handle. Freeing twice returns an error.
#[no_mangle]
pub extern "C" fn nominal_run_create_free(staging: i32) -> i32 {
    guard(|| {
        if RunCreateStagingHandle::remove(staging) {
            0
        } else {
            fail(
                NominalErrorCode::InvalidHandle,
                format!("invalid run-create staging handle: {staging}"),
            )
        }
    })
}

/// Accumulated fields for a run-update request. `None` = leave that field
/// untouched; `Some` = replace it entirely (upstream semantics).
#[derive(Default)]
pub(crate) struct RunUpdateParams {
    name: Option<String>,
    description: Option<String>,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
    labels: Option<Vec<String>>,
    properties: Option<HashMap<String, String>>,
}

handle_registry!(RunUpdateStagingHandle, Mutex<RunUpdateParams>);

/// Starts staging a run update. Only fields set via the
/// `nominal_run_update_set_*` / `_add_*` calls are changed at commit; the
/// rest remain untouched. Free with `nominal_run_update_free`.
#[no_mangle]
pub extern "C" fn nominal_run_update_begin(out_staging: *mut i32) -> i32 {
    guard(|| {
        if out_staging.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_staging must not be null",
            );
        }
        // SAFETY: out_staging checked non-null above; caller owns it.
        unsafe {
            *out_staging = RunUpdateStagingHandle::insert(Mutex::new(RunUpdateParams::default()))
        };
        0
    })
}

/// Stages a new name.
#[no_mangle]
pub extern "C" fn nominal_run_update_set_name(staging: i32, name: *const c_char) -> i32 {
    guard(|| {
        let staging = lookup_handle!(RunUpdateStagingHandle, staging);
        let name = match read_required_str(name, "name") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if name.is_empty() {
            return fail(NominalErrorCode::InvalidArgument, "name must not be empty");
        }
        staging
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .name = Some(name);
        0
    })
}

/// Stages a new description (empty clears the description).
#[no_mangle]
pub extern "C" fn nominal_run_update_set_description(
    staging: i32,
    description: *const c_char,
) -> i32 {
    guard(|| {
        let staging = lookup_handle!(RunUpdateStagingHandle, staging);
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

/// Stages a new start time (`f64` Unix milliseconds).
#[no_mangle]
pub extern "C" fn nominal_run_update_set_start(staging: i32, start_ms: f64) -> i32 {
    guard(|| {
        let staging = lookup_handle!(RunUpdateStagingHandle, staging);
        let start = match ms_to_datetime(start_ms, "start_ms") {
            Ok(value) => value,
            Err(code) => return code,
        };
        staging
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .start = Some(start);
        0
    })
}

/// Stages a new end time (`f64` Unix milliseconds).
#[no_mangle]
pub extern "C" fn nominal_run_update_set_end(staging: i32, end_ms: f64) -> i32 {
    guard(|| {
        let staging = lookup_handle!(RunUpdateStagingHandle, staging);
        let end = match ms_to_datetime(end_ms, "end_ms") {
            Ok(value) => value,
            Err(code) => return code,
        };
        staging
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .end = Some(end);
        0
    })
}

/// Adds one label to the staged update. NOTE: touching labels at all means
/// the commit REPLACES the run's entire label set with exactly the labels
/// accumulated here (upstream semantics) — to keep existing labels, add them
/// too.
#[no_mangle]
pub extern "C" fn nominal_run_update_add_label(staging: i32, label: *const c_char) -> i32 {
    guard(|| {
        let staging = lookup_handle!(RunUpdateStagingHandle, staging);
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
            .get_or_insert_with(Vec::new)
            .push(label);
        0
    })
}

/// Sets one property on the staged update (same key overwrites). NOTE: same
/// replace semantics as labels — touching properties at all means the commit
/// replaces the run's entire property map with the ones accumulated here.
#[no_mangle]
pub extern "C" fn nominal_run_update_set_property(
    staging: i32,
    key: *const c_char,
    value: *const c_char,
) -> i32 {
    guard(|| {
        let staging = lookup_handle!(RunUpdateStagingHandle, staging);
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
            .get_or_insert_with(HashMap::new)
            .insert(key, value);
        0
    })
}

/// Applies the staged update to the run with the given RID, writing a handle
/// to the updated run to `out_run` (free with `nominal_run_free`). At least
/// one field must have been staged. The staging handle stays valid — free it
/// with `nominal_run_update_free`.
#[no_mangle]
pub extern "C" fn nominal_run_update_commit(
    client: i32,
    rid: *const c_char,
    staging: i32,
    out_run: *mut i32,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let staging = lookup_handle!(RunUpdateStagingHandle, staging);
        let rid = match read_required_str(rid, "rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if out_run.is_null() {
            return fail(NominalErrorCode::NullArgument, "out_run must not be null");
        }

        let update = {
            let params = staging
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if params.name.is_none()
                && params.description.is_none()
                && params.start.is_none()
                && params.end.is_none()
                && params.labels.is_none()
                && params.properties.is_none()
            {
                return fail(
                    NominalErrorCode::InvalidArgument,
                    "staged update has no fields set",
                );
            }
            let mut update = RunUpdate::new();
            if let Some(name) = &params.name {
                update = update.name(name.clone());
            }
            if let Some(description) = &params.description {
                update = update.description(description.clone());
            }
            if let Some(start) = params.start {
                update = update.start(start);
            }
            if let Some(end) = params.end {
                update = update.end(end);
            }
            if let Some(labels) = &params.labels {
                update = update.labels(labels.clone());
            }
            if let Some(properties) = &params.properties {
                update = update.properties(properties.clone());
            }
            update
        };

        match block_on(client.runs().update(&rid, update)) {
            Ok(run) => {
                // SAFETY: out_run checked non-null above; caller owns it.
                unsafe { *out_run = RunHandle::insert(run) };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Frees a run-update staging handle. Freeing twice returns an error.
#[no_mangle]
pub extern "C" fn nominal_run_update_free(staging: i32) -> i32 {
    guard(|| {
        if RunUpdateStagingHandle::remove(staging) {
            0
        } else {
            fail(
                NominalErrorCode::InvalidHandle,
                format!("invalid run-update staging handle: {staging}"),
            )
        }
    })
}

// ---------------------------------------------------------------------------
// Field getters
// ---------------------------------------------------------------------------

/// Writes the run's RID into `buf`, storing the byte count needed in
/// `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_run_rid(
    run: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let run = lookup_handle!(RunHandle, run);
        write_str_field(run.rid(), buf, cap, out_needed)
    })
}

/// Writes the run's name into `buf`, storing the byte count needed in
/// `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_run_name(
    run: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let run = lookup_handle!(RunHandle, run);
        write_str_field(run.name(), buf, cap, out_needed)
    })
}

/// Writes the run's description into `buf` (always present upstream — may be
/// empty), storing the byte count needed in `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_run_description(
    run: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let run = lookup_handle!(RunHandle, run);
        write_str_field(run.description(), buf, cap, out_needed)
    })
}

/// Writes the URL for viewing this run in the Nominal web app into `buf`,
/// storing the byte count needed in `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_run_url(
    run: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let run = lookup_handle!(RunHandle, run);
        write_str_field(&run.nominal_url(), buf, cap, out_needed)
    })
}

/// Stores the run's number in `out_number`.
#[no_mangle]
pub extern "C" fn nominal_run_number(run: i32, out_number: *mut u32) -> i32 {
    guard(|| {
        let run = lookup_handle!(RunHandle, run);
        if out_number.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_number must not be null",
            );
        }
        // SAFETY: out_number checked non-null above; caller owns it.
        unsafe { *out_number = run.run_number() };
        0
    })
}

/// Writes the run's start time to `out_millis` as `f64` Unix milliseconds
/// (UTC).
#[no_mangle]
pub extern "C" fn nominal_run_start(run: i32, out_millis: *mut f64) -> i32 {
    guard(|| {
        let run = lookup_handle!(RunHandle, run);
        if out_millis.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_millis must not be null",
            );
        }
        // SAFETY: out_millis checked non-null above; caller owns it.
        unsafe { *out_millis = run.start().timestamp_millis() as f64 };
        0
    })
}

/// Writes the run's end time to `out_millis` (`f64` Unix milliseconds, UTC)
/// and whether one is set to `is_present` (an absent end — a still-running
/// run — reports 0).
#[no_mangle]
pub extern "C" fn nominal_run_end(run: i32, out_millis: *mut f64, is_present: *mut bool) -> i32 {
    guard(|| {
        let run = lookup_handle!(RunHandle, run);
        if out_millis.is_null() || is_present.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_millis and is_present must not be null",
            );
        }
        let end = run.end();
        // SAFETY: out pointers checked non-null above; caller owns them.
        unsafe {
            *is_present = end.is_some();
            *out_millis = end.map_or(0.0, |t| t.timestamp_millis() as f64);
        }
        0
    })
}

/// Writes the run's creation time to `out_millis` as `f64` Unix milliseconds
/// (UTC).
#[no_mangle]
pub extern "C" fn nominal_run_created_at(run: i32, out_millis: *mut f64) -> i32 {
    guard(|| {
        let run = lookup_handle!(RunHandle, run);
        if out_millis.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_millis must not be null",
            );
        }
        // SAFETY: out_millis checked non-null above; caller owns it.
        unsafe { *out_millis = run.created_at().timestamp_millis() as f64 };
        0
    })
}

/// Stores the number of properties on the run in `out_count`.
#[no_mangle]
pub extern "C" fn nominal_run_property_count(run: i32, out_count: *mut u32) -> i32 {
    guard(|| {
        let run = lookup_handle!(RunHandle, run);
        write_count(run.properties().len(), out_count)
    })
}

/// Writes the key of the property at `index` (0-based, sorted-key order) into
/// `buf`, storing the byte count needed in `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_run_property_key_at(
    run: i32,
    index: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let run = lookup_handle!(RunHandle, run);
        match property_at(&run, index) {
            Ok((key, _)) => write_str_field(key, buf, cap, out_needed),
            Err(code) => code,
        }
    })
}

/// Writes the value of the property at `index` (0-based, sorted-key order)
/// into `buf`, storing the byte count needed in `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_run_property_value_at(
    run: i32,
    index: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let run = lookup_handle!(RunHandle, run);
        match property_at(&run, index) {
            Ok((_, value)) => write_str_field(value, buf, cap, out_needed),
            Err(code) => code,
        }
    })
}

/// Stores the number of labels on the run in `out_count`.
#[no_mangle]
pub extern "C" fn nominal_run_label_count(run: i32, out_count: *mut u32) -> i32 {
    guard(|| {
        let run = lookup_handle!(RunHandle, run);
        write_count(run.labels().len(), out_count)
    })
}

/// Writes the label at `index` (0-based) into `buf`, storing the byte count
/// needed in `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_run_label_at(
    run: i32,
    index: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let run = lookup_handle!(RunHandle, run);
        let labels = run.labels();
        match usize::try_from(index).ok().and_then(|i| labels.get(i)) {
            Some(label) => write_str_field(label, buf, cap, out_needed),
            None => index_error(index, labels.len()),
        }
    })
}

/// Stores the number of assets linked to the run in `out_count`.
#[no_mangle]
pub extern "C" fn nominal_run_asset_count(run: i32, out_count: *mut u32) -> i32 {
    guard(|| {
        let run = lookup_handle!(RunHandle, run);
        write_count(run.assets().len(), out_count)
    })
}

/// Writes the RID of the linked asset at `index` (0-based) into `buf`,
/// storing the byte count needed in `out_needed`. Fetch the full asset with
/// `nominal_asset_get`.
#[no_mangle]
pub extern "C" fn nominal_run_asset_rid_at(
    run: i32,
    index: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let run = lookup_handle!(RunHandle, run);
        let assets = run.assets();
        match usize::try_from(index).ok().and_then(|i| assets.get(i)) {
            Some(rid) => write_str_field(rid, buf, cap, out_needed),
            None => index_error(index, assets.len()),
        }
    })
}

/// Stores the number of data sources attached to the run in `out_count`.
/// (Empty for multi-asset runs — data sources live on the assets there.)
#[no_mangle]
pub extern "C" fn nominal_run_data_source_count(run: i32, out_count: *mut u32) -> i32 {
    guard(|| {
        let run = lookup_handle!(RunHandle, run);
        write_count(run.data_sources().len(), out_count)
    })
}

/// Writes the ref name of the data source at `index` (0-based, sorted-name
/// order) into `buf`, storing the byte count needed in `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_run_data_source_name_at(
    run: i32,
    index: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let run = lookup_handle!(RunHandle, run);
        match data_source_at(&run, index) {
            Ok((name, _)) => write_str_field(name, buf, cap, out_needed),
            Err(code) => code,
        }
    })
}

/// Writes the RID of the data source at `index` (0-based, sorted-name order)
/// into `buf`, storing the byte count needed in `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_run_data_source_rid_at(
    run: i32,
    index: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let run = lookup_handle!(RunHandle, run);
        match data_source_at(&run, index) {
            Ok((_, source)) => write_str_field(source.rid(), buf, cap, out_needed),
            Err(code) => code,
        }
    })
}

/// Writes the kind of the data source at `index` (0-based, sorted-name order)
/// to `out_type` as a `NominalDataSourceType` value.
#[no_mangle]
pub extern "C" fn nominal_run_data_source_type_at(run: i32, index: i32, out_type: *mut i32) -> i32 {
    guard(|| {
        let run = lookup_handle!(RunHandle, run);
        if out_type.is_null() {
            return fail(NominalErrorCode::NullArgument, "out_type must not be null");
        }
        match data_source_at(&run, index) {
            Ok((_, source)) => {
                // Exhaustive on purpose: a new upstream variant should fail
                // this build (the drift signal), not get silently mislabeled.
                let kind = match source {
                    DataSource::Dataset(_) => NominalDataSourceType::Dataset,
                    DataSource::Video(_) => NominalDataSourceType::Video,
                    DataSource::Connection(_) => NominalDataSourceType::Connection,
                };
                // SAFETY: out_type checked non-null above; caller owns it.
                unsafe { *out_type = kind as i32 };
                0
            }
            // Message already set by data_source_at via index_error.
            Err(code) => code,
        }
    })
}
