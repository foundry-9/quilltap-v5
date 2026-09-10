//! The clear-generated-image-placeholder-descriptions boot heal (v4 migration
//! `clear-generated-image-placeholder-descriptions-v1`, `78b381a96` — bug 132's
//! data pass).
//!
//! Two image jobs wrote a LABEL into the column every reader treats as "what
//! this picture shows". The story-background job stored `Story background for:
//! <scene or chat title>` and the wardrobe-portrait job stored `<Name> —
//! wardrobe portrait`, on the `files` row and on the Scriptorium link beside
//! it. `describe_image` served whatever sat in that column before it would look
//! at the generation prompt or spend a vision call, so a character asking what
//! a backdrop depicted was told its chat title.
//!
//! **This pass is the fix's load-bearing half, not a tidy-up.** The forward
//! change ([`crate::services::story_background_job`],
//! [`crate::services::character_avatar_job`],
//! [`crate::tools::photo::handle_describe_image_precheck`]) only governs images
//! written from now on. For every image already on disk the caption still sits
//! in the column, and
//! [`crate::photos::auto_describe_attachment::auto_describe_precheck`] answers
//! `already-described` on the strength of it — so the vision tier is
//! unreachable until the column is cleared.
//!
//! `files.description` goes to NULL and `doc_mount_file_links.description` to
//! its `''` default. Only `source = 'GENERATED'` files are touched on the main
//! side; the link side has no source column, so it matches on the two exact
//! label shapes and an image MIME type.
//!
//! ## ⚠ Blast radius, named because it is real
//!
//! [`crate::api::chat_media`]'s `ensure_image_description` treats the LINK
//! description as a CACHE (`chat_media.rs:1465`: a non-blank
//! `blob.description` is returned as-is and no vision call is made). Clearing
//! `''` onto those links therefore makes the next attach of one of these images
//! run a real vision call — spend, once, per affected image. That is faithful:
//! v4's migration clears exactly the same rows and its own reader has the same
//! shape. It is recorded here so the first dogfood pass after this lands is not
//! surprised by an unexplained burst of describe calls.
//!
//! ## ⚠ A v4-side gap this pass deliberately does NOT close
//!
//! A THIRD writer stamps a caption that v4's own fix misses:
//! `app/api/v1/wardrobe/preview-avatar/route.ts:160,:185` writes
//! `` `${character.name} — outfit preview` `` with `source='GENERATED'`, and
//! `78b381a96` does not touch it (`git show --stat 78b381a96 -- <that file>` is
//! EMPTY). v5 has the identical writer at [`crate::api::wardrobe`] (`:1112`).
//! **The ruling for this port: reproduce v4 EXACTLY — keep the third writer,
//! keep the predicate narrow — and file it upstream.** The reader reorder masks
//! it for `describe_image` (those rows carry a `generationPrompt`); the residual
//! harm is a stale `On file: ` tail and the already-described gate. The heal
//! family carries an `— outfit preview` row that must SURVIVE on both sides:
//! that arm is the convergence tripwire, and it goes red by design the day v4
//! widens its predicate.
//!
//! ## The once-only mechanism — the P4.D140 ledger shape
//!
//! Data-only, no schema delta to key off, so the pass is guarded by v4's own
//! `migrations_state` ledger: a row from EITHER app is honoured (a v4 boot on
//! the shared instance will already have run its migration), and the row is
//! written only on a pass that actually cleared something.
//!
//! **No divergence here, and that is a measurement rather than an assumption:**
//! unlike the sha256 realign (whose v4 `shouldRun()` tests for PRESENCE, so v4
//! stamps a zero-affected pass and
//! [`super::files_sha256_realign_heal`] records that divergence), THIS
//! migration's `shouldRun()` counts placeholders on both sides and is false when
//! there are none. A pass that clears nothing means v4 never ran and recorded
//! nothing either — so "clear nothing, write nothing" agrees with v4 exactly.

use rusqlite::Connection;

