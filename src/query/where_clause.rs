use crate::cursor::IsarCursors;
use crate::error::Result;
use crate::key::IdKey;
use crate::object::isar_object::IsarObject;
use crate::query::id_where_clause::IdWhereClause;
use crate::query::index_where_clause::IndexWhereClause;
use intmap::IntMap;

#[derive(Clone)]
pub(crate) enum WhereClause {
    Id(IdWhereClause),
    Index(IndexWhereClause),
}

impl WhereClause {
    pub fn matches(&self, id: i64, object: IsarObject) -> bool {
        match self {
            WhereClause::Id(wc) => wc.id_matches(id),
            WhereClause::Index(wc) => wc.object_matches(object),
        }
    }

    pub fn iter<'txn, 'env, 'a, F>(
        &self,
        cursors: &IsarCursors<'txn, 'env>,
        result_ids: Option<&mut IntMap<()>>,
        callback: F,
    ) -> Result<bool>
    where
        F: FnMut(IdKey<'txn>, IsarObject<'txn>) -> Result<bool>,
    {
        match self {
            WhereClause::Id(wc) => wc.iter(cursors, result_ids, callback),
            WhereClause::Index(wc) => wc.iter(cursors, result_ids, callback),
        }
    }

    pub(crate) fn is_overlapping(&self, other: &Self) -> bool {
        match (self, other) {
            (WhereClause::Id(wc1), WhereClause::Id(wc2)) => wc1.is_overlapping(wc2),
            (WhereClause::Index(wc1), WhereClause::Index(wc2)) => wc1.is_overlapping(wc2),
            _ => false,
        }
    }

    pub(crate) fn has_duplicates(&self) -> bool {
        match self {
            WhereClause::Id(_) => false,
            WhereClause::Index(wc) => wc.has_duplicates(),
        }
    }
}
