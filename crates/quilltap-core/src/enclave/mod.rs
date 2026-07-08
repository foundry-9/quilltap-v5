//! The autonomous-room ("enclave") engine — Phase-3 Unit 4.
//!
//! Ports v4's autonomous-room subsystem (`lib/background-jobs/handlers/
//! autonomous-room-*.ts`, `autonomous-run-start.ts`, and
//! `lib/services/chat-message/autonomous-room.service.ts`) per the
//! decomposition in `docs/developer/porting/enclave-engine.md`:
//!
//! - [`cron`] — the croner-semantics next-occurrence computation the
//!   schedule tick / manual start / run-end recompute all share.
//! - [`milestones`] — the pacing-milestone bitmask logic + the Host-voiced
//!   milestone/grace message bodies.
//! - [`announce`] — the run-start row patch + the Host-authored
//!   `autonomous-room-*` announcement writers.
//! - [`lifecycle`] — start/pause/resume/stop/update-settings + the startup
//!   and failed-turn reconciliation.
//! - `step` (lands with U4.4) — the persisted one-transition turn handler +
//!   the schedule tick.
//!
//! The pure budget math (`check_budget` / `compute_budget_progress` /
//! `compute_autonomous_context_cap`) predates this module — it lives in
//! [`crate::enclave_budget`] (a Phase-1 unit) and is consumed from there.
//!
//! Design invariant (api-boundary.md Part 3): **no loops or timers in the
//! core.** `step()` performs one persisted transition per claimed job;
//! cadence is the host driver's.

pub mod announce;
pub mod cron;
pub mod lifecycle;
pub mod milestones;
pub mod step;
