use crate::async_txn::{AsyncResponse, IsarAsyncTxn};
use crate::raw_object_set::{RawObject, RawObjectSend, RawObjectSet};
use isar_core::collection::IsarCollection;
use isar_core::error::Result;
use isar_core::txn::IsarTxn;

#[no_mangle]
pub unsafe extern "C" fn isar_get(
    collection: Option<&IsarCollection>,
    txn: Option<&IsarTxn>,
    object: &mut RawObject,
) -> u8 {
    isar_try! {
        let collection = collection.unwrap();
        let object_id = object.get_object_id(collection).unwrap();
        let result = collection.get(txn.unwrap(), object_id)?;
        if let Some(result) = result {
            object.set_object(result);
        } else {
            object.set_empty();
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn isar_get_async(
    collection: Option<&'static IsarCollection>,
    txn: Option<&IsarAsyncTxn>,
    object: RawObjectSend,
) {
    let collection = collection.unwrap();
    let oid = object.0.get_object_id(collection).unwrap();
    txn.unwrap().exec(move |txn| -> Result<AsyncResponse> {
        let result = collection.get(txn, oid)?;
        if let Some(result) = result {
            object.0.set_object(result);
        } else {
            object.0.set_empty();
        }
        Ok(AsyncResponse::success())
    });
}

#[no_mangle]
pub unsafe extern "C" fn isar_put(
    collection: Option<&mut IsarCollection>,
    txn: Option<&mut IsarTxn>,
    object: &mut RawObject,
) -> u8 {
    isar_try! {
        let collection = collection.unwrap();
        let oid = object.get_object_id(collection);
        let data = object.object_as_slice();
        let oid = collection.put(txn.unwrap(), oid, data)?;
        object.set_object_id(oid);
    }
}

#[no_mangle]
pub unsafe extern "C" fn isar_put_async(
    collection: Option<&'static IsarCollection>,
    txn: Option<&IsarAsyncTxn>,
    object: RawObjectSend,
) {
    let collection = collection.unwrap();
    let oid = object.0.get_object_id(collection);
    txn.unwrap().exec(move |txn| -> Result<AsyncResponse> {
        let data = object.0.object_as_slice();
        let oid = collection.put(txn, oid, data)?;
        object.0.set_object_id(oid);
        Ok(AsyncResponse::success())
    });
}

#[no_mangle]
pub unsafe extern "C" fn isar_delete(
    collection: Option<&IsarCollection>,
    txn: Option<&mut IsarTxn>,
    object: &RawObject,
) -> u8 {
    isar_try! {
        let collection = collection.unwrap();
        collection.delete(txn.unwrap(), object.get_object_id(collection).unwrap())?;
    }
}

#[no_mangle]
pub unsafe extern "C" fn isar_clear(
    collection: Option<&IsarCollection>,
    txn: Option<&mut IsarTxn>,
) -> u8 {
    isar_try! {
        let collection = collection.unwrap();
        collection.clear(txn.unwrap())?;
    }
}
