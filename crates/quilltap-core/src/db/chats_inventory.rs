//! The `chats` **carried-Inventory** ops (P4.D324, the Inventory keystone):
//! v4's four `ChatsRepository` methods over `chats.equippedInventory`
//! (`chats.repository.ts:780-877`, `4fbdad186` + `e1872c94f`) and the
//! module-level `normalizeEquippedInventory` (`:1005-1024`).
//!
//! `equippedInventory` is `{ [characterId]: string[] }` of Inventory object
//! ids. Like the Concierge columns it is PATCH-ONLY in v4 (`patchOnlyFields`
//! — a whole-row `update` never carries a stale copy back); in v5 that is
//! structural: `ChatUpdate` is a patch struct and no whole-row writer exists.
//! [`ChatsInventoryRepository::set_equipped_inventory`] is the one runtime
//! writer (reached through `inventory::equip` and `inventory::apply_defaults`);
//! `chats.create` carries the column whole on restore / import (P4.D327's).
//!
//! ## v4's raw collection ops, ported as raw SQL
//!
//! v4's writers read the row through `collection.findOne` and write through
//! `collection.updateOne({ $set })` — `UPDATE "chats" SET
//! "equippedInventory" = ? WHERE "id" = ?` with the map `JSON.stringify`d,
//! NOT the repository's `update` (so `updatedAt` is never touched). The port
//! reads and writes the one column the same way. The stored text is
//! `JSON.stringify` of the map in INSERTION order (`preserve_order`): a
//! character's entry keeps its position on a re-set; `delete` then re-add
//! APPENDS (`reset_inventory_to_defaults` moves the entry to the END — T2).
//!
//! ## The fallbacks
//!
//! Every method is a v4 FALLBACK `safeQuery` — a failure logs v4's sentence
//! through `db::fallback` (the bare SQLite message) and answers the fallback;
//! nothing throws. A v5 that reaches a pre-column `chats` (a committed
//! harness pair opened without the host's ensures — the migration runner is
//! deferred) hits exactly `Failed to write equipped inventory` (R-K).
//!
//! The `LIKE '%<id>%'` pre-filter of [`find_chat_ids_carrying_inventory_id`]
//! is v4's `$like` verbatim (no `ESCAPE` clause — `query-translator.ts`'s
//! `"<col>" LIKE ?`): an id carrying `%` / `_` would over-match before the JS
//! filter; UUIDs never do (recorded, not guarded — v4 doesn't).
//!
//! ## The pre-column table and the DQS build difference (measured)
//!
//! v4's SQLite refuses a double-quoted identifier that names no column
//! (`no such column: "equippedInventory" - should this be a string literal in
//! single-quotes?`); v5's build keeps SQLite's legacy double-quoted-string
//! fallback, so the same SQL here would silently read the TEXT
//! `'equippedInventory'`. The two reads that must behave like v4's whole-row
//! `findOne` (the column simply absent) therefore PROBE the column, and the
//! `LIKE` filter names the column UNQUOTED so a pre-column table fails as v4's
//! does (`no such column: equippedInventory` — v4's text less its hint).

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{Map, Value};

use super::{chats_read, fallback, DbError};

/// v4 `EQUIPPED_INVENTORY_FIELD`.
pub const EQUIPPED_INVENTORY_FIELD: &str = "equippedInventory";

/// The carried map: `characterId → ids`, in v4's `Object.entries` order.
pub type EquippedInventoryMap = Map<String, Value>;

