//! The extraction prompts — v4 `lib/memory/extraction/prompts/` (pin
//! `83334b9a5`). The big bodies live in the generated [`prompt_text`]
//! submodule (extracted mechanically from the four v4 files by
//! `harness/oracle/scripts/extract-memory-task-prompts.py`, never
//! hand-transcribed).

pub mod episode;
pub mod other;
pub(crate) mod prompt_text;
pub mod self_turn;
pub mod shared;
