//! `nominal_ingest_*` — mirrors `nominal::core::ingest` for tabular files.
//!
//! This module covers the workhorse path: upload a CSV or Parquet file from
//! disk into an existing dataset, then track the server-side ingest job.
//! (The other upstream formats — MCAP, video, DataFlash, Avro-stream,
//! journald JSON — follow the same pattern and are not wired up yet.)
//!
//! CSV and Parquet share one option surface, so they share one staging
//! handle: `nominal_ingest_tabular_begin`, the `_set_*`/`_add_*` calls, then
//! either `nominal_ingest_csv` or `nominal_ingest_parquet` to upload+ingest.
//! A timestamp spec (which column holds time, and how it's encoded) is
//! required before committing.
//!
//! Uploads block the calling thread for the duration (multipart upload with
//! sane defaults: 64 MiB parts, 8 concurrent, 3 retries). The upstream
//! progress callback is not exposed — C function pointers don't cross this
//! boundary cleanly. Poll the returned job with `nominal_ingest_job_get`, or
//! block until it finishes with `nominal_ingest_job_wait`.

use std::collections::{BTreeMap, BTreeSet};
use std::os::raw::c_char;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use nominal::core::{CsvIngest, IngestJob, IngestJobStatus, ParquetIngest, TimeUnit, Timestamp};

use crate::client::ClientHandle;
use crate::error::{fail, fail_sdk, guard, NominalErrorCode};
use crate::handles::{handle_registry, lookup_handle};
use crate::runtime::block_on;
use crate::strings::{read_required_str, write_opt_str_field, write_str_field};

/// The lifecycle status of an ingest job, as reported by
/// `nominal_ingest_job_status`. Completed/Failed/Cancelled/Unknown are
/// terminal.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NominalIngestJobStatus {
    Submitted = 0,
    Queued = 1,
    InProgress = 2,
    Completed = 3,
    Failed = 4,
    Cancelled = 5,
    /// A status this library does not recognize (treated as terminal).
    Unknown = 6,
}

fn status_to_i32(status: &IngestJobStatus) -> i32 {
    (match status {
        IngestJobStatus::Submitted => NominalIngestJobStatus::Submitted,
        IngestJobStatus::Queued => NominalIngestJobStatus::Queued,
        IngestJobStatus::InProgress => NominalIngestJobStatus::InProgress,
        IngestJobStatus::Completed => NominalIngestJobStatus::Completed,
        IngestJobStatus::Failed => NominalIngestJobStatus::Failed,
        IngestJobStatus::Cancelled => NominalIngestJobStatus::Cancelled,
        // Exhaustive on purpose: a new upstream variant should fail this
        // build (the drift signal), not get silently mislabeled.
        IngestJobStatus::Unknown(_) => NominalIngestJobStatus::Unknown,
    }) as i32
}

/// The time unit for numeric timestamps in a file, accepted by the
/// `nominal_ingest_tabular_set_timestamp_epoch`/`_relative` calls.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NominalTimeUnit {
    Nanoseconds = 0,
    Microseconds = 1,
    Milliseconds = 2,
    Seconds = 3,
    Minutes = 4,
    Hours = 5,
    Days = 6,
}

fn time_unit_from_i32(value: i32, arg: &str) -> Result<TimeUnit, i32> {
    Ok(match value {
        0 => TimeUnit::Nanoseconds,
        1 => TimeUnit::Microseconds,
        2 => TimeUnit::Milliseconds,
        3 => TimeUnit::Seconds,
        4 => TimeUnit::Minutes,
        5 => TimeUnit::Hours,
        6 => TimeUnit::Days,
        other => {
            return Err(fail(
                NominalErrorCode::InvalidArgument,
                format!("argument '{arg}' is not a valid time unit: {other}"),
            ))
        }
    })
}

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

// ---------------------------------------------------------------------------
// Tabular ingest staging (shared by CSV and Parquet)
// ---------------------------------------------------------------------------

/// Accumulated options for a tabular (CSV/Parquet) ingest.
pub(crate) struct TabularIngestParams {
    timestamp: Option<Timestamp>,
    channel_prefix: Option<String>,
    tag_columns: BTreeMap<String, String>,
    file_tags: BTreeMap<String, String>,
    exclude_columns: BTreeSet<String>,
    is_archive: Option<bool>,
}

handle_registry!(TabularIngestStagingHandle, Mutex<TabularIngestParams>);

