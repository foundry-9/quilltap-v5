//! The startup seeding wire (v4 `lib/startup/seed-initial-data.ts` — the
//! sample-content slice). On a first boot (the **zero-characters gate**) this
//! imports the committed `.qtap` seed and seeds the seed characters' avatars, so
//! a fresh v5 instance boots with Lorian + Riya + 42 memories + Lorian's avatar,
//! matching a fresh v4 instance.
//!
//! Every layer **swallows and collects** its errors (v4: "seeding failure should
//! not prevent startup") — nothing here ever blocks the boot. Core carries no
//! logger, so per-item failures accumulate into [`SeedReport::warnings`] for the
//! caller (and the smoke test) to inspect.
//!
//! Run inside a single [`crate::db::runtime::Db::write`] closure (both `main` +
//! `mount` writable) — the host wires it into its per-assemble seed step behind
//! the gate. The built-in roleplay templates + mount stores run UNGATED before
//! this (P4.4u3); this is the gated `seedFromImports` + `seedAvatars` tail.

use rusqlite::Connection;
use serde_json::Value;

use super::seed_assets::{LORIAN_AND_RIYA_QTAP, SEED_AVATARS};
use super::{execute_import, parse_export_file, ImportOptions};
use crate::db::characters::{CharacterUpdate, CharactersRepository};
use crate::db::{characters_read, DbError};
use crate::photos::resolve_character_avatar::resolve_character_avatar;
use crate::services::file_storage::PixelCodec;
use crate::services::image_job_storage::write_main_avatar_to_vault;
use crate::services::provisioning::SINGLE_USER_ID;

/// A summary of what the seed did — for the caller and the smoke test. Warnings
/// collect the per-item failures v4 logs and swallows.
#[derive(Debug, Default, Clone)]
pub struct SeedReport {
    /// True when the zero-characters gate short-circuited (already seeded).
    pub gate_short_circuited: bool,
    pub imported_characters: u32,
    pub imported_memories: u32,
    pub avatars_written: u32,
    pub warnings: Vec<String>,
}

/// v4 `seedInitialData`'s gated tail (the sample-content slice): the
/// zero-characters gate, then `seedFromImports` + `seedAvatars`. Never errors —
/// every failure is collected into the report. Run inside a `Db::write` closure.
pub fn seed_sample_content(
    main: &Connection,
    mount: &Connection,
    codec: &dyn PixelCodec,
) -> SeedReport {
    let mut report = SeedReport::default();

    // The zero-characters gate (v4 :66–68: `findByUserId(SINGLE_USER_ID).length
    // > 0 → return`). Idempotent: a re-boot with characters present short-circuits.
    match character_count(main, SINGLE_USER_ID) {
        Ok(0) => {}
        Ok(_) => {
            report.gate_short_circuited = true;
            return report;
        }
        Err(e) => {
            report.warnings.push(format!("seed gate check failed: {e}"));
            return report;
        }
    }

    seed_from_imports(main, mount, &mut report);
    seed_avatars(main, mount, codec, None, &mut report);
    report
}

/// v4 `seedFromImports` (:197): load every seed `.qtap` (here: the one committed
/// asset) and run it through `executeImport` with `{skip, includeMemories:true,
/// includeRelatedEntities:false}`. Per-file try/catch → warn + continue.
fn seed_from_imports(main: &Connection, mount: &Connection, report: &mut SeedReport) {
    let export = match parse_export_file(LORIAN_AND_RIYA_QTAP) {
        Ok(e) => e,
        Err(e) => {
            report.warnings.push(format!(
                "Failed to parse seed import lorian-and-riya.qtap: {e}"
            ));
            return;
        }
    };
    match execute_import(
        main,
        mount,
        SINGLE_USER_ID,
        &export,
        &ImportOptions::seed_defaults(),
        None,
    ) {
        Ok(result) => {
            report.imported_characters = result.imported.characters;
            report.imported_memories = result.imported.memories;
            report.warnings.extend(result.warnings);
        }
        Err(e) => {
            report.warnings.push(format!(
                "Failed to execute seed import lorian-and-riya.qtap: {e}"
            ));
        }
    }
}

