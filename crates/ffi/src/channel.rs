//! `nominal_channel_*` — mirrors the channel operations of
//! `nominal::core::catalog`.
//!
//! Channels are the time-series signals on a data source (dataset, video,
//! connection). Unlike the other resource types they have no
//! create/archive lifecycle — they come into existence through ingest.
//! This module lists/searches them, reads their metadata, and writes the
//! editable metadata (description + unit). All arguments are scalars, so
//! `set_metadata` is a single flat call — no staging handles here.

use std::os::raw::c_char;

use nominal::core::{Channel, ChannelDataType, ChannelQuery, ChannelUpdate};

use crate::client::ClientHandle;
use crate::error::{fail, fail_sdk, guard, NominalErrorCode};
use crate::handles::{handle_registry, insert_handle_list, lookup_handle};
use crate::runtime::block_on;
use crate::strings::{read_optional_str, read_required_str, write_opt_str_field, write_str_field};

handle_registry!(ChannelHandle, Channel);

/// The data type of a channel's values, as reported by
/// `nominal_channel_data_type` and accepted (except `Unknown`) by
/// `nominal_channel_set_metadata` and the search filter.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NominalChannelDataType {
    Double = 0,
    Int = 1,
    Uint = 2,
    String = 3,
    Log = 4,
    DoubleArray = 5,
    StringArray = 6,
    Struct = 7,
    Video = 8,
    Spatial = 9,
    /// Reported for data types this library does not recognize — never valid
    /// as an input.
    Unknown = 10,
}

/// Converts an incoming discriminant to the upstream enum, rejecting values
/// this library can't send (including `Unknown`).
fn data_type_from_i32(value: i32, arg: &str) -> Result<ChannelDataType, i32> {
    Ok(match value {
        0 => ChannelDataType::Double,
        1 => ChannelDataType::Int,
        2 => ChannelDataType::Uint,
        3 => ChannelDataType::String,
        4 => ChannelDataType::Log,
        5 => ChannelDataType::DoubleArray,
        6 => ChannelDataType::StringArray,
        7 => ChannelDataType::Struct,
        8 => ChannelDataType::Video,
        9 => ChannelDataType::Spatial,
        other => {
            return Err(fail(
                NominalErrorCode::InvalidArgument,
                format!("argument '{arg}' is not a valid channel data type: {other}"),
            ))
        }
    })
}

