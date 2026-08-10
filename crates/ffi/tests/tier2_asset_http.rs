//! Tier 2: real request-building + response-parsing + FFI conversion against
//! a wiremock server speaking the Conjure wire format, including error paths
//! (4xx and malformed bodies must produce error codes, never panics).

mod common;

use common::{cstr, last_error, read_count, read_string, TEST_RT};
use serde_json::json;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use nominal_ffi::asset::{
    nominal_asset_add_connection, nominal_asset_add_dataset, nominal_asset_add_video,
    nominal_asset_archive, nominal_asset_attach_dataset_add_tag,
    nominal_asset_attach_dataset_begin, nominal_asset_attach_dataset_commit,
    nominal_asset_attach_dataset_free, nominal_asset_create, nominal_asset_create_add_label,
    nominal_asset_create_begin, nominal_asset_create_commit, nominal_asset_create_free,
    nominal_asset_create_set_description, nominal_asset_create_set_property,
    nominal_asset_created_at, nominal_asset_data_source_count, nominal_asset_data_source_name_at,
    nominal_asset_data_source_rid_at, nominal_asset_data_source_type_at, nominal_asset_description,
    nominal_asset_free, nominal_asset_get, nominal_asset_label_at, nominal_asset_label_count,
    nominal_asset_list, nominal_asset_name, nominal_asset_property_count,
    nominal_asset_property_key_at, nominal_asset_property_value_at, nominal_asset_rid,
    nominal_asset_search, nominal_asset_unarchive, nominal_asset_update,
    nominal_asset_update_add_label, nominal_asset_update_begin, nominal_asset_update_commit,
    nominal_asset_update_free, nominal_asset_update_set_property, nominal_asset_url,
};
use nominal_ffi::client::{nominal_client_free, nominal_client_new};
use nominal_ffi::error::NominalErrorCode;
use nominal_ffi::handles::{
    nominal_handle_list_count, nominal_handle_list_free, nominal_handle_list_get,
};

const ASSET_RID: &str = "ri.scout.cerulean-staging.asset.00000000-0000-0000-0000-000000000001";
const DATASET_RID: &str =
    "ri.catalog.cerulean-staging.dataset.00000000-0000-0000-0000-000000000002";
const VIDEO_RID: &str = "ri.catalog.cerulean-staging.video.00000000-0000-0000-0000-000000000003";
const CONNECTION_RID: &str =
    "ri.datasource.cerulean-staging.connection.00000000-0000-0000-0000-000000000004";
// 2024-01-15T10:30:00Z as Unix milliseconds (double — exact, well below 2^53).
const CREATED_AT_MILLIS: f64 = 1_705_314_600_000.0;

/// A full Conjure-format asset with every field this FFI exposes.
fn full_asset_json() -> serde_json::Value {
    json!({
        "rid": ASSET_RID,
        "title": "Flight 42",
        "description": "Qualification flight",
        "properties": {"vehicle": "rocket-1", "engine": "m9"},
        "labels": ["flight", "prod"],
        "dataScopes": [
            {
                "dataScopeName": "flight-data",
                "dataSource": {"type": "dataset", "dataset": DATASET_RID},
                "timestampType": "ABSOLUTE"
            },
            {
                "dataScopeName": "cockpit-cam",
                "dataSource": {"type": "video", "video": VIDEO_RID},
                "timestampType": "ABSOLUTE"
            }
        ],
        "createdAt": "2024-01-15T10:30:00Z",
        "updatedAt": "2024-01-15T11:00:00Z",
        "isStaged": false,
        "isArchived": false
    })
}

