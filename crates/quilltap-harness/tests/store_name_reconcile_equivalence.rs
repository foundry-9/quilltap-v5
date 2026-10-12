//! P4.D274 unit 4 — the headline tier-2 differential: v4's REAL
//! `reconcileStoreNames` (`lib/mount-index/reconcile-store-names.ts`,
//! `ed8b15b50`, bug 186) against `quilltap_core::db::store_name_reconcile`,
//! scenario by scenario of `harness/oracle/fixtures/store-name-reconcile-
//! spec.json`: the BEFORE / AFTER `doc_mount_points` dumps (`updatedAt` →
//! `<now>` iff it moved), the result object (or the error text), and the
//! captured lines in order (level, message, fields in v4's key order).
//!
//! **The ping-pong proof (cross-run).** On an instance v4 and v5 share, the two
//! must plan the SAME renames or each boot undoes the other. So:
//!   - stage 1 also runs v5's reconcile over each v4 AFTER pair
//!     (`QT_SNR_V4_AFTER_DIR`) and asserts it renames NOTHING; and writes v5's
//!     own AFTER pairs to `QT_SNR_V5_AFTER_DIR`;
//!   - stage 2 (`QT_ORACLE_STORE_NAME_RECONCILE_CROSS`) reads v4's reconcile
//!     over those v5 pairs and asserts `renamed: []` for every scenario —
//!     except the `RECONCILE_READ_FAILURE_STRICT` rows, where v5 skipped and v4
//!     replays exactly its own stage-1 plan (v5 lags v4; it never undoes it).
//!
//! **`RECONCILE_READ_FAILURE_STRICT`** (R-A arm 2), both ways in
//! [`READ_FAILURE_STRICT`]: when a whole-query read fails, v4's `_findAll`
//! answers `[]` — a failed characters read RETIRES every vault-shaped
//! `character` store — while v5 logs v4's same ERROR, then its own skip WARN,
//! answers `ReadFailed` and writes nothing. A v4 convergence trips the table.
//!
//! Regenerate + run (the pinned v4 worktree, Node 24, TZ=UTC; four stages):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   rm -rf /tmp/qt-store-name-reconcile ; rm -f /tmp/oracle-store-name-reconcile.ndjson /tmp/oracle-store-name-reconcile-cross.ndjson
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_SNR_DIR=/tmp/qt-store-name-reconcile $N/npx tsx $V5W/harness/oracle/fixtures/build-store-name-reconcile-fixture.ts
//!   TZ=UTC QT_FIXTURE_SNR_DIR=/tmp/qt-store-name-reconcile QT_SNR_OUT_DIR=/tmp/qt-store-name-reconcile/v4-after $N/npx tsx $V5W/harness/oracle/cases/store-name-reconcile.ts > /tmp/oracle-store-name-reconcile.ndjson
//!   cd $V5W
//!   QT_ORACLE_STORE_NAME_RECONCILE=/tmp/oracle-store-name-reconcile.ndjson QT_FIXTURE_SNR_DIR=/tmp/qt-store-name-reconcile QT_SNR_V4_AFTER_DIR=/tmp/qt-store-name-reconcile/v4-after QT_SNR_V5_AFTER_DIR=/tmp/qt-store-name-reconcile/v5-after cargo test -p quilltap-harness --test store_name_reconcile_equivalence -- --nocapture
//!   cd ~/source/quilltap-server
//!   TZ=UTC QT_FIXTURE_SNR_DIR=/tmp/qt-store-name-reconcile QT_SNR_CROSS_DIR=/tmp/qt-store-name-reconcile/v5-after $N/npx tsx $V5W/harness/oracle/cases/store-name-reconcile.ts > /tmp/oracle-store-name-reconcile-cross.ndjson
//!   cd $V5W
//!   QT_ORACLE_STORE_NAME_RECONCILE=/tmp/oracle-store-name-reconcile.ndjson QT_ORACLE_STORE_NAME_RECONCILE_CROSS=/tmp/oracle-store-name-reconcile-cross.ndjson QT_FIXTURE_SNR_DIR=/tmp/qt-store-name-reconcile QT_SNR_V4_AFTER_DIR=/tmp/qt-store-name-reconcile/v4-after QT_SNR_V5_AFTER_DIR=/tmp/qt-store-name-reconcile/v5-after cargo test -p quilltap-harness --test store_name_reconcile_equivalence -- --nocapture

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use quilltap_core::db::character_vault::{create_character, delete_character_and_reconcile};
use quilltap_core::db::characters::CharacterCreate;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::store_name_reconcile::{reconcile_store_names, StoreNameReconcileResult};
use quilltap_core::db::vault_character_update::update_character;
use quilltap_core::db::vault_character_write::CharacterVaultWriteInput;
use quilltap_core::db::Writer;
use quilltap_core::services::delete_all::{delete_user_data_on_writer, DeleteUserDataOptions};
use quilltap_core::services::file_storage::PassthroughPixelCodec;
use quilltap_core::services::quilltap_import::{
    execute_import, parse_export_file, ConflictStrategy, ImportOptions, PreserveIdsMode,
};
use quilltap_core::store_names::store_name_timestamp;
use rusqlite::{params, Connection};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    pepper: String,
    #[serde(rename = "userId")]
    user_id: String,
    scenarios: Vec<Scenario>,
}

