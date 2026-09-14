//! P4.D182 — `chats.transcriptVersion` exists and NOTHING reads, projects,
//! remaps or exports it (v4 `5029075bb`).
//!
//! v4 keeps the column deliberately outside `ChatMetadataSchema` and
//! `ChatMetadataBaseSchema` — two identical comment blocks in
//! `lib/schemas/chat.types.ts` explain that a counter carried as a schema
//! field could be rewound by any concurrent whole-row rewrite, which is
//! exactly the "answered unchanged while a message is missing" failure the
//! counter exists to prevent. The consequences ripple outward, and every one
//! of them is a NEGATIVE: Zod strips the key from every write, `generateDDL`
//! never emits the column, and it is absent from `.qtap` exports and backups
//! **for free**. v4 has NO export-side filter for it, and this port must
//! invent none — its absence from every column list IS the mechanism.
//!
//! An absence is the hardest thing to keep. So these are the pins the round's
//! later lanes build on: P4.D183 adds the ONE writer and the ONE reader, and
//! nothing else may grow a third.
//!
//! This file lives in the harness rather than beside the code because
//! `db/chats_read.rs` and `db/chats.rs` are outside P4.D182's ownership (the
//! read census must not move, and `db/chats.rs` is P4.D183's). Nothing here
//! needs an oracle: v4's behaviour is the absence, and the comparand is v5's
//! own surface.

use std::collections::BTreeSet;
use std::path::PathBuf;

use rusqlite::Connection;
use serde_json::Value;

const COLUMN: &str = "transcriptVersion";

