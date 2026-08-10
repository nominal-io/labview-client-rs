//! Tier 2 for channels: request-building + response-parsing + FFI conversion
//! against a wiremock server speaking the Conjure wire format. Channels span
//! three services (data-source search, channel-metadata get, series-archetype
//! upsert) — that routing plus the data-type enum mapping is the ground
//! covered here.

mod common;

use common::{cstr, last_error, read_string, TEST_RT};
use serde_json::json;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use nominal_ffi::channel::{
    nominal_channel_data_source_rid, nominal_channel_data_type, nominal_channel_description,
    nominal_channel_free, nominal_channel_get, nominal_channel_list, nominal_channel_name,
    nominal_channel_search, nominal_channel_set_metadata, nominal_channel_unit,
};
use nominal_ffi::client::{nominal_client_free, nominal_client_new};
use nominal_ffi::error::NominalErrorCode;
use nominal_ffi::handles::{nominal_handle_list_free, nominal_handle_list_get};

const DATASET_RID: &str =
    "ri.catalog.cerulean-staging.dataset.00000000-0000-0000-0000-000000000002";

/// A channel as returned by the data-source search endpoint.
fn search_channel_json(name: &str, data_type: &str) -> serde_json::Value {
    json!({
        "name": name,
        "dataSource": DATASET_RID,
        "unit": {"symbol": "degC"},
        "description": "engine temperature",
        "dataType": data_type
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
fn list_channels_marshals_every_field() {
    let server = start_server();
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/data-source/v1/data-sources/search-channels"))
            .and(body_partial_json(json!({
                "dataSources": [DATASET_RID]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "results": [search_channel_json("engine.temp", "DOUBLE")]
            }))),
    );
    let client = new_client(&server);

    let rid = cstr(DATASET_RID);
    let mut list = 0i32;
    let mut count = 0u32;
    let code = nominal_channel_list(client, rid.as_ptr(), &mut list, &mut count);
    assert_eq!(code, 0, "list failed: {}", last_error());
    assert_eq!(count, 1);

    let mut channel = 0i32;
    assert_eq!(nominal_handle_list_get(list, 0, &mut channel), 0);

    let name = read_string(|b, c, n| nominal_channel_name(channel, b, c, n)).unwrap();
    assert_eq!(name, "engine.temp");
    let ds_rid = read_string(|b, c, n| nominal_channel_data_source_rid(channel, b, c, n)).unwrap();
    assert_eq!(ds_rid, DATASET_RID);

    let mut is_present = false;
    let description =
        read_string(|b, c, n| nominal_channel_description(channel, b, c, n, &mut is_present))
            .unwrap();
    assert!(is_present);
    assert_eq!(description, "engine temperature");
    let unit =
        read_string(|b, c, n| nominal_channel_unit(channel, b, c, n, &mut is_present)).unwrap();
    assert!(is_present);
    assert_eq!(unit, "degC");

    let mut data_type = -1i32;
    assert_eq!(nominal_channel_data_type(channel, &mut data_type), 0);
    assert_eq!(data_type, 0, "DOUBLE = 0");

    nominal_channel_free(channel);
    nominal_handle_list_free(list);
    nominal_client_free(client);
}

#[test]
fn search_sends_substring_and_data_type_filters() {
    let server = start_server();
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/data-source/v1/data-sources/search-channels"))
            .and(body_partial_json(json!({
                "dataSources": [DATASET_RID],
                "exactMatch": ["temp"],
                "dataTypes": ["DOUBLE"]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "results": [search_channel_json("engine.temp", "DOUBLE")]
            }))),
    );
    let client = new_client(&server);

    let rid = cstr(DATASET_RID);
    let substring = cstr("temp");
    let mut list = 0i32;
    let mut count = 0u32;
    let code = nominal_channel_search(
        client,
        rid.as_ptr(),
        substring.as_ptr(),
        0, // Double
        &mut list,
        &mut count,
    );
    assert_eq!(code, 0, "search failed: {}", last_error());
    assert_eq!(count, 1);

    let mut channel = 0i32;
    nominal_handle_list_get(list, 0, &mut channel);
    nominal_channel_free(channel);
    nominal_handle_list_free(list);
    nominal_client_free(client);
}

#[test]
fn search_client_side_substring_filter_applies() {
    // The substring also filters client-side (exactMatch is only a server
    // hint) — a result NOT containing the substring must be dropped.
    let server = start_server();
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/data-source/v1/data-sources/search-channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "results": [
                    search_channel_json("engine.temp", "DOUBLE"),
                    search_channel_json("engine.rpm", "DOUBLE")
                ]
            }))),
    );
    let client = new_client(&server);

    let rid = cstr(DATASET_RID);
    let substring = cstr("temp");
    let mut list = 0i32;
    let mut count = 0u32;
    let code = nominal_channel_search(
        client,
        rid.as_ptr(),
        substring.as_ptr(),
        -1,
        &mut list,
        &mut count,
    );
    assert_eq!(code, 0, "search failed: {}", last_error());
    assert_eq!(count, 1, "engine.rpm must be filtered out");

    let mut channel = 0i32;
    nominal_handle_list_get(list, 0, &mut channel);
    let name = read_string(|b, c, n| nominal_channel_name(channel, b, c, n)).unwrap();
    assert_eq!(name, "engine.temp");

    nominal_channel_free(channel);
    nominal_handle_list_free(list);
    nominal_client_free(client);
}

