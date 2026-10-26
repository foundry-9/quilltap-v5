//! Tier-2/3 differential (P4.D328 — v4 `e1872c94f` + `102a23648` at
//! `322a5488e`): the nine `inventory_*` tool handlers, the turn's pending
//! Inventory map and its drain, Pascal's carried-object line, and the
//! transfer module's two route-facing error shapes (R-15).
//!
//! `cases/inventory-tools.ts` runs every case of `inventory-tools.json`
//! through v4's REAL `INVENTORY_TOOL_TABLE` (the steps naming one `turn`
//! sharing ONE pending map; a `flush` step through v4's REAL
//! `flushPendingInventoryAnnouncements`) in its own child process on a fresh
//! copy of the tools pair (`build-inventory-tools-fixture.ts`, BUILT at
//! regen). Here each case runs on its own fresh copy exactly as the executor
//! composes a call — `tool_table::run_inventory_tool_handler` on the two
//! writer connections (the in-write half), then `finish_inventory_tool` over
//! the case's pending map (the after-commit half) — and per case the sides
//! are compared on:
//!   - every step's RUN, `{ success, result, error }`, as BYTES
//!     (`formattedText` first, the payload in the handler's literal key
//!     order), and after a turn step that turn's pending map;
//!   - the DELTA: per table (`doc_mount_documents` / `_files` /
//!     `_file_links` / `_folders`, main `chats` / `chat_messages` /
//!     `background_jobs`), the rows that differ from the fixture's, whole, and
//!     the fixture ids now gone;
//!   - the LINES: this lane's — `[InventoryTools]`, `[InventoryEffects]`,
//!     `[InventoryContext]`, `[InventoryTransfers] Inventory object
//!     transferred` — WHOLE, every field in v4's order (the `…Json`
//!     file-layer convention for arrays and objects); the keystone's
//!     (`[InventoryEquip]`, `[InventoryPool]`, `[InventoryLocation]`,
//!     `[InventoryMutations]`, `[InventoryDependents]`, `[Pascal] Inventory
//!     change posted`, …) by TAG and LEVEL only (§R.5 — their bytes are
//!     P4.D324's family's);
//!   - the realtime publishes (`Realtime publish queued`), as a SORTED list:
//!     the keystone's chokepoints publish inside the write, the handlers after
//!     it, so across a commit the order can differ (`inventory_create` +
//!     `equip`: v4 `inventory` then `chats`, v5 `chats` then `inventory`) — a
//!     recorded-only class (`PUBLISH_ORDER_ACROSS_THE_COMMIT`).
//!
//! The two `transfers` rows drive `inventory::transfers::transfer_inventory_
//! item` directly: an id the destination already holds (`INVALID_SHAPE`, 400,
//! `details.itemId`) and a record listed nowhere in its source (the move's
//! plain `Failed to remove the object from its source after the move`).
//!
//! NORMALISATION (both sides, identically — the keystone family's): a UUID or
//! ISO timestamp the fixture does not carry was MINTED by the case —
//! timestamps become `<ts>`, UUIDs numbered tokens in order of first
//! appearance; a document's `contentSha256` (and its file's `sha256`) is
//! recomputed over the masked content.
//!
//! Generate (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_IT_MAIN=/tmp/qt-it-main.db QT_FIXTURE_IT_MOUNT=/tmp/qt-it-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-inventory-tools-fixture.ts
//!   QT_FIXTURE_IT_MAIN=/tmp/qt-it-main.db QT_FIXTURE_IT_MOUNT=/tmp/qt-it-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/cases/inventory-tools.ts \
//!     > /tmp/oracle-inventory-tools.ndjson
//! Run:
//!   QT_ORACLE_IT=/tmp/oracle-inventory-tools.ndjson \
//!   QT_FIXTURE_IT_MAIN=/tmp/qt-it-main.db QT_FIXTURE_IT_MOUNT=/tmp/qt-it-mount.db \
//!     cargo test -p quilltap-harness --test inventory_tools_equivalence -- --nocapture

