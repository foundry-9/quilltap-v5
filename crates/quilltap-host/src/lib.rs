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
//! What is NOT here yet (the P4.1 lanes): provider IO (the streaming
//! composer), file bytes/image codecs, PTY, the scheduler sweeps' daily
//! cadence, the instance lock. Job types whose handlers need those seams stay
//! on the runner's loud fallback until their lane lands.

pub mod host;
pub mod instances;
pub mod paths;

pub use host::{Host, HostConfig};
pub use instances::InstanceRegistry;
