//! Tier-2 differential (P4.D329, v4 `e1872c94f` at `322a5488e`): the Pascal
//! INVENTORY-effects applier (v4 `lib/pascal/inventory-effects.ts`), ported as
//! `quilltap_core::pascal::inventory_effects` behind
//! `pascal::side_effects::apply_effects_for_run`.
//!
//! Both sides run the SAME step matrix (`pascal-inventory-effects.json`) over
//! the Inventory KEYSTONE pair (`build-inventory-mutations-fixture.ts`, reused
//! read-only), each step on a FRESH copy. v4 drives its REAL
//! `applyInventoryEffects` (REAL pool, refs, equip chokepoints and notify);
//! v5 its PRODUCTION seam `apply_effects_for_run` (the in-write half inside
//! `Db::write`, then the after-commit notify + DEBUG) with a synthetic run
//! result carrying the step's gestures. Compared per step, as BYTES:
//!
//! - the applied records (v4's `applied` halves, in order);
//! - the chat's `equippedInventory` cell after the step;
//! - the chat's `chat_messages` rows (role / content / systemSender /
//!   systemKind) — v4's `postPascalInventoryChange` lines on the posted-now
//!   path, none on the joined path;
//! - the item documents the step added (path + parsed content);
//! - the turn's pending map (entries sorted, items as ids; `null` without one);
//! - the module's own lines (context `pascal.inventory-effects`) WHOLE, every
//!   field in v4's order (a v4 array field is v5's `…Json`).
//!
//! Ids the step MINTS (a picked-up instance) are masked positionally on each
//! side (`<minted#n>`, by first appearance) — everything the keystone spec
//! names is compared verbatim.
//!
//! The bag is loaded through P4.D328's real loader (v4's
//! `loadInventoryContext`), so every step compares WHOLE — those that reach
//! the load and those that return before it (no gestures, nobody rolled).
//!
//! The in-write lines run on the WRITER thread, which neither `captured_with`
//! nor `global_capture` sees (memory note `capture-rig-writer-thread`) — so
//! this binary owns a process-wide recording layer, and its ONE test is the
//! only thing that logs.
//!
//! Not covered here (recorded): a re-read FAILURE after a landed gesture (v4's
//! `stopped — could not be re-read`) needs a plant that throws on the SECOND
//! pool read only; it is `inventory_effects.rs`'s rig test.
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout pinned at the
//! oracle baseline — the sweep's `--v4`):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
//!   mkdir -p /tmp/p4d329
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/p4d329/qt-pie-main.db QT_FIXTURE_MOUNT_OUT=/tmp/p4d329/qt-pie-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-inventory-mutations-fixture.ts
//!   QT_FIXTURE_INV_MAIN=/tmp/p4d329/qt-pie-main.db QT_FIXTURE_INV_MOUNT=/tmp/p4d329/qt-pie-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/cases/pascal-inventory-effects.ts \
//!     > /tmp/p4d329/oracle-pascal-inventory-effects.ndjson
//! Run:
//!   QT_ORACLE_PASCAL_INVENTORY_EFFECTS=/tmp/p4d329/oracle-pascal-inventory-effects.ndjson \
//!   QT_FIXTURE_INV_MAIN=/tmp/p4d329/qt-pie-main.db QT_FIXTURE_INV_MOUNT=/tmp/p4d329/qt-pie-mount.db \
//!     cargo test -p quilltap-harness --test pascal_inventory_effects_equivalence -- --nocapture

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::pascal::custom_tool_types::{InventoryEffectAction, OutcomeState, Visibility};
use quilltap_core::pascal::custom_tools::{
    CustomToolRunResult, ResolvedEffect, ResolvedInventoryGesture, RollForm,
};
use quilltap_core::pascal::side_effects::apply_effects_for_run;
use quilltap_core::services::inventory_change_effects::PendingInventoryAnnouncements;
use quilltap_core::test_support::CaptureLayer;
use serde::Deserialize;
use serde_json::{json, Map, Value};

const MODULE_CONTEXT: &str = "pascal.inventory-effects";
/// The messages whose lines are this unit's (each carries the module's context).
const MODULE_MESSAGES: [&str; 6] = [
    "Custom tool: no rolling character, inventory effects skipped",
    "Custom tool inventory effects skipped — the inventory could not be read",
    "Custom tool inventory effects skipped — no such chat",
    "Custom tool inventory effect not applied",
    "Custom tool inventory effects stopped — the inventory could not be re-read",
    "Custom tool inventory effects applied",
];

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "toolName")]
    tool_name: String,
    characters: HashMap<String, String>,
    chats: HashMap<String, String>,
    steps: Vec<Step>,
}