mod common;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use quilltap_core::db::fallback::error_text;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::Writer;
use quilltap_core::inventory::errors::{InventoryError, InventoryRefusalCode};
use quilltap_core::inventory::location::{
    resolve_inventory_location, ResolveInventoryLocationOptions,
};
use quilltap_core::inventory::transfers::{
    transfer_inventory_item, InventoryTransferAction, TransferInventoryItemParams,
};
use quilltap_core::inventory::types::{InventoryRecord, InventoryScope};
use quilltap_core::realtime::bus::{
    arm_realtime_bus_for_current_thread, disarm_realtime_bus_for_current_thread, BusSpawner,
};
use quilltap_core::services::inventory_change_effects::{
    flush_pending_inventory_announcements, PendingInventoryAnnouncements,
};
use quilltap_core::services::mount_index::blob_transcode::RefusingWebpTranscoder;
use quilltap_core::test_support::captured_with;
use quilltap_core::tools::inventory_tool_table::{
    finish_inventory_tool, run_inventory_tool_handler, InventoryHandlerCx, InventoryToolContext,
};
use regex::Regex;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

/// The tables a delta covers: (db, table) — the oracle's `TABLES`.
const TABLES: &[(&str, &str)] = &[
    ("mount", "doc_mount_documents"),
    ("mount", "doc_mount_files"),
    ("mount", "doc_mount_file_links"),
    ("mount", "doc_mount_folders"),
    ("main", "chats"),
    ("main", "chat_messages"),
    ("main", "background_jobs"),
];

/// This lane's lines, compared WHOLE.
const OWNED_WHOLE: &[&str] = &[
    "[InventoryTools] ",
    "[InventoryEffects] ",
    "[InventoryContext] ",
    "[InventoryTransfers] Inventory object transferred",
];

/// The keystone's lines this family sees, compared by tag + level.
fn keystone_tagged(message: &str) -> bool {
    (message.starts_with("[Inventory") && !OWNED_WHOLE.iter().any(|p| message.starts_with(p)))
        || message == "[Pascal] Inventory change posted"
}

fn owned_whole(message: &str) -> bool {
    OWNED_WHOLE.iter().any(|p| message.starts_with(p))
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/inventory-tools.json")
}

// ---------------------------------------------------------------------------
// The dumps (the keystone family's shape)
// ---------------------------------------------------------------------------

fn dump_table(conn: &rusqlite::Connection, table: &str) -> Vec<Value> {
    let cols: Vec<String> = conn
        .prepare(&format!("PRAGMA table_info(\"{table}\")"))
        .unwrap()
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    if cols.is_empty() {
        return Vec::new();
    }
    let mut stmt = conn.prepare(&format!("SELECT * FROM \"{table}\"")).unwrap();
    stmt.query_map([], |r| {
        let mut o = Map::new();
        for (j, c) in cols.iter().enumerate() {
            let v = match r.get_ref(j)? {
                rusqlite::types::ValueRef::Null => Value::Null,
                rusqlite::types::ValueRef::Integer(x) => json!(x),
                rusqlite::types::ValueRef::Real(x) if x.fract() == 0.0 && x.abs() < 9e15 => {
                    json!(x as i64)
                }
                rusqlite::types::ValueRef::Real(x) => json!(x),
                rusqlite::types::ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
                rusqlite::types::ValueRef::Blob(b) => json!(format!("<blob {}>", b.len())),
            };
            o.insert(c.clone(), v);
        }
        Ok(Value::Object(o))
    })
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

fn dump_all(main: &rusqlite::Connection, mount: &rusqlite::Connection) -> Vec<Vec<Value>> {
    TABLES
        .iter()
        .map(|(db, t)| dump_table(if *db == "main" { main } else { mount }, t))
        .collect()
}

fn delta(before: &[Vec<Value>], after: &[Vec<Value>]) -> Value {
    let mut out = Map::new();
    for (i, (_, table)) in TABLES.iter().enumerate() {
        let id = |r: &Value| r["id"].as_str().unwrap_or("").to_string();
        let was: HashMap<String, String> =
            before[i].iter().map(|r| (id(r), r.to_string())).collect();
        let now: HashSet<String> = after[i].iter().map(id).collect();
        let changed: Vec<Value> = after[i]
            .iter()
            .filter(|r| was.get(&id(r)) != Some(&r.to_string()))
            .cloned()
            .collect();
        let removed: Vec<String> = before[i]
            .iter()
            .map(id)
            .filter(|k| !now.contains(k))
            .collect();
        out.insert(
            table.to_string(),
            json!({ "changed": changed, "removed": removed }),
        );
    }
    Value::Object(out)
}

// ---------------------------------------------------------------------------
// The normaliser (the keystone family's)
// ---------------------------------------------------------------------------

fn uuid_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")
            .unwrap()
    })
}

