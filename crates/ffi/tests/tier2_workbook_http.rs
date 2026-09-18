//! Tier 2 for templates + workbooks: request-building + response-parsing +
//! FFI conversion against a wiremock server speaking the Conjure wire
//! format. The deep Notebook/Template response shapes (layout unions, data
//! scopes, locks, commits) are the ground covered here.

mod common;

use common::{cstr, last_error, read_count, read_string, TEST_RT};
use serde_json::json;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use nominal_ffi::client::{nominal_client_free, nominal_client_new};
use nominal_ffi::error::NominalErrorCode;
use nominal_ffi::handles::{nominal_handle_list_free, nominal_handle_list_get};
use nominal_ffi::template::{
    nominal_template_commit_id, nominal_template_description, nominal_template_free,
    nominal_template_get, nominal_template_rid, nominal_template_title,
};
use nominal_ffi::workbook::{
    nominal_workbook_archive, nominal_workbook_create_add_scope_asset,
    nominal_workbook_create_add_scope_run, nominal_workbook_create_begin,
    nominal_workbook_create_commit, nominal_workbook_create_free,
    nominal_workbook_create_set_title, nominal_workbook_free, nominal_workbook_get,
    nominal_workbook_label_count, nominal_workbook_name, nominal_workbook_rid,
    nominal_workbook_scope_rid_at, nominal_workbook_scope_rid_count, nominal_workbook_scope_type,
    nominal_workbook_search, nominal_workbook_unarchive, nominal_workbook_url,
};

const TEMPLATE_RID: &str =
    "ri.scout.cerulean-staging.template.00000000-0000-0000-0000-000000000050";
const WORKBOOK_RID: &str =
    "ri.scout.cerulean-staging.notebook.00000000-0000-0000-0000-000000000060";
const ASSET_RID: &str = "ri.scout.cerulean-staging.asset.00000000-0000-0000-0000-000000000001";
const USER_RID: &str = "ri.security.cerulean-staging.user.00000000-0000-0000-0000-0000000000aa";
const SNAPSHOT_RID: &str =
    "ri.scout.cerulean-staging.snapshot.00000000-0000-0000-0000-000000000070";

/// A minimal-but-valid layout: one empty root panel.
fn layout_json() -> serde_json::Value {
    json!({
        "type": "v1",
        "v1": {"rootPanel": {"type": "empty", "empty": {"type": "v1", "v1": {"id": "root"}}}}
    })
}

fn template_json() -> serde_json::Value {
    json!({
        "rid": TEMPLATE_RID,
        "metadata": {
            "title": "Flight Review",
            "description": "Standard flight analysis",
            "isArchived": false,
            "isPublished": true,
            "createdBy": USER_RID,
            "createdAt": "2024-01-15T10:30:00Z",
            "updatedAt": "2024-01-15T10:30:00Z",
            "editedAt": "2024-01-15T10:30:00Z"
        },
        "commit": {
            "id": "commit-abc",
            "resourceRid": TEMPLATE_RID,
            "message": "initial",
            "isWorkingState": false,
            "committedBy": USER_RID,
            "committedAt": "2024-01-15T10:30:00Z"
        },
        "layout": layout_json(),
        "content": {}
    })
}

/// A full Notebook response (the create/get shape).
fn workbook_json(scope: serde_json::Value) -> serde_json::Value {
    json!({
        "rid": WORKBOOK_RID,
        "snapshotRid": SNAPSHOT_RID,
        "snapshotAuthorRid": USER_RID,
        "snapshotCreatedAt": "2024-01-15T10:30:00Z",
        "metadata": workbook_metadata_json(scope),
        "stateAsJson": "{}",
        "layout": layout_json(),
        "contentV2": {"type": "workbook", "workbook": {}}
    })
}

