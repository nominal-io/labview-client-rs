//! `nominal_asset_*` — mirrors `nominal::core::asset`.
//!
//! Every function takes a client or asset handle and returns an `i32` status
//! code, with results in out-parameters (see `error.rs` module docs).
//!
//! Collection fields (properties, labels, data sources) are exposed as
//! `_count` + `_at(index)` pairs. Properties and data sources live in hash
//! maps upstream, so getters iterate keys in sorted order — indices are
//! stable and `_key_at(i)` / `_value_at(i)` always describe the same entry.

use std::collections::HashMap;
use std::os::raw::c_char;
use std::sync::Mutex;

use nominal::core::{Asset, AssetCreate, AssetQuery, AssetUpdate, DataSource};

use crate::client::ClientHandle;
use crate::error::{fail, fail_sdk, guard, NominalErrorCode};
use crate::handles::{handle_registry, insert_handle_list, lookup_handle};
use crate::runtime::block_on;
use crate::strings::{read_optional_str, read_required_str, write_opt_str_field, write_str_field};

handle_registry!(AssetHandle, Asset);

/// The kind of a data source attached to an asset, as returned by
/// `nominal_asset_data_source_type_at`.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NominalDataSourceType {
    Dataset = 0,
    Video = 1,
    Connection = 2,
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

