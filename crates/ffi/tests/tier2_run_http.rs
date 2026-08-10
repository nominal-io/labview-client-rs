//! Tier 2 for runs: request-building + response-parsing + FFI conversion
//! against a wiremock server speaking the Conjure wire format. Mirrors the
//! asset suite; timestamp marshaling (f64 Unix ms <-> UtcTimestamp) is the
//! run-specific ground covered here.

mod common;

use common::{cstr, last_error, read_count, read_string, TEST_RT};
use serde_json::json;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use nominal_ffi::client::{nominal_client_free, nominal_client_new};
use nominal_ffi::error::NominalErrorCode;
use nominal_ffi::handles::{nominal_handle_list_free, nominal_handle_list_get};
use nominal_ffi::run::{
    nominal_run_archive, nominal_run_asset_count, nominal_run_asset_rid_at, nominal_run_create,
    nominal_run_create_add_asset, nominal_run_create_add_label, nominal_run_create_begin,
    nominal_run_create_commit, nominal_run_create_free, nominal_run_create_set_description,
    nominal_run_create_set_end, nominal_run_create_set_property, nominal_run_created_at,
    nominal_run_data_source_count, nominal_run_data_source_name_at, nominal_run_data_source_rid_at,
    nominal_run_data_source_type_at, nominal_run_description, nominal_run_end, nominal_run_free,
    nominal_run_get, nominal_run_label_at, nominal_run_label_count, nominal_run_list,
    nominal_run_name, nominal_run_number, nominal_run_property_count, nominal_run_property_key_at,
    nominal_run_property_value_at, nominal_run_rid, nominal_run_search, nominal_run_start,
    nominal_run_unarchive, nominal_run_update, nominal_run_update_add_label,
    nominal_run_update_begin, nominal_run_update_commit, nominal_run_update_free,
    nominal_run_update_set_start, nominal_run_url,
};

const RUN_RID: &str = "ri.scout.cerulean-staging.run.00000000-0000-0000-0000-000000000010";
const ASSET_RID: &str = "ri.scout.cerulean-staging.asset.00000000-0000-0000-0000-000000000001";
const DATASET_RID: &str =
    "ri.catalog.cerulean-staging.dataset.00000000-0000-0000-0000-000000000002";
// 2024-01-15T10:30:00.500Z: seconds 1_705_314_600 + 500ms of nanos.
const START_MS: f64 = 1_705_314_600_500.0;
// One hour later, whole second.
const END_MS: f64 = 1_705_318_200_000.0;
const CREATED_AT_MS: f64 = 1_705_314_600_000.0;

/// A full Conjure-format run with every field this FFI exposes.
fn full_run_json() -> serde_json::Value {
    json!({
        "rid": RUN_RID,
        "runNumber": 42,
        "title": "Burn 42",
        "description": "Orbit raise burn",
        "startTime": {"secondsSinceEpoch": 1_705_314_600i64, "offsetNanoseconds": 500_000_000i64},
        "endTime": {"secondsSinceEpoch": 1_705_318_200i64},
        "properties": {"vehicle": "rocket-1"},
        "labels": ["prod"],
        "assets": [ASSET_RID],
        "dataSources": {
            "flight-data": {
                "dataSource": {"type": "dataset", "dataset": DATASET_RID},
                "offset": {"seconds": 0, "nanos": 0},
                "refName": "flight-data",
                "timestampType": "ABSOLUTE"
            }
        },
        "createdAt": "2024-01-15T10:30:00Z",
        "updatedAt": "2024-01-15T11:00:00Z",
        "isArchived": false
    })
}

