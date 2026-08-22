//! The retire-prefill-on-thinking-profiles boot heal (v4 migration
//! `retire-prefill-on-thinking-profiles-v1`, `97d2fcb5` — data pass,
//! `12fe3e6f` — its coverage).
//!
//! `add-profile-multi-character-prefill-field-v1` gave every connection
//! profile a `multiCharacterPrefill` column and backfilled it by provider:
//! 0 for ANTHROPIC, 1 for everything else. That preserved the behaviour of
//! the day, including its blind spot — the prefill's hostility is a property
//! of the *model*, not the provider, and a model that reasons is the one it
//! breaks on (v4 bugs 68 + 85). Fixing the default alone fixes nothing for
//! rows already written: the literal `1` those profiles carry was put there
//! by the old provider default at creation, not by any user choice, and it
//! outranks any default thereafter. This pass clears it — and only it.
//!
//! ## The once-only mechanism is v4's OWN migration ledger — not a column
//!
//! Unlike the P4.D79/D63/D73/D77 ensures, this pass has NO schema delta to
//! key off: it is data-only, and re-running it would clobber a user's
//! re-ticked `true` on a thinking profile. The only guard that survives BOTH
//! apps opening the same instance is v4's `migrations_state` ledger:
//!
//!   - v4's runner skips any migration whose ledger row exists
//!     (`isMigrationCompleted`, `migrations/index.ts:126`) — so a row THIS
//!     heal writes stops a later v4 run from re-clearing;
//!   - this heal skips when the row exists — so an instance v4 already
//!     migrated is never re-cleared by v5.
//!
//! The table and row are created exactly as v4's `migrations/state.ts`
//! creates them (lazy `CREATE TABLE IF NOT EXISTS`, the metadata table
//! bundled under the same absence check), which keeps this D23-compatible:
//! nothing is invented — v5 writes only shapes v4 itself writes, and only on
//! the heal's write path against an existing instance (the fresh-provisioning
//! schema does not carry `migrations_state`, and v4's own fresh instances
//! also gain the row only when its runner first executes — same observable
//! outcome). `quilltapVersion` records which APP stamped the row: v4 writes
//! its package version (`4.9.0-dev.43` at the pin); v5 stamps this crate's
//! version — the column is informational, the `id` is the key.
//!
//! Skips that write NO ledger row (matching v4's `shouldRun` semantics — a
//! skipped migration is retried on the next boot): the `connection_profiles`
//! table or the `multiCharacterPrefill` column being absent.
//!
//! ## The rule table below is a deliberate frozen copy
//!
//! At runtime the same question is answered by the provider registry's
//! declared `thinkingTurnRule` plus its model catalogue's `thinksByDefault`
//! flag, evaluated by [`crate::services::thinking_turn`]. v4's migrations run
//! before the plugin registry is up, so its migration carries its own
//! snapshot of what those plugins declared the day it was written — and this
//! heal carries the same one. Do not "fix" it to follow the manifests later —
//! a migration describes the world it ran in.

use rusqlite::Connection;
use serde_json::{json, Value};

use super::DbError;
use crate::services::thinking_turn::{
    evaluate_thinking_turn, ThinkingModelFacts, ThinkingTurnRule,
};

/// v4's migration id — the ledger key both apps honour.
const MIGRATION_ID: &str = "retire-prefill-on-thinking-profiles-v1";

