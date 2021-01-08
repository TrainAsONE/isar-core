use crate::error::{IsarError, Result};
use crate::index::IndexType;
use crate::lmdb::cursor::Cursor;
use crate::object::object_id::ObjectId;
use crate::option;
use crate::query::where_clause::WhereClause;
use hashbrown::HashSet;

pub(super) struct WhereExecutor<'a, 'txn> {
    where_clauses: &'a [WhereClause],
    where_clauses_overlapping: bool,
    primary_cursor: Cursor<'txn>,
    secondary_cursor: Option<Cursor<'txn>>,
    secondary_dup_cursor: Option<Cursor<'txn>>,
}

impl<'a, 'txn> WhereExecutor<'a, 'txn> {
    pub fn new(
        primary_cursor: Cursor<'txn>,
        secondary_cursor: Option<Cursor<'txn>>,
        secondary_dup_cursor: Option<Cursor<'txn>>,
        where_clauses: &'a [WhereClause],
        where_clauses_overlapping: bool,
    ) -> Self {
        assert!(!where_clauses.is_empty());
        WhereExecutor {
            where_clauses,
            where_clauses_overlapping,
            primary_cursor,
            secondary_cursor,
            secondary_dup_cursor,
        }
    }

    pub fn run<F>(&mut self, mut callback: F) -> Result<()>
    where
        F: FnMut(&'txn ObjectId, &'txn [u8]) -> bool,
    {
        match self.where_clauses.len() {
            1 => {
                let where_clause = self.where_clauses.first().unwrap();
                self.execute_where_clause(&where_clause, &mut None, &mut callback)?;
            }
            _ => {
                let mut hash_set = HashSet::new();
                let mut result_ids = option!(self.where_clauses_overlapping, &mut hash_set);
                for where_clause in self.where_clauses {
                    let result =
                        self.execute_where_clause(&where_clause, &mut result_ids, &mut callback)?;
                    if !result {
                        return Ok(());
                    }
                }
            }
        }
        Ok(())
    }

    fn execute_where_clause(
        &mut self,
        where_clause: &WhereClause,
        result_ids: &mut Option<&mut HashSet<&'txn [u8]>>,
        callback: &mut impl FnMut(&'txn ObjectId, &'txn [u8]) -> bool,
    ) -> Result<bool> {
        if where_clause.index_type == IndexType::Primary {
            self.execute_primary_where_clause(where_clause, result_ids, callback)
        } else {
            self.execute_secondary_where_clause(where_clause, result_ids, callback)
        }
    }