/// A minimal asset (only required Conjure fields), for optional-field checks.
fn minimal_asset_json(rid: &str, title: &str) -> serde_json::Value {
    json!({
        "rid": rid,
        "title": title,
        "createdAt": "2024-01-15T10:30:00Z",
        "updatedAt": "2024-01-15T10:30:00Z",
        "isStaged": false,
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

fn create_asset(client: i32, name: &str, description: Option<&str>) -> Result<i32, i32> {
    let name = cstr(name);
    let description = description.map(cstr);
    let mut asset = 0i32;
    let code = nominal_asset_create(
        client,
        name.as_ptr(),
        description
            .as_ref()
            .map_or(std::ptr::null(), |s| s.as_ptr()),
        &mut asset,
    );
    if code == 0 {
        Ok(asset)
    } else {
        Err(code)
    }
}

/// Reads the description via the size-query pattern, returning
/// (code, needed, is_present).
fn query_description(asset: i32) -> (i32, u32, bool) {
    let mut needed = 0u32;
    let mut is_present = false;
    let code =
        nominal_asset_description(asset, std::ptr::null_mut(), 0, &mut needed, &mut is_present);
    (code, needed, is_present)
}

#[test]
fn create_asset_marshals_every_field() {
    let _guard = common::message_lock();
    let server = start_server();
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/scout/v1/asset"))
            .and(body_partial_json(json!({
                "title": "Flight 42",
                "description": "Qualification flight"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(full_asset_json())),
    );
    let client = new_client(&server);

    let asset = create_asset(client, "Flight 42", Some("Qualification flight"))
        .unwrap_or_else(|code| panic!("create failed ({code}): {}", last_error()));

    // Scalar fields.
    let rid = read_string(|b, c, n| nominal_asset_rid(asset, b, c, n)).unwrap();
    assert_eq!(rid, ASSET_RID);
    let name = read_string(|b, c, n| nominal_asset_name(asset, b, c, n)).unwrap();
    assert_eq!(name, "Flight 42");
    let url = read_string(|b, c, n| nominal_asset_url(asset, b, c, n)).unwrap();
    assert!(url.ends_with(&format!("/assets/{ASSET_RID}")), "got: {url}");

    let mut is_present = false;
    let description =
        read_string(|b, c, n| nominal_asset_description(asset, b, c, n, &mut is_present)).unwrap();
    assert!(is_present);
    assert_eq!(description, "Qualification flight");

    let mut millis = 0f64;
    assert_eq!(nominal_asset_created_at(asset, &mut millis), 0);
    assert_eq!(millis, CREATED_AT_MILLIS);

    // Properties come back in sorted-key order: engine, then vehicle.
    assert_eq!(
        read_count(|c| nominal_asset_property_count(asset, c)).unwrap(),
        2
    );
    let keys_values: Vec<(String, String)> = (0..2)
        .map(|i| {
            (
                read_string(|b, c, n| nominal_asset_property_key_at(asset, i, b, c, n)).unwrap(),
                read_string(|b, c, n| nominal_asset_property_value_at(asset, i, b, c, n)).unwrap(),
            )
        })
        .collect();
    assert_eq!(
        keys_values,
        vec![
            ("engine".into(), "m9".into()),
            ("vehicle".into(), "rocket-1".into())
        ]
    );

    // Labels.
    assert_eq!(
        read_count(|c| nominal_asset_label_count(asset, c)).unwrap(),
        2
    );
    let labels: Vec<String> = (0..2)
        .map(|i| read_string(|b, c, n| nominal_asset_label_at(asset, i, b, c, n)).unwrap())
        .collect();
    assert_eq!(labels, vec!["flight", "prod"]);

    // Data sources in sorted-scope-name order: cockpit-cam, then flight-data.
    assert_eq!(
        read_count(|c| nominal_asset_data_source_count(asset, c)).unwrap(),
        2
    );
    let name0 =
        read_string(|b, c, n| nominal_asset_data_source_name_at(asset, 0, b, c, n)).unwrap();
    assert_eq!(name0, "cockpit-cam");
    let rid0 = read_string(|b, c, n| nominal_asset_data_source_rid_at(asset, 0, b, c, n)).unwrap();
    assert_eq!(rid0, VIDEO_RID);
    let mut kind = -1i32;
    assert_eq!(nominal_asset_data_source_type_at(asset, 0, &mut kind), 0);
    assert_eq!(kind, 1, "video = 1");
    let name1 =
        read_string(|b, c, n| nominal_asset_data_source_name_at(asset, 1, b, c, n)).unwrap();
    assert_eq!(name1, "flight-data");
    assert_eq!(nominal_asset_data_source_type_at(asset, 1, &mut kind), 0);
    assert_eq!(kind, 0, "dataset = 0");

    // Out-of-range indices fail with a code, not a panic.
    let err = read_string(|b, c, n| nominal_asset_label_at(asset, 2, b, c, n)).unwrap_err();
    assert_eq!(err, NominalErrorCode::IndexOutOfRange as i32);
    let err = read_string(|b, c, n| nominal_asset_property_key_at(asset, -1, b, c, n)).unwrap_err();
    assert_eq!(err, NominalErrorCode::IndexOutOfRange as i32);

    // Freed handles become invalid.
    assert_eq!(nominal_asset_free(asset), 0);
    let err = read_string(|b, c, n| nominal_asset_name(asset, b, c, n)).unwrap_err();
    assert_eq!(err, NominalErrorCode::InvalidHandle as i32);

    nominal_client_free(client);
}

#[test]
fn minimal_asset_reports_absent_optionals() {
    let server = start_server();
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/scout/v1/asset"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(minimal_asset_json(ASSET_RID, "Bare")),
            ),
    );
    let client = new_client(&server);
    let asset = create_asset(client, "Bare", None).unwrap();

    let (code, needed, is_present) = query_description(asset);
    assert_eq!((code, needed, is_present), (0, 0, false));
    assert_eq!(
        read_count(|c| nominal_asset_property_count(asset, c)).unwrap(),
        0
    );
    assert_eq!(
        read_count(|c| nominal_asset_label_count(asset, c)).unwrap(),
        0
    );
    assert_eq!(
        read_count(|c| nominal_asset_data_source_count(asset, c)).unwrap(),
        0
    );

    nominal_asset_free(asset);
    nominal_client_free(client);
}

#[test]
fn get_asset_by_rid() {
    let server = start_server();
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/scout/v1/asset/multiple"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({ ASSET_RID: full_asset_json() })),
            ),
    );
    let client = new_client(&server);

    let rid = cstr(ASSET_RID);
    let mut asset = 0i32;
    let code = nominal_asset_get(client, rid.as_ptr(), &mut asset);
    assert_eq!(code, 0, "get failed: {}", last_error());
    let name = read_string(|b, c, n| nominal_asset_name(asset, b, c, n)).unwrap();
    assert_eq!(name, "Flight 42");

    nominal_asset_free(asset);
    nominal_client_free(client);
}

#[test]
fn get_asset_not_found_maps_to_not_found_code() {
    let _guard = common::message_lock();
    let server = start_server();
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/scout/v1/asset/multiple"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({}))),
    );
    let client = new_client(&server);

    let rid = cstr(ASSET_RID);
    let mut asset = 0i32;
    let code = nominal_asset_get(client, rid.as_ptr(), &mut asset);
    assert_eq!(code, NominalErrorCode::NotFound as i32);
    assert!(last_error().contains("not found"), "got: {}", last_error());

    nominal_client_free(client);
}

