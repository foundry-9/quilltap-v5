//! # quilltap-host — the composition root
//!
//! Assembles the `quilltap-core` engine into a running process (Phase-4
//! P4.0): resolve the instance directory, boot the
//! [`CoreEngine`](quilltap_core::api::CoreEngine) through the pepper
//! provisioning gate, and drive the cadence the core deliberately does not
//! own (D20 — the core is scheduler-free):
//!
//! - the **job-runner pump** (v4's dispatcher loop: wake-on-enqueue, the
//!   next-due wake timer, the poll interval),
//! - the **stuck-job reset** tick (5 minutes),
//! - the **enclave schedule tick** (60 seconds + once at startup — v4
//!   `scheduled-autonomous-rooms.ts`).
//!
//! Transports (`quilltap-web`, `quilltap-cli`, `quilltap-tauri`) sit on top
//! of the [`Host`]'s engine handle; they never reach past the boundary.
//!
//! The P4.1 host-driver lanes all live here now: **provider IO** (P4.1a —
//! [`wire`], the reqwest `WireTransport`s, + [`providers`], the `ProviderIo`
//! constructor bundle and the live pricing fetch); the **file/image byte
//! layer** (P4.1b — [`files_store`], the local storage backend behind the
//! core's `FileBytesStore` seam, [`image_codec`], the `HostImageCodec` behind
//! the core's `ImageTranscoder`/codec seams, and [`apply_fs`], the
//! `write_apply` fs ops); the **PTY host driver** (P4.1c — [`terminal`]: the
//! session manager over `portable-pty`, the terminal WebSocket protocol types
//! P4.2's route marshals, the Ariel flush drivers, and the production
//! scrollback source); and the **environment/cadence lane** (P4.1d — the
//! **single-instance lock** ([`lock`] — acquire on assemble, 60 s heartbeat,
//! release on shutdown; a live conflict is a typed boot error), the **four
//! scheduler sweeps** (LLM-log cleanup / memory housekeeping / daily
//! maintenance / the danger-scan enqueuer, each stop-aware in `host`), and
//! the **production `SelfInventoryEnv`** + runtime-mode probes ([`env`])).

pub mod apply_fs;
pub mod avatar_preview;
pub mod env;
pub mod files_store;
pub mod host;
pub mod image_codec;
pub mod instances;
pub mod job_pump;
pub mod lock;
pub mod paths;
pub mod providers;
pub mod spine;
pub mod terminal;
pub mod wire;

pub use apply_fs::ApplyFsOps;
pub use avatar_preview::HostAvatarPreviewRenderer;
pub use files_store::LocalStorageBackend;
pub use host::{Host, HostConfig};
pub use image_codec::HostImageCodec;
pub use instances::InstanceRegistry;
pub use providers::{LivePricingFetch, ProviderIo};
pub use spine::{ChatSpine, ProductionSpineFactory, SpineBundle, SpineFactory};
pub use wire::{BlockingWireTransport, ReqwestWireTransport};
