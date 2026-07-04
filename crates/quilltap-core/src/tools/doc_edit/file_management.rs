//! doc-edit `file_management` handlers — stub (filled by W4.1d3b build).
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
stub!(handle_move_file);
stub!(handle_copy_file);
stub!(handle_delete_file);
stub!(handle_create_folder);
stub!(handle_delete_folder);
stub!(handle_move_folder);
