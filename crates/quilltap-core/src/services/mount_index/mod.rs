//! The mount-index file-operations service layer (P4.6v) — the port of v4's
//! `lib/mount-index/` service modules that back the mount-point file-CRUD
//! surface, the `?action=` verb dispatch, and the folders/blobs sub-routes.
//!
//! The mount-index *data layer* (the `doc_mount_*` repos + `database_store`)
//! lives under `crate::db`; this module is the service tier over it.

pub mod base_path_availability;
pub mod blob_transcode;
pub mod chunker;
pub mod converters;
pub mod embedding_scheduler;
pub mod file_op_error;
pub mod file_op_status;
pub mod file_ops;
pub mod folder_ops;
pub mod general_state;
pub mod link_groups;
pub mod list;
pub mod path_utils;
pub mod read_file;
pub mod refresh;
pub mod reindex;
pub mod reindex_file;
pub mod scanner;
pub mod store_file;
