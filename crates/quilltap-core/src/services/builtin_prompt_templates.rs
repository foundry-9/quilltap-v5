//! The built-in **prompt-template** seeder (P4.83) — v4's 21 "Sample Prompts".
//!
//! Ports `PromptTemplatesRepository.seedSamplePrompts()` /
//! `_doSeedSamplePrompts()` (`lib/database/repositories/prompt-templates.
//! repository.ts:34-148`). ⚠ Not to be confused with
//! [`crate::services::builtin_templates`], the ROLEPLAY-template seeder: that
//! one runs on every boot and UPDATES rows in place; this one is **lazy** (it
//! runs from inside the list reads, the first time anybody opens the
//! Import-from-Template modal) and **never updates** — a sample prompt already
//! on file by name keeps whatever content it has, forever, even after v4
//! modernizes the shipped text. Both halves are pinned by
//! `prompt_templates_routes_equivalence`.
//!
//! Per catalogue entry, in catalogue order:
//!   - `collection.findOne({name, isBuiltIn: true})`
//!     ([`crate::db::prompt_templates::built_in_name_exists`]) — the
//!     `isBuiltIn` half is load-bearing: a USER template sharing the name does
//!     NOT suppress the seed;
//!   - **absent** → insert `{id: randomUUID(), userId: null, name, content,
//!     description: `${category} prompt optimized for ${modelHint} models`,
//!     isBuiltIn: true, category, modelHint, tags: [], createdAt: now,
//!     updatedAt: now}` (v4 recomputes `now` per row, inside the loop) and log
//!     `Sample prompt template seeded from plugin` at INFO with v4's five-field
//!     bag;
//!   - **present** → nothing at all: no update, no log.
//!
//! ## Where the catalogue comes from
//!
//! v4 reads it from `systemPromptRegistry` — the plugin
//! `qtap-plugin-default-system-prompts`, whose `loadPrompts()` walks
//! `prompts/*.md`. v5 has no plugin system, so the table is VENDORED
//! ([`BUILTIN_PROMPT_TEMPLATES_JSON`]) by
//! `harness/oracle/provision/dump-prompt-templates.ts` driving v4's real plugin
//! module through v4's real registry (the `[[byte-exact-static-data-
//! transcription]]` pattern; the `builtin_templates` precedent).
//!
//! ⚠ **The row `name` is the registry's DISPLAY name, not the filename.**
//! `system-prompt-registry.ts`'s `loadPromptsFromPlugin` computes
//! `` `${modelHint} ${category[0] + category.slice(1).toLowerCase()}` `` and the
//! seeder writes THAT. `MODERN_GENERAL.md` becomes the row **`MODERN General`**;
//! the filename survives only inside the registry id
//! (`default-system-prompts/MODERN_GENERAL`), which the seed log's `promptId`
//! field carries. This corrects the work order's own "click `MODERN_GENERAL`"
//! premise, measured 2026-09-07 against v4's real registry.
//!
//! ## The three refusals (recorded, never silent)
//!
//! 1. **The filesystem `prompts/` fallback** (`sample-prompts-loader.ts`) —
//!    NO-COUNTERPART. It is `@deprecated` in v4 and its directory does not exist
//!    on an install, so it answers `[]` and warns; nothing to port.
//! 2. **The registry-EMPTY arm** — a v4 install whose plugin is missing or
//!    disabled seeds NOTHING. v5 always holds the vendored table, so v5 seeds
//!    where that v4 would not. A RECORDED divergence in v5's favour, pinned by
//!    this note rather than a tripwire: no committed corpus can express "the
//!    plugin is absent" on the v5 side, because the catalogue is compiled in.
//! 3. **`systemPromptRegistry` itself** (plugin discovery, `getAll`, the
//!    display-name computation) — not ported. The vendored table stands in, and
//!    carries the same `promptId` strings so the log line is byte-identical.
//!
//! v4's `seedingPromise` single-flight is a Node concurrency guard with no v5
//! counterpart: the seed runs on the single writer, which serializes it by
//! construction.

use rusqlite::{Connection, OptionalExtension};
use serde::Deserialize;

use crate::clock;
use crate::db::prompt_templates::{
    built_in_name_exists, ensure_prompt_templates_table, CreateOptions, PromptTemplatesRepository,
    PtCreate,
};
use crate::db::DbError;

/// v4's `logger` has no prefix on these lines (the repository logs bare).
pub const LOG_TARGET: &str = "quilltap::db";

/// The verbatim built-in prompt-template catalogue. Regenerate with
/// `harness/oracle/provision/dump-prompt-templates.ts` (recipe in its header);
/// held byte-identical to the v4 checkout's 21 `.md` files by
/// `crates/quilltap-harness/tests/builtin_prompt_templates_guard.rs`.
static BUILTIN_PROMPT_TEMPLATES_JSON: &str = include_str!("builtin_prompt_templates.json");