#[derive(Deserialize, Clone)]
struct Op {
    kind: String,
    alias: Option<String>,
    name: Option<String>,
    /// `import`: the conflict strategy and the payload's `mountPoints`.
    strategy: Option<String>,
    #[serde(rename = "mountPoints")]
    mount_points: Option<Value>,
}

#[derive(Deserialize)]
struct Scenario {
    name: String,
    trigger: String,
    plant: Option<String>,
    /// Unit 5: the character hooks (real create / rename / delete, the wipe)
    /// instead of a direct reconcile.
    ops: Option<Vec<Op>>,
    stores: Vec<StoreRow>,
    characters: Vec<CharRow>,
}

#[derive(Deserialize)]
struct StoreRow {
    id: String,
    name: String,
    #[serde(rename = "storeType")]
    store_type: Option<String>,
    #[serde(rename = "createdAt")]
    created_at: String,
}

#[derive(Deserialize)]
struct CharRow {
    id: String,
    name: String,
    #[serde(rename = "characterDocumentMountPointId")]
    vault: Option<String>,
    #[serde(rename = "createdAt")]
    created_at: String,
    #[serde(rename = "archivedAt")]
    archived_at: Option<String>,
}

/// `RECONCILE_READ_FAILURE_STRICT`, both ways: `(scenario, v4's rename count,
/// v4's first line, v5's lines)`. v4 answers the failed read with `[]` and
/// plans on; v5 logs v4's ERROR and skips.
const READ_FAILURE_STRICT: &[(&str, usize, &str, &[&str])] = &[
    (
        "e2-characters-read-fails",
        2,
        "ERROR Error finding all entities collection=characters error=malformed JSON",
        &[
            "ERROR Error finding all entities collection=characters error=malformed JSON",
            "WARN Store-name reconcile skipped: a store or character read failed module=mount-index:reconcile-store-names trigger=boot collection=characters error=malformed JSON",
        ],
    ),
    (
        "e2-stores-read-fails",
        0,
        "ERROR Error finding all entities collection=doc_mount_points error=malformed JSON",
        &[
            "ERROR Error finding all entities collection=doc_mount_points error=malformed JSON",
            "WARN Store-name reconcile skipped: a store or character read failed module=mount-index:reconcile-store-names trigger=boot collection=doc_mount_points error=malformed JSON",
        ],
    ),
];

fn spec() -> Spec {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/store-name-reconcile-spec.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("read spec")).expect("parse spec")
}

/// v4's captured line → `LEVEL message k=v …` (key order kept).
fn render_v4_line(l: &Value) -> String {
    let mut out = format!(
        "{} {}",
        l["level"].as_str().unwrap().to_uppercase(),
        l["message"].as_str().unwrap()
    );
    for (k, v) in l["fields"].as_object().unwrap() {
        let rendered = match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        out.push_str(&format!(" {k}={rendered}"));
    }
    out
}

/// `RECONCILE_READ_FAILURE_STRICT`'s import half, both ways: inside the
/// `.qtap` import's strict scope a failed characters read throws out of v4's
/// reconcile (its ERROR gains `strictFailures=true`) and v4 pushes the warning;
/// v5 logs the same ERROR, then its skip WARN, and pushes nothing.
/// `(scenario, v4's warning, v5's lines)`.
const IMPORT_READ_FAILURE_STRICT: &[(&str, &str, &[&str])] = &[(
    "q-import-strict-characters-read-fails",
    "Failed to tidy document-store names after import: malformed JSON",
    &[
        "ERROR Error finding all entities collection=characters error=malformed JSON strictFailures=true",
        "WARN Store-name reconcile skipped: a store or character read failed module=mount-index:reconcile-store-names trigger=qtap-import collection=characters error=malformed JSON",
    ],
)];

