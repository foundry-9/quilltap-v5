//! Tier-2 differential test: v4's `CharactersRepository` array / sub-array ops
//! (Phase-2, the store-backed capstone sub-unit 4b).
//!
//! Both sides start from the SAME baked fixture (a character + vault created by
//! v4's real `repos.characters.create`, with one baked systemPrompt, one scenario,
//! one partnerLink), apply the SAME op sequence, then SIX tables are
//! structural-diffed: the main slim `characters` row + the mount-index store tables
//! (`doc_mount_points` / `_folders` / `_files` / `_documents` / `_file_links`). The
//! Rust port drives [`vault_character_arrays`] over two writers; v4 drives the real
//! repository methods (see the oracle).
//!
//! The id-taking prompt/scenario ops carry a `targetName` / `targetTitle`; each
//! side resolves it to the current item's id via `find_by_id` (the read overlay)
//! right before the op — the id is path-derived, so both sides agree.
//!
//! Minted-values remap with ONE shared id-map across all six tables (FKs verify by
//! relationship); timestamps → `<ts>`; the link `chunkCount` diffs EXACTLY since
//! P4.6BK (v5 chunks on write); `doc_mount_chunks` excluded.
//!
//! Banks: addSystemPrompt (default-demote + non-default), updateSystemPrompt
//! (rename → sweep + content), setDefaultSystemPrompt, deleteSystemPrompt (deletes
//! the default → survivor promotion), addScenario / updateScenario / removeScenario,
//! addPartnerLink / removePartnerLink (slim column), and the
//! setFavorite / setControlledBy / setCanBeCarina setters.
//!
//! Generate the oracle output + fixtures (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_CHARARR_MAIN=/tmp/qt-chararr-main.db \
//!   QT_FIXTURE_CHARARR_MOUNT=/tmp/qt-chararr-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-characters-arrays-fixture.ts
//!   QT_FIXTURE_CHARARR_MAIN=/tmp/qt-chararr-main.db \
//!   QT_FIXTURE_CHARARR_MOUNT=/tmp/qt-chararr-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/characters-arrays.ts > /tmp/oracle-chararr.ndjson
//! Run:
//!   QT_ORACLE_CHARARR=/tmp/oracle-chararr.ndjson \
//!   QT_FIXTURE_CHARARR_MAIN=/tmp/qt-chararr-main.db \
//!   QT_FIXTURE_CHARARR_MOUNT=/tmp/qt-chararr-mount.db \
//!     cargo test -p quilltap-harness --test characters_arrays_tier2_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::vault_character_arrays as arr;
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::{Map, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Op {
    op: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    is_default: Option<bool>,
    #[serde(default)]
    title: Option<String>,
    /// [P4.D201 / v4 `baa85e19b`] `setDefaultSystemPrompt` accepts `null` — the
    /// clear-the-default arm — so an ABSENT or explicitly-`null` `targetName`
    /// deserializes to `None` and is passed through rather than resolved.
    #[serde(default)]
    target_name: Option<String>,
    /// [P4.D201] a LITERAL prompt id the character does not have (the refusal arm).
    #[serde(default)]
    prompt_id: Option<String>,
    #[serde(default)]
    target_title: Option<String>,
    /// [P4.D120 / v4 `d25dacc1`] `addScenario`'s optional `archived` flag.
    #[serde(default)]
    archived: Option<bool>,
    #[serde(default)]
    data: Option<Map<String, Value>>,
    #[serde(default)]
    partner_id: Option<String>,
    #[serde(default)]
    value: Option<Value>,
}

struct TableSpec {
    table: &'static str,
    oracle_key: &'static str,
    order_by: &'static str,
    id_columns: &'static [&'static str],
    ts_columns: &'static [&'static str],
    from_mount: bool,
    pin_chunk_count: bool,
}

