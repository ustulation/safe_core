// Copyright 2015 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under (1) the MaidSafe.net Commercial License,
// version 1.0 or later, or (2) The General Public License (GPL), version 3, depending on which
// licence you accepted on initial access to the Software (the "Licences").
//
// By contributing code to the SAFE Network Software, or to this project generally, you agree to be
// bound by the terms of the MaidSafe Contributor Agreement, version 1.0.  This, along with the
// Licenses can be found in the root directory of this project at LICENSE, COPYING and CONTRIBUTOR.
//
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.
//
// Please review the Licences for the specific language governing permissions and limitations
// relating to use of the SAFE Network Software.
use nfs;
use maidsafe_types;
use rustc_serialize::{Decodable, Encodable};
use routing;
use routing::sendable::Sendable;
use cbor;
use client;
use WaitCondition;
use std::error::Error;
use maidsafe_types::TypeTag;

// TODO: Remove the tag_id values and get from maidsafe_types
const SDV_TAG: u64 = 100u64;

/// DirectoryHelper provides helper functions to perform Operations on Directory
pub struct DirectoryHelper {
    client: ::std::sync::Arc<::std::sync::Mutex<client::Client>>
}

fn serialise<T>(data: T) -> Vec<u8> where T : Encodable {
    let mut e = cbor::Encoder::from_memory();
    e.encode(&[&data]);
    e.into_bytes()
}

fn deserialise<T>(data: Vec<u8>) -> T where T : Decodable {
    let mut d = cbor::Decoder::from_bytes(data);
    d.decode().next().unwrap().unwrap()
}


impl DirectoryHelper {
    /// Create a new DirectoryHelper instance
    pub fn new(client: ::std::sync::Arc<::std::sync::Mutex<client::Client>>) -> DirectoryHelper {
        DirectoryHelper {
            client: client
        }
    }

    fn get_response(&self, client: ::std::sync::Arc<::std::sync::Mutex<client::Client>>, wait_condition: WaitCondition) -> Result<Vec<u8>, &str> {
        let waiting_message_id = wait_condition.0.clone();
        let pair = wait_condition.1.clone();
        let &(ref lock, ref cvar) = &*pair;
        loop {
            let mut message_id = lock.lock().unwrap();
            message_id = cvar.wait(message_id).unwrap();
            if *message_id == waiting_message_id {
                let client_mutex = client.clone();
                let mut client = client_mutex.lock().unwrap();
                 return match client.get_response(*message_id) {
                     Ok(data) => Ok(data),
                     Err(err) => Err("IO Error")
                 }
            }
        }
    }

    fn network_get(&self, client: ::std::sync::Arc<::std::sync::Mutex<client::Client>>, tag_id: u64,
        name: routing::NameType) -> Result<Vec<u8>, &str> {
        let client_mutex = client.clone();
        let mut safe = client_mutex.lock().unwrap();
        let get_result = safe.get(tag_id, name);
        if get_result.is_err() {
            return Err("Network IO Error");
        }
        self.get_response(client, get_result.unwrap())
    }

    fn network_put<T>(&self, client: ::std::sync::Arc<::std::sync::Mutex<client::Client>>, sendable: T) -> Result<Vec<u8>, &str> where T: Sendable {
        let client_mutex = client.clone();
        let mut safe = client_mutex.lock().unwrap();
        let get_result = safe.put(sendable);
        if get_result.is_err() {
            return Err("Network IO Error");
        }
        self.get_response(client, get_result.unwrap())
    }