/// Creates an asset with the given name and (optional, may be null or empty)
/// description, writing the new asset's handle to `out_asset`.
/// Free with `nominal_asset_free`.
#[no_mangle]
pub extern "C" fn nominal_asset_create(
    client: i32,
    name: *const c_char,
    description: *const c_char,
    out_asset: *mut i32,
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
        if out_asset.is_null() {
            return fail(NominalErrorCode::NullArgument, "out_asset must not be null");
        }

        let mut create = AssetCreate::new(name);
        if let Some(description) = description {
            create = create.description(description);
        }

        match block_on(client.assets().create(create)) {
            Ok(asset) => {
                // SAFETY: out_asset checked non-null above; caller owns it.
                unsafe { *out_asset = AssetHandle::insert(asset) };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Fetches the asset with the given RID, writing its handle to `out_asset`.
/// Free with `nominal_asset_free`.
#[no_mangle]
pub extern "C" fn nominal_asset_get(client: i32, rid: *const c_char, out_asset: *mut i32) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let rid = match read_required_str(rid, "rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if out_asset.is_null() {
            return fail(NominalErrorCode::NullArgument, "out_asset must not be null");
        }

        match block_on(client.assets().get(&rid)) {
            Ok(asset) => {
                // SAFETY: out_asset checked non-null above; caller owns it.
                unsafe { *out_asset = AssetHandle::insert(asset) };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Lists all assets (newest first), returning a handle list.
///
/// On success `*out_list` is a handle list of `*out_count` asset handles —
/// read them with `nominal_handle_list_get` and free the list with
/// `nominal_handle_list_free`. Each asset handle stays valid until passed to
/// `nominal_asset_free`, independent of the list.
#[no_mangle]
pub extern "C" fn nominal_asset_list(client: i32, out_list: *mut i32, out_count: *mut u32) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        if out_list.is_null() || out_count.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_list and out_count must not be null",
            );
        }

        match block_on(client.assets().list()) {
            Ok(assets) => {
                let handles: Vec<i32> = assets.into_iter().map(AssetHandle::insert).collect();
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

/// Searches assets, returning a handle list (see `nominal_asset_list` for
/// ownership).
///
/// Filters may each be null or empty ("no filter"); the ones provided are
/// combined with AND:
/// - `search_text`: fuzzy full-text match on name and description
/// - `label`: exact label match
/// - `property_key` + `property_value`: property match (both or neither)
#[no_mangle]
pub extern "C" fn nominal_asset_search(
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
            filters.push(AssetQuery::search_text(text));
        }
        if let Some(label) = label {
            filters.push(AssetQuery::label(label));
        }
        match (property_key, property_value) {
            (Some(key), Some(value)) => filters.push(AssetQuery::property(key, value)),
            (None, None) => {}
            _ => {
                return fail(
                    NominalErrorCode::InvalidArgument,
                    "property_key and property_value must be provided together",
                )
            }
        }
        // No filters at all matches everything — same behavior as list.
        let query = AssetQuery::and(filters);

        match block_on(client.assets().search(query)) {
            Ok(assets) => {
                let handles: Vec<i32> = assets.into_iter().map(AssetHandle::insert).collect();
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

/// Updates an asset's name and/or description. Null or empty arguments leave
/// that field unchanged. Writes a handle to the updated asset to `out_asset`
/// (free with `nominal_asset_free`); handles to the old version stay valid
/// but keep their stale field values.
#[no_mangle]
pub extern "C" fn nominal_asset_update(
    client: i32,
    rid: *const c_char,
    name: *const c_char,
    description: *const c_char,
    out_asset: *mut i32,
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
        if out_asset.is_null() {
            return fail(NominalErrorCode::NullArgument, "out_asset must not be null");
        }
        if name.is_none() && description.is_none() {
            return fail(
                NominalErrorCode::InvalidArgument,
                "at least one of name or description must be provided",
            );
        }

        let mut update = AssetUpdate::new();
        if let Some(name) = name {
            update = update.name(name);
        }
        if let Some(description) = description {
            update = update.description(description);
        }

        match block_on(client.assets().update(&rid, update)) {
            Ok(asset) => {
                // SAFETY: out_asset checked non-null above; caller owns it.
                unsafe { *out_asset = AssetHandle::insert(asset) };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

// ---------------------------------------------------------------------------
// Staging handles for create/update with labels and properties
//
// A C string array (char**) has no LabVIEW representation, so collections
// arrive one string per call: _begin returns a staging handle, _add/_set
// calls accumulate onto it, _commit fires the API request. Commit does NOT
// free the staging handle — every handle is freed explicitly, uniformly.
// ---------------------------------------------------------------------------

/// Accumulated fields for an asset-create request (plain FFI-side struct —
/// the upstream `AssetCreate` builder is constructed only at commit).
pub(crate) struct AssetCreateParams {
    name: String,
    description: Option<String>,
    labels: Vec<String>,
    properties: HashMap<String, String>,
}

handle_registry!(AssetCreateStagingHandle, Mutex<AssetCreateParams>);

/// Starts staging an asset-create request with the (required) name. Add
/// optional fields with the `nominal_asset_create_set_*` / `_add_*` calls,
/// then fire it with `nominal_asset_create_commit`. Free with
/// `nominal_asset_create_free` (commit does not free).
#[no_mangle]
pub extern "C" fn nominal_asset_create_begin(name: *const c_char, out_staging: *mut i32) -> i32 {
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
        let params = AssetCreateParams {
            name,
            description: None,
            labels: Vec::new(),
            properties: HashMap::new(),
        };
        // SAFETY: out_staging checked non-null above; caller owns it.
        unsafe { *out_staging = AssetCreateStagingHandle::insert(Mutex::new(params)) };
        0
    })
}

/// Sets the description on a staged create (overwrites any previous value;
/// empty clears it).
#[no_mangle]
pub extern "C" fn nominal_asset_create_set_description(
    staging: i32,
    description: *const c_char,
) -> i32 {
    guard(|| {
        let staging = lookup_handle!(AssetCreateStagingHandle, staging);
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

/// Adds one label to a staged create (duplicates are collapsed server-side).
#[no_mangle]
pub extern "C" fn nominal_asset_create_add_label(staging: i32, label: *const c_char) -> i32 {
    guard(|| {
        let staging = lookup_handle!(AssetCreateStagingHandle, staging);
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

/// Sets one property on a staged create (same key overwrites).
#[no_mangle]
pub extern "C" fn nominal_asset_create_set_property(
    staging: i32,
    key: *const c_char,
    value: *const c_char,
) -> i32 {
    guard(|| {
        let staging = lookup_handle!(AssetCreateStagingHandle, staging);
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

/// Creates the staged asset, writing the new asset's handle to `out_asset`
/// (free with `nominal_asset_free`). The staging handle stays valid — free it
/// with `nominal_asset_create_free`, or commit it again for another asset.
#[no_mangle]
pub extern "C" fn nominal_asset_create_commit(
    client: i32,
    staging: i32,
    out_asset: *mut i32,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let staging = lookup_handle!(AssetCreateStagingHandle, staging);
        if out_asset.is_null() {
            return fail(NominalErrorCode::NullArgument, "out_asset must not be null");
        }

        let create = {
            let params = staging
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let mut create = AssetCreate::new(params.name.clone());
            if let Some(description) = &params.description {
                create = create.description(description.clone());
            }
            if !params.labels.is_empty() {
                create = create.labels(params.labels.clone());
            }
            if !params.properties.is_empty() {
                create = create.properties(params.properties.clone());
            }
            create
        };

        match block_on(client.assets().create(create)) {
            Ok(asset) => {
                // SAFETY: out_asset checked non-null above; caller owns it.
                unsafe { *out_asset = AssetHandle::insert(asset) };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Frees an asset-create staging handle. Freeing twice returns an error.
#[no_mangle]
pub extern "C" fn nominal_asset_create_free(staging: i32) -> i32 {
    guard(|| {
        if AssetCreateStagingHandle::remove(staging) {
            0
        } else {
            fail(
                NominalErrorCode::InvalidHandle,
                format!("invalid asset-create staging handle: {staging}"),
            )
        }
    })
}

/// Accumulated fields for an asset-update request. `None` = leave that field
/// untouched; `Some` = replace it entirely (upstream semantics).
#[derive(Default)]
pub(crate) struct AssetUpdateParams {
    name: Option<String>,
    description: Option<String>,
    labels: Option<Vec<String>>,
    properties: Option<HashMap<String, String>>,
}

handle_registry!(AssetUpdateStagingHandle, Mutex<AssetUpdateParams>);

/// Starts staging an asset update. Only fields set via the
/// `nominal_asset_update_set_*` / `_add_*` calls are changed at commit; the
/// rest remain untouched. Free with `nominal_asset_update_free`.
#[no_mangle]
pub extern "C" fn nominal_asset_update_begin(out_staging: *mut i32) -> i32 {
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
                AssetUpdateStagingHandle::insert(Mutex::new(AssetUpdateParams::default()))
        };
        0
    })
}

/// Stages a new name.
#[no_mangle]
pub extern "C" fn nominal_asset_update_set_name(staging: i32, name: *const c_char) -> i32 {
    guard(|| {
        let staging = lookup_handle!(AssetUpdateStagingHandle, staging);
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
pub extern "C" fn nominal_asset_update_set_description(
    staging: i32,
    description: *const c_char,
) -> i32 {
    guard(|| {
        let staging = lookup_handle!(AssetUpdateStagingHandle, staging);
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
/// the commit REPLACES the asset's entire label set with exactly the labels
/// accumulated here (upstream semantics) — to keep existing labels, add them
/// too.
#[no_mangle]
pub extern "C" fn nominal_asset_update_add_label(staging: i32, label: *const c_char) -> i32 {
    guard(|| {
        let staging = lookup_handle!(AssetUpdateStagingHandle, staging);
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
/// replaces the asset's entire property map with the ones accumulated here.
#[no_mangle]
pub extern "C" fn nominal_asset_update_set_property(
    staging: i32,
    key: *const c_char,
    value: *const c_char,
) -> i32 {
    guard(|| {
        let staging = lookup_handle!(AssetUpdateStagingHandle, staging);
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

/// Applies the staged update to the asset with the given RID, writing a
/// handle to the updated asset to `out_asset` (free with
/// `nominal_asset_free`). At least one field must have been staged. The
/// staging handle stays valid — free it with `nominal_asset_update_free`.
#[no_mangle]
pub extern "C" fn nominal_asset_update_commit(
    client: i32,
    rid: *const c_char,
    staging: i32,
    out_asset: *mut i32,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let staging = lookup_handle!(AssetUpdateStagingHandle, staging);
        let rid = match read_required_str(rid, "rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if out_asset.is_null() {
            return fail(NominalErrorCode::NullArgument, "out_asset must not be null");
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
            let mut update = AssetUpdate::new();
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

        match block_on(client.assets().update(&rid, update)) {
            Ok(asset) => {
                // SAFETY: out_asset checked non-null above; caller owns it.
                unsafe { *out_asset = AssetHandle::insert(asset) };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Frees an asset-update staging handle. Freeing twice returns an error.
#[no_mangle]
pub extern "C" fn nominal_asset_update_free(staging: i32) -> i32 {
    guard(|| {
        if AssetUpdateStagingHandle::remove(staging) {
            0
        } else {
            fail(
                NominalErrorCode::InvalidHandle,
                format!("invalid asset-update staging handle: {staging}"),
            )
        }
    })
}

/// Archives an asset (hidden from the UI, not deleted).
#[no_mangle]
pub extern "C" fn nominal_asset_archive(client: i32, rid: *const c_char) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let rid = match read_required_str(rid, "rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        match block_on(client.assets().archive(&rid)) {
            Ok(()) => 0,
            Err(err) => fail_sdk(err),
        }
    })
}

/// Unarchives an asset, restoring its visibility in the UI.
#[no_mangle]
pub extern "C" fn nominal_asset_unarchive(client: i32, rid: *const c_char) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let rid = match read_required_str(rid, "rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        match block_on(client.assets().unarchive(&rid)) {
            Ok(()) => 0,
            Err(err) => fail_sdk(err),
        }
    })
}

/// Frees an asset handle. Freeing twice returns an error.
#[no_mangle]
pub extern "C" fn nominal_asset_free(asset: i32) -> i32 {
    guard(|| {
        if AssetHandle::remove(asset) {
            0
        } else {
            fail(
                NominalErrorCode::InvalidHandle,
                format!("invalid asset handle: {asset}"),
            )
        }
    })
}

// ---------------------------------------------------------------------------
// Field getters
// ---------------------------------------------------------------------------

/// Writes the asset's RID into `buf`, storing the byte count needed in
/// `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_asset_rid(
    asset: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let asset = lookup_handle!(AssetHandle, asset);
        write_str_field(asset.rid(), buf, cap, out_needed)
    })
}

/// Writes the asset's name into `buf`, storing the byte count needed in
/// `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_asset_name(
    asset: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let asset = lookup_handle!(AssetHandle, asset);
        write_str_field(asset.name(), buf, cap, out_needed)
    })
}

/// Writes the asset's description into `buf`, and whether one is set into
/// `is_present` (an absent description reports 0 bytes needed).
#[no_mangle]
pub extern "C" fn nominal_asset_description(
    asset: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
    is_present: *mut bool,
) -> i32 {
    guard(|| {
        let asset = lookup_handle!(AssetHandle, asset);
        write_opt_str_field(asset.description(), buf, cap, out_needed, is_present)
    })
}

/// Writes the URL for viewing this asset in the Nominal web app into `buf`,
/// storing the byte count needed in `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_asset_url(
    asset: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let asset = lookup_handle!(AssetHandle, asset);
        write_str_field(&asset.nominal_url(), buf, cap, out_needed)
    })
}

/// Writes the asset's creation time to `out_millis` as Unix milliseconds
/// (UTC), as a `double` — exact for whole milliseconds up to 2^53 (LabVIEW's
/// import wizard cannot parse 64-bit integer types, so `i64` is not an
/// option here).
#[no_mangle]
pub extern "C" fn nominal_asset_created_at(asset: i32, out_millis: *mut f64) -> i32 {
    guard(|| {
        let asset = lookup_handle!(AssetHandle, asset);
        if out_millis.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_millis must not be null",
            );
        }
        // SAFETY: out_millis checked non-null above; caller owns it.
        unsafe { *out_millis = asset.created_at().timestamp_millis() as f64 };
        0
    })
}

/// Writes a collection size to a `*mut u32` out-parameter — shared prologue
/// of every `_count` getter.
fn write_count(count: usize, out_count: *mut u32) -> i32 {
    if out_count.is_null() {
        return fail(NominalErrorCode::NullArgument, "out_count must not be null");
    }
    // SAFETY: out_count checked non-null above; caller owns it.
    unsafe { *out_count = count as u32 };
    0
}

fn index_error(index: i32, len: usize) -> i32 {
    fail(
        NominalErrorCode::IndexOutOfRange,
        format!("index {index} out of range for collection of {len}"),
    )
}

/// Properties in sorted-key order, so `index` means the same entry across the
/// `_count` / `_key_at` / `_value_at` calls.
fn property_at(asset: &Asset, index: i32) -> Result<(&String, &String), i32> {
    let mut keys: Vec<&String> = asset.properties().keys().collect();
    keys.sort();
    let key = keys.get(usize::try_from(index).map_err(|_| index_error(index, keys.len()))?);
    match key {
        Some(&key) => Ok((key, &asset.properties()[key])),
        None => Err(index_error(index, keys.len())),
    }
}

/// Stores the number of properties on the asset in `out_count`.
#[no_mangle]
pub extern "C" fn nominal_asset_property_count(asset: i32, out_count: *mut u32) -> i32 {
    guard(|| {
        let asset = lookup_handle!(AssetHandle, asset);
        write_count(asset.properties().len(), out_count)
    })
}

/// Writes the key of the property at `index` (0-based, sorted-key order) into
/// `buf`, storing the byte count needed in `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_asset_property_key_at(
    asset: i32,
    index: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let asset = lookup_handle!(AssetHandle, asset);
        match property_at(&asset, index) {
            Ok((key, _)) => write_str_field(key, buf, cap, out_needed),
            Err(code) => code,
        }
    })
}

/// Writes the value of the property at `index` (0-based, sorted-key order)
/// into `buf`, storing the byte count needed in `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_asset_property_value_at(
    asset: i32,
    index: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let asset = lookup_handle!(AssetHandle, asset);
        match property_at(&asset, index) {
            Ok((_, value)) => write_str_field(value, buf, cap, out_needed),
            Err(code) => code,
        }
    })
}

/// Stores the number of labels on the asset in `out_count`.
#[no_mangle]
pub extern "C" fn nominal_asset_label_count(asset: i32, out_count: *mut u32) -> i32 {
    guard(|| {
        let asset = lookup_handle!(AssetHandle, asset);
        write_count(asset.labels().len(), out_count)
    })
}

/// Writes the label at `index` (0-based) into `buf`, storing the byte count
/// needed in `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_asset_label_at(
    asset: i32,
    index: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let asset = lookup_handle!(AssetHandle, asset);
        let labels = asset.labels();
        match usize::try_from(index).ok().and_then(|i| labels.get(i)) {
            Some(label) => write_str_field(label, buf, cap, out_needed),
            None => index_error(index, labels.len()),
        }
    })
}

/// Data sources in sorted-scope-name order, so `index` means the same entry
/// across the `_count` / `_name_at` / `_rid_at` / `_type_at` calls.
fn data_source_at(asset: &Asset, index: i32) -> Result<(&String, &DataSource), i32> {
    let mut names: Vec<&String> = asset.data_sources().keys().collect();
    names.sort();
    let name = names.get(usize::try_from(index).map_err(|_| index_error(index, names.len()))?);
    match name {
        Some(&name) => Ok((name, &asset.data_sources()[name])),
        None => Err(index_error(index, names.len())),
    }
}

/// Stores the number of data sources attached to the asset in `out_count`.
#[no_mangle]
pub extern "C" fn nominal_asset_data_source_count(asset: i32, out_count: *mut u32) -> i32 {
    guard(|| {
        let asset = lookup_handle!(AssetHandle, asset);
        write_count(asset.data_sources().len(), out_count)
    })
}

/// Writes the scope name of the data source at `index` (0-based, sorted-name
/// order) into `buf`, storing the byte count needed in `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_asset_data_source_name_at(
    asset: i32,
    index: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let asset = lookup_handle!(AssetHandle, asset);
        match data_source_at(&asset, index) {
            Ok((name, _)) => write_str_field(name, buf, cap, out_needed),
            Err(code) => code,
        }
    })
}

/// Writes the RID of the data source at `index` (0-based, sorted-name order)
/// into `buf`, storing the byte count needed in `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_asset_data_source_rid_at(
    asset: i32,
    index: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let asset = lookup_handle!(AssetHandle, asset);
        match data_source_at(&asset, index) {
            Ok((_, source)) => write_str_field(source.rid(), buf, cap, out_needed),
            Err(code) => code,
        }
    })
}

/// Writes the kind of the data source at `index` (0-based, sorted-name order)
/// to `out_type` as a `NominalDataSourceType` value.
#[no_mangle]
pub extern "C" fn nominal_asset_data_source_type_at(
    asset: i32,
    index: i32,
    out_type: *mut i32,
) -> i32 {
    guard(|| {
        let asset = lookup_handle!(AssetHandle, asset);
        if out_type.is_null() {
            return fail(NominalErrorCode::NullArgument, "out_type must not be null");
        }
        match data_source_at(&asset, index) {
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
