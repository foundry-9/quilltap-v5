//! The strip-Anthropic-sampling-parameters boot heal — P4.D362 (v4
//! `544c7ab1e`, migration `strip-anthropic-sampling-parameters-v1`,
//! `dependsOn: []`, `introducedInVersion '4.12.0'`).
//!
//! v4's why, carried (the migration's docblock): the Anthropic plugin now uses
//! its SDK's default API version, `2026-10-21`, which returns a 400 for any
//! Messages request carrying `temperature`, `top_p` or `top_k` — on every
//! model, even at the default value. The plugin no longer sends them and
//! declares `capabilities.samplingParameters: false`, and the
//! connection-profiles repository strips them on every write
//! ([`super::connection_profiles`]). Rows written before that still carry the
//! keys, and the profile card and any export would keep showing values that do
//! nothing. This pass removes them once. Only the sampling keys go;
//! `max_tokens` and the Anthropic provider options (caching, thinking) are
//! kept.
//!
//! ## The provider list is a deliberate frozen copy
//!
//! At runtime the question is answered by each manifest's
//! `capabilities.samplingParameters`
//! ([`crate::provider_manifest::Registry::accepts_sampling_parameters`]), but
//! v4's migrations run before the plugin registry is up, so this one names the
//! providers that declared `false` the day it was written. Do not "fix" it to
//! follow the manifests later — a migration describes the world it ran in.
//!
//! ## STAMPED (the R-7 ruling — "stamp what v4 would re-run")
//!
//! v4's `shouldRun` is TRUE on every instance whose `connection_profiles`
//! carries a `parameters` column — fresh instances included — so the ONLY
//! thing that stops v4's runner re-running the pass on an instance v5 leaves
//! behind is the `migrations_state` row. This heal therefore writes v4's row
//! (through [`super::migrations_ledger::stamp_completed`], v4's
//! `recordCompletedMigration` shape) and honours a row EITHER app wrote. A
//! skipped run (`shouldRun` false — no table, or no `parameters` column)
//! records nothing, exactly as v4's runner records nothing.
//!
//! ## Log lines (§R.5's R-7 exception — v4's own texts)
//!
//! Because the heal reproduces v4's runner semantics, it logs v4's own lines:
//! per cleaned row the DEBUG `Stripped sampling parameters from Anthropic
//! profile`, after the loop the INFO `Stripped sampling parameters from
//! Anthropic connection profiles` (on a fresh instance too, `examined = 0`),
//! and on a failure the ERROR `Failed to strip sampling parameters from
//! Anthropic profiles` before the `Err` (FATAL through the host's `?`). The
//! `context` is v4's literal — it carries NO `-v1`. v4's `reportProgress` bar
//! is NO-PORT (no CLI progress UI on the boot path).

use rusqlite::Connection;
use serde_json::Value;

use super::memory_rewrite_heals::LedgeredHealOutcome;
use super::migrations_ledger::{ensure_migrations_tables, ledger_has, stamp_completed};
use super::schema_probe::{column_exists, in_savepoint, table_exists};
use super::DbError;
use crate::sampling_params::without_sampling_parameters;

/// v4's migration id — the ledger key both apps honour.
pub const MIGRATION_ID: &str = "strip-anthropic-sampling-parameters-v1";

/// v4's `context` literal (NO `-v1`).
const LOG_CONTEXT: &str = "migration.strip-anthropic-sampling-parameters";

/// v4's `FROZEN_PROVIDERS` — the providers that declared
/// `samplingParameters: false` when the migration was written. NEVER the
/// manifest (see the module note).
pub const FROZEN_PROVIDERS: [&str; 1] = ["ANTHROPIC"];

/// v4 `parseParameters`: falsy (NULL / `''`) → nothing; a `JSON.parse`
/// failure → nothing; a parsed `null`, scalar or array → nothing; an object →
/// it, re-keyed in V8's own-property order (integer-like keys first), which
/// is the order v4's `JSON.stringify` then writes it in.
fn parse_parameters(raw: Option<&str>) -> Option<serde_json::Map<String, Value>> {
    let raw = raw.filter(|s| !s.is_empty())?;
    match crate::inventory::types::js_json_parse(raw) {
        Ok(Value::Object(map)) => Some(map),
        _ => None,
    }
}

