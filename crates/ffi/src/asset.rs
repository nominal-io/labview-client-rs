//! `nominal_asset_*` — mirrors `nominal::core::asset`.
//!
//! Action functions (create/get/list/search/update/archive/unarchive) take a
//! client handle and return `i32` codes; field getters take an asset handle
//! and follow the string/count getter convention (see `error.rs` module docs).
//!
//! Collection fields (properties, labels, data sources) are exposed as
//! `_count` + `_at(index)` pairs. Properties and data sources live in hash
//! maps upstream, so getters iterate keys in sorted order — indices are
//! stable and `_key_at(i)` / `_value_at(i)` always describe the same entry.

use std::os::raw::c_char;

use nominal::core::{Asset, AssetCreate, AssetQuery, AssetUpdate, DataSource};

use crate::client::ClientHandle;
use crate::error::{fail, fail_i64, fail_sdk, guard, guard_i64, NominalErrorCode};
use crate::handles::{handle_registry, handles_into_raw, lookup_handle};
use crate::runtime::block_on;
use crate::strings::{read_optional_str, read_required_str, write_opt_str_out, write_str_out};

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
// Action functions (i32 return)
// ---------------------------------------------------------------------------

/// Creates an asset with the given name and (optional, may be null or empty)
/// description, writing the new asset's handle to `out_asset`.
/// Free with `nominal_asset_free`.
#[no_mangle]
pub extern "C" fn nominal_asset_create(
    client: i64,
    name: *const c_char,
    description: *const c_char,
    out_asset: *mut i64,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client, NominalErrorCode::InvalidHandle as i32);
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
pub extern "C" fn nominal_asset_get(client: i64, rid: *const c_char, out_asset: *mut i64) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client, NominalErrorCode::InvalidHandle as i32);
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