use super::DbError;

const MIGRATION_ID: &str = "clear-generated-image-placeholder-descriptions-v1";

/// v4's `PLACEHOLDER_PREDICATE`, verbatim — the two labels as the writers
/// produced them, kept as ONE predicate so the count and the update agree. The
/// dash is U+2014 EM DASH with a space on each side, exactly as
/// `` `${character.name} — wardrobe portrait` `` rendered it.
const PLACEHOLDER_PREDICATE: &str = "(\n  \"description\" LIKE 'Story background for: %'\n  OR \"description\" LIKE '% — wardrobe portrait'\n)";

fn files_where() -> String {
    format!("\"source\" = 'GENERATED' AND {PLACEHOLDER_PREDICATE}")
}

fn links_where() -> String {
    format!("\"originalMimeType\" LIKE 'image/%' AND {PLACEHOLDER_PREDICATE}")
}

/// What one pass did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaceholderHealOutcome {
    /// The ledger already carries the row (either app completed the pass).
    AlreadyCompleted,
    /// v4's `shouldRun() === false`: no usable `files` table, or nothing on
    /// either side matches. Nothing written, nothing stamped, re-checked next
    /// boot.
    NotApplicable,
    /// The pass ran and cleared at least one row.
    Ran {
        files_cleared: usize,
        links_cleared: usize,
        /// v4's `linksSkipped`: the mount index was absent, its links table
        /// unusable, or reading it failed. A link that keeps its label is a
        /// stale caption on a search hit, not a wrong answer from
        /// `describe_image` (which reads the FileEntry), so this degrades the
        /// sweep rather than failing it.
        links_skipped: bool,
    },
}

impl PlaceholderHealOutcome {
    /// v4's `run()` summary sentence, byte-for-byte — including both singular
    /// forms and the parenthetical.
    pub fn message(&self) -> Option<String> {
        let PlaceholderHealOutcome::Ran {
            files_cleared,
            links_cleared,
            links_skipped,
        } = self
        else {
            return None;
        };
        Some(format!(
            "Cleared placeholder descriptions from {files_cleared} generated image{} and {links_cleared} Scriptorium link{}{}",
            if *files_cleared == 1 { "" } else { "s" },
            if *links_cleared == 1 { "" } else { "s" },
            if *links_skipped {
                " (mount index not inspected)"
            } else {
                ""
            }
        ))
    }
}