#[test]
fn invalid_rid_fails_before_any_request() {
    let _guard = common::message_lock();
    // No mocks mounted: a request would fail differently than InvalidArgument.
    let server = start_server();
    let client = new_client(&server);

    let rid = cstr("not-a-rid");
    let mut asset = 0i32;
    let code = nominal_asset_get(client, rid.as_ptr(), &mut asset);
    assert_eq!(code, NominalErrorCode::InvalidArgument as i32);

    nominal_client_free(client);
}

/// Matches only first-page requests (no nextPageToken in the body).
struct FirstPageOnly;

impl wiremock::Match for FirstPageOnly {
    fn matches(&self, request: &Request) -> bool {
        serde_json::from_slice::<serde_json::Value>(&request.body)
            .map(|body| body.get("nextPageToken").is_none())
            .unwrap_or(false)
    }
}

#[test]
fn list_assets_follows_pagination() {
    let rid2 = ASSET_RID.replace("0001", "9999");
    let server = start_server();
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/scout/v1/search-assets"))
            .and(body_partial_json(json!({"nextPageToken": "page-2"})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "results": [minimal_asset_json(&rid2, "Second")]
            }))),
    );
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/scout/v1/search-assets"))
            .and(FirstPageOnly)
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "results": [minimal_asset_json(ASSET_RID, "First")],
                "nextPageToken": "page-2"
            }))),
    );
    let client = new_client(&server);

    let mut list = 0i32;
    let mut count: u32 = 0;
    let code = nominal_asset_list(client, &mut list, &mut count);
    assert_eq!(code, 0, "list failed: {}", last_error());
    assert_eq!(count, 2);

    let mut list_count = 0u32;
    assert_eq!(nominal_handle_list_count(list, &mut list_count), 0);
    assert_eq!(list_count, count, "out_count and list count must agree");

    let handles: Vec<i32> = (0..count as i32)
        .map(|i| {
            let mut handle = 0i32;
            assert_eq!(nominal_handle_list_get(list, i, &mut handle), 0);
            handle
        })
        .collect();
    let names: Vec<String> = handles
        .iter()
        .map(|&h| read_string(|b, c, n| nominal_asset_name(h, b, c, n)).unwrap())
        .collect();
    assert_eq!(names, vec!["First", "Second"]);

    assert_eq!(nominal_handle_list_free(list), 0);
    for handle in handles {
        assert_eq!(nominal_asset_free(handle), 0, "handles outlive the list");
    }
    nominal_client_free(client);
}

