//! P4.D184 tier-2 differential: the collapse-duplicate-avatar-rolls boot heal
//! (v4 migration `collapse-duplicate-avatar-rolls-v1`, `7fbf8a55b`) vs
//! `quilltap_core::db::avatar_rolls_collapse_heal::collapse_duplicate_avatar_rolls`.
//!
//! Both sides walk the SAME scenarios (`harness/oracle/fixtures/avatar-rolls-
//! collapse-heal.json` — v4's own thirteen test cases rebuilt as data, plus four
//! its suite does not ask, plus bug 145's album/census shapes at P4.D192) and
//! the WHOLE post-pass state of every table the pass can touch is diffed: `files`, `chats.characterAvatars`,
//! `characters.avatarOverrides`, `chat_messages.{attachments,content,
//! opaqueContent}` and the five mount tables. So a survivor kept, a victim
//! deleted, a reference repointed, a uuid swapped inline, and a blob's chunks
//! going with it are all comparands rather than claims.
//!
//! The `migrations_state` row is compared by PRESENCE and by what it claims
//! (`itemsAffected`, `message`); its `completedAt` and `quilltapVersion` are
//! nondeterministic and each app writes its own, so they are excluded.
//!
//! v4's logger lines are comparands too — the protected-roll arm and the summary
//! counts are otherwise invisible on the state.
//!
//! ⚠ The migration this drives arrived at v4 `7fbf8a55b` and was rewritten at
//! **`23abc1ba1`** (bug 145) — both PAST the `ffb6b3119` oracle baseline, so
//! until the baseline moves, regenerate through the sweep driver with a pin at
//! or after `23abc1ba1` (`recipe_sweep.py --v4 <pinned worktree> --run
//! avatar_rolls_collapse_heal_equivalence`), which rewrites the `cd` below.
//! Against a checkout still at the baseline the mount DDL now carries a
//! `relativePath` column v4's pre-fix migration never selects, so the album
//! scenarios would record the OLD, data-losing behaviour as v4's contract —
//! verify the pin by grepping the fresh NDJSON for `did not choose to double
//! up` (MUST be > 0; the pre-fix tree cannot emit it).
//!
//! [P4.D192] The bug-145 comparands: `doc_mount_file_links.relativePath` is in
//! the dump (an album link and a roll link are only distinguishable by path),
//! the summary bag carries `protectedKept`/`albumCopiesKept`, the ledger
//! `message` carries v4's kept clause, and `warns` carries the in-pass
//! duplicate-key census (whose `fileIds` is a JSON ARRAY — see `shape_line`).
//!
//! Generate the oracle output (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=<this worktree root>
//!   TMPO=/tmp/qt-avatar-rolls-collapse-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/avatar-rolls-collapse-heal.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/avatar-rolls-collapse-heal.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-avatar-rolls-collapse.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- "avatar-rolls-collapse-heal\.test\.ts$"
//! Run:
//!   QT_ORACLE_AVATAR_COLLAPSE=/tmp/oracle-avatar-rolls-collapse.ndjson \
//!     cargo test -p quilltap-harness --test avatar_rolls_collapse_heal_equivalence

use std::collections::HashMap;

use quilltap_core::db::avatar_rolls_collapse_heal::{
    collapse_duplicate_avatar_rolls, CollapseOutcome,
};
use quilltap_core::services::avatar_cache::derive_legacy_avatar_cache_key;
use quilltap_core::test_support::captured_with;
use rusqlite::Connection;
use serde_json::{json, Value};

const NOW_ISO: &str = "2026-09-14T12:00:00.000Z";

/// v4's `VAULT_MOUNT` — the character's own vault, where the album lives.
const VAULT_MOUNT: &str = "vault-1";

fn spec_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/avatar-rolls-collapse-heal.json")
}

/// The MAIN partition at the migration's own vintage — the same four tables
/// v4's test builds, and all the pass reads or writes.
fn build_main() -> Connection {
    let c = Connection::open_in_memory().expect("main");
    c.execute_batch(
        r#"
        CREATE TABLE "files" (
          "id" TEXT PRIMARY KEY,
          "originalFilename" TEXT NOT NULL,
          "category" TEXT NOT NULL,
          "generationPrompt" TEXT,
          "generationModel" TEXT,
          "generationKey" TEXT,
          "storageKey" TEXT,
          "createdAt" TEXT NOT NULL
        );
        CREATE TABLE "chats" ("id" TEXT PRIMARY KEY, "characterAvatars" TEXT);
        CREATE TABLE "characters" (
          "id" TEXT PRIMARY KEY,
          "avatarOverrides" TEXT,
          "defaultImageId" TEXT
        );
        CREATE TABLE "chat_messages" (
          "id" TEXT PRIMARY KEY,
          "attachments" TEXT,
          "content" TEXT,
          "opaqueContent" TEXT
        );
        "#,
    )
    .expect("main ddl");
    c
}

