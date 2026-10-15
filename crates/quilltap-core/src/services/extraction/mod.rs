//! The stateful half of v4's ONE memory-extraction pipeline
//! (`lib/memory/extraction/`, `ac6348c5f` — memory refactor Phase D; bug 245
//! from `64743ee13`; pin `83334b9a5`): the per-unit context ([`context`]),
//! the cheap-LLM route ([`route`]), the live rate limiter ([`rate_limit`]),
//! the cursor store ([`cursors`]), the one pass runner into admission
//! ([`run`]), the four passes ([`passes`]), and the turn grain ([`turn`]).
//! The pure leaves live in [`crate::memory_extraction`] (work order P4.D296).

pub mod context;
pub mod cursors;
pub mod passes;
pub mod rate_limit;
pub mod route;
pub mod run;
pub mod turn;