fn search(
    client: i32,
    search_text: Option<&str>,
    label: Option<&str>,
    property: Option<(&str, &str)>,
) -> Result<Vec<i32>, i32> {
    let search_text = search_text.map(cstr);
    let label = label.map(cstr);
    let property_key = property.map(|(k, _)| cstr(k));
    let property_value = property.map(|(_, v)| cstr(v));
    let as_ptr =
        |opt: &Option<std::ffi::CString>| opt.as_ref().map_or(std::ptr::null(), |s| s.as_ptr());

    let mut list = 0i32;
    let mut count: u32 = 0;
    let code = nominal_asset_search(
        client,
        as_ptr(&search_text),
        as_ptr(&label),
        as_ptr(&property_key),
        as_ptr(&property_value),
        &mut list,
        &mut count,
    );
    if code != 0 {
        return Err(code);
    }
    let handles = (0..count as i32)
        .map(|i| {
            let mut handle = 0i32;
            assert_eq!(nominal_handle_list_get(list, i, &mut handle), 0);
            handle
        })
        .collect();
    nominal_handle_list_free(list);
    Ok(handles)
}

#[test]
fn search_sends_combined_filters() {
    let server = start_server();
    // Only respond when the request carries the AND of all three filters —
    // a request with the wrong query shape gets no match and fails the call.
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/scout/v1/search-assets"))
            .and(body_partial_json(json!({
                "query": {
                    "type": "and",
                    "and": [
                        {"type": "searchText", "searchText": "flight"},
                        {"type": "labels", "labels": {"labels": ["prod"], "operator": "OR"}},
                        {"type": "properties", "properties": {"name": "vehicle", "values": ["rocket-1"]}}
                    ]
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "results": [minimal_asset_json(ASSET_RID, "Flight 42")]
            }))),
    );
    let client = new_client(&server);

    let handles = search(
        client,
        Some("flight"),
        Some("prod"),
        Some(("vehicle", "rocket-1")),
    )
    .unwrap_or_else(|code| panic!("search failed ({code}): {}", last_error()));
    assert_eq!(handles.len(), 1);

    for handle in handles {
        nominal_asset_free(handle);
    }
    nominal_client_free(client);
}

