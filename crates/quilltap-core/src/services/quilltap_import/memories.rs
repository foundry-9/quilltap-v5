//! v4 `importMemories` (`import-entities.ts:433-548` at `01a83539d`) — remap-only, always insert (no
//! conflict check). `characterId` MUST resolve through the character id-map (else
//! warn + skip); `aboutCharacterId` remaps through the SAME character map or →
//! null; `chatId`/`projectId` remap through the chats/projects maps or → null
//! (which, after a `duplicate` import, can be a PHANTOM id — the map quirk rides
//! straight into the stored FK, exactly as in v4); `tags` remap through the tags
//! map with `get(tag) || tag` (an unmapped tag keeps its ORIGINAL id, unlike the
//! null-on-miss FK remaps). Strips id/createdAt/updatedAt (create mints), then runs the whole entity
//! through `MemorySchema` ([`parse_create_memory`](crate::db::memories::parse_create_memory),
//! P4.161) BEFORE the write — a refused item logs v4's three repository
//! ERRORs, warns `Failed to import memory: <ZodError message>`, WARNs, and
//! counts `skipped` (v4 `:533-545`).
//!
//! ## The memory ids are minted UP FRONT (v4 `d58548051`, P4.D272)
//!
//! v4's comment, carried verbatim: "Memory ids are minted up front so digest
//! <-> cold-member references (`supersededById`, `consolidatedFrom`) can follow
//! their targets even when the target appears later in the bundle. With
//! `preserveIds` the source id is kept, so the map is the identity."
//! (`:445-453`). Before the loop, ONE id per bundle memory — every one,
//! including those later skipped for a missing character, refused by the
//! schema, or skipped as `preserveIdsSkips` — goes into a JS `Map` keyed by the
//! RAW `memory.id`. Then, per created row:
//!
//! - `relatedMemoryIds` / `consolidatedFrom` → `(x ?? []).map(remapMemoryRef)`,
//!   and `supersededById` → `x ? remapMemoryRef(x) : null` (JS truthiness, so
//!   `""` / `0` / `false` become `null`). `remapMemoryRef` is `map.get(ref) ??
//!   ref`: a ref to an id that is not in the bundle keeps its SOURCE id, and a
//!   ref to a bundle memory that is later SKIPPED or REFUSED gets that memory's
//!   MINTED id — which names no row, so it dangles by construction. Both are
//!   v4's shapes, ported as they are. A non-null, non-array link list has no
//!   `.map`, so the item fails with v4's TypeError sentence (`(memory.
//!   relatedMemoryIds ?? []).map is not a function`), after the tags'
//!   (evaluation order: tags are built before the payload literal).
//! - `tier ?? 'hot'` and `consolidatedAt ?? null` default.
//! - The row is created under the pre-minted id (`createOptions = { id:
//!   newMemoryId }` is now ALWAYS passed).
//! - `Imported memory` DEBUG `{memoryId, tier, source}` follows the create —
//!   `source` the bundle's RAW value, omitted when absent (winston drops
//!   `undefined`).
//!
//! **The `Map` keys follow JS SameValueZero over the JSON scalar** ([`MemoryKey`]):
//! an ABSENT id is its own key (`undefined`), distinct from `null` and `""`; a
//! string never equals a number; an object or array id is keyed by its own
//! identity, so no ref can ever reach it. **Duplicate keys: the LAST mint
//! wins** (`Map.set` overwrites) — two bundle memories sharing an id, or two
//! with no id at all, both resolve to ONE minted id, so the first lands and the
//! second fails the insert (`UNIQUE constraint failed: memories.id`). That is a
//! behaviour change from before `d58548051` (each minted its own and both
//! landed), ported faithfully under P4.D272 R-A and recorded as a v4-filing
//! candidate.
//!
//! **No heal** (P4.D272 R-G): instances whose first-startup seed import wrote
//! raw source ids into `relatedMemoryIds` (72 dangling refs across 21 of the
//! seed's 42 memories) keep them — v4 ships no backfill either. Imports from
//! now on resolve.
//!
//! **A non-string preserved id** (`preserveIds` with `id: 5`) still MINTS here
//! ([`super::mint_or_preserve`] reads `""`) where v4 passes the `5` through and
//! `MemorySchema` refuses it — a pre-existing divergence older than
//! `d58548051` (`getPreserveIdsCreateOptions` did the same), deferred by name
//! under P4.D272 R-E.
//!
//! ## Embeddings are dropped, not validated (v4 `7189a968`)
//!
//! `embedding` is excluded on purpose. A vector is only meaningful against the
//! model that produced it, so a foreign one silently corrupts semantic search
//! whenever the dimensionality happens to match — and v4's boot repair
//! (`repair-text-embeddings.ts`) would *preserve* a bad vector by converting it
//! to a valid blob rather than discarding it. Import time is the only correct
//! place to drop it. The orchestrator enqueues an `EMBEDDING_GENERATE` per
//! created row (see `mod.rs`'s `enqueue_imported_memory_embeddings`).
//!
//! (History: before `7189a968`, v4's Zod union REJECTED the NDJSON writer's
//! `JSON.stringify(Float32Array)` object shape, so an embedding-bearing export
//! could not re-import its memories at all — each landed in the per-item catch.
//! The destructure-before-validate at `import-entities.ts:508` retired that
//! trap on both sides: any embedding shape is now silently dropped. The
//! standing irony note that used to live here is therefore history too.)

