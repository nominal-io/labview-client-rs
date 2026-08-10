//! Tier 2 for ingest: the full upload + trigger + job flow against wiremock.
//! One server plays four roles: the upload service (initiate/sign/complete),
//! the "S3" the presigned PUT lands on, the ingest trigger, and the job
//! status endpoint. This is the deepest end-to-end path in the library.

mod common;

use std::io::Write;

use common::{cstr, last_error, read_string, TEST_RT};
use serde_json::json;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use nominal_ffi::client::{nominal_client_free, nominal_client_new};
use nominal_ffi::error::NominalErrorCode;
use nominal_ffi::ingest::{
    nominal_ingest_avro_stream, nominal_ingest_csv, nominal_ingest_dataflash,
    nominal_ingest_dataflash_add_file_tag, nominal_ingest_dataflash_begin,
    nominal_ingest_dataflash_free, nominal_ingest_job_free, nominal_ingest_job_get,
    nominal_ingest_job_result_rid, nominal_ingest_job_rid, nominal_ingest_job_status,
    nominal_ingest_job_wait, nominal_ingest_journal_json, nominal_ingest_mcap,
    nominal_ingest_mcap_add_file_tag, nominal_ingest_mcap_begin, nominal_ingest_mcap_exclude_topic,
    nominal_ingest_mcap_free, nominal_ingest_mcap_include_topic,
    nominal_ingest_mcap_set_ignore_invalid_topics, nominal_ingest_tabular_add_file_tag,
    nominal_ingest_tabular_begin, nominal_ingest_tabular_free,
    nominal_ingest_tabular_set_is_archive, nominal_ingest_tabular_set_timestamp_epoch,
    nominal_ingest_video, nominal_ingest_video_mcap,
};

const DATASET_RID: &str =
    "ri.catalog.cerulean-staging.dataset.00000000-0000-0000-0000-000000000002";
const VIDEO_RID: &str = "ri.catalog.cerulean-staging.video.00000000-0000-0000-0000-000000000003";
const JOB_RID: &str = "ri.ingest.cerulean-staging.ingest-job.00000000-0000-0000-0000-000000000042";

fn job_json(status: &str) -> serde_json::Value {
    json!({
        "ingestJobRid": JOB_RID,
        "status": status,
        "createdBy": "00000000-0000-0000-0000-0000000000bb",
        "orgUuid": "00000000-0000-0000-0000-0000000000cc",
        "ingestType": "TABULAR"
    })
}

fn start_server() -> MockServer {
    TEST_RT.block_on(MockServer::start())
}

fn mount(server: &MockServer, mock: Mock) {
    TEST_RT.block_on(mock.mount(server));
}

fn new_client(server: &MockServer) -> i32 {
    let token = cstr("test-token");
    let base_url = cstr(&server.uri());
    let mut handle = 0i32;
    let code = nominal_client_new(
        token.as_ptr(),
        std::ptr::null(),
        base_url.as_ptr(),
        &mut handle,
    );
    assert_eq!(code, 0, "client_new failed: {}", last_error());
    handle
}

/// Writes a small CSV to the temp dir, returning its path.
fn temp_csv(tag: &str) -> std::path::PathBuf {
    temp_file(tag, "csv", "time,temp\n1,20.5\n2,21.0\n")
}

/// Writes a small file with arbitrary content and extension to the temp dir.
/// The upload path never parses the content — the server does — so a few
/// placeholder bytes stand in for every format.
fn temp_file(tag: &str, ext: &str, content: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "nominal_ffi_ingest_test_{}_{}.{}",
        std::process::id(),
        tag,
        ext
    ));
    let mut file = std::fs::File::create(&path).expect("create temp file");
    write!(file, "{content}").expect("write temp file");
    path
}

