//! The carriers' pure helpers — P4.D288 (C1-11). Both carrier lanes (the
//! backup / restore lane and the `.qtap` export / import lane) run in parallel
//! after `KEYSTONE`, so the two pieces of v4 they share land here:
//!
//! - [`cursor_bundle`] — v4 `lib/memory/extraction/cursor-bundle.ts`: a chat's
//!   extraction cursors on a bundle's legacy chat keys, and back;
//! - [`scenario_source_remap`] — v4 `lib/scenarios/source-remap.ts`: a chat's
//!   `scenarioSource` read from a bundle and re-pointed at its mount's new id.

pub mod cursor_bundle;
pub mod scenario_source_remap;
