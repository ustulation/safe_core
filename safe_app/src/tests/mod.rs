// Copyright 2017 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under (1) the MaidSafe.net Commercial License,
// version 1.0 or later, or (2) The General Public License (GPL), version 3, depending on which
// licence you accepted on initial access to the Software (the "Licences").
//
// By contributing code to the SAFE Network Software, or to this project generally, you agree to be
// bound by the terms of the MaidSafe Contributor Agreement.  This, along with the Licenses can be
// found in the root directory of this project at LICENSE, COPYING and CONTRIBUTOR.
//
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.
//
// Please review the Licences for the specific language governing permissions and limitations
// relating to use of the SAFE Network Software.

mod mutable_data;

use App;
use futures::Future;
#[cfg(feature = "use-mock-routing")]
use routing::{ClientError, Request, Response};
use safe_authenticator::Authenticator;
use safe_authenticator::test_utils as authenticator;
use safe_authenticator::test_utils::revoke;
#[cfg(feature = "use-mock-routing")]
use safe_core::MockRouting;
use safe_core::ipc::Permission;
use safe_core::ipc::req::AppExchangeInfo;
use safe_core::ipc::req::AuthReq;
use std::collections::HashMap;
use test_utils::{create_app_with_access, run};
use test_utils::gen_app_exchange_info;

// Test refreshing access info by fetching it from the network.
#[test]
fn refresh_access_info() {
    // Shared container
    let mut container_permissions = HashMap::new();
    let _ = container_permissions.insert(
        "_videos".to_string(),
        btree_set![Permission::Read, Permission::Insert],
    );

    let app = create_app_with_access(container_permissions.clone());

    run(&app, move |client, context| {
        let reg = unwrap!(context.as_registered()).clone();
        assert!(reg.access_info.borrow().is_empty());

        context.refresh_access_info(client).then(move |result| {
            unwrap!(result);
            let access_info = reg.access_info.borrow();
            assert_eq!(
                unwrap!(access_info.get("_videos")).1,
                *unwrap!(container_permissions.get("_videos"))
            );

            Ok(())
        })
    });
}

// Test fetching containers that an app has access to.
#[test]
fn get_access_info() {
    let mut container_permissions = HashMap::new();
    let _ = container_permissions.insert("_videos".to_string(), btree_set![Permission::Read]);
    let _ = container_permissions.insert("_downloads".to_string(), btree_set![Permission::Insert]);

    let app = create_app_with_access(container_permissions);

    run(&app, move |client, context| {
        context.get_access_info(client).then(move |res| {
            let info = unwrap!(res);
            assert!(info.contains_key(&"_videos".to_string()));
            assert!(info.contains_key(&"_downloads".to_string()));
            assert_eq!(info.len(), 3); // third item is the app container

            let (ref _md_info, ref perms) = info["_videos"];
            assert_eq!(perms, &btree_set![Permission::Read]);

            let (ref _md_info, ref perms) = info["_downloads"];
            assert_eq!(perms, &btree_set![Permission::Insert]);

            Ok(())
        })
    });
}