#[test]
fn search_property_key_without_value_is_rejected() {
    let _guard = common::message_lock();
    let server = start_server();
    let client = new_client(&server);

    let key = cstr("vehicle");
    let mut list = 0i32;
    let mut count: u32 = 0;
    let code = nominal_asset_search(
        client,
        std::ptr::null(),
        std::ptr::null(),
        key.as_ptr(),
        std::ptr::null(),
        &mut list,
        &mut count,
    );
    assert_eq!(code, NominalErrorCode::InvalidArgument as i32);

    nominal_client_free(client);
}

#[test]
fn update_asset_sends_only_set_fields() {
    let server = start_server();
    mount(
        &server,
        Mock::given(method("PUT"))
            .and(path(format!("/scout/v1/asset/{ASSET_RID}")))
            .and(body_partial_json(json!({"title": "Renamed"})))
            .respond_with(ResponseTemplate::new(200).set_body_json({
                let mut asset = full_asset_json();
                asset["title"] = json!("Renamed");
                asset
            })),
    );
    let client = new_client(&server);

    let rid = cstr(ASSET_RID);
    let name = cstr("Renamed");
    let mut updated = 0i32;
    let code = nominal_asset_update(
        client,
        rid.as_ptr(),
        name.as_ptr(),
        std::ptr::null(),
        &mut updated,
    );
    assert_eq!(code, 0, "update failed: {}", last_error());
    let new_name = read_string(|b, c, n| nominal_asset_name(updated, b, c, n)).unwrap();
    assert_eq!(new_name, "Renamed");

    nominal_asset_free(updated);
    nominal_client_free(client);
}

#[test]
fn update_with_nothing_to_change_is_rejected() {
    let _guard = common::message_lock();
    let server = start_server();
    let client = new_client(&server);

    let rid = cstr(ASSET_RID);
    let mut updated = 0i32;
    let code = nominal_asset_update(
        client,
        rid.as_ptr(),
        std::ptr::null(),
        std::ptr::null(),
        &mut updated,
    );
    assert_eq!(code, NominalErrorCode::InvalidArgument as i32);

    nominal_client_free(client);
}

#[test]
fn archive_and_unarchive() {
    let server = start_server();
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path(format!("/scout/v1/archive/{ASSET_RID}")))
            .respond_with(ResponseTemplate::new(204)),
    );
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path(format!("/scout/v1/unarchive/{ASSET_RID}")))
            .respond_with(ResponseTemplate::new(204)),
    );
    let client = new_client(&server);

    let rid = cstr(ASSET_RID);
    assert_eq!(
        nominal_asset_archive(client, rid.as_ptr()),
        0,
        "archive failed: {}",
        last_error()
    );
    assert_eq!(
        nominal_asset_unarchive(client, rid.as_ptr()),
        0,
        "unarchive failed: {}",
        last_error()
    );

    nominal_client_free(client);
}

#[test]
fn http_403_maps_to_api_error_not_panic() {
    let _guard = common::message_lock();
    let server = start_server();
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/scout/v1/asset"))
            .respond_with(ResponseTemplate::new(403).set_body_json(json!({
                "errorCode": "PERMISSION_DENIED",
                "errorName": "Default:PermissionDenied",
                "errorInstanceId": "00000000-0000-0000-0000-000000000000",
                "parameters": {}
            }))),
    );
    let client = new_client(&server);

    let code = create_asset(client, "Denied", None).unwrap_err();
    assert_eq!(code, NominalErrorCode::ApiError as i32);
    assert!(!last_error().is_empty());

    nominal_client_free(client);
}

#[test]
fn malformed_response_body_maps_to_error_not_panic() {
    let _guard = common::message_lock();
    let server = start_server();
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/scout/v1/asset"))
            .respond_with(ResponseTemplate::new(200).set_body_string("this is not json {{{")),
    );
    let client = new_client(&server);

    let code = create_asset(client, "Garbled", None).unwrap_err();
    assert!(code > 0, "expected an error code, got {code}");
    assert_ne!(
        code,
        NominalErrorCode::Panic as i32,
        "malformed body must not panic: {}",
        last_error()
    );

    nominal_client_free(client);
}