/// Run the placeholder clear once per instance, guarded by v4's own migration
/// ledger. `now_iso` stamps the rewritten `files.updatedAt` and the ledger's
/// `completedAt`/`lastChecked` (the caller passes [`crate::clock::now_iso`]).
///
/// `mount` is the mount-index partition; `None` is v4's
/// `openMountIndexDbIfPresent()` returning nothing.
pub fn clear_generated_image_placeholder_descriptions(
    main: &Connection,
    mount: Option<&Connection>,
    now_iso: &str,
) -> Result<PlaceholderHealOutcome, DbError> {
    // The completed check comes FIRST, exactly as v4's runner orders it
    // (`isMigrationCompleted` before `shouldRun`).
    if table_exists(main, "migrations_state")? {
        let mut stmt = main.prepare("SELECT 1 FROM \"migrations_state\" WHERE \"id\" = ?1")?;
        if stmt.exists([MIGRATION_ID])? {
            return Ok(PlaceholderHealOutcome::AlreadyCompleted);
        }
    }

    // v4 `shouldRun`: the `files` table, carrying both `description` and
    // `source`.
    if !table_exists(main, "files")? {
        return Ok(PlaceholderHealOutcome::NotApplicable);
    }
    let file_cols = table_columns(main, "files")?;
    if !file_cols.iter().any(|c| c == "description") || !file_cols.iter().any(|c| c == "source") {
        return Ok(PlaceholderHealOutcome::NotApplicable);
    }

    let context = format!("migration.{MIGRATION_ID}");

    // ── v4 `shouldRun()` ────────────────────────────────────────────────────
    // Short-circuits on the files side: a positive count returns true WITHOUT
    // touching the mount partition, so a mount that cannot be read is invisible
    // whenever the main side already has work. Only when the files side is
    // clean does v4 inspect the links (a re-imported file, or a link written
    // before the FileEntry, can carry a label the files side no longer does),
    // and an error THERE is warned about and read as "nothing to do".
    let files_to_clear = count_where(main, "files", &files_where())?;
    if files_to_clear == 0 {
        // v4 puts the usability probe AND the count inside ONE `try`
        // (`clear-generated-image-placeholder-descriptions.ts:96-108`), so a
        // throw from either reads as "nothing to do" — never a failed boot.
        let links_present = match (mount, links_table_usable(mount)) {
            (Some(mount), Ok(true)) => {
                match count_where(mount, "doc_mount_file_links", &links_where()) {
                    Ok(n) => n > 0,
                    Err(e) => {
                        tracing::warn!(
                            context = %context,
                            error = %e,
                            "Could not inspect mount-index links for placeholder descriptions"
                        );
                        false
                    }
                }
            }
            (Some(_), Err(e)) => {
                tracing::warn!(
                    context = %context,
                    error = %e,
                    "Could not inspect mount-index links for placeholder descriptions"
                );
                false
            }
            _ => false,
        };
        if !links_present {
            return Ok(PlaceholderHealOutcome::NotApplicable);
        }
    }

    // ── v4 `run()` ──────────────────────────────────────────────────────────
    tracing::debug!(
        context = %context,
        files = files_to_clear,
        "Scanning generated images for placeholder descriptions"
    );

    let mut files_cleared = 0usize;
    if files_to_clear > 0 {
        files_cleared = main.execute(
            &format!(
                "UPDATE \"files\" SET \"description\" = NULL, \"updatedAt\" = ?1 WHERE {}",
                files_where()
            ),
            rusqlite::params![now_iso],
        )?;
    }

    // A link that keeps its label is a stale caption on a search hit, not a
    // wrong answer from `describe_image` (which reads the FileEntry), so an
    // unreadable mount index DEGRADES the sweep rather than failing it — v4's
    // try/catch, ported as a swallowed `Err` rather than a propagated one. The
    // plain absent/unusable arm is v4's `else`, which sets the flag and stays
    // SILENT; only a genuine failure warns.
    let mut links_cleared = 0usize;
    let mut links_skipped = false;
    // The probe sits INSIDE v4's `try` (`:150-172`) with the count and the
    // UPDATE, so its failure is the same swallowed, warned, `linksSkipped` arm.
    let usable = match links_table_usable(mount) {
        Ok(u) => u,
        Err(e) => {
            links_skipped = true;
            tracing::warn!(
                context = %context,
                error = %e,
                "Could not clear placeholder descriptions on mount-index links"
            );
            false
        }
    };
    match (mount, usable) {
        (Some(mount), true) => {
            match count_where(mount, "doc_mount_file_links", &links_where()).and_then(|n| {
                if n == 0 {
                    return Ok(0);
                }
                Ok(mount.execute(
                    &format!(
                        "UPDATE \"doc_mount_file_links\" SET \"description\" = '' WHERE {}",
                        links_where()
                    ),
                    [],
                )?)
            }) {
                Ok(n) => links_cleared = n,
                Err(e) => {
                    links_skipped = true;
                    tracing::warn!(
                        context = %context,
                        error = %e,
                        "Could not clear placeholder descriptions on mount-index links"
                    );
                }
            }
        }
        _ => links_skipped = true,
    }

    tracing::info!(
        context = %context,
        files_cleared,
        links_cleared,
        links_skipped,
        "Cleared placeholder descriptions from generated images"
    );

    let outcome = PlaceholderHealOutcome::Ran {
        files_cleared,
        links_cleared,
        links_skipped,
    };

    // The ledger write — v4's `migrations/state.ts` shapes verbatim (the
    // P4.D140 / P4.D152 heals' shapes, hand-duplicated because there is no
    // shared helper).
    // P4.88: ONE home for the two ledger tables, and unguarded — the four heals
    // each carried this DDL under an `if !table_exists(main, "migrations_state")`
    // guard (v4's own shape), which cannot heal a partition carrying
    // `migrations_state` WITHOUT `migrations_metadata`: the upsert below then
    // fails the whole boot. See `super::migrations_ledger`.
    super::migrations_ledger::ensure_migrations_tables(main)?;
    main.execute(
        "INSERT INTO \"migrations_state\" (id, completedAt, quilltapVersion, itemsAffected, message)\n         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            MIGRATION_ID,
            now_iso,
            env!("CARGO_PKG_VERSION"),
            (files_cleared + links_cleared) as i64,
            outcome.message().expect("Ran renders a message")
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

    Ok(outcome)
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool, DbError> {
    let mut stmt =
        conn.prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1")?;
    Ok(stmt.exists([name])?)
}

fn table_columns(conn: &Connection, name: &str) -> Result<Vec<String>, DbError> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{name}\")"))?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// v4's `linksTableUsable`: the table exists AND carries both columns the
/// predicate names.
fn links_table_usable(mount: Option<&Connection>) -> Result<bool, DbError> {
    let Some(mount) = mount else {
        return Ok(false);
    };
    if !table_exists(mount, "doc_mount_file_links")? {
        return Ok(false);
    }
    let cols = table_columns(mount, "doc_mount_file_links")?;
    Ok(cols.iter().any(|c| c == "description") && cols.iter().any(|c| c == "originalMimeType"))
}