#[derive(Deserialize)]
struct Step {
    name: String,
    chat: String,
    character: Option<String>,
    #[serde(default, rename = "withPending")]
    with_pending: bool,
    gestures: Vec<Value>,
}

#[derive(Deserialize)]
struct OracleRow {
    step: String,
    returned: String,
    pending: Value,
    equipped: Value,
    messages: Value,
    created: Value,
    logs: Vec<Value>,
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures")
}

fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("SKIP: set {key} (see test header).");
            None
        }
    }
}

/// The process-wide recorder: every thread's events (the writer's included).
fn recorder() -> &'static Arc<Mutex<Vec<String>>> {
    static LOGS: OnceLock<Arc<Mutex<Vec<String>>>> = OnceLock::new();
    LOGS.get_or_init(|| {
        use tracing_subscriber::layer::SubscriberExt;
        let logs = Arc::new(Mutex::new(Vec::new()));
        tracing::subscriber::set_global_default(
            tracing_subscriber::registry().with(CaptureLayer(logs.clone())),
        )
        .expect("this binary owns its global subscriber");
        logs
    })
}

fn gesture(v: &Value) -> ResolvedInventoryGesture {
    ResolvedInventoryGesture {
        action: match v["action"].as_str().expect("action") {
            "equip" => InventoryEffectAction::Equip,
            "unequip" => InventoryEffectAction::Unequip,
            "pick_up" => InventoryEffectAction::PickUp,
            other => panic!("unknown action {other}"),
        },
        items: v["items"]
            .as_array()
            .expect("items")
            .iter()
            .map(|i| i.as_str().expect("item").to_string())
            .collect(),
        equip: v.get("equip").and_then(Value::as_bool),
    }
}

/// A finished run whose only effects are the step's gestures.
fn run_result(tool: &str, gestures: &[Value]) -> CustomToolRunResult {
    CustomToolRunResult {
        tool: tool.to_string(),
        params: Vec::new(),
        roll_form: RollForm::Range,
        notation: None,
        raw: 1.0,
        dice_rolls: None,
        value: 1.0,
        state: OutcomeState::Info,
        outcome_index: 0,
        message: "-".into(),
        dice_breakdown: String::new(),
        visibility: Visibility::Public,
        metadata_tested: None,
        llm: None,
        chip_label: None,
        effects: Some(
            gestures
                .iter()
                .map(|g| ResolvedEffect::Inventory {
                    index: g["index"].as_u64().expect("index") as usize,
                    inventory: gesture(&g["inventory"]),
                })
                .collect(),
        ),
    }
}

type Line = (String, String, Vec<(String, String)>);

/// v4's captured line → `(level, message, fields in order)`.
fn oracle_line(v: &Value) -> Line {
    let fields = v["fields"]
        .as_object()
        .expect("fields")
        .iter()
        .map(|(k, val)| match val {
            Value::String(s) => (k.clone(), s.clone()),
            Value::Array(_) | Value::Object(_) => (format!("{k}Json"), val.to_string()),
            other => (k.clone(), other.to_string()),
        })
        .collect();
    (
        v["level"].as_str().expect("level").to_lowercase(),
        v["message"].as_str().expect("message").to_string(),
        fields,
    )
}

/// A rig line (`LEVEL target message k=v …`) → the same shape. A value may
/// carry spaces, so a field runs up to the next ` <ident>=`.
fn rust_line(line: &str) -> Option<Line> {
    let (level, rest) = line.split_once(' ')?;
    let (_target, rest) = rest.split_once(' ')?;
    let message = MODULE_MESSAGES.iter().find(|m| rest.starts_with(**m))?;
    let mut tail = rest[message.len()..].trim_start();
    let mut fields = Vec::new();
    while !tail.is_empty() {
        let (key, after) = tail.split_once('=')?;
        let mut end = after.len();
        let mut search = 0;
        while let Some(off) = after[search..].find(' ') {
            let cand = &after[search + off + 1..];
            let ident_len = cand
                .bytes()
                .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
                .count();
            if ident_len > 0 && cand[ident_len..].starts_with('=') {
                end = search + off;
                break;
            }
            search += off + 1;
        }
        fields.push((key.to_string(), after[..end].to_string()));
        tail = after[end..].trim_start();
    }
    Some((level.to_lowercase(), message.to_string(), fields))
}

/// Mask every UUID the fixture does not name, by first appearance.
struct Masker<'a> {
    known: &'a HashSet<String>,
    minted: Vec<String>,
}

