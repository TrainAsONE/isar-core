use libc::c_int;
use lmdb_sys as ffi;
use std::result::Result;

#[derive(Debug)]
pub enum LmdbError {
    KeyExist {},
    NotFound {},
    MapFull {},
    Other { code: c_int },
}

impl LmdbError {
    pub fn from_err_code(err_code: c_int) -> LmdbError {
        match err_code {
            ffi::MDB_KEYEXIST => LmdbError::KeyExist {},
            ffi::MDB_NOTFOUND => LmdbError::NotFound {},
            ffi::MDB_MAP_FULL => LmdbError::MapFull {},
            other => LmdbError::Other { code: other },
        }
    }

    pub fn to_err_code(&self) -> i32 {
        match self {
            LmdbError::KeyExist {} => ffi::MDB_KEYEXIST,
            LmdbError::NotFound {} => ffi::MDB_NOTFOUND,
            LmdbError::MapFull {} => ffi::MDB_MAP_FULL,
            LmdbError::Other { code: other } => *other,
        }
    }
}

#[inline]
pub fn lmdb_result(err_code: c_int) -> Result<(), LmdbError> {
    if err_code == ffi::MDB_SUCCESS {
        Ok(())
    } else {
        Err(LmdbError::from_err_code(err_code))
    }
}