/// Run the strip once per instance, guarded by v4's own migration ledger.
/// `now` stamps `completedAt` / `lastChecked` (the host passes
/// [`crate::clock::now_iso`]).
///
/// `Ran { rewritten, total }` is v4's `{cleaned, examined}`.
pub fn strip_anthropic_sampling_parameters(
    main: &Connection,
    now: &str,
) -> Result<LedgeredHealOutcome, DbError> {
    match run(main, now) {
        Ok(outcome) => Ok(outcome),
        Err(error) => {
            tracing::error!(
                target: "quilltap::boot",
                context = LOG_CONTEXT,
                error = %error,
                "Failed to strip sampling parameters from Anthropic profiles"
            );
            Err(error)
        }
    }
}

fn run(main: &Connection, now: &str) -> Result<LedgeredHealOutcome, DbError> {
    // v4's runner order: the ledger (created lazily by `loadMigrationState`)
    // and its completed check come BEFORE `shouldRun`.
    ensure_migrations_tables(main)?;
    if ledger_has(main, MIGRATION_ID)? {
        return Ok(LedgeredHealOutcome::AlreadyCompleted);
    }
    // v4 `shouldRun`: the table AND its `parameters` column — else skip
    // WITHOUT stamping.
    if !table_exists(main, "connection_profiles")?
        || !column_exists(main, "connection_profiles", "parameters")?
    {
        return Ok(LedgeredHealOutcome::NotNeeded);
    }

    in_savepoint(main, "strip_anthropic_sampling_parameters", |main| {
        // v4's SELECT verbatim (one `?` per frozen provider), rows in
        // SQLite's own order.
        let rows: Vec<(String, Option<String>)> = {
            let mut stmt = main.prepare(
                "SELECT \"id\", \"parameters\"\n             FROM \"connection_profiles\"\n            WHERE upper(\"provider\") IN (?1)",
            )?;
            let mapped = stmt.query_map([FROZEN_PROVIDERS[0]], |r| Ok((r.get(0)?, r.get(1)?)))?;
            mapped.collect::<Result<_, _>>()?
        };

        let mut cleaned = 0usize;
        for (id, raw) in &rows {
            let Some(params) = parse_parameters(raw.as_deref()) else {
                continue;
            };
            let (parameters, removed) = without_sampling_parameters(&params);
            if removed.is_empty() {
                continue;
            }
            let json = crate::pascal::js_value::json_stringify(&Value::Object(parameters));
            main.execute(
                "UPDATE \"connection_profiles\" SET \"parameters\" = ?1 WHERE \"id\" = ?2",
                rusqlite::params![json, id],
            )?;
            cleaned += 1;
            let removed_json = serde_json::to_string(&removed).unwrap_or_default();
            tracing::debug!(
                target: "quilltap::boot",
                context = LOG_CONTEXT,
                profileId = %id,
                removedJson = %removed_json,
                "Stripped sampling parameters from Anthropic profile"
            );
        }

        let examined = rows.len();
        tracing::info!(
            target: "quilltap::boot",
            context = LOG_CONTEXT,
            examined,
            cleaned,
            "Stripped sampling parameters from Anthropic connection profiles"
        );

        stamp_completed(
            main,
            MIGRATION_ID,
            now,
            cleaned as i64,
            &format!(
                "Examined {examined} Anthropic profile(s); removed sampling parameters from {cleaned}"
            ),
        )?;
        Ok(LedgeredHealOutcome::Ran {
            rewritten: cleaned,
            total: examined,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "2026-11-05T00:00:00.000Z";

    fn db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE connection_profiles (id TEXT PRIMARY KEY, provider TEXT, parameters TEXT)",
        )
        .unwrap();
        db
    }

    fn seed(db: &Connection, id: &str, provider: &str, params: Option<&str>) {
        db.execute(
            "INSERT INTO connection_profiles VALUES (?1, ?2, ?3)",
            rusqlite::params![id, provider, params],
        )
        .unwrap();
    }

    fn params(db: &Connection, id: &str) -> Option<String> {
        db.query_row(
            "SELECT parameters FROM connection_profiles WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn strips_anthropic_rows_only_and_stamps() {
        let db = db();
        seed(
            &db,
            "a",
            "ANTHROPIC",
            Some(r#"{"temperature":0.7,"top_p":1,"max_tokens":512}"#),
        );
        seed(&db, "b", "anthropic", Some(r#"{"topK":5}"#));
        seed(&db, "c", "ANTHROPIC", Some(r#"{"max_tokens":512}"#));
        seed(&db, "d", "OPENAI", Some(r#"{"temperature":0.7}"#));
        seed(&db, "e", "ANTHROPIC", Some("not json"));
        let out = strip_anthropic_sampling_parameters(&db, NOW).unwrap();
        assert_eq!(
            out,
            LedgeredHealOutcome::Ran {
                rewritten: 2,
                total: 4
            }
        );
        assert_eq!(params(&db, "a").as_deref(), Some(r#"{"max_tokens":512}"#));
        assert_eq!(params(&db, "b").as_deref(), Some("{}"));
        assert_eq!(params(&db, "c").as_deref(), Some(r#"{"max_tokens":512}"#));
        assert_eq!(params(&db, "d").as_deref(), Some(r#"{"temperature":0.7}"#));
        assert_eq!(params(&db, "e").as_deref(), Some("not json"));
        let msg: String = db
            .query_row(
                "SELECT message FROM migrations_state WHERE id = ?1",
                [MIGRATION_ID],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            msg,
            "Examined 4 Anthropic profile(s); removed sampling parameters from 2"
        );
        // The second boot: the stamp stops it.
        seed(&db, "f", "ANTHROPIC", Some(r#"{"temperature":1}"#));
        assert_eq!(
            strip_anthropic_sampling_parameters(&db, NOW).unwrap(),
            LedgeredHealOutcome::AlreadyCompleted
        );
        assert_eq!(params(&db, "f").as_deref(), Some(r#"{"temperature":1}"#));
    }

    #[test]
    fn no_table_or_no_column_is_not_needed_and_unstamped() {
        let db = Connection::open_in_memory().unwrap();
        assert_eq!(
            strip_anthropic_sampling_parameters(&db, NOW).unwrap(),
            LedgeredHealOutcome::NotNeeded
        );
        db.execute_batch("CREATE TABLE connection_profiles (id TEXT PRIMARY KEY, provider TEXT)")
            .unwrap();
        assert_eq!(
            strip_anthropic_sampling_parameters(&db, NOW).unwrap(),
            LedgeredHealOutcome::NotNeeded
        );
        assert!(!ledger_has(&db, MIGRATION_ID).unwrap());
    }

    #[test]
    fn a_fresh_instance_runs_with_zero_examined_and_stamps() {
        let db = db();
        assert_eq!(
            strip_anthropic_sampling_parameters(&db, NOW).unwrap(),
            LedgeredHealOutcome::Ran {
                rewritten: 0,
                total: 0
            }
        );
        assert!(ledger_has(&db, MIGRATION_ID).unwrap());
    }

    #[test]
    fn js_number_formatting_and_key_order_survive_the_rewrite() {
        let db = db();
        seed(
            &db,
            "a",
            "ANTHROPIC",
            Some(r#"{ "b": 1.0, "2": "x", "top_p": 1, "anthropic": {"cacheControl": true} }"#),
        );
        strip_anthropic_sampling_parameters(&db, NOW).unwrap();
        assert_eq!(
            params(&db, "a").as_deref(),
            Some(r#"{"2":"x","b":1,"anthropic":{"cacheControl":true}}"#)
        );
    }
}