/// Starts staging options for a CSV or Parquet ingest. Set the (required)
/// timestamp spec with one of the `nominal_ingest_tabular_set_timestamp_*`
/// calls, add optional settings, then fire it with `nominal_ingest_csv` or
/// `nominal_ingest_parquet`. Free with `nominal_ingest_tabular_free` (the
/// ingest calls do not free, so one staging handle can serve several files).
#[no_mangle]
pub extern "C" fn nominal_ingest_tabular_begin(out_staging: *mut i32) -> i32 {
    guard(|| {
        if out_staging.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_staging must not be null",
            );
        }
        let params = TabularIngestParams {
            timestamp: None,
            channel_prefix: None,
            tag_columns: BTreeMap::new(),
            file_tags: BTreeMap::new(),
            exclude_columns: BTreeSet::new(),
            is_archive: None,
        };
        // SAFETY: out_staging checked non-null above; caller owns it.
        unsafe { *out_staging = TabularIngestStagingHandle::insert(Mutex::new(params)) };
        0
    })
}

/// Timestamps are ISO 8601 strings in the named column.
#[no_mangle]
pub extern "C" fn nominal_ingest_tabular_set_timestamp_iso8601(
    staging: i32,
    column: *const c_char,
) -> i32 {
    guard(|| {
        let staging = lookup_handle!(TabularIngestStagingHandle, staging);
        let column = match read_required_str(column, "column") {
            Ok(value) => value,
            Err(code) => return code,
        };
        staging
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .timestamp = Some(Timestamp::iso8601(column));
        0
    })
}

/// Timestamps are numeric epochs in the named column, in the given
/// `NominalTimeUnit` (e.g. epoch-seconds).
#[no_mangle]
pub extern "C" fn nominal_ingest_tabular_set_timestamp_epoch(
    staging: i32,
    column: *const c_char,
    time_unit: i32,
) -> i32 {
    guard(|| {
        let staging = lookup_handle!(TabularIngestStagingHandle, staging);
        let column = match read_required_str(column, "column") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let unit = match time_unit_from_i32(time_unit, "time_unit") {
            Ok(value) => value,
            Err(code) => return code,
        };
        staging
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .timestamp = Some(Timestamp::epoch(column, unit));
        0
    })
}

/// Timestamps use a custom format string (Java `DateTimeFormatter` syntax)
/// in the named column. `default_year` / `default_day_of_year` fill in
/// missing date parts for formats like IRIG; pass 0 to leave unset.
#[no_mangle]
pub extern "C" fn nominal_ingest_tabular_set_timestamp_custom(
    staging: i32,
    column: *const c_char,
    format: *const c_char,
    default_year: i32,
    default_day_of_year: i32,
) -> i32 {
    guard(|| {
        let staging = lookup_handle!(TabularIngestStagingHandle, staging);
        let column = match read_required_str(column, "column") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let format = match read_required_str(format, "format") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let mut timestamp = Timestamp::custom(column, format);
        if default_year != 0 {
            timestamp = timestamp.with_default_year(default_year);
        }
        if default_day_of_year != 0 {
            timestamp = timestamp.with_default_day_of_year(default_day_of_year);
        }
        staging
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .timestamp = Some(timestamp);
        0
    })
}

/// Timestamps are numeric offsets in the named column, in the given
/// `NominalTimeUnit`, relative to a start time. `offset_ms` (`f64` Unix
/// milliseconds, with `has_offset` true) anchors the offsets — required when
/// ingesting into an existing dataset.
#[no_mangle]
pub extern "C" fn nominal_ingest_tabular_set_timestamp_relative(
    staging: i32,
    column: *const c_char,
    time_unit: i32,
    offset_ms: f64,
    has_offset: bool,
) -> i32 {
    guard(|| {
        let staging = lookup_handle!(TabularIngestStagingHandle, staging);
        let column = match read_required_str(column, "column") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let unit = match time_unit_from_i32(time_unit, "time_unit") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let mut timestamp = Timestamp::relative(column, unit);
        if has_offset {
            let offset = match ms_to_datetime(offset_ms, "offset_ms") {
                Ok(value) => value,
                Err(code) => return code,
            };
            timestamp = timestamp.with_offset(offset);
        }
        staging
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .timestamp = Some(timestamp);
        0
    })
}