fn build_mount() -> Connection {
    let c = Connection::open_in_memory().expect("mount");
    c.execute_batch(
        r#"
        CREATE TABLE "doc_mount_files" ("id" TEXT PRIMARY KEY);
        CREATE TABLE "doc_mount_blobs" ("id" TEXT PRIMARY KEY, "fileId" TEXT NOT NULL);
        CREATE TABLE "doc_mount_documents" ("id" TEXT PRIMARY KEY, "fileId" TEXT NOT NULL);
        CREATE TABLE "doc_mount_file_links" (
          "id" TEXT PRIMARY KEY,
          "fileId" TEXT NOT NULL,
          "mountPointId" TEXT NOT NULL,
          "relativePath" TEXT NOT NULL DEFAULT ''
        );
        CREATE TABLE "doc_mount_chunks" ("id" TEXT PRIMARY KEY, "linkId" TEXT NOT NULL);
        "#,
    )
    .expect("mount ddl");
    c
}

/// v4's `seedRoll`, transcribed: a `files` row plus its vault file/blob/document/
/// link/chunk. `noBlob` seeds the `files` row alone.
fn seed_roll(main: &Connection, mount: &Connection, mount_point_id: &str, roll: &Value) {
    let id = roll["id"].as_str().expect("roll id");
    let blob_id = format!("blob-{id}");
    let no_blob = roll.get("noBlob").and_then(Value::as_bool).unwrap_or(false);
    let filename = roll
        .get("filename")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("avatar_Friday_{id}.webp"));
    let category = roll
        .get("category")
        .and_then(Value::as_str)
        .unwrap_or("IMAGE");
    // The oracle's `roll.model === undefined ? 'flux-dev' : roll.model`: an
    // ABSENT key defaults, an explicit null stays null.
    let model: Option<&str> = match roll.get("model") {
        None => Some("flux-dev"),
        Some(Value::Null) => None,
        Some(v) => v.as_str(),
    };
    let storage_key = if no_blob {
        None
    } else {
        Some(format!("mount-blob:{mount_point_id}:{blob_id}"))
    };
    main.execute(
        r#"INSERT INTO "files" (id, originalFilename, category, generationPrompt, generationModel, generationKey, storageKey, createdAt)
           VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7)"#,
        rusqlite::params![
            id,
            filename,
            category,
            roll["prompt"].as_str(),
            model,
            storage_key,
            roll["createdAt"].as_str().expect("createdAt")
        ],
    )
    .expect("insert file");
    if no_blob {
        return;
    }
    let content_id = format!("content-{id}");
    mount
        .execute(
            r#"INSERT INTO "doc_mount_files" (id) VALUES (?1)"#,
            [&content_id],
        )
        .expect("mount file");
    mount
        .execute(
            r#"INSERT INTO "doc_mount_blobs" (id, fileId) VALUES (?1, ?2)"#,
            rusqlite::params![blob_id, content_id],
        )
        .expect("mount blob");
    mount
        .execute(
            r#"INSERT INTO "doc_mount_documents" (id, fileId) VALUES (?1, ?2)"#,
            rusqlite::params![format!("doc-{id}"), content_id],
        )
        .expect("mount doc");
    let relative_path = roll
        .get("relativePath")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("character-avatars/avatar_Friday_{id}.webp"));
    mount
        .execute(
            r#"INSERT INTO "doc_mount_file_links" (id, fileId, mountPointId, relativePath)
               VALUES (?1, ?2, ?3, ?4)"#,
            rusqlite::params![
                format!("link-{id}"),
                content_id,
                mount_point_id,
                relative_path
            ],
        )
        .expect("mount link");
    mount
        .execute(
            r#"INSERT INTO "doc_mount_chunks" (id, linkId) VALUES (?1, ?2)"#,
            rusqlite::params![format!("chunk-{id}"), format!("link-{id}")],
        )
        .expect("mount chunk");
}