fn core_src(rel: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../quilltap-core/src")
        .join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// The `chats` DDL from the D23 dump — the single source of truth, and the
/// thing that must NOT name the column.
fn chats_ddl_from_the_dump() -> String {
    let raw = core_src("services/provisioning/fresh_schema.json");
    let schema: Value = serde_json::from_str(&raw).expect("fresh_schema.json parses");
    schema["main"]
        .as_array()
        .expect("a main partition")
        .iter()
        .filter_map(Value::as_str)
        .find(|s| s.starts_with("CREATE TABLE \"chats\" ("))
        .expect("the dump carries the chats DDL")
        .to_string()
}

/// §R.5(a), executable: `generateDDL` cannot emit this column, so the re-dump
/// must not carry it — not in `chats`, not anywhere in any partition. If a
/// later re-dump ever DOES carry it, v4 has moved the field into the schema
/// and this whole file's premise needs re-measuring; that is what this arm is
/// for.
#[test]
fn the_d23_dump_carries_the_column_nowhere() {
    let raw = core_src("services/provisioning/fresh_schema.json");
    assert!(
        !raw.contains(COLUMN),
        "the D23 dump names `{COLUMN}` — v4 has moved it into ChatMetadataSchema, \
         which would make the boot ensure, the ChatUpdate guard and the export \
         negatives below all wrong. Re-measure before touching any of them."
    );
}

/// A real `chats` table, built from the dump and then healed the way a boot
/// heals a live instance, carrying a BUMPED counter — and `chats_read` still
/// hands back a row with no such key.
///
/// The plant matters: a test over a row whose counter is `0` (or whose column
/// is missing) would pass with the column freely marshalled, because nothing
/// would look different. Seven is a value only a marshal could show.
#[test]
fn a_bumped_counter_never_reaches_the_marshalled_row() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(&chats_ddl_from_the_dump()).unwrap();
    quilltap_core::db::chats_transcript_version_repair::ensure_chats_transcript_version_column(
        &conn,
    )
    .unwrap();
    conn.execute(
        "INSERT INTO chats (id, userId, title, createdAt, updatedAt) \
         VALUES ('c1', 'u1', 'The Reading Room', '2026-01-01T00:00:00.000Z', \
                 '2026-01-01T00:00:00.000Z')",
        [],
    )
    .unwrap();
    conn.execute("UPDATE chats SET transcriptVersion = 7 WHERE id = 'c1'", [])
        .unwrap();

    // The plant is real — or the assertion below proves nothing.
    let planted: i64 = conn
        .query_row(
            "SELECT transcriptVersion FROM chats WHERE id = 'c1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(planted, 7);

    let row = quilltap_core::db::chats_read::find_by_id(&conn, "c1")
        .unwrap()
        .expect("the planted chat");
    let keys: BTreeSet<&String> = row.as_object().expect("an object").keys().collect();
    assert!(
        !keys.iter().any(|k| k.as_str() == COLUMN),
        "`chats_read::marshal_row` marshalled `{COLUMN}` — it reads by POSITION \
         from its own 100-column list, so the only way this fails is someone \
         adding the column to that list"
    );

    // …and the same row is what the backup + `.qtap` chat writers serialize
    // (`services/backup/collect.rs` reads `chats_read::find_all`;
    // `qtap_export/records.rs` reads `chats_read::find_by_id`), so the absence
    // above IS the export mechanism. Nothing filters it out downstream,
    // because there is nothing to filter.
    let all = quilltap_core::db::chats_read::find_all(&conn).unwrap();
    assert_eq!(all.len(), 1);
    assert!(all[0].get(COLUMN).is_none());
}

/// The zone of `db/chats.rs` that can write a `chats` column — the
/// `ChatUpdate` struct and the `update` body — must never name the counter.
///
/// A source census in the `db_error_key_guard` idiom, because a behavioural
/// test alone is weak here: a new writer would pass one simply by living
/// somewhere else. Scoped to the two zones rather than the whole file, so that
/// P4.D183's `get_transcript_version` — a raw single-column READ, the one
/// reader v4 has — can land beside it without tripping this.
#[test]
fn the_chat_update_surface_cannot_reach_the_transcript_version() {
    let src = core_src("db/chats.rs");

    let struct_zone = braced_zone(&src, "pub struct ChatUpdate {");
    assert!(
        !struct_zone.contains(COLUMN) && !struct_zone.contains("transcript_version"),
        "`ChatUpdate` grew a transcript-version field. v4's only writer is the \
         atomic `SET v = v + 1` in `ChatMessagesOps.announceTranscriptChange`; a \
         settable field here is precisely the whole-row rewind the column was \
         moved out of the schema to prevent."
    );

    let update_zone = braced_zone(&src, "pub fn update(");
    assert!(
        !update_zone.contains(COLUMN),
        "`ChatsRepository::update` names `{COLUMN}` — the whole-row update path \
         must not touch the counter"
    );
}

/// The backup/`.qtap` `chats` field spec is the marshalled row itself, but the
/// FILE spec is an explicit list — assert the counter is not smuggled in
/// through the neighbouring table's spec either. Cheap, and it fails loudly if
/// someone "helpfully" adds a chats field list later.
#[test]
fn no_backup_field_spec_names_the_transcript_version() {
    let src = core_src("services/backup/collect.rs");
    assert!(
        !src.contains(COLUMN),
        "`backup/collect.rs` names `{COLUMN}` — v4 exports it from nowhere, and \
         it has no export-side filter to copy because it needs none"
    );
    let src = core_src("services/qtap_export/records.rs");
    assert!(
        !src.contains(COLUMN),
        "`qtap_export/records.rs` names `{COLUMN}` — a `.qtap` chat record must \
         not carry it, and an import must start every chat at zero"
    );
}

/// The body of the first `{ … }` block opening after `needle`, by brace
/// balance. String literals in these zones carry no braces, and both zones are
/// production code (the `#[cfg(test)]` module sits below `update`).
fn braced_zone(src: &str, needle: &str) -> String {
    let start = src
        .find(needle)
        .unwrap_or_else(|| panic!("`{needle}` is defined in db/chats.rs"));
    let open = start + src[start..].find('{').expect("a body");
    let mut depth = 0usize;
    for (offset, ch) in src[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let zone = &src[open..open + offset];
                    assert!(
                        !zone.contains("#[cfg(test)]"),
                        "the scanned zone must be production code only"
                    );
                    return zone.to_string();
                }
            }
            _ => {}
        }
    }
    panic!("unbalanced braces after `{needle}`")
}
