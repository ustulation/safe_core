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

use App;
use errors::AppError;
use ffi::helper::send_sync;
use ffi_utils::{FFI_RESULT_OK, FfiResult, OpaqueCtx, SafePtr, catch_unwind_cb,
                vec_clone_from_raw_parts};
use maidsafe_utilities::serialisation::{deserialise, serialise};
use object_cache::MDataInfoHandle;
use routing::XorName;
use rust_sodium::crypto::secretbox;
use safe_core::MDataInfo;
use safe_core::crypto::shared_secretbox;
use safe_core::ffi::arrays::{SymNonce, SymSecretKey, XorNameArray};
use std::os::raw::c_void;

/// Create non-encrypted mdata info with explicit data name.
///
/// Callback parameters: user data, error code, mdata info handle
#[no_mangle]
pub unsafe extern "C" fn mdata_info_new_public(
    app: *const App,
    name: *const XorNameArray,
    type_tag: u64,
    user_data: *mut c_void,
    o_cb: extern "C" fn(user_data: *mut c_void,
                        result: FfiResult,
                        mdata_info_h: MDataInfoHandle),
) {
    catch_unwind_cb(user_data, o_cb, || {
        let name = XorName(*name);

        send_sync(app, user_data, o_cb, move |_, context| {
            let info = MDataInfo::new_public(name, type_tag);
            Ok(context.object_cache().insert_mdata_info(info))
        })
    })
}

/// Create encrypted mdata info with explicit data name and a
/// provided private key.
///
/// Callback parameters: user data, error code, mdata info handle
#[no_mangle]
pub unsafe extern "C" fn mdata_info_new_private(
    app: *const App,
    name: *const XorNameArray,
    type_tag: u64,
    secret_key: *const SymSecretKey,
    nonce: *const SymNonce,
    user_data: *mut c_void,
    o_cb: extern "C" fn(user_data: *mut c_void,
                        result: FfiResult,
                        mdata_info_h: MDataInfoHandle),
) {
    catch_unwind_cb(user_data, o_cb, || {
        let name = XorName(*name);

        let sk = shared_secretbox::Key::from_raw(&*secret_key);
        let nonce = secretbox::Nonce(*nonce);

        send_sync(app, user_data, o_cb, move |_, context| {
            let info = MDataInfo::new_private(name, type_tag, (sk, nonce));
            Ok(context.object_cache().insert_mdata_info(info))
        })
    })
}

/// Create random, non-encrypted mdata info.
///
/// Callback parameters: user data, error code, mdata info handle
#[no_mangle]
pub unsafe extern "C" fn mdata_info_random_public(
    app: *const App,
    type_tag: u64,
    user_data: *mut c_void,
    o_cb: extern "C" fn(user_data: *mut c_void,
                        result: FfiResult,
                        mdata_info_h: MDataInfoHandle),
) {
    catch_unwind_cb(user_data, o_cb, || {
        send_sync(app, user_data, o_cb, move |_, context| {
            let info = MDataInfo::random_public(type_tag)?;
            Ok(context.object_cache().insert_mdata_info(info))
        })
    })
}

/// Create random, encrypted mdata info.
///
/// Callback parameters: user data, error code, mdata info handle
#[no_mangle]
pub unsafe extern "C" fn mdata_info_random_private(
    app: *const App,
    type_tag: u64,
    user_data: *mut c_void,
    o_cb: extern "C" fn(user_data: *mut c_void,
                        result: FfiResult,
                        mdata_info_h: MDataInfoHandle),
) {
    catch_unwind_cb(user_data, o_cb, || {
        send_sync(app, user_data, o_cb, move |_, context| {
            let info = MDataInfo::random_private(type_tag)?;
            Ok(context.object_cache().insert_mdata_info(info))
        })
    })
}

/// Encrypt mdata entry key using the corresponding mdata info.
///
/// Callback parameters: user data, error code, encrypted entry key vector, vector size
#[no_mangle]
pub unsafe extern "C" fn mdata_info_encrypt_entry_key(
    app: *const App,
    info_h: MDataInfoHandle,
    input_ptr: *const u8,
    input_len: usize,
    user_data: *mut c_void,
    o_cb: extern "C" fn(user_data: *mut c_void,
                        result: FfiResult,
                        enc_entry_key_ptr: *const u8,
                        enc_entry_key_len: usize),
) {
    catch_unwind_cb(user_data, o_cb, || {
        let user_data = OpaqueCtx(user_data);
        let input = vec_clone_from_raw_parts(input_ptr, input_len);

        (*app).send(move |_, context| {
            let info = try_cb!(
                context.object_cache().get_mdata_info(info_h),
                user_data,
                o_cb
            );
            let vec = try_cb!(
                info.enc_entry_key(&input).map_err(AppError::from),
                user_data,
                o_cb
            );

            o_cb(user_data.0, FFI_RESULT_OK, vec.as_safe_ptr(), vec.len());

            None
        })
    })
}

