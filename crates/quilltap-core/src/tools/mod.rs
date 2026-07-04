//! The tool-handler family (v4 `lib/tools/`). Wave-4 (W4.1) begins here with the
//! RNG tool — the first handler ported. The tool loops, the registry, and the
//! provider-facing `buildTools` slate are later W4.1 sub-units; this module holds
//! the executable handlers themselves.

pub mod definitions;
pub mod native_tool_prompt;
pub mod pseudo_tool_support;
pub mod rng;
pub mod simple_json_parser;
pub mod simple_json_prompt;
pub mod text_block_parser;
pub mod text_block_prompt;