use std::collections::HashMap;

use rusqlite::Connection;
use serde_json::Value;

use super::{IdMaps, ImportOptions};
use crate::db::memories::{CreateOptions, MemoriesRepository};
use crate::db::DbError;

pub(super) struct Counts {
    pub imported: u32,
    pub skipped: u32,
    /// v4 `createdIds` (`import-entities.ts:443`) — `{id, characterId}` per
    /// created row, in creation order, for the post-reconcile embedding enqueue.
    pub created_ids: Vec<(String, String)>,
}

/// A bundle memory's raw `id` as a key of v4's `memoryIdMap` — JS `Map`
/// equality (SameValueZero) over the JSON value. See the module doc.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum MemoryKey {
    /// `memory.id` is `undefined` — its own key, distinct from `null`.
    Absent,
    Null,
    Bool(bool),
    /// The number's IEEE-754 bits, `-0` folded into `+0` (SameValueZero), so
    /// `1` and `1.0` are one key. JSON carries no `NaN`.
    Number(u64),
    String(String),
    /// An object / array id: identity-keyed, so the key is the item's own
    /// position — no ref, being a different object, can reach it.
    Identity(usize),
}

impl MemoryKey {
    /// The key a ref looks up: `None` for an object / array (never a hit).
    fn of_scalar(v: &Value) -> Option<MemoryKey> {
        Some(match v {
            Value::Null => MemoryKey::Null,
            Value::Bool(b) => MemoryKey::Bool(*b),
            Value::Number(n) => {
                let f = n.as_f64().unwrap_or(f64::NAN);
                MemoryKey::Number(if f == 0.0 {
                    0f64.to_bits()
                } else {
                    f.to_bits()
                })
            }
            Value::String(s) => MemoryKey::String(s.clone()),
            Value::Array(_) | Value::Object(_) => return None,
        })
    }

    /// The key bundle item `index` sets: its raw `id`.
    fn of_item(memory: &Value, index: usize) -> MemoryKey {
        match memory.get("id") {
            None => MemoryKey::Absent,
            Some(id) => MemoryKey::of_scalar(id).unwrap_or(MemoryKey::Identity(index)),
        }
    }
}

/// v4's `memoryIdMap` + `remapMemoryRef` (`import-entities.ts:449-453`).
struct MemoryIdMap {
    /// `None` under `preserveIds`: the map is the identity there (every value
    /// is its own key), so every lookup answers the ref / source id itself.
    minted: Option<HashMap<MemoryKey, String>>,
}