fn ts_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z").unwrap())
}

fn sha_hex(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

struct Known {
    uuids: HashSet<String>,
    ts: HashSet<String>,
}

impl Known {
    fn of(texts: &[String]) -> Self {
        let mut uuids = HashSet::new();
        let mut ts = HashSet::new();
        for t in texts {
            for m in uuid_re().find_iter(t) {
                uuids.insert(m.as_str().to_lowercase());
                uuids.insert(m.as_str().to_string());
            }
            for m in ts_re().find_iter(t) {
                ts.insert(m.as_str().to_string());
            }
        }
        Known { uuids, ts }
    }
}

struct Norm<'k> {
    known: &'k Known,
    tokens: HashMap<String, String>,
}

impl<'k> Norm<'k> {
    fn new(known: &'k Known) -> Self {
        Norm {
            known,
            tokens: HashMap::new(),
        }
    }

    fn ts(&self, s: &str) -> String {
        ts_re()
            .replace_all(s, |c: &regex::Captures| {
                if self.known.ts.contains(&c[0]) {
                    c[0].to_string()
                } else {
                    "<ts>".to_string()
                }
            })
            .into_owned()
    }

    fn mask(&self, s: &str) -> String {
        let s = self.ts(s);
        uuid_re()
            .replace_all(&s, |c: &regex::Captures| {
                if self.known.uuids.contains(&c[0]) {
                    c[0].to_string()
                } else {
                    "<uuid>".to_string()
                }
            })
            .into_owned()
    }

    fn tokenize(&mut self, s: &str) -> String {
        let s = self.ts(s);
        let mut out = String::new();
        let mut last = 0;
        for m in uuid_re().find_iter(&s) {
            out.push_str(&s[last..m.start()]);
            let id = m.as_str();
            if self.known.uuids.contains(id) {
                out.push_str(id);
            } else {
                let n = self.tokens.len();
                let t = self
                    .tokens
                    .entry(id.to_string())
                    .or_insert_with(|| format!("<minted-{n}>"));
                out.push_str(t);
            }
            last = m.end();
        }
        out.push_str(&s[last..]);
        out
    }

