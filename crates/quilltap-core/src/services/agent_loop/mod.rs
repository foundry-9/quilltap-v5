//! Agent loops shared by more than one surface (v4 `lib/services/agent-loop/`).
//!
//! - [`one_shot_loop`] — v4 `runOneShotToolLoop` (`d1c06cd9d`): "one request →
//!   tools → one answer" with nothing written to a chat. Callers: the Brahma
//!   one-shot console (`services::brahma_console::run_brahma_query`) and the
//!   Scenario Builder.

pub mod one_shot_loop;
