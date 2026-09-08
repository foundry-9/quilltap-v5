//! The one-time head-and-shoulders backfill SCAN (v4
//! `lib/startup/enqueue-headshoulders-backfill.ts`, P4.82).
//!
//! Enqueues a `CHARACTER_HEADSHOULDERS_BACKFILL` job for every existing
//! character that has appearance text but no `headAndShouldersPrompt` yet. v4
//! ENQUEUES rather than generating inline for two reasons it states outright:
//! per-character generation must not block the startup loading screen, and a
//! job survives a missing or cold provider via retry.
//!
//! ## ⚠ The gate is a SHARED, cross-app flag — by design
//!
//! `instance_settings['headshoulders_backfill_enqueued_v1']` is the same row v4
//! writes. v4 has set it on every instance it has booted since the feature
//! shipped, the Friday copy included — so a v5 boot on such an instance MUST
//! scan nothing, enqueue nothing and write nothing. That is not a degraded
//! path; it is the contract. The reverse holds too: once v5 has run the scan,
//! a later v4 boot honours v5's flag. The dogfood pass proves the cross-app
//! leg on real data.
//!
//! Unlike the `migrations_state`-guarded boot heals next door, this flag is
//! written EVEN WHEN ZERO jobs were enqueued (v4's own comment: "so
//! steady-state startups no-op"), because the scan itself is the expensive
//! part, not the enqueues.
//!
//! ## Where it runs in v5
//!
//! v4 chains it LAST in `instrumentation.ts`'s vault-backfill `.then`, behind
//! `backfillCharacterVaults → migrateVaultPhysicalFiles → refreshVaultWardrobe
//! → moveSharedWardrobeToGeneral`. v5 has no twin of that chain as a boot stage
//! (those v4 migrations were absorbed elsewhere), so the v5 slot is the
//! boot-repair thread `seed_built_ins`, after every repair in it — the same
//! spawned, off-the-boot-path thread, which is what "must not block the loading
//! screen" means here.
//!
//! ## The scan's whitespace rule DISAGREES with the handler's, deliberately
//!
//! `hasSeed` is `Boolean(mediumPrompt || shortPrompt || …)` — **not trimmed**,
//! where the handler trims. So a whitespace-only `mediumPrompt` gets a job
//! enqueued that then returns silently. Both halves are v4's and both are
//! pinned by a fixture character.
//!
//! ## One gate that no differential can discriminate — measured, not assumed
//!
//! Deleting the `characterDocumentMountPointId` gate leaves this family GREEN,
//! and that is correct rather than a coverage hole: a vault-LESS character's
//! `physicalDescription` reads back OMITTED (the managed field sits at its Zod
//! default on both sides), so the very next gate skips it anyway. The two arms
//! are outcome-equivalent for the only shape v4 can produce — there is no
//! "linked vault, no physical description" character to seed. The fixture
//! builder's header carries the measurement; the mutation is recorded as
//! SURVIVING BY DESIGN so a later reader does not chase it.
//!
//! ## NO-COUNTERPART: the per-character yield
//!
//! v4 awaits `new Promise(resolve => setImmediate(resolve))` after each
//! character so a large library does not hog the Node event loop during
//! startup. v5's scan runs on its own OS thread inside the writer's closure —
//! there is no shared event loop to yield to, and a yield would only lengthen
//! the transaction. Recorded, not simulated.

use rusqlite::Connection;
use serde_json::Value;

use crate::db::characters_read;
use crate::services::headshoulders_backfill_job::head_prompt_is_filled;
use crate::services::queue_service::enqueue_character_headshoulders_backfill_blocking;

/// v4's `FLAG_KEY`.
pub const FLAG_KEY: &str = "headshoulders_backfill_enqueued_v1";

/// v4's `maxAttempts: 3` — "a cold job child whose plugins/provider aren't
/// ready yet should retry, not burn its one shot. Generation is idempotent."
const MAX_ATTEMPTS: f64 = 3.0;

/// v4 `HeadShouldersBackfillEnqueueResult`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeadShouldersBackfillEnqueueResult {
    pub scanned: usize,
    pub enqueued: usize,
    pub skipped: usize,
    pub already_done: bool,
}