    /// One side's whole case output, normalised.
    fn case(&mut self, steps: &[String], delta: &Value, lines: &[String]) -> Value {
        let steps: Vec<String> = steps.iter().map(|s| self.tokenize(s)).collect();
        let mut delta = delta.clone();
        let mut file_sha: HashMap<String, String> = HashMap::new();
        if let Some(docs) = delta["doc_mount_documents"]["changed"].as_array_mut() {
            for d in docs.iter_mut() {
                let masked = self.mask(d["content"].as_str().unwrap_or(""));
                let sha = format!("SHA({})", sha_hex(&masked));
                if let Some(f) = d["fileId"].as_str() {
                    file_sha.insert(f.to_string(), sha.clone());
                }
                d["contentSha256"] = json!(sha);
            }
        }
        if let Some(files) = delta["doc_mount_files"]["changed"].as_array_mut() {
            for f in files.iter_mut() {
                if let Some(sha) = f["id"].as_str().and_then(|id| file_sha.get(id)) {
                    f["sha256"] = json!(sha);
                }
            }
        }
        if let Some(chats) = delta["chats"]["changed"].as_array_mut() {
            for c in chats.iter_mut() {
                if let Some(cell) = c["equippedInventory"].as_str() {
                    let mut map: Map<String, Value> =
                        serde_json::from_str(&self.tokenize(cell)).unwrap_or_default();
                    for ids in map.values_mut() {
                        if let Some(a) = ids.as_array_mut() {
                            a.sort_by(|x, y| x.as_str().cmp(&y.as_str()));
                        }
                    }
                    c["equippedInventory"] = json!(serde_json::to_string(&map).unwrap());
                }
            }
        }
        let mut tables = Map::new();
        for (_, table) in TABLES {
            let mut rows: Vec<String> = delta[*table]["changed"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r.to_string())
                .collect();
            rows.sort_by_key(|r| self.mask(r));
            let mut tokenized: Vec<String> = rows.iter().map(|r| self.tokenize(r)).collect();
            tokenized.sort();
            let mut removed: Vec<String> = delta[*table]["removed"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| self.tokenize(r.as_str().unwrap()))
                .collect();
            removed.sort();
            tables.insert(
                table.to_string(),
                json!({ "changed": tokenized, "removed": removed }),
            );
        }
        let lines: Vec<String> = lines.iter().map(|l| self.tokenize(l)).collect();
        json!({ "steps": steps, "delta": tables, "lines": lines })
    }
}

// ---------------------------------------------------------------------------
// The lines, both sides
// ---------------------------------------------------------------------------

/// v4's lines → (owned lines as the capture rig renders them, publishes).
fn v4_lines(logs: &Value) -> (Vec<String>, Vec<String>) {
    let mut lines = Vec::new();
    let mut publishes = Vec::new();
    for log in logs.as_array().unwrap() {
        let message = log["message"].as_str().unwrap();
        let fields: Map<String, Value> = log["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|kv| (kv[0].as_str().unwrap().to_string(), kv[1].clone()))
            .collect();
        if message == "Realtime publish queued" {
            publishes.push(format!(
                "{}:{}",
                fields.get("topic").and_then(Value::as_str).unwrap_or(""),
                fields.get("id").and_then(Value::as_str).unwrap_or("")
            ));
            continue;
        }
        let level = log["level"].as_str().unwrap().to_ascii_uppercase();
        if owned_whole(message) {
            let line = common::v4_log_as_capture_line(
                "T",
                &json!({ "level": log["level"], "message": message, "fields": Value::Object(fields) }),
            );
            lines.push(line.replacen(" T ", " ", 1));
        } else if keystone_tagged(message) {
            lines.push(format!("{level} {message} <tagged>"));
        }
    }
    publishes.sort();
    (lines, publishes)
}

/// v5's captured lines → the same shape.
fn v5_lines(captured: &[String]) -> (Vec<String>, Vec<String>) {
    let mut lines = Vec::new();
    let mut publishes = Vec::new();
    for l in captured {
        let mut parts = l.splitn(3, ' ');
        let (level, _target, rest) = (
            parts.next().unwrap(),
            parts.next().unwrap_or(""),
            parts.next().unwrap_or(""),
        );
        if let Some(fields) = rest.strip_prefix("Realtime publish queued") {
            let f = common::capture_fields(fields.trim());
            let get = |k: &str| f.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string());
            publishes.push(format!(
                "{}:{}",
                get("topic").unwrap_or_default(),
                get("id").unwrap_or_default()
            ));
            continue;
        }
        if owned_whole(rest) {
            lines.push(format!("{level} {rest}"));
        } else if rest.starts_with("[Inventory")
            || rest.starts_with("[Pascal] Inventory change posted")
        {
            // The keystone's line, by its message alone (fields follow the
            // first ` k=v`).
            let message = message_of(rest);
            if keystone_tagged(message) {
                lines.push(format!("{level} {message} <tagged>"));
            }
        }
    }
    publishes.sort();
    (lines, publishes)
}

/// A captured `message k=v …` line's message: everything before the first
/// ` <ident>=`.
fn message_of(rest: &str) -> &str {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r" [A-Za-z_][A-Za-z0-9_]*=").unwrap());
    match re.find(rest) {
        Some(m) => &rest[..m.start()],
        None => rest,
    }
}

