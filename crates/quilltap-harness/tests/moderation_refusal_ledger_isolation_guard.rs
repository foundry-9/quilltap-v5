//! P4.D225 — the Concierge refusal ledger's two `chats` columns
//! (`moderationRefusalCount`, `lastModerationRefusalAt`, v4 `49059fb14`) exist,
//! and NOTHING but the ledger's three repository operations reads, projects,
//! remaps or exports them.
//!
//! v4 keeps both deliberately outside `ChatMetadataSchema` and
//! `ChatMetadataBaseSchema` (`chats.repository.ts`'s "CONCIERGE REFUSAL LEDGER"
//! block: a counter inside the schema could be rewound by any concurrent
//! whole-row rewrite — the `transcriptVersion` reasoning). The phase-2 SPEC
//! says the opposite (declared in both schemas, exported) — the as-built code
//! is the authority (§R.4(a)). The negatives follow: Zod strips them from every
//! write, `generateDDL` never emits them, and a `.qtap` bundle never carries
//! them — `49059fb14` leaves the export schema untouched.
//!
//! The template is `transcript_version_isolation_guard.rs` (P4.D182): an
//! absence is the hardest thing to keep, so each negative is pinned where it
//! would break.

use std::collections::BTreeSet;
use std::path::PathBuf;

use rusqlite::Connection;
use serde_json::Value;

const COLUMNS: [&str; 2] = ["moderationRefusalCount", "lastModerationRefusalAt"];

fn core_src(rel: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../quilltap-core/src")
        .join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

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

/// `generateDDL` cannot emit either column, so the D23 dump carries neither —
/// in `chats` or anywhere. If a later re-dump ever DOES, v4 has moved them
/// into the schema and every negative below needs re-measuring.
#[test]
fn the_d23_dump_carries_neither_column() {
    let raw = core_src("services/provisioning/fresh_schema.json");
    for c in COLUMNS {
        assert!(
            !raw.contains(c),
            "the D23 dump names `{c}` — v4 has moved it into ChatMetadataSchema; \
             re-measure the boot ensure, the ChatUpdate guard and the export negatives"
        );
    }
}

/// A real `chats` table from the dump, healed as a boot heals it, carrying a
/// NON-default tally — and `chats_read` hands back a row with neither key.
#[test]
fn a_recorded_tally_never_reaches_the_marshalled_row() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(&chats_ddl_from_the_dump()).unwrap();
    quilltap_core::db::chats_moderation_refusal_ledger_repair::ensure_chats_moderation_refusal_ledger_columns(&conn)
        .unwrap();
    conn.execute(
        "INSERT INTO chats (id, userId, title, createdAt, updatedAt) \
         VALUES ('c1', 'u1', 'The Reading Room', '2026-01-01T00:00:00.000Z', \
                 '2026-01-01T00:00:00.000Z')",
        [],
    )
    .unwrap();
    conn.execute(
        "UPDATE chats SET moderationRefusalCount = 3, \
           lastModerationRefusalAt = '2026-09-25T00:00:00.000Z' WHERE id = 'c1'",
        [],
    )
    .unwrap();

    // The plant is real — through the ledger's own reader.
    let ledger =
        quilltap_core::db::chats::ChatsRepository::new(&conn).get_moderation_refusal_ledger("c1");
    assert_eq!(ledger.count, 3);

    let row = quilltap_core::db::chats_read::find_by_id(&conn, "c1")
        .unwrap()
        .expect("the planted chat");
    let keys: BTreeSet<&String> = row.as_object().expect("an object").keys().collect();
    for c in COLUMNS {
        assert!(
            !keys.iter().any(|k| k.as_str() == c),
            "`chats_read::marshal_row` marshalled `{c}` — it reads by POSITION from \
             `ALL_COLUMNS`, so someone added the column to that list"
        );
    }
    // The same rows feed the backup + `.qtap` chat writers.
    let all = quilltap_core::db::chats_read::find_all(&conn).unwrap();
    assert_eq!(all.len(), 1);
    for c in COLUMNS {
        assert!(all[0].get(c).is_none());
    }
}

/// `ChatUpdate` and the whole-row `update` body must never name either column
/// — a settable field is exactly the rewind v4 moved them out of the schema to
/// prevent. The ledger's three methods live elsewhere in the file.
#[test]
fn the_chat_update_surface_cannot_reach_the_ledger() {
    let src = core_src("db/chats.rs");
    let struct_zone = braced_zone(&src, "pub struct ChatUpdate {");
    let update_zone = braced_zone(&src, "pub fn update(");
    for c in COLUMNS {
        assert!(!struct_zone.contains(c), "`ChatUpdate` grew `{c}`");
        assert!(
            !update_zone.contains(c),
            "`ChatsRepository::update` names `{c}`"
        );
    }
    for field in ["moderation_refusal_count", "last_moderation_refusal_at"] {
        assert!(!struct_zone.contains(field), "`ChatUpdate` grew `{field}`");
    }
}

/// Neither the backup collector, the `.qtap` record writer, the read
/// projection nor the vendored export schema names either column.
#[test]
fn nothing_exports_the_ledger() {
    for rel in [
        "services/backup/collect.rs",
        "services/qtap_export/records.rs",
        "db/chats_read.rs",
        "generators/qtap-export.schema.json",
        "services/qtap_export/schema-key-order.json",
    ] {
        let src = core_src(rel);
        for c in COLUMNS {
            assert!(
                !src.contains(c),
                "`{rel}` names `{c}` — v4 exports the ledger from nowhere"
            );
        }
    }
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