#[test]
fn search_rejects_invalid_data_type() {
    let _guard = common::message_lock();
    let server = start_server();
    let client = new_client(&server);

    let mut list = 0i32;
    let mut count = 0u32;
    for bad in [10, 11, -2] {
        assert_eq!(
            nominal_channel_search(
                client,
                std::ptr::null(),
                std::ptr::null(),
                bad,
                &mut list,
                &mut count
            ),
            NominalErrorCode::InvalidArgument as i32,
            "data_type {bad} must be rejected"
        );
    }

    nominal_client_free(client);
}

#[test]
fn get_channel_reads_stored_metadata() {
    let server = start_server();
    // The stored-metadata shape differs from search: identifier object, and
    // the unit is a bare string.
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/timeseries/channel-metadata/v1/channel-metadata/get"))
            .and(body_partial_json(json!({
                "channelIdentifier": {
                    "channelName": "engine.temp",
                    "dataSourceRid": DATASET_RID
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "channelIdentifier": {
                    "channelName": "engine.temp",
                    "dataSourceRid": DATASET_RID
                },
                "description": "engine temperature",
                "unit": "degC",
                "dataType": "DOUBLE"
            }))),
    );
    let client = new_client(&server);

    let rid = cstr(DATASET_RID);
    let name = cstr("engine.temp");
    let mut channel = 0i32;
    let code = nominal_channel_get(client, rid.as_ptr(), name.as_ptr(), &mut channel);
    assert_eq!(code, 0, "get failed: {}", last_error());

    let name = read_string(|b, c, n| nominal_channel_name(channel, b, c, n)).unwrap();
    assert_eq!(name, "engine.temp");
    let mut is_present = false;
    let unit =
        read_string(|b, c, n| nominal_channel_unit(channel, b, c, n, &mut is_present)).unwrap();
    assert!(is_present);
    assert_eq!(unit, "degC");
    let mut data_type = -1i32;
    assert_eq!(nominal_channel_data_type(channel, &mut data_type), 0);
    assert_eq!(data_type, 0);

    nominal_channel_free(channel);
    nominal_client_free(client);
}

#[test]
fn set_metadata_sends_upsert_and_reflects_update() {
    let server = start_server();
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path(
                "/timeseries/archetype/v1/series-archetype/create-or-update",
            ))
            .and(body_partial_json(json!({
                "channel": "engine.temp",
                "dataSourceRid": DATASET_RID,
                "description": "engine temperature",
                "unit": "degC"
            })))
            .respond_with(ResponseTemplate::new(204)),
    );
    let client = new_client(&server);

    let rid = cstr(DATASET_RID);
    let name = cstr("engine.temp");
    let description = cstr("engine temperature");
    let unit = cstr("degC");
    let mut channel = 0i32;
    let code = nominal_channel_set_metadata(
        client,
        rid.as_ptr(),
        name.as_ptr(),
        0, // Double
        description.as_ptr(),
        unit.as_ptr(),
        false,
        &mut channel,
    );
    assert_eq!(code, 0, "set_metadata failed: {}", last_error());

    // The returned handle reflects what was just written.
    let got = read_string(|b, c, n| nominal_channel_name(channel, b, c, n)).unwrap();
    assert_eq!(got, "engine.temp");
    let mut is_present = false;
    let got_unit =
        read_string(|b, c, n| nominal_channel_unit(channel, b, c, n, &mut is_present)).unwrap();
    assert!(is_present);
    assert_eq!(got_unit, "degC");

    nominal_channel_free(channel);
    nominal_client_free(client);
}

#[test]
fn set_metadata_rejects_unit_conflict_and_bad_type() {
    let _guard = common::message_lock();
    let server = start_server();
    let client = new_client(&server);

    let rid = cstr(DATASET_RID);
    let name = cstr("engine.temp");
    let unit = cstr("degC");
    let mut channel = 0i32;

    // unit + clear_unit together.
    assert_eq!(
        nominal_channel_set_metadata(
            client,
            rid.as_ptr(),
            name.as_ptr(),
            0,
            std::ptr::null(),
            unit.as_ptr(),
            true,
            &mut channel
        ),
        NominalErrorCode::InvalidArgument as i32
    );
    assert!(
        last_error().contains("mutually exclusive"),
        "got: {}",
        last_error()
    );

    // Unknown (10) is never a valid input.
    assert_eq!(
        nominal_channel_set_metadata(
            client,
            rid.as_ptr(),
            name.as_ptr(),
            10,
            std::ptr::null(),
            std::ptr::null(),
            false,
            &mut channel
        ),
        NominalErrorCode::InvalidArgument as i32
    );

    nominal_client_free(client);
}

#[test]
fn channel_calls_reject_invalid_handles() {
    let _guard = common::message_lock();
    let rid = cstr(DATASET_RID);
    let mut list = 0i32;
    let mut count = 0u32;
    assert_eq!(
        nominal_channel_list(0, rid.as_ptr(), &mut list, &mut count),
        NominalErrorCode::InvalidHandle as i32
    );
    let err = read_string(|b, c, n| nominal_channel_name(999_999_999, b, c, n)).unwrap_err();
    assert_eq!(err, NominalErrorCode::InvalidHandle as i32);
}