// ---------------------------------------------------------------------------
// The errors (the transfers rows — the keystone family's class map)
// ---------------------------------------------------------------------------

fn error_json(e: &InventoryError) -> Value {
    match e {
        InventoryError::Refusal(r) => {
            let name = match r.code {
                InventoryRefusalCode::InvalidShape
                    if r.message == "Source and destination are the same" =>
                {
                    "InventoryTransferSameLocationError"
                }
                InventoryRefusalCode::InvalidShape => "InventoryShapeError",
                InventoryRefusalCode::KitStranded => "InventoryKitStrandedError",
                InventoryRefusalCode::NotCopyable => "InventoryNotCopyableError",
                _ => "InventoryRefusal",
            };
            json!({
                "name": name,
                "message": r.message,
                "code": r.code.as_str(),
                "status": r.status,
                "details": Value::Object(r.details.clone()),
            })
        }
        InventoryError::Db(d) => json!({ "name": "Error", "message": error_text(d) }),
        other => json!({ "name": "Error", "message": other.to_string() }),
    }
}

// ---------------------------------------------------------------------------
// A case, on v5
// ---------------------------------------------------------------------------

fn run_json(success: bool, result: &Value, error: &Option<String>) -> String {
    let mut m = Map::new();
    m.insert("success".into(), Value::Bool(success));
    m.insert("result".into(), result.clone());
    if let Some(e) = error {
        m.insert("error".into(), Value::String(e.clone()));
    }
    serde_json::to_string(&Value::Object(m)).unwrap()
}

fn pending_json(pending: &PendingInventoryAnnouncements) -> String {
    let map = pending.lock().unwrap();
    let mut entries: Vec<(&String, _)> = map.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    serde_json::to_string(&entries).unwrap()
}

/// v4's step output, re-rendered in the comparand's shape.
fn want_step(step: &Value) -> String {
    if let Some(turn) = step.get("flush") {
        return format!("flush {}", turn.as_str().unwrap_or(""));
    }
    let mut out = format!(
        "{} {}",
        step["tool"].as_str().unwrap(),
        step["run"].as_str().unwrap()
    );
    if let Some(p) = step.get("pending").and_then(Value::as_str) {
        // v4's Map in insertion order; v5's in key order (R-6) — re-sorted.
        let mut entries: Vec<Value> = serde_json::from_str::<Vec<Value>>(p).unwrap();
        entries.sort_by(|a, b| a[0].as_str().cmp(&b[0].as_str()));
        out.push_str(&format!(
            " pending={}",
            serde_json::to_string(&entries).unwrap()
        ));
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn run_case_steps(
    rt: &tokio::runtime::Runtime,
    db: &Db,
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    user_id: &str,
    steps: &[Value],
) -> Vec<String> {
    let blob = RefusingWebpTranscoder;
    let mut turns: HashMap<String, PendingInventoryAnnouncements> = HashMap::new();
    let mut out = Vec::new();
    for step in steps {
        if let Some(turn) = step.get("flush").and_then(Value::as_str) {
            let pending = turns
                .entry(turn.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(HashMap::new())))
                .clone();
            rt.block_on(flush_pending_inventory_announcements(Some(&pending), db));
            out.push(format!("flush {turn}"));
            continue;
        }
        let tool = step["tool"].as_str().unwrap();
        let chat_id = step["chatId"].as_str().unwrap();
        let character_id = step["characterId"].as_str().unwrap();
        let turn = step.get("turn").and_then(Value::as_str).map(|t| {
            turns
                .entry(t.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(HashMap::new())))
                .clone()
        });
        let stage = run_inventory_tool_handler(
            tool,
            &InventoryHandlerCx {
                main,
                mount,
                user_id,
                chat_id,
                character_id,
                blob_webp: &blob,
            },
            &step["args"],
        )
        .unwrap_or_else(|| panic!("the table holds no `{tool}`"));
        let output = rt.block_on(finish_inventory_tool(
            db,
            &InventoryToolContext {
                user_id,
                chat_id,
                character_id,
                pending_inventory_announcements: turn.as_ref(),
            },
            stage,
        ));
        let run = output.run();
        let mut line = format!("{tool} {}", run_json(run.success, &run.result, &run.error));
        if let Some(pending) = &turn {
            line.push_str(&format!(" pending={}", pending_json(pending)));
        }
        out.push(line);
    }
    out
}