/// v4 `enqueueHeadShouldersBackfill`.
///
/// Never fails the boot: every failure arm warns and continues, exactly as v4's
/// does. `main` must be the WRITER's connection (the scan enqueues and writes
/// the flag).
pub fn enqueue_headshoulders_backfill(
    main: &Connection,
    mount: &Connection,
) -> HeadShouldersBackfillEnqueueResult {
    let mut result = HeadShouldersBackfillEnqueueResult::default();

    if has_run(main) {
        result.already_done = true;
        return result;
    }

    // Overlay-aware so `physicalDescription` is hydrated from each vault; the
    // BATCH overlay drops broken vaults rather than throwing (v4's comment —
    // and v5's `find_all` has the same drop semantics).
    let characters = match characters_read::find_all(main, mount) {
        Ok(c) => c,
        Err(e) => {
            // v4's `findAll` failure would reject the whole startup promise,
            // which its caller's `.catch` warns over. v5's boot chain must not
            // fail either, so the same shape: warn, return the zero result.
            tracing::warn!(
                target: "quilltap::boot",
                error = %e,
                "Failed to scan characters for the head-and-shoulders backfill"
            );
            return result;
        }
    };
    result.scanned = characters.len();

    tracing::info!(
        target: "quilltap::boot",
        total = characters.len(),
        "Head-and-shoulders backfill scanning"
    );

    for character in &characters {
        // No vault → nowhere to persist the field (the job cannot provision
        // one). The vault backfill ran earlier in v4's startup chain.
        let has_vault = character
            .get("characterDocumentMountPointId")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty());
        if !has_vault {
            result.skipped += 1;
            continue;
        }
        let Some(pd) = character
            .get("physicalDescription")
            .filter(|v| !v.is_null())
        else {
            result.skipped += 1;
            continue;
        };
        if head_prompt_is_filled(pd) {
            result.skipped += 1;
            continue;
        }
        // v4 `Boolean(a || b || c || d || e)` — NOT trimmed here (see the
        // module header's asymmetry note).
        let has_seed = [
            "mediumPrompt",
            "shortPrompt",
            "longPrompt",
            "completePrompt",
            "fullDescription",
        ]
        .iter()
        .any(|k| {
            pd.get(*k)
                .and_then(Value::as_str)
                .is_some_and(|s| !s.is_empty())
        });
        if !has_seed {
            result.skipped += 1;
            continue;
        }

        let (Some(user_id), Some(character_id)) = (
            character.get("userId").and_then(Value::as_str),
            character.get("id").and_then(Value::as_str),
        ) else {
            result.skipped += 1;
            continue;
        };

        match enqueue_character_headshoulders_backfill_blocking(
            main,
            user_id,
            character_id,
            MAX_ATTEMPTS,
        ) {
            Ok((_, true)) => result.enqueued += 1,
            Ok((_, false)) => result.skipped += 1,
            Err(e) => {
                tracing::warn!(
                    target: "quilltap::boot",
                    character_id = %character_id,
                    error = %e,
                    "Failed to enqueue head-and-shoulders backfill"
                );
            }
        }
        // v4's `setImmediate` yield has no counterpart here — see the module
        // header.
    }

    // Record the flag even if zero were enqueued, so steady-state startups
    // no-op.
    mark_run(main);
    tracing::info!(
        target: "quilltap::boot",
        scanned = result.scanned,
        enqueued = result.enqueued,
        skipped = result.skipped,
        "Head-and-shoulders backfill enqueue complete"
    );
    result
}

/// v4 `hasRun()` — the raw `instance_settings` read, `=== 'true'`. A read
/// FAILURE (v4: a missing table makes `db.prepare(…).get()` throw) warns and is
/// treated as not-yet-run, so a fresh instance still gets its scan.
fn has_run(main: &Connection) -> bool {
    match main.prepare(r#"SELECT "value" FROM "instance_settings" WHERE "key" = ?"#) {
        Ok(mut stmt) => {
            match stmt.query_row([FLAG_KEY], |row| row.get::<_, Option<String>>(0)) {
                Ok(v) => v.as_deref() == Some("true"),
                // v4's `.get()` returns `undefined` for no row — NOT a throw —
                // so an absent row is simply "not run", with no warn.
                Err(rusqlite::Error::QueryReturnedNoRows) => false,
                Err(e) => {
                    warn_flag_read(&e.to_string());
                    false
                }
            }
        }
        Err(e) => {
            warn_flag_read(&e.to_string());
            false
        }
    }
}

fn warn_flag_read(error: &str) {
    tracing::warn!(
        target: "quilltap::boot",
        error = %error,
        "Failed to read head-and-shoulders backfill flag; treating as not-yet-run"
    );
}

/// v4 `markRun()` — the upsert. A write error warns and is never a failure.
fn mark_run(main: &Connection) {
    let sql = r#"INSERT INTO "instance_settings" ("key", "value") VALUES (?, ?)
       ON CONFLICT("key") DO UPDATE SET "value" = excluded."value""#;
    if let Err(e) = main.execute(sql, rusqlite::params![FLAG_KEY, "true"]) {
        tracing::warn!(
            target: "quilltap::boot",
            error = %e,
            "Failed to record head-and-shoulders backfill flag"
        );
    }
}

/// Read the flag back — the boot test's comparand, and the only public reader.
pub fn flag_value(main: &Connection) -> Option<String> {
    main.query_row(
        r#"SELECT "value" FROM "instance_settings" WHERE "key" = ?"#,
        [FLAG_KEY],
        |row| row.get::<_, String>(0),
    )
    .ok()
}
