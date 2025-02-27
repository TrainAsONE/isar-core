use crate::raw_object_set::{RawObject, RawObjectSet};
use crate::txn::IsarDartTxn;
use crate::{from_c_str, BoolSend, UintSend};
use isar_core::collection::IsarCollection;
use isar_core::index::index_key::IndexKey;
use serde_json::Value;
use std::os::raw::c_char;

#[no_mangle]
pub unsafe extern "C" fn isar_get(
    collection: &'static IsarCollection,
    txn: &mut IsarDartTxn,
    object: &'static mut RawObject,
) -> i64 {
    isar_try_txn!(txn, move |txn| {
        let id = object.get_id();
        let result = collection.get(txn, id)?;
        object.set_object(result);
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn isar_get_by_index(
    collection: &'static IsarCollection,
    txn: &mut IsarDartTxn,
    index_index: u32,
    key: *mut IndexKey,
    object: &'static mut RawObject,
) -> i64 {
    let key = *Box::from_raw(key);
    isar_try_txn!(txn, move |txn| {
        let result = collection.get_by_index(txn, index_index as usize, &key)?;
        if let Some((id, obj)) = result {
            object.set_id(id);
            object.set_object(Some(obj));
        } else {
            object.set_object(None);
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn isar_get_all(
    collection: &'static IsarCollection,
    txn: &mut IsarDartTxn,
    objects: &'static mut RawObjectSet,
) -> i64 {
    isar_try_txn!(txn, move |txn| {
        for object in objects.get_objects() {
            let id = object.get_id();
            let result = collection.get(txn, id)?;
            object.set_object(result);
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn isar_get_all_by_index(
    collection: &'static IsarCollection,
    txn: &mut IsarDartTxn,
    index_index: u32,
    keys: *const *mut IndexKey,
    objects: &'static mut RawObjectSet,
) -> i64 {
    let slice = std::slice::from_raw_parts(keys, objects.get_length());
    let keys: Vec<IndexKey> = slice.iter().map(|k| *Box::from_raw(*k)).collect();
    isar_try_txn!(txn, move |txn| {
        for (object, key) in objects.get_objects().iter_mut().zip(keys) {
            let result = collection.get_by_index(txn, index_index as usize, &key)?;
            if let Some((id, obj)) = result {
                object.set_id(id);
                object.set_object(Some(obj));
            } else {
                object.set_object(None);
            }
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn isar_put(
    collection: &'static mut IsarCollection,
    txn: &mut IsarDartTxn,
    object: &'static mut RawObject,
    replace_on_conflict: bool,
) -> i64 {
    isar_try_txn!(txn, move |txn| {
        let id = if object.get_id() != i64::MIN {
            Some(object.get_id())
        } else {
            None
        };
        let id = collection.put(txn, id, object.get_object(), replace_on_conflict)?;
        object.set_id(id);
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn isar_put_all(
    collection: &'static IsarCollection,
    txn: &mut IsarDartTxn,
    objects: &'static mut RawObjectSet,
    replace_on_conflict: bool,
) -> i64 {
    isar_try_txn!(txn, move |txn| {
        for object in objects.get_objects() {
            let id = if object.get_id() != i64::MIN {
                Some(object.get_id())
            } else {
                None
            };
            let id = collection.put(txn, id, object.get_object(), replace_on_conflict)?;
            object.set_id(id)
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn isar_delete(
    collection: &'static IsarCollection,
    txn: &mut IsarDartTxn,
    id: i64,
    deleted: &'static mut bool,
) -> i64 {
    let deleted = BoolSend(deleted);
    isar_try_txn!(txn, move |txn| {
        *deleted.0 = collection.delete(txn, id)?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn isar_delete_by_index(
    collection: &'static IsarCollection,
    txn: &mut IsarDartTxn,
    index_index: u32,
    key: *mut IndexKey,
    deleted: &'static mut bool,
) -> i64 {
    let deleted = BoolSend(deleted);
    let key = *Box::from_raw(key);
    isar_try_txn!(txn, move |txn| {
        *deleted.0 = collection.delete_by_index(txn, index_index as usize, &key)?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn isar_delete_all(
    collection: &'static IsarCollection,
    txn: &mut IsarDartTxn,
    ids: *const i64,
    ids_length: u32,
    count: &'static mut u32,
) -> i64 {
    let ids = std::slice::from_raw_parts(ids, ids_length as usize);
    let count = UintSend(count);
    isar_try_txn!(txn, move |txn| {
        let mut n = 0u32;
        for id in ids {
            if collection.delete(txn, *id)? {
                n += 1;
            }
        }
        *count.0 = n;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn isar_delete_all_by_index(
    collection: &'static IsarCollection,
    txn: &mut IsarDartTxn,
    index_index: u32,
    keys: *const *mut IndexKey,
    keys_length: u32,
    count: &'static mut u32,
) -> i64 {
    let slice = std::slice::from_raw_parts(keys, keys_length as usize);
    let keys: Vec<IndexKey> = slice.iter().map(|k| *Box::from_raw(*k)).collect();
    let count = UintSend(count);
    isar_try_txn!(txn, move |txn| {
        let mut n = 0u32;
        for key in keys {
            if collection.delete_by_index(txn, index_index as usize, &key)? {
                n += 1;
            }
        }
        *count.0 = n;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn isar_clear(
    collection: &'static IsarCollection,
    txn: &mut IsarDartTxn,
) -> i64 {
    isar_try_txn!(txn, move |txn| collection.clear(txn))
}

#[no_mangle]
pub unsafe extern "C" fn isar_json_import(
    collection: &'static IsarCollection,
    txn: &mut IsarDartTxn,
    id_name: *const c_char,
    json_bytes: *const u8,
    json_length: u32,
    replace_on_conflict: bool,
) -> i64 {
    let id_name = from_c_str(id_name).unwrap();
    let bytes = std::slice::from_raw_parts(json_bytes, json_length as usize);
    let json: Value = serde_json::from_slice(bytes).unwrap();
    isar_try_txn!(txn, move |txn| {
        collection.import_json(txn, id_name, json, replace_on_conflict)
    })
}