/// Encrypt mdata entry value using the corresponding mdata info.
///
/// Callback parameters: user data, error code, encrypted entry value vector, vector size
#[no_mangle]
pub unsafe extern "C" fn mdata_info_encrypt_entry_value(
    app: *const App,
    info_h: MDataInfoHandle,
    input_ptr: *const u8,
    input_len: usize,
    user_data: *mut c_void,
    o_cb: extern "C" fn(user_data: *mut c_void,
                        result: FfiResult,
                        enc_entry_value_ptr: *const u8,
                        enc_entry_value_len: usize),
) {
    catch_unwind_cb(user_data, o_cb, || {
        let user_data = OpaqueCtx(user_data);
        let input = vec_clone_from_raw_parts(input_ptr, input_len);

        (*app).send(move |_, context| {
            let info = try_cb!(
                context.object_cache().get_mdata_info(info_h),
                user_data,
                o_cb
            );
            let vec = try_cb!(
                info.enc_entry_value(&input).map_err(AppError::from),
                user_data,
                o_cb
            );

            o_cb(user_data.0, FFI_RESULT_OK, vec.as_safe_ptr(), vec.len());

            None
        })
    })
}

/// Decrypt mdata entry value or a key using the corresponding mdata info.
///
/// Callback parameters: user data, error code, decrypted mdata info vector, vector size
#[no_mangle]
pub unsafe extern "C" fn mdata_info_decrypt(
    app: *const App,
    info_h: MDataInfoHandle,
    input_ptr: *const u8,
    input_len: usize,
    user_data: *mut c_void,
    o_cb: extern "C" fn(user_data: *mut c_void,
                        result: FfiResult,
                        mdata_info_decrypt_ptr: *const u8,
                        mdata_info_decrypt_len: usize),
) {
    catch_unwind_cb(user_data, o_cb, || {
        let user_data = OpaqueCtx(user_data);
        let encoded = vec_clone_from_raw_parts(input_ptr, input_len);

        (*app).send(move |_, context| {
            let info = try_cb!(
                context.object_cache().get_mdata_info(info_h),
                user_data,
                o_cb
            );
            let decrypted = try_cb!(
                info.decrypt(&encoded).map_err(AppError::from),
                user_data,
                o_cb
            );

            o_cb(
                user_data.0,
                FFI_RESULT_OK,
                decrypted.as_safe_ptr(),
                decrypted.len(),
            );

            None
        })
    })
}

/// Extract name and type tag from the mdata info.
///
/// Callback parameters: user data, error code, xor name, type type
#[no_mangle]
pub unsafe extern "C" fn mdata_info_extract_name_and_type_tag(
    app: *const App,
    info_h: MDataInfoHandle,
    user_data: *mut c_void,
    o_cb: extern "C" fn(user_data: *mut c_void,
                        result: FfiResult,
                        name: *const XorNameArray,
                        tag: u64),
) {
    catch_unwind_cb(user_data, o_cb, || {
        send_sync(app, user_data, o_cb, move |_, context| {
            let info = context.object_cache().get_mdata_info(info_h)?;
            Ok((&info.name.0, info.type_tag))
        })
    })
}

/// Serialise `MDataInfo`.
///
/// Callback parameters: user data, error code, serialised mdata info
#[no_mangle]
pub unsafe extern "C" fn mdata_info_serialise(
    app: *const App,
    info_h: MDataInfoHandle,
    user_data: *mut c_void,
    o_cb: extern "C" fn(user_data: *mut c_void,
                        result: FfiResult,
                        encoded_ptr: *const u8,
                        encoded_len: usize),
) {
    catch_unwind_cb(user_data, o_cb, || {
        let user_data = OpaqueCtx(user_data);

        (*app).send(move |_, context| {
            let info = try_cb!(
                context.object_cache().get_mdata_info(info_h),
                user_data,
                o_cb
            );
            let encoded = try_cb!(serialise(&*info).map_err(AppError::from), user_data, o_cb);

            o_cb(
                user_data.0,
                FFI_RESULT_OK,
                encoded.as_safe_ptr(),
                encoded.len(),
            );
            None
        })
    })
}

