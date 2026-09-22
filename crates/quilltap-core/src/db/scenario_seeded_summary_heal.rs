//! The clear-scenario-seeded-chat-summaries boot heal (v4 migration
//! `clear-scenario-seeded-chat-summaries-v1`, `da9c4f34f` — bug 158's data
//! pass).
//!
//! Until bug 158, creating a chat wrote the chosen scenario into
//! `contextSummary` as well as `scenarioText` — a leftover from before
//! `add-chat-scenario-text-field-v1` (4.1.0) gave the scenario a column of its
//! own. Every reader of `contextSummary` therefore believed a brand-new chat
//! had already been summarized. The one that hurt was the greeting's "Recent
//! Conversations" block, which inlined the column whole: a prior chat that was
//! opened and never summarized contributed its entire raw scenario to the next
//! greeting's prompt, and the character opened in that room instead of the one
//! the operator chose. On v4's own live instance, 186 of 712 chats were in that
//! state.
//!
//! The seed is gone ([`crate::services::chat_create`]). This clears the rows
//! already on disk: `contextSummary` to NULL wherever it is byte-identical to
//! the same row's `scenarioText`.
//!
//! Equality is the whole predicate, deliberately. A real summary is written only
//! by the fold in [`crate::services::context_summary`], which replaces the
//! column outright — so a chat that has been summarized even once no longer
//! matches, and a summary that happens to quote the scenario is not
//! byte-identical to it. Rows where either column is NULL are left alone; so is
//! a chat whose scenario is the empty string, which carries no information
//! either way.
//!
//! [`crate::services::scenario_seeded_summary`] states the same rule in Rust.
//! **The two must agree — change both or neither**; `the_sql_and_the_predicate_
//! agree` is that pairing under test.
//!
//! ## The once-only mechanism is v4's OWN migration ledger — not a column
//!
//! Like the P4.D97 retire-prefill and P4.D140 recompute heals (this module's
//! templates), the pass is DATA-only with no schema delta to key off, and
//! re-running it would be harmless but pointless. The only guard that survives
//! BOTH apps opening the same instance is v4's `migrations_state` ledger: v4's
//! runner skips any migration whose ledger row exists, and this heal skips when
//! the row exists.
//!
//! **A clean boot writes NO ledger row** — v4's `shouldRun()` here COUNTS the
//! seeded rows, so it is false when nothing has drifted and its runner never
//! records the migration. v5 must match exactly: a stamp on a clean boot would
//! make a LATER v4 boot skip a migration it never ran.
//!
//! v4's `dependsOn: ['sqlite-initial-schema-v1', 'add-chat-scenario-text-field-v1']`
//! is its runner's ordering contract, which v5 has no analogue for; the column
//! check below is what it buys in practice. The prettify label v4 adds alongside
//! the migration (*"Separating the scene that was set from the tale that was
//! told, so nobody greets you from the wrong room…"*) is v4-runner UI with no v5
//! counterpart — a deliberate non-port.

use rusqlite::Connection;

use super::DbError;

/// v4's migration id — the ledger key both apps honour.
const MIGRATION_ID: &str = "clear-scenario-seeded-chat-summaries-v1";

/// The seeded shape, in SQL. **ONE string**, exactly as v4 keeps it, "so the
/// count and the UPDATE can never disagree about what they are addressing".
///
/// Note `"scenarioText" <> ''` — an empty scenario is not a seed, matching the
/// `!scenario.is_empty()` guard in
/// [`crate::services::scenario_seeded_summary::is_scenario_seeded_summary`].
const SEEDED_WHERE: &str = "\n  \"contextSummary\" IS NOT NULL\n  AND \"scenarioText\" IS NOT NULL\n  AND \"scenarioText\" <> ''\n  AND \"contextSummary\" = \"scenarioText\"\n";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SeededSummaryHealOutcome {
    /// The ledger already carries the row (either app completed the pass).
    AlreadyCompleted,
    /// The table or a column is not there yet — retried next boot, nothing
    /// stamped (v4's `shouldRun() === false` arm).
    NotApplicable,
    /// No chat is carrying its scenario as a summary. v4's runner skips and
    /// records NOTHING, so neither does this.
    NoDrift,
    /// The pass ran: `cleared` chats had their summary nulled, and the ledger
    /// row was written.
    Ran { cleared: usize },
}