impl MemoryIdMap {
    /// One `randomUUID()` per bundle memory, in bundle order, `Map.set` —
    /// so a repeated key keeps the LAST mint.
    fn mint(memories: &[Value], options: &ImportOptions) -> Self {
        let minted = (!options.preserve_ids).then(|| {
            let mut map = HashMap::new();
            for (index, memory) in memories.iter().enumerate() {
                map.insert(
                    MemoryKey::of_item(memory, index),
                    uuid::Uuid::new_v4().to_string(),
                );
            }
            map
        });
        MemoryIdMap { minted }
    }

    /// `remapMemoryRef(ref)` = `memoryIdMap.get(ref) ?? ref`.
    fn remap_ref(&self, r: &Value) -> Value {
        self.minted
            .as_ref()
            .and_then(|map| map.get(&MemoryKey::of_scalar(r)?))
            .map_or_else(|| r.clone(), |id| Value::String(id.clone()))
    }

    /// `newMemoryId = memoryIdMap.get(memory.id) ?? memory.id`, reaching
    /// `_create`'s `options?.id || this.generateId()`: the minted id, or under
    /// `preserveIds` the source id (an absent / empty one mints —
    /// [`super::mint_or_preserve`]).
    fn new_id(&self, memory: &Value, index: usize, options: &ImportOptions) -> String {
        match &self.minted {
            Some(map) => map
                .get(&MemoryKey::of_item(memory, index))
                .cloned()
                .expect("every bundle memory was minted up front"),
            None => super::mint_or_preserve(options, &super::id_of(memory)).0,
        }
    }
}

/// `(memory[key] ?? []).map(remapMemoryRef)` — a present, non-null,
/// non-array value has no `.map`: v4's TypeError, the item's failure.
fn remapped_link_list(
    memory: &Value,
    key: &str,
    type_error: &str,
    ids: &MemoryIdMap,
) -> Result<Value, String> {
    match memory.get(key) {
        None | Some(Value::Null) => Ok(Value::Array(Vec::new())),
        Some(Value::Array(items)) => Ok(Value::Array(
            items.iter().map(|r| ids.remap_ref(r)).collect(),
        )),
        Some(_) => Err(type_error.to_string()),
    }
}

/// The five consolidation fields of v4's payload literal (`:516-528`), in its
/// evaluation order, written over the spread: `relatedMemoryIds`, `tier ??
/// 'hot'`, `supersededById` (truthy → remapped, else `null`),
/// `consolidatedFrom`, `consolidatedAt ?? null`.
fn insert_consolidation_fields(
    memory: &Value,
    payload: &mut serde_json::Map<String, Value>,
    ids: &MemoryIdMap,
) -> Result<(), String> {
    let present = |k: &str| memory.get(k).filter(|v| !v.is_null()).cloned();
    let related = remapped_link_list(
        memory,
        "relatedMemoryIds",
        "(memory.relatedMemoryIds ?? []).map is not a function",
        ids,
    )?;
    let tier = present("tier").unwrap_or_else(|| Value::String("hot".into()));
    let superseded_by = match memory.get("supersededById") {
        Some(raw) if crate::api::system_qtap::js_truthy(Some(raw)) => ids.remap_ref(raw),
        _ => Value::Null,
    };
    let consolidated_from = remapped_link_list(
        memory,
        "consolidatedFrom",
        "(memory.consolidatedFrom ?? []).map is not a function",
        ids,
    )?;
    let consolidated_at = present("consolidatedAt").unwrap_or(Value::Null);
    payload.insert("relatedMemoryIds".into(), related);
    payload.insert("tier".into(), tier);
    payload.insert("supersededById".into(), superseded_by);
    payload.insert("consolidatedFrom".into(), consolidated_from);
    payload.insert("consolidatedAt".into(), consolidated_at);
    Ok(())
}