/// Frozen snapshot of the `thinkingTurnRule` each provider plugin declared
/// when v4's migration was written. See the module note above. ORDER matters:
/// v4 builds the SQL `IN` placeholders from `Object.keys(FROZEN_RULES)`.
fn frozen_rules() -> [(&'static str, ThinkingTurnRule); 2] {
    [
        (
            "DEEPSEEK",
            ThinkingTurnRule {
                option_key: "thinking".into(),
                enabled_values: Some(vec![json!("enabled")]),
                disabled_values: Some(vec![json!("disabled")]),
            },
        ),
        (
            "OLLAMA",
            ThinkingTurnRule {
                option_key: "enable_thinking".into(),
                enabled_values: Some(vec![json!(true)]),
                disabled_values: Some(vec![json!(false)]),
            },
        ),
    ]
}

/// Frozen snapshot of the models whose catalogue entry said they reason
/// without being asked. Ollama contributes none — its models are whatever the
/// user has pulled, so only an explicit Enable Thinking counts there.
const FROZEN_THINKS_BY_DEFAULT_DEEPSEEK: &[&str] = &["deepseek-v4-flash", "deepseek-v4-pro"];

/// v4 `parseParameters`: raw cell → the bag the evaluator reads. Unparseable
/// (or null) → `{}`; v4's `typeof parsed === 'object'` admits arrays too, and
/// the evaluator reads no key off either shape, so passing the parsed value
/// through matches.
fn parse_parameters(raw: Option<&str>) -> Value {
    let Some(raw) = raw else { return json!({}) };
    match serde_json::from_str::<Value>(raw) {
        Ok(v) if v.is_object() || v.is_array() => v,
        _ => json!({}),
    }
}

/// One candidate row from the provider scan: `(id, provider, modelName,
/// parameters)` — the four columns v4's SELECT reads.
type CandidateRow = (String, Option<String>, Option<String>, Option<String>);

/// The heal's observable outcome (the differential + the host log read it).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RetireOutcome {
    /// The ledger already carries the row (either app completed the pass).
    AlreadyCompleted,
    /// The table/column is not there yet — retried next boot, nothing stamped
    /// (v4's `shouldRun() === false` arm).
    NotApplicable,
    /// The pass ran: `examined` candidate rows, `cleared` of them turned off,
    /// and the ledger row written.
    Ran { examined: usize, cleared: usize },
}