/// Lists all assets (newest first), returning an array of new asset handles.
///
/// On success `*out_assets` points to an array of `*out_count` handles. Free
/// the array with `nominal_handle_array_free`; free each handle in it with
/// `nominal_asset_free`.
#[no_mangle]
pub extern "C" fn nominal_asset_list(
    client: i64,
    out_assets: *mut *mut i64,
    out_count: *mut usize,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client, NominalErrorCode::InvalidHandle as i32);
        if out_assets.is_null() || out_count.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_assets and out_count must not be null",
            );
        }

        match block_on(client.assets().list()) {
            Ok(assets) => {
                let handles = assets.into_iter().map(AssetHandle::insert).collect();
                let (ptr, count) = handles_into_raw(handles);
                // SAFETY: out pointers checked non-null above; caller owns them.
                unsafe {
                    *out_assets = ptr;
                    *out_count = count;
                }
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Searches assets, returning an array of new asset handles (see
/// `nominal_asset_list` for ownership).
///
/// Filters may each be null or empty ("no filter"); the ones provided are
/// combined with AND:
/// - `search_text`: fuzzy full-text match on name and description
/// - `label`: exact label match
/// - `property_key` + `property_value`: property match (both or neither)
#[no_mangle]
pub extern "C" fn nominal_asset_search(
    client: i64,
    search_text: *const c_char,
    label: *const c_char,
    property_key: *const c_char,
    property_value: *const c_char,
    out_assets: *mut *mut i64,
    out_count: *mut usize,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client, NominalErrorCode::InvalidHandle as i32);
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
        if out_assets.is_null() || out_count.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_assets and out_count must not be null",
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
                let handles = assets.into_iter().map(AssetHandle::insert).collect();
                let (ptr, count) = handles_into_raw(handles);
                // SAFETY: out pointers checked non-null above; caller owns them.
                unsafe {
                    *out_assets = ptr;
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
    client: i64,
    rid: *const c_char,
    name: *const c_char,
    description: *const c_char,
    out_asset: *mut i64,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client, NominalErrorCode::InvalidHandle as i32);
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

/// Archives an asset (hidden from the UI, not deleted).
#[no_mangle]
pub extern "C" fn nominal_asset_archive(client: i64, rid: *const c_char) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client, NominalErrorCode::InvalidHandle as i32);
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
pub extern "C" fn nominal_asset_unarchive(client: i64, rid: *const c_char) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client, NominalErrorCode::InvalidHandle as i32);
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
pub extern "C" fn nominal_asset_free(asset: i64) -> i32 {
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
// Field getters (i64 return: bytes/count needed, or negative error code)
// ---------------------------------------------------------------------------

const INVALID_HANDLE: i64 = -(NominalErrorCode::InvalidHandle as i64);

/// Writes the asset's RID into `buf`.
#[no_mangle]
pub extern "C" fn nominal_asset_rid(asset: i64, buf: *mut c_char, cap: usize) -> i64 {
    guard_i64(|| {
        let asset = lookup_handle!(AssetHandle, asset, INVALID_HANDLE);
        write_str_out(asset.rid(), buf, cap)
    })
}

/// Writes the asset's name into `buf`.
#[no_mangle]
pub extern "C" fn nominal_asset_name(asset: i64, buf: *mut c_char, cap: usize) -> i64 {
    guard_i64(|| {
        let asset = lookup_handle!(AssetHandle, asset, INVALID_HANDLE);
        write_str_out(asset.name(), buf, cap)
    })
}

/// Writes the asset's description into `buf`, and whether one is set into
/// `is_present` (an absent description reports 0 bytes needed).
#[no_mangle]
pub extern "C" fn nominal_asset_description(
    asset: i64,
    buf: *mut c_char,
    cap: usize,
    is_present: *mut bool,
) -> i64 {
    guard_i64(|| {
        let asset = lookup_handle!(AssetHandle, asset, INVALID_HANDLE);
        write_opt_str_out(asset.description(), buf, cap, is_present)
    })
}

/// Writes the URL for viewing this asset in the Nominal web app into `buf`.
#[no_mangle]
pub extern "C" fn nominal_asset_url(asset: i64, buf: *mut c_char, cap: usize) -> i64 {
    guard_i64(|| {
        let asset = lookup_handle!(AssetHandle, asset, INVALID_HANDLE);
        write_str_out(&asset.nominal_url(), buf, cap)
    })
}

/// Writes the asset's creation time to `out_millis` as Unix milliseconds (UTC).
#[no_mangle]
pub extern "C" fn nominal_asset_created_at(asset: i64, out_millis: *mut i64) -> i32 {
    guard(|| {
        let asset = lookup_handle!(AssetHandle, asset, NominalErrorCode::InvalidHandle as i32);
        if out_millis.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_millis must not be null",
            );
        }
        // SAFETY: out_millis checked non-null above; caller owns it.
        unsafe { *out_millis = asset.created_at().timestamp_millis() };
        0
    })
}

/// Properties in sorted-key order, so `index` means the same entry across the
/// `_count` / `_key_at` / `_value_at` calls.
fn property_at(asset: &Asset, index: i64) -> Result<(&String, &String), i64> {
    let mut keys: Vec<&String> = asset.properties().keys().collect();
    keys.sort();
    let key = keys.get(usize::try_from(index).map_err(|_| index_error(index, keys.len()))?);
    match key {
        Some(&key) => Ok((key, &asset.properties()[key])),
        None => Err(index_error(index, keys.len())),
    }
}

fn index_error(index: i64, len: usize) -> i64 {
    fail_i64(
        NominalErrorCode::IndexOutOfRange,
        format!("index {index} out of range for collection of {len}"),
    )
}

/// Returns the number of properties on the asset.
#[no_mangle]
pub extern "C" fn nominal_asset_property_count(asset: i64) -> i64 {
    guard_i64(|| {
        let asset = lookup_handle!(AssetHandle, asset, INVALID_HANDLE);
        asset.properties().len() as i64
    })
}

/// Writes the key of the property at `index` (0-based, sorted-key order) into `buf`.
#[no_mangle]
pub extern "C" fn nominal_asset_property_key_at(
    asset: i64,
    index: i64,
    buf: *mut c_char,
    cap: usize,
) -> i64 {
    guard_i64(|| {
        let asset = lookup_handle!(AssetHandle, asset, INVALID_HANDLE);
        match property_at(&asset, index) {
            Ok((key, _)) => write_str_out(key, buf, cap),
            Err(code) => code,
        }
    })
}

