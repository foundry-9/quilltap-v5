//! The ONE chain of the `624d0d25c` round's boot ensures — P4.D362 (R-L2).
//!
//! v4 registers the round's migrations LAST at the pin
//! (`migrations/scripts/index.ts:950-960`):
//!
//! ```text
//! … addRoleplayTemplateUnmarkedFieldsMigration,
//! stripAnthropicSamplingParametersMigration,       ← here, step 1
//! reindexChatMessageFtsCharacterOnlyMigration,     ← NOT here (P4.D363)
//! addEstablishedFactFieldMigration,                ← here, step 2
//! addProfileMaxImagesPerRequestMigration           ← here, step 3
//! ```
//!
//! This chain runs the three P4.D362 owns in that order. The FTS
//! character-only reindex between them is P4.D363's STAMPED ensure, slotted
//! BEFORE the FTS reconcile in the host (v4 runs the reindex inside its
//! migration phase; v5's FTS shadow is built by the reconcile, so the reindex
//! must precede it) — it is deliberately not a step of this chain, and none of
//! the three steps here reads the FTS table, so the relative order is
//! unobservable.
//!
//! The host, the vintage restore's mirror boot
//! (`restore_vintage_state`), the zero-rerun differential's `run_chain` and
//! [`crate::test_support::ensure_4_12_columns_on`]'s column half all call this
//! ONE function (or its two column steps), so the lists cannot drift apart
//! (the P4.D288 lesson).

use rusqlite::Connection;

use super::chat_messages_established_fact_repair::ensure_chat_messages_established_fact;
use super::connection_profiles_max_images_repair::{
    ensure_connection_profiles_max_images, MaxImagesEnsureOutcome,
};
use super::connection_profiles_sampling_strip_heal::strip_anthropic_sampling_parameters;
use super::memory_rewrite_heals::LedgeredHealOutcome;
use super::DbError;

/// What the chain did, step by step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ensures412Outcome {
    /// `strip-anthropic-sampling-parameters-v1` (STAMPED).
    pub strip: LedgeredHealOutcome,
    /// `add-established-fact-field-v1` (UNSTAMPED) added the column.
    pub established_fact_added: bool,
    /// `add-profile-max-images-per-request-v1` (UNSTAMPED).
    pub max_images: MaxImagesEnsureOutcome,
}

/// Run the round's three main-partition steps in v4's registration order.
/// `now` stamps the strip's ledger row.
pub fn run_4_12_ensures(main: &Connection, now: &str) -> Result<Ensures412Outcome, DbError> {
    let strip = strip_anthropic_sampling_parameters(main, now)?;
    let established_fact_added = ensure_chat_messages_established_fact(main)?;
    let max_images = ensure_connection_profiles_max_images(main)?;
    Ok(Ensures412Outcome {
        strip,
        established_fact_added,
        max_images,
    })
}

/// The two COLUMN steps alone (no ledger write) — what a fixture that predates
/// the round needs before a write naming either column. The test twin
/// ([`crate::test_support::ensure_4_12_columns_on`]) calls it.
pub fn run_4_12_column_ensures(main: &Connection) -> Result<(), DbError> {
    ensure_chat_messages_established_fact(main)?;
    ensure_connection_profiles_max_images(main)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Item 14 (R-4): the two FRESH-ONLY columns stay untouched by v5 code — no
    /// `.rs` under `crates/quilltap-core/src/` outside the provisioning
    /// artifacts names `brahmaConnectionProfileId` in SQL text, and no
    /// `characters` writer's column list names `kind`. A fresh instance carries
    /// both columns and a v4-MIGRATED one never does, so nothing may assume
    /// they exist. ⚠ This census is P4.D369's / P4.D374's to WIDEN when they add
    /// their deliberate readers / writers (the Brahma key is a vault/store key,
    /// not a column read — P4.D374; `kind` is `MANAGED_FIELDS` — P4.D369).
    #[test]
    fn the_fresh_only_columns_are_named_by_no_v5_sql() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut offenders = Vec::new();
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs")
                    || path.ends_with("schema_4_12_repair.rs")
                {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap();
                for (n, line) in text.lines().enumerate() {
                    // SQL text names a column quoted inside a string literal.
                    if line.contains("\\\"brahmaConnectionProfileId\\\"")
                        || (line.contains("INSERT INTO characters") && line.contains(" kind"))
                        || line.contains("characters SET kind")
                    {
                        offenders.push(format!("{}:{}", path.display(), n + 1));
                    }
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "fresh-only column named in v5 SQL: {offenders:?}"
        );
    }

    #[test]
    fn the_chain_runs_in_order_and_is_idempotent() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            r#"CREATE TABLE "chat_messages" ("id" TEXT PRIMARY KEY);
               CREATE TABLE "connection_profiles" ("id" TEXT PRIMARY KEY, "provider" TEXT, "parameters" TEXT);
               INSERT INTO connection_profiles VALUES ('a', 'ANTHROPIC', '{"temperature":1}');"#,
        )
        .unwrap();
        let first = run_4_12_ensures(&db, "2026-11-05T00:00:00.000Z").unwrap();
        assert_eq!(
            first,
            Ensures412Outcome {
                strip: LedgeredHealOutcome::Ran {
                    rewritten: 1,
                    total: 1
                },
                established_fact_added: true,
                max_images: MaxImagesEnsureOutcome::Added { backfilled: 0 },
            }
        );
        let second = run_4_12_ensures(&db, "2026-11-05T00:00:01.000Z").unwrap();
        assert_eq!(
            second,
            Ensures412Outcome {
                strip: LedgeredHealOutcome::AlreadyCompleted,
                established_fact_added: false,
                max_images: MaxImagesEnsureOutcome::NotNeeded,
            }
        );
    }
}