/// v4 `seedAvatars` (:254): match each seed avatar file to a character by
/// case-insensitive name, skip when the character already has a valid avatar
/// (idempotency), else write the avatar into the vault (delete-then-insert) and
/// point `defaultImageId` at the new link. `target_names` restricts which
/// avatars to (re)seed (v4 `reseedAvatarsForCharacters` passes the builtins).
pub fn seed_avatars(
    main: &Connection,
    mount: &Connection,
    codec: &dyn PixelCodec,
    target_names: Option<&[&str]>,
    report: &mut SeedReport,
) {
    let normalized_targets: Option<Vec<String>> =
        target_names.map(|names| names.iter().map(|n| n.to_lowercase()).collect());

    let avatars: Vec<&super::seed_assets::SeedAvatar> = SEED_AVATARS
        .iter()
        .filter(|a| match &normalized_targets {
            None => true,
            Some(t) if t.is_empty() => true,
            Some(t) => t.contains(&a.character_name.to_lowercase()),
        })
        .collect();

    if avatars.is_empty() {
        return;
    }

    // All characters for the single user, to match by name.
    let all_characters = match characters_read::find_by_user_id(main, mount, SINGLE_USER_ID) {
        Ok(c) => c,
        Err(e) => {
            report
                .warnings
                .push(format!("avatar seeding: failed to list characters: {e}"));
            return;
        }
    };

    for avatar in avatars {
        let name_lc = avatar.character_name.to_lowercase();
        let Some(character) = all_characters.iter().find(|c| {
            c.get("name")
                .and_then(Value::as_str)
                .map(|n| n.to_lowercase() == name_lc)
                .unwrap_or(false)
        }) else {
            report.warnings.push(format!(
                "No matching character found for seed avatar {}",
                avatar.character_name
            ));
            continue;
        };

        let character_id = character
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        // Idempotency: skip when the character already has a resolvable avatar.
        let existing_default = character
            .get("defaultImageId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty());
        if let Some(existing) = existing_default {
            match resolve_character_avatar(main, mount, Some(existing)) {
                Ok(Some(_)) => continue,
                Ok(None) => {}
                Err(e) => {
                    report.warnings.push(format!(
                        "avatar seeding: resolve check failed for {}: {e}",
                        avatar.character_name
                    ));
                    // v4 would throw into the per-avatar catch here; treat as a
                    // per-item failure and continue.
                    continue;
                }
            }
        }

        // Write the avatar into the character vault (main kind, delete-then-insert)
        // and repoint defaultImageId at the new link.
        match write_main_avatar_to_vault(
            main,
            mount,
            codec,
            &character_id,
            avatar.filename,
            avatar.content,
            avatar.mime_type,
            None,
        ) {
            Ok(written) => {
                let now = crate::clock::now_iso();
                let patch = CharacterUpdate {
                    default_image_id: Some(written.link_id),
                    updated_at: now,
                    ..Default::default()
                };
                if let Err(e) = CharactersRepository::new(main).update(&character_id, &patch) {
                    report.warnings.push(format!(
                        "avatar seeding: failed to set defaultImageId for {}: {e}",
                        avatar.character_name
                    ));
                } else {
                    report.avatars_written += 1;
                }
            }
            Err(e) => {
                report.warnings.push(format!(
                    "Failed to seed avatar for character {}: {e}",
                    avatar.character_name
                ));
            }
        }
    }
}

/// v4 `reseedAvatarsForCharacters` (:361) — the reset-builtins entry point.
/// Reseeds only the named characters' avatars.
pub fn reseed_avatars_for_characters(
    main: &Connection,
    mount: &Connection,
    codec: &dyn PixelCodec,
    character_names: &[&str],
) -> SeedReport {
    let mut report = SeedReport::default();
    seed_avatars(main, mount, codec, Some(character_names), &mut report);
    report
}

/// The zero-characters gate count (v4 `findByUserId(SINGLE_USER_ID).length`).
fn character_count(main: &Connection, user_id: &str) -> Result<i64, DbError> {
    Ok(main.query_row(
        "SELECT count(*) FROM characters WHERE userId = ?1",
        [user_id],
        |r| r.get::<_, i64>(0),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::file_storage::NotConfiguredPixelCodec;

    /// The bug-59 fail-closed pin (v4 4.8.1): a database whose emptiness probe
    /// CANNOT be answered must not seed — an unanswerable question is not a
    /// yes. v4's `seedInitialData` probes with `countOrThrow` and skips (with
    /// an explicit log line) when the probe throws; v5's gate was already this
    /// shape — `character_count` returns `Result`, and the `Err` arm records a
    /// warning and returns without seeding. This test is the regression pin
    /// porting the INTENT of v4's `seed-initial-data.test.ts` probe-throws
    /// case: the probe here fails because the `characters` table is missing
    /// (a half-materialized or unreachable database reads the same way), and
    /// nothing may be written.
    #[test]
    fn failed_gate_probe_seeds_nothing() {
        // Schema-less connections: the characters read errors (`no such table`).
        let main = Connection::open_in_memory().unwrap();
        let mount = Connection::open_in_memory().unwrap();

        let report = seed_sample_content(&main, &mount, &NotConfiguredPixelCodec);

        // Fail closed: not the short-circuit arm, nothing imported, and the
        // skip is SAID (v4 logs "Skipping initial-data seeding"; v5 reports).
        assert!(!report.gate_short_circuited);
        assert_eq!(report.imported_characters, 0);
        assert_eq!(report.imported_memories, 0);
        assert_eq!(report.avatars_written, 0);
        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].starts_with("seed gate check failed:"));

        // And the seeding path truly never ran: neither database gained a table.
        for conn in [&main, &mount] {
            let n: i64 = conn
                .query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 0);
        }
    }
}