/// One catalogue entry as the generator emits it.
#[derive(Debug, Deserialize)]
pub struct BuiltInPromptTemplate {
    /// The registry id (`default-system-prompts/<FILE>`) — the log's `promptId`.
    #[serde(rename = "promptId")]
    pub prompt_id: String,
    /// The registry DISPLAY name — the `prompt_templates.name` column.
    pub name: String,
    pub content: String,
    #[serde(rename = "modelHint")]
    pub model_hint: String,
    pub category: String,
}

/// The parsed catalogue, in seed order (the plugin's sorted filenames).
pub fn catalogue() -> Result<Vec<BuiltInPromptTemplate>, DbError> {
    serde_json::from_str(BUILTIN_PROMPT_TEMPLATES_JSON)
        .map_err(|e| DbError::Internal(format!("builtin_prompt_templates.json: {e}")))
}

/// v4's per-row `description` literal.
pub fn seed_description(category: &str, model_hint: &str) -> String {
    format!("{category} prompt optimized for {model_hint} models")
}

/// True when at least one catalogue entry is missing from `conn` — the cheap
/// READ-side probe that keeps a list request off the writer entirely once the
/// table is seeded (v4 pays 21 `findOne`s on every single list; v5 pays the same
/// 21 on the read pool and takes the writer only when there is something to
/// insert). The inserts themselves re-run the same lookup under the write lock,
/// so the decision is never made on a stale read.
pub fn needs_seeding(conn: &Connection) -> Result<bool, DbError> {
    // No table at all means v4 would `ensureCollection` and seed the lot — so
    // this is the MOST work to do, not the least (the e2e beat's first live run
    // caught this arm answering `false` and leaving the catalogue empty).
    if !has_table(conn)? {
        return Ok(true);
    }
    for entry in catalogue()? {
        if !built_in_name_exists(conn, &entry.name)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn has_table(conn: &Connection) -> Result<bool, DbError> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'prompt_templates'",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// v4 `_doSeedSamplePrompts` — insert-if-absent, in catalogue order, NEVER
/// update. `mint_id` supplies `this.generateId()` (production: a v4 UUID) so a
/// differential can pin the minted ids.
pub fn seed_sample_prompts(
    conn: &Connection,
    mint_id: &mut dyn FnMut() -> String,
) -> Result<(), DbError> {
    // v4's `getCollection()` `ensureCollection`s on first access, so an instance
    // that has never opened the modal HAS no table and gets one here. This is a
    // writable connection (the caller takes the writer).
    ensure_prompt_templates_table(conn)?;
    let repo = PromptTemplatesRepository::new(conn);

    for entry in catalogue()? {
        if built_in_name_exists(conn, &entry.name)? {
            continue;
        }
        let id = mint_id();
        // v4 recomputes `getCurrentTimestamp()` inside the loop, per inserted row.
        let now = clock::now_iso();
        repo.create(
            &PtCreate {
                user_id: None,
                name: entry.name.clone(),
                content: entry.content.clone(),
                description: Some(seed_description(&entry.category, &entry.model_hint)),
                is_built_in: true,
                category: Some(entry.category.clone()),
                model_hint: Some(entry.model_hint.clone()),
                tags: Vec::new(),
            },
            &CreateOptions {
                id: id.clone(),
                created_at: now.clone(),
                updated_at: now,
            },
        )?;
        tracing::info!(
            target: LOG_TARGET,
            template_id = %id,
            name = %entry.name,
            prompt_id = %entry.prompt_id,
            model_hint = %entry.model_hint,
            category = %entry.category,
            "Sample prompt template seeded from plugin"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalogue_parses_and_carries_the_shipped_21() {
        let rows = catalogue().expect("catalogue parses");
        assert_eq!(
            rows.len(),
            21,
            "the vendored catalogue is v4's 21 sample prompts"
        );
        // Seed order is the plugin's sorted filenames; the FIRST and LAST anchor it.
        assert_eq!(rows[0].name, "CLAUDE Companion");
        assert_eq!(rows[0].prompt_id, "default-system-prompts/CLAUDE_COMPANION");
        assert_eq!(rows[20].name, "OLLAMA Romantic");
        assert_eq!(rows[20].prompt_id, "default-system-prompts/OLLAMA_ROMANTIC");
        // The display-name rule, which is NOT the filename.
        assert!(rows.iter().any(|r| r.name == "MODERN General"));
        assert!(rows.iter().all(|r| !r.name.contains('_')));
        assert!(rows.iter().all(|r| !r.content.is_empty()));
    }

    #[test]
    fn the_description_is_v4s_literal() {
        assert_eq!(
            seed_description("COMPANION", "CLAUDE"),
            "COMPANION prompt optimized for CLAUDE models"
        );
    }
}
