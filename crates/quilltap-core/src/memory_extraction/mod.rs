//! The pure leaves of v4's ONE memory-extraction pipeline
//! (`lib/memory/extraction/`, `ac6348c5f` — memory refactor Phase D; pin
//! `83334b9a5`): the transcript windows and fold rules ([`window`]), the
//! draft parsers and caps ([`parse`]), and the prompt builders
//! ([`prompts`]). Everything here is deterministic string / JSON shaping; the
//! stateful half (the context, the route, the rate limiter, the cursors, the
//! pass runner, the four passes, the turn and fold runners, the catch-ups)
//! lives in `crate::services::extraction` (work order P4.D296).

pub mod parse;
pub mod prompts;
pub mod window;
