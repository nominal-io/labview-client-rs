//! Tier 2 for videos: request-building + response-parsing + FFI conversion
//! against a wiremock server speaking the Conjure wire format. Videos share
//! the dataset field surface but live on `/video/v1` with their own verbs
//! (archive/unarchive are PUT here) — that routing is the ground covered.

mod common;

use common::{cstr, last_error, read_count, read_string, TEST_RT};
use serde_json::json;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use nominal_ffi::client::{nominal_client_free, nominal_client_new};
use nominal_ffi::error::NominalErrorCode;
use nominal_ffi::handles::{nominal_handle_list_free, nominal_handle_list_get};
use nominal_ffi::video::{
    nominal_video_archive, nominal_video_create, nominal_video_create_add_label,
    nominal_video_create_begin, nominal_video_create_commit, nominal_video_create_free,
    nominal_video_create_set_description, nominal_video_create_set_property,
    nominal_video_created_at, nominal_video_description, nominal_video_free, nominal_video_get,
    nominal_video_label_at, nominal_video_label_count, nominal_video_list, nominal_video_name,
    nominal_video_property_count, nominal_video_property_key_at, nominal_video_property_value_at,
    nominal_video_rid, nominal_video_search, nominal_video_unarchive, nominal_video_update,
    nominal_video_update_add_label, nominal_video_update_begin, nominal_video_update_commit,
    nominal_video_update_free, nominal_video_url,
};

const VIDEO_RID: &str = "ri.catalog.cerulean-staging.video.00000000-0000-0000-0000-000000000003";
const CREATED_AT_MS: f64 = 1_705_314_600_000.0;

/// A full Conjure-format video with every field this FFI exposes.
fn full_video_json() -> serde_json::Value {
    json!({
        "rid": VIDEO_RID,
        "title": "Cockpit Cam",
        "description": "Front cockpit camera",
        "labels": ["prod"],
        "properties": {"camera": "front"},
        "createdBy": "ri.security.cerulean-staging.user.00000000-0000-0000-0000-0000000000aa",
        "createdAt": "2024-01-15T10:30:00Z",
        "isArchived": false
    })
}