    fn execute_primary_where_clause(
        &mut self,
        where_clause: &WhereClause,
        result_ids: &mut Option<&mut HashSet<&'txn [u8]>>,
        callback: &mut impl FnMut(&'txn ObjectId, &'txn [u8]) -> bool,
    ) -> Result<bool> {
        let cursor = &mut self.primary_cursor;
        if let Some(iter) = where_clause.iter(cursor)? {
            for entry in iter {
                let (key, val) = entry?;
                if let Some(result_ids) = result_ids {
                    if !result_ids.insert(key) {
                        continue;
                    }
                }
                if !callback(ObjectId::from_bytes(key), val) {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    fn execute_secondary_where_clause(
        &mut self,
        where_clause: &WhereClause,
        result_ids: &mut Option<&mut HashSet<&'txn [u8]>>,
        callback: &mut impl FnMut(&'txn ObjectId, &'txn [u8]) -> bool,
    ) -> Result<bool> {
        let cursor = if where_clause.index_type == IndexType::Secondary {
            self.secondary_cursor.as_mut().unwrap()
        } else {
            self.secondary_dup_cursor.as_mut().unwrap()
        };
        if let Some(iter) = where_clause.iter(cursor)? {
            for index_entry in iter {
                let (_, key) = index_entry?;
                if let Some(result_ids) = result_ids {
                    if !result_ids.insert(key) {
                        continue;
                    }
                }

                let entry = self.primary_cursor.move_to(key)?;
                if let Some((_, val)) = entry {
                    if !callback(ObjectId::from_bytes(key), val) {
                        return Ok(false);
                    }
                } else {
                    return Err(IsarError::DbCorrupted {
                        source: None,
                        message: "Could not find object specified in index.".to_string(),
                    });
                }
            }
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instance::IsarInstance;
    use crate::utils::debug::fill_db;
    use crate::*;

    fn execute_where_clauses(
        isar: &IsarInstance,
        wc: &[WhereClause],
        overlapping: bool,
    ) -> Vec<u32> {
        let txn = isar.begin_txn(false).unwrap();
        let lmdb_txn = txn.get_txn().unwrap();
        let primary_cursor = isar.debug_get_primary_db().cursor(lmdb_txn).unwrap();
        let secondary_cursor = isar.debug_get_secondary_db().cursor(lmdb_txn).unwrap();
        let secondary_dup_cursor = isar.debug_get_secondary_dup_db().cursor(lmdb_txn).unwrap();
        let mut executer = WhereExecutor::new(
            primary_cursor,
            Some(secondary_cursor),
            Some(secondary_dup_cursor),
            &wc,
            overlapping,
        );
        let mut entries = vec![];
        executer
            .run(|oid, _| {
                entries.push(oid.get_time());
                true
            })
            .unwrap();
        entries
    }

    fn get_test_db() -> IsarInstance {
        isar!(isar, col => col!(f1 => Int, f2=> Int, f3 => String; ind!(f1, f3), ind!(f2; true)));
        let mut txn = isar.begin_txn(true).unwrap();

        let build_value = |field1: i32, field2: i32, field3: &str| {
            let mut builder = col.get_object_builder();
            builder.write_int(field1);
            builder.write_int(field2);
            builder.write_string(Some(field3));
            builder.finish()
        };

        let oid = |time: u32| Some(col.get_object_id(time, 5));

        let data = vec![
            (oid(1), build_value(1, 1, "aaa")),
            (oid(2), build_value(1, 2, "abb")),
            (oid(3), build_value(2, 3, "abb")),
            (oid(4), build_value(2, 4, "bbb")),
            (oid(5), build_value(3, 5, "bbb")),
            (oid(6), build_value(3, 6, "bcc")),
        ];
        fill_db(col, &mut txn, &data);
        txn.commit().unwrap();

        isar
    }

    #[test]
    fn test_run_single_primary_where_clause() {
        let isar = get_test_db();
        let col = isar.get_collection(0).unwrap();

        let mut wc = col.create_primary_where_clause();
        wc.add_oid_time(4, u32::MAX);
        assert_eq!(
            execute_where_clauses(&isar, &[wc.clone()], false),
            vec![4, 5, 6]
        );

        let mut wc = col.create_primary_where_clause();
        wc.add_oid_time(4, 4);
        assert_eq!(execute_where_clauses(&isar, &[wc], false), vec![4]);

        let mut wc = col.create_secondary_where_clause(0).unwrap();
        wc.add_oid_time(u32::MAX, u32::MAX);
        assert_eq!(execute_where_clauses(&isar, &[wc], false), vec![]);
    }

    #[test]
    fn test_run_single_secondary_where_clause() {
        let isar = get_test_db();
        let col = isar.get_collection(0).unwrap();

        let mut wc = col.create_secondary_where_clause(0).unwrap();
        wc.add_int(2, i32::MAX);
        assert_eq!(
            execute_where_clauses(&isar, &[wc.clone()], false),
            vec![3, 4, 5, 6]
        );

        let mut wc = col.create_secondary_where_clause(0).unwrap();
        wc.add_int(2, 2);
        assert_eq!(execute_where_clauses(&isar, &[wc], false), vec![3, 4]);

        let mut wc = col.create_secondary_where_clause(0).unwrap();
        wc.add_int(50, i32::MAX);
        assert_eq!(execute_where_clauses(&isar, &[wc], false), vec![]);
    }

    #[test]
    fn test_run_single_secondary_where_clause_unique() {
        let isar = get_test_db();
        let col = isar.get_collection(0).unwrap();

        let mut wc = col.create_secondary_where_clause(1).unwrap();
        wc.add_int(4, i32::MAX);
        assert_eq!(
            execute_where_clauses(&isar, &[wc.clone()], false),
            vec![4, 5, 6]
        );

        let mut wc = col.create_secondary_where_clause(1).unwrap();
        wc.add_int(4, 5);
        assert_eq!(execute_where_clauses(&isar, &[wc], false), vec![4, 5]);

        let mut wc = col.create_secondary_where_clause(0).unwrap();
        wc.add_int(50, i32::MAX);
        assert_eq!(execute_where_clauses(&isar, &[wc], false), vec![]);
    }

    #[test]
    fn test_run_single_secondary_compound_where_clause() {
        let isar = get_test_db();
        let col = isar.get_collection(0).unwrap();

        let mut wc = col.create_secondary_where_clause(0).unwrap();
        wc.add_int(2, i32::MAX);
        assert_eq!(
            execute_where_clauses(&isar, &[wc.clone()], false),
            vec![3, 4, 5, 6]
        );

        //wc.add_int(4, 5);
        //assert_eq!(execute_where_clauses(&isar, &[wc], false), vec![4, 5]);
    }

    #[test]
    fn test_run_non_overlapping_where_clauses() {
        let isar = get_test_db();
        let col = isar.get_collection(0).unwrap();

        let mut wc1 = col.create_secondary_where_clause(0).unwrap();
        wc1.add_int(1, 1);

        let mut wc2 = col.create_secondary_where_clause(0).unwrap();
        wc2.add_int(3, 3);
        assert_eq!(
            execute_where_clauses(&isar, &[wc1, wc2], false),
            vec![1, 2, 5, 6]
        );
    }

    #[test]
    fn test_run_overlapping_where_clauses() {
        let isar = get_test_db();
        let col = isar.get_collection(0).unwrap();

        let mut wc1 = col.create_secondary_where_clause(0).unwrap();
        wc1.add_int(2, i32::MAX);

        let mut wc2 = col.create_secondary_where_clause(0).unwrap();
        wc2.add_int(2, 3);

        let mut result = execute_where_clauses(&isar, &[wc1.clone(), wc2, wc1], true);
        result.sort_unstable();
        assert_eq!(result, vec![3, 4, 5, 6]);
    }
}