// Make sure we can login to a registered app with low balance.
#[cfg(feature = "use-mock-routing")]
#[test]
pub fn login_registered_with_low_balance() {
    // Register a hook prohibiting mutations and login
    let routing_hook = move |mut routing: MockRouting| -> MockRouting {
        routing.set_request_hook(move |req| {
            match *req {
                Request::PutIData { msg_id, .. } => Some(Response::PutIData {
                    res: Err(ClientError::LowBalance),
                    msg_id,
                }),
                Request::PutMData { msg_id, .. } => Some(Response::PutMData {
                    res: Err(ClientError::LowBalance),
                    msg_id,
                }),
                Request::MutateMDataEntries { msg_id, .. } => {
                    Some(Response::MutateMDataEntries {
                        res: Err(ClientError::LowBalance),
                        msg_id,
                    })
                }
                Request::SetMDataUserPermissions { msg_id, .. } => {
                    Some(Response::SetMDataUserPermissions {
                        res: Err(ClientError::LowBalance),
                        msg_id,
                    })
                }
                Request::DelMDataUserPermissions { msg_id, .. } => {
                    Some(Response::DelMDataUserPermissions {
                        res: Err(ClientError::LowBalance),
                        msg_id,
                    })
                }
                Request::ChangeMDataOwner { msg_id, .. } => Some(Response::ChangeMDataOwner {
                    res: Err(ClientError::LowBalance),
                    msg_id,
                }),
                Request::InsAuthKey { msg_id, .. } => Some(Response::InsAuthKey {
                    res: Err(ClientError::LowBalance),
                    msg_id,
                }),
                Request::DelAuthKey { msg_id, .. } => Some(Response::DelAuthKey {
                    res: Err(ClientError::LowBalance),
                    msg_id,
                }),
                // Pass-through
                _ => None,
            }
        });
        routing
    };

    // Login to the client
    let auth = authenticator::create_account_and_login();

    // Register and login to the app
    let app_info = gen_app_exchange_info();
    let app_id = app_info.id.clone();

    let auth_granted = unwrap!(authenticator::register_app(
        &auth,
        &AuthReq {
            app: app_info,
            app_container: false,
            containers: HashMap::new(),
        },
    ));

    let _app = unwrap!(App::registered_with_hook(
        app_id,
        auth_granted,
        |_network_event| (),
        routing_hook,
    ));
}

// Authorise an app with `app_container`.
fn authorise_app(
    auth: &Authenticator,
    app_info: &AppExchangeInfo,
    app_id: &str,
    app_container: bool,
) -> App {
    let auth_granted = unwrap!(authenticator::register_app(
        auth,
        &AuthReq {
            app: app_info.clone(),
            app_container: app_container,
            containers: HashMap::new(),
        },
    ));

    unwrap!(App::registered(
        String::from(app_id),
        auth_granted,
        |_network_event| (),
    ))
}

// Get the number of containers for `app`
fn num_containers(app: &App) -> usize {
    run(app, move |client, context| {
        context.get_access_info(client).then(move |res| {
            let info = unwrap!(res);
            Ok(info.len())
        })
    })
}

// Test app container creation under the following circumstances:
// 1. An app is authorised for the first time with `app_container` set to `true`.
// 2. If an app is authorised for the first time with `app_container` set to `false`,
// then any subsequent authorisation with `app_container` set to `true` should trigger
// the creation of the app's own container.
// 3. Make sure that the app's own container is also created when it's re-authorised
// with `app_container` set to `true` after it's been revoked.
#[test]
fn app_container_creation() {
    // Authorise an app for the first time with `app_container` set to `true`.
    let auth = authenticator::create_account_and_login();

    let app_info = gen_app_exchange_info();
    let app_id = app_info.id.clone();
    let app = authorise_app(&auth, &app_info, &app_id, true);

    assert_eq!(num_containers(&app), 1); // should only contain app container

    // Authorise a new app with `app_container` set to `false`.
    let auth = authenticator::create_account_and_login();

    let app_info = gen_app_exchange_info();
    let app_id = app_info.id.clone();
    let app = authorise_app(&auth, &app_info, &app_id, false);

    assert_eq!(num_containers(&app), 0); // should be empty

    // Re-authorise the app with `app_container` set to `true`.
    let app = authorise_app(&auth, &app_info, &app_id, true);

    assert_eq!(num_containers(&app), 1); // should only contain app container

    // Authorise a new app with `app_container` set to `false`.
    let auth = authenticator::create_account_and_login();

    let app_info = gen_app_exchange_info();
    let app_id = app_info.id.clone();
    let app = authorise_app(&auth, &app_info, &app_id, false);

    assert_eq!(num_containers(&app), 0); // should be empty

    // Revoke the app
    revoke(&auth, &app_id);

    // Re-authorise the app with `app_container` set to `true`.
    let app = authorise_app(&auth, &app_info, &app_id, true);

    assert_eq!(num_containers(&app), 1); // should only contain app container
}
