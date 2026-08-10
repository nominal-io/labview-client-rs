//! Tier 2 for datasets: request-building + response-parsing + FFI conversion
//! against a wiremock server speaking the Conjure wire format. Mirrors the
//! asset suite; the `EnrichedDataset` response shape (with its required
//! nested objects) is the dataset-specific ground covered here.

mod common;

use common::{cstr, last_error, read_count, read_string, TEST_RT};
use serde_json::json;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use nominal_ffi::client::{nominal_client_free, nominal_client_new};
use nominal_ffi::dataset::{
    nominal_dataset_archive, nominal_dataset_create, nominal_dataset_create_add_label,
    nominal_dataset_create_begin, nominal_dataset_create_commit, nominal_dataset_create_free,
    nominal_dataset_create_set_channel_delimiter, nominal_dataset_create_set_description,
    nominal_dataset_create_set_property, nominal_dataset_created_at, nominal_dataset_description,
    nominal_dataset_free, nominal_dataset_get, nominal_dataset_label_at,
    nominal_dataset_label_count, nominal_dataset_list, nominal_dataset_name,
    nominal_dataset_property_count, nominal_dataset_property_key_at,
    nominal_dataset_property_value_at, nominal_dataset_rid, nominal_dataset_search,
    nominal_dataset_unarchive, nominal_dataset_update, nominal_dataset_update_add_label,
    nominal_dataset_update_begin, nominal_dataset_update_commit, nominal_dataset_update_free,
    nominal_dataset_url,
};
use nominal_ffi::error::NominalErrorCode;
use nominal_ffi::handles::{nominal_handle_list_free, nominal_handle_list_get};

const DATASET_RID: &str =
    "ri.catalog.cerulean-staging.dataset.00000000-0000-0000-0000-000000000002";
const CREATED_AT_MS: f64 = 1_705_314_600_000.0;

/// A full Conjure-format EnrichedDataset with every field this FFI exposes
/// (plus the required nested objects the upstream parser insists on).
fn full_dataset_json() -> serde_json::Value {
    json!({
        "rid": DATASET_RID,
        "uuid": "00000000-0000-0000-0000-000000000002",
        "name": "Flight Telemetry",
        "displayName": "Flight Telemetry",
        "description": "CSV telemetry from flight 42",
        "ingestDate": "2024-01-15T10:30:00Z",
        "originMetadata": {},
        "lastIngestStatus": {"type": "success", "success": {}},
        "retentionPolicy": {"type": "KEEP_FOREVER"},
        "timestampType": "ABSOLUTE",
        "labels": ["prod"],
        "properties": {"vehicle": "rocket-1"},
        "granularity": "NANOSECONDS",
        "allowStreaming": false,
        "isArchived": false
    })
}

