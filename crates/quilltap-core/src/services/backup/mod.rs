//! The backup family (P4.9G5) — v4 `lib/backup/**`.
//!
//! Unit 1 lands the read-and-project half: [`collect::collect_user_data`]
//! (v4 `collectUserData`), [`manifest::create_manifest`], and
//! [`staging::stage_backup`] (the archive tree on disk). The zip, the
//! single-use temp store, the dispatch verbs and the restore side land in the
//! later units — see the order's status header for what is still OPEN.

pub mod collect;
pub mod manifest;
mod marshal;
pub mod staging;

pub use collect::{collect_user_data, BackupData};
pub use manifest::{create_manifest, HostCounts};
pub use staging::{count_subdirs, stage_backup, HostDirs, StageReport};