pub(super) fn import_memories(
    main: &Connection,
    memories: &[Value],
    options: &ImportOptions,
    id_maps: &IdMaps,
    warnings: &mut Vec<String>,
) -> Result<Counts, DbError> {
    let mut imported = 0u32;
    let mut skipped = 0u32;
    let mut created_ids: Vec<(String, String)> = Vec::new();

    let repo = MemoriesRepository::new(main);
    let memory_ids = MemoryIdMap::mint(memories, options);

    for (index, memory) in memories.iter().enumerate() {
        // Skip-if-present rehydrate (spec §6/F4, `01e481f6`): the memory is
        // already back — a partial restore being re-run. The surviving row
        // wins. v4 checks this BEFORE the character remap, so a sanctioned skip
        // never trips the "references non-existent character" warning.
        let source_id = super::id_of(memory);
        if options.preserve_ids && id_maps.preserve_ids_skips.contains(&source_id) {
            skipped += 1;
            continue;
        }

        // Remap character ID (required — a memory with no destination character is
        // dropped with a warning).
        let source_character_id = memory
            .get("characterId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let Some(new_character_id) = id_maps.characters.get(source_character_id) else {
            warnings.push(format!(
                "Memory references non-existent character {source_character_id}"
            ));
            skipped += 1;
            continue;
        };

        // v4 builds the create payload from the RAW item (`import-entities.ts:
        // 459-498`): the item minus `id` / `createdAt` / `updatedAt` /
        // `embedding`, with the four FKs remapped. Each FK remap runs only on a
        // TRUTHY value (`if (memory.aboutCharacterId)`) and answers the map's
        // hit `|| null`; a falsy value (`""`, `0`, `null`) rides through as it
        // came — so `aboutCharacterId: ""` reaches the schema, which refuses it.
        let mut payload = memory.as_object().cloned().unwrap_or_default();
        for k in ["id", "createdAt", "updatedAt", "embedding"] {
            payload.remove(k);
        }
        let remap = |k: &str, map: &super::IdMap| {
            let raw = memory.get(k)?;
            Some(if crate::api::system_qtap::js_truthy(Some(raw)) {
                raw.as_str()
                    .and_then(|v| map.get(v))
                    .map_or(Value::Null, |v| Value::String(v.to_string()))
            } else {
                raw.clone()
            })
        };
        payload.insert(
            "characterId".into(),
            Value::String(new_character_id.to_string()),
        );
        for (k, map) in [
            ("aboutCharacterId", &id_maps.characters),
            ("chatId", &id_maps.chats),
            ("projectId", &id_maps.projects),
        ] {
            if let Some(v) = remap(k, map) {
                payload.insert(k.into(), v);
            }
        }
        // tags: `if (memory.tags && memory.tags.length > 0)` → `get(tag) ||
        // tag` per element (an unmapped tag keeps its ORIGINAL id), then
        // `.filter(id => id !== null)` — a `null` element is DROPPED, never
        // handed to the schema (`import-entities.ts:485-488`; the `94fbb1ae3`
        // boot-hardness unification's review). A non-empty STRING passes the
        // guard and has no `.map` — v4's TypeError is the item's failure.
        let tags_failure = match memory.get("tags") {
            Some(Value::Array(items)) if !items.is_empty() => {
                let mapped = items
                    .iter()
                    .map(|t| match t.as_str().and_then(|t| id_maps.tags.get(t)) {
                        Some(hit) => Value::String(hit.to_string()),
                        None => t.clone(),
                    })
                    .filter(|t| !t.is_null())
                    .collect();
                payload.insert("tags".into(), Value::Array(mapped));
                None
            }
            Some(Value::String(s)) if !s.is_empty() => {
                Some("memory.tags.map is not a function".to_string())
            }
            _ => None,
        };

        // `:509` / `:512` — the pre-minted id, ALWAYS the create's id.
        let new_id = memory_ids.new_id(memory, index, options);
        // `:516-528` — the payload literal's consolidation fields, AFTER the
        // tags (whose TypeError, built first, wins).
        let fields = tags_failure.map_or_else(
            || insert_consolidation_fields(memory, &mut payload, &memory_ids),
            Err,
        );
        // The log's `tier` is `payload.tier` — `memory.tier ?? 'hot'`.
        let tier = super::log_field_text(payload.get("tier")).unwrap_or_default();
        // v4's `_create` validates the WHOLE entity (`MemorySchema`) before the
        // insert (P4.161, dogfood #152 — v5 used to coerce: `importance: 5`
        // and `kind: "bogus-kind"` were written as they came).
        let parsed = match fields {
            Err(type_error) => Err(type_error),
            Ok(()) => {
                crate::db::memories::parse_create_memory(&Value::Object(payload), Some(&new_id))
                    .inspect_err(|zod| {
                        crate::db::fallback::log_refused_create("memories", zod);
                        crate::db::fallback::log_memory_create_failure(
                            new_character_id,
                            &DbError::Internal(zod.clone()),
                        );
                    })
            }
        };
        let now = crate::clock::now_iso();
        let opts = CreateOptions {
            id: new_id,
            created_at: now.clone(),
            updated_at: now,
        };
        let created = parsed.and_then(|create| {
            repo.create(&create, &opts)
                .map_err(|e| super::item_error_text(&e))
        });
        match created {
            Ok(()) => {
                created_ids.push((opts.id.clone(), new_character_id.to_string()));
                // v4 `:531` (`d58548051`): after the push, before the count.
                // `source` is the bundle's RAW value — absent → omitted.
                tracing::debug!(
                    memoryId = %opts.id,
                    tier = %tier,
                    source = super::log_field_text(memory.get("source")).map(tracing::field::display),
                    "Imported memory"
                );
                imported += 1;
            }
            Err(text) => {
                warnings.push(format!("Failed to import memory: {text}"));
                // v4 `import-entities.ts:540` (P4.148 Tier 2 item 17).
                tracing::warn!(memoryId = super::id_field(memory), error = %text, "Failed to import memory");
                skipped += 1;
            }
        }
    }

    Ok(Counts {
        imported,
        skipped,
        created_ids,
    })
}