/// The messages both sides compare (the oracle's `CAPTURED` set); v5's
/// capture keeps every line, so it is filtered to these.
const CAPTURED: &[&str] = &[
    "Store-name reconcile planned",
    "Renamed document store to its rightful name",
    "Store-name reconcile skipped: mount-index database unavailable",
    "Store-name reconcile skipped: a store or character read failed",
    "Store-name reconcile after a character change failed",
    "Data validation failed",
    "Safe validation failed",
    "Error finding all entities",
    "Mount-index case repair: renamed document store whose name collided with a peer except for casing",
    "Mount-index: created the unique document-store name index",
    "Character created",
    "Character deleted",
];

/// v5's captured lines with the tracing target dropped, kept to [`CAPTURED`].
fn v5_lines(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let (level, rest) = line.split_once(' ')?;
            let (_target, rest) = rest.split_once(' ')?;
            CAPTURED
                .iter()
                .any(|m| rest.starts_with(m))
                .then(|| format!("{level} {rest}"))
        })
        .collect()
}

/// Replace every uuid-shaped token outside `known` with `<id-N>` (N by first
/// appearance in `order`) and every compact store stamp outside `stamps` with
/// `<stamp>`: the ids a create MINTS and the retired names built from a minted
/// vault's `createdAt` differ run to run, on both sides.
fn normalize(
    v: &Value,
    known: &HashSet<String>,
    stamps: &HashSet<String>,
    order: &mut Vec<String>,
) -> Value {
    match v {
        Value::String(s) => Value::String(normalize_str(s, known, stamps, order)),
        Value::Array(a) => Value::Array(
            a.iter()
                .map(|x| normalize(x, known, stamps, order))
                .collect(),
        ),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, x)| (k.clone(), normalize(x, known, stamps, order)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn normalize_str(
    s: &str,
    known: &HashSet<String>,
    stamps: &HashSet<String>,
    order: &mut Vec<String>,
) -> String {
    let b = s.as_bytes();
    let is_hex = |c: u8| c.is_ascii_digit() || (b'a'..=b'f').contains(&c);
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        if i + 36 <= b.len() && s.is_char_boundary(i) && s.is_char_boundary(i + 36) {
            let uuid = b[i..i + 36].iter().enumerate().all(|(j, &c)| {
                if [8, 13, 18, 23].contains(&j) {
                    c == b'-'
                } else {
                    is_hex(c)
                }
            });
            if uuid {
                let id = &s[i..i + 36];
                if known.contains(id) {
                    out.push_str(id);
                } else {
                    let n = order.iter().position(|x| x == id).unwrap_or_else(|| {
                        order.push(id.to_string());
                        order.len() - 1
                    });
                    out.push_str(&format!("<id-{n}>"));
                }
                i += 36;
                continue;
            }
        }
        if i + 18 <= b.len() && s.is_char_boundary(i) && s.is_char_boundary(i + 18) {
            let stamp = b[i..i + 18].iter().enumerate().all(|(j, &x)| match j {
                4 | 7 => x == b'-',
                10 => x == b'T',
                17 => x == b'Z',
                _ => x.is_ascii_digit(),
            });
            if stamp {
                let t = &s[i..i + 18];
                out.push_str(if stamps.contains(t) { t } else { "<stamp>" });
                i += 18;
                continue;
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// The oracle's dump: planted rows by id, then the rows a create MINTED in
/// insertion order (their stamps `<now>`).
fn dump_stores(mount: &Connection, planted: &HashMap<String, String>) -> Value {
    let mut stmt = mount
        .prepare(
            "SELECT id, name, storeType, createdAt, updatedAt FROM doc_mount_points ORDER BY rowid",
        )
        .unwrap();
    let rows: Vec<Value> = stmt
        .query_map([], |r| {
            let id: String = r.get(0)?;
            let updated: String = r.get(4)?;
            let known = planted.contains_key(&id);
            let moved = planted.get(&id) != Some(&updated);
            Ok(json!({
                "id": id,
                "name": r.get::<_, String>(1)?,
                "storeType": r.get::<_, Option<String>>(2)?,
                "createdAt": if known { json!(r.get::<_, String>(3)?) } else { json!("<now>") },
                "updatedAt": if moved { json!("<now>") } else { json!(updated) },
            }))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    let (mut known, minted): (Vec<Value>, Vec<Value>) = rows
        .into_iter()
        .partition(|r| planted.contains_key(r["id"].as_str().unwrap()));
    known.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    Value::Array(known.into_iter().chain(minted).collect())
}

fn character_rows(main: &Connection) -> Value {
    let mut stmt = main
        .prepare("SELECT id, name, characterDocumentMountPointId FROM characters ORDER BY rowid")
        .unwrap();
    let rows = stmt
        .query_map([], |r| {
            Ok(json!({
                "id": r.get::<_, String>(0)?,
                "name": r.get::<_, String>(1)?,
                "characterDocumentMountPointId": r.get::<_, Option<String>>(2)?,
            }))
        })
        .unwrap();
    Value::Array(rows.map(|r| r.unwrap()).collect())
}

fn slim(user_id: &str, name: &str) -> CharacterCreate {
    CharacterCreate {
        user_id: user_id.to_string(),
        name: name.to_string(),
        default_image_id: None,
        default_connection_profile_id: None,
        default_partner_id: None,
        default_roleplay_template_id: None,
        default_image_profile_id: None,
        silly_tavern_data: None,
        is_favorite: false,
        npc: false,
        controlled_by: "llm".into(),
        default_agent_mode_enabled: None,
        default_help_tools_enabled: None,
        default_timestamp_config: None,
        default_scenario_id: None,
        default_system_prompt_id: None,
        character_document_mount_point_id: None,
        can_dress_themselves: None,
        can_create_outfits: None,
        system_transparency: None,
        core_whisper_enabled: None,
        can_be_carina: None,
        partner_links: Vec::new(),
        tags: Vec::new(),
        avatar_overrides: Vec::new(),
    }
}

fn result_json(r: &StoreNameReconcileResult) -> Value {
    json!({
        "renamed": r.renamed.iter().map(|x| x.to_json()).collect::<Vec<_>>(),
        "skippedReason": r.skipped_reason.map(|s| s.as_str()),
    })
}

struct Run {
    before: Value,
    after: Value,
    result: Value,
    error: Value,
    lines: Vec<String>,
    /// Ops scenarios: alias → minted character id (alias order), each op's
    /// outcome, and the character rows after.
    ids: Value,
    op_results: Value,
    characters: Value,
}

/// Copy the pair under `work`, plant the scenario when `plant`, run it (a
/// reconcile on the caller thread, so `captured_with` sees every line; or, for
/// an `ops` scenario when `plant`, the character hooks), and dump.
fn run_pair(
    spec: &Spec,
    sc: &Scenario,
    main_src: &Path,
    mount_src: &Path,
    work: &Path,
    plant: bool,
) -> (Run, PathBuf, PathBuf) {
    std::fs::create_dir_all(work).unwrap();
    let main_path = work.join(format!("{}-main.db", sc.name));
    let mount_path = work.join(format!("{}-mount.db", sc.name));
    std::fs::copy(main_src, &main_path).unwrap();
    std::fs::copy(mount_src, &mount_path).unwrap();
    let degraded = sc.plant.as_deref() == Some("mount-index-degraded");

    let main = Writer::open_writable(&main_path, &spec.pepper).expect("open main");
    let mount =
        (!degraded).then(|| Writer::open_writable(&mount_path, &spec.pepper).expect("open mount"));
    let mut planted = HashMap::new();
    if plant {
        if let Some(m) = &mount {
            for s in &sc.stores {
                m.connection()
                    .execute(
                        "INSERT INTO doc_mount_points (id, name, storeType, createdAt, updatedAt) VALUES (?1, ?2, ?3, ?4, ?4)",
                        params![s.id, s.name, s.store_type, s.created_at],
                    )
                    .unwrap();
            }
        }
        for c in &sc.characters {
            main.connection()
                .execute(
                    "INSERT INTO characters (id, userId, name, characterDocumentMountPointId, archivedAt, createdAt, updatedAt) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                    params![c.id, spec.user_id, c.name, c.vault, c.archived_at, c.created_at],
                )
                .unwrap();
        }
        let boom = "ADD COLUMN qt_boom TEXT GENERATED ALWAYS AS (json('not json')) VIRTUAL";
        match sc.plant.as_deref() {
            Some("characters-read-fails") => main
                .connection()
                .execute_batch(&format!("ALTER TABLE characters {boom}"))
                .unwrap(),
            Some("stores-read-fails") => mount
                .as_ref()
                .unwrap()
                .connection()
                .execute_batch(&format!("ALTER TABLE doc_mount_points {boom}"))
                .unwrap(),
            _ => {}
        }
    }
    if let Some(m) = &mount {
        let mut stmt = m
            .connection()
            .prepare("SELECT id, updatedAt FROM doc_mount_points")
            .unwrap();
        for r in stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .unwrap()
        {
            let (id, u) = r.unwrap();
            planted.insert(id, u);
        }
    }

    let before = mount
        .as_ref()
        .map(|m| dump_stores(m.connection(), &planted))
        .unwrap_or(json!([]));
    let Some(ops) = sc.ops.as_ref().filter(|_| plant) else {
        let (outcome, lines) = quilltap_core::test_support::captured_with(|| {
            reconcile_store_names(
                main.connection(),
                mount.as_ref().map(|m| m.connection()),
                &sc.trigger,
            )
        });
        let after = mount
            .as_ref()
            .map(|m| dump_stores(m.connection(), &planted))
            .unwrap_or(json!([]));
        let (result, error) = match outcome {
            Ok(r) => (result_json(&r), Value::Null),
            Err(e) => (
                Value::Null,
                json!(quilltap_core::db::fallback::error_text(&e)),
            ),
        };
        drop(main);
        drop(mount);
        return (
            Run {
                before,
                after,
                result,
                error,
                lines: v5_lines(&lines),
                ids: Value::Null,
                op_results: Value::Null,
                characters: Value::Null,
            },
            main_path,
            mount_path,
        );
    };

    // The hooks: every op on the caller thread, except the wipe, which runs
    // on the writer thread of a `Db` over the same files (captured there).
    let mut ids = serde_json::Map::new();
    let mut op_results = Vec::new();
    let mut lines = Vec::new();
    let mut main = Some(main);
    let mut mount = mount;
    for op in ops {
        let alias = op.alias.clone().unwrap_or_default();
        let id_of = |ids: &serde_json::Map<String, Value>| {
            ids.get(&alias)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        if op.kind == "import" {
            let m = main.as_ref().unwrap().connection();
            let mi = mount.as_ref().unwrap().connection();
            let payload = json!({
                "manifest": {
                    "format": "quilltap-export",
                    "version": "1.0",
                    "exportType": "document-stores",
                    "createdAt": "2026-01-01T00:00:00.000Z",
                    "appVersion": "4.10.0",
                    "settings": { "includeMemories": false, "scope": "all" },
                    "counts": { "documentStores": op.mount_points.as_ref().and_then(Value::as_array).map(Vec::len).unwrap_or(0) }
                },
                "data": { "mountPoints": op.mount_points.clone().unwrap_or(json!([])), "documents": [], "blobs": [] }
            });
            let export = parse_export_file(&payload.to_string()).expect("parse payload");
            let options = ImportOptions {
                conflict_strategy: match op.strategy.as_deref() {
                    Some("overwrite") => ConflictStrategy::Overwrite,
                    Some("duplicate") => ConflictStrategy::Duplicate,
                    _ => ConflictStrategy::Skip,
                },
                include_memories: false,
                include_related_entities: false,
                selected_ids: None,
                preserve_ids: false,
                preserve_ids_mode: PreserveIdsMode::RefuseOnCollision,
            };
            let (result, captured) = quilltap_core::test_support::captured_with(|| {
                execute_import(
                    m,
                    mi,
                    &spec.user_id,
                    &export,
                    &options,
                    &PassthroughPixelCodec {
                        width: 1,
                        height: 1,
                    },
                )
            });
            lines.extend(captured);
            let r = result.expect("execute_import");
            op_results.push(json!({"op": "import", "success": r.success, "warnings": r.warnings}));
            continue;
        }
        if op.kind == "wipe" {
            drop(main.take());
            drop(mount.take());
            let db = Db::open(
                DbPaths {
                    main: main_path.clone(),
                    mount_index: Some(mount_path.clone()),
                    llm_logs: None,
                },
                &spec.pepper,
            )
            .expect("open Db");
            let user = spec.user_id.clone();
            let rt = tokio::runtime::Runtime::new().unwrap();
            let (outcome, wiped) = rt
                .block_on(db.write(move |ws| {
                    Ok(quilltap_core::test_support::global_capture::capture(|| {
                        delete_user_data_on_writer(
                            ws,
                            &user,
                            DeleteUserDataOptions {
                                keep_archived_character_bundles: None,
                            },
                        )
                    }))
                }))
                .expect("writer");
            drop(db);
            drop(rt);
            lines.extend(wiped);
            op_results.push(match outcome {
                Ok(()) => json!({"op": "wipe", "ok": true}),
                Err(e) => json!({"op": "wipe", "error": e.to_string()}),
            });
            main = Some(Writer::open_writable(&main_path, &spec.pepper).expect("reopen main"));
            mount = Some(Writer::open_writable(&mount_path, &spec.pepper).expect("reopen mount"));
            continue;
        }
        let m = main.as_ref().unwrap().connection();
        let mi = mount.as_ref().unwrap().connection();
        let kind = op.kind.as_str();
        let (result, captured) = quilltap_core::test_support::captured_with(|| match kind {
            "create" => create_character(
                m,
                mi,
                &slim(&spec.user_id, op.name.as_deref().unwrap()),
                &CharacterVaultWriteInput::default(),
            )
            .map(|id| json!(id))
            .map_err(|e| e.to_string()),
            "rename" => {
                let mut patch = serde_json::Map::new();
                patch.insert("name".into(), json!(op.name.clone().unwrap()));
                update_character(m, mi, &id_of(&ids), &patch)
                    .map(|_| json!(true))
                    .map_err(|e| e.to_string())
            }
            "archive" => m
                .execute(
                    "UPDATE characters SET archivedAt = '2026-05-01T00:00:00.000Z' WHERE id = ?1",
                    [id_of(&ids)],
                )
                .map(|_| json!(true))
                .map_err(|e| e.to_string()),
            "delete" => delete_character_and_reconcile(m, Some(mi), &id_of(&ids))
                .map(|d| json!(d))
                .map_err(|e| e.to_string()),
            other => panic!("unknown op {other}"),
        });
        lines.extend(captured);
        op_results.push(match result {
            Ok(v) if kind == "create" => {
                ids.insert(alias.clone(), v);
                json!({"op": kind, "alias": alias, "ok": true})
            }
            Ok(v) if kind == "delete" => json!({"op": kind, "alias": alias, "ok": v}),
            Ok(_) => json!({"op": kind, "alias": alias, "ok": true}),
            Err(e) => json!({"op": kind, "alias": alias, "error": e}),
        });
    }
    let main = main.unwrap();
    let mount = mount.unwrap();
    let after = dump_stores(mount.connection(), &planted);
    let characters = character_rows(main.connection());
    drop(main);
    drop(mount);
    (
        Run {
            before,
            after,
            result: Value::Null,
            error: Value::Null,
            lines: v5_lines(&lines),
            ids: Value::Object(ids),
            op_results: Value::Array(op_results),
            characters,
        },
        main_path,
        mount_path,
    )
}

fn oracle_lines(path: &str) -> HashMap<String, Value> {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    assert!(!text.trim().is_empty(), "the oracle file {path} is EMPTY");
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Value = serde_json::from_str(l).expect("oracle line");
            (v["scenario"].as_str().unwrap().to_string(), v)
        })
        .collect()
}

#[test]
fn store_name_reconcile_matches_oracle() {
    let (Ok(oracle_path), Ok(base_dir)) = (
        std::env::var("QT_ORACLE_STORE_NAME_RECONCILE"),
        std::env::var("QT_FIXTURE_SNR_DIR"),
    ) else {
        eprintln!("SKIP: set QT_ORACLE_STORE_NAME_RECONCILE and QT_FIXTURE_SNR_DIR (see header).");
        return;
    };
    // The wipe's lines are captured on the writer thread (`global_capture`),
    // so the global rig owns this binary from the first callsite on.
    quilltap_core::test_support::global_capture::install();
    let spec = spec();
    let oracle = oracle_lines(&oracle_path);
    let base = PathBuf::from(&base_dir);
    let work = tempfile::Builder::new()
        .prefix("qt-snr-rust-")
        .tempdir()
        .unwrap();
    let v4_after = std::env::var("QT_SNR_V4_AFTER_DIR").ok().map(PathBuf::from);
    let v5_after = std::env::var("QT_SNR_V5_AFTER_DIR").ok().map(PathBuf::from);
    if let Some(d) = &v5_after {
        std::fs::create_dir_all(d).unwrap();
    }

    let mut failures = Vec::new();
    let mut checked = 0usize;
    let mut cross_checked = 0usize;
    for sc in &spec.scenarios {
        let want = oracle
            .get(&sc.name)
            .unwrap_or_else(|| panic!("no oracle line for {}", sc.name));
        let (run, main_after, mount_after) = run_pair(
            &spec,
            sc,
            &base.join("base-main.db"),
            &base.join("base-mount.db"),
            &work.path().join("stage1"),
            true,
        );
        checked += 1;
        let want_lines: Vec<String> = want["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(render_v4_line)
            .collect();

        if let Some((_, v4_renames, v4_first, v5_lines)) =
            READ_FAILURE_STRICT.iter().find(|(n, ..)| *n == sc.name)
        {
            // v4's side, as recorded.
            assert_eq!(
                want["result"]["renamed"].as_array().map(Vec::len),
                Some(*v4_renames),
                "RECONCILE_READ_FAILURE_STRICT {}: v4's rename count moved — v4 converged? re-measure",
                sc.name
            );
            assert_eq!(
                want_lines.first().map(String::as_str),
                Some(*v4_first),
                "{}",
                sc.name
            );
            // v5's side: v4's ERROR, the skip WARN, nothing written.
            assert_eq!(run.lines, *v5_lines, "{}: v5 lines", sc.name);
            assert_eq!(
                run.result,
                json!({"renamed": [], "skippedReason": "read-failed"}),
                "{}",
                sc.name
            );
            assert_eq!(
                run.before, run.after,
                "{}: v5 wrote on a failed read",
                sc.name
            );
        } else if let Some((_, v4_warning, v5_lines)) = IMPORT_READ_FAILURE_STRICT
            .iter()
            .find(|(n, ..)| *n == sc.name)
        {
            // v4's side, as recorded: the warning, and the ERROR first.
            assert_eq!(
                want["opResults"][0]["warnings"],
                json!([v4_warning]),
                "IMPORT_READ_FAILURE_STRICT {}: v4's warning moved — v4 converged? re-measure",
                sc.name
            );
            assert_eq!(
                want_lines.first().map(String::as_str),
                v5_lines.first().copied(),
                "{}: v4's ERROR",
                sc.name
            );
            // v5's side: the ERROR + the skip WARN, no warning, nothing renamed.
            assert_eq!(run.lines, *v5_lines, "{}: v5 lines", sc.name);
            assert_eq!(run.op_results[0]["warnings"], json!([]), "{}", sc.name);
            assert_eq!(run.after, want["after"], "{}: the stores", sc.name);
        } else if sc.ops.is_some() {
            // Minted ids and stamps normalized, both sides, by first appearance
            // in the same traversal.
            let mut known: HashSet<String> = sc.stores.iter().map(|s| s.id.clone()).collect();
            known.extend(sc.characters.iter().map(|c| c.id.clone()));
            known.insert(spec.user_id.clone());
            let stamps: HashSet<String> = sc
                .stores
                .iter()
                .map(|s| store_name_timestamp(&s.created_at))
                .collect();
            let shape = |ids: &Value, after: &Value, chars: &Value, lines: Value, ops: &Value| json!({"ids": ids, "after": after, "characters": chars, "lines": lines, "opResults": ops});
            let got = normalize(
                &shape(
                    &run.ids,
                    &run.after,
                    &run.characters,
                    json!(run.lines),
                    &run.op_results,
                ),
                &known,
                &stamps,
                &mut Vec::new(),
            );
            let exp = normalize(
                &shape(
                    &want["ids"],
                    &want["after"],
                    &want["characters"],
                    json!(want_lines),
                    &want["opResults"],
                ),
                &known,
                &stamps,
                &mut Vec::new(),
            );
            if serde_json::to_string(&got).unwrap() != serde_json::to_string(&exp).unwrap() {
                failures.push(format!(
                    "{} (ops):\n  v5 {}\n  v4 {}",
                    sc.name,
                    serde_json::to_string_pretty(&got).unwrap(),
                    serde_json::to_string_pretty(&exp).unwrap()
                ));
            }
        } else {
            for (label, got, exp) in [
                ("before", &run.before, &want["before"]),
                ("after", &run.after, &want["after"]),
                ("result", &run.result, &want["result"]),
                ("error", &run.error, &want["error"]),
            ] {
                if serde_json::to_string(got).unwrap() != serde_json::to_string(exp).unwrap() {
                    failures.push(format!("{} {label}:\n  v5 {got}\n  v4 {exp}", sc.name));
                }
            }
            if run.lines != want_lines {
                failures.push(format!(
                    "{} lines:\n  v5 {:#?}\n  v4 {:#?}",
                    sc.name, run.lines, want_lines
                ));
            }
        }

        if let Some(d) = &v5_after {
            if sc.plant.as_deref() != Some("mount-index-degraded") {
                std::fs::copy(&main_after, d.join(format!("{}-main.db", sc.name))).unwrap();
                std::fs::copy(&mount_after, d.join(format!("{}-mount.db", sc.name))).unwrap();
            }
        }

        // Cross-run, v4 → v5: v5 over v4's reconciled pair renames nothing.
        if let Some(d) = &v4_after {
            let m = d.join(format!("{}-main.db", sc.name));
            if m.exists() {
                let (cross, ..) = run_pair(
                    &spec,
                    sc,
                    &m,
                    &d.join(format!("{}-mount.db", sc.name)),
                    &work.path().join("cross"),
                    false,
                );
                cross_checked += 1;
                let renamed = cross.result["renamed"]
                    .as_array()
                    .map(Vec::len)
                    .unwrap_or(0);
                if renamed != 0 || cross.before != cross.after {
                    failures.push(format!(
                        "{} cross v4→v5: v5 renamed {renamed} on v4's reconciled pair: {}",
                        sc.name, cross.result
                    ));
                }
            }
        }
    }

    // Stage 2, v5 → v4: v4 over v5's reconciled pairs renames nothing.
    let mut v4_cross = 0usize;
    if let Ok(cross_path) = std::env::var("QT_ORACLE_STORE_NAME_RECONCILE_CROSS") {
        for (name, v) in oracle_lines(&cross_path) {
            v4_cross += 1;
            let renamed = v["result"]["renamed"].as_array().map(Vec::len).unwrap_or(0);
            if READ_FAILURE_STRICT.iter().any(|(n, ..)| *n == name) {
                // v5 skipped where v4 planned, so v4 makes exactly its own
                // stage-1 renames over v5's (untouched) pair: v5 LAGS v4, it
                // never undoes v4's names — the divergence's safety argument.
                assert_eq!(
                    v["result"]["renamed"], oracle[&name]["result"]["renamed"],
                    "RECONCILE_READ_FAILURE_STRICT {name}: v4 over v5's pair must replay its own plan"
                );
                continue;
            }
            if IMPORT_READ_FAILURE_STRICT.iter().any(|(n, ..)| *n == name) {
                // Outside the import's strict scope v4's failed characters read
                // answers `[]` and its reconcile RETIRES the unlinked vaults v5
                // left alone — the same divergence, lagging: only retirements.
                let all_retired = v["result"]["renamed"].as_array().is_some_and(|r| {
                    !r.is_empty() && r.iter().all(|x| x["reason"] == "retired-vault")
                });
                assert!(
                    all_retired,
                    "IMPORT_READ_FAILURE_STRICT {name}: v4 over v5's pair should only retire: {}",
                    v["result"]
                );
                continue;
            }
            if renamed != 0 || v["before"] != v["after"] {
                failures.push(format!(
                    "{name} cross v5→v4: v4 renamed {renamed}: {}",
                    v["result"]
                ));
            }
        }
        assert!(v4_cross > 0, "the cross oracle is empty");
    }

    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(checked, 18, "scenario count");
    eprintln!(
        "OK: store-name reconcile matched oracle ({checked} scenarios; {} RECONCILE_READ_FAILURE_STRICT both ways; cross v4→v5 {cross_checked}, v5→v4 {v4_cross}).",
        READ_FAILURE_STRICT.len()
    );
}