/// v4's `keepInAlbum` (`23abc1ba1`): a SECOND link, over the SAME content row,
/// in a `photos/` folder — what "the operator kept this plate" looks like in
/// the mount index. Defaults to the character's own vault, a DIFFERENT mount
/// from the roll's.
fn keep_in_album(mount: &Connection, link: &Value) {
    let roll_id = link["rollId"].as_str().expect("album rollId");
    let mount_point_id = link
        .get("mountPointId")
        .and_then(Value::as_str)
        .unwrap_or(VAULT_MOUNT);
    mount
        .execute(
            r#"INSERT INTO "doc_mount_file_links" (id, fileId, mountPointId, relativePath)
               VALUES (?1, ?2, ?3, ?4)"#,
            rusqlite::params![
                format!("album-{roll_id}"),
                format!("content-{roll_id}"),
                mount_point_id,
                format!("photos/kept-{roll_id}.webp")
            ],
        )
        .expect("album link");
}

/// A `files` row the pass never groups — v4's `stowaway`. A `generationKey` of
/// `survivor-of:<rollId>` is DERIVED here through the same helper the pass
/// groups on, so neither side ever carries a hand-copied hash.
fn seed_extra_file(main: &Connection, scenario: &Value, extra: &Value) {
    let key: Option<String> = match extra.get("generationKey").and_then(Value::as_str) {
        None => None,
        Some(k) => match k.strip_prefix("survivor-of:") {
            None => Some(k.to_string()),
            Some(roll_id) => {
                let roll = scenario["rolls"]
                    .as_array()
                    .expect("rolls")
                    .iter()
                    .find(|r| r["id"].as_str() == Some(roll_id))
                    .unwrap_or_else(|| panic!("survivor-of names no roll: {roll_id}"));
                let model: Option<&str> = match roll.get("model") {
                    None => Some("flux-dev"),
                    Some(Value::Null) => None,
                    Some(v) => v.as_str(),
                };
                Some(derive_legacy_avatar_cache_key(
                    model,
                    roll["prompt"].as_str().unwrap_or(""),
                ))
            }
        },
    };
    let model: Option<&str> = match extra.get("model") {
        None => Some("flux-dev"),
        Some(Value::Null) => None,
        Some(v) => v.as_str(),
    };
    main.execute(
        r#"INSERT INTO "files" (id, originalFilename, category, generationPrompt, generationModel, generationKey, storageKey, createdAt)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"#,
        rusqlite::params![
            extra["id"].as_str().expect("extra id"),
            extra["originalFilename"].as_str().expect("extra filename"),
            extra.get("category").and_then(Value::as_str).unwrap_or("IMAGE"),
            extra["prompt"].as_str(),
            model,
            key,
            extra.get("storageKey").and_then(Value::as_str),
            extra["createdAt"].as_str().expect("extra createdAt")
        ],
    )
    .expect("insert extra file");
}

fn plant_ledger_row(main: &Connection) {
    main.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS "migrations_state" (
          "id" TEXT PRIMARY KEY,
          "completedAt" TEXT NOT NULL,
          "quilltapVersion" TEXT NOT NULL,
          "itemsAffected" INTEGER NOT NULL DEFAULT 0,
          "message" TEXT
        );
        CREATE TABLE IF NOT EXISTS "migrations_metadata" (
          "key" TEXT PRIMARY KEY,
          "value" TEXT NOT NULL
        );
        "#,
    )
    .expect("ledger ddl");
    main.execute(
        "INSERT INTO migrations_state (id, completedAt, quilltapVersion, itemsAffected, message) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            "collapse-duplicate-avatar-rolls-v1",
            NOW_ISO,
            "4.10.0",
            0,
            "planted by the other app"
        ],
    )
    .expect("plant ledger");
}