fn workbook_metadata_json(scope: serde_json::Value) -> serde_json::Value {
    json!({
        "dataScope": scope,
        "notebookType": "WORKBOOK",
        "title": "Flight Review",
        "description": "",
        "isDraft": false,
        "isArchived": false,
        "lock": {"isLocked": false},
        "createdByRid": USER_RID,
        "createdAt": "2024-01-15T10:30:00Z",
        "labels": ["prod"]
    })
}

fn asset_scope() -> serde_json::Value {
    json!({"type": "assetRids", "assetRids": [ASSET_RID]})
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

fn get_template(client: i32) -> i32 {
    let rid = cstr(TEMPLATE_RID);
    let mut template = 0i32;
    let code = nominal_template_get(client, rid.as_ptr(), &mut template);
    assert_eq!(code, 0, "template_get failed: {}", last_error());
    template
}

#[test]
fn template_get_marshals_fields() {
    let server = start_server();
    mount(
        &server,
        Mock::given(method("GET"))
            .and(path(format!("/scout/v1/template/{TEMPLATE_RID}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(template_json())),
    );
    let client = new_client(&server);

    let template = get_template(client);
    let rid = read_string(|b, c, n| nominal_template_rid(template, b, c, n)).unwrap();
    assert_eq!(rid, TEMPLATE_RID);
    let title = read_string(|b, c, n| nominal_template_title(template, b, c, n)).unwrap();
    assert_eq!(title, "Flight Review");
    let mut is_present = false;
    let description =
        read_string(|b, c, n| nominal_template_description(template, b, c, n, &mut is_present))
            .unwrap();
    assert!(is_present);
    assert_eq!(description, "Standard flight analysis");
    let commit = read_string(|b, c, n| nominal_template_commit_id(template, b, c, n)).unwrap();
    assert_eq!(commit, "commit-abc");

    assert_eq!(nominal_template_free(template), 0);
    assert_eq!(
        nominal_template_free(template),
        NominalErrorCode::InvalidHandle as i32
    );
    nominal_client_free(client);
}

#[test]
fn create_workbook_from_template_with_asset_scope() {
    let _guard = common::message_lock();
    let server = start_server();
    mount(
        &server,
        Mock::given(method("GET"))
            .and(path(format!("/scout/v1/template/{TEMPLATE_RID}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(template_json())),
    );
    // The create request must carry the staged title and scope.
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/scout/v2/notebook"))
            .and(body_partial_json(json!({
                "title": "My Flight Review",
                "dataScope": {"type": "assetRids", "assetRids": [ASSET_RID]}
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(workbook_json(asset_scope()))),
    );
    let client = new_client(&server);
    let template = get_template(client);

    let mut staging = 0i32;
    assert_eq!(nominal_workbook_create_begin(&mut staging), 0);
    let title = cstr("My Flight Review");
    assert_eq!(
        nominal_workbook_create_set_title(staging, title.as_ptr()),
        0
    );
    let asset = cstr(ASSET_RID);
    assert_eq!(
        nominal_workbook_create_add_scope_asset(staging, asset.as_ptr()),
        0
    );
    // Mixing scopes must fail.
    let run = cstr("ri.scout.cerulean-staging.run.00000000-0000-0000-0000-000000000010");
    assert_eq!(
        nominal_workbook_create_add_scope_run(staging, run.as_ptr()),
        NominalErrorCode::InvalidArgument as i32
    );

    let mut workbook = 0i32;
    let code = nominal_workbook_create_commit(client, template, staging, &mut workbook);
    assert_eq!(code, 0, "create_commit failed: {}", last_error());

    let rid = read_string(|b, c, n| nominal_workbook_rid(workbook, b, c, n)).unwrap();
    assert_eq!(rid, WORKBOOK_RID);
    let url = read_string(|b, c, n| nominal_workbook_url(workbook, b, c, n)).unwrap();
    assert!(
        url.ends_with(&format!("/workbooks/{WORKBOOK_RID}")),
        "got: {url}"
    );

    let mut scope_type = -1i32;
    assert_eq!(nominal_workbook_scope_type(workbook, &mut scope_type), 0);
    assert_eq!(scope_type, 0, "Assets = 0");
    assert_eq!(
        read_count(|c| nominal_workbook_scope_rid_count(workbook, c)).unwrap(),
        1
    );
    let scope_rid =
        read_string(|b, c, n| nominal_workbook_scope_rid_at(workbook, 0, b, c, n)).unwrap();
    assert_eq!(scope_rid, ASSET_RID);

    nominal_workbook_create_free(staging);
    nominal_workbook_free(workbook);
    nominal_template_free(template);
    nominal_client_free(client);
}

#[test]
fn create_commit_requires_a_scope() {
    let _guard = common::message_lock();
    let server = start_server();
    mount(
        &server,
        Mock::given(method("GET"))
            .and(path(format!("/scout/v1/template/{TEMPLATE_RID}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(template_json())),
    );
    let client = new_client(&server);
    let template = get_template(client);

    let mut staging = 0i32;
    assert_eq!(nominal_workbook_create_begin(&mut staging), 0);
    let mut workbook = 0i32;
    assert_eq!(
        nominal_workbook_create_commit(client, template, staging, &mut workbook),
        NominalErrorCode::InvalidArgument as i32
    );
    assert!(last_error().contains("scope"), "got: {}", last_error());

    nominal_workbook_create_free(staging);
    nominal_template_free(template);
    nominal_client_free(client);
}

#[test]
fn get_workbook_with_run_scope() {
    let server = start_server();
    let run_rid = "ri.scout.cerulean-staging.run.00000000-0000-0000-0000-000000000010";
    mount(
        &server,
        Mock::given(method("GET"))
            .and(path(format!("/scout/v2/notebook/{WORKBOOK_RID}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(workbook_json(
                json!({"type": "runRids", "runRids": [run_rid]}),
            ))),
    );
    let client = new_client(&server);

    let rid = cstr(WORKBOOK_RID);
    let mut workbook = 0i32;
    let code = nominal_workbook_get(client, rid.as_ptr(), &mut workbook);
    assert_eq!(code, 0, "get failed: {}", last_error());

    let name = read_string(|b, c, n| nominal_workbook_name(workbook, b, c, n)).unwrap();
    assert_eq!(name, "Flight Review");
    let mut scope_type = -1i32;
    assert_eq!(nominal_workbook_scope_type(workbook, &mut scope_type), 0);
    assert_eq!(scope_type, 1, "Runs = 1");
    let scope_rid =
        read_string(|b, c, n| nominal_workbook_scope_rid_at(workbook, 0, b, c, n)).unwrap();
    assert_eq!(scope_rid, run_rid);
    assert_eq!(
        read_count(|c| nominal_workbook_label_count(workbook, c)).unwrap(),
        1
    );

    nominal_workbook_free(workbook);
    nominal_client_free(client);
}

#[test]
fn search_sends_asset_filter() {
    let server = start_server();
    // Search results use the metadata-with-rid shape, not full notebooks.
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/scout/v2/notebook/search"))
            .and(body_partial_json(json!({
                "query": {
                    "type": "and",
                    "and": [
                        {"type": "assetRids", "assetRids": {"assets": [ASSET_RID], "operator": "OR"}}
                    ]
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "results": [{"rid": WORKBOOK_RID, "metadata": workbook_metadata_json(asset_scope())}]
            }))),
    );
    let client = new_client(&server);

    let asset = cstr(ASSET_RID);
    let mut list = 0i32;
    let mut count = 0u32;
    let code = nominal_workbook_search(
        client,
        std::ptr::null(),
        std::ptr::null(),
        std::ptr::null(),
        std::ptr::null(),
        asset.as_ptr(),
        std::ptr::null(),
        &mut list,
        &mut count,
    );
    assert_eq!(code, 0, "search failed: {}", last_error());
    assert_eq!(count, 1);

    let mut workbook = 0i32;
    assert_eq!(nominal_handle_list_get(list, 0, &mut workbook), 0);
    let rid = read_string(|b, c, n| nominal_workbook_rid(workbook, b, c, n)).unwrap();
    assert_eq!(rid, WORKBOOK_RID);

    nominal_workbook_free(workbook);
    nominal_handle_list_free(list);
    nominal_client_free(client);
}

#[test]
fn archive_and_unarchive() {
    let server = start_server();
    mount(
        &server,
        Mock::given(method("PUT"))
            .and(path(format!("/scout/v2/notebook/{WORKBOOK_RID}/archive")))
            .respond_with(ResponseTemplate::new(204)),
    );
    mount(
        &server,
        Mock::given(method("PUT"))
            .and(path(format!("/scout/v2/notebook/{WORKBOOK_RID}/unarchive")))
            .respond_with(ResponseTemplate::new(204)),
    );
    let client = new_client(&server);

    let rid = cstr(WORKBOOK_RID);
    assert_eq!(
        nominal_workbook_archive(client, rid.as_ptr()),
        0,
        "archive failed: {}",
        last_error()
    );
    assert_eq!(
        nominal_workbook_unarchive(client, rid.as_ptr()),
        0,
        "unarchive failed: {}",
        last_error()
    );

    nominal_client_free(client);
}

#[test]
fn workbook_calls_reject_invalid_handles() {
    let _guard = common::message_lock();
    let rid = cstr(WORKBOOK_RID);
    let mut out = 0i32;
    assert_eq!(
        nominal_workbook_get(0, rid.as_ptr(), &mut out),
        NominalErrorCode::InvalidHandle as i32
    );
    let err = read_string(|b, c, n| nominal_workbook_name(999_999_999, b, c, n)).unwrap_err();
    assert_eq!(err, NominalErrorCode::InvalidHandle as i32);
    let err = read_string(|b, c, n| nominal_template_title(999_999_998, b, c, n)).unwrap_err();
    assert_eq!(err, NominalErrorCode::InvalidHandle as i32);
}

use nominal_ffi::handles::{nominal_rid_list_add, nominal_rid_list_begin, nominal_rid_list_free};
use nominal_ffi::workbook::nominal_workbook_get_batch;

#[test]
fn get_batch_returns_handles_sorted_by_rid() {
    let rid_b = WORKBOOK_RID.replace("0060", "0065");
    let mut wb_b = workbook_json(asset_scope());
    wb_b["rid"] = json!(&rid_b);
    let server = start_server();
    mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/scout/v2/notebook/batch-get"))
            .and(body_partial_json(json!([WORKBOOK_RID, rid_b])))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!([wb_b, workbook_json(asset_scope())])),
            ),
    );
    let client = new_client(&server);

    let mut rid_list = 0i32;
    assert_eq!(nominal_rid_list_begin(&mut rid_list), 0);
    for rid in [WORKBOOK_RID, rid_b.as_str()] {
        let rid = cstr(rid);
        assert_eq!(nominal_rid_list_add(rid_list, rid.as_ptr()), 0);
    }

    let mut list = 0i32;
    let mut count = 0u32;
    let code = nominal_workbook_get_batch(client, rid_list, &mut list, &mut count);
    assert_eq!(code, 0, "get_batch failed: {}", last_error());
    assert_eq!(count, 2);

    let mut handle = 0i32;
    assert_eq!(nominal_handle_list_get(list, 0, &mut handle), 0);
    let rid0 = read_string(|b, c, n| nominal_workbook_rid(handle, b, c, n)).unwrap();
    assert_eq!(rid0, WORKBOOK_RID);
    nominal_workbook_free(handle);

    assert_eq!(nominal_handle_list_free(list), 0);
    assert_eq!(nominal_rid_list_free(rid_list), 0);
    nominal_client_free(client);
}