const TABLES: &[TableSpec] = &[
    TableSpec {
        table: "characters",
        oracle_key: "characters",
        order_by: "name",
        id_columns: &["id", "characterDocumentMountPointId"],
        ts_columns: &["createdAt", "updatedAt"],
        from_mount: false,
        pin_chunk_count: false,
    },
    TableSpec {
        table: "doc_mount_points",
        oracle_key: "points",
        order_by: "name",
        id_columns: &["id"],
        ts_columns: &["createdAt", "updatedAt", "lastScannedAt"],
        from_mount: true,
        pin_chunk_count: false,
    },
    TableSpec {
        table: "doc_mount_folders",
        oracle_key: "folders",
        order_by: "path",
        id_columns: &["id", "parentId", "mountPointId"],
        ts_columns: &["createdAt", "updatedAt"],
        from_mount: true,
        pin_chunk_count: false,
    },
    TableSpec {
        table: "doc_mount_files",
        oracle_key: "files",
        order_by: "sha256",
        id_columns: &["id"],
        ts_columns: &["createdAt", "updatedAt"],
        from_mount: true,
        pin_chunk_count: false,
    },
    TableSpec {
        table: "doc_mount_documents",
        oracle_key: "documents",
        order_by: "contentSha256",
        id_columns: &["id", "fileId"],
        ts_columns: &["createdAt", "updatedAt"],
        from_mount: true,
        pin_chunk_count: false,
    },
    TableSpec {
        table: "doc_mount_file_links",
        oracle_key: "links",
        order_by: "relativePath",
        id_columns: &["id", "fileId", "folderId", "mountPointId"],
        ts_columns: &[
            "lastModified",
            "descriptionUpdatedAt",
            "createdAt",
            "updatedAt",
        ],
        from_mount: true,
        pin_chunk_count: false, // P4.6BK: v5 chunks on write — chunkCount now diffs exactly
    },
];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/characters-arrays-tier2.json")
}

fn normalize_table(dump: &mut Value, spec: &TableSpec, id_map: &mut HashMap<String, String>) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .unwrap_or_else(|| panic!("{}: dump has no rows array", spec.table));

    for row in rows.iter_mut() {
        let obj = row
            .as_object_mut()
            .unwrap_or_else(|| panic!("{}: row is not an object", spec.table));

        for col in spec.id_columns {
            if let Some(Value::String(raw)) = obj.get(*col) {
                let next = format!("ID_{}", id_map.len());
                let token = id_map.entry(raw.clone()).or_insert(next).clone();
                obj.insert((*col).to_string(), Value::String(token));
            }
        }
        for col in spec.ts_columns {
            if obj.get(*col).map(|v| !v.is_null()).unwrap_or(false) {
                obj.insert((*col).to_string(), Value::String("<ts>".to_string()));
            }
        }
        if spec.pin_chunk_count {
            obj.insert("chunkCount".to_string(), Value::String("<cc>".to_string()));
        }
    }
}

fn normalize_all(dumps: &mut [Value]) {
    let mut id_map: HashMap<String, String> = HashMap::new();
    for (i, spec) in TABLES.iter().enumerate() {
        normalize_table(&mut dumps[i], spec, &mut id_map);
    }
}

/// Resolve a `systemPrompts` / `scenarios` item id by its name/title via the read
/// overlay (mirrors the oracle's `findById`-based resolution).
fn resolve_item_id(
    main: &Writer,
    mount: &Writer,
    character_id: &str,
    array_key: &str,
    name_key: &str,
    name_value: &str,
) -> String {
    let character = arr::find_by_id(main.connection(), mount.connection(), character_id)
        .unwrap_or_else(|e| panic!("find_by_id during resolve: {e}"))
        .unwrap_or_else(|| panic!("character {character_id} vanished during resolve"));
    character
        .get(array_key)
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find(|i| i.get(name_key).and_then(Value::as_str) == Some(name_value))
        })
        .and_then(|i| i.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .unwrap_or_else(|| panic!("{array_key} item not found for {name_key}={name_value}"))
}

