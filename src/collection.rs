use crate::error::{illegal_arg, Result};
use crate::index::{Index, IndexType};
use crate::lmdb::db::Db;
use crate::lmdb::txn::Txn;
use crate::object::object_builder::ObjectBuilder;
use crate::object::object_id::ObjectId;
use crate::object::object_id_generator::ObjectIdGenerator;
use crate::object::object_info::ObjectInfo;
use crate::object::property::Property;
use crate::query::where_clause::WhereClause;

use crate::txn::IsarTxn;
#[cfg(test)]
use {crate::utils::debug::dump_db, hashbrown::HashSet};

pub struct IsarCollection {
    id: u16,
    object_info: ObjectInfo,
    indexes: Vec<Index>,
    db: Db,
    oidg: ObjectIdGenerator,
}

pub struct PendingPut<'a> {
    lmdb_txn: Txn,
    oid: ObjectId,
    object: &'a mut [u8],
}

impl<'a> PendingPut<'a> {
    pub fn get_oid(&mut self) -> ObjectId {
        self.oid
    }

    pub fn get_writable_space(&mut self) -> &mut [u8] {
        self.object
    }
}

impl IsarCollection {
    pub(crate) fn new(id: u16, object_info: ObjectInfo, indexes: Vec<Index>, db: Db) -> Self {
        IsarCollection {
            id,
            object_info,
            indexes,
            db,
            oidg: ObjectIdGenerator::new(id),
        }
    }

    pub fn get_object_builder(&self) -> ObjectBuilder {
        ObjectBuilder::new(&self.object_info)
    }

    pub fn get_object_id(&self, time: u32, rand_counter: u64) -> ObjectId {
        ObjectId::new(self.id, time, rand_counter)
    }

    pub fn verify_object_id(&self, oid: ObjectId) -> Result<()> {
        if oid.get_prefix() != self.id {
            illegal_arg("Invalid ObjectId for this collection.")
        } else {
            Ok(())
        }
    }