fn count_where(conn: &Connection, table: &str, whr: &str) -> Result<usize, DbError> {
    let sql = format!("SELECT COUNT(*) AS n FROM \"{table}\" WHERE {whr}");
    Ok(conn.query_row(&sql, [], |r| r.get::<_, i64>(0))? as usize)
}

#[cfg(test)]
mod unify_review_tests {
    use super::*;

    /// The §3 unification review of the `78b381a96` round: `links_table_usable`
    /// used to propagate with `?` on both legs while v4 keeps the probe inside
    /// the same `try` as the count (`clear-generated-image-placeholder-
    /// descriptions.ts:96-108`, `:150-172`) — so an unreadable mount index
    /// DEGRADED the sweep in v4 and FAILED THE BOOT in v5. Forced here with a
    /// mount whose schema read is refused (an exclusive lock held by a sibling
    /// connection with a zero busy timeout), the only way to make the probe
    /// itself error rather than the count it guards.
    #[test]
    fn an_unreadable_mount_index_degrades_the_sweep_instead_of_failing_it() {
        let dir = tempfile::tempdir().unwrap();
        let mount_path = dir.path().join("mount.db");
        let holder = rusqlite::Connection::open(&mount_path).unwrap();
        holder
            .execute_batch(
                "CREATE TABLE doc_mount_file_links (id TEXT PRIMARY KEY, description TEXT, originalMimeType TEXT);",
            )
            .unwrap();
        let mount = rusqlite::Connection::open(&mount_path).unwrap();
        mount
            .busy_timeout(std::time::Duration::from_millis(0))
            .unwrap();
        let main = rusqlite::Connection::open_in_memory().unwrap();
        main.execute_batch(
            "CREATE TABLE files (id TEXT PRIMARY KEY, source TEXT, description TEXT, updatedAt TEXT);
             INSERT INTO files VALUES ('f1', 'GENERATED', 'Story background for: X', '2026-01-01T00:00:00.000Z');",
        )
        .unwrap();
        // Hold the file exclusively so the second connection's schema read fails.
        holder.execute_batch("BEGIN EXCLUSIVE").unwrap();
        assert!(
            links_table_usable(Some(&mount)).is_err(),
            "the probe itself must error under the lock for this test to mean anything"
        );
        let outcome = clear_generated_image_placeholder_descriptions(
            &main,
            Some(&mount),
            "2026-09-10T00:00:00.000Z",
        )
        .expect("an unreadable mount index must not fail the pass");
        match outcome {
            PlaceholderHealOutcome::Ran {
                files_cleared,
                links_cleared,
                links_skipped,
            } => {
                assert_eq!(files_cleared, 1);
                assert_eq!(links_cleared, 0);
                assert!(links_skipped, "the link leg is skipped, as v4's catch does");
            }
            other => panic!("expected Ran, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // P4.88 — the shared ledger tables' boot shape (`super::migrations_ledger`).
    //
    // Every heal used to gate its `CREATE TABLE IF NOT EXISTS` batch on
    // `migrations_state` alone and then upsert into `migrations_metadata`, so a
    // partition carrying the FIRST table without the SECOND failed the whole
    // BOOT. v4's `ensureSQLiteMigrationsTable` has the identical guard, so the
    // shape is v4-faithful and unreachable FROM v4 — but reachable from a
    // hand-built partition, a partial restore, or a fixture, which is exactly
    // how P4.D175's boot-wiring test first hit it. Each arm below plants one
    // table, or both (the Friday shape — §R.12: the hardening must stay a
    // no-op there).
    // -----------------------------------------------------------------------

    /// The three partial shapes, planted before the heal runs.
    fn plant_ledger_shape(main: &Connection, shape: &str) {
        if shape.contains("state") {
            main.execute_batch(
                "CREATE TABLE \"migrations_state\" (\"id\" TEXT PRIMARY KEY, \
                 \"completedAt\" TEXT NOT NULL, \"quilltapVersion\" TEXT NOT NULL, \
                 \"itemsAffected\" INTEGER NOT NULL DEFAULT 0, \"message\" TEXT);",
            )
            .expect("plant migrations_state");
        }
        if shape.contains("metadata") {
            main.execute_batch(
                "CREATE TABLE \"migrations_metadata\" (\"key\" TEXT PRIMARY KEY, \
                 \"value\" TEXT NOT NULL);",
            )
            .expect("plant migrations_metadata");
        }
    }

    /// The ledger row and both metadata keys landed.
    fn assert_ledger_stamped(main: &Connection) {
        let rows: i64 = main
            .query_row("SELECT COUNT(*) FROM \"migrations_state\"", [], |r| {
                r.get(0)
            })
            .expect("the ledger row landed");
        assert_eq!(rows, 1);
        let keys: i64 = main
            .query_row(
                "SELECT COUNT(*) FROM \"migrations_metadata\" \
                 WHERE key IN ('lastChecked', 'quilltapVersion')",
                [],
                |r| r.get(0),
            )
            .expect("the metadata upserts landed");
        assert_eq!(keys, 2, "v4 writes both keys");
    }

    #[test]
    fn the_ledger_stamp_survives_every_partial_table_shape() {
        for shape in ["state", "metadata", "state+metadata"] {
            let main = rusqlite::Connection::open_in_memory().unwrap();
            main.execute_batch(
                "CREATE TABLE files (id TEXT PRIMARY KEY, source TEXT, description TEXT, updatedAt TEXT);
                 INSERT INTO files VALUES ('f1', 'GENERATED', 'Story background for: X', '2026-01-01T00:00:00.000Z');",
            )
            .unwrap();
            plant_ledger_shape(&main, shape);
            let out = clear_generated_image_placeholder_descriptions(
                &main,
                None,
                "2026-09-10T00:00:00.000Z",
            )
            .unwrap_or_else(|e| panic!("shape {shape}: {e}"));
            assert!(
                matches!(
                    out,
                    PlaceholderHealOutcome::Ran {
                        files_cleared: 1,
                        ..
                    }
                ),
                "shape {shape}: {out:?}"
            );
            assert_ledger_stamped(&main);
        }
    }
}
