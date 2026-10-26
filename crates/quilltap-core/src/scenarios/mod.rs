//! Scenarios from anywhere — v4 `lib/scenarios/**` (`545e299bb`, pinned at
//! `83334b9a5`).
//!
//! - [`location`] — the one answer to "where does this scenario live, and how
//!   do I read and write there" (v4 `location.ts`).
//! - [`everywhere`] — the instance-wide catalogue, excerpts only (v4
//!   `everywhere.ts`).
//! - [`sort`] — the one comparator every list of scenario objects sorts by
//!   (v4 `sort.ts`, `1d0a31132`).
//! - [`decode_uri`] — JavaScript's `decodeURIComponent`, which every scenario
//!   path the location normalises goes through (ruling R-Z).
//!
//! The file-IO primitives the locations wrap (parse / build list / set
//! default / file content / path rules) stay in [`crate::db::scenarios`], as
//! v4's stay in `lib/mount-index/scenarios-common.ts`.

pub mod decode_uri;
pub mod everywhere;
pub mod location;
pub mod sort;