/// P4.D272 MC.1 — the up-front mint, the link remap, the tier defaults and the
/// `Imported memory` DEBUG, over an in-memory instance provisioned from
/// `fresh_schema.json` (the `import_warn_pins` idiom). The cross-engine proof
/// is `system_import_state`'s `execute_memory_tiers{,_preserve}` and
/// `qtap_import_equivalence`'s link comparand; these pin the shapes in place.
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const CHAR_SRC: &str = "a2720000-0000-4000-8000-000000000001";
    const CHAR_DST: &str = "a2720000-0000-4000-8000-0000000000d1";
    const TARGET: &str = "quilltap_core::services::quilltap_import::memories";

    fn mid(n: u32) -> String {
        format!("f2720000-0000-4000-8000-0000000009{n:02}")
    }

    fn main_conn() -> Connection {
        let schema: Value =
            serde_json::from_str(include_str!("../provisioning/fresh_schema.json")).unwrap();
        let conn = Connection::open_in_memory().unwrap();
        for ddl in schema["main"].as_array().unwrap() {
            conn.execute_batch(ddl.as_str().unwrap()).unwrap();
        }
        conn
    }

    fn mem(id: Option<&str>, patch: Value) -> Value {
        let mut m = json!({
            "characterId": CHAR_SRC,
            "content": "c",
            "summary": "s",
            "source": "AUTO",
        });
        if let Some(id) = id {
            m["id"] = json!(id);
        }
        for (k, v) in patch.as_object().unwrap() {
            m[k] = v.clone();
        }
        m
    }

    /// Run the importer on the caller thread (a plain `Connection`, no
    /// writer), capturing its lines.
    fn run(
        conn: &Connection,
        memories: &[Value],
        preserve: bool,
    ) -> (Counts, Vec<String>, Vec<String>) {
        let mut opts = ImportOptions::seed_defaults();
        opts.preserve_ids = preserve;
        let mut id_maps = IdMaps::default();
        id_maps.characters.set(CHAR_SRC.into(), CHAR_DST.into());
        let mut warnings = Vec::new();
        let (counts, lines) = crate::test_support::captured_with(|| {
            import_memories(conn, memories, &opts, &id_maps, &mut warnings).unwrap()
        });
        (counts, warnings, lines)
    }

    /// `summary → (id, tier, supersededById, consolidatedFrom,
    /// consolidatedAt, relatedMemoryIds)`, one row per summary.
    fn rows(conn: &Connection) -> Vec<(String, Value)> {
        let mut stmt = conn
            .prepare(
                "SELECT summary, id, tier, supersededById, consolidatedFrom, consolidatedAt, \
                 relatedMemoryIds FROM memories ORDER BY summary, id",
            )
            .unwrap();
        stmt.query_map([], |r| {
            let arr = |i: usize| -> rusqlite::Result<Value> {
                Ok(serde_json::from_str(&r.get::<_, String>(i)?).unwrap())
            };
            Ok((
                r.get::<_, String>(0)?,
                json!({
                    "id": r.get::<_, String>(1)?,
                    "tier": r.get::<_, Option<String>>(2)?,
                    "supersededById": r.get::<_, Option<String>>(3)?,
                    "consolidatedFrom": arr(4)?,
                    "consolidatedAt": r.get::<_, Option<String>>(5)?,
                    "relatedMemoryIds": arr(6)?,
                }),
            ))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
    }

    fn row<'a>(rows: &'a [(String, Value)], summary: &str) -> &'a Value {
        &rows.iter().find(|(s, _)| s == summary).unwrap().1
    }

    fn imported_lines(lines: &[String]) -> Vec<&String> {
        lines
            .iter()
            .filter(|l| l.contains(" Imported memory"))
            .collect()
    }

    /// v4's jest `imports a digest and its cold members with links remapped to
    /// the new ids` — the member listed BEFORE the digest it points at.
    #[test]
    fn a_digest_and_its_cold_member_link_to_the_new_ids() {
        let conn = main_conn();
        let (member, digest) = (mid(1), mid(2));
        let (counts, warnings, _) = run(
            &conn,
            &[
                mem(
                    Some(&member),
                    json!({"summary": "member", "tier": "cold", "supersededById": digest}),
                ),
                mem(
                    Some(&digest),
                    json!({"summary": "digest", "source": "CONSOLIDATED",
                           "consolidatedFrom": [member], "relatedMemoryIds": [member]}),
                ),
            ],
            false,
        );
        assert_eq!((counts.imported, counts.skipped), (2, 0), "{warnings:?}");
        let rows = rows(&conn);
        let (m, d) = (row(&rows, "member"), row(&rows, "digest"));
        assert_ne!(m["id"], json!(member), "the member was re-minted");
        assert_ne!(d["id"], json!(digest), "the digest was re-minted");
        assert_eq!(m["tier"], json!("cold"));
        assert_eq!(m["supersededById"], d["id"]);
        assert_eq!(d["consolidatedFrom"], json!([m["id"]]));
        assert_eq!(d["relatedMemoryIds"], json!([m["id"]]));
        // `createdIds` carries the minted ids, in creation order.
        let ids: Vec<&str> = counts.created_ids.iter().map(|(i, _)| i.as_str()).collect();
        assert_eq!(
            ids,
            vec![m["id"].as_str().unwrap(), d["id"].as_str().unwrap()]
        );
    }

    /// v4's jest `imports a bundle memory lacking tier fields as hot,
    /// unlinked`.
    #[test]
    fn a_bundle_memory_lacking_tier_fields_reads_hot_unlinked() {
        let conn = main_conn();
        let (counts, warnings, _) = run(&conn, &[mem(Some(&mid(1)), json!({}))], false);
        assert_eq!(counts.imported, 1, "{warnings:?}");
        let rows = rows(&conn);
        let r = row(&rows, "s");
        assert_eq!(r["tier"], json!("hot"));
        assert_eq!(r["supersededById"], Value::Null);
        assert_eq!(r["consolidatedFrom"], json!([]));
        assert_eq!(r["consolidatedAt"], Value::Null);
        assert_eq!(r["relatedMemoryIds"], json!([]));
    }

    /// R-A: two bundle memories sharing an id — `Map.set` keeps the LAST mint,
    /// so both resolve to ONE id and the second fails the insert.
    #[test]
    fn a_shared_source_id_pair_loses_its_second_member() {
        let conn = main_conn();
        let (counts, warnings, lines) = run(
            &conn,
            &[
                mem(Some(&mid(1)), json!({"summary": "first"})),
                mem(Some(&mid(1)), json!({"summary": "second"})),
            ],
            false,
        );
        assert_eq!((counts.imported, counts.skipped), (1, 1));
        assert_eq!(
            warnings,
            vec!["Failed to import memory: UNIQUE constraint failed: memories.id".to_string()]
        );
        assert_eq!(rows(&conn).len(), 1);
        assert_eq!(imported_lines(&lines).len(), 1);
    }

    /// R-A, the `undefined` key: two ID-LESS memories collide the same way —
    /// and the WARN carries no `memoryId` (winston drops `undefined`).
    #[test]
    fn an_id_less_pair_loses_its_second_member() {
        let conn = main_conn();
        let (counts, warnings, lines) = run(
            &conn,
            &[
                mem(None, json!({"summary": "first"})),
                mem(None, json!({"summary": "second"})),
            ],
            false,
        );
        assert_eq!((counts.imported, counts.skipped), (1, 1), "{warnings:?}");
        assert!(lines.contains(&format!(
            "WARN {TARGET} Failed to import memory error=UNIQUE constraint failed: memories.id"
        )));
    }

    /// The keys are JS SameValueZero over the JSON value: a string id and a
    /// number id never collide, and `1` / `1.0` are one key.
    #[test]
    fn map_keys_follow_same_value_zero() {
        let k = |v: Value| MemoryKey::of_item(&json!({ "id": v }), 0);
        assert_ne!(k(json!("1")), k(json!(1)));
        assert_eq!(k(json!(1)), k(json!(1.0)));
        assert_eq!(k(json!(0)), k(json!(-0.0)));
        assert_ne!(k(Value::Null), MemoryKey::of_item(&json!({}), 0));
        assert_ne!(k(json!("")), k(Value::Null));
        // An object id is identity-keyed: no ref can reach it.
        assert_eq!(k(json!({"a": 1})), MemoryKey::Identity(0));
        assert_eq!(MemoryKey::of_scalar(&json!({"a": 1})), None);
    }

    /// Under `preserveIds` the map is the identity: every source id is kept,
    /// every ref unchanged — and an id-less pair BOTH land (each mints).
    #[test]
    fn preserve_ids_keeps_every_source_id_and_ref() {
        let conn = main_conn();
        let (member, digest) = (mid(1), mid(2));
        let (counts, warnings, _) = run(
            &conn,
            &[
                mem(
                    Some(&member),
                    json!({"summary": "member", "tier": "cold", "supersededById": digest}),
                ),
                mem(
                    Some(&digest),
                    json!({"summary": "digest", "consolidatedFrom": [member]}),
                ),
                mem(None, json!({"summary": "loose a"})),
                mem(None, json!({"summary": "loose b"})),
            ],
            true,
        );
        assert_eq!((counts.imported, counts.skipped), (4, 0), "{warnings:?}");
        let rows = rows(&conn);
        assert_eq!(row(&rows, "member")["id"], json!(member));
        assert_eq!(row(&rows, "member")["supersededById"], json!(digest));
        assert_eq!(row(&rows, "digest")["consolidatedFrom"], json!([member]));
        assert_ne!(row(&rows, "loose a")["id"], row(&rows, "loose b")["id"]);
    }

    /// An unknown ref keeps its SOURCE id; a ref to a bundle memory that is
    /// later SKIPPED gets that memory's MINTED id — which names no row.
    #[test]
    fn unknown_refs_keep_their_source_id_and_skipped_targets_dangle() {
        let conn = main_conn();
        let unknown = "f2720000-0000-4000-8000-0000000002ff";
        let skipped = mid(9);
        let (counts, _, _) = run(
            &conn,
            &[
                mem(
                    Some(&mid(1)),
                    json!({"summary": "linker", "supersededById": unknown,
                           "relatedMemoryIds": [unknown, skipped]}),
                ),
                mem(
                    Some(&skipped),
                    json!({"characterId": "a2720000-0000-4000-8000-0000000000ff"}),
                ),
            ],
            false,
        );
        assert_eq!((counts.imported, counts.skipped), (1, 1));
        let rows = rows(&conn);
        let r = row(&rows, "linker");
        assert_eq!(r["supersededById"], json!(unknown));
        let related = r["relatedMemoryIds"].as_array().unwrap();
        assert_eq!(related[0], json!(unknown));
        assert_ne!(
            related[1],
            json!(skipped),
            "the skipped target's id was minted"
        );
        assert!(
            rows.iter().all(|(_, row)| row["id"] != related[1]),
            "…and dangles"
        );
    }

    /// The `Imported memory` DEBUG — v4 `:531`'s fields, on the caller thread.
    #[test]
    fn the_imported_memory_debug_carries_v4s_fields() {
        let conn = main_conn();
        let (counts, _, lines) = run(&conn, &[mem(Some(&mid(1)), json!({"tier": "cold"}))], false);
        let id = &counts.created_ids[0].0;
        assert_eq!(
            imported_lines(&lines),
            vec![&format!(
                "DEBUG {TARGET} Imported memory memoryId={id} tier=cold source=AUTO"
            )]
        );
    }

    /// The silence legs: a refused, a skipped and a TypeError item log NO
    /// `Imported memory`; an absent `source` OMITS the field; a `null` tier
    /// logs `hot` (`memory.tier ?? 'hot'`).
    #[test]
    fn refused_skipped_and_type_error_items_log_nothing() {
        let conn = main_conn();
        let mut no_source = mem(Some(&mid(5)), json!({"tier": null}));
        no_source.as_object_mut().unwrap().remove("source");
        let (counts, warnings, lines) = run(
            &conn,
            &[
                mem(Some(&mid(1)), json!({"tier": "warm"})),
                mem(
                    Some(&mid(2)),
                    json!({"characterId": "a2720000-0000-4000-8000-0000000000ff"}),
                ),
                mem(Some(&mid(3)), json!({"consolidatedFrom": "x"})),
                mem(
                    Some(&mid(4)),
                    json!({"relatedMemoryIds": 5, "consolidatedFrom": "x"}),
                ),
                mem(Some(&mid(6)), json!({"tags": "x", "relatedMemoryIds": "y"})),
                no_source,
            ],
            false,
        );
        assert_eq!((counts.imported, counts.skipped), (1, 5));
        assert!(warnings.contains(
            &"Failed to import memory: (memory.consolidatedFrom ?? []).map is not a function"
                .to_string()
        ));
        assert!(warnings.contains(
            &"Failed to import memory: (memory.relatedMemoryIds ?? []).map is not a function"
                .to_string()
        ));
        assert!(warnings
            .contains(&"Failed to import memory: memory.tags.map is not a function".to_string()));
        let id = &counts.created_ids[0].0;
        assert_eq!(
            imported_lines(&lines),
            vec![&format!(
                "DEBUG {TARGET} Imported memory memoryId={id} tier=hot"
            )]
        );
    }
}
