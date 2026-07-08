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
//! The PTY host driver (P4.1c) lives in [`terminal`]: the session manager
//! over `portable-pty`, the terminal WebSocket protocol types P4.2's route
//! marshals, the Ariel flush drivers, and the production scrollback source.
//!
//! What is NOT here yet (the other P4.1 lanes): provider IO (the streaming
//! composer), file bytes/image codecs, the scheduler sweeps' daily cadence,
//! the instance lock. Job types whose handlers need those seams stay on the
//! runner's loud fallback until their lane lands.

pub mod host;
pub mod instances;
pub mod paths;
pub mod providers;
pub mod terminal;
pub mod wire;

pub use host::{Host, HostConfig};
pub use instances::InstanceRegistry;
pub use providers::{LivePricingFetch, ProviderIo};
pub use wire::{BlockingWireTransport, ReqwestWireTransport};