/// Prefixes every channel name in the file with the given string.
#[no_mangle]
pub extern "C" fn nominal_ingest_tabular_set_channel_prefix(
    staging: i32,
    prefix: *const c_char,
) -> i32 {
    guard(|| {
        let staging = lookup_handle!(TabularIngestStagingHandle, staging);
        let prefix = match read_required_str(prefix, "prefix") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if prefix.is_empty() {
            return fail(
                NominalErrorCode::InvalidArgument,
                "prefix must not be empty",
            );
        }
        staging
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .channel_prefix = Some(prefix);
        0
    })
}

/// Derives the given tag's value from the named column (repeatable).
#[no_mangle]
pub extern "C" fn nominal_ingest_tabular_add_tag_column(
    staging: i32,
    tag: *const c_char,
    column: *const c_char,
) -> i32 {
    guard(|| {
        let staging = lookup_handle!(TabularIngestStagingHandle, staging);
        let tag = match read_required_str(tag, "tag") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if tag.is_empty() {
            return fail(NominalErrorCode::InvalidArgument, "tag must not be empty");
        }
        let column = match read_required_str(column, "column") {
            Ok(value) => value,
            Err(code) => return code,
        };
        staging
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .tag_columns
            .insert(tag, column);
        0
    })
}

/// Applies a fixed tag value to every row in the file (repeatable).
#[no_mangle]
pub extern "C" fn nominal_ingest_tabular_add_file_tag(
    staging: i32,
    tag: *const c_char,
    value: *const c_char,
) -> i32 {
    guard(|| {
        let staging = lookup_handle!(TabularIngestStagingHandle, staging);
        let tag = match read_required_str(tag, "tag") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if tag.is_empty() {
            return fail(NominalErrorCode::InvalidArgument, "tag must not be empty");
        }
        let value = match read_required_str(value, "value") {
            Ok(value) => value,
            Err(code) => return code,
        };
        staging
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .file_tags
            .insert(tag, value);
        0
    })
}

/// Excludes the named column from ingestion (repeatable).
#[no_mangle]
pub extern "C" fn nominal_ingest_tabular_exclude_column(
    staging: i32,
    column: *const c_char,
) -> i32 {
    guard(|| {
        let staging = lookup_handle!(TabularIngestStagingHandle, staging);
        let column = match read_required_str(column, "column") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if column.is_empty() {
            return fail(
                NominalErrorCode::InvalidArgument,
                "column must not be empty",
            );
        }
        staging
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .exclude_columns
            .insert(column);
        0
    })
}

/// Marks the file as an archive (.tar, .tar.gz, .zip) whose .parquet entries
/// will be extracted and ingested. Parquet only — `nominal_ingest_csv`
/// rejects a staging handle with this set.
#[no_mangle]
pub extern "C" fn nominal_ingest_tabular_set_is_archive(staging: i32, is_archive: bool) -> i32 {
    guard(|| {
        let staging = lookup_handle!(TabularIngestStagingHandle, staging);
        staging
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_archive = Some(is_archive);
        0
    })
}

/// Frees a tabular-ingest staging handle. Freeing twice returns an error.
#[no_mangle]
pub extern "C" fn nominal_ingest_tabular_free(staging: i32) -> i32 {
    guard(|| {
        if TabularIngestStagingHandle::remove(staging) {
            0
        } else {
            fail(
                NominalErrorCode::InvalidHandle,
                format!("invalid tabular-ingest staging handle: {staging}"),
            )
        }
    })
}

// ---------------------------------------------------------------------------
// Upload + ingest
// ---------------------------------------------------------------------------

/// The ingest job plus, for jobs created by an upload here, the RID of the
/// dataset the data landed in.
pub(crate) struct FfiIngestJob {
    job: IngestJob,
    result_rid: Option<String>,
}

handle_registry!(IngestJobHandle, FfiIngestJob);

/// Reads the staged params into a fresh CsvIngest/ParquetIngest-agnostic
/// closure input. Returns the timestamp separately since both builders
/// require it at construction.
#[allow(clippy::type_complexity)]
fn staged_parts(
    staging: &Mutex<TabularIngestParams>,
) -> Result<
    (
        Timestamp,
        Option<String>,
        BTreeMap<String, String>,
        BTreeMap<String, String>,
        BTreeSet<String>,
        Option<bool>,
    ),
    i32,
> {
    let params = staging
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let timestamp = params.timestamp.clone().ok_or_else(|| {
        fail(
            NominalErrorCode::InvalidArgument,
            "no timestamp spec staged: call one of the set_timestamp_* functions first",
        )
    })?;
    Ok((
        timestamp,
        params.channel_prefix.clone(),
        params.tag_columns.clone(),
        params.file_tags.clone(),
        params.exclude_columns.clone(),
        params.is_archive,
    ))
}