/// Deserialise `MDataInfo`.
///
/// Callback parameters: user data, error code, mdata info handle
#[no_mangle]
pub unsafe extern "C" fn mdata_info_deserialise(
    app: *const App,
    ptr: *const u8,
    len: usize,
    user_data: *mut c_void,
    o_cb: extern "C" fn(user_data: *mut c_void,
                        result: FfiResult,
                        mdata_info_h: MDataInfoHandle),
) {
    catch_unwind_cb(user_data, o_cb, || {
        let encoded = vec_clone_from_raw_parts(ptr, len);

        send_sync(app, user_data, o_cb, move |_, context| {
            let info = deserialise(&encoded)?;
            Ok(context.object_cache().insert_mdata_info(info))
        })
    })
}

/// Free `MDataInfo` from memory.
///
/// Callback parameters: user data, error code
#[no_mangle]
pub unsafe extern "C" fn mdata_info_free(
    app: *const App,
    info_h: MDataInfoHandle,
    user_data: *mut c_void,
    o_cb: extern "C" fn(user_data: *mut c_void, result: FfiResult),
) {

    catch_unwind_cb(user_data, o_cb, || {
        send_sync(app, user_data, o_cb, move |_, context| {
            let _ = context.object_cache().remove_mdata_info(info_h);
            Ok(())
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ffi_utils::test_utils::{call_0, call_1, call_vec_u8};
    use rand;
    use routing::XOR_NAME_LEN;
    use rust_sodium::crypto::secretbox;
    use safe_core::MDataInfo;
    use test_utils::{create_app, run_now};

    // Test creating non-encrypted mdata info.
    #[test]
    fn create_public() {
        let app = create_app();
        let type_tag: u64 = rand::random();

        let info_h = unsafe {
            unwrap!(call_1(
                |ud, cb| mdata_info_random_public(&app, type_tag, ud, cb),
            ))
        };

        run_now(&app, move |_, context| {
            let info = unwrap!(context.object_cache().get_mdata_info(info_h));
            assert_eq!(info.type_tag, type_tag);
            assert!(info.enc_info.is_none());
        })
    }

    // Test creating encrypted mdata info.
    #[test]
    fn create_private() {
        let app = create_app();
        let type_tag: u64 = rand::random();

        let rand_info_h = unsafe {
            unwrap!(call_1(
                |ud, cb| mdata_info_random_private(&app, type_tag, ud, cb),
            ))
        };

        let key = shared_secretbox::gen_key();
        let nonce = secretbox::gen_nonce();
        let new_info_h = unsafe {
            unwrap!(call_1(|ud, cb| {
                mdata_info_new_private(&app, &[2; XOR_NAME_LEN], type_tag, &key.0, &nonce.0, ud, cb)
            }))
        };

        run_now(&app, move |_, context| {
            {
                let rand_info = unwrap!(context.object_cache().get_mdata_info(rand_info_h));
                assert_eq!(rand_info.type_tag, type_tag);
                assert!(rand_info.enc_info.is_some());
            }

            {
                let new_info = unwrap!(context.object_cache().get_mdata_info(new_info_h));
                assert_eq!(new_info.type_tag, type_tag);
                match new_info.enc_info {
                    Some((ref got_key, ref got_nonce)) => {
                        assert_eq!(*got_key, key);
                        assert_eq!(*got_nonce, nonce);
                    }
                    None => panic!("Unexpected result: no enc_info in private MDataInfo"),
                }
            }
        });
    }

    // Test serialising and deserialising mdata_info.
    #[test]
    fn serialise_deserialise() {
        let app = create_app();
        let info1 = unwrap!(MDataInfo::random_private(1000));

        let info1_h = {
            let info = info1.clone();
            run_now(&app, move |_, context| {
                context.object_cache().insert_mdata_info(info)
            })
        };

        let encoded = unsafe {
            unwrap!(call_vec_u8(
                |ud, cb| mdata_info_serialise(&app, info1_h, ud, cb),
            ))
        };

        let info2_h = unsafe {
            let res = call_1(|ud, cb| {
                mdata_info_deserialise(&app, encoded.as_ptr(), encoded.len(), ud, cb)
            });

            unwrap!(res)
        };

        run_now(&app, move |_, context| {
            let info2 = unwrap!(context.object_cache().get_mdata_info(info2_h));
            assert_eq!(info1, *info2);
        });

        unsafe {
            unwrap!(call_0(|ud, cb| mdata_info_free(&app, info1_h, ud, cb)));
            unwrap!(call_0(|ud, cb| mdata_info_free(&app, info2_h, ud, cb)));
        }
    }
}
