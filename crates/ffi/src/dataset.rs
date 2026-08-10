//! `nominal_dataset_*` — mirrors the dataset operations of
//! `nominal::core::catalog`.
//!
//! Same shape as `asset.rs`: actions return `i32` status codes with results
//! in out-parameters; collections are `_count` + `_at(index)` pairs;
//! create/update with labels or properties go through staging handles (see
//! the CLAUDE.md cookbook). Dataset-specific extra: an optional channel
//! delimiter on create (splits channel names into a prefix tree in the UI).

use std::collections::HashMap;
use std::os::raw::c_char;
use std::sync::Mutex;

use nominal::core::{Dataset, DatasetCreate, DatasetQuery, DatasetUpdate};

use crate::client::ClientHandle;
use crate::error::{fail, fail_sdk, guard, NominalErrorCode};
use crate::handles::{handle_registry, insert_handle_list, lookup_handle};
use crate::runtime::block_on;
use crate::strings::{read_optional_str, read_required_str, write_opt_str_field, write_str_field};

handle_registry!(DatasetHandle, Dataset);

fn index_error(index: i32, len: usize) -> i32 {
    fail(
        NominalErrorCode::IndexOutOfRange,
        format!("index {index} out of range for collection of {len}"),
    )
}

/// Properties in sorted-key order, so `index` means the same entry across the
/// `_count` / `_key_at` / `_value_at` calls.
fn property_at(dataset: &Dataset, index: i32) -> Result<(&String, &String), i32> {
    let mut keys: Vec<&String> = dataset.properties().keys().collect();
    keys.sort();
    let key = keys.get(usize::try_from(index).map_err(|_| index_error(index, keys.len()))?);
    match key {
        Some(&key) => Ok((key, &dataset.properties()[key])),
        None => Err(index_error(index, keys.len())),
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

/// Creates an empty dataset with the given name and (optional, may be null
/// or empty) description, writing the new dataset's handle to `out_dataset`.
/// Data is added by ingest, separately. Free with `nominal_dataset_free`.
/// For labels, properties, or a channel delimiter use the
/// `nominal_dataset_create_begin` staging flow instead.
#[no_mangle]
pub extern "C" fn nominal_dataset_create(
    client: i32,
    name: *const c_char,
    description: *const c_char,
    out_dataset: *mut i32,
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
        if out_dataset.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_dataset must not be null",
            );
        }

        let mut create = DatasetCreate::new(name);
        if let Some(description) = description {
            create = create.description(description);
        }

        match block_on(client.catalog().create_dataset(create)) {
            Ok(dataset) => {
                // SAFETY: out_dataset checked non-null above; caller owns it.
                unsafe { *out_dataset = DatasetHandle::insert(dataset) };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Fetches the dataset with the given RID, writing its handle to
/// `out_dataset`. Free with `nominal_dataset_free`.
#[no_mangle]
pub extern "C" fn nominal_dataset_get(
    client: i32,
    rid: *const c_char,
    out_dataset: *mut i32,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let rid = match read_required_str(rid, "rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if out_dataset.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_dataset must not be null",
            );
        }

        match block_on(client.catalog().get_dataset(&rid)) {
            Ok(dataset) => {
                // SAFETY: out_dataset checked non-null above; caller owns it.
                unsafe { *out_dataset = DatasetHandle::insert(dataset) };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Lists all datasets (newest first), returning a handle list.
///
/// On success `*out_list` is a handle list of `*out_count` dataset handles —
/// read them with `nominal_handle_list_get` and free the list with
/// `nominal_handle_list_free`. Each dataset handle stays valid until passed
/// to `nominal_dataset_free`, independent of the list.
#[no_mangle]
pub extern "C" fn nominal_dataset_list(
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

        match block_on(client.catalog().list_datasets()) {
            Ok(datasets) => {
                let handles: Vec<i32> = datasets.into_iter().map(DatasetHandle::insert).collect();
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

/// Searches datasets, returning a handle list (see `nominal_dataset_list`
/// for ownership).
///
/// Filters may each be null or empty ("no filter"); the ones provided are
/// combined with AND:
/// - `search_text`: fuzzy full-text match on name and description
/// - `label`: exact label match
/// - `property_key` + `property_value`: property match (both or neither)
#[no_mangle]
pub extern "C" fn nominal_dataset_search(
    client: i32,
    search_text: *const c_char,
    label: *const c_char,
    property_key: *const c_char,
    property_value: *const c_char,
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
            filters.push(DatasetQuery::search_text(text));
        }
        if let Some(label) = label {
            filters.push(DatasetQuery::label(label));
        }
        match (property_key, property_value) {
            (Some(key), Some(value)) => filters.push(DatasetQuery::property(key, value)),
            (None, None) => {}
            _ => {
                return fail(
                    NominalErrorCode::InvalidArgument,
                    "property_key and property_value must be provided together",
                )
            }
        }
        // No filters at all matches everything — same behavior as list.
        let query = DatasetQuery::and(filters);

        match block_on(client.catalog().search_datasets(query)) {
            Ok(datasets) => {
                let handles: Vec<i32> = datasets.into_iter().map(DatasetHandle::insert).collect();
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

/// Updates a dataset's name and/or description. Null or empty arguments
/// leave that field unchanged. Writes a handle to the updated dataset to
/// `out_dataset` (free with `nominal_dataset_free`). For labels or
/// properties use the `nominal_dataset_update_begin` staging flow instead.
#[no_mangle]
pub extern "C" fn nominal_dataset_update(
    client: i32,
    rid: *const c_char,
    name: *const c_char,
    description: *const c_char,
    out_dataset: *mut i32,
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
        if out_dataset.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_dataset must not be null",
            );
        }
        if name.is_none() && description.is_none() {
            return fail(
                NominalErrorCode::InvalidArgument,
                "at least one of name or description must be provided",
            );
        }

        let mut update = DatasetUpdate::new();
        if let Some(name) = name {
            update = update.name(name);
        }
        if let Some(description) = description {
            update = update.description(description);
        }

        match block_on(client.catalog().update_dataset(&rid, update)) {
            Ok(dataset) => {
                // SAFETY: out_dataset checked non-null above; caller owns it.
                unsafe { *out_dataset = DatasetHandle::insert(dataset) };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Archives a dataset (hidden from the UI, not deleted).
#[no_mangle]
pub extern "C" fn nominal_dataset_archive(client: i32, rid: *const c_char) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let rid = match read_required_str(rid, "rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        match block_on(client.catalog().archive_dataset(&rid)) {
            Ok(()) => 0,
            Err(err) => fail_sdk(err),
        }
    })
}

/// Unarchives a dataset, restoring its visibility in the UI.
#[no_mangle]
pub extern "C" fn nominal_dataset_unarchive(client: i32, rid: *const c_char) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let rid = match read_required_str(rid, "rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        match block_on(client.catalog().unarchive_dataset(&rid)) {
            Ok(()) => 0,
            Err(err) => fail_sdk(err),
        }
    })
}

/// Frees a dataset handle. Freeing twice returns an error.
#[no_mangle]
pub extern "C" fn nominal_dataset_free(dataset: i32) -> i32 {
    guard(|| {
        if DatasetHandle::remove(dataset) {
            0
        } else {
            fail(
                NominalErrorCode::InvalidHandle,
                format!("invalid dataset handle: {dataset}"),
            )
        }
    })
}

// ---------------------------------------------------------------------------
// Staging handles for create/update with labels and properties
// ---------------------------------------------------------------------------

/// Accumulated fields for a dataset-create request.
pub(crate) struct DatasetCreateParams {
    name: String,
    description: Option<String>,
    channel_delimiter: Option<String>,
    labels: Vec<String>,
    properties: HashMap<String, String>,
}

handle_registry!(DatasetCreateStagingHandle, Mutex<DatasetCreateParams>);

/// Starts staging a dataset-create request with the (required) name. Add
/// optional fields with the `nominal_dataset_create_set_*` / `_add_*` calls,
/// then fire it with `nominal_dataset_create_commit`. Free with
/// `nominal_dataset_create_free` (commit does not free).
#[no_mangle]
pub extern "C" fn nominal_dataset_create_begin(name: *const c_char, out_staging: *mut i32) -> i32 {
    guard(|| {
        let name = match read_required_str(name, "name") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if name.is_empty() {
            return fail(NominalErrorCode::InvalidArgument, "name must not be empty");
        }
        if out_staging.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_staging must not be null",
            );
        }
        let params = DatasetCreateParams {
            name,
            description: None,
            channel_delimiter: None,
            labels: Vec::new(),
            properties: HashMap::new(),
        };
        // SAFETY: out_staging checked non-null above; caller owns it.
        unsafe { *out_staging = DatasetCreateStagingHandle::insert(Mutex::new(params)) };
        0
    })
}

/// Sets the description on a staged dataset create (empty clears it).
#[no_mangle]
pub extern "C" fn nominal_dataset_create_set_description(
    staging: i32,
    description: *const c_char,
) -> i32 {
    guard(|| {
        let staging = lookup_handle!(DatasetCreateStagingHandle, staging);
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

/// Sets the channel-name delimiter on a staged dataset create (e.g. "." to
/// group channels like "engine.temp" into a prefix tree in the UI).
#[no_mangle]
pub extern "C" fn nominal_dataset_create_set_channel_delimiter(
    staging: i32,
    delimiter: *const c_char,
) -> i32 {
    guard(|| {
        let staging = lookup_handle!(DatasetCreateStagingHandle, staging);
        let delimiter = match read_required_str(delimiter, "delimiter") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if delimiter.is_empty() {
            return fail(
                NominalErrorCode::InvalidArgument,
                "delimiter must not be empty",
            );
        }
        staging
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .channel_delimiter = Some(delimiter);
        0
    })
}

/// Adds one label to a staged dataset create.
#[no_mangle]
pub extern "C" fn nominal_dataset_create_add_label(staging: i32, label: *const c_char) -> i32 {
    guard(|| {
        let staging = lookup_handle!(DatasetCreateStagingHandle, staging);
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

/// Sets one property on a staged dataset create (same key overwrites).
#[no_mangle]
pub extern "C" fn nominal_dataset_create_set_property(
    staging: i32,
    key: *const c_char,
    value: *const c_char,
) -> i32 {
    guard(|| {
        let staging = lookup_handle!(DatasetCreateStagingHandle, staging);
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

/// Creates the staged dataset, writing the new dataset's handle to
/// `out_dataset` (free with `nominal_dataset_free`). The staging handle
/// stays valid — free it with `nominal_dataset_create_free`.
#[no_mangle]
pub extern "C" fn nominal_dataset_create_commit(
    client: i32,
    staging: i32,
    out_dataset: *mut i32,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let staging = lookup_handle!(DatasetCreateStagingHandle, staging);
        if out_dataset.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_dataset must not be null",
            );
        }

        let create = {
            let params = staging
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let mut create = DatasetCreate::new(params.name.clone());
            if let Some(description) = &params.description {
                create = create.description(description.clone());
            }
            if let Some(delimiter) = &params.channel_delimiter {
                create = create.channel_delimiter(delimiter.clone());
            }
            if !params.labels.is_empty() {
                create = create.labels(params.labels.clone());
            }
            if !params.properties.is_empty() {
                create = create.properties(params.properties.clone());
            }
            create
        };

        match block_on(client.catalog().create_dataset(create)) {
            Ok(dataset) => {
                // SAFETY: out_dataset checked non-null above; caller owns it.
                unsafe { *out_dataset = DatasetHandle::insert(dataset) };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Frees a dataset-create staging handle. Freeing twice returns an error.
#[no_mangle]
pub extern "C" fn nominal_dataset_create_free(staging: i32) -> i32 {
    guard(|| {
        if DatasetCreateStagingHandle::remove(staging) {
            0
        } else {
            fail(
                NominalErrorCode::InvalidHandle,
                format!("invalid dataset-create staging handle: {staging}"),
            )
        }
    })
}

/// Accumulated fields for a dataset-update request. `None` = leave that
/// field untouched; `Some` = replace it entirely (upstream semantics).
#[derive(Default)]
pub(crate) struct DatasetUpdateParams {
    name: Option<String>,
    description: Option<String>,
    labels: Option<Vec<String>>,
    properties: Option<HashMap<String, String>>,
}

handle_registry!(DatasetUpdateStagingHandle, Mutex<DatasetUpdateParams>);

/// Starts staging a dataset update. Only fields set via the
/// `nominal_dataset_update_set_*` / `_add_*` calls are changed at commit;
/// the rest remain untouched. Free with `nominal_dataset_update_free`.
#[no_mangle]
pub extern "C" fn nominal_dataset_update_begin(out_staging: *mut i32) -> i32 {
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
                DatasetUpdateStagingHandle::insert(Mutex::new(DatasetUpdateParams::default()))
        };
        0
    })
}

/// Stages a new name.
#[no_mangle]
pub extern "C" fn nominal_dataset_update_set_name(staging: i32, name: *const c_char) -> i32 {
    guard(|| {
        let staging = lookup_handle!(DatasetUpdateStagingHandle, staging);
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
pub extern "C" fn nominal_dataset_update_set_description(
    staging: i32,
    description: *const c_char,
) -> i32 {
    guard(|| {
        let staging = lookup_handle!(DatasetUpdateStagingHandle, staging);
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

/// Adds one label to the staged update. NOTE: touching labels at all means
/// the commit REPLACES the dataset's entire label set with exactly the
/// labels accumulated here (upstream semantics) — to keep existing labels,
/// add them too.
#[no_mangle]
pub extern "C" fn nominal_dataset_update_add_label(staging: i32, label: *const c_char) -> i32 {
    guard(|| {
        let staging = lookup_handle!(DatasetUpdateStagingHandle, staging);
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
/// replaces the dataset's entire property map with the ones accumulated
/// here.
#[no_mangle]
pub extern "C" fn nominal_dataset_update_set_property(
    staging: i32,
    key: *const c_char,
    value: *const c_char,
) -> i32 {
    guard(|| {
        let staging = lookup_handle!(DatasetUpdateStagingHandle, staging);
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

/// Applies the staged update to the dataset with the given RID, writing a
/// handle to the updated dataset to `out_dataset` (free with
/// `nominal_dataset_free`). At least one field must have been staged. The
/// staging handle stays valid — free it with `nominal_dataset_update_free`.
#[no_mangle]
pub extern "C" fn nominal_dataset_update_commit(
    client: i32,
    rid: *const c_char,
    staging: i32,
    out_dataset: *mut i32,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let staging = lookup_handle!(DatasetUpdateStagingHandle, staging);
        let rid = match read_required_str(rid, "rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if out_dataset.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_dataset must not be null",
            );
        }

        let update = {
            let params = staging
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if params.name.is_none()
                && params.description.is_none()
                && params.labels.is_none()
                && params.properties.is_none()
            {
                return fail(
                    NominalErrorCode::InvalidArgument,
                    "staged update has no fields set",
                );
            }
            let mut update = DatasetUpdate::new();
            if let Some(name) = &params.name {
                update = update.name(name.clone());
            }
            if let Some(description) = &params.description {
                update = update.description(description.clone());
            }
            if let Some(labels) = &params.labels {
                update = update.labels(labels.clone());
            }
            if let Some(properties) = &params.properties {
                update = update.properties(properties.clone());
            }
            update
        };

        match block_on(client.catalog().update_dataset(&rid, update)) {
            Ok(dataset) => {
                // SAFETY: out_dataset checked non-null above; caller owns it.
                unsafe { *out_dataset = DatasetHandle::insert(dataset) };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Frees a dataset-update staging handle. Freeing twice returns an error.
#[no_mangle]
pub extern "C" fn nominal_dataset_update_free(staging: i32) -> i32 {
    guard(|| {
        if DatasetUpdateStagingHandle::remove(staging) {
            0
        } else {
            fail(
                NominalErrorCode::InvalidHandle,
                format!("invalid dataset-update staging handle: {staging}"),
            )
        }
    })
}

// ---------------------------------------------------------------------------
// Field getters
// ---------------------------------------------------------------------------

/// Writes the dataset's RID into `buf`, storing the byte count needed in
/// `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_dataset_rid(
    dataset: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let dataset = lookup_handle!(DatasetHandle, dataset);
        write_str_field(dataset.rid(), buf, cap, out_needed)
    })
}

/// Writes the dataset's name into `buf`, storing the byte count needed in
/// `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_dataset_name(
    dataset: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let dataset = lookup_handle!(DatasetHandle, dataset);
        write_str_field(dataset.name(), buf, cap, out_needed)
    })
}

/// Writes the dataset's description into `buf`, and whether one is set into
/// `is_present` (an absent description reports 0 bytes needed).
#[no_mangle]
pub extern "C" fn nominal_dataset_description(
    dataset: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
    is_present: *mut bool,
) -> i32 {
    guard(|| {
        let dataset = lookup_handle!(DatasetHandle, dataset);
        write_opt_str_field(dataset.description(), buf, cap, out_needed, is_present)
    })
}

/// Writes the URL for viewing this dataset in the Nominal web app into
/// `buf`, storing the byte count needed in `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_dataset_url(
    dataset: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let dataset = lookup_handle!(DatasetHandle, dataset);
        write_str_field(&dataset.nominal_url(), buf, cap, out_needed)
    })
}

/// Writes the dataset's creation (first-ingest) time to `out_millis` as
/// `f64` Unix milliseconds (UTC).
#[no_mangle]
pub extern "C" fn nominal_dataset_created_at(dataset: i32, out_millis: *mut f64) -> i32 {
    guard(|| {
        let dataset = lookup_handle!(DatasetHandle, dataset);
        if out_millis.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_millis must not be null",
            );
        }
        // SAFETY: out_millis checked non-null above; caller owns it.
        unsafe { *out_millis = dataset.created_at().timestamp_millis() as f64 };
        0
    })
}

/// Stores the number of properties on the dataset in `out_count`.
#[no_mangle]
pub extern "C" fn nominal_dataset_property_count(dataset: i32, out_count: *mut u32) -> i32 {
    guard(|| {
        let dataset = lookup_handle!(DatasetHandle, dataset);
        write_count(dataset.properties().len(), out_count)
    })
}

/// Writes the key of the property at `index` (0-based, sorted-key order)
/// into `buf`, storing the byte count needed in `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_dataset_property_key_at(
    dataset: i32,
    index: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let dataset = lookup_handle!(DatasetHandle, dataset);
        match property_at(&dataset, index) {
            Ok((key, _)) => write_str_field(key, buf, cap, out_needed),
            Err(code) => code,
        }
    })
}

/// Writes the value of the property at `index` (0-based, sorted-key order)
/// into `buf`, storing the byte count needed in `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_dataset_property_value_at(
    dataset: i32,
    index: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let dataset = lookup_handle!(DatasetHandle, dataset);
        match property_at(&dataset, index) {
            Ok((_, value)) => write_str_field(value, buf, cap, out_needed),
            Err(code) => code,
        }
    })
}

/// Stores the number of labels on the dataset in `out_count`.
#[no_mangle]
pub extern "C" fn nominal_dataset_label_count(dataset: i32, out_count: *mut u32) -> i32 {
    guard(|| {
        let dataset = lookup_handle!(DatasetHandle, dataset);
        write_count(dataset.labels().len(), out_count)
    })
}

/// Writes the label at `index` (0-based) into `buf`, storing the byte count
/// needed in `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_dataset_label_at(
    dataset: i32,
    index: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let dataset = lookup_handle!(DatasetHandle, dataset);
        let labels = dataset.labels();
        match usize::try_from(index).ok().and_then(|i| labels.get(i)) {
            Some(label) => write_str_field(label, buf, cap, out_needed),
            None => index_error(index, labels.len()),
        }
    })
}