fn data_type_to_i32(data_type: &ChannelDataType) -> i32 {
    (match data_type {
        ChannelDataType::Double => NominalChannelDataType::Double,
        ChannelDataType::Int => NominalChannelDataType::Int,
        ChannelDataType::Uint => NominalChannelDataType::Uint,
        ChannelDataType::String => NominalChannelDataType::String,
        ChannelDataType::Log => NominalChannelDataType::Log,
        ChannelDataType::DoubleArray => NominalChannelDataType::DoubleArray,
        ChannelDataType::StringArray => NominalChannelDataType::StringArray,
        ChannelDataType::Struct => NominalChannelDataType::Struct,
        ChannelDataType::Video => NominalChannelDataType::Video,
        ChannelDataType::Spatial => NominalChannelDataType::Spatial,
        // Exhaustive on purpose: a new upstream variant should fail this
        // build (the drift signal), not get silently mislabeled.
        ChannelDataType::Unknown(_) => NominalChannelDataType::Unknown,
    }) as i32
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

/// Lists every channel on a data source (dataset, video, or connection RID),
/// returning a handle list.
///
/// On success `*out_list` is a handle list of `*out_count` channel handles —
/// read them with `nominal_handle_list_get` and free the list with
/// `nominal_handle_list_free`. Each channel handle stays valid until passed
/// to `nominal_channel_free`, independent of the list.
#[no_mangle]
pub extern "C" fn nominal_channel_list(
    client: i32,
    data_source_rid: *const c_char,
    out_list: *mut i32,
    out_count: *mut u32,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let data_source_rid = match read_required_str(data_source_rid, "data_source_rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if out_list.is_null() || out_count.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_list and out_count must not be null",
            );
        }

        match block_on(client.catalog().list_channels(&data_source_rid)) {
            Ok(channels) => {
                let handles: Vec<i32> = channels.into_iter().map(ChannelHandle::insert).collect();
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

/// Searches channels, returning a handle list (see `nominal_channel_list`
/// for ownership).
///
/// Filters are optional and combined with AND:
/// - `data_source_rid`: restrict to one data source (null or empty = all the
///   caller can see — expect that to be slow on large workspaces)
/// - `substring`: case-insensitive substring the channel name must contain
/// - `data_type`: a `NominalChannelDataType` value, or -1 for no filter
#[no_mangle]
pub extern "C" fn nominal_channel_search(
    client: i32,
    data_source_rid: *const c_char,
    substring: *const c_char,
    data_type: i32,
    out_list: *mut i32,
    out_count: *mut u32,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let data_source_rid = match read_optional_str(data_source_rid, "data_source_rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let substring = match read_optional_str(substring, "substring") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if out_list.is_null() || out_count.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_list and out_count must not be null",
            );
        }

        let mut query = ChannelQuery::new();
        if let Some(rid) = data_source_rid {
            query = query.data_source(rid);
        }
        if let Some(substring) = substring {
            query = query.substring_match(substring);
        }
        if data_type != -1 {
            let data_type = match data_type_from_i32(data_type, "data_type") {
                Ok(value) => value,
                Err(code) => return code,
            };
            query = query.data_type(data_type);
        }

        match block_on(client.catalog().search_channels(query)) {
            Ok(channels) => {
                let handles: Vec<i32> = channels.into_iter().map(ChannelHandle::insert).collect();
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

/// Fetches one channel's metadata by data-source RID and channel name,
/// writing its handle to `out_channel`. Free with `nominal_channel_free`.
#[no_mangle]
pub extern "C" fn nominal_channel_get(
    client: i32,
    data_source_rid: *const c_char,
    name: *const c_char,
    out_channel: *mut i32,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let data_source_rid = match read_required_str(data_source_rid, "data_source_rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let name = match read_required_str(name, "name") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if out_channel.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_channel must not be null",
            );
        }

        match block_on(client.catalog().get_channel(&data_source_rid, &name)) {
            Ok(channel) => {
                // SAFETY: out_channel checked non-null above; caller owns it.
                unsafe { *out_channel = ChannelHandle::insert(channel) };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Sets a channel's editable metadata. `data_type` (a `NominalChannelDataType`
/// value) is required — the upsert endpoint needs it when the metadata record
/// doesn't exist yet. `description` and `unit` are optional (null or empty =
/// leave untouched); pass `clear_unit` true to remove an existing unit
/// (mutually exclusive with `unit`). Writes a handle reflecting the updated
/// metadata to `out_channel` (free with `nominal_channel_free`).
#[no_mangle]
pub extern "C" fn nominal_channel_set_metadata(
    client: i32,
    data_source_rid: *const c_char,
    name: *const c_char,
    data_type: i32,
    description: *const c_char,
    unit: *const c_char,
    clear_unit: bool,
    out_channel: *mut i32,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let data_source_rid = match read_required_str(data_source_rid, "data_source_rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let name = match read_required_str(name, "name") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let data_type = match data_type_from_i32(data_type, "data_type") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let description = match read_optional_str(description, "description") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let unit = match read_optional_str(unit, "unit") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if out_channel.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_channel must not be null",
            );
        }
        if unit.is_some() && clear_unit {
            return fail(
                NominalErrorCode::InvalidArgument,
                "unit and clear_unit are mutually exclusive",
            );
        }

        let mut update = ChannelUpdate::new(data_type);
        if let Some(description) = description {
            update = update.description(description);
        }
        if let Some(unit) = unit {
            update = update.unit(unit);
        }
        if clear_unit {
            update = update.clear_unit();
        }

        match block_on(
            client
                .catalog()
                .set_channel_metadata(&data_source_rid, &name, update),
        ) {
            Ok(channel) => {
                // SAFETY: out_channel checked non-null above; caller owns it.
                unsafe { *out_channel = ChannelHandle::insert(channel) };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Frees a channel handle. Freeing twice returns an error.
#[no_mangle]
pub extern "C" fn nominal_channel_free(channel: i32) -> i32 {
    guard(|| {
        if ChannelHandle::remove(channel) {
            0
        } else {
            fail(
                NominalErrorCode::InvalidHandle,
                format!("invalid channel handle: {channel}"),
            )
        }
    })
}

// ---------------------------------------------------------------------------
// Field getters
// ---------------------------------------------------------------------------

/// Writes the channel's name into `buf`, storing the byte count needed in
/// `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_channel_name(
    channel: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let channel = lookup_handle!(ChannelHandle, channel);
        write_str_field(channel.name(), buf, cap, out_needed)
    })
}

/// Writes the RID of the data source that owns this channel into `buf`,
/// storing the byte count needed in `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_channel_data_source_rid(
    channel: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let channel = lookup_handle!(ChannelHandle, channel);
        write_str_field(channel.data_source_rid(), buf, cap, out_needed)
    })
}

/// Writes the channel's description into `buf`, and whether one is set into
/// `is_present` (an absent description reports 0 bytes needed).
#[no_mangle]
pub extern "C" fn nominal_channel_description(
    channel: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
    is_present: *mut bool,
) -> i32 {
    guard(|| {
        let channel = lookup_handle!(ChannelHandle, channel);
        write_opt_str_field(channel.description(), buf, cap, out_needed, is_present)
    })
}

/// Writes the channel's unit symbol (e.g. "m/s") into `buf`, and whether one
/// is set into `is_present` (an absent unit reports 0 bytes needed).
#[no_mangle]
pub extern "C" fn nominal_channel_unit(
    channel: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
    is_present: *mut bool,
) -> i32 {
    guard(|| {
        let channel = lookup_handle!(ChannelHandle, channel);
        write_opt_str_field(channel.unit(), buf, cap, out_needed, is_present)
    })
}

/// Stores the channel's data type in `out_type` as a
/// `NominalChannelDataType` value (`Unknown` = 10 for types this library
/// doesn't recognize).
#[no_mangle]
pub extern "C" fn nominal_channel_data_type(channel: i32, out_type: *mut i32) -> i32 {
    guard(|| {
        let channel = lookup_handle!(ChannelHandle, channel);
        if out_type.is_null() {
            return fail(NominalErrorCode::NullArgument, "out_type must not be null");
        }
        // SAFETY: out_type checked non-null above; caller owns it.
        unsafe { *out_type = data_type_to_i32(channel.data_type()) };
        0
    })
}