/// Uploads a CSV file (`.csv` / `.csv.gz`) and ingests it into the existing
/// dataset with the given RID, using the staged options (a timestamp spec is
/// required). Blocks until the upload completes and the server accepts the
/// ingest — the ingest itself continues server-side. Writes a job handle to
/// `out_job`; poll it with `nominal_ingest_job_get`/`_wait`, read the target
/// dataset back with `nominal_ingest_job_result_rid`, and free it with
/// `nominal_ingest_job_free`. The staging handle stays valid for reuse.
#[no_mangle]
pub extern "C" fn nominal_ingest_csv(
    client: i32,
    staging: i32,
    file_path: *const c_char,
    dataset_rid: *const c_char,
    out_job: *mut i32,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let staging = lookup_handle!(TabularIngestStagingHandle, staging);
        let file_path = match read_required_str(file_path, "file_path") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let dataset_rid = match read_required_str(dataset_rid, "dataset_rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if out_job.is_null() {
            return fail(NominalErrorCode::NullArgument, "out_job must not be null");
        }

        let (timestamp, prefix, tag_columns, file_tags, exclude_columns, is_archive) =
            match staged_parts(&staging) {
                Ok(parts) => parts,
                Err(code) => return code,
            };
        if is_archive.is_some() {
            return fail(
                NominalErrorCode::InvalidArgument,
                "is_archive applies to parquet only — use nominal_ingest_parquet",
            );
        }

        let mut ingest = CsvIngest::new(timestamp);
        if let Some(prefix) = prefix {
            ingest = ingest.channel_prefix(prefix);
        }
        for (tag, column) in tag_columns {
            ingest = ingest.tag_column(tag, column);
        }
        for (tag, value) in file_tags {
            ingest = ingest.additional_file_tag(tag, value);
        }
        for column in exclude_columns {
            ingest = ingest.exclude_column(column);
        }

        match block_on(
            client
                .ingest()
                .upload_csv(&file_path, dataset_rid.as_str(), ingest),
        ) {
            Ok((job, rid)) => {
                let handle = IngestJobHandle::insert(FfiIngestJob {
                    job,
                    result_rid: Some(rid),
                });
                // SAFETY: out_job checked non-null above; caller owns it.
                unsafe { *out_job = handle };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Uploads a Parquet file (or archive of Parquet files — see
/// `nominal_ingest_tabular_set_is_archive`) and ingests it into the existing
/// dataset with the given RID. Otherwise identical to `nominal_ingest_csv`.
#[no_mangle]
pub extern "C" fn nominal_ingest_parquet(
    client: i32,
    staging: i32,
    file_path: *const c_char,
    dataset_rid: *const c_char,
    out_job: *mut i32,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let staging = lookup_handle!(TabularIngestStagingHandle, staging);
        let file_path = match read_required_str(file_path, "file_path") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let dataset_rid = match read_required_str(dataset_rid, "dataset_rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if out_job.is_null() {
            return fail(NominalErrorCode::NullArgument, "out_job must not be null");
        }

        let (timestamp, prefix, tag_columns, file_tags, exclude_columns, is_archive) =
            match staged_parts(&staging) {
                Ok(parts) => parts,
                Err(code) => return code,
            };

        let mut ingest = ParquetIngest::new(timestamp);
        if let Some(prefix) = prefix {
            ingest = ingest.channel_prefix(prefix);
        }
        for (tag, column) in tag_columns {
            ingest = ingest.tag_column(tag, column);
        }
        for (tag, value) in file_tags {
            ingest = ingest.additional_file_tag(tag, value);
        }
        for column in exclude_columns {
            ingest = ingest.exclude_column(column);
        }
        if let Some(is_archive) = is_archive {
            ingest = ingest.is_archive(is_archive);
        }

        match block_on(
            client
                .ingest()
                .upload_parquet(&file_path, dataset_rid.as_str(), ingest),
        ) {
            Ok((job, rid)) => {
                let handle = IngestJobHandle::insert(FfiIngestJob {
                    job,
                    result_rid: Some(rid),
                });
                // SAFETY: out_job checked non-null above; caller owns it.
                unsafe { *out_job = handle };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

// ---------------------------------------------------------------------------
// Ingest jobs
// ---------------------------------------------------------------------------

/// Fetches the current state of the ingest job with the given RID, writing a
/// job handle to `out_job` (free with `nominal_ingest_job_free`).
#[no_mangle]
pub extern "C" fn nominal_ingest_job_get(
    client: i32,
    rid: *const c_char,
    out_job: *mut i32,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let rid = match read_required_str(rid, "rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if out_job.is_null() {
            return fail(NominalErrorCode::NullArgument, "out_job must not be null");
        }

        match block_on(client.ingest().get_ingest_job(&rid)) {
            Ok(job) => {
                let handle = IngestJobHandle::insert(FfiIngestJob {
                    job,
                    result_rid: None,
                });
                // SAFETY: out_job checked non-null above; caller owns it.
                unsafe { *out_job = handle };
                0
            }
            Err(err) => fail_sdk(err),
        }
    })
}

/// Polls the ingest job with the given RID until it reaches a terminal state
/// (Completed, Failed, Cancelled, or Unknown), then writes a job handle to
/// `out_job`. BLOCKS the calling thread for as long as the job runs. A
/// terminal-but-unsuccessful job is NOT an error here — check
/// `nominal_ingest_job_status` on the returned handle. `poll_interval_ms`
/// <= 0 uses the default (2000 ms).
#[no_mangle]
pub extern "C" fn nominal_ingest_job_wait(
    client: i32,
    rid: *const c_char,
    poll_interval_ms: f64,
    out_job: *mut i32,
) -> i32 {
    guard(|| {
        let client = lookup_handle!(ClientHandle, client);
        let rid = match read_required_str(rid, "rid") {
            Ok(value) => value,
            Err(code) => return code,
        };
        if out_job.is_null() {
            return fail(NominalErrorCode::NullArgument, "out_job must not be null");
        }
        let interval = if poll_interval_ms > 0.0 && poll_interval_ms.is_finite() {
            std::time::Duration::from_millis(poll_interval_ms.round() as u64)
        } else {
            std::time::Duration::from_millis(2000)
        };

        loop {
            let job = match block_on(client.ingest().get_ingest_job(&rid)) {
                Ok(job) => job,
                Err(err) => return fail_sdk(err),
            };
            if job.status().is_terminal() {
                let handle = IngestJobHandle::insert(FfiIngestJob {
                    job,
                    result_rid: None,
                });
                // SAFETY: out_job checked non-null above; caller owns it.
                unsafe { *out_job = handle };
                return 0;
            }
            std::thread::sleep(interval);
        }
    })
}

/// Writes the ingest job's RID into `buf`, storing the byte count needed in
/// `out_needed`.
#[no_mangle]
pub extern "C" fn nominal_ingest_job_rid(
    job: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
) -> i32 {
    guard(|| {
        let job = lookup_handle!(IngestJobHandle, job);
        write_str_field(job.job.rid(), buf, cap, out_needed)
    })
}

/// Stores the job's status in `out_status` as a `NominalIngestJobStatus`
/// value. This is a snapshot from when the handle was created — re-fetch
/// with `nominal_ingest_job_get` for fresh state.
#[no_mangle]
pub extern "C" fn nominal_ingest_job_status(job: i32, out_status: *mut i32) -> i32 {
    guard(|| {
        let job = lookup_handle!(IngestJobHandle, job);
        if out_status.is_null() {
            return fail(
                NominalErrorCode::NullArgument,
                "out_status must not be null",
            );
        }
        // SAFETY: out_status checked non-null above; caller owns it.
        unsafe { *out_status = status_to_i32(job.job.status()) };
        0
    })
}

/// Writes the RID of the dataset (or video) the ingest landed in into `buf`,
/// and whether it is known into `is_present`. Only handles returned by the
/// upload functions carry this; handles from `_get`/`_wait` report absent.
#[no_mangle]
pub extern "C" fn nominal_ingest_job_result_rid(
    job: i32,
    buf: *mut c_char,
    cap: u32,
    out_needed: *mut u32,
    is_present: *mut bool,
) -> i32 {
    guard(|| {
        let job = lookup_handle!(IngestJobHandle, job);
        write_opt_str_field(job.result_rid.as_deref(), buf, cap, out_needed, is_present)
    })
}

/// Frees an ingest-job handle. Freeing twice returns an error.
#[no_mangle]
pub extern "C" fn nominal_ingest_job_free(job: i32) -> i32 {
    guard(|| {
        if IngestJobHandle::remove(job) {
            0
        } else {
            fail(
                NominalErrorCode::InvalidHandle,
                format!("invalid ingest-job handle: {job}"),
            )
        }
    })
}