    pub fn get<'txn>(&self, txn: &'txn IsarTxn, oid: ObjectId) -> Result<Option<&'txn [u8]>> {
        self.verify_object_id(oid)?;
        let oid_bytes = oid.as_bytes();
        self.db.get(txn.get_read_txn()?, &oid_bytes)
    }

    fn generate_or_veriy_oid(&self, lmdb_txn: &Txn, oid: Option<ObjectId>) -> Result<ObjectId> {
        if let Some(oid) = oid {
            self.verify_object_id(oid)?;
            self.delete_from_indexes(lmdb_txn, oid)?;
            Ok(oid)
        } else {
            Ok(self.oidg.generate())
        }
    }

    pub fn put(&self, txn: &mut IsarTxn, oid: Option<ObjectId>, object: &[u8]) -> Result<ObjectId> {
        let lmdb_txn = txn.take_write_txn()?;
        let oid = self.generate_or_veriy_oid(&lmdb_txn, oid)?;

        if !self.object_info.verify_object(object) {
            illegal_arg("Provided object is invalid.")?;
        }

        let oid_bytes = oid.as_bytes();
        for index in &self.indexes {
            index.create_for_object(&lmdb_txn, &oid_bytes, object)?;
        }

        self.db.put(&lmdb_txn, &oid_bytes, object)?;
        txn.put_write_txn(lmdb_txn);
        Ok(oid)
    }

    pub fn prepare_put(
        &self,
        txn: &mut IsarTxn,
        oid: Option<ObjectId>,
        size: usize,
    ) -> Result<PendingPut> {
        let lmdb_txn = txn.take_write_txn()?;
        let oid = self.generate_or_veriy_oid(&lmdb_txn, oid)?;

        let object = self.db.reserve(&lmdb_txn, oid.as_bytes(), size)?;
        let pending_put = PendingPut {
            lmdb_txn,
            oid,
            object,
        };
        Ok(pending_put)
    }

    pub fn finish_put(&self, txn: &mut IsarTxn, pending: PendingPut) -> Result<()> {
        if !self.object_info.verify_object(pending.object) {
            illegal_arg("Provided object is invalid.")?;
        }

        let oid_bytes = pending.oid.as_bytes();
        for index in &self.indexes {
            index.create_for_object(&pending.lmdb_txn, &oid_bytes, pending.object)?;
        }
        txn.put_write_txn(pending.lmdb_txn);
        Ok(())
    }

    pub fn delete(&self, txn: &mut IsarTxn, oid: ObjectId) -> Result<()> {
        self.verify_object_id(oid)?;
        let lmdb_txn = txn.take_write_txn()?;
        if self.delete_from_indexes(&lmdb_txn, oid)? {
            let oid_bytes = oid.as_bytes();
            self.db.delete(&lmdb_txn, &oid_bytes, None)?;
        }
        txn.put_write_txn(lmdb_txn);
        Ok(())
    }

    pub fn clear(&self, txn: &mut IsarTxn) -> Result<()> {
        let lmdb_txn = txn.take_write_txn()?;
        for index in &self.indexes {
            index.clear(&lmdb_txn)?;
        }
        self.db
            .delete_key_prefix(&lmdb_txn, &self.id.to_le_bytes())?;
        txn.put_write_txn(lmdb_txn);

        Ok(())
    }

    pub fn create_primary_where_clause(&self) -> WhereClause {
        WhereClause::new(&self.id.to_le_bytes(), IndexType::Primary)
    }

    pub fn create_secondary_where_clause(&self, index_index: usize) -> Option<WhereClause> {
        self.indexes
            .get(index_index)
            .map(|i| i.create_where_clause())
    }

    pub fn get_property_by_index(&self, property_index: usize) -> Option<Property> {
        self.object_info.properties.get(property_index).copied()
    }

    fn delete_from_indexes(&self, lmdb_txn: &Txn, oid: ObjectId) -> Result<bool> {
        let oid_bytes = oid.as_bytes();
        let existing_object = self.db.get(lmdb_txn, &oid_bytes)?;
        if let Some(existing_object) = existing_object {
            for index in &self.indexes {
                index.delete_for_object(&lmdb_txn, oid_bytes, existing_object)?;
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    #[cfg(test)]
    pub fn debug_dump(&self, txn: &IsarTxn) -> HashSet<(Vec<u8>, Vec<u8>)> {
        dump_db(self.db, &txn, Some(&self.id.to_le_bytes()))
            .into_iter()
            .map(|(key, val)| (key.to_vec(), val))
            .collect()
    }

    #[cfg(test)]
    pub fn debug_get_index(&self, index: usize) -> &Index {
        self.indexes.get(index).unwrap()
    }

    #[cfg(test)]
    pub fn debug_get_db(&self) -> Db {
        self.db
    }

    #[cfg(test)]
    pub fn debug_get_id(&self) -> u16 {
        self.id
    }

    #[cfg(test)]
    pub(crate) fn debug_get_object_info(&self) -> ObjectInfo {
        self.object_info.clone()
    }
}

#[cfg(test)]
mod tests {
    use crate::{col, ind, isar, set};

    #[test]
    fn test_put_new() {
        isar!(isar, col => col!(field1 => Int));
        let mut txn = isar.begin_txn(true).unwrap();

        let mut builder = col.get_object_builder();
        builder.write_int(1111111);
        let object1 = builder.finish();
        let oid1 = col.put(&mut txn, None, object1.as_bytes()).unwrap();

        let mut builder = col.get_object_builder();
        builder.write_int(123123123);
        let object2 = builder.finish();
        let oid2 = col.put(&mut txn, None, object2.as_bytes()).unwrap();

        let mut builder = col.get_object_builder();
        builder.write_int(123123123);
        let object3 = builder.finish();
        let oid3 = col.put(&mut txn, None, object3.as_bytes()).unwrap();

        assert_eq!(
            col.debug_dump(&txn),
            set![
                (oid1.as_bytes().to_vec(), object1.as_bytes().to_vec()),
                (oid2.as_bytes().to_vec(), object2.as_bytes().to_vec()),
                (oid3.as_bytes().to_vec(), object3.as_bytes().to_vec())
            ]
        );
    }

    #[test]
    fn test_put_existing() {
        isar!(isar, col => col!(field1 => Int));

        let mut txn = isar.begin_txn(true).unwrap();

        let mut builder = col.get_object_builder();
        builder.write_int(1111111);
        let object1 = builder.finish();
        let oid1 = col.put(&mut txn, None, object1.as_bytes()).unwrap();

        let mut builder = col.get_object_builder();
        builder.write_int(123123123);
        let object2 = builder.finish();
        let oid2 = col.put(&mut txn, Some(oid1), object2.as_bytes()).unwrap();
        assert_eq!(oid1, oid2);

        let new_oid = col.oidg.generate();
        let mut builder = col.get_object_builder();
        builder.write_int(55555555);
        let object3 = builder.finish();
        let oid3 = col
            .put(&mut txn, Some(new_oid), object3.as_bytes())
            .unwrap();
        assert_eq!(new_oid, oid3);

        assert_eq!(
            col.debug_dump(&txn),
            set![
                (oid1.as_bytes().to_vec(), object2.as_bytes().to_vec()),
                (new_oid.as_bytes().to_vec(), object3.as_bytes().to_vec())
            ]
        );
    }

    #[test]
    fn test_put_creates_index() {
        isar!(isar, col => col!(field1 => Int; ind!(field1)));

        let mut txn = isar.begin_txn(true).unwrap();

        let mut builder = col.get_object_builder();
        builder.write_int(1234);
        let object = builder.finish();
        let oid = col.put(&mut txn, None, object.as_bytes()).unwrap();

        let index = &col.indexes[0];
        assert_eq!(
            index.debug_dump(&txn),
            set![(
                index.debug_create_key(object.as_bytes()),
                oid.as_bytes().to_vec()
            )]
        );
    }

    #[test]
    fn test_put_clears_old_index() {
        isar!(isar, col => col!(field1 => Int; ind!(field1)));

        let mut txn = isar.begin_txn(true).unwrap();

        let mut builder = col.get_object_builder();
        builder.write_int(1234);
        let object = builder.finish();
        let oid = col.put(&mut txn, None, object.as_bytes()).unwrap();

        let mut builder = col.get_object_builder();
        builder.write_int(5678);
        let object2 = builder.finish();
        col.put(&mut txn, Some(oid), object2.as_bytes()).unwrap();

        let index = &col.indexes[0];
        assert_eq!(
            index.debug_dump(&txn),
            set![(
                index.debug_create_key(object2.as_bytes()),
                oid.as_bytes().to_vec()
            )]
        );
    }

    #[test]
    fn test_delete() {
        isar!(isar, col => col!(field1 => Int; ind!(field1)));

        let mut txn = isar.begin_txn(true).unwrap();

        let mut builder = col.get_object_builder();
        builder.write_int(12345);
        let object = builder.finish();
        let oid = col.put(&mut txn, None, object.as_bytes()).unwrap();

        let mut builder = col.get_object_builder();
        builder.write_int(54321);
        let object2 = builder.finish();
        let oid2 = col.put(&mut txn, None, object2.as_bytes()).unwrap();

        col.delete(&mut txn, oid).unwrap();

        assert_eq!(
            col.debug_dump(&txn),
            set![(oid2.as_bytes().to_vec(), object2.as_bytes().to_vec())],
        );

        let index = &col.indexes[0];
        assert_eq!(
            index.debug_dump(&txn),
            set![(
                index.debug_create_key(object2.as_bytes()),
                oid2.as_bytes().to_vec()
            )],
        );
    }

    #[test]
    fn test_clear() {
        isar!(isar, col1 => col!(f1 => Int; ind!(f1)), col2 => col!(f2 => Int; ind!(f2)));

        let mut txn = isar.begin_txn(true).unwrap();

        let mut builder = col1.get_object_builder();
        builder.write_int(12345);
        let object1 = builder.finish();

        let mut builder = col1.get_object_builder();
        builder.write_int(54321);
        let object2 = builder.finish();

        col1.put(&mut txn, None, object1.as_bytes()).unwrap();
        col1.put(&mut txn, None, object2.as_bytes()).unwrap();
        let oid1 = col2.put(&mut txn, None, object1.as_bytes()).unwrap();
        let oid2 = col2.put(&mut txn, None, object2.as_bytes()).unwrap();

        col1.clear(&mut txn).unwrap();

        assert!(col1.debug_dump(&txn).is_empty());
        assert!(&col1.indexes[0].debug_dump(&txn).is_empty());

        assert_eq!(
            col2.debug_dump(&txn),
            set![
                (oid1.as_bytes().to_vec(), object1.as_bytes().to_vec()),
                (oid2.as_bytes().to_vec(), object2.as_bytes().to_vec())
            ],
        );

        let index2 = &col2.indexes[0];
        assert_eq!(
            index2.debug_dump(&txn),
            set![
                (
                    index2.debug_create_key(object1.as_bytes()),
                    oid1.as_bytes().to_vec()
                ),
                (
                    index2.debug_create_key(object2.as_bytes()),
                    oid2.as_bytes().to_vec()
                )
            ]
        );
    }
}