/// `chats.equippedInventory` as a clean map (v4 `normalizeEquippedInventory`):
/// unconstrained JSON on disk (and a string when read raw), so anything that
/// is not a `{ id: string[] }` entry is dropped on the way out. A string is
/// `JSON.parse`d (a failure → `{}`); a non-object / array / null → `{}`; a
/// non-array value DROPPED; arrays filtered to strings.
pub fn normalize_equipped_inventory(raw: Option<&Value>) -> Vec<(String, Vec<String>)> {
    let parsed;
    let value = match raw {
        Some(Value::String(s)) => match serde_json::from_str::<Value>(s) {
            Ok(v) => {
                parsed = v;
                &parsed
            }
            Err(_) => return Vec::new(),
        },
        Some(v) => v,
        None => return Vec::new(),
    };
    let Value::Object(o) = value else {
        return Vec::new();
    };
    o.iter()
        .filter_map(|(character_id, ids)| {
            let Value::Array(a) = ids else {
                return None;
            };
            Some((
                character_id.clone(),
                a.iter()
                    .filter_map(|id| id.as_str().map(str::to_string))
                    .collect(),
            ))
        })
        .collect()
}

/// The normalised map as the stored JSON object (insertion order kept).
fn to_object(state: &[(String, Vec<String>)]) -> Map<String, Value> {
    state
        .iter()
        .map(|(k, ids)| {
            (
                k.clone(),
                Value::Array(ids.iter().cloned().map(Value::String).collect()),
            )
        })
        .collect()
}

/// `ids.length ?? null`, rendered for the DEBUG line.
fn count_field(ids: Option<&[String]>) -> String {
    ids.map(|ids| ids.len().to_string())
        .unwrap_or_else(|| "null".to_string())
}

/// Repository over a borrowed MAIN-db connection.
pub struct ChatsInventoryRepository<'c> {
    conn: &'c Connection,
}

