//! `nominal_video_*` — mirrors the video operations of
//! `nominal::core::catalog`.
//!
//! Same shape as `dataset.rs` (videos and datasets share the catalog client
//! and field surface): actions return `i32` status codes with results in
//! out-parameters; collections are `_count` + `_at(index)` pairs;
//! create/update with labels or properties go through staging handles (see
//! the CLAUDE.md cookbook). Video content itself arrives via ingest,
//! separately — creating a video here makes an empty metadata shell.

use std::collections::HashMap;
use std::os::raw::c_char;
use std::sync::Mutex;

use nominal::core::{Video, VideoCreate, VideoQuery, VideoUpdate};

use crate::client::ClientHandle;
use crate::error::{fail, fail_sdk, guard, NominalErrorCode};
use crate::handles::{handle_registry, insert_handle_list, lookup_handle};
use crate::runtime::block_on;
use crate::strings::{read_optional_str, read_required_str, write_opt_str_field, write_str_field};

handle_registry!(VideoHandle, Video);

fn index_error(index: i32, len: usize) -> i32 {
    fail(
        NominalErrorCode::IndexOutOfRange,
        format!("index {index} out of range for collection of {len}"),
    )
}

/// Properties in sorted-key order, so `index` means the same entry across the
/// `_count` / `_key_at` / `_value_at` calls.
fn property_at(video: &Video, index: i32) -> Result<(&String, &String), i32> {
    let mut keys: Vec<&String> = video.properties().keys().collect();
    keys.sort();
    let key = keys.get(usize::try_from(index).map_err(|_| index_error(index, keys.len()))?);
    match key {
        Some(&key) => Ok((key, &video.properties()[key])),
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

/// Creates an empty video (metadata shell — the media file arrives via
/// ingest) with the given name and optional description, writing the new
/// video's handle to `out_video`. Free with `nominal_video_free`. For labels
/// or properties use the `nominal_video_create_begin` staging flow instead.
#[no_mangle]
pub extern "C" fn nominal_video_create(
    client: i32,
    name: *const c_char,
    description: *const c_char,
    out_video: *mut i32,
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
        if out_video.is_null() {
            return fail(NominalErrorCode::NullArgument, "out_video must not be null");
        }

        let mut create = VideoCreate::new(name);
        if let Some(description) = description {
            create = create.description(description);
        }

        match block_on(client.catalog().create_video(create)) {
            Ok(video) => {
                // SAFETY: out_video checked non-null above; caller owns it.
                unsafe { *out_video = VideoHandle::insert(video) };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Fetches the video with the given RID, writing its handle to `out_video`.
/// Free with `nominal_video_free`.
#[no_mangle]
pub extern "C" fn nominal_video_get(client: i32, rid: *const c_char, out_video: *mut i32) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let rid = match read_required_str(rid, "rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if out_video.is_null() {
            return fail(NominalErrorCode::NullArgument, "out_video must not be null");
        }

        match block_on(client.catalog().get_video(&rid)) {
            Ok(video) => {
                // SAFETY: out_video checked non-null above; caller owns it.
                unsafe { *out_video = VideoHandle::insert(video) };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Lists all videos (newest first), returning a handle list.
///
/// On success `*out_list` is a handle list of `*out_count` video handles —
/// read them with `nominal_handle_list_get` and free the list with
/// `nominal_handle_list_free`. Each video handle stays valid until passed to
/// `nominal_video_free`, independent of the list.
#[no_mangle]
pub extern "C" fn nominal_video_list(client: i32, out_list: *mut i32, out_count: *mut u32) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        if out_list.is_null() || out_count.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_list and out_count must not be null",
            );
        }

        match block_on(client.catalog().list_videos()) {
            Ok(videos) => {
                let handles: Vec<i32> = videos.into_iter().map(VideoHandle::insert).collect();
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

/// Searches videos, returning a handle list (see `nominal_video_list` for
/// ownership).
///
/// Filters may each be null or empty ("no filter"); the ones provided are
/// combined with AND:
/// - `search_text`: fuzzy full-text match on name and description
/// - `label`: exact label match
/// - `property_key` + `property_value`: property match (both or neither)
#[no_mangle]
pub extern "C" fn nominal_video_search(
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
            filters.push(VideoQuery::search_text(text));
        }
        if let Some(label) = label {
            filters.push(VideoQuery::label(label));
        }
        match (property_key, property_value) {
            (Some(key), Some(value)) => filters.push(VideoQuery::property(key, value)),
            (None, None) => {}
            _ => {
                return fail(
                    NominalErrorCode::InvalidArgument,
                    "property_key and property_value must be provided together",
                )
            }
        }
        // No filters at all matches everything — same behavior as list.
        let query = VideoQuery::and(filters);

        match block_on(client.catalog().search_videos(query)) {
            Ok(videos) => {
                let handles: Vec<i32> = videos.into_iter().map(VideoHandle::insert).collect();
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

/// Updates a video's name and/or description. Null or empty arguments leave
/// that field unchanged. Writes a handle to the updated video to `out_video`
/// (free with `nominal_video_free`). For labels or properties use the
/// `nominal_video_update_begin` staging flow instead.
#[no_mangle]
pub extern "C" fn nominal_video_update(
    client: i32,
    rid: *const c_char,
    name: *const c_char,
    description: *const c_char,
    out_video: *mut i32,
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
        if out_video.is_null() {
            return fail(NominalErrorCode::NullArgument, "out_video must not be null");
        }
        if name.is_none() && description.is_none() {
            return fail(
                NominalErrorCode::InvalidArgument,
                "at least one of name or description must be provided",
            );
        }

        let mut update = VideoUpdate::new();
        if let Some(name) = name {
            update = update.name(name);
        }
        if let Some(description) = description {
            update = update.description(description);
        }

        match block_on(client.catalog().update_video(&rid, update)) {
            Ok(video) => {
                // SAFETY: out_video checked non-null above; caller owns it.
                unsafe { *out_video = VideoHandle::insert(video) };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Archives a video (hidden from the UI, not deleted).
#[no_mangle]
pub extern "C" fn nominal_video_archive(client: i32, rid: *const c_char) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let rid = match read_required_str(rid, "rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        match block_on(client.catalog().archive_video(&rid)) {
            Ok(()) => 0,
            Err(err) => fail_sdk(err),
        }
    })
}

/// Unarchives a video, restoring its visibility in the UI.
#[no_mangle]
pub extern "C" fn nominal_video_unarchive(client: i32, rid: *const c_char) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let rid = match read_required_str(rid, "rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        match block_on(client.catalog().unarchive_video(&rid)) {
            Ok(()) => 0,
            Err(err) => fail_sdk(err),
        }
    })
}

/// Frees a video handle. Freeing twice returns an error.
#[no_mangle]
pub extern "C" fn nominal_video_free(video: i32) -> i32 {
    guard(|| {
        if VideoHandle::remove(video) {
            0
        } else {
            fail(
                NominalErrorCode::InvalidHandle,
                format!("invalid video handle: {video}"),
            )
        }
    })
}

// ---------------------------------------------------------------------------
// Staging handles for create/update with labels and properties
// ---------------------------------------------------------------------------

/// Accumulated fields for a video-create request.
pub(crate) struct VideoCreateParams {
    name: String,
    description: Option<String>,
    labels: Vec<String>,
    properties: HashMap<String, String>,
}

impl VideoCreateParams {
    /// Builds the upstream request from the accumulated fields. Shared by
    /// `nominal_video_create_commit` and the `nominal_ingest_video*_new`
    /// functions (which create the video atomically with an ingest).
    pub(crate) fn to_create(&self) -> VideoCreate {
        let mut create = VideoCreate::new(self.name.clone());
        if let Some(description) = &self.description {
            create = create.description(description.clone());
        }
        if !self.labels.is_empty() {
            create = create.labels(self.labels.clone());
        }
        if !self.properties.is_empty() {
            create = create.properties(self.properties.clone());
        }
        create
    }
}

handle_registry!(VideoCreateStagingHandle, Mutex<VideoCreateParams>);

/// Starts staging a video-create request with the (required) name. Add
/// optional fields with the `nominal_video_create_set_*` / `_add_*` calls,
/// then fire it with `nominal_video_create_commit`. Free with
/// `nominal_video_create_free` (commit does not free).
#[no_mangle]
pub extern "C" fn nominal_video_create_begin(name: *const c_char, out_staging: *mut i32) -> i32 {
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
        let params = VideoCreateParams {
            name,
            description: None,
            labels: Vec::new(),
            properties: HashMap::new(),
        };
        // SAFETY: out_staging checked non-null above; caller owns it.
        unsafe { *out_staging = VideoCreateStagingHandle::insert(Mutex::new(params)) };
        0
    })
}

/// Sets the description on a staged video create (empty clears it).
#[no_mangle]
pub extern "C" fn nominal_video_create_set_description(
    staging: i32,
    description: *const c_char,
) -> i32 {
    guard(|| {
        let staging = lookup_handle!(VideoCreateStagingHandle, staging);
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

/// Adds one label to a staged video create.
#[no_mangle]
pub extern "C" fn nominal_video_create_add_label(staging: i32, label: *const c_char) -> i32 {
    guard(|| {
        let staging = lookup_handle!(VideoCreateStagingHandle, staging);
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

/// Sets one property on a staged video create (same key overwrites).
#[no_mangle]
pub extern "C" fn nominal_video_create_set_property(
    staging: i32,
    key: *const c_char,
    value: *const c_char,
) -> i32 {
    guard(|| {
        let staging = lookup_handle!(VideoCreateStagingHandle, staging);
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

/// Creates the staged video, writing the new video's handle to `out_video`
/// (free with `nominal_video_free`). The staging handle stays valid — free
/// it with `nominal_video_create_free`.
#[no_mangle]
pub extern "C" fn nominal_video_create_commit(
    client: i32,
    staging: i32,
    out_video: *mut i32,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let staging = lookup_handle!(VideoCreateStagingHandle, staging);
        if out_video.is_null() {
            return fail(NominalErrorCode::NullArgument, "out_video must not be null");
        }

        let create = staging
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .to_create();

        match block_on(client.catalog().create_video(create)) {
            Ok(video) => {
                // SAFETY: out_video checked non-null above; caller owns it.
                unsafe { *out_video = VideoHandle::insert(video) };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Frees a video-create staging handle. Freeing twice returns an error.
#[no_mangle]
pub extern "C" fn nominal_video_create_free(staging: i32) -> i32 {
    guard(|| {
        if VideoCreateStagingHandle::remove(staging) {
            0
        } else {
            fail(
                NominalErrorCode::InvalidHandle,
                format!("invalid video-create staging handle: {staging}"),
            )
        }
    })
}

/// Accumulated fields for a video-update request. `None` = leave that field
/// untouched; `Some` = replace it entirely (upstream semantics).
#[derive(Default)]
pub(crate) struct VideoUpdateParams {
    name: Option<String>,
    description: Option<String>,
    labels: Option<Vec<String>>,
    properties: Option<HashMap<String, String>>,
}

handle_registry!(VideoUpdateStagingHandle, Mutex<VideoUpdateParams>);

/// Starts staging a video update. Only fields set via the
/// `nominal_video_update_set_*` / `_add_*` calls are changed at commit; the
/// rest remain untouched. Free with `nominal_video_update_free`.
#[no_mangle]
pub extern "C" fn nominal_video_update_begin(out_staging: *mut i32) -> i32 {
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
                VideoUpdateStagingHandle::insert(Mutex::new(VideoUpdateParams::default()))
        };
        0
    })
}

/// Stages a new name.
#[no_mangle]
pub extern "C" fn nominal_video_update_set_name(staging: i32, name: *const c_char) -> i32 {
    guard(|| {
        let staging = lookup_handle!(VideoUpdateStagingHandle, staging);
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
pub extern "C" fn nominal_video_update_set_description(
    staging: i32,
    description: *const c_char,
) -> i32 {
    guard(|| {
        let staging = lookup_handle!(VideoUpdateStagingHandle, staging);
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
/// the commit REPLACES the video's entire label set with exactly the labels
/// accumulated here (upstream semantics) — to keep existing labels, add them
/// too.
#[no_mangle]
pub extern "C" fn nominal_video_update_add_label(staging: i32, label: *const c_char) -> i32 {
    guard(|| {
        let staging = lookup_handle!(VideoUpdateStagingHandle, staging);
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
/// replaces the video's entire property map with the ones accumulated here.
#[no_mangle]
pub extern "C" fn nominal_video_update_set_property(
    staging: i32,
    key: *const c_char,
    value: *const c_char,
) -> i32 {
    guard(|| {
        let staging = lookup_handle!(VideoUpdateStagingHandle, staging);
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

/// Applies the staged update to the video with the given RID, writing a
/// handle to the updated video to `out_video` (free with
/// `nominal_video_free`). At least one field must have been staged. The
/// staging handle stays valid — free it with `nominal_video_update_free`.
#[no_mangle]
pub extern "C" fn nominal_video_update_commit(
    client: i32,
    rid: *const c_char,
    staging: i32,
    out_video: *mut i32,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let staging = lookup_handle!(VideoUpdateStagingHandle, staging);
        let rid = match read_required_str(rid, "rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if out_video.is_null() {
            return fail(NominalErrorCode::NullArgument, "out_video must not be null");
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
            let mut update = VideoUpdate::new();
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

        match block_on(client.catalog().update_video(&rid, update)) {
            Ok(video) => {
                // SAFETY: out_video checked non-null above; caller owns it.
                unsafe { *out_video = VideoHandle::insert(video) };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Frees a video-update staging handle. Freeing twice returns an error.
#[no_mangle]
pub extern "C" fn nominal_video_update_free(staging: i32) -> i32 {
    guard(|| {
        if VideoUpdateStagingHandle::remove(staging) {
            0
        } else {
            fail(
                NominalErrorCode::InvalidHandle,
                format!("invalid video-update staging handle: {staging}"),
            )
        }
    })
}

// ---------------------------------------------------------------------------
// Field getters
// ---------------------------------------------------------------------------

/// Writes the video's RID into `buf`, storing the byte count needed in
/// `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_video_rid(
    video: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let video = lookup_handle!(VideoHandle, video);
        write_str_field(video.rid(), buf, cap, out_needed)
    })
}

/// Writes the video's name into `buf`, storing the byte count needed in
/// `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_video_name(
    video: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let video = lookup_handle!(VideoHandle, video);
        write_str_field(video.name(), buf, cap, out_needed)
    })
}

/// Writes the video's description into `buf`, and whether one is set into
/// `is_present` (an absent description reports 0 bytes needed).
#[no_mangle]
pub extern "C" fn nominal_video_description(
    video: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
    is_present: *mut bool,
) -> i32 {
    guard(|| {
        let video = lookup_handle!(VideoHandle, video);
        write_opt_str_field(video.description(), buf, cap, out_needed, is_present)
    })
}

/// Writes the URL for viewing this video in the Nominal web app into `buf`,
/// storing the byte count needed in `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_video_url(
    video: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let video = lookup_handle!(VideoHandle, video);
        write_str_field(&video.nominal_url(), buf, cap, out_needed)
    })
}

/// Writes the video's creation time to `out_millis` as `f64` Unix
/// milliseconds (UTC).
#[no_mangle]
pub extern "C" fn nominal_video_created_at(video: i32, out_millis: *mut f64) -> i32 {
    guard(|| {
        let video = lookup_handle!(VideoHandle, video);
        if out_millis.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_millis must not be null",
            );
        }
        // SAFETY: out_millis checked non-null above; caller owns it.
        unsafe { *out_millis = video.created_at().timestamp_millis() as f64 };
        0
    })
}

/// Stores the number of properties on the video in `out_count`.
#[no_mangle]
pub extern "C" fn nominal_video_property_count(video: i32, out_count: *mut u32) -> i32 {
    guard(|| {
        let video = lookup_handle!(VideoHandle, video);
        write_count(video.properties().len(), out_count)
    })
}

/// Writes the key of the property at `index` (0-based, sorted-key order)
/// into `buf`, storing the byte count needed in `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_video_property_key_at(
    video: i32,
    index: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let video = lookup_handle!(VideoHandle, video);
        match property_at(&video, index) {
            Ok((key, _)) => write_str_field(key, buf, cap, out_needed),
            Err(code) => code,
        }
    })
}

/// Writes the value of the property at `index` (0-based, sorted-key order)
/// into `buf`, storing the byte count needed in `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_video_property_value_at(
    video: i32,
    index: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let video = lookup_handle!(VideoHandle, video);
        match property_at(&video, index) {
            Ok((_, value)) => write_str_field(value, buf, cap, out_needed),
            Err(code) => code,
        }
    })
}

/// Stores the number of labels on the video in `out_count`.
#[no_mangle]
pub extern "C" fn nominal_video_label_count(video: i32, out_count: *mut u32) -> i32 {
    guard(|| {
        let video = lookup_handle!(VideoHandle, video);
        write_count(video.labels().len(), out_count)
    })
}

/// Writes the label at `index` (0-based) into `buf`, storing the byte count
/// needed in `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_video_label_at(
    video: i32,
    index: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let video = lookup_handle!(VideoHandle, video);
        let labels = video.labels();
        match usize::try_from(index).ok().and_then(|i| labels.get(i)) {
            Some(label) => write_str_field(label, buf, cap, out_needed),
            None => index_error(index, labels.len()),
        }
    })
}
