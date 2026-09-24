//! The Scenario Builder — The Host researches and drafts a starting scene
//! (v4 `d1c06cd9d`, P4.D217).
//!
//! One builder run is one request → tool loop → one scene. Stateless on the
//! server: no chat row, no messages, no participant; the only durable trace is
//! the `llm_logs` rows the loop writes, typed `SCENARIO_BUILDER`.

pub mod mount_pool;
pub mod request_schema;
pub mod system_prompt;