/// A minimal dataset (required fields only), for optional-field checks.
fn minimal_dataset_json(rid: &str, name: &str) -> serde_json::Value {
    json!({
        "rid": rid,
        "uuid": "00000000-0000-0000-0000-000000000099",
        "name": name,
        "displayName": name,
        "ingestDate": "2024-01-15T10:30:00Z",
        "originMetadata": {},
        "lastIngestStatus": {"type": "success", "success": {}},
        "retentionPolicy": {"type": "KEEP_FOREVER"},
        "timestampType": "ABSOLUTE",
        "granularity": "NANOSECONDS",
        "allowStreaming": false,
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
fn create_dataset_marshals_every_field() {
    let _guard = common::message_lock();
    let server = start_server();
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/catalog/v1/datasets"))
            .and(body_partial_json(json!({
                "name": "Flight Telemetry",
                "description": "CSV telemetry from flight 42"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(full_dataset_json())),
    );
    let client = new_client(&server);

    let name = cstr("Flight Telemetry");
    let description = cstr("CSV telemetry from flight 42");
    let mut dataset = 0i32;
    let code = nominal_dataset_create(client, name.as_ptr(), description.as_ptr(), &mut dataset);
    assert_eq!(code, 0, "create failed: {}", last_error());

    let rid = read_string(|b, c, n| nominal_dataset_rid(dataset, b, c, n)).unwrap();
    assert_eq!(rid, DATASET_RID);
    let name = read_string(|b, c, n| nominal_dataset_name(dataset, b, c, n)).unwrap();
    assert_eq!(name, "Flight Telemetry");
    let url = read_string(|b, c, n| nominal_dataset_url(dataset, b, c, n)).unwrap();
    assert!(
        url.ends_with(&format!("/data-sources/{DATASET_RID}")),
        "got: {url}"
    );

    let mut is_present = false;
    let description =
        read_string(|b, c, n| nominal_dataset_description(dataset, b, c, n, &mut is_present))
            .unwrap();
    assert!(is_present);
    assert_eq!(description, "CSV telemetry from flight 42");

    let mut ms = 0f64;
    assert_eq!(nominal_dataset_created_at(dataset, &mut ms), 0);
    assert_eq!(ms, CREATED_AT_MS);

    assert_eq!(
        read_count(|c| nominal_dataset_property_count(dataset, c)).unwrap(),
        1
    );
    let key = read_string(|b, c, n| nominal_dataset_property_key_at(dataset, 0, b, c, n)).unwrap();
    let value =
        read_string(|b, c, n| nominal_dataset_property_value_at(dataset, 0, b, c, n)).unwrap();
    assert_eq!((key.as_str(), value.as_str()), ("vehicle", "rocket-1"));

    assert_eq!(
        read_count(|c| nominal_dataset_label_count(dataset, c)).unwrap(),
        1
    );
    let label = read_string(|b, c, n| nominal_dataset_label_at(dataset, 0, b, c, n)).unwrap();
    assert_eq!(label, "prod");

    // Out-of-range index fails with a code, not a panic.
    let err = read_string(|b, c, n| nominal_dataset_label_at(dataset, 1, b, c, n)).unwrap_err();
    assert_eq!(err, NominalErrorCode::IndexOutOfRange as i32);

    nominal_dataset_free(dataset);
    nominal_client_free(client);
}

#[test]
fn staged_create_sends_labels_properties_and_delimiter() {
    let _guard = common::message_lock();
    let server = start_server();
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/catalog/v1/datasets"))
            .and(body_partial_json(json!({
                "name": "Staged Telemetry",
                "labels": ["prod"],
                "properties": {"vehicle": "rocket-1"},
                "originMetadata": {
                    "channelConfig": {"prefixTreeDelimiter": "."}
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(full_dataset_json())),
    );
    let client = new_client(&server);

    let name = cstr("Staged Telemetry");
    let mut staging = 0i32;
    assert_eq!(nominal_dataset_create_begin(name.as_ptr(), &mut staging), 0);
    let description = cstr("desc");
    assert_eq!(
        nominal_dataset_create_set_description(staging, description.as_ptr()),
        0
    );
    let delimiter = cstr(".");
    assert_eq!(
        nominal_dataset_create_set_channel_delimiter(staging, delimiter.as_ptr()),
        0
    );
    let label = cstr("prod");
    assert_eq!(nominal_dataset_create_add_label(staging, label.as_ptr()), 0);
    let (key, value) = (cstr("vehicle"), cstr("rocket-1"));
    assert_eq!(
        nominal_dataset_create_set_property(staging, key.as_ptr(), value.as_ptr()),
        0
    );

    let mut dataset = 0i32;
    let code = nominal_dataset_create_commit(client, staging, &mut dataset);
    assert_eq!(code, 0, "staged create failed: {}", last_error());

    assert_eq!(nominal_dataset_create_free(staging), 0);
    assert_eq!(
        nominal_dataset_create_free(staging),
        NominalErrorCode::InvalidHandle as i32
    );

    nominal_dataset_free(dataset);
    nominal_client_free(client);
}

#[test]
fn get_dataset_by_rid_and_absent_description() {
    let server = start_server();
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/catalog/v1/datasets/multiple"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!([minimal_dataset_json(DATASET_RID, "Bare")])),
            ),
    );
    let client = new_client(&server);

    let rid = cstr(DATASET_RID);
    let mut dataset = 0i32;
    let code = nominal_dataset_get(client, rid.as_ptr(), &mut dataset);
    assert_eq!(code, 0, "get failed: {}", last_error());

    let name = read_string(|b, c, n| nominal_dataset_name(dataset, b, c, n)).unwrap();
    assert_eq!(name, "Bare");

    let mut is_present = true;
    let mut needed = 0u32;
    let code = nominal_dataset_description(
        dataset,
        std::ptr::null_mut(),
        0,
        &mut needed,
        &mut is_present,
    );
    assert_eq!((code, needed, is_present), (0, 0, false));
    assert_eq!(
        read_count(|c| nominal_dataset_label_count(dataset, c)).unwrap(),
        0
    );

    nominal_dataset_free(dataset);
    nominal_client_free(client);
}

#[test]
fn get_dataset_not_found_maps_to_not_found_code() {
    let _guard = common::message_lock();
    let server = start_server();
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/catalog/v1/datasets/multiple"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([]))),
    );
    let client = new_client(&server);

    let rid = cstr(DATASET_RID);
    let mut dataset = 0i32;
    assert_eq!(
        nominal_dataset_get(client, rid.as_ptr(), &mut dataset),
        NominalErrorCode::NotFound as i32
    );
    assert!(last_error().contains("not found"), "got: {}", last_error());

    nominal_client_free(client);
}

#[test]
fn list_and_search_datasets() {
    let server = start_server();
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/catalog/v1/search-datasets-v2"))
            .and(body_partial_json(json!({
                "query": {
                    "type": "and",
                    "and": [
                        {"type": "searchText", "searchText": "telemetry"},
                        {"type": "label", "label": "prod"}
                    ]
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "results": [minimal_dataset_json(DATASET_RID, "Telemetry")]
            }))),
    );
    let client = new_client(&server);

    let search_text = cstr("telemetry");
    let label = cstr("prod");
    let mut list = 0i32;
    let mut count = 0u32;
    let code = nominal_dataset_search(
        client,
        search_text.as_ptr(),
        label.as_ptr(),
        std::ptr::null(),
        std::ptr::null(),
        &mut list,
        &mut count,
    );
    assert_eq!(code, 0, "search failed: {}", last_error());
    assert_eq!(count, 1);

    let mut handle = 0i32;
    assert_eq!(nominal_handle_list_get(list, 0, &mut handle), 0);
    let name = read_string(|b, c, n| nominal_dataset_name(handle, b, c, n)).unwrap();
    assert_eq!(name, "Telemetry");

    nominal_dataset_free(handle);
    nominal_handle_list_free(list);
    nominal_client_free(client);
}

#[test]
fn list_datasets_returns_handle_list() {
    let server = start_server();
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/catalog/v1/search-datasets-v2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "results": [
                    minimal_dataset_json(DATASET_RID, "First"),
                    minimal_dataset_json(&DATASET_RID.replace("0002", "0003"), "Second")
                ]
            }))),
    );
    let client = new_client(&server);

    let mut list = 0i32;
    let mut count = 0u32;
    assert_eq!(nominal_dataset_list(client, &mut list, &mut count), 0);
    assert_eq!(count, 2);

    for i in 0..count as i32 {
        let mut handle = 0i32;
        assert_eq!(nominal_handle_list_get(list, i, &mut handle), 0);
        nominal_dataset_free(handle);
    }
    nominal_handle_list_free(list);
    nominal_client_free(client);
}

