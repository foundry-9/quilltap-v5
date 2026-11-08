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
pub mod document_text_search;
pub mod embedding_scheduler;
pub mod file_op_error;
pub mod file_op_status;
pub mod file_ops;
pub mod folder_ops;
pub mod general_state;
pub mod link_groups;
pub mod list;
pub mod normalize_blob_image;
pub mod path_utils;
pub mod persistent_target;
pub mod read_file;
pub mod refresh;
pub mod reindex;
pub mod reindex_file;
pub mod scanner;
pub mod store_file;
pub mod sync;
pub mod upload_blob_text_backfill;
// === P4.D381 ===
pub mod cloud_placeholder;
pub mod docx_html;
pub mod realtime;
// === /P4.D381 ===