    /// Creates a Directory in the network.
    pub fn create(&mut self, owner: routing::NameType, directory_name: String, user_metadata: Vec<u8>) -> Result<(), &str> {
        let directory = nfs::types::DirectoryListing::new(directory_name, user_metadata);
        let serialised_directory = serialise(directory.clone());
        let immutable_data = maidsafe_types::ImmutableData::new(serialised_directory);
        let save_res = self.network_put(self.client.clone(), immutable_data.clone());
        if save_res.is_err() {
            return Err("Save Failed");
        }
        let mut sdv: maidsafe_types::StructuredData = maidsafe_types::StructuredData::new(directory.get_id(), owner,
            vec![immutable_data.name()]);
        let save_sdv_res = self.network_put(self.client.clone(), sdv);
        if save_res.is_err() {
            return Err("Failed to create directory");
        }
        Ok(())
    }

    /// Updates an existing nfs::types::DirectoryListing in the network.
    pub fn update(&mut self, directory: nfs::types::DirectoryListing) -> Result<(), &str> {
        let result = self.network_get(self.client.clone(), SDV_TAG, directory.get_id());
        if result.is_err() {
            return Err("Network IO Error");
        }
        let mut sdv: maidsafe_types::StructuredData = deserialise(result.unwrap());
        let serialised_directory = serialise(directory.clone());
        let immutable_data = maidsafe_types::ImmutableData::new(serialised_directory);
        let immutable_data_put_result = self.network_put(self.client.clone(), immutable_data.clone());
        if immutable_data_put_result.is_err() {
            return Err("Failed to save directory");
        };
        let mut versions = sdv.value();
        versions.push(immutable_data.name());
        sdv.set_value(versions);
        let sdv_put_result = self.network_put(self.client.clone(), sdv);
        if sdv_put_result.is_err() {
            return Err("Failed to update directory version");
        };
        Ok(())
    }

    /// Return the versions of the directory
    pub fn get_versions(&mut self, directory_id: routing::NameType) -> Result<Vec<routing::NameType>, &str> {
        let result = self.network_get(self.client.clone(), SDV_TAG, directory_id);
        if result.is_err() {
            return Err("Network IO Error");
        }
        let sdv: maidsafe_types::StructuredData = deserialise(result.unwrap());
        Ok(sdv.value())
    }

    /// Return the nfs::types::DirectoryListing for the specified version
    pub fn get_by_version(&mut self, directory_id: routing::NameType, version: routing::NameType) -> Result<nfs::types::DirectoryListing, &str> {
        let data_res = self.network_get(self.client.clone(), SDV_TAG, directory_id);
        if data_res.is_err() {
            return Err("Network IO Error");
        }
        let sdv: maidsafe_types::StructuredData = deserialise(data_res.unwrap());
        if !sdv.value().contains(&version) {
            return Err("Version not found");
        };
        let immutable_data_type_id: maidsafe_types::ImmutableDataTypeTag = unsafe { ::std::mem::uninitialized() };
        let get_data = self.network_get(self.client.clone(), immutable_data_type_id.type_tag(), version);
        if get_data.is_err() {
            return Err("Network IO Error");
        }
        let imm: maidsafe_types::ImmutableData = deserialise(get_data.unwrap());
        Ok(deserialise(imm.value().clone()))
    }

    /// Return the nfs::types::DirectoryListing for the latest version
    pub fn get(&mut self, directory_id: routing::NameType) -> Result<nfs::types::DirectoryListing, &str> {
        let sdv_res = self.network_get(self.client.clone(), SDV_TAG, directory_id);
        if sdv_res.is_err() {
            return Err("Network IO Error");
        }
        let sdv: maidsafe_types::StructuredData = deserialise(sdv_res.unwrap());
        let name = match sdv.value().last() {
            Some(data) => routing::NameType(data.0),
            None => return Err("Could not find data")
        };
        let immutable_data_type_id: maidsafe_types::ImmutableDataTypeTag = unsafe { ::std::mem::uninitialized() };
        let imm_data_res = self.network_get(self.client.clone(), immutable_data_type_id.type_tag(), name);
        if imm_data_res.is_err() {
            return Err("Network IO Error");
        }
        let imm: maidsafe_types::ImmutableData = deserialise(imm_data_res.unwrap());
        Ok(deserialise(imm.value().clone()))
    }

}