//! doc-edit `blob` handlers — stub (filled by W4.1d3b build).
#![allow(unused_variables)]

use rusqlite::Connection;
use serde_json::Value;

use super::shared::DocEditToolContext;
use super::DocEditToolResult;

macro_rules! stub {
    ($name:ident) => {
        pub fn $name(
            _m: &Connection,
            _x: &Connection,
            _a: &Value,
            _c: &DocEditToolContext,
        ) -> Result<DocEditToolResult, String> {
            Err(concat!(stringify!($name), " not yet ported").into())
        }
    };
}
stub!(handle_write_blob);
stub!(handle_read_blob);
stub!(handle_list_blobs);
stub!(handle_delete_blob);