impl<'c> ChatsInventoryRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// The raw cell through v4's `collection.findOne({id})` — `None` when the
    /// row is absent; `Some(None)` for a NULL cell, or when the table has no
    /// `equippedInventory` column at all (v4's whole-row read finds the key
    /// absent; see the header's DQS note).
    fn raw_cell(&self, chat_id: &str) -> Result<Option<Option<String>>, DbError> {
        let present = self
            .conn
            .prepare("SELECT 1 FROM pragma_table_info('chats') WHERE name = 'equippedInventory'")
            .and_then(|mut stmt| stmt.exists([]))
            .map_err(DbError::Sqlite)?;
        let sql = if present {
            "SELECT \"equippedInventory\" FROM \"chats\" WHERE \"id\" = ?1"
        } else {
            "SELECT NULL FROM \"chats\" WHERE \"id\" = ?1"
        };
        self.conn
            .query_row(sql, params![chat_id], |r| r.get::<_, Option<String>>(0))
            .optional()
            .map_err(DbError::Sqlite)
    }

    fn write_state(&self, chat_id: &str, state: &[(String, Vec<String>)]) -> Result<bool, DbError> {
        let text = serde_json::to_string(&Value::Object(to_object(state)))
            .map_err(|e| DbError::Internal(e.to_string()))?;
        let changed = self
            .conn
            .execute(
                "UPDATE \"chats\" SET \"equippedInventory\" = ?1 WHERE \"id\" = ?2",
                params![text, chat_id],
            )
            .map_err(DbError::Sqlite)?;
        Ok(changed > 0)
    }

    /// `getEquippedInventory` — the chat's whole carried-objects map, `{}`
    /// when none is stored (or the chat is absent). The chat is read through
    /// the repository's own fallback `findById`.
    pub fn get_equipped_inventory(&self, chat_id: &str) -> Vec<(String, Vec<String>)> {
        fallback::equipped_inventory_or_empty(chat_id, || {
            let chat = chats_read::find_by_id_or_none(self.conn, chat_id);
            Ok(normalize_equipped_inventory(
                chat.as_ref().and_then(|c| c.get(EQUIPPED_INVENTORY_FIELD)),
            ))
        })
    }

    /// `setEquippedInventory` — replace one character's carried ids in a chat
    /// (de-duplicated first-seen, then sorted with JS's default sort — code
    /// units, byte order on lowercase-hex UUIDs — so equal sets hash equal)
    /// and patch the column alone. `None` removes the character's entry, so
    /// arrival defaults apply again. Answers whether a row was written; the
    /// DEBUG `Equipped inventory write` follows on BOTH arms (`count` is the
    /// UN-deduped length).
    pub fn set_equipped_inventory(
        &self,
        chat_id: &str,
        character_id: &str,
        ids: Option<&[String]>,
    ) -> bool {
        let written = fallback::equipped_inventory_written_or_false(chat_id, character_id, || {
            let Some(cell) = self.raw_cell(chat_id)? else {
                return Ok(false);
            };
            let mut state = normalize_equipped_inventory(cell.map(Value::String).as_ref());
            match ids {
                None => state.retain(|(k, _)| k != character_id),
                Some(ids) => {
                    let mut unique: Vec<String> = Vec::new();
                    for id in ids {
                        if !unique.contains(id) {
                            unique.push(id.clone());
                        }
                    }
                    unique.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
                    match state.iter_mut().find(|(k, _)| k == character_id) {
                        Some((_, existing)) => *existing = unique,
                        None => state.push((character_id.to_string(), unique)),
                    }
                }
            }
            self.write_state(chat_id, &state)
        });
        tracing::debug!(
            target: "quilltap::db",
            chatId = %chat_id,
            characterId = %character_id,
            count = %count_field(ids),
            cleared = ids.is_none(),
            written,
            context = "inventory",
            "Equipped inventory write"
        );
        written
    }

    /// `findChatIdsCarryingInventoryId` — the chats whose carried set for
    /// `character_id` lists `item_id`: v4's `$like '%<id>%'` pre-filter, then
    /// the exact JS filter. A read; the caller rewrites each under its chat's
    /// write (`drop_carried_everywhere`).
    pub fn find_chat_ids_carrying_inventory_id(
        &self,
        character_id: &str,
        item_id: &str,
    ) -> Vec<String> {
        let ids = fallback::chats_carrying_inventory_id_or_empty(character_id, item_id, || {
            let mut stmt = self
                .conn
                .prepare("SELECT \"id\", equippedInventory FROM \"chats\" WHERE equippedInventory LIKE ?1")
                .map_err(DbError::Sqlite)?;
            let rows = stmt
                .query_map(params![format!("%{item_id}%")], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
                })
                .map_err(DbError::Sqlite)?;
            let mut out = Vec::new();
            for row in rows {
                let (id, cell) = row.map_err(DbError::Sqlite)?;
                let carries = normalize_equipped_inventory(cell.map(Value::String).as_ref())
                    .into_iter()
                    .find(|(k, _)| k == character_id)
                    .is_some_and(|(_, ids)| ids.iter().any(|i| i == item_id));
                if carries {
                    out.push(id);
                }
            }
            Ok(out)
        });
        tracing::debug!(
            target: "quilltap::db",
            characterId = %character_id,
            itemId = %item_id,
            chats = ids.len(),
            context = "inventory",
            "Chats carrying an inventory id"
        );
        ids
    }

    /// `removeEquippedInventoryId` — drop one object id from one character's
    /// carried set in one chat (a read-modify-write of the whole map; other
    /// characters' entries untouched; the rest NOT re-sorted). `false` when
    /// the row is missing or the id is absent.
    pub fn remove_equipped_inventory_id(
        &self,
        chat_id: &str,
        character_id: &str,
        item_id: &str,
    ) -> bool {
        let written = fallback::equipped_inventory_id_removed_or_false(
            chat_id,
            character_id,
            item_id,
            || {
                let Some(cell) = self.raw_cell(chat_id)? else {
                    return Ok(false);
                };
                let mut state = normalize_equipped_inventory(cell.map(Value::String).as_ref());
                let Some((_, ids)) = state.iter_mut().find(|(k, _)| k == character_id) else {
                    return Ok(false);
                };
                if !ids.iter().any(|i| i == item_id) {
                    return Ok(false);
                }
                ids.retain(|i| i != item_id);
                self.write_state(chat_id, &state)
            },
        );
        tracing::debug!(
            target: "quilltap::db",
            chatId = %chat_id,
            characterId = %character_id,
            itemId = %item_id,
            written,
            context = "inventory",
            "Equipped inventory id removed"
        );
        written
    }
}