fn run_op(main: &Writer, mount: &Writer, character_id: &str, op: &Op) {
    let cid = character_id;
    let m = main.connection();
    let mo = mount.connection();
    match op.op.as_str() {
        "addSystemPrompt" => {
            arr::add_system_prompt(
                m,
                mo,
                cid,
                op.name.as_deref().expect("name"),
                op.content.as_deref().expect("content"),
                op.is_default.expect("isDefault"),
            )
            .expect("add_system_prompt");
        }
        "updateSystemPrompt" => {
            let id = resolve_item_id(
                main,
                mount,
                cid,
                "systemPrompts",
                "name",
                op.target_name.as_deref().expect("targetName"),
            );
            arr::update_system_prompt(m, mo, cid, &id, op.data.as_ref().expect("data"))
                .expect("update_system_prompt");
        }
        "setDefaultSystemPrompt" => {
            // [P4.D201 / v4 `baa85e19b`] a `null` / absent `targetName` is the
            // CLEAR arm — passed through as `None`, not resolved to an id.
            let id = op
                .target_name
                .as_deref()
                .map(|name| resolve_item_id(main, mount, cid, "systemPrompts", "name", name));
            // The SILENCE leg of the `System prompt not found` warn (below): a
            // resolved id — or the `None` clear — must not fire it.
            let (accepted, lines) = quilltap_core::test_support::captured_with(|| {
                arr::set_default_system_prompt(m, mo, cid, id.as_deref())
                    .expect("set_default_system_prompt")
            });
            assert!(accepted, "setDefaultSystemPrompt: v5 refused a resolved id");
            assert!(
                !lines.iter().any(|l| l.contains("System prompt not found")),
                "setDefaultSystemPrompt: the miss warn fired on a hit: {lines:?}"
            );
        }
        // [P4.D201 / v4 `baa85e19b`] the refusal arm: a NON-null id the character
        // does not have. v4 warns and returns null having written nothing; v5
        // answers `Ok(false)`. The six-table census proves the nothing; the
        // capture layer pins v4's `logger.warn('System prompt not found',
        // { characterId, promptId })` with both fields (the `baa85e19b` round's
        // §3 review — the order's Tier-1 item 2 asked for the pin and the lane
        // ported the line without one).
        "setDefaultSystemPromptMissing" => {
            let pid = op.prompt_id.as_deref().expect("promptId");
            let (accepted, lines) = quilltap_core::test_support::captured_with(|| {
                arr::set_default_system_prompt(m, mo, cid, Some(pid))
                    .expect("set_default_system_prompt (missing)")
            });
            assert!(
                !accepted,
                "setDefaultSystemPromptMissing: v5 accepted a foreign prompt id"
            );
            assert!(
                lines.iter().any(|l| l.contains("System prompt not found")
                    && l.contains(&format!("characterId={cid}"))
                    && l.contains(&format!("promptId={pid}"))),
                "setDefaultSystemPromptMissing: v4's warn with both fields is missing: {lines:?}"
            );
        }
        "deleteSystemPrompt" => {
            let id = resolve_item_id(
                main,
                mount,
                cid,
                "systemPrompts",
                "name",
                op.target_name.as_deref().expect("targetName"),
            );
            arr::delete_system_prompt(m, mo, cid, &id).expect("delete_system_prompt");
        }
        "addScenario" => {
            arr::add_scenario(
                m,
                mo,
                cid,
                op.title.as_deref().expect("title"),
                op.content.as_deref().expect("content"),
                op.archived,
            )
            .expect("add_scenario");
        }
        "updateScenario" => {
            let id = resolve_item_id(
                main,
                mount,
                cid,
                "scenarios",
                "title",
                op.target_title.as_deref().expect("targetTitle"),
            );
            arr::update_scenario(m, mo, cid, &id, op.data.as_ref().expect("data"))
                .expect("update_scenario");
        }
        "removeScenario" => {
            let id = resolve_item_id(
                main,
                mount,
                cid,
                "scenarios",
                "title",
                op.target_title.as_deref().expect("targetTitle"),
            );
            arr::remove_scenario(m, mo, cid, &id).expect("remove_scenario");
        }
        "addPartnerLink" => {
            arr::add_partner_link(
                m,
                mo,
                cid,
                op.partner_id.as_deref().expect("partnerId"),
                op.is_default.expect("isDefault"),
            )
            .expect("add_partner_link");
        }
        "removePartnerLink" => {
            arr::remove_partner_link(m, mo, cid, op.partner_id.as_deref().expect("partnerId"))
                .expect("remove_partner_link");
        }
        "setFavorite" => {
            arr::set_favorite(
                m,
                mo,
                cid,
                op.value.as_ref().and_then(Value::as_bool).expect("value"),
            )
            .expect("set_favorite");
        }
        "setControlledBy" => {
            arr::set_controlled_by(
                m,
                mo,
                cid,
                op.value.as_ref().and_then(Value::as_str).expect("value"),
            )
            .expect("set_controlled_by");
        }
        "setCanBeCarina" => {
            arr::set_can_be_carina(
                m,
                mo,
                cid,
                op.value.as_ref().and_then(Value::as_bool).expect("value"),
            )
            .expect("set_can_be_carina");
        }
        other => panic!("unknown op: {other}"),
    }
}