/// Run the retire pass once per instance, guarded by v4's own migration
/// ledger. `now_iso` stamps `completedAt`/`lastChecked` (the caller passes
/// [`crate::clock::now_iso`]).
pub fn retire_prefill_on_thinking_profiles(
    main: &Connection,
    now_iso: &str,
) -> Result<RetireOutcome, DbError> {
    // The completed check comes FIRST, exactly as v4's runner orders it
    // (`isMigrationCompleted` before `shouldRun`).
    if table_exists(main, "migrations_state")? {
        let mut stmt = main.prepare("SELECT 1 FROM \"migrations_state\" WHERE \"id\" = ?1")?;
        if stmt.exists([MIGRATION_ID])? {
            return Ok(RetireOutcome::AlreadyCompleted);
        }
    }

    // v4 `shouldRun`: table + column present — else skip WITHOUT stamping.
    if !table_exists(main, "connection_profiles")? {
        return Ok(RetireOutcome::NotApplicable);
    }
    if !column_names(main, "connection_profiles")?
        .iter()
        .any(|c| c == "multiCharacterPrefill")
    {
        return Ok(RetireOutcome::NotApplicable);
    }

    // Only rows still carrying the prefill, and only on the providers that
    // declared a thinking rule — everything else is already correct.
    let rules = frozen_rules();
    let mut stmt = main.prepare(
        "SELECT \"id\", \"provider\", \"modelName\", \"parameters\"\n           FROM \"connection_profiles\"\n          WHERE \"multiCharacterPrefill\" = 1\n            AND upper(\"provider\") IN (?1, ?2)",
    )?;
    let rows: Vec<CandidateRow> = stmt
        .query_map([rules[0].0, rules[1].0], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<Result<_, _>>()?;

    let mut cleared = 0usize;
    for (id, provider, model_name, parameters) in &rows {
        let provider_upper = provider.clone().unwrap_or_default().to_uppercase();
        let rule = rules
            .iter()
            .find(|(p, _)| *p == provider_upper)
            .map(|(_, r)| r);
        let thinks_by_default = provider_upper == "DEEPSEEK"
            && model_name
                .as_deref()
                .is_some_and(|m| FROZEN_THINKS_BY_DEFAULT_DEEPSEEK.contains(&m));
        let params = parse_parameters(parameters.as_deref());
        let runs = evaluate_thinking_turn(
            rule,
            Some(&params),
            Some(&ThinkingModelFacts {
                supports_thinking: None,
                thinks_by_default: Some(thinks_by_default),
            }),
        );
        if runs {
            main.execute(
                "UPDATE \"connection_profiles\" SET \"multiCharacterPrefill\" = 0 WHERE \"id\" = ?1",
                [id],
            )?;
            cleared += 1;
        }
    }

    // The ledger write — v4's `migrations/state.ts` shapes verbatim: both
    // tables created lazily under the migrations_state absence check, the row
    // appended, the two metadata keys upserted.
    if !table_exists(main, "migrations_state")? {
        main.execute_batch(
            "CREATE TABLE IF NOT EXISTS \"migrations_state\" (\n        \"id\" TEXT PRIMARY KEY,\n        \"completedAt\" TEXT NOT NULL,\n        \"quilltapVersion\" TEXT NOT NULL,\n        \"itemsAffected\" INTEGER NOT NULL DEFAULT 0,\n        \"message\" TEXT\n      );\n      CREATE TABLE IF NOT EXISTS \"migrations_metadata\" (\n        \"key\" TEXT PRIMARY KEY,\n        \"value\" TEXT NOT NULL\n      );",
        )?;
    }
    let message = format!(
        "Examined {} prefill-enabled profile(s) on thinking-capable providers; turned the [Name] prefill off on {}",
        rows.len(),
        cleared
    );
    main.execute(
        "INSERT INTO \"migrations_state\" (id, completedAt, quilltapVersion, itemsAffected, message)\n         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            MIGRATION_ID,
            now_iso,
            env!("CARGO_PKG_VERSION"),
            cleared as i64,
            message
        ],
    )?;
    for (k, v) in [
        ("lastChecked", now_iso),
        ("quilltapVersion", env!("CARGO_PKG_VERSION")),
    ] {
        main.execute(
            "INSERT INTO migrations_metadata (key, value) VALUES (?1, ?2)\n             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![k, v],
        )?;
    }

    Ok(RetireOutcome::Ran {
        examined: rows.len(),
        cleared,
    })
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool, DbError> {
    let mut stmt =
        conn.prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1")?;
    Ok(stmt.exists([name])?)
}

fn column_names(conn: &Connection, table: &str) -> Result<Vec<String>, DbError> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{table}\")"))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heal_vintage_db() -> Connection {
        let db = Connection::open_in_memory().expect("open");
        db.execute_batch(
            "CREATE TABLE connection_profiles (\n               id TEXT PRIMARY KEY,\n               name TEXT NOT NULL,\n               provider TEXT NOT NULL,\n               modelName TEXT,\n               parameters TEXT,\n               multiCharacterPrefill INTEGER DEFAULT 1\n             )",
        )
        .expect("ddl");
        db
    }

    fn seed(db: &Connection, id: &str, provider: &str, model: Option<&str>, params: Option<&str>) {
        db.execute(
            "INSERT INTO connection_profiles (id, name, provider, modelName, parameters) \
             VALUES (?1, ?1, ?2, ?3, ?4)",
            rusqlite::params![id, provider, model, params],
        )
        .expect("seed");
    }

    fn prefill(db: &Connection, id: &str) -> Option<i64> {
        db.query_row(
            "SELECT multiCharacterPrefill FROM connection_profiles WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .expect("read")
    }

    #[test]
    fn clears_thinking_rows_and_stamps_the_ledger() {
        let db = heal_vintage_db();
        seed(&db, "ds", "DEEPSEEK", Some("deepseek-v4-flash"), Some("{}"));
        seed(&db, "openai", "OPENAI", Some("gpt-5"), Some("{}"));

        let out = retire_prefill_on_thinking_profiles(&db, "2026-08-21T00:00:00.000Z")
            .expect("heal runs");
        assert_eq!(
            out,
            RetireOutcome::Ran {
                examined: 1,
                cleared: 1
            }
        );
        assert_eq!(prefill(&db, "ds"), Some(0));
        assert_eq!(prefill(&db, "openai"), Some(1));

        let (completed_at, version, items, message): (String, String, i64, String) = db
            .query_row(
                "SELECT completedAt, quilltapVersion, itemsAffected, message \
                 FROM migrations_state WHERE id = ?1",
                [MIGRATION_ID],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .expect("ledger row");
        assert_eq!(completed_at, "2026-08-21T00:00:00.000Z");
        assert_eq!(version, env!("CARGO_PKG_VERSION"));
        assert_eq!(items, 1);
        assert_eq!(
            message,
            "Examined 1 prefill-enabled profile(s) on thinking-capable providers; \
             turned the [Name] prefill off on 1"
        );
    }

    /// ⚠ The property the ledger exists for: a second boot is a no-op, and a
    /// re-ticked `true` on a thinking profile survives it.
    #[test]
    fn a_second_boot_never_reclobbers_a_reticked_true() {
        let db = heal_vintage_db();
        seed(&db, "ds", "DEEPSEEK", Some("deepseek-v4-flash"), Some("{}"));

        retire_prefill_on_thinking_profiles(&db, "2026-08-21T00:00:00.000Z").expect("first");
        assert_eq!(prefill(&db, "ds"), Some(0));

        // The user overrules us (permitted — the editor warns, never vetoes).
        db.execute(
            "UPDATE connection_profiles SET multiCharacterPrefill = 1 WHERE id = 'ds'",
            [],
        )
        .expect("re-tick");

        let out = retire_prefill_on_thinking_profiles(&db, "2026-08-22T00:00:00.000Z")
            .expect("second boot");
        assert_eq!(out, RetireOutcome::AlreadyCompleted);
        assert_eq!(prefill(&db, "ds"), Some(1), "the re-ticked true survives");
    }

    /// The cross-app half: a ledger row v4's runner already wrote (any
    /// completedAt/version) makes this heal skip — v4 migrated first.
    #[test]
    fn a_v4_written_ledger_row_makes_the_heal_skip() {
        let db = heal_vintage_db();
        seed(&db, "ds", "DEEPSEEK", Some("deepseek-v4-flash"), Some("{}"));
        db.execute_batch(
            "CREATE TABLE migrations_state (id TEXT PRIMARY KEY, completedAt TEXT NOT NULL, \
             quilltapVersion TEXT NOT NULL, itemsAffected INTEGER NOT NULL DEFAULT 0, message TEXT); \
             INSERT INTO migrations_state VALUES \
             ('retire-prefill-on-thinking-profiles-v1', '2026-08-21T01:02:03.000Z', '4.9.0-dev.43', 1, 'x');",
        )
        .expect("v4 ledger");

        let out =
            retire_prefill_on_thinking_profiles(&db, "2026-08-22T00:00:00.000Z").expect("heal");
        assert_eq!(out, RetireOutcome::AlreadyCompleted);
        assert_eq!(
            prefill(&db, "ds"),
            Some(1),
            "v4's completed pass is honoured"
        );
    }

    /// Absent table/column: retried next boot, nothing stamped (v4's
    /// `shouldRun` false arm records nothing).
    #[test]
    fn missing_table_or_column_skips_without_stamping() {
        let db = Connection::open_in_memory().expect("open");
        let out =
            retire_prefill_on_thinking_profiles(&db, "2026-08-21T00:00:00.000Z").expect("no table");
        assert_eq!(out, RetireOutcome::NotApplicable);
        assert!(!table_exists(&db, "migrations_state").unwrap());

        db.execute_batch("CREATE TABLE connection_profiles (id TEXT PRIMARY KEY, provider TEXT)")
            .expect("pre-4.9 table");
        let out = retire_prefill_on_thinking_profiles(&db, "2026-08-21T00:00:00.000Z")
            .expect("no column");
        assert_eq!(out, RetireOutcome::NotApplicable);
        assert!(!table_exists(&db, "migrations_state").unwrap());
    }
}