fn run_transfer(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    user_id: &str,
    t: &Value,
) -> String {
    let locate = |at: &Value, ensure: bool| {
        resolve_inventory_location(
            main,
            mount,
            InventoryScope::parse(at["scope"].as_str().unwrap()).unwrap(),
            at["id"].as_str(),
            user_id,
            ResolveInventoryLocationOptions { ensure },
        )
        .unwrap()
        .expect("a location")
    };
    let source = locate(&t["source"], false);
    let destination = locate(&t["destination"], true);
    let record = match t.get("record") {
        Some(r) => InventoryRecord::new(r.as_object().unwrap().clone()),
        None => source
            .find_item(mount, t["itemId"].as_str().unwrap())
            .unwrap()
            .expect("the source record"),
    };
    let outcome = transfer_inventory_item(
        main,
        mount,
        TransferInventoryItemParams {
            action: InventoryTransferAction::Move,
            source: &source,
            destination: &destination,
            record: &record,
            user_id,
        },
        &RefusingWebpTranscoder,
    );
    match outcome {
        Ok(r) => format!(
            "result {}",
            serde_json::to_string(
                &json!({ "stored": r.stored.to_value(), "item": r.item.to_value() })
            )
            .unwrap()
        ),
        Err(e) => format!("error {}", serde_json::to_string(&error_json(&e)).unwrap()),
    }
}