/// A minimal video (required fields only), for optional-field checks.
fn minimal_video_json(rid: &str, title: &str) -> serde_json::Value {
    json!({
        "rid": rid,
        "title": title,
        "createdBy": "ri.security.cerulean-staging.user.00000000-0000-0000-0000-0000000000aa",
        "createdAt": "2024-01-15T10:30:00Z",
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
fn create_video_marshals_every_field() {
    let _guard = common::message_lock();
    let server = start_server();
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/video/v1/videos"))
            .and(body_partial_json(json!({
                "title": "Cockpit Cam",
                "description": "Front cockpit camera"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(full_video_json())),
    );
    let client = new_client(&server);

    let name = cstr("Cockpit Cam");
    let description = cstr("Front cockpit camera");
    let mut video = 0i32;
    let code = nominal_video_create(client, name.as_ptr(), description.as_ptr(), &mut video);
    assert_eq!(code, 0, "create failed: {}", last_error());

    let rid = read_string(|b, c, n| nominal_video_rid(video, b, c, n)).unwrap();
    assert_eq!(rid, VIDEO_RID);
    let name = read_string(|b, c, n| nominal_video_name(video, b, c, n)).unwrap();
    assert_eq!(name, "Cockpit Cam");
    let url = read_string(|b, c, n| nominal_video_url(video, b, c, n)).unwrap();
    assert!(
        url.ends_with(&format!("/data-sources/{VIDEO_RID}")),
        "got: {url}"
    );

    let mut is_present = false;
    let description =
        read_string(|b, c, n| nominal_video_description(video, b, c, n, &mut is_present)).unwrap();
    assert!(is_present);
    assert_eq!(description, "Front cockpit camera");

    let mut ms = 0f64;
    assert_eq!(nominal_video_created_at(video, &mut ms), 0);
    assert_eq!(ms, CREATED_AT_MS);

    assert_eq!(
        read_count(|c| nominal_video_property_count(video, c)).unwrap(),
        1
    );
    let key = read_string(|b, c, n| nominal_video_property_key_at(video, 0, b, c, n)).unwrap();
    let value = read_string(|b, c, n| nominal_video_property_value_at(video, 0, b, c, n)).unwrap();
    assert_eq!((key.as_str(), value.as_str()), ("camera", "front"));

    assert_eq!(
        read_count(|c| nominal_video_label_count(video, c)).unwrap(),
        1
    );
    let label = read_string(|b, c, n| nominal_video_label_at(video, 0, b, c, n)).unwrap();
    assert_eq!(label, "prod");

    // Out-of-range index fails with a code, not a panic.
    let err = read_string(|b, c, n| nominal_video_label_at(video, 1, b, c, n)).unwrap_err();
    assert_eq!(err, NominalErrorCode::IndexOutOfRange as i32);

    nominal_video_free(video);
    nominal_client_free(client);
}

#[test]
fn staged_create_sends_labels_and_properties() {
    let _guard = common::message_lock();
    let server = start_server();
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/video/v1/videos"))
            .and(body_partial_json(json!({
                "title": "Staged Cam",
                "labels": ["prod"],
                "properties": {"camera": "front"}
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(full_video_json())),
    );
    let client = new_client(&server);

    let name = cstr("Staged Cam");
    let mut staging = 0i32;
    assert_eq!(nominal_video_create_begin(name.as_ptr(), &mut staging), 0);
    let description = cstr("desc");
    assert_eq!(
        nominal_video_create_set_description(staging, description.as_ptr()),
        0
    );
    let label = cstr("prod");
    assert_eq!(nominal_video_create_add_label(staging, label.as_ptr()), 0);
    let (key, value) = (cstr("camera"), cstr("front"));
    assert_eq!(
        nominal_video_create_set_property(staging, key.as_ptr(), value.as_ptr()),
        0
    );

    let mut video = 0i32;
    let code = nominal_video_create_commit(client, staging, &mut video);
    assert_eq!(code, 0, "staged create failed: {}", last_error());

    assert_eq!(nominal_video_create_free(staging), 0);
    assert_eq!(
        nominal_video_create_free(staging),
        NominalErrorCode::InvalidHandle as i32
    );

    nominal_video_free(video);
    nominal_client_free(client);
}

#[test]
fn get_video_by_rid_and_absent_description() {
    let server = start_server();
    mount(
        &server,
        Mock::given(method("GET"))
            .and(path(format!("/video/v1/videos/{VIDEO_RID}")))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(minimal_video_json(VIDEO_RID, "Bare")),
            ),
    );
    let client = new_client(&server);

    let rid = cstr(VIDEO_RID);
    let mut video = 0i32;
    let code = nominal_video_get(client, rid.as_ptr(), &mut video);
    assert_eq!(code, 0, "get failed: {}", last_error());

    let name = read_string(|b, c, n| nominal_video_name(video, b, c, n)).unwrap();
    assert_eq!(name, "Bare");

    let mut is_present = true;
    let mut needed = 0u32;
    let code =
        nominal_video_description(video, std::ptr::null_mut(), 0, &mut needed, &mut is_present);
    assert_eq!((code, needed, is_present), (0, 0, false));
    assert_eq!(
        read_count(|c| nominal_video_label_count(video, c)).unwrap(),
        0
    );

    nominal_video_free(video);
    nominal_client_free(client);
}

#[test]
fn list_and_search_videos() {
    let server = start_server();
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/video/v1/videos/search"))
            .and(body_partial_json(json!({
                "query": {
                    "type": "and",
                    "and": [
                        {"type": "searchText", "searchText": "cam"},
                        {"type": "label", "label": "prod"}
                    ]
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "results": [minimal_video_json(VIDEO_RID, "Cam")]
            }))),
    );
    let client = new_client(&server);

    let search_text = cstr("cam");
    let label = cstr("prod");
    let mut list = 0i32;
    let mut count = 0u32;
    let code = nominal_video_search(
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
    let name = read_string(|b, c, n| nominal_video_name(handle, b, c, n)).unwrap();
    assert_eq!(name, "Cam");

    nominal_video_free(handle);
    nominal_handle_list_free(list);
    nominal_client_free(client);
}

#[test]
fn list_videos_returns_handle_list() {
    let server = start_server();
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/video/v1/videos/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "results": [
                    minimal_video_json(VIDEO_RID, "First"),
                    minimal_video_json(&VIDEO_RID.replace("0003", "0004"), "Second")
                ]
            }))),
    );
    let client = new_client(&server);

    let mut list = 0i32;
    let mut count = 0u32;
    assert_eq!(nominal_video_list(client, &mut list, &mut count), 0);
    assert_eq!(count, 2);

    for i in 0..count as i32 {
        let mut handle = 0i32;
        assert_eq!(nominal_handle_list_get(list, i, &mut handle), 0);
        nominal_video_free(handle);
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
            .and(path(format!("/video/v1/videos/{VIDEO_RID}")))
            .and(body_partial_json(json!({
                "labels": ["only-label"]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(full_video_json())),
    );
    let client = new_client(&server);

    let mut staging = 0i32;
    assert_eq!(nominal_video_update_begin(&mut staging), 0);
    let label = cstr("only-label");
    assert_eq!(nominal_video_update_add_label(staging, label.as_ptr()), 0);

    let rid = cstr(VIDEO_RID);
    let mut updated = 0i32;
    let code = nominal_video_update_commit(client, rid.as_ptr(), staging, &mut updated);
    assert_eq!(code, 0, "staged update failed: {}", last_error());

    nominal_video_update_free(staging);
    nominal_video_free(updated);
    nominal_client_free(client);
}

#[test]
fn flat_update_with_nothing_to_change_is_rejected() {
    let _guard = common::message_lock();
    let server = start_server();
    let client = new_client(&server);

    let rid = cstr(VIDEO_RID);
    let mut updated = 0i32;
    assert_eq!(
        nominal_video_update(
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
    // PUT on /video/v1 (assets use POST on /scout/v1 — verbs differ per service).
    mount(
        &server,
        Mock::given(method("PUT"))
            .and(path(format!("/video/v1/videos/{VIDEO_RID}/archive")))
            .respond_with(ResponseTemplate::new(204)),
    );
    mount(
        &server,
        Mock::given(method("PUT"))
            .and(path(format!("/video/v1/videos/{VIDEO_RID}/unarchive")))
            .respond_with(ResponseTemplate::new(204)),
    );
    let client = new_client(&server);

    let rid = cstr(VIDEO_RID);
    assert_eq!(
        nominal_video_archive(client, rid.as_ptr()),
        0,
        "archive failed: {}",
        last_error()
    );
    assert_eq!(
        nominal_video_unarchive(client, rid.as_ptr()),
        0,
        "unarchive failed: {}",
        last_error()
    );

    nominal_client_free(client);
}

#[test]
fn video_calls_reject_invalid_handles() {
    let _guard = common::message_lock();
    let rid = cstr(VIDEO_RID);
    let mut out = 0i32;
    assert_eq!(
        nominal_video_get(0, rid.as_ptr(), &mut out),
        NominalErrorCode::InvalidHandle as i32
    );
    let err = read_string(|b, c, n| nominal_video_name(999_999_999, b, c, n)).unwrap_err();
    assert_eq!(err, NominalErrorCode::InvalidHandle as i32);
}