/// Dump one table as `[{col: value}]`, ordered by the oracle's own `ORDER BY`.
fn dump(conn: &Connection, sql: &str, cols: &[&str]) -> Value {
    let mut stmt = conn.prepare(sql).expect("dump prepare");
    let rows = stmt
        .query_map([], |r| {
            let mut obj = serde_json::Map::new();
            for (i, col) in cols.iter().enumerate() {
                let v = match r.get_ref(i)? {
                    rusqlite::types::ValueRef::Null => Value::Null,
                    rusqlite::types::ValueRef::Integer(n) => Value::from(n),
                    rusqlite::types::ValueRef::Real(f) => Value::from(f),
                    rusqlite::types::ValueRef::Text(t) => {
                        Value::String(String::from_utf8_lossy(t).into_owned())
                    }
                    rusqlite::types::ValueRef::Blob(b) => Value::String(hex(b)),
                };
                obj.insert((*col).to_string(), v);
            }
            Ok(Value::Object(obj))
        })
        .expect("dump query")
        .collect::<Result<Vec<_>, _>>()
        .expect("dump rows");
    Value::Array(rows)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn dump_all(main: &Connection, mount: &Connection) -> Value {
    json!({
        "files": dump(
            main,
            "SELECT id, generationKey, storageKey, createdAt FROM files ORDER BY id",
            &["id", "generationKey", "storageKey", "createdAt"],
        ),
        "chats": dump(
            main,
            "SELECT id, characterAvatars FROM chats ORDER BY id",
            &["id", "characterAvatars"],
        ),
        "characters": dump(
            main,
            "SELECT id, avatarOverrides, defaultImageId FROM characters ORDER BY id",
            &["id", "avatarOverrides", "defaultImageId"],
        ),
        "chat_messages": dump(
            main,
            "SELECT id, attachments, content, opaqueContent FROM chat_messages ORDER BY id",
            &["id", "attachments", "content", "opaqueContent"],
        ),
        "doc_mount_files": dump(
            mount, "SELECT id FROM doc_mount_files ORDER BY id", &["id"],
        ),
        "doc_mount_blobs": dump(
            mount,
            "SELECT id, fileId FROM doc_mount_blobs ORDER BY id",
            &["id", "fileId"],
        ),
        "doc_mount_documents": dump(
            mount,
            "SELECT id, fileId FROM doc_mount_documents ORDER BY id",
            &["id", "fileId"],
        ),
        "doc_mount_file_links": dump(
            mount,
            "SELECT id, fileId, mountPointId, relativePath FROM doc_mount_file_links ORDER BY id",
            &["id", "fileId", "mountPointId", "relativePath"],
        ),
        "doc_mount_chunks": dump(
            mount,
            "SELECT id, linkId FROM doc_mount_chunks ORDER BY id",
            &["id", "linkId"],
        ),
    })
}

/// The ledger row, minus the two nondeterministic columns.
fn ledger_row(main: &Connection) -> Value {
    let exists: bool = main
        .prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name='migrations_state'")
        .and_then(|mut s| s.exists([]))
        .unwrap_or(false);
    if !exists {
        return Value::Null;
    }
    main.query_row(
        "SELECT id, itemsAffected, message FROM migrations_state WHERE id = ?1",
        ["collapse-duplicate-avatar-rolls-v1"],
        |r| {
            Ok(json!({
                "id": r.get::<_, String>(0)?,
                "itemsAffected": r.get::<_, i64>(1)?,
                "message": r.get::<_, Option<String>>(2)?,
            }))
        },
    )
    .unwrap_or(Value::Null)
}

/// v4 logs through `logger.info(message, context)`; v5 through `tracing` with the
/// same field names. Reduce a captured line to the oracle's `shapeLogs` shape so
/// the two are comparable.
///
/// The capture layer renders `<LEVEL> <target> <message> field=value …`
/// (`test_support::FieldVisitor`). Two things about that shape are easy to get
/// wrong and were, first time: the message is NOT quoted (its `{:?}` is
/// `fmt::Arguments`, which Debug-formats as the plain text), and it comes FIRST
/// — `tracing` hoists the message field ahead of the others no matter where the
/// macro's format string sat. So the message is everything between the target
/// and the first `key=value` token.
fn shape_line(line: &str) -> Option<Value> {
    let mut tokens = line.split(' ').filter(|t| !t.is_empty());
    let _level = tokens.next()?;
    let _target = tokens.next()?;
    let message = tokens
        .clone()
        .take_while(|t| !t.contains('='))
        .collect::<Vec<_>>()
        .join(" ");
    if message.is_empty() {
        return None;
    }
    let field = |name: &str| -> Value {
        let needle = format!("{name}=");
        line.split(' ')
            .find_map(|tok| tok.strip_prefix(needle.as_str()))
            .map(|v| {
                v.parse::<i64>()
                    .map(Value::from)
                    .unwrap_or_else(|_| Value::String(v.to_string()))
            })
            .unwrap_or(Value::Null)
    };
    // `fileIds` is v4's `unexplained.slice(0, 20)` — a JSON ARRAY, not a scalar.
    // v5 emits it as compact JSON through `record_str` under the FILE layer's
    // `…Json` convention (P4.91 — `log_file.rs` re-parses `fileIdsJson` into a
    // real array under `fileIds` in `combined.log`). This capture is the
    // thread-scoped rig, which sees the RAW field name, so the reader asks for
    // `fileIdsJson` and applies the same re-parse; the comparand is then the
    // array v4's `logger.warn` context carries. (The `1fefadb9a` round's
    // unification: the rename landed after this family's last consumer run.)
    let json_field = |name: &str| -> Value {
        match field(name) {
            Value::String(raw) => serde_json::from_str(&raw).unwrap_or(Value::String(raw)),
            other => other,
        }
    };
    Some(json!({
        "message": message,
        "context": field("context"),
        "fileId": field("fileId"),
        "blobId": field("blobId"),
        "avatarRows": field("avatarRows"),
        "configurations": field("configurations"),
        "victims": field("victims"),
        "rowsKeyed": field("rowsKeyed"),
        "victimsDeleted": field("victimsDeleted"),
        "blobsDeleted": field("blobsDeleted"),
        "chatsChanged": field("chatsChanged"),
        "charactersChanged": field("charactersChanged"),
        "messagesChanged": field("messagesChanged"),
        "protectedKept": field("protectedKept"),
        "albumCopiesKept": field("albumCopiesKept"),
        "unexplainedCount": field("unexplainedCount"),
        "fileIds": json_field("fileIdsJson"),
    }))
}

#[test]
fn avatar_rolls_collapse_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_AVATAR_COLLAPSE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_AVATAR_COLLAPSE to the oracle NDJSON (see header).");
            return;
        }
    };
    let body = std::fs::read_to_string(&oracle_path).expect("read oracle");
    let spec: Value =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).expect("read spec"))
            .expect("parse spec");
    let mount_point_id = spec["mountPointId"].as_str().expect("mountPointId");
    let scenarios: HashMap<String, Value> = spec["scenarios"]
        .as_array()
        .expect("scenarios")
        .iter()
        .map(|s| (s["name"].as_str().expect("name").to_string(), s.clone()))
        .collect();

    let mut seen = 0usize;
    for line in body.lines().filter(|l| !l.trim().is_empty()) {
        let want: Value = serde_json::from_str(line).expect("parse oracle row");
        let name = want["name"].as_str().expect("name").to_string();
        let scenario = scenarios
            .get(&name)
            .unwrap_or_else(|| panic!("oracle row {name} has no corpus scenario"));

        let main = build_main();
        let mount = build_mount();
        // rowid order is a comparand: `drop_victim_roll_link`'s links SELECT has
        // no ORDER BY (v4's has none either), so whichever link was inserted
        // first is the one v4's `Array.prototype.find` reaches first.
        let album_links_first = scenario
            .get("albumLinksFirst")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let album_links = scenario
            .get("albumLinks")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if album_links_first {
            for link in &album_links {
                keep_in_album(&mount, link);
            }
        }
        for roll in scenario["rolls"].as_array().into_iter().flatten() {
            seed_roll(&main, &mount, mount_point_id, roll);
        }
        if !album_links_first {
            for link in &album_links {
                keep_in_album(&mount, link);
            }
        }
        for link_id in scenario
            .get("deleteLinks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            mount
                .execute(
                    r#"DELETE FROM "doc_mount_file_links" WHERE id = ?1"#,
                    [link_id.as_str().expect("deleteLinks id")],
                )
                .expect("delete link");
        }
        for extra in scenario
            .get("extraFiles")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            seed_extra_file(&main, scenario, extra);
        }
        for c in scenario
            .get("chats")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            main.execute(
                "INSERT INTO chats (id, characterAvatars) VALUES (?1, ?2)",
                rusqlite::params![
                    c["id"].as_str(),
                    match &c["characterAvatars"] {
                        Value::Null => None,
                        v => Some(serde_json::to_string(v).expect("json")),
                    }
                ],
            )
            .expect("insert chat");
        }
        for c in scenario
            .get("characters")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            main.execute(
                "INSERT INTO characters (id, avatarOverrides, defaultImageId) VALUES (?1, ?2, ?3)",
                rusqlite::params![
                    c["id"].as_str(),
                    match &c["avatarOverrides"] {
                        Value::Null => None,
                        v => Some(serde_json::to_string(v).expect("json")),
                    },
                    c.get("defaultImageId").and_then(Value::as_str)
                ],
            )
            .expect("insert character");
        }
        for m in scenario
            .get("messages")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            main.execute(
                "INSERT INTO chat_messages (id, attachments, content, opaqueContent) \
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![
                    m["id"].as_str(),
                    match &m["attachments"] {
                        Value::Null => None,
                        v => Some(serde_json::to_string(v).expect("json")),
                    },
                    m.get("content").and_then(Value::as_str),
                    m.get("opaqueContent").and_then(Value::as_str)
                ],
            )
            .expect("insert message");
        }
        if scenario
            .get("preCompleted")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            plant_ledger_row(&main);
        }

        let (outcome, lines) = captured_with(|| {
            collapse_duplicate_avatar_rolls(&main, Some(&mount), NOW_ISO).expect("collapse")
        });

        // ---- the gate's two decisions -------------------------------------
        let want_completed_before = want["completedBefore"].as_bool().expect("completedBefore");
        let want_should_run = want["shouldRun"].as_bool().expect("shouldRun");
        match (&outcome, want_completed_before, want_should_run) {
            (CollapseOutcome::AlreadyCompleted, true, _) => {}
            (CollapseOutcome::NotApplicable, false, false) => {}
            (CollapseOutcome::Ran { .. }, false, true) => {}
            (got, cb, sr) => {
                panic!("{name}: gate diverged — v5 {got:?}, v4 completedBefore={cb} shouldRun={sr}")
            }
        }

        // ---- the whole post-pass state ------------------------------------
        let got_dumps = dump_all(&main, &mount);
        let want_dumps = &want["dumps"];
        for table in [
            "files",
            "chats",
            "characters",
            "chat_messages",
            "doc_mount_files",
            "doc_mount_blobs",
            "doc_mount_documents",
            "doc_mount_file_links",
            "doc_mount_chunks",
        ] {
            assert_eq!(
                got_dumps[table], want_dumps[table],
                "{name}: {table} diverged\n  rust:   {}\n  oracle: {}",
                got_dumps[table], want_dumps[table]
            );
        }

        // ---- what v4's runner recorded ------------------------------------
        assert_eq!(
            ledger_row(&main),
            want["ledgerRow"],
            "{name}: the migrations_state row diverged"
        );

        // ---- the log lines -------------------------------------------------
        let got_infos: Vec<Value> = lines
            .iter()
            .filter(|l| l.starts_with("INFO"))
            .filter_map(|l| shape_line(l))
            .collect();
        assert_eq!(
            Value::Array(got_infos),
            want["infos"],
            "{name}: the info lines diverged"
        );
        let got_warns: Vec<Value> = lines
            .iter()
            .filter(|l| l.starts_with("WARN"))
            .filter_map(|l| shape_line(l))
            .collect();
        assert_eq!(
            Value::Array(got_warns),
            want["warns"],
            "{name}: the warn lines diverged"
        );

        // ---- idempotence ---------------------------------------------------
        if scenario
            .get("runTwice")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            // v4 calls `run()` again directly (its runner would skip on the
            // ledger row); v5's entry point honours the row it just wrote, so
            // the second call must answer AlreadyCompleted and change nothing.
            let before = dump_all(&main, &mount);
            let second =
                collapse_duplicate_avatar_rolls(&main, Some(&mount), NOW_ISO).expect("second");
            assert_eq!(
                second,
                CollapseOutcome::AlreadyCompleted,
                "{name}: the second pass must honour the ledger row it wrote"
            );
            assert_eq!(
                dump_all(&main, &mount),
                before,
                "{name}: the second pass changed state"
            );
            assert_eq!(
                want["dumpsAfterSecond"]["files"], want["dumps"]["files"],
                "{name}: v4's own second pass moved the files table — the case no \
                 longer measures idempotence"
            );
        }

        seen += 1;
    }

    assert_eq!(
        seen,
        scenarios.len(),
        "every corpus scenario must appear in the oracle NDJSON"
    );
}