/// Mounts the ingest trigger: only matches when the request body carries the
/// expected options shape, and answers with the given details object.
fn mount_trigger(
    server: &MockServer,
    options_match: serde_json::Value,
    details: serde_json::Value,
) {
    mount(
        server,
        Mock::given(method("POST"))
            .and(path("/ingest/v1/ingest"))
            .and(body_partial_json(json!({"options": options_match})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ingestJobRid": JOB_RID,
                "details": details
            }))),
    );
}

/// The details object for an ingest that landed in the standard test dataset.
fn dataset_details() -> serde_json::Value {
    json!({"type": "dataset", "dataset": {"datasetRid": DATASET_RID}})
}

/// Mounts the upload half of the pipeline (initiate/sign/S3-PUT/complete)
/// plus the job-status fetch that follows every trigger. Each test mounts
/// its own trigger via `mount_trigger` so the request shape is verified.
fn mount_upload_pipeline(server: &MockServer) {
    // 1. Initiate multipart upload.
    mount(
        server,
        Mock::given(method("POST"))
            .and(path("/upload/v1/multipart-upload"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "uploadId": "up-1",
                "key": "key-1",
                "bucket": "bkt"
            }))),
    );
    // 2. Sign a part — the presigned URL points back at this same server.
    mount(
        server,
        Mock::given(method("POST"))
            .and(path("/upload/v1/multipart-upload/up-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "url": format!("{}/fake-s3/part-1", server.uri())
            }))),
    );
    // 3. The "S3" PUT — must return an ETag header.
    mount(
        server,
        Mock::given(method("PUT"))
            .and(path("/fake-s3/part-1"))
            .respond_with(ResponseTemplate::new(200).insert_header("ETag", "\"etag-1\"")),
    );
    // 4. Complete the multipart upload.
    mount(
        server,
        Mock::given(method("POST"))
            .and(path("/upload/v1/multipart-upload/up-1/complete"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "location": "s3://bkt/key-1"
            }))),
    );
    // 5. The job status fetch that follows the trigger.
    mount(
        server,
        Mock::given(method("GET"))
            .and(path(format!("/ingest/v1/ingest-job/{JOB_RID}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(job_json("IN_PROGRESS"))),
    );
}

#[test]
fn csv_upload_and_ingest_end_to_end() {
    let server = start_server();
    mount_upload_pipeline(&server);
    mount_trigger(&server, json!({"type": "csv"}), dataset_details());
    let client = new_client(&server);
    let csv = temp_csv("e2e");

    let mut staging = 0i32;
    assert_eq!(nominal_ingest_tabular_begin(&mut staging), 0);
    let column = cstr("time");
    assert_eq!(
        nominal_ingest_tabular_set_timestamp_epoch(staging, column.as_ptr(), 3), // Seconds
        0
    );
    let (tag, value) = (cstr("vehicle"), cstr("rocket-1"));
    assert_eq!(
        nominal_ingest_tabular_add_file_tag(staging, tag.as_ptr(), value.as_ptr()),
        0
    );

    let file_path = cstr(csv.to_str().unwrap());
    let dataset = cstr(DATASET_RID);
    let mut job = 0i32;
    let code = nominal_ingest_csv(
        client,
        staging,
        file_path.as_ptr(),
        dataset.as_ptr(),
        &mut job,
    );
    assert_eq!(code, 0, "ingest_csv failed: {}", last_error());

    let rid = read_string(|b, c, n| nominal_ingest_job_rid(job, b, c, n)).unwrap();
    assert_eq!(rid, JOB_RID);
    let mut status = -1i32;
    assert_eq!(nominal_ingest_job_status(job, &mut status), 0);
    assert_eq!(status, 2, "IN_PROGRESS = 2");
    let mut is_present = false;
    let result_rid =
        read_string(|b, c, n| nominal_ingest_job_result_rid(job, b, c, n, &mut is_present))
            .unwrap();
    assert!(is_present, "upload-created jobs carry the dataset RID");
    assert_eq!(result_rid, DATASET_RID);

    assert_eq!(nominal_ingest_job_free(job), 0);
    assert_eq!(nominal_ingest_tabular_free(staging), 0);
    nominal_client_free(client);
    let _ = std::fs::remove_file(csv);
}

#[test]
fn csv_requires_a_timestamp_spec() {
    let _guard = common::message_lock();
    let server = start_server();
    let client = new_client(&server);
    let csv = temp_csv("no_ts");

    let mut staging = 0i32;
    assert_eq!(nominal_ingest_tabular_begin(&mut staging), 0);

    let file_path = cstr(csv.to_str().unwrap());
    let dataset = cstr(DATASET_RID);
    let mut job = 0i32;
    assert_eq!(
        nominal_ingest_csv(
            client,
            staging,
            file_path.as_ptr(),
            dataset.as_ptr(),
            &mut job
        ),
        NominalErrorCode::InvalidArgument as i32
    );
    assert!(last_error().contains("timestamp"), "got: {}", last_error());

    nominal_ingest_tabular_free(staging);
    nominal_client_free(client);
    let _ = std::fs::remove_file(csv);
}

#[test]
fn csv_rejects_is_archive() {
    let _guard = common::message_lock();
    let server = start_server();
    let client = new_client(&server);
    let csv = temp_csv("archive");

    let mut staging = 0i32;
    assert_eq!(nominal_ingest_tabular_begin(&mut staging), 0);
    let column = cstr("time");
    assert_eq!(
        nominal_ingest_tabular_set_timestamp_epoch(staging, column.as_ptr(), 3),
        0
    );
    assert_eq!(nominal_ingest_tabular_set_is_archive(staging, true), 0);

    let file_path = cstr(csv.to_str().unwrap());
    let dataset = cstr(DATASET_RID);
    let mut job = 0i32;
    assert_eq!(
        nominal_ingest_csv(
            client,
            staging,
            file_path.as_ptr(),
            dataset.as_ptr(),
            &mut job
        ),
        NominalErrorCode::InvalidArgument as i32
    );
    assert!(last_error().contains("parquet"), "got: {}", last_error());

    nominal_ingest_tabular_free(staging);
    nominal_client_free(client);
    let _ = std::fs::remove_file(csv);
}

#[test]
fn invalid_time_unit_is_rejected() {
    let _guard = common::message_lock();
    let mut staging = 0i32;
    assert_eq!(nominal_ingest_tabular_begin(&mut staging), 0);
    let column = cstr("time");
    assert_eq!(
        nominal_ingest_tabular_set_timestamp_epoch(staging, column.as_ptr(), 7),
        NominalErrorCode::InvalidArgument as i32
    );
    nominal_ingest_tabular_free(staging);
}

#[test]
fn job_get_reads_status_without_result_rid() {
    let server = start_server();
    mount(
        &server,
        Mock::given(method("GET"))
            .and(path(format!("/ingest/v1/ingest-job/{JOB_RID}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(job_json("QUEUED"))),
    );
    let client = new_client(&server);

    let rid = cstr(JOB_RID);
    let mut job = 0i32;
    let code = nominal_ingest_job_get(client, rid.as_ptr(), &mut job);
    assert_eq!(code, 0, "job_get failed: {}", last_error());

    let mut status = -1i32;
    assert_eq!(nominal_ingest_job_status(job, &mut status), 0);
    assert_eq!(status, 1, "QUEUED = 1");

    let mut is_present = true;
    let mut needed = 0u32;
    let code =
        nominal_ingest_job_result_rid(job, std::ptr::null_mut(), 0, &mut needed, &mut is_present);
    assert_eq!(
        (code, is_present),
        (0, false),
        "get-jobs carry no result RID"
    );

    nominal_ingest_job_free(job);
    nominal_client_free(client);
}

#[test]
fn job_wait_returns_terminal_failed_without_erroring() {
    let server = start_server();
    // A FAILED job is a result, not an FFI error — the caller inspects status.
    mount(
        &server,
        Mock::given(method("GET"))
            .and(path(format!("/ingest/v1/ingest-job/{JOB_RID}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(job_json("FAILED"))),
    );
    let client = new_client(&server);

    let rid = cstr(JOB_RID);
    let mut job = 0i32;
    let code = nominal_ingest_job_wait(client, rid.as_ptr(), 10.0, &mut job);
    assert_eq!(code, 0, "job_wait failed: {}", last_error());

    let mut status = -1i32;
    assert_eq!(nominal_ingest_job_status(job, &mut status), 0);
    assert_eq!(status, 4, "FAILED = 4");

    nominal_ingest_job_free(job);
    nominal_client_free(client);
}

/// Reads the job handle's result RID, asserting it's present, then frees the
/// job — shared tail of every happy-path upload test.
fn assert_result_rid_and_free(job: i32, expected: &str) {
    let mut is_present = false;
    let result_rid =
        read_string(|b, c, n| nominal_ingest_job_result_rid(job, b, c, n, &mut is_present))
            .unwrap();
    assert!(is_present, "upload-created jobs carry the result RID");
    assert_eq!(result_rid, expected);
    assert_eq!(nominal_ingest_job_free(job), 0);
}

#[test]
fn mcap_ingest_sends_topic_filters_and_tags() {
    let server = start_server();
    mount_upload_pipeline(&server);
    // The trigger only matches when both include topics, the file tag, and
    // the ignore flag arrive in the mcapProtobufTimeseries options.
    mount_trigger(
        &server,
        json!({
            "type": "mcapProtobufTimeseries",
            "mcapProtobufTimeseries": {
                "channelFilter": {
                    "type": "include",
                    "include": [
                        {"type": "topic", "topic": "/camera"},
                        {"type": "topic", "topic": "/imu"}
                    ]
                },
                "additionalFileTags": {"source": "labview"},
                "ignoreInvalidTopics": true
            }
        }),
        dataset_details(),
    );
    let client = new_client(&server);
    let mcap = temp_file("mcap_e2e", "mcap", "not-a-real-mcap");

    let mut staging = 0i32;
    assert_eq!(nominal_ingest_mcap_begin(&mut staging), 0);
    for topic in ["/camera", "/imu"] {
        let topic = cstr(topic);
        assert_eq!(
            nominal_ingest_mcap_include_topic(staging, topic.as_ptr()),
            0
        );
    }
    let (tag, value) = (cstr("source"), cstr("labview"));
    assert_eq!(
        nominal_ingest_mcap_add_file_tag(staging, tag.as_ptr(), value.as_ptr()),
        0
    );
    assert_eq!(
        nominal_ingest_mcap_set_ignore_invalid_topics(staging, true),
        0
    );

    let file_path = cstr(mcap.to_str().unwrap());
    let dataset = cstr(DATASET_RID);
    let mut job = 0i32;
    let code = nominal_ingest_mcap(
        client,
        staging,
        file_path.as_ptr(),
        dataset.as_ptr(),
        &mut job,
    );
    assert_eq!(code, 0, "ingest_mcap failed: {}", last_error());
    assert_result_rid_and_free(job, DATASET_RID);

    assert_eq!(nominal_ingest_mcap_free(staging), 0);
    assert_eq!(
        nominal_ingest_mcap_free(staging),
        NominalErrorCode::InvalidHandle as i32,
        "double free of staging must fail"
    );
    nominal_client_free(client);
    let _ = std::fs::remove_file(mcap);
}

#[test]
fn mcap_include_and_exclude_are_mutually_exclusive() {
    let _guard = common::message_lock();
    // No pipeline mounted: the conflict must be caught before any upload.
    let server = start_server();
    let client = new_client(&server);
    let mcap = temp_file("mcap_conflict", "mcap", "x");

    let mut staging = 0i32;
    assert_eq!(nominal_ingest_mcap_begin(&mut staging), 0);
    let include = cstr("/camera");
    assert_eq!(
        nominal_ingest_mcap_include_topic(staging, include.as_ptr()),
        0
    );
    let exclude = cstr("/imu");
    assert_eq!(
        nominal_ingest_mcap_exclude_topic(staging, exclude.as_ptr()),
        0
    );

    let file_path = cstr(mcap.to_str().unwrap());
    let dataset = cstr(DATASET_RID);
    let mut job = 0i32;
    assert_eq!(
        nominal_ingest_mcap(
            client,
            staging,
            file_path.as_ptr(),
            dataset.as_ptr(),
            &mut job
        ),
        NominalErrorCode::InvalidArgument as i32
    );
    assert!(
        last_error().contains("mutually exclusive"),
        "got: {}",
        last_error()
    );

    nominal_ingest_mcap_free(staging);
    nominal_client_free(client);
    let _ = std::fs::remove_file(mcap);
}

#[test]
fn journal_json_sends_channel() {
    let server = start_server();
    mount_upload_pipeline(&server);
    mount_trigger(
        &server,
        json!({"type": "journalJson", "journalJson": {"channel": "syslog"}}),
        dataset_details(),
    );
    let client = new_client(&server);
    let jsonl = temp_file("journal_e2e", "jsonl", "{\"MESSAGE\":\"boot\"}\n");

    let file_path = cstr(jsonl.to_str().unwrap());
    let dataset = cstr(DATASET_RID);
    let channel = cstr("syslog");
    let mut job = 0i32;
    let code = nominal_ingest_journal_json(
        client,
        file_path.as_ptr(),
        dataset.as_ptr(),
        channel.as_ptr(),
        &mut job,
    );
    assert_eq!(code, 0, "ingest_journal_json failed: {}", last_error());
    assert_result_rid_and_free(job, DATASET_RID);

    nominal_client_free(client);
    let _ = std::fs::remove_file(jsonl);
}

#[test]
fn avro_stream_end_to_end() {
    let server = start_server();
    mount_upload_pipeline(&server);
    mount_trigger(&server, json!({"type": "avroStream"}), dataset_details());
    let client = new_client(&server);
    let avro = temp_file("avro_e2e", "avro", "not-a-real-avro");

    let file_path = cstr(avro.to_str().unwrap());
    let dataset = cstr(DATASET_RID);
    let mut job = 0i32;
    let code = nominal_ingest_avro_stream(client, file_path.as_ptr(), dataset.as_ptr(), &mut job);
    assert_eq!(code, 0, "ingest_avro_stream failed: {}", last_error());
    assert_result_rid_and_free(job, DATASET_RID);

    nominal_client_free(client);
    let _ = std::fs::remove_file(avro);
}

#[test]
fn dataflash_sends_file_tags() {
    let server = start_server();
    mount_upload_pipeline(&server);
    mount_trigger(
        &server,
        json!({
            "type": "dataflash",
            "dataflash": {"additionalFileTags": {"vehicle": "drone-1"}}
        }),
        dataset_details(),
    );
    let client = new_client(&server);
    let bin = temp_file("dataflash_e2e", "bin", "not-a-real-dataflash");

    let mut staging = 0i32;
    assert_eq!(nominal_ingest_dataflash_begin(&mut staging), 0);
    let (tag, value) = (cstr("vehicle"), cstr("drone-1"));
    assert_eq!(
        nominal_ingest_dataflash_add_file_tag(staging, tag.as_ptr(), value.as_ptr()),
        0
    );

    let file_path = cstr(bin.to_str().unwrap());
    let dataset = cstr(DATASET_RID);
    let mut job = 0i32;
    let code = nominal_ingest_dataflash(
        client,
        staging,
        file_path.as_ptr(),
        dataset.as_ptr(),
        &mut job,
    );
    assert_eq!(code, 0, "ingest_dataflash failed: {}", last_error());
    assert_result_rid_and_free(job, DATASET_RID);

    assert_eq!(nominal_ingest_dataflash_free(staging), 0);
    nominal_client_free(client);
    let _ = std::fs::remove_file(bin);
}

#[test]
fn video_ingest_targets_video_resource() {
    let server = start_server();
    mount_upload_pipeline(&server);
    // 2024-01-15T10:30:00.500Z — the manifest must carry the split
    // seconds + nanos and the existing-video target.
    mount_trigger(
        &server,
        json!({
            "type": "video",
            "video": {
                "target": {"type": "existing", "existing": {"videoRid": VIDEO_RID}},
                "timestampManifest": {
                    "type": "noManifest",
                    "noManifest": {
                        "startingTimestamp": {"seconds": 1_705_314_600i64, "nanos": 500_000_000i64}
                    }
                }
            }
        }),
        json!({"type": "video", "video": {"videoRid": VIDEO_RID, "videoFileRid": "ri.catalog.cerulean-staging.video-file.00000000-0000-0000-0000-000000000009"}}),
    );
    let client = new_client(&server);
    let mp4 = temp_file("video_e2e", "mp4", "not-a-real-mp4");

    let file_path = cstr(mp4.to_str().unwrap());
    let video = cstr(VIDEO_RID);
    let mut job = 0i32;
    let code = nominal_ingest_video(
        client,
        file_path.as_ptr(),
        video.as_ptr(),
        1_705_314_600_500.0,
        &mut job,
    );
    assert_eq!(code, 0, "ingest_video failed: {}", last_error());
    assert_result_rid_and_free(job, VIDEO_RID);

    nominal_client_free(client);
    let _ = std::fs::remove_file(mp4);
}

#[test]
fn video_mcap_sends_topic_manifest() {
    let server = start_server();
    mount_upload_pipeline(&server);
    mount_trigger(
        &server,
        json!({
            "type": "video",
            "video": {
                "timestampManifest": {
                    "type": "mcap",
                    "mcap": {
                        "mcapChannelLocator": {"type": "topic", "topic": "/cam0"}
                    }
                }
            }
        }),
        json!({"type": "video", "video": {"videoRid": VIDEO_RID, "videoFileRid": "ri.catalog.cerulean-staging.video-file.00000000-0000-0000-0000-000000000009"}}),
    );
    let client = new_client(&server);
    let mcap = temp_file("video_mcap_e2e", "mcap", "not-a-real-mcap");

    let file_path = cstr(mcap.to_str().unwrap());
    let video = cstr(VIDEO_RID);
    let topic = cstr("/cam0");
    let mut job = 0i32;
    let code = nominal_ingest_video_mcap(
        client,
        file_path.as_ptr(),
        video.as_ptr(),
        topic.as_ptr(),
        &mut job,
    );
    assert_eq!(code, 0, "ingest_video_mcap failed: {}", last_error());
    assert_result_rid_and_free(job, VIDEO_RID);

    nominal_client_free(client);
    let _ = std::fs::remove_file(mcap);
}

#[test]
fn ingest_calls_reject_invalid_handles() {
    let _guard = common::message_lock();
    let column = cstr("time");
    assert_eq!(
        nominal_ingest_tabular_set_timestamp_epoch(0, column.as_ptr(), 3),
        NominalErrorCode::InvalidHandle as i32
    );
    let rid = cstr(JOB_RID);
    let mut out = 0i32;
    assert_eq!(
        nominal_ingest_job_get(987_654_321, rid.as_ptr(), &mut out),
        NominalErrorCode::InvalidHandle as i32
    );
    let err = read_string(|b, c, n| nominal_ingest_job_rid(0, b, c, n)).unwrap_err();
    assert_eq!(err, NominalErrorCode::InvalidHandle as i32);
}