/// `normalize_equipped_inventory` as the stored-object form (what a chat
/// row's `equippedInventory` reads as once cleaned).
pub fn normalized_equipped_inventory_object(raw: Option<&Value>) -> Map<String, Value> {
    to_object(&normalize_equipped_inventory(raw))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::global_capture;
    use serde_json::json;

    // The pre-column pin shares its callsites with the un-captured tests here,
    // so every test installs the process-global rig first: a thread-scoped
    // `captured_with` lost the `Interest` race (a sibling reached the WARN
    // with no subscriber armed and tracing cached it as "never").
    fn db() -> Connection {
        global_capture::install();
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE chats (id TEXT PRIMARY KEY, equippedInventory TEXT DEFAULT NULL);\
             INSERT INTO chats (id) VALUES ('c1');",
        )
        .unwrap();
        c
    }

    fn cell(c: &Connection) -> Option<String> {
        c.query_row(
            "SELECT equippedInventory FROM chats WHERE id = 'c1'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn set_dedups_sorts_keeps_position_and_reset_appends() {
        let c = db();
        let repo = ChatsInventoryRepository::new(&c);
        assert!(repo.set_equipped_inventory(
            "c1",
            "a",
            Some(&["y".into(), "x".into(), "y".into()])
        ));
        assert!(repo.set_equipped_inventory("c1", "b", Some(&[])));
        assert!(repo.set_equipped_inventory("c1", "a", Some(&["z".into()])));
        assert_eq!(cell(&c).as_deref(), Some(r#"{"a":["z"],"b":[]}"#));
        assert!(repo.set_equipped_inventory("c1", "a", None));
        assert!(repo.set_equipped_inventory("c1", "a", Some(&["q".into()])));
        assert_eq!(cell(&c).as_deref(), Some(r#"{"b":[],"a":["q"]}"#));
        assert!(!repo.set_equipped_inventory("missing", "a", Some(&[])));
        assert!(repo.remove_equipped_inventory_id("c1", "a", "q"));
        assert!(!repo.remove_equipped_inventory_id("c1", "a", "q"));
        assert_eq!(cell(&c).as_deref(), Some(r#"{"b":[],"a":[]}"#));
    }

    #[test]
    fn normalize_shapes() {
        global_capture::install();
        assert!(normalize_equipped_inventory(Some(&json!("junk"))).is_empty());
        assert!(normalize_equipped_inventory(Some(&json!("[]"))).is_empty());
        assert!(normalize_equipped_inventory(Some(&Value::Null)).is_empty());
        assert_eq!(
            normalize_equipped_inventory(Some(&json!(r#"{"a":"nope","b":["z",1]}"#))),
            vec![("b".to_string(), vec!["z".to_string()])]
        );
    }

    #[test]
    fn a_pre_column_table_falls_back_to_false() {
        global_capture::install();
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE chats (id TEXT PRIMARY KEY); INSERT INTO chats VALUES ('c1');",
        )
        .unwrap();
        let (written, lines) = global_capture::capture(|| {
            ChatsInventoryRepository::new(&c).set_equipped_inventory("c1", "a", Some(&[]))
        });
        assert!(!written);
        assert!(
            lines[0].contains("Failed to write equipped inventory"),
            "{lines:?}"
        );
        assert!(
            lines[0].contains("error=no such column: equippedInventory"),
            "{lines:?}"
        );
        assert!(
            lines[1].contains("Equipped inventory write") && lines[1].contains("written=false"),
            "{lines:?}"
        );
    }
}