#[test]
fn characters_arrays_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CHARARR") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CHARARR to the oracle NDJSON (see header).");
            return;
        }
    };
    let main_fixture = match std::env::var("QT_FIXTURE_CHARARR_MAIN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CHARARR_MAIN to the main fixture .db (header).");
            return;
        }
    };
    let mount_fixture = match std::env::var("QT_FIXTURE_CHARARR_MOUNT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CHARARR_MOUNT to the mount fixture .db (header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle: Value = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle dump");

    // Fresh copies so the shared baked fixtures stay pristine.
    let pid = std::process::id();
    let main_work = std::env::temp_dir().join(format!("qt-chararr-main-rust-{pid}.db"));
    let mount_work = std::env::temp_dir().join(format!("qt-chararr-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));

    let main = Writer::open_writable(&main_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open main: {e}"));
    let mount = Writer::open_writable(&mount_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open mount: {e}"));

    // Read the baked character id (both sides target the same one).
    let character_id: String = main
        .connection()
        .query_row("SELECT id FROM characters LIMIT 1", [], |row| {
            row.get::<_, String>(0)
        })
        .expect("read baked character id");

    // [P4.D201 / v4 `baa85e19b`] The per-op default-prompt trail.
    //
    // The six-table census below is a FINAL-STATE diff, so an op whose effect a
    // later op overwrites is INVISIBLE to it — MEASURED: inserting this round's
    // three new system-prompt arms left the dump the exact size it already was,
    // because the sequence still ends with a delete that re-heals the column.
    // The lockstep is a claim about what EACH write leaves behind, so it needs a
    // per-op comparand. The prompt ids are path-derived from the SHARED fixture,
    // so both sides mint the same ones and the trail compares EXACTLY.
    let snapshot = |op_name: &str| -> Value {
        let column: Option<String> = main
            .connection()
            .query_row(
                "SELECT defaultSystemPromptId FROM characters WHERE id = ?1",
                [&character_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .expect("read defaultSystemPromptId");
        let character = arr::find_by_id(main.connection(), mount.connection(), &character_id)
            .expect("find_by_id during snapshot")
            .expect("character vanished during snapshot");
        let prompts: Vec<Value> = character
            .get("systemPrompts")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(|p| {
                        serde_json::json!([
                            p.get("name").and_then(Value::as_str).unwrap_or_default(),
                            p.get("isDefault").and_then(Value::as_bool).unwrap_or(false),
                        ])
                    })
                    .collect()
            })
            .unwrap_or_default();
        serde_json::json!({ "op": op_name, "column": column, "prompts": prompts })
    };

    let mut trail: Vec<Value> = vec![snapshot("<initial>")];
    for op in &spec.ops {
        run_op(&main, &mount, &character_id, op);
        trail.push(snapshot(&op.op));
    }

    let want_trail = oracle
        .get("defaultColumnTrail")
        .unwrap_or_else(|| panic!("oracle carries no defaultColumnTrail — stale NDJSON?"));
    assert_eq!(
        &Value::Array(trail.clone()),
        want_trail,
        "the per-op default-prompt trail diverged"
    );
    // The shapes the corpus must actually contain, so a future trimmed spec
    // cannot go green having stopped asking the lockstep's questions.
    assert!(
        trail
            .iter()
            .any(|e| e["op"] == "setDefaultSystemPromptMissing"),
        "the trail asks no refusal arm"
    );
    assert!(
        trail
            .iter()
            .any(|e| e["op"] == "setDefaultSystemPrompt" && e["column"].is_null()),
        "the trail asks no CLEAR arm (a setDefaultSystemPrompt leaving the column null)"
    );
    assert!(
        trail
            .iter()
            .any(|e| e["op"] == "addSystemPrompt" && e["column"].is_null()),
        "the trail asks no transient-id arm (an addSystemPrompt leaving the column null)"
    );
    assert!(
        trail
            .iter()
            .any(|e| e["op"] == "updateSystemPrompt" && !e["column"].is_null()),
        "the trail asks no promotion-through-UPDATE arm"
    );

    let mut got: Vec<Value> = TABLES
        .iter()
        .map(|s| {
            let w = if s.from_mount { &mount } else { &main };
            w.dump_table_json(s.table, s.order_by)
                .unwrap_or_else(|e| panic!("dump {}: {e}", s.table))
        })
        .collect();
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);

    let mut want: Vec<Value> = TABLES
        .iter()
        .map(|s| {
            oracle
                .get(s.oracle_key)
                .cloned()
                .unwrap_or_else(|| panic!("oracle missing dump for {}", s.oracle_key))
        })
        .collect();

    normalize_all(&mut got);
    normalize_all(&mut want);

    for (i, s) in TABLES.iter().enumerate() {
        assert_eq!(got[i]["table"], want[i]["table"], "{}: table name", s.table);
        assert_eq!(
            got[i]["columns"], want[i]["columns"],
            "{}: column set / order",
            s.table
        );
        assert_eq!(
            got[i]["rows"], want[i]["rows"],
            "{}: remapped row state diverged\n  rust:   {}\n  oracle: {}",
            s.table, got[i]["rows"], want[i]["rows"]
        );
    }

    eprintln!(
        "OK: characters arrays tier-2 matched oracle (6 tables, 2 DBs, {} ops, {} trail entries).",
        spec.ops.len(),
        trail.len()
    );
}