/// A minimal run (only required Conjure fields), for optional-field checks.
fn minimal_run_json(rid: &str, title: &str, run_number: u32) -> serde_json::Value {
    json!({
        "rid": rid,
        "runNumber": run_number,
        "title": title,
        "description": "",
        "startTime": {"secondsSinceEpoch": 1_705_314_600i64},
        "createdAt": "2024-01-15T10:30:00Z",
        "updatedAt": "2024-01-15T10:30:00Z",
        "isArchived": false
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

#[test]
fn create_run_marshals_every_field() {
    let server = start_server();
    // The request must carry the exact start time, split into seconds + nanos.
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/scout/v1/run"))
            .and(body_partial_json(json!({
                "title": "Burn 42",
                "startTime": {"secondsSinceEpoch": 1_705_314_600i64, "offsetNanoseconds": 500_000_000i64}
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(full_run_json())),
    );
    let client = new_client(&server);

    let name = cstr("Burn 42");
    let description = cstr("Orbit raise burn");
    let mut run = 0i32;
    let code = nominal_run_create(
        client,
        name.as_ptr(),
        description.as_ptr(),
        START_MS,
        0.0,
        false,
        &mut run,
    );
    assert_eq!(code, 0, "create failed: {}", last_error());

    // Scalars.
    let rid = read_string(|b, c, n| nominal_run_rid(run, b, c, n)).unwrap();
    assert_eq!(rid, RUN_RID);
    let name = read_string(|b, c, n| nominal_run_name(run, b, c, n)).unwrap();
    assert_eq!(name, "Burn 42");
    let description = read_string(|b, c, n| nominal_run_description(run, b, c, n)).unwrap();
    assert_eq!(description, "Orbit raise burn");
    let url = read_string(|b, c, n| nominal_run_url(run, b, c, n)).unwrap();
    assert!(url.ends_with("/runs/42"), "got: {url}");

    let mut number = 0u32;
    assert_eq!(nominal_run_number(run, &mut number), 0);
    assert_eq!(number, 42);

    // Timestamps (the 500ms fraction must round-trip).
    let mut ms = 0f64;
    assert_eq!(nominal_run_start(run, &mut ms), 0);
    assert_eq!(ms, START_MS);
    let mut is_present = false;
    assert_eq!(nominal_run_end(run, &mut ms, &mut is_present), 0);
    assert!(is_present);
    assert_eq!(ms, END_MS);
    assert_eq!(nominal_run_created_at(run, &mut ms), 0);
    assert_eq!(ms, CREATED_AT_MS);

    // Collections.
    assert_eq!(
        read_count(|c| nominal_run_property_count(run, c)).unwrap(),
        1
    );
    let key = read_string(|b, c, n| nominal_run_property_key_at(run, 0, b, c, n)).unwrap();
    let value = read_string(|b, c, n| nominal_run_property_value_at(run, 0, b, c, n)).unwrap();
    assert_eq!((key.as_str(), value.as_str()), ("vehicle", "rocket-1"));

    assert_eq!(read_count(|c| nominal_run_label_count(run, c)).unwrap(), 1);
    let label = read_string(|b, c, n| nominal_run_label_at(run, 0, b, c, n)).unwrap();
    assert_eq!(label, "prod");

    assert_eq!(read_count(|c| nominal_run_asset_count(run, c)).unwrap(), 1);
    let asset_rid = read_string(|b, c, n| nominal_run_asset_rid_at(run, 0, b, c, n)).unwrap();
    assert_eq!(asset_rid, ASSET_RID);

    assert_eq!(
        read_count(|c| nominal_run_data_source_count(run, c)).unwrap(),
        1
    );
    let ds_name = read_string(|b, c, n| nominal_run_data_source_name_at(run, 0, b, c, n)).unwrap();
    assert_eq!(ds_name, "flight-data");
    let ds_rid = read_string(|b, c, n| nominal_run_data_source_rid_at(run, 0, b, c, n)).unwrap();
    assert_eq!(ds_rid, DATASET_RID);
    let mut kind = -1i32;
    assert_eq!(nominal_run_data_source_type_at(run, 0, &mut kind), 0);
    assert_eq!(kind, 0, "dataset = 0");

    nominal_run_free(run);
    nominal_client_free(client);
}

#[test]
fn create_rejects_non_finite_start() {
    let server = start_server();
    let client = new_client(&server);

    let name = cstr("Bad");
    let mut run = 0i32;
    let code = nominal_run_create(
        client,
        name.as_ptr(),
        std::ptr::null(),
        f64::NAN,
        0.0,
        false,
        &mut run,
    );
    assert_eq!(code, NominalErrorCode::InvalidArgument as i32);

    nominal_client_free(client);
}

#[test]
fn staged_create_sends_everything() {
    let server = start_server();
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/scout/v1/run"))
            .and(body_partial_json(json!({
                "title": "Burn 43",
                "description": "Staged burn",
                "startTime": {"secondsSinceEpoch": 1_705_314_600i64},
                "endTime": {"secondsSinceEpoch": 1_705_318_200i64},
                "labels": ["prod"],
                "properties": {"vehicle": "rocket-1"},
                "assets": [ASSET_RID]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(full_run_json())),
    );
    let client = new_client(&server);

    let name = cstr("Burn 43");
    let mut staging = 0i32;
    assert_eq!(
        nominal_run_create_begin(name.as_ptr(), 1_705_314_600_000.0, &mut staging),
        0
    );
    let description = cstr("Staged burn");
    assert_eq!(
        nominal_run_create_set_description(staging, description.as_ptr()),
        0
    );
    assert_eq!(nominal_run_create_set_end(staging, END_MS), 0);
    let label = cstr("prod");
    assert_eq!(nominal_run_create_add_label(staging, label.as_ptr()), 0);
    let (key, value) = (cstr("vehicle"), cstr("rocket-1"));
    assert_eq!(
        nominal_run_create_set_property(staging, key.as_ptr(), value.as_ptr()),
        0
    );
    let asset = cstr(ASSET_RID);
    assert_eq!(nominal_run_create_add_asset(staging, asset.as_ptr()), 0);

    let mut run = 0i32;
    let code = nominal_run_create_commit(client, staging, &mut run);
    assert_eq!(code, 0, "staged create failed: {}", last_error());

    assert_eq!(nominal_run_create_free(staging), 0);
    assert_eq!(
        nominal_run_create_free(staging),
        NominalErrorCode::InvalidHandle as i32
    );

    nominal_run_free(run);
    nominal_client_free(client);
}

#[test]
fn staged_create_rejects_bad_asset_rid_at_commit() {
    // The RID is validated upstream at commit (request-build time) — a bad
    // one must fail with InvalidArgument before any request is sent.
    let server = start_server();
    let client = new_client(&server);

    let name = cstr("Burn 44");
    let mut staging = 0i32;
    assert_eq!(
        nominal_run_create_begin(name.as_ptr(), START_MS, &mut staging),
        0
    );
    let bad = cstr("not-a-rid");
    assert_eq!(nominal_run_create_add_asset(staging, bad.as_ptr()), 0);

    let mut run = 0i32;
    assert_eq!(
        nominal_run_create_commit(client, staging, &mut run),
        NominalErrorCode::InvalidArgument as i32
    );

    nominal_run_create_free(staging);
    nominal_client_free(client);
}

#[test]
fn get_run_by_rid_and_absent_end() {
    let server = start_server();
    mount(
        &server,
        Mock::given(method("GET"))
            .and(path(format!("/scout/v1/run/{RUN_RID}")))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(minimal_run_json(RUN_RID, "Bare", 7)),
            ),
    );
    let client = new_client(&server);

    let rid = cstr(RUN_RID);
    let mut run = 0i32;
    let code = nominal_run_get(client, rid.as_ptr(), &mut run);
    assert_eq!(code, 0, "get failed: {}", last_error());

    let name = read_string(|b, c, n| nominal_run_name(run, b, c, n)).unwrap();
    assert_eq!(name, "Bare");
    let description = read_string(|b, c, n| nominal_run_description(run, b, c, n)).unwrap();
    assert_eq!(description, "");

    // A run with no end time reports not-present.
    let mut ms = -1f64;
    let mut is_present = true;
    assert_eq!(nominal_run_end(run, &mut ms, &mut is_present), 0);
    assert!(!is_present);
    assert_eq!(ms, 0.0);

    assert_eq!(read_count(|c| nominal_run_label_count(run, c)).unwrap(), 0);
    assert_eq!(read_count(|c| nominal_run_asset_count(run, c)).unwrap(), 0);

    nominal_run_free(run);
    nominal_client_free(client);
}

#[test]
fn list_runs_returns_handle_list() {
    let server = start_server();
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/scout/v1/search-runs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "results": [
                    minimal_run_json(RUN_RID, "First", 1),
                    minimal_run_json(&RUN_RID.replace("0010", "0011"), "Second", 2)
                ]
            }))),
    );
    let client = new_client(&server);

    let mut list = 0i32;
    let mut count = 0u32;
    let code = nominal_run_list(client, &mut list, &mut count);
    assert_eq!(code, 0, "list failed: {}", last_error());
    assert_eq!(count, 2);

    let names: Vec<String> = (0..count as i32)
        .map(|i| {
            let mut handle = 0i32;
            assert_eq!(nominal_handle_list_get(list, i, &mut handle), 0);
            let name = read_string(|b, c, n| nominal_run_name(handle, b, c, n)).unwrap();
            nominal_run_free(handle);
            name
        })
        .collect();
    assert_eq!(names, vec!["First", "Second"]);

    nominal_handle_list_free(list);
    nominal_client_free(client);
}

#[test]
fn search_sends_combined_filters() {
    let server = start_server();
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/scout/v1/search-runs"))
            .and(body_partial_json(json!({
                "query": {
                    "type": "and",
                    "and": [
                        {"type": "searchText", "searchText": "burn"},
                        {"type": "labels", "labels": {"labels": ["prod"], "operator": "OR"}},
                        {"type": "runNumber", "runNumber": 42}
                    ]
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "results": [minimal_run_json(RUN_RID, "Burn 42", 42)]
            }))),
    );
    let client = new_client(&server);

    let search_text = cstr("burn");
    let label = cstr("prod");
    let mut list = 0i32;
    let mut count = 0u32;
    let code = nominal_run_search(
        client,
        search_text.as_ptr(),
        label.as_ptr(),
        std::ptr::null(),
        std::ptr::null(),
        42,
        0.0,
        false,
        0.0,
        false,
        &mut list,
        &mut count,
    );
    assert_eq!(code, 0, "search failed: {}", last_error());
    assert_eq!(count, 1);

    let mut handle = 0i32;
    nominal_handle_list_get(list, 0, &mut handle);
    nominal_run_free(handle);
    nominal_handle_list_free(list);
    nominal_client_free(client);
}

#[test]
fn flat_update_sends_only_set_fields() {
    let server = start_server();
    mount(
        &server,
        Mock::given(method("PUT"))
            .and(path(format!("/scout/v1/run/{RUN_RID}")))
            .and(body_partial_json(json!({"title": "Renamed"})))
            .respond_with(ResponseTemplate::new(200).set_body_json(full_run_json())),
    );
    let client = new_client(&server);

    let rid = cstr(RUN_RID);
    let name = cstr("Renamed");
    let mut updated = 0i32;
    let code = nominal_run_update(
        client,
        rid.as_ptr(),
        name.as_ptr(),
        std::ptr::null(),
        &mut updated,
    );
    assert_eq!(code, 0, "update failed: {}", last_error());

    nominal_run_free(updated);
    nominal_client_free(client);
}

#[test]
fn staged_update_replaces_labels_and_start() {
    let server = start_server();
    mount(
        &server,
        Mock::given(method("PUT"))
            .and(path(format!("/scout/v1/run/{RUN_RID}")))
            .and(body_partial_json(json!({
                "labels": ["only-label"],
                "startTime": {"secondsSinceEpoch": 1_705_314_600i64}
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(full_run_json())),
    );
    let client = new_client(&server);

    let mut staging = 0i32;
    assert_eq!(nominal_run_update_begin(&mut staging), 0);
    let label = cstr("only-label");
    assert_eq!(nominal_run_update_add_label(staging, label.as_ptr()), 0);
    assert_eq!(
        nominal_run_update_set_start(staging, 1_705_314_600_000.0),
        0
    );

    let rid = cstr(RUN_RID);
    let mut updated = 0i32;
    let code = nominal_run_update_commit(client, rid.as_ptr(), staging, &mut updated);
    assert_eq!(code, 0, "staged update failed: {}", last_error());

    nominal_run_update_free(staging);
    nominal_run_free(updated);
    nominal_client_free(client);
}

#[test]
fn staged_update_with_no_fields_is_rejected() {
    let _guard = common::message_lock();
    let server = start_server();
    let client = new_client(&server);

    let mut staging = 0i32;
    assert_eq!(nominal_run_update_begin(&mut staging), 0);
    let rid = cstr(RUN_RID);
    let mut updated = 0i32;
    assert_eq!(
        nominal_run_update_commit(client, rid.as_ptr(), staging, &mut updated),
        NominalErrorCode::InvalidArgument as i32
    );
    assert!(last_error().contains("no fields"), "got: {}", last_error());

    nominal_run_update_free(staging);
    nominal_client_free(client);
}

#[test]
fn archive_and_unarchive() {
    let server = start_server();
    // PUT (not POST like assets), and these endpoints return a JSON bool.
    mount(
        &server,
        Mock::given(method("PUT"))
            .and(path(format!("/scout/v1/archive-run/{RUN_RID}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(true)),
    );
    mount(
        &server,
        Mock::given(method("PUT"))
            .and(path(format!("/scout/v1/unarchive-run/{RUN_RID}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(true)),
    );
    let client = new_client(&server);

    let rid = cstr(RUN_RID);
    assert_eq!(
        nominal_run_archive(client, rid.as_ptr()),
        0,
        "archive failed: {}",
        last_error()
    );
    assert_eq!(
        nominal_run_unarchive(client, rid.as_ptr()),
        0,
        "unarchive failed: {}",
        last_error()
    );

    nominal_client_free(client);
}

#[test]
fn run_calls_reject_invalid_handles() {
    let _guard = common::message_lock();
    let rid = cstr(RUN_RID);
    let mut out = 0i32;
    assert_eq!(
        nominal_run_get(0, rid.as_ptr(), &mut out),
        NominalErrorCode::InvalidHandle as i32
    );
    let err = read_string(|b, c, n| nominal_run_name(999_999_999, b, c, n)).unwrap_err();
    assert_eq!(err, NominalErrorCode::InvalidHandle as i32);
    assert_eq!(
        nominal_run_update_set_start(0, 0.0),
        NominalErrorCode::InvalidHandle as i32
    );
}