/// v4's `run()` message for the no-drift branch. Neither runner reaches it
/// (`shouldRun()` gates `run()`), and neither does v5's boot path — it is
/// carried, and pinned, so the two implementations describe the same world.
pub const NO_DRIFT_MESSAGE: &str = "No conversation is carrying its scenario as a summary";

/// v4's `run()` message for the cleared branch, with its singular/plural.
pub fn cleared_message(cleared: usize) -> String {
    format!(
        "Cleared the scenario standing in as a summary on {cleared} conversation{}",
        if cleared == 1 { "" } else { "s" }
    )
}

fn count_seeded(conn: &Connection) -> Result<usize, DbError> {
    let sql = format!("SELECT COUNT(*) AS n FROM \"chats\" WHERE {SEEDED_WHERE}");
    let n: i64 = conn.query_row(&sql, [], |r| r.get(0))?;
    Ok(n as usize)
}

/// Run the clear once per instance, guarded by v4's own migration ledger.
/// `now_iso` stamps the cleared rows' `updatedAt` and the ledger's
/// `completedAt`/`lastChecked` (the caller passes [`crate::clock::now_iso`]).
pub fn clear_scenario_seeded_chat_summaries(
    main: &Connection,
    now_iso: &str,
) -> Result<SeededSummaryHealOutcome, DbError> {
    // The completed check comes FIRST, exactly as v4's runner orders it
    // (`isMigrationCompleted` before `shouldRun`).
    if table_exists(main, "migrations_state")? {
        let mut stmt = main.prepare("SELECT 1 FROM \"migrations_state\" WHERE \"id\" = ?1")?;
        if stmt.exists([MIGRATION_ID])? {
            return Ok(SeededSummaryHealOutcome::AlreadyCompleted);
        }
    }

    // v4 `shouldRun`'s `chatsTableUsable()`: the table and BOTH columns, else
    // skip WITHOUT stamping.
    if !table_exists(main, "chats")? {
        return Ok(SeededSummaryHealOutcome::NotApplicable);
    }
    let columns = column_names(main, "chats")?;
    if !["contextSummary", "scenarioText"]
        .iter()
        .all(|want| columns.iter().any(|c| c == want))
    {
        return Ok(SeededSummaryHealOutcome::NotApplicable);
    }

    let to_clear = count_seeded(main)?;
    tracing::debug!(
        target: "quilltap::db",
        chats = to_clear,
        "Scanning chats for scenario-seeded summaries"
    );
    if to_clear == 0 {
        // v4's `shouldRun()` is false here: no run, NO ledger row, retried next
        // boot. See the module header — a stamp would poison v4's own skip.
        return Ok(SeededSummaryHealOutcome::NoDrift);
    }

    // v4 bumps `updatedAt` on exactly the rows it clears, in the SAME statement,
    // under the SAME predicate string.
    let sql = format!(
        "UPDATE \"chats\" SET \"contextSummary\" = NULL, \"updatedAt\" = ?1 WHERE {SEEDED_WHERE}"
    );
    let cleared = main.execute(&sql, rusqlite::params![now_iso])?;

    tracing::info!(
        target: "quilltap::db",
        cleared,
        "Cleared scenario-seeded chat summaries"
    );

    super::migrations_ledger::ensure_migrations_tables(main)?;
    main.execute(
        "INSERT INTO \"migrations_state\" (id, completedAt, quilltapVersion, itemsAffected, message)\n         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            MIGRATION_ID,
            now_iso,
            env!("CARGO_PKG_VERSION"),
            cleared as i64,
            cleared_message(cleared)
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

    Ok(SeededSummaryHealOutcome::Ran { cleared })
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool, DbError> {
    let mut stmt =
        conn.prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1")?;
    Ok(stmt.exists([name])?)
}

fn column_names(conn: &Connection, table: &str) -> Result<Vec<String>, DbError> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{table}\")"))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v4 keeps `SEEDED_WHERE` as ONE string so "the count and the UPDATE can
    /// never disagree about what they are addressing". v5 keeps the same
    /// discipline; this is that discipline under test, by comparing the BYTES
    /// both statements embed rather than trusting that they were written from
    /// the same constant.
    #[test]
    fn the_count_and_the_update_embed_the_same_predicate() {
        let count = format!("SELECT COUNT(*) AS n FROM \"chats\" WHERE {SEEDED_WHERE}");
        let update = format!(
            "UPDATE \"chats\" SET \"contextSummary\" = NULL, \"updatedAt\" = ?1 WHERE {SEEDED_WHERE}"
        );
        let count_pred = count.split_once(" WHERE ").unwrap().1;
        let update_pred = update.split_once(" WHERE ").unwrap().1;
        assert_eq!(count_pred, update_pred);
        assert_eq!(count_pred, SEEDED_WHERE);
    }

    /// The SQL and the Rust predicate are two statements of one rule, and v4
    /// says so in both files: "change both or neither". This drives the SAME
    /// rows through both and requires them to agree, so a later edit to one
    /// alone is a red rather than a silent divergence.
    #[test]
    fn the_sql_and_the_predicate_agree() {
        use crate::services::scenario_seeded_summary::is_scenario_seeded_summary;
        use serde_json::json;

        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE \"chats\" (\"id\" TEXT PRIMARY KEY, \"contextSummary\" TEXT, \
             \"scenarioText\" TEXT, \"updatedAt\" TEXT);",
        )
        .unwrap();

        // (id, contextSummary, scenarioText)
        let rows: &[(&str, Option<&str>, Option<&str>)] = &[
            ("seeded", Some("a scene"), Some("a scene")),
            (
                "real-summary",
                Some("a scene and then some"),
                Some("a scene"),
            ),
            ("no-scenario-null", Some("a scene"), None),
            ("no-summary-null", None, Some("a scene")),
            ("both-null", None, None),
            ("both-empty", Some(""), Some("")),
            ("empty-scenario", Some("a scene"), Some("")),
            ("whitespace-differs", Some("a scene\n"), Some("a scene")),
            ("seeded-whitespace-only", Some("   "), Some("   ")),
            ("seeded-unicode", Some("a 𝒳 ☃ b"), Some("a 𝒳 ☃ b")),
        ];
        for (id, cs, st) in rows {
            conn.execute(
                "INSERT INTO \"chats\" (id, \"contextSummary\", \"scenarioText\", \"updatedAt\") \
                 VALUES (?1, ?2, ?3, 't0')",
                rusqlite::params![id, cs, st],
            )
            .unwrap();
        }

        let sql = format!("SELECT id FROM \"chats\" WHERE {SEEDED_WHERE} ORDER BY id");
        let mut stmt = conn.prepare(&sql).unwrap();
        let mut by_sql: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        by_sql.sort();

        let mut by_predicate: Vec<String> = rows
            .iter()
            .filter(|(_, cs, st)| {
                is_scenario_seeded_summary(&json!({
                    "contextSummary": cs,
                    "scenarioText": st,
                }))
            })
            .map(|(id, _, _)| (*id).to_string())
            .collect();
        by_predicate.sort();

        assert_eq!(
            by_sql, by_predicate,
            "the heal's SQL and the Rust predicate must address the same rows"
        );
        assert!(!by_sql.is_empty(), "no row reached either — nothing proven");
    }
}
