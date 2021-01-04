#![allow(clippy::missing_safety_doc)]

use isar_core::error::{IsarError, Result};
use std::ffi::CStr;
use std::os::raw::c_char;

#[macro_use]
mod isar_try;

mod async_txn;
pub mod crud;
mod dart;
pub mod filter;
pub mod instance;
pub mod query;
pub mod raw_object_set;
pub mod schema;
pub mod txn;
pub mod where_clause;

pub unsafe fn from_c_str<'a>(str: *const c_char) -> Result<&'a str> {
    match CStr::from_ptr(str).to_str() {
        Ok(str) => Ok(str),
        Err(e) => Err(IsarError::IllegalArgument {
            source: Some(Box::new(e)),
            message: "The provided String is not valid.".to_string(),
        }),
    }
}
