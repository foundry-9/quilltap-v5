//! The built-in **prompt-template** seeder (P4.83; the refresh arm P4.D237) —
//! v4's 21 "Sample Prompts".
//!
//! Ports `PromptTemplatesRepository.seedSamplePrompts()` /
//! `_doSeedSamplePrompts()` / `upsertBuiltInPrompt()`
//! (`lib/database/repositories/prompt-templates.repository.ts:36-151` at v4
//! `c3eefa752`). ⚠ Not to be confused with
//! [`crate::services::builtin_templates`], the ROLEPLAY-template seeder: that
//! one runs on every boot; this one is **lazy** — it runs from inside the
//! template reads (list / get / update / delete), the first time anybody opens
//! the Import-from-Template modal and on every read after. Both implementations
//! agree on that: v4 has NO startup call, and neither does v5. (The drift
//! ledger's "writes on boot for every existing instance" reading of
//! `c3eefa752` was FALSE for both — P4.D237 §R.4(d).)
//!
//! Per catalogue entry, in catalogue order (v4 `upsertBuiltInPrompt`):
//!   - `description = `${category} prompt optimized for ${modelHint} models``;
//!     `now = getCurrentTimestamp()` — ONCE per entry, BEFORE the lookup, so an
//!     insert's `createdAt`/`updatedAt` and a refresh's `updatedAt` are the same
//!     per-entry instant;
//!   - `collection.findOne({name, isBuiltIn: true})`
//!     ([`crate::db::prompt_templates::find_built_in_by_name`] — the `LIMIT 1`
//!     row, rowid order) — the `isBuiltIn` half is load-bearing: a USER
//!     template sharing the name is never matched;
//!   - **absent** → insert `{id: generateId(), userId: null, name, content,
//!     description, isBuiltIn: true, category, modelHint, tags: [], createdAt:
//!     now, updatedAt: now}` and log `Sample prompt template seeded` at INFO
//!     with `{templateId, name, source, modelHint, category}`;
//!   - **present, and `content`, `description`, `category`, `modelHint` all
//!     `===` the shipped values** (in that order; a NULL column is `undefined`
//!     in v4 and so DIFFERS) → nothing at all: no write, no log;
//!   - **present but different** → the RAW `$set` of those four + `updatedAt:
//!     now` ([`crate::db::prompt_templates::PromptTemplatesRepository::
//!     seed_refresh`] — the repo's own `update` refuses built-ins) and log
//!     `Built-in prompt template refreshed from shipped text` at INFO with
//!     `{templateId, name, source}`.
//!
//! Built-ins are read-only through every route (PUT/DELETE → 403), so the only
//! "edited" built-in a refresh can clobber is a direct-DB edit or an older
//! shipped text — v4's docblock: "the shipped source is authoritative".
//! Characters that imported a template hold their own copy in
//! `characters.systemPrompts`; nothing here reads or writes characters.
//!
//! `source` is always `plugin` in v5 (see refusal 1 below). Both log sentences
//! were RENAMED at `c3eefa752` — the old `Sample prompt template seeded from
//! plugin` (with a `promptId` field) and `…seeded from filesystem` are both
//! gone — so the catalogue's `promptId` is no longer logged; it stays in the
//! vendored JSON because the dump still emits it and the guard checks it.
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
//! field carried until v4 `c3eefa752` dropped it. This corrects the work order's own "click `MODERN_GENERAL`"
//! premise, measured 2026-09-07 against v4's real registry.
//!
//! ## The three refusals (recorded, never silent)
//!
//! 1. **The filesystem `prompts/` fallback** (`sample-prompts-loader.ts`) —
//!    NO-COUNTERPART. It is `@deprecated` in v4 and its directory does not exist
//!    on an install, so it answers `[]` and warns; nothing to port. Since
//!    `c3eefa752` it runs through the same `upsertBuiltInPrompt` with `source:
//!    'filesystem'`; v5 only ever logs `source=plugin`.
//! 2. **The registry-EMPTY arm** — a v4 install whose plugin is missing or
//!    disabled seeds NOTHING. v5 always holds the vendored table, so v5 seeds
//!    where that v4 would not. A RECORDED divergence in v5's favour, pinned by
//!    this note rather than a tripwire: no committed corpus can express "the
//!    plugin is absent" on the v5 side, because the catalogue is compiled in.
//! 3. **`systemPromptRegistry` itself** (plugin discovery, `getAll`, the
//!    display-name computation) — not ported. The vendored table stands in.
//!    (The plugin's own version stamp — `1.1.24` at `c3eefa752` — is recorded
//!    nowhere in v5: NO-PORT.)
//!
//! v4's `seedingPromise` single-flight is a Node concurrency guard with no v5
//! counterpart: the seed runs on the single writer, which serializes it by
//! construction.

