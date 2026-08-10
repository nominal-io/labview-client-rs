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
    nominal_ingest_csv, nominal_ingest_job_free, nominal_ingest_job_get,
    nominal_ingest_job_result_rid, nominal_ingest_job_rid, nominal_ingest_job_status,
    nominal_ingest_job_wait, nominal_ingest_tabular_add_file_tag, nominal_ingest_tabular_begin,
    nominal_ingest_tabular_free, nominal_ingest_tabular_set_is_archive,
    nominal_ingest_tabular_set_timestamp_epoch,
};

const DATASET_RID: &str =
    "ri.catalog.cerulean-staging.dataset.00000000-0000-0000-0000-000000000002";
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
    let path = std::env::temp_dir().join(format!(
        "nominal_ffi_ingest_test_{}_{}.csv",
        std::process::id(),
        tag
    ));
    let mut file = std::fs::File::create(&path).expect("create temp csv");
    writeln!(file, "time,temp\n1,20.5\n2,21.0").expect("write temp csv");
    path
}

/// Mounts the whole happy-path upload+ingest pipeline on `server`.
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
    // 5. Trigger the ingest.
    mount(
        server,
        Mock::given(method("POST"))
            .and(path("/ingest/v1/ingest"))
            .and(body_partial_json(json!({"options": {"type": "csv"}})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ingestJobRid": JOB_RID,
                "details": {"type": "dataset", "dataset": {"datasetRid": DATASET_RID}}
            }))),
    );
    // 6. The job status fetch that follows the trigger.
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
