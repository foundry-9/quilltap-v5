//! The persistent-state family — v4's `lib/state/` (NEW at `f48f34dc`, the
//! cascading-state feature): the pure path helpers and the shared four-tier
//! cascade resolver (chat → project → group → general).

pub mod cascade;
pub mod paths;