use rusqlite::{Connection, OptionalExtension};
use serde::Deserialize;

use crate::clock;
use crate::db::prompt_templates::{
    ensure_prompt_templates_table, find_built_in_by_name, BuiltInSeedRow, CreateOptions,
    PromptTemplatesRepository, PtCreate, PtSeedRefresh,
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
    /// The registry id (`default-system-prompts/<FILE>`). No longer logged (v4
    /// `c3eefa752` dropped `promptId` from the seed line); kept because the
    /// dump emits it and `builtin_prompt_templates_guard` checks it.
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

/// v4's "SEED SOURCE" field value. Always `plugin` in v5 (the filesystem arm
/// is NO-COUNTERPART — see the module doc).
pub const SEED_SOURCE: &str = "plugin";

/// True when `row` still matches the shipped `entry` in all four compared
/// columns — v4's `existing.content === prompt.content && existing.description
/// === description && existing.category === prompt.category &&
/// existing.modelHint === prompt.modelHint`. A NULL (`None`) never matches: in
/// v4 it is `undefined`, which `===` no string.
pub fn row_matches_shipped(row: &BuiltInSeedRow, entry: &BuiltInPromptTemplate) -> bool {
    let description = seed_description(&entry.category, &entry.model_hint);
    row.content.as_deref() == Some(entry.content.as_str())
        && row.description.as_deref() == Some(description.as_str())
        && row.category.as_deref() == Some(entry.category.as_str())
        && row.model_hint.as_deref() == Some(entry.model_hint.as_str())
}

/// True when at least one catalogue entry would be INSERTED or REFRESHED on
/// `conn` — the cheap READ-side probe that keeps a list request off the writer
/// entirely once the table is current (v4 pays 21 `findOne`s on every single
/// read; v5 pays the same 21 on the read pool and takes the writer only when
/// there is something to write). [`seed_sample_prompts`] re-derives every
/// entry's decision under the write lock, so a stale answer here costs at most
/// one no-op writer turn and is never acted on.
///
/// ⚠ Until P4.D237 this probe was NAMES-ONLY, which could never let a refresh
/// fire: once all 21 names were on file it answered `false` forever.
pub fn needs_seeding(conn: &Connection) -> Result<bool, DbError> {
    // No table at all means v4 would `ensureCollection` and seed the lot — so
    // this is the MOST work to do, not the least (the e2e beat's first live run
    // caught this arm answering `false` and leaving the catalogue empty).
    if !has_table(conn)? {
        return Ok(true);
    }
    for entry in catalogue()? {
        match find_built_in_by_name(conn, &entry.name)? {
            None => return Ok(true),
            Some(row) if !row_matches_shipped(&row, &entry) => return Ok(true),
            Some(_) => {}
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

/// v4 `_doSeedSamplePrompts` → `upsertBuiltInPrompt(prompt, 'plugin')` per
/// catalogue entry, in catalogue order: insert when absent, refresh when the
/// `LIMIT 1` built-in row differs, nothing when it matches (the module doc has
/// the whole contract). `mint_id` supplies `this.generateId()` (production: a
/// v4 UUID) so a differential can pin the minted ids; it is called only for an
/// insert, as v4's is.
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
        let description = seed_description(&entry.category, &entry.model_hint);
        // v4 takes `now` ONCE per entry, BEFORE the lookup (`c3eefa752`).
        let now = clock::now_iso();

        match find_built_in_by_name(conn, &entry.name)? {
            None => {
                let id = mint_id();
                repo.create(
                    &PtCreate {
                        user_id: None,
                        name: entry.name.clone(),
                        content: entry.content.clone(),
                        description: Some(description),
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
                    source = SEED_SOURCE,
                    model_hint = %entry.model_hint,
                    category = %entry.category,
                    "Sample prompt template seeded"
                );
            }
            Some(row) if row_matches_shipped(&row, &entry) => {}
            Some(row) => {
                repo.seed_refresh(
                    &row.id,
                    &PtSeedRefresh {
                        content: entry.content.clone(),
                        description,
                        category: entry.category.clone(),
                        model_hint: entry.model_hint.clone(),
                        updated_at: now,
                    },
                )?;
                tracing::info!(
                    target: LOG_TARGET,
                    template_id = %row.id,
                    name = %entry.name,
                    source = SEED_SOURCE,
                    "Built-in prompt template refreshed from shipped text"
                );
            }
        }
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

    // ── P4.D237: the refresh arm (v4 `c3eefa752` `upsertBuiltInPrompt`) ──────
    //
    // v4's own test (`prompt-templates-seed-refresh.test.ts`) MOCKS the
    // collection; its three shapes (empty → insert, stale → refresh in place,
    // same → neither) are the first three below, over a real table. The
    // differential proof is `prompt_templates_routes_equivalence`.

    use crate::test_support::captured_with;

    const T0: &str = "2026-07-01T00:00:00.000Z";

    fn table() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        ensure_prompt_templates_table(&c).unwrap();
        c
    }

    fn modern_general() -> BuiltInPromptTemplate {
        catalogue()
            .unwrap()
            .into_iter()
            .find(|e| e.name == "MODERN General")
            .unwrap()
    }

    #[allow(clippy::too_many_arguments)]
    fn plant(
        c: &Connection,
        id: &str,
        name: &str,
        content: &str,
        description: Option<&str>,
        category: Option<&str>,
        model_hint: Option<&str>,
        built_in: bool,
    ) {
        c.execute(
            "INSERT INTO prompt_templates (id, userId, name, content, description, isBuiltIn, \
             category, modelHint, tags, createdAt, updatedAt) \
             VALUES (?1, NULL, ?2, ?3, ?4, ?5, ?6, ?7, '[\"t\"]', ?8, ?8)",
            rusqlite::params![
                id,
                name,
                content,
                description,
                i64::from(built_in),
                category,
                model_hint,
                T0
            ],
        )
        .unwrap();
    }

    /// An override of a plant's `[content, description, category, modelHint]`.
    type Fields = fn(&mut [Option<String>; 4]);

    /// A MODERN General built-in carrying the shipped values, then `over`.
    fn plant_shipped(c: &Connection, id: &str, over: impl FnOnce(&mut [Option<String>; 4])) {
        let e = modern_general();
        let mut f = [
            Some(e.content.clone()),
            Some(seed_description(&e.category, &e.model_hint)),
            Some(e.category.clone()),
            Some(e.model_hint.clone()),
        ];
        over(&mut f);
        plant(
            c,
            id,
            &e.name,
            f[0].as_deref().unwrap(),
            f[1].as_deref(),
            f[2].as_deref(),
            f[3].as_deref(),
            true,
        );
    }

    type Row = (
        String,
        Option<String>,
        String,
        String,
        Option<String>,
        i64,
        Option<String>,
        Option<String>,
        String,
        String,
        String,
    );

    fn row(c: &Connection, id: &str) -> Row {
        c.query_row(
            "SELECT id, userId, name, content, description, isBuiltIn, category, modelHint, \
             tags, createdAt, updatedAt FROM prompt_templates WHERE id = ?1",
            [id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                    r.get(9)?,
                    r.get(10)?,
                ))
            },
        )
        .unwrap()
    }

    fn count(c: &Connection) -> i64 {
        c.query_row("SELECT count(*) FROM prompt_templates", [], |r| r.get(0))
            .unwrap()
    }

    fn seed(c: &Connection) -> Vec<String> {
        let mut n = 0;
        let ((), lines) = captured_with(|| {
            seed_sample_prompts(c, &mut || {
                n += 1;
                format!("minted-{n}")
            })
            .unwrap()
        });
        lines
    }

    fn refreshed_line(id: &str) -> String {
        format!(
            "INFO quilltap::db Built-in prompt template refreshed from shipped text \
             template_id={id} name=MODERN General source=plugin"
        )
    }

    #[test]
    fn an_empty_table_seeds_the_catalogue_under_the_renamed_line() {
        let c = table();
        assert!(needs_seeding(&c).unwrap());
        let lines = seed(&c);
        assert_eq!(count(&c), 21);
        assert_eq!(
            lines.len(),
            21,
            "one seed line per inserted row: {lines:#?}"
        );
        // v4's bag, in v4's order: {templateId, name, source, modelHint,
        // category} — `promptId` DROPPED, `source` THIRD.
        assert_eq!(
            lines[0],
            "INFO quilltap::db Sample prompt template seeded template_id=minted-1 \
             name=CLAUDE Companion source=plugin model_hint=CLAUDE category=COMPANION"
        );
        assert!(
            lines
                .iter()
                .all(|l| l
                    .starts_with("INFO quilltap::db Sample prompt template seeded template_id="))
        );
        assert!(lines.iter().all(|l| !l.contains("prompt_id=")));
        // Insert: createdAt == updatedAt (one per-entry `now`).
        let r = row(&c, "minted-1");
        assert_eq!(r.9, r.10);
        assert_eq!(r.8, "[]");
        assert_eq!(r.1, None);
        assert_eq!(r.5, 1);
    }

    #[test]
    fn a_current_table_probes_false_and_a_second_pass_is_silent() {
        let c = table();
        seed(&c);
        let before: Vec<Row> = (1..=21).map(|i| row(&c, &format!("minted-{i}"))).collect();
        assert!(!needs_seeding(&c).unwrap());
        // The silence leg for BOTH lines: nothing inserted, nothing refreshed.
        assert_eq!(seed(&c), Vec::<String>::new());
        let after: Vec<Row> = (1..=21).map(|i| row(&c, &format!("minted-{i}"))).collect();
        assert_eq!(
            before, after,
            "an exact row keeps every byte, updatedAt included"
        );
    }

    #[test]
    fn a_stale_builtin_is_refreshed_in_place_under_its_own_id() {
        let c = table();
        plant_shipped(&c, "stale", |f| {
            f[0] = Some("old text".into());
            f[1] = Some("A stale plant".into());
        });
        assert!(needs_seeding(&c).unwrap());
        let lines = seed(&c);
        assert_eq!(
            count(&c),
            21,
            "20 inserted + the one refreshed, never a second row"
        );
        let e = modern_general();
        let r = row(&c, "stale");
        assert_eq!(r.3, e.content);
        assert_eq!(r.4, Some(seed_description(&e.category, &e.model_hint)));
        assert_eq!(r.6.as_deref(), Some(e.category.as_str()));
        assert_eq!(r.7.as_deref(), Some(e.model_hint.as_str()));
        // Untouched: name, userId, isBuiltIn, tags, createdAt.
        assert_eq!(r.2, "MODERN General");
        assert_eq!(r.1, None);
        assert_eq!(r.5, 1);
        assert_eq!(r.8, "[\"t\"]");
        assert_eq!(r.9, T0);
        assert_ne!(r.10, T0, "updatedAt moves to the per-entry now");
        // The refresh line — THREE fields — at MODERN General's catalogue slot.
        let slot = catalogue()
            .unwrap()
            .iter()
            .position(|x| x.name == "MODERN General")
            .unwrap();
        assert_eq!(lines.len(), 21);
        assert_eq!(lines[slot], refreshed_line("stale"));
        assert_eq!(
            lines
                .iter()
                .filter(|l| l.contains("Built-in prompt template refreshed"))
                .count(),
            1
        );
    }

    #[test]
    fn each_compared_field_alone_triggers_the_refresh_and_null_counts_as_different() {
        // content, description, category, modelHint, then a NULL description
        // (v4 hydrates NULL to `undefined`, which `===` no string).
        let cases: [(&str, Fields); 7] = [
            ("content", |f| f[0] = Some("x".into())),
            ("description", |f| f[1] = Some("x".into())),
            ("category", |f| f[2] = Some("COMPANION".into())),
            ("modelHint", |f| f[3] = Some("CLAUDE".into())),
            ("null description", |f| f[1] = None),
            ("null category", |f| f[2] = None),
            ("null modelHint", |f| f[3] = None),
        ];
        for (label, over) in cases {
            let c = table();
            plant_shipped(&c, "p", over);
            // Seed the other 20 so the probe can only be answering for "p".
            let others = seed(&c);
            assert!(
                others.iter().any(|l| *l == refreshed_line("p")),
                "{label}: the pass refreshes"
            );
            plant_shipped(&c, "q", over);
            c.execute("DELETE FROM prompt_templates WHERE id = 'p'", [])
                .unwrap();
            assert!(
                needs_seeding(&c).unwrap(),
                "{label}: the probe sees the drift"
            );
            assert_eq!(seed(&c), vec![refreshed_line("q")], "{label}");
            assert!(!needs_seeding(&c).unwrap(), "{label}: current afterwards");
        }
    }

    #[test]
    fn an_exact_builtin_is_never_written() {
        let c = table();
        plant_shipped(&c, "exact", |_| {});
        let before = row(&c, "exact");
        let lines = seed(&c);
        assert_eq!(lines.len(), 20, "20 inserts, no refresh: {lines:#?}");
        assert!(lines.iter().all(|l| !l.contains("refreshed")));
        assert_eq!(row(&c, "exact"), before, "updatedAt stays {T0}");
    }

    #[test]
    fn only_the_first_duplicate_builtin_is_compared_and_refreshed() {
        // v4 `findOne` → `LIMIT 1`, no ORDER BY → rowid order.
        let c = table();
        plant_shipped(&c, "first", |f| f[0] = Some("old one".into()));
        plant_shipped(&c, "second", |f| f[0] = Some("old two".into()));
        let lines = seed(&c);
        assert!(lines.contains(&refreshed_line("first")));
        assert!(!lines.iter().any(|l| l.contains("template_id=second")));
        assert_eq!(row(&c, "first").3, modern_general().content);
        assert_eq!(row(&c, "second").3, "old two");
        assert_eq!(row(&c, "second").10, T0);
        // …and since only the first is compared, the table now probes CURRENT
        // even though the second row is still stale.
        assert!(!needs_seeding(&c).unwrap());
    }

    #[test]
    fn a_first_duplicate_that_matches_hides_a_stale_second_one() {
        let c = table();
        plant_shipped(&c, "first", |_| {});
        plant_shipped(&c, "second", |f| f[0] = Some("old two".into()));
        let lines = seed(&c);
        assert!(lines.iter().all(|l| !l.contains("refreshed")));
        assert_eq!(row(&c, "second").3, "old two");
    }

    #[test]
    fn a_user_template_sharing_the_name_is_never_matched_or_touched() {
        let c = table();
        plant(
            &c,
            "user",
            "MODERN General",
            "mine",
            None,
            None,
            None,
            false,
        );
        let before = row(&c, "user");
        let lines = seed(&c);
        assert_eq!(count(&c), 22, "the built-in is still inserted beside it");
        assert_eq!(lines.len(), 21);
        assert!(lines.iter().all(|l| !l.contains("refreshed")));
        assert_eq!(row(&c, "user"), before);
    }

    #[test]
    fn the_writer_pass_rederives_rather_than_trusting_a_stale_probe() {
        // The probe runs on the READ pool; the pass on the writer. A row that
        // became current between the two (another writer turn, a sibling
        // request) must not be written again — the pass re-reads per entry.
        let c = table();
        plant_shipped(&c, "stale", |f| f[0] = Some("old".into()));
        assert!(
            needs_seeding(&c).unwrap(),
            "the probe answered on the stale state"
        );
        seed(&c);
        // Race: the table moves to current before the writer turn runs.
        let snapshot: Vec<Row> = std::iter::once(row(&c, "stale"))
            .chain((1..=20).map(|i| row(&c, &format!("minted-{i}"))))
            .collect();
        assert_eq!(seed(&c), Vec::<String>::new(), "no insert, no refresh");
        let again: Vec<Row> = std::iter::once(row(&c, "stale"))
            .chain((1..=20).map(|i| row(&c, &format!("minted-{i}"))))
            .collect();
        assert_eq!(snapshot, again);
        assert_eq!(count(&c), 21);
    }

    #[test]
    fn a_characters_own_prompt_copy_is_never_read_or_written() {
        // Characters that imported a template hold their own copy; the seeder
        // has no path to them (v4's docblock says the same of its own).
        let c = table();
        c.execute_batch(
            "CREATE TABLE characters (id TEXT PRIMARY KEY, systemPrompts TEXT); \
             INSERT INTO characters VALUES ('ch', '[{\"content\":\"old text\"}]');",
        )
        .unwrap();
        plant_shipped(&c, "stale", |f| f[0] = Some("old text".into()));
        seed(&c);
        let copy: String = c
            .query_row(
                "SELECT systemPrompts FROM characters WHERE id = 'ch'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(copy, "[{\"content\":\"old text\"}]");
    }

    #[test]
    fn the_description_is_v4s_literal() {
        assert_eq!(
            seed_description("COMPANION", "CLAUDE"),
            "COMPANION prompt optimized for CLAUDE models"
        );
    }
}