impl Masker<'_> {
    fn text(&mut self, s: &str) -> String {
        let bytes = s.as_bytes();
        let mut out = String::with_capacity(s.len());
        let mut i = 0;
        while i < s.len() {
            if i + 36 <= s.len() && is_uuid(&bytes[i..i + 36]) {
                let id = &s[i..i + 36];
                if self.known.contains(id) {
                    out.push_str(id);
                } else {
                    let n = match self.minted.iter().position(|m| m == id) {
                        Some(n) => n,
                        None => {
                            self.minted.push(id.to_string());
                            self.minted.len() - 1
                        }
                    };
                    out.push_str(&format!("<minted#{n}>"));
                }
                i += 36;
            } else {
                let ch = s[i..].chars().next().unwrap();
                out.push(ch);
                i += ch.len_utf8();
            }
        }
        out
    }
}

/// The two legitimately nondeterministic shapes a MINTING step leaves, once
/// the ids are masked:
///
/// - the carried set is stored SORTED by code unit (v4 `setEquippedInventory`,
///   v5 `set_equipped_inventory`), so a minted id's POSITION follows its random
///   value — each side mints its own. An array of strings holding a
///   `<minted#n>` is compared as sorted (the fixture ids' order is the same
///   sort on both sides, so nothing else is lost);
/// - a created document's `createdAt` / `updatedAt` are the write's wall
///   clock — masked to `<iso>`.
fn settle_minted(text: &str) -> String {
    fn walk(v: &mut Value) {
        match v {
            Value::Array(a) => {
                a.iter_mut().for_each(walk);
                let minted = a
                    .iter()
                    .any(|x| x.as_str().is_some_and(|s| s.starts_with("<minted#")));
                if minted && a.iter().all(Value::is_string) {
                    a.sort_by(|x, y| x.as_str().cmp(&y.as_str()));
                }
            }
            Value::Object(o) => o.values_mut().for_each(walk),
            // The `equippedInventory` cell is JSON TEXT (a string holding the
            // map): settle inside it too.
            Value::String(s) if s.contains("<minted#") => {
                if let Ok(mut inner) = serde_json::from_str::<Value>(s) {
                    if inner.is_object() || inner.is_array() {
                        walk(&mut inner);
                        *s = inner.to_string();
                    }
                }
            }
            _ => {}
        }
    }
    static ISO: OnceLock<regex::Regex> = OnceLock::new();
    let iso = ISO
        .get_or_init(|| regex::Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z").unwrap());
    let text = if text.contains("<minted#") {
        iso.replace_all(text, "<iso>").into_owned()
    } else {
        text.to_string()
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(mut v) => {
            walk(&mut v);
            v.to_string()
        }
        Err(_) => text,
    }
}

fn is_uuid(b: &[u8]) -> bool {
    b.iter().enumerate().all(|(i, c)| match i {
        8 | 13 | 18 | 23 => *c == b'-',
        _ => c.is_ascii_hexdigit(),
    })
}

fn known_ids(texts: &[String]) -> HashSet<String> {
    let mut known = HashSet::new();
    for t in texts {
        let b = t.as_bytes();
        let mut i = 0;
        while i + 36 <= b.len() {
            if is_uuid(&b[i..i + 36]) {
                known.insert(t[i..i + 36].to_string());
                i += 36;
            } else {
                i += 1;
            }
        }
    }
    known
}

fn dump_messages(db: &Db, chat_id: &str) -> Value {
    let chat_id = chat_id.to_string();
    db.read_main(move |c| {
        let exists: bool = c
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'chat_messages'",
                [],
                |_| Ok(true),
            )
            .unwrap_or(false);
        if !exists {
            return Ok(Value::Array(Vec::new()));
        }
        let mut st = c.prepare(
            r#"SELECT "role", "content", "systemSender", "systemKind" FROM "chat_messages" WHERE "chatId" = ?1 ORDER BY rowid"#,
        )?;
        let rows = st
            .query_map([&chat_id], |r| {
                Ok(json!({
                    "role": r.get::<_, Option<String>>(0)?,
                    "content": r.get::<_, Option<String>>(1)?,
                    "systemSender": r.get::<_, Option<String>>(2)?,
                    "systemKind": r.get::<_, Option<String>>(3)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Value::Array(rows))
    })
    .expect("dump chat_messages")
}

/// The keystone builder never posts a message, so its main DB has no
/// `chat_messages` table. v4's repository creates it LAZILY on the first
/// insert (`CREATE TABLE IF NOT EXISTS`), which is how the oracle's posted-now
/// lines land; v5 never creates a MAIN table lazily — every provisioned or
/// v4-written instance that has chats carries it. So the v5 copy gets the
/// table v4's first insert would make, from the committed fresh-schema dump
/// (table + indexes), and only when it is absent.
fn ensure_chat_messages(main: &Path, pepper: &str) {
    let schema: Value = serde_json::from_str(include_str!(
        "../../quilltap-core/src/services/provisioning/fresh_schema.json"
    ))
    .expect("fresh_schema.json");
    let writer = quilltap_core::db::Writer::open_writable(main, pepper).expect("open main");
    let conn = writer.connection();
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'chat_messages'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if exists {
        return;
    }
    let ddl: Vec<&str> = schema["main"]
        .as_array()
        .expect("main DDL")
        .iter()
        .filter_map(Value::as_str)
        .filter(|sql| {
            sql.starts_with("CREATE TABLE \"chat_messages\" (")
                || ((sql.starts_with("CREATE INDEX ") || sql.starts_with("CREATE UNIQUE INDEX "))
                    && (sql.contains(" ON \"chat_messages\" (")
                        || sql.contains(" ON chat_messages (")))
        })
        .collect();
    assert!(!ddl.is_empty(), "no chat_messages DDL in fresh_schema.json");
    for sql in ddl {
        conn.execute_batch(sql).expect("create chat_messages");
    }
}

fn link_ids(db: &Db) -> HashSet<String> {
    db.read_mount_index(|c| {
        let mut st = c.prepare(r#"SELECT "id" FROM "doc_mount_file_links""#)?;
        let ids = st
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<HashSet<_>, _>>()?;
        Ok(ids)
    })
    .expect("read link ids")
}

fn dump_created(db: &Db, known_links: &HashSet<String>) -> Value {
    let rows: Vec<(String, String, Option<String>)> = db
        .read_mount_index(|c| {
            let mut st = c.prepare(
                r#"SELECT l."id", l."relativePath", d."content" FROM "doc_mount_file_links" l JOIN "doc_mount_documents" d ON d."fileId" = l."fileId" ORDER BY l."relativePath""#,
            )?;
            let rows = st
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .expect("dump created documents");
    Value::Array(
        rows.into_iter()
            .filter(|(id, _, _)| !known_links.contains(id))
            .map(|(_, path, content)| json!({ "path": path, "content": content }))
            .collect(),
    )
}

fn dump_equipped(db: &Db, chat_id: &str) -> Value {
    let chat_id = chat_id.to_string();
    db.read_main(move |c| {
        Ok(c.query_row(
            r#"SELECT "equippedInventory" FROM "chats" WHERE "id" = ?1"#,
            [&chat_id],
            |r| r.get::<_, Option<String>>(0),
        )
        .ok()
        .flatten()
        .map(Value::String)
        .unwrap_or(Value::Null))
    })
    .expect("read equippedInventory")
}

fn pending_json(map: &PendingInventoryAnnouncements) -> Value {
    use quilltap_core::inventory::types::InventoryFields;
    let held = map.lock().unwrap();
    let mut keys: Vec<&String> = held.keys().collect();
    keys.sort();
    Value::Array(
        keys.into_iter()
            .map(|k| {
                let e = &held[k];
                let ids = |items: &[quilltap_core::inventory::types::ResolvedInventoryItem]| {
                    items
                        .iter()
                        .map(|i| Value::String(i.id().to_string()))
                        .collect::<Vec<_>>()
                };
                json!({
                    "key": k,
                    "characterId": e.character_id,
                    "equipped": ids(&e.equipped),
                    "unequipped": ids(&e.unequipped),
                    "pickedUp": ids(&e.picked_up),
                    "provenances": e.provenances.iter().map(|p| p.as_str()).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

#[test]
fn pascal_inventory_effects_match_oracle() {
    let logs = recorder();
    let (Some(oracle_path), Some(fx_main), Some(fx_mount)) = (
        env_or_skip("QT_ORACLE_PASCAL_INVENTORY_EFFECTS"),
        env_or_skip("QT_FIXTURE_INV_MAIN"),
        env_or_skip("QT_FIXTURE_INV_MOUNT"),
    ) else {
        return;
    };
    let spec_text = std::fs::read_to_string(fixtures_dir().join("pascal-inventory-effects.json"))
        .expect("spec");
    let keystone_text =
        std::fs::read_to_string(fixtures_dir().join("inventory-keystone.json")).expect("keystone");
    let spec: Spec = serde_json::from_str(&spec_text).expect("parse spec");
    let keystone: Value = serde_json::from_str(&keystone_text).expect("parse keystone");
    let pepper = keystone["testPepperBase64"].as_str().unwrap().to_string();
    let known = known_ids(&[spec_text.clone(), keystone_text.clone()]);
    let oracle: Vec<OracleRow> = std::fs::read_to_string(&oracle_path)
        .expect("read oracle")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle line"))
        .collect();
    assert_eq!(
        oracle.len(),
        spec.steps.len(),
        "oracle step count — regenerate"
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let mut failed: Vec<String> = Vec::new();
    let mut report: Vec<String> = Vec::new();
    let (mut landed_steps, mut module_lines) = (0usize, 0usize);
    for (step, want) in spec.steps.iter().zip(&oracle) {
        assert_eq!(step.name, want.step, "oracle step order");
        let scratch = tempfile::Builder::new()
            .prefix("qt-pie-")
            .tempdir()
            .unwrap();
        let main = scratch.path().join("main.db");
        let mount = scratch.path().join("mount.db");
        std::fs::copy(&fx_main, &main).expect("copy main");
        std::fs::copy(&fx_mount, &mount).expect("copy mount");
        ensure_chat_messages(&main, &pepper);
        let db = Db::open(
            DbPaths {
                main,
                mount_index: Some(mount),
                llm_logs: None,
            },
            &pepper,
        )
        .expect("open the fixture copy");
        let known_links = link_ids(&db);

        let chat_id = spec.chats[&step.chat].clone();
        let character_id = step.character.as_ref().map(|c| spec.characters[c].clone());
        let pending: Option<PendingInventoryAnnouncements> = step
            .with_pending
            .then(|| Arc::new(Mutex::new(HashMap::new())));
        let result = run_result(&spec.tool_name, &step.gestures);

        logs.lock().unwrap().clear();
        let applied = rt.block_on(apply_effects_for_run(
            &db,
            &spec.user_id,
            &chat_id,
            &result,
            None,
            character_id,
            &Map::new(),
            0,
            None,
            None,
            None,
            pending.as_ref(),
        ));
        let lines: Vec<String> = std::mem::take(&mut *logs.lock().unwrap());

        let mut got_mask = Masker {
            known: &known,
            minted: Vec::new(),
        };
        let mut want_mask = Masker {
            known: &known,
            minted: Vec::new(),
        };
        let mut diffs: Vec<String> = Vec::new();
        let mut check = |what: &str, got: String, expected: String| {
            let (g, w) = (
                settle_minted(&got_mask.text(&got)),
                settle_minted(&want_mask.text(&expected)),
            );
            if g != w {
                diffs.push(format!("{what}\n    v5: {g}\n    v4: {w}"));
            }
        };

        let want_returned: Value = serde_json::from_str(&want.returned).expect("returned");
        let want_applied: Vec<Value> = want_returned
            .as_array()
            .expect("returned array")
            .iter()
            .map(|e| e["applied"].clone())
            .collect();
        if !want_applied.is_empty() {
            landed_steps += 1;
        }
        check(
            "applied",
            serde_json::to_string(&applied).unwrap(),
            serde_json::to_string(&want_applied).unwrap(),
        );
        check(
            "equippedInventory",
            dump_equipped(&db, &chat_id).to_string(),
            want.equipped.to_string(),
        );
        check(
            "chat_messages",
            dump_messages(&db, &chat_id).to_string(),
            want.messages.to_string(),
        );
        check(
            "created",
            dump_created(&db, &known_links).to_string(),
            want.created.to_string(),
        );
        check(
            "pending",
            pending
                .as_ref()
                .map(pending_json)
                .unwrap_or(Value::Null)
                .to_string(),
            want.pending.to_string(),
        );

        let want_lines: Vec<Line> = want
            .logs
            .iter()
            .filter(|l| l["fields"]["context"].as_str() == Some(MODULE_CONTEXT))
            .map(oracle_line)
            .collect();
        let got_lines: Vec<Line> = lines.iter().filter_map(|l| rust_line(l)).collect();
        module_lines += want_lines.len();
        check("lines", format!("{got_lines:?}"), format!("{want_lines:?}"));

        if !diffs.is_empty() {
            failed.push(step.name.clone());
            report.push(format!("step {}:\n  {}", step.name, diffs.join("\n  ")));
        }
        drop(db);
    }

    for r in &report {
        eprintln!("{r}");
    }
    assert!(failed.is_empty(), "steps differ from v4: {failed:?}");
    assert!(spec.steps.len() >= 20, "the step matrix looks short");
    assert!(
        landed_steps >= 12,
        "only {landed_steps} v4 steps landed a gesture"
    );
    assert!(
        module_lines >= 25,
        "only {module_lines} module lines compared"
    );
    eprintln!(
        "OK: pascal inventory effects — {} steps, every one compared whole.",
        spec.steps.len()
    );
}