/// Writes the value of the property at `index` (0-based, sorted-key order) into `buf`.
#[no_mangle]
pub extern "C" fn nominal_asset_property_value_at(
    asset: i64,
    index: i64,
    buf: *mut c_char,
    cap: usize,
) -> i64 {
    guard_i64(|| {
        let asset = lookup_handle!(AssetHandle, asset, INVALID_HANDLE);
        match property_at(&asset, index) {
            Ok((_, value)) => write_str_out(value, buf, cap),
            Err(code) => code,
        }
    })
}

/// Returns the number of labels on the asset.
#[no_mangle]
pub extern "C" fn nominal_asset_label_count(asset: i64) -> i64 {
    guard_i64(|| {
        let asset = lookup_handle!(AssetHandle, asset, INVALID_HANDLE);
        asset.labels().len() as i64
    })
}

/// Writes the label at `index` (0-based) into `buf`.
#[no_mangle]
pub extern "C" fn nominal_asset_label_at(
    asset: i64,
    index: i64,
    buf: *mut c_char,
    cap: usize,
) -> i64 {
    guard_i64(|| {
        let asset = lookup_handle!(AssetHandle, asset, INVALID_HANDLE);
        let labels = asset.labels();
        match usize::try_from(index).ok().and_then(|i| labels.get(i)) {
            Some(label) => write_str_out(label, buf, cap),
            None => index_error(index, labels.len()),
        }
    })
}

/// Data sources in sorted-scope-name order, so `index` means the same entry
/// across the `_count` / `_name_at` / `_rid_at` / `_type_at` calls.
fn data_source_at(asset: &Asset, index: i64) -> Result<(&String, &DataSource), i64> {
    let mut names: Vec<&String> = asset.data_sources().keys().collect();
    names.sort();
    let name = names.get(usize::try_from(index).map_err(|_| index_error(index, names.len()))?);
    match name {
        Some(&name) => Ok((name, &asset.data_sources()[name])),
        None => Err(index_error(index, names.len())),
    }
}

/// Returns the number of data sources attached to the asset.
#[no_mangle]
pub extern "C" fn nominal_asset_data_source_count(asset: i64) -> i64 {
    guard_i64(|| {
        let asset = lookup_handle!(AssetHandle, asset, INVALID_HANDLE);
        asset.data_sources().len() as i64
    })
}

/// Writes the scope name of the data source at `index` (0-based, sorted-name
/// order) into `buf`.
#[no_mangle]
pub extern "C" fn nominal_asset_data_source_name_at(
    asset: i64,
    index: i64,
    buf: *mut c_char,
    cap: usize,
) -> i64 {
    guard_i64(|| {
        let asset = lookup_handle!(AssetHandle, asset, INVALID_HANDLE);
        match data_source_at(&asset, index) {
            Ok((name, _)) => write_str_out(name, buf, cap),
            Err(code) => code,
        }
    })
}

/// Writes the RID of the data source at `index` (0-based, sorted-name order)
/// into `buf`.
#[no_mangle]
pub extern "C" fn nominal_asset_data_source_rid_at(
    asset: i64,
    index: i64,
    buf: *mut c_char,
    cap: usize,
) -> i64 {
    guard_i64(|| {
        let asset = lookup_handle!(AssetHandle, asset, INVALID_HANDLE);
        match data_source_at(&asset, index) {
            Ok((_, source)) => write_str_out(source.rid(), buf, cap),
            Err(code) => code,
        }
    })
}

/// Writes the kind of the data source at `index` (0-based, sorted-name order)
/// to `out_type` as a `NominalDataSourceType` value.
#[no_mangle]
pub extern "C" fn nominal_asset_data_source_type_at(
    asset: i64,
    index: i64,
    out_type: *mut i32,
) -> i32 {
    guard(|| {
        let asset = lookup_handle!(AssetHandle, asset, NominalErrorCode::InvalidHandle as i32);
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
            Err(_) => NominalErrorCode::IndexOutOfRange as i32,
        }
    })
}