#[test]
fn staged_create_sends_labels_and_properties() {
    let server = start_server();
    // The mock only matches when the request body carries everything staged —
    // wrong or missing fields mean no match and a failed call.
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/scout/v1/asset"))
            .and(body_partial_json(json!({
                "title": "Flight 43",
                "description": "Staged flight",
                "labels": ["flight", "prod"],
                "properties": {"engine": "m9", "vehicle": "rocket-2"}
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(full_asset_json())),
    );
    let client = new_client(&server);

    let name = cstr("Flight 43");
    let mut staging = 0i32;
    assert_eq!(nominal_asset_create_begin(name.as_ptr(), &mut staging), 0);
    assert!(staging > 0);

    let description = cstr("Staged flight");
    assert_eq!(
        nominal_asset_create_set_description(staging, description.as_ptr()),
        0
    );
    for label in ["prod", "flight"] {
        let label = cstr(label);
        assert_eq!(nominal_asset_create_add_label(staging, label.as_ptr()), 0);
    }
    for (key, value) in [("vehicle", "rocket-1"), ("engine", "m9")] {
        let (key, value) = (cstr(key), cstr(value));
        assert_eq!(
            nominal_asset_create_set_property(staging, key.as_ptr(), value.as_ptr()),
            0
        );
    }
    // Same key overwrites — the request must carry rocket-2, not rocket-1.
    let (key, value) = (cstr("vehicle"), cstr("rocket-2"));
    assert_eq!(
        nominal_asset_create_set_property(staging, key.as_ptr(), value.as_ptr()),
        0
    );

    let mut asset = 0i32;
    let code = nominal_asset_create_commit(client, staging, &mut asset);
    assert_eq!(code, 0, "staged create failed: {}", last_error());

    assert_eq!(nominal_asset_create_free(staging), 0);
    assert_eq!(
        nominal_asset_create_free(staging),
        NominalErrorCode::InvalidHandle as i32,
        "double free of staging must fail"
    );

    nominal_asset_free(asset);
    nominal_client_free(client);
}

#[test]
fn staged_create_rejects_empty_name_and_label() {
    let mut staging = 0i32;
    let empty = cstr("");
    assert_eq!(
        nominal_asset_create_begin(empty.as_ptr(), &mut staging),
        NominalErrorCode::InvalidArgument as i32
    );

    let name = cstr("Valid");
    assert_eq!(nominal_asset_create_begin(name.as_ptr(), &mut staging), 0);
    assert_eq!(
        nominal_asset_create_add_label(staging, empty.as_ptr()),
        NominalErrorCode::InvalidArgument as i32
    );
    let value = cstr("v");
    assert_eq!(
        nominal_asset_create_set_property(staging, empty.as_ptr(), value.as_ptr()),
        NominalErrorCode::InvalidArgument as i32
    );
    nominal_asset_create_free(staging);
}

#[test]
fn staged_update_replaces_labels_and_properties() {
    let server = start_server();
    mount(
        &server,
        Mock::given(method("PUT"))
            .and(path(format!("/scout/v1/asset/{ASSET_RID}")))
            .and(body_partial_json(json!({
                "labels": ["only-label"],
                "properties": {"phase": "descent"}
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(full_asset_json())),
    );
    let client = new_client(&server);

    let mut staging = 0i32;
    assert_eq!(nominal_asset_update_begin(&mut staging), 0);
    let label = cstr("only-label");
    assert_eq!(nominal_asset_update_add_label(staging, label.as_ptr()), 0);
    let (key, value) = (cstr("phase"), cstr("descent"));
    assert_eq!(
        nominal_asset_update_set_property(staging, key.as_ptr(), value.as_ptr()),
        0
    );

    let rid = cstr(ASSET_RID);
    let mut updated = 0i32;
    let code = nominal_asset_update_commit(client, rid.as_ptr(), staging, &mut updated);
    assert_eq!(code, 0, "staged update failed: {}", last_error());

    nominal_asset_update_free(staging);
    nominal_asset_free(updated);
    nominal_client_free(client);
}

#[test]
fn staged_update_with_no_fields_is_rejected() {
    let _guard = common::message_lock();
    let server = start_server();
    let client = new_client(&server);

    let mut staging = 0i32;
    assert_eq!(nominal_asset_update_begin(&mut staging), 0);
    let rid = cstr(ASSET_RID);
    let mut updated = 0i32;
    assert_eq!(
        nominal_asset_update_commit(client, rid.as_ptr(), staging, &mut updated),
        NominalErrorCode::InvalidArgument as i32
    );
    assert!(last_error().contains("no fields"), "got: {}", last_error());

    nominal_asset_update_free(staging);
    nominal_client_free(client);
}

#[test]
fn staging_calls_reject_invalid_staging_handle() {
    let _guard = common::message_lock();
    let label = cstr("x");
    assert_eq!(
        nominal_asset_create_add_label(0, label.as_ptr()),
        NominalErrorCode::InvalidHandle as i32
    );
    assert_eq!(
        nominal_asset_update_add_label(987_654_321, label.as_ptr()),
        NominalErrorCode::InvalidHandle as i32
    );
}

#[test]
fn add_dataset_sends_data_scope() {
    let server = start_server();
    // The mock only matches when the request carries exactly the one data
    // scope, dataset-typed, under the scope name — wrong shape, no match.
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path(format!("/scout/v1/asset/{ASSET_RID}/data-sources")))
            .and(body_partial_json(json!({
                "dataScopes": [{
                    "dataScopeName": "flight-data",
                    "dataSource": {"type": "dataset", "dataset": DATASET_RID}
                }]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(full_asset_json())),
    );
    let client = new_client(&server);

    let rid = cstr(ASSET_RID);
    let scope_name = cstr("flight-data");
    let dataset_rid = cstr(DATASET_RID);
    let mut updated = 0i32;
    let code = nominal_asset_add_dataset(
        client,
        rid.as_ptr(),
        scope_name.as_ptr(),
        dataset_rid.as_ptr(),
        &mut updated,
    );
    assert_eq!(code, 0, "add_dataset failed: {}", last_error());

    // The updated asset comes back with its data scopes readable.
    assert_eq!(
        read_count(|c| nominal_asset_data_source_count(updated, c)).unwrap(),
        2
    );

    nominal_asset_free(updated);
    nominal_client_free(client);
}

#[test]
fn add_video_and_connection_send_typed_sources() {
    let server = start_server();
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path(format!("/scout/v1/asset/{ASSET_RID}/data-sources")))
            .and(body_partial_json(json!({
                "dataScopes": [{
                    "dataScopeName": "cockpit-cam",
                    "dataSource": {"type": "video", "video": VIDEO_RID}
                }]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(full_asset_json())),
    );
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path(format!("/scout/v1/asset/{ASSET_RID}/data-sources")))
            .and(body_partial_json(json!({
                "dataScopes": [{
                    "dataScopeName": "telemetry",
                    "dataSource": {"type": "connection", "connection": CONNECTION_RID}
                }]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(full_asset_json())),
    );
    let client = new_client(&server);
    let rid = cstr(ASSET_RID);

    let scope_name = cstr("cockpit-cam");
    let video_rid = cstr(VIDEO_RID);
    let mut updated = 0i32;
    let code = nominal_asset_add_video(
        client,
        rid.as_ptr(),
        scope_name.as_ptr(),
        video_rid.as_ptr(),
        &mut updated,
    );
    assert_eq!(code, 0, "add_video failed: {}", last_error());
    nominal_asset_free(updated);

    let scope_name = cstr("telemetry");
    let connection_rid = cstr(CONNECTION_RID);
    let code = nominal_asset_add_connection(
        client,
        rid.as_ptr(),
        scope_name.as_ptr(),
        connection_rid.as_ptr(),
        &mut updated,
    );
    assert_eq!(code, 0, "add_connection failed: {}", last_error());
    nominal_asset_free(updated);

    nominal_client_free(client);
}

#[test]
fn staged_attach_sends_series_tags() {
    let server = start_server();
    // Tags must arrive with the same-key overwrite applied: rocket-2, not
    // rocket-1.
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path(format!("/scout/v1/asset/{ASSET_RID}/data-sources")))
            .and(body_partial_json(json!({
                "dataScopes": [{
                    "dataScopeName": "flight-data",
                    "dataSource": {"type": "dataset", "dataset": DATASET_RID},
                    "seriesTags": {"phase": "ascent", "vehicle": "rocket-2"}
                }]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(full_asset_json())),
    );
    let client = new_client(&server);

    let scope_name = cstr("flight-data");
    let dataset_rid = cstr(DATASET_RID);
    let mut staging = 0i32;
    assert_eq!(
        nominal_asset_attach_dataset_begin(scope_name.as_ptr(), dataset_rid.as_ptr(), &mut staging),
        0
    );
    for (key, value) in [
        ("vehicle", "rocket-1"),
        ("phase", "ascent"),
        ("vehicle", "rocket-2"),
    ] {
        let (key, value) = (cstr(key), cstr(value));
        assert_eq!(
            nominal_asset_attach_dataset_add_tag(staging, key.as_ptr(), value.as_ptr()),
            0
        );
    }

    let rid = cstr(ASSET_RID);
    let mut updated = 0i32;
    let code = nominal_asset_attach_dataset_commit(client, rid.as_ptr(), staging, &mut updated);
    assert_eq!(code, 0, "staged attach failed: {}", last_error());

    assert_eq!(nominal_asset_attach_dataset_free(staging), 0);
    assert_eq!(
        nominal_asset_attach_dataset_free(staging),
        NominalErrorCode::InvalidHandle as i32,
        "double free of staging must fail"
    );

    nominal_asset_free(updated);
    nominal_client_free(client);
}

#[test]
fn attach_begin_rejects_empty_arguments() {
    let _guard = common::message_lock();
    let empty = cstr("");
    let scope_name = cstr("flight-data");
    let dataset_rid = cstr(DATASET_RID);
    let mut staging = 0i32;
    assert_eq!(
        nominal_asset_attach_dataset_begin(empty.as_ptr(), dataset_rid.as_ptr(), &mut staging),
        NominalErrorCode::InvalidArgument as i32
    );
    assert_eq!(
        nominal_asset_attach_dataset_begin(scope_name.as_ptr(), empty.as_ptr(), &mut staging),
        NominalErrorCode::InvalidArgument as i32
    );
}

#[test]
fn add_dataset_conflict_maps_to_api_error() {
    let _guard = common::message_lock();
    let server = start_server();
    // The server rejects a scope name the asset already has.
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path(format!("/scout/v1/asset/{ASSET_RID}/data-sources")))
            .respond_with(ResponseTemplate::new(409).set_body_json(json!({
                "errorCode": "CONFLICT",
                "errorName": "Scout:DataScopeNameAlreadyExists",
                "errorInstanceId": "00000000-0000-0000-0000-000000000000",
                "parameters": {}
            }))),
    );
    let client = new_client(&server);

    let rid = cstr(ASSET_RID);
    let scope_name = cstr("flight-data");
    let dataset_rid = cstr(DATASET_RID);
    let mut updated = 0i32;
    let code = nominal_asset_add_dataset(
        client,
        rid.as_ptr(),
        scope_name.as_ptr(),
        dataset_rid.as_ptr(),
        &mut updated,
    );
    assert_eq!(code, NominalErrorCode::ApiError as i32);
    assert!(!last_error().is_empty());

    nominal_client_free(client);
}

#[test]
fn asset_calls_reject_invalid_client_handle() {
    let _guard = common::message_lock();
    let rid = cstr(ASSET_RID);
    let mut out = 0i32;
    assert_eq!(
        nominal_asset_get(0, rid.as_ptr(), &mut out),
        NominalErrorCode::InvalidHandle as i32
    );
    assert_eq!(
        nominal_asset_archive(12_345_678, rid.as_ptr()),
        NominalErrorCode::InvalidHandle as i32
    );
}