#[test]
fn staged_update_replaces_labels() {
    let server = start_server();
    mount(
        &server,
        Mock::given(method("PUT"))
            .and(path(format!("/catalog/v1/datasets/{DATASET_RID}")))
            .and(body_partial_json(json!({
                "labels": ["only-label"],
                "properties": {"phase": "post"}
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(full_dataset_json())),
    );
    let client = new_client(&server);

    let mut staging = 0i32;
    assert_eq!(nominal_dataset_update_begin(&mut staging), 0);
    let label = cstr("only-label");
    assert_eq!(nominal_dataset_update_add_label(staging, label.as_ptr()), 0);
    let (key, value) = (cstr("phase"), cstr("post"));
    assert_eq!(
        nominal_ffi::dataset::nominal_dataset_update_set_property(
            staging,
            key.as_ptr(),
            value.as_ptr()
        ),
        0
    );

    let rid = cstr(DATASET_RID);
    let mut updated = 0i32;
    let code = nominal_dataset_update_commit(client, rid.as_ptr(), staging, &mut updated);
    assert_eq!(code, 0, "staged update failed: {}", last_error());

    nominal_dataset_update_free(staging);
    nominal_dataset_free(updated);
    nominal_client_free(client);
}

#[test]
fn flat_update_with_nothing_to_change_is_rejected() {
    let _guard = common::message_lock();
    let server = start_server();
    let client = new_client(&server);

    let rid = cstr(DATASET_RID);
    let mut updated = 0i32;
    assert_eq!(
        nominal_dataset_update(
            client,
            rid.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            &mut updated
        ),
        NominalErrorCode::InvalidArgument as i32
    );

    nominal_client_free(client);
}

#[test]
fn archive_and_unarchive() {
    let server = start_server();
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path(format!("/catalog/v1/datasets/{DATASET_RID}/archive")))
            .respond_with(ResponseTemplate::new(204)),
    );
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path(format!(
                "/catalog/v1/datasets/{DATASET_RID}/unarchive"
            )))
            .respond_with(ResponseTemplate::new(204)),
    );
    let client = new_client(&server);

    let rid = cstr(DATASET_RID);
    assert_eq!(
        nominal_dataset_archive(client, rid.as_ptr()),
        0,
        "archive failed: {}",
        last_error()
    );
    assert_eq!(
        nominal_dataset_unarchive(client, rid.as_ptr()),
        0,
        "unarchive failed: {}",
        last_error()
    );

    nominal_client_free(client);
}

#[test]
fn dataset_calls_reject_invalid_handles() {
    let _guard = common::message_lock();
    let rid = cstr(DATASET_RID);
    let mut out = 0i32;
    assert_eq!(
        nominal_dataset_get(0, rid.as_ptr(), &mut out),
        NominalErrorCode::InvalidHandle as i32
    );
    let err = read_string(|b, c, n| nominal_dataset_name(999_999_999, b, c, n)).unwrap_err();
    assert_eq!(err, NominalErrorCode::InvalidHandle as i32);
}
