//! Tier 2 for users + workspaces: the two smallest read-only surfaces.

mod common;

use common::{cstr, last_error, read_string, TEST_RT};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use nominal_ffi::client::{nominal_client_free, nominal_client_new};
use nominal_ffi::error::NominalErrorCode;
use nominal_ffi::handles::{nominal_handle_list_free, nominal_handle_list_get};
use nominal_ffi::user::{
    nominal_user_display_name, nominal_user_email, nominal_user_free, nominal_user_me,
    nominal_user_org_rid, nominal_user_rid,
};
use nominal_ffi::workspace::{
    nominal_workspace_display_name, nominal_workspace_free, nominal_workspace_list,
    nominal_workspace_rid,
};

const USER_RID: &str = "ri.security.cerulean-staging.user.00000000-0000-0000-0000-0000000000aa";
const ORG_RID: &str = "ri.security.cerulean-staging.org.00000000-0000-0000-0000-0000000000dd";
const WORKSPACE_RID: &str =
    "ri.security.cerulean-staging.workspace.00000000-0000-0000-0000-0000000000ee";

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
fn user_me_marshals_fields() {
    let server = start_server();
    mount(
        &server,
        Mock::given(method("GET"))
            .and(path("/authentication/v2/my/profile"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "rid": USER_RID,
                "orgRid": ORG_RID,
                "email": "tom@example.com",
                "displayName": "Tom",
                "avatarUrl": "https://example.com/avatar.png"
            }))),
    );
    let client = new_client(&server);

    let mut user = 0i32;
    let code = nominal_user_me(client, &mut user);
    assert_eq!(code, 0, "user_me failed: {}", last_error());

    let rid = read_string(|b, c, n| nominal_user_rid(user, b, c, n)).unwrap();
    assert_eq!(rid, USER_RID);
    let org = read_string(|b, c, n| nominal_user_org_rid(user, b, c, n)).unwrap();
    assert_eq!(org, ORG_RID);
    let email = read_string(|b, c, n| nominal_user_email(user, b, c, n)).unwrap();
    assert_eq!(email, "tom@example.com");
    let name = read_string(|b, c, n| nominal_user_display_name(user, b, c, n)).unwrap();
    assert_eq!(name, "Tom");

    assert_eq!(nominal_user_free(user), 0);
    assert_eq!(
        nominal_user_free(user),
        NominalErrorCode::InvalidHandle as i32
    );
    nominal_client_free(client);
}

#[test]
fn workspace_list_marshals_and_sorts() {
    let server = start_server();
    // Returned unsorted; the SDK sorts by display name.
    mount(
        &server,
        Mock::given(method("GET"))
            .and(path("/workspaces/v1/workspaces"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {
                    "id": "00000000-0000-0000-0000-0000000000f2",
                    "rid": WORKSPACE_RID.replace("00ee", "00f2"),
                    "org": ORG_RID,
                    "displayName": "Zulu",
                    "settings": {}
                },
                {
                    "id": "00000000-0000-0000-0000-0000000000ee",
                    "rid": WORKSPACE_RID,
                    "org": ORG_RID,
                    "displayName": "Alpha",
                    "settings": {}
                }
            ]))),
    );
    let client = new_client(&server);

    let mut list = 0i32;
    let mut count = 0u32;
    let code = nominal_workspace_list(client, &mut list, &mut count);
    assert_eq!(code, 0, "workspace_list failed: {}", last_error());
    assert_eq!(count, 2);

    // First entry is "Alpha" (sorted), with the un-substituted RID.
    let mut workspace = 0i32;
    assert_eq!(nominal_handle_list_get(list, 0, &mut workspace), 0);
    let mut is_present = false;
    let name =
        read_string(|b, c, n| nominal_workspace_display_name(workspace, b, c, n, &mut is_present))
            .unwrap();
    assert!(is_present);
    assert_eq!(name, "Alpha");
    let rid = read_string(|b, c, n| nominal_workspace_rid(workspace, b, c, n)).unwrap();
    assert_eq!(rid, WORKSPACE_RID);
    nominal_workspace_free(workspace);

    let mut second = 0i32;
    assert_eq!(nominal_handle_list_get(list, 1, &mut second), 0);
    let name =
        read_string(|b, c, n| nominal_workspace_display_name(second, b, c, n, &mut is_present))
            .unwrap();
    assert_eq!(name, "Zulu");
    nominal_workspace_free(second);

    nominal_handle_list_free(list);
    nominal_client_free(client);
}

#[test]
fn workspace_absent_display_name() {
    let server = start_server();
    mount(
        &server,
        Mock::given(method("GET"))
            .and(path("/workspaces/v1/workspaces"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {
                    "id": "00000000-0000-0000-0000-0000000000ee",
                    "rid": WORKSPACE_RID,
                    "org": ORG_RID,
                    "settings": {}
                }
            ]))),
    );
    let client = new_client(&server);

    let mut list = 0i32;
    let mut count = 0u32;
    assert_eq!(nominal_workspace_list(client, &mut list, &mut count), 0);
    assert_eq!(count, 1);

    let mut workspace = 0i32;
    nominal_handle_list_get(list, 0, &mut workspace);
    let mut is_present = true;
    let mut needed = 0u32;
    let code = nominal_workspace_display_name(
        workspace,
        std::ptr::null_mut(),
        0,
        &mut needed,
        &mut is_present,
    );
    assert_eq!((code, needed, is_present), (0, 0, false));

    nominal_workspace_free(workspace);
    nominal_handle_list_free(list);
    nominal_client_free(client);
}

#[test]
fn user_workspace_calls_reject_invalid_handles() {
    let _guard = common::message_lock();
    let mut out = 0i32;
    assert_eq!(
        nominal_user_me(0, &mut out),
        NominalErrorCode::InvalidHandle as i32
    );
    let err = read_string(|b, c, n| nominal_user_email(999_999_999, b, c, n)).unwrap_err();
    assert_eq!(err, NominalErrorCode::InvalidHandle as i32);
    let err = read_string(|b, c, n| nominal_workspace_rid(999_999_998, b, c, n)).unwrap_err();
    assert_eq!(err, NominalErrorCode::InvalidHandle as i32);
}
