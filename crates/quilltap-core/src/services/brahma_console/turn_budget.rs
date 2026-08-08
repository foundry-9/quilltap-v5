//! v4 `lib/services/brahma-console/turn-budget.ts` — the shared agent-turn
//! budget resolver both Brahma Console paths use (the streaming orchestrator and
//! the one-shot `@Brahma` cousin) so they always agree.
//!
//! Never throws: an unreadable setting falls back to the documented default
//! (v4's `try/catch → DEFAULT_BRAHMA_MAX_AGENT_TURNS`). This is only the hard
//! ceiling on tool-use rounds; the duplicate/stale-query guard in the loop
//! (`MAX_DUPLICATE_TOOL_CALLS`) is independent and still stops a stuck loop well
//! before it.

use crate::db::instance_settings::get_brahma_console_settings;
use crate::db::runtime::Db;

/// v4 `DEFAULT_BRAHMA_MAX_AGENT_TURNS` — the fallback budget, kept in step with
/// `BrahmaConsoleSettingsSchema`'s default (50) and `DEFAULT_BRAHMA_CONSOLE_SETTINGS`.
/// Re-exported from the accessor module so there is a single source of truth.
pub use crate::db::instance_settings::DEFAULT_BRAHMA_MAX_AGENT_TURNS;

/// v4 `resolveBrahmaMaxAgentTurns()` — the operator-set per-query agent-turn
/// budget (Settings → Chat → Brahma Console), or the documented default on any
/// read error. `get_brahma_console_settings` already resolves an
/// unset/unparseable/out-of-range value to the default; this additionally
/// swallows a connection-level `read_main` failure, mirroring v4's outer
/// `try/catch`.
pub fn resolve_brahma_max_agent_turns(db: &Db) -> i64 {
    db.read_main(get_brahma_console_settings)
        .unwrap_or(DEFAULT_BRAHMA_MAX_AGENT_TURNS)
}