#[test]
fn inventory_tools_match_oracle() {
    let (Ok(oracle_path), Ok(fixture_main), Ok(fixture_mount)) = (
        std::env::var("QT_ORACLE_IT"),
        std::env::var("QT_FIXTURE_IT_MAIN"),
        std::env::var("QT_FIXTURE_IT_MOUNT"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_IT, QT_FIXTURE_IT_MAIN and QT_FIXTURE_IT_MOUNT (see header)."
        );
        return;
    };
    let spec_text = std::fs::read_to_string(spec_path()).unwrap();
    let spec: Value = serde_json::from_str(&spec_text).unwrap();
    let pepper = spec["testPepperBase64"].as_str().unwrap();
    let user_id = spec["userId"].as_str().unwrap();
    let cases = spec["cases"].as_array().unwrap();
    let transfers = spec["transfers"].as_array().unwrap();
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(
        oracle.len(),
        cases.len() + transfers.len(),
        "one oracle row per case and transfer (regenerate after a spec change)"
    );
    let only = std::env::var("QT_IT_CASE").ok();

    // What the fixture already carries — everything else a case mints.
    let known = {
        let main = Writer::open_writable(Path::new(&fixture_main), pepper).unwrap();
        let mount = Writer::open_writable(Path::new(&fixture_mount), pepper).unwrap();
        let mut texts: Vec<String> = dump_all(main.connection(), mount.connection())
            .iter()
            .flatten()
            .map(|r| r.to_string())
            .collect();
        for t in ["characters", "projects", "groups", "users", "files"] {
            texts.extend(
                dump_table(main.connection(), t)
                    .iter()
                    .map(|r| r.to_string()),
            );
        }
        for t in [
            "doc_mount_points",
            "group_character_members",
            "project_doc_mount_links",
            "group_doc_mount_links",
        ] {
            texts.extend(
                dump_table(mount.connection(), t)
                    .iter()
                    .map(|r| r.to_string()),
            );
        }
        texts.push(spec_text.clone());
        Known::of(&texts)
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut failures: Vec<String> = Vec::new();
    let mut ran = 0usize;
    let mut lines_compared = 0usize;
    let jobs: Vec<(&str, &Value)> = cases
        .iter()
        .map(|c| ("case", c))
        .chain(transfers.iter().map(|t| ("transfer", t)))
        .collect();
    for (i, ((kind, job), want)) in jobs.iter().zip(&oracle).enumerate() {
        let name = job["name"].as_str().unwrap();
        assert_eq!(
            want["name"].as_str(),
            Some(name),
            "oracle row #{i} is a different case"
        );
        if only.as_deref().is_some_and(|o| o != name) {
            continue;
        }
        let dir = tempfile::tempdir().unwrap();
        let (main_path, mount_path) = (dir.path().join("main.db"), dir.path().join("mount.db"));
        std::fs::copy(&fixture_main, &main_path).unwrap();
        std::fs::copy(&fixture_mount, &mount_path).unwrap();
        let main_w = Writer::open_writable(&main_path, pepper).unwrap();
        let mount_w = Writer::open_writable(&mount_path, pepper).unwrap();
        let db = Db::open(
            DbPaths {
                main: main_path.clone(),
                mount_index: Some(mount_path.clone()),
                llm_logs: None,
            },
            pepper,
        )
        .unwrap();
        let (main, mount) = (main_w.connection(), mount_w.connection());
        let before = dump_all(main, mount);

        let (tx, _rx) = tokio::sync::broadcast::channel(256);
        let spawner: BusSpawner = Arc::new(|_fut| {});
        arm_realtime_bus_for_current_thread(tx, spawner, || 0);
        let (got_steps, captured) = captured_with(|| {
            if *kind == "case" {
                run_case_steps(
                    &rt,
                    &db,
                    main,
                    mount,
                    user_id,
                    job["steps"].as_array().unwrap(),
                )
            } else {
                vec![run_transfer(main, mount, user_id, job)]
            }
        });
        disarm_realtime_bus_for_current_thread();
        if std::env::var("QT_IT_DUMP").is_ok() {
            for l in &captured {
                eprintln!("CAPTURED {l}");
            }
        }
        let after = dump_all(main, mount);
        let got_delta = delta(&before, &after);

        let want_steps: Vec<String> = if *kind == "case" {
            want["steps"]
                .as_array()
                .unwrap()
                .iter()
                .map(want_step)
                .collect()
        } else {
            vec![match want.get("error") {
                Some(e) => format!("error {}", serde_json::to_string(e).unwrap()),
                None => format!("result {}", want["result"].as_str().unwrap()),
            }]
        };
        let (want_lines, want_pub) = v4_lines(&want["logs"]);
        let (got_lines, got_pub) = v5_lines(&captured);
        lines_compared += want_lines.len();
        let g = Norm::new(&known).case(&got_steps, &got_delta, &got_lines);
        let w = Norm::new(&known).case(&want_steps, &want["delta"], &want_lines);
        let label = format!("#{i} {name}");
        if g["steps"] != w["steps"] {
            failures.push(format!(
                "{label} STEPS\n  v5: {:#}\n  v4: {:#}",
                g["steps"], w["steps"]
            ));
        }
        for (_, table) in TABLES {
            if g["delta"][*table] != w["delta"][*table] {
                failures.push(format!(
                    "{label} DELTA {table}\n  v5: {:#}\n  v4: {:#}",
                    g["delta"][*table], w["delta"][*table]
                ));
            }
        }
        if g["lines"] != w["lines"] {
            failures.push(format!(
                "{label} LINES\n  v5: {:#}\n  v4: {:#}",
                g["lines"], w["lines"]
            ));
        }
        if got_pub != want_pub {
            failures.push(format!(
                "{label} PUBLISHES\n  v5: {got_pub:?}\n  v4: {want_pub:?}"
            ));
        }
        ran += 1;
        drop(db);
    }
    eprintln!("inventory tools: {ran} cases, {lines_compared} owned / tagged v4 lines");
    let names: Vec<&str> = failures
        .iter()
        .map(|f| f.split('\n').next().unwrap_or(""))
        .collect();
    eprintln!("failing: {names:#?}");
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
