// Copyright 2016 MaidSafe.net limited.
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

//! Routines that control the access container.
//!
//! Access container is stored in the user's session packet.

use super::{AccessContainerEntry, AuthError, AuthFuture};
use futures::Future;
use maidsafe_utilities::serialisation::{deserialise, serialise};
use routing::EntryActions;
use rust_sodium::crypto::secretbox;
use safe_core::{Client, FutureExt, MDataInfo, recovery};
use safe_core::ipc::AppKeys;
use safe_core::ipc::resp::access_container_enc_key;
use safe_core::utils::{symmetric_decrypt, symmetric_encrypt};
use std::collections::HashMap;

/// Key of the authenticator entry in the access container
pub const AUTHENTICATOR_ENTRY: &str = "authenticator";

/// Gets access container entry key corresponding to the given app.
pub fn enc_key(
    access_container: &MDataInfo,
    app_id: &str,
    secret_key: &secretbox::Key,
) -> Result<Vec<u8>, AuthError> {
    let nonce = access_container.nonce().ok_or_else(|| {
        AuthError::from("No valid nonce for access container")
    })?;
    Ok(access_container_enc_key(app_id, secret_key, nonce)?)
}

/// Decode raw authenticator entry.
pub fn decode_authenticator_entry(
    encoded: &[u8],
    enc_key: &secretbox::Key,
) -> Result<HashMap<String, MDataInfo>, AuthError> {
    let plaintext = symmetric_decrypt(encoded, enc_key)?;
    Ok(deserialise(&plaintext)?)
}

/// Encode authenticator entry into raw mdata content.
pub fn encode_authenticator_entry(
    decoded: &HashMap<String, MDataInfo>,
    enc_key: &secretbox::Key,
) -> Result<Vec<u8>, AuthError> {
    let plaintext = serialise(decoded)?;
    Ok(symmetric_encrypt(&plaintext, enc_key, None)?)
}

/// Gets an authenticator entry from the access container
pub fn fetch_authenticator_entry<T: 'static>(
    client: &Client<T>,
) -> Box<AuthFuture<(u64, HashMap<String, MDataInfo>)>> {
    let c2 = client.clone();
    let access_container = fry!(client.access_container());

    let key = {
        let sk = fry!(client.secret_symmetric_key());
        fry!(enc_key(&access_container, AUTHENTICATOR_ENTRY, &sk))
    };

    client
        .get_mdata_value(access_container.name, access_container.type_tag, key)
        .map_err(From::from)
        .and_then(move |value| {
            let enc_key = c2.secret_symmetric_key()?;
            decode_authenticator_entry(&value.content, &enc_key).map(
                |decoded| (value.entry_version, decoded),
            )
        })
        .into_box()
}

/// Updates the authenticator entry
pub fn put_authenticator_entry<T: 'static>(
    client: &Client<T>,
    new_value: &HashMap<String, MDataInfo>,
    version: u64,
) -> Box<AuthFuture<()>> {
    let access_container = fry!(client.access_container());
    let (key, ciphertext) = {
        let sk = fry!(client.secret_symmetric_key());
        let key = fry!(enc_key(&access_container, AUTHENTICATOR_ENTRY, &sk));
        let ciphertext = fry!(encode_authenticator_entry(new_value, &sk));

        (key, ciphertext)
    };

    let actions = if version == 0 {
        EntryActions::new().ins(key, ciphertext, 0)
    } else {
        EntryActions::new().update(key, ciphertext, version)
    };

    recovery::mutate_mdata_entries(
        client,
        access_container.name,
        access_container.type_tag,
        actions.into(),
    ).map_err(From::from)
        .into_box()
}

/// Decode raw app entry.
pub fn decode_app_entry(
    encoded: &[u8],
    enc_key: &secretbox::Key,
) -> Result<AccessContainerEntry, AuthError> {
    let plaintext = symmetric_decrypt(encoded, enc_key)?;
    Ok(deserialise(&plaintext)?)
}

/// Encode app entry into raw mdata content.
pub fn encode_app_entry(
    decoded: &AccessContainerEntry,
    enc_key: &secretbox::Key,
) -> Result<Vec<u8>, AuthError> {
    let plaintext = serialise(decoded)?;
    Ok(symmetric_encrypt(&plaintext, enc_key, None)?)
}

/// Gets an access container entry
pub fn fetch_entry<T>(
    client: &Client<T>,
    app_id: &str,
    app_keys: AppKeys,
) -> Box<AuthFuture<(u64, Option<AccessContainerEntry>)>>
where
    T: 'static,
{
    let access_container = fry!(client.access_container());
    let key = fry!(enc_key(&access_container, app_id, &app_keys.enc_key));

    client
        .get_mdata_value(access_container.name, access_container.type_tag, key)
        .map_err(From::from)
        .and_then(move |value| {
            let decoded = if value.content.is_empty() {
                None
            } else {
                Some(decode_app_entry(&value.content, &app_keys.enc_key)?)
            };

            Ok((value.entry_version, decoded))
        })
        .into_box()
}

/// Adds a new entry to the authenticator access container
pub fn put_entry<T>(
    client: &Client<T>,
    app_id: &str,
    app_keys: &AppKeys,
    permissions: &AccessContainerEntry,
    version: u64,
) -> Box<AuthFuture<()>>
where
    T: 'static,
{
    let access_container = fry!(client.access_container());
    let key = fry!(enc_key(&access_container, app_id, &app_keys.enc_key));
    let ciphertext = fry!(encode_app_entry(permissions, &app_keys.enc_key));

    let actions = if version == 0 {
        EntryActions::new().ins(key, ciphertext, 0)
    } else {
        EntryActions::new().update(key, ciphertext, version)
    };

    recovery::mutate_mdata_entries(
        client,
        access_container.name,
        access_container.type_tag,
        actions.into(),
    ).map_err(From::from)
        .into_box()
}

/// Deletes entry from the access container.
pub fn delete_entry<T: 'static>(
    client: &Client<T>,
    app_id: &str,
    app_keys: &AppKeys,
    version: u64,
) -> Box<AuthFuture<()>> {
    // TODO: make sure this can't be called for authenticator Entry-0

    let access_container = fry!(client.access_container());
    let key = fry!(enc_key(&access_container, app_id, &app_keys.enc_key));
    let actions = EntryActions::new().del(key, version);

    recovery::mutate_mdata_entries(
        client,
        access_container.name,
        access_container.type_tag,
        actions.into(),
    ).map_err(From::from)
        .into_box()
}
