//! Tier-2 differential (P4.D324, the Inventory KEYSTONE — v4 `4fbdad186` at
//! `322a5488e`): location, pool, dependents, the equipped set, the mutations,
//! the pictures, the transfers and equip / arrival defaults, op for op.
//!
//! `cases/inventory-keystone-tier2.ts` runs every op of
//! `inventory-keystone.json` through v4's REAL `lib/inventory/**` in its own
//! child process on a fresh copy of the ONE keystone pair
//! (`build-inventory-mutations-fixture.ts`). Here each op runs through
//! `quilltap_core::inventory` on its own fresh copy, and per op the two sides
//! are compared on:
//!   - the RESULT (`JSON.stringify` / `serde_json::to_string` — key order is
//!     the contract) or the ERROR (`{name, message, code?, status?,
//!     details?}` — the v4 class name each v5 variant stands for);
//!   - the DELTA: per table (`doc_mount_documents` / `_files` /
//!     `_file_links` / `_folders` / `_blobs`, main `files`, `chats`), the rows
//!     that differ from the fixture's, whole, and the fixture ids now gone;
//!   - the OWNED LINES: every `[Inventory…]` line (bar the title lint's and
//!     the change-effects' — P4.D340's and P4.D328's), the inventory
//!     repository lines and the roster check, rendered as v5's capture rig
//!     renders them, field order included;
//!   - the realtime publishes (`Realtime publish queued`, as `{topic, id}`).
//!
//! NORMALISATION (both sides, identically): a UUID or an ISO timestamp the
//! fixture does not already carry was MINTED by the op — timestamps become
//! `<ts>`, UUIDs numbered tokens in order of first appearance (the result
//! first, then each table's rows sorted by their masked form, then the
//! lines). A document's `contentSha256` (and its file's `sha256`) is
//! recomputed over its masked content, so content-addressed bytes that embed
//! a minted id or `now` still line up.
//!
//! Generate (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-inv-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-inv-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-inventory-mutations-fixture.ts
//!   QT_FIXTURE_INV_MAIN=/tmp/qt-inv-main.db QT_FIXTURE_INV_MOUNT=/tmp/qt-inv-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/cases/inventory-keystone-tier2.ts \
//!     > /tmp/oracle-inv-keystone.ndjson
//! (The case runs v4 WITHOUT its title lint — `withoutTitleLint`, R-7: the
//! keystone ports the mutations without the lint calls; the lint is P4.D340's.)
//! Run:
//!   QT_ORACLE_INV_KEYSTONE=/tmp/oracle-inv-keystone.ndjson \
//!   QT_FIXTURE_INV_MAIN=/tmp/qt-inv-main.db QT_FIXTURE_INV_MOUNT=/tmp/qt-inv-mount.db \
//!     cargo test -p quilltap-harness --test inventory_keystone_tier2_equivalence -- --nocapture
//! (`QT_INV_FAMILY=<family>` narrows the run to one family's ops.)

mod common;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use quilltap_core::db::fallback::error_text;
use quilltap_core::db::Writer;
use quilltap_core::inventory::apply_defaults::{
    apply_inventory_defaults_holding_lock, reset_inventory_to_defaults_holding_lock,
    ApplyInventoryDefaultsOptions, InventoryArrivalSource,
};
use quilltap_core::inventory::dependents::{
    find_dependents, find_kits_stranded_by_move, find_records_anywhere,
};
use quilltap_core::inventory::equip::{
    assert_participant, drop_carried_everywhere, equip_items_holding_lock,
    pick_up_items_holding_lock, unequip_items_holding_lock, EquipChat, EquipOptions,
    InventoryChangeContext, InventoryChangeProvenance, PickUpOptions,
};
use quilltap_core::inventory::equipped_set::read_equipped_set;
use quilltap_core::inventory::errors::{InventoryError, InventoryRefusal, InventoryRefusalCode};
use quilltap_core::inventory::folders::InventoryFolderError;
use quilltap_core::inventory::item_images::{
    add_inventory_item_image, delete_inventory_item_image, list_inventory_images_with_fallthrough,
    list_inventory_item_images, resolve_inventory_item_home, set_current_inventory_item_image,
    AddInventoryItemImageInput,
};
use quilltap_core::inventory::item_mutations as mutations;
use quilltap_core::inventory::location::{
    resolve_inventory_location, InventoryLocation, ResolveInventoryLocationOptions,
};
use quilltap_core::inventory::pool::{
    load_inventory_pool, load_pool_for_location, InventoryPool, LoadInventoryPoolOptions,
    LoadPoolContext,
};
use quilltap_core::inventory::transfers::{
    transfer_inventory_item, InventoryTransferAction, TransferInventoryItemParams,
};
use quilltap_core::inventory::types::{
    parse_create_inventory_body, parse_instantiate_inventory_body, parse_update_inventory_body,
    InventoryFields, InventoryRecord, InventoryScope,
};
use quilltap_core::realtime::bus::{
    arm_realtime_bus_for_current_thread, disarm_realtime_bus_for_current_thread, BusSpawner,
};
use quilltap_core::services::mount_index::blob_transcode::RefusingWebpTranscoder;
use quilltap_core::services::wardrobe_image_bridge::WardrobeImageKind;
use quilltap_core::services::wardrobe_item_images::service::ImageMint;
use quilltap_core::test_support::captured_with;
use regex::Regex;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

/// The tables a delta covers: (db, table) — the oracle's `TABLES`.
const TABLES: &[(&str, &str)] = &[
    ("mount", "doc_mount_documents"),
    ("mount", "doc_mount_files"),
    ("mount", "doc_mount_file_links"),
    ("mount", "doc_mount_folders"),
    ("mount", "doc_mount_blobs"),
    ("main", "files"),
    ("main", "chats"),
];

/// The inventory repository / writer lines (`quilltap::db`) the keystone owns.
const DB_OWNED: &[&str] = &[
    "Equipped inventory write",
    "Equipped inventory id removed",
    "Chats carrying an inventory id",
    "Inventory folder read",
    "Inventory folder ensured",
    "Inventory object created in folder",
    "Inventory objects updated in folder",
    "Inventory objects deleted from folder",
    "Inventory folder deleted; contents sent up",
    "Inventory folder renamed",
    "Inventory folders listed",
    "Two Inventory files claim one id; the first is kept",
    "[ProjectRoster] Tool access check",
];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/inventory-keystone.json")
}

fn owned(message: &str) -> bool {
    (message.starts_with("[Inventory")
        && !message.starts_with("[InventoryTitleLint]")
        && !message.starts_with("[InventoryEffects]"))
        || DB_OWNED.contains(&message)
}

// ---------------------------------------------------------------------------
// The dumps
// ---------------------------------------------------------------------------

fn dump_table(conn: &rusqlite::Connection, table: &str) -> Vec<Value> {
    let cols: Vec<String> = conn
        .prepare(&format!("PRAGMA table_info(\"{table}\")"))
        .unwrap()
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .map(Result::unwrap)
        .collect();
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
// The normaliser
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
    // The ISO form, and the bridge's `<yyyymmdd>-<hhmmss>` leaf stem.
    RE.get_or_init(|| {
        Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z|\b\d{8}-\d{6}\b").unwrap()
    })
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

    /// Minted values masked to constants (side-independent sort keys, shas).
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

    /// Minted UUIDs numbered by first appearance.
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

    /// One side's whole op output, normalised.
    fn op(&mut self, outcome: &str, delta: &Value, lines: &[String]) -> Value {
        let outcome = self.tokenize(outcome);
        // Content shas over the masked content (and the doc's file sha).
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
        // A carried list is SORTED by code unit on both sides, so a minted id's
        // place in it is the mint's, not the logic's: once tokenised, each
        // character's list is compared sorted (the known ids' own order still
        // has to agree — sorting a sorted list is the identity).
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
        json!({ "outcome": outcome, "delta": tables, "lines": lines })
    }
}

// ---------------------------------------------------------------------------
// The v4 side's lines, and v5's
// ---------------------------------------------------------------------------

/// `PROMISE_ALL_TIER_READ_ORDER` (recorded divergence): v4's pool reads its
/// tiers under ONE `Promise.all` over better-sqlite3's SYNCHRONOUS reads, so
/// the order the per-mount `Inventory folder read` lines land in is the await
/// depth of each tier's path, not the data; v5 reads the tiers in turn. Each
/// maximal run of consecutive `Inventory folder read` lines is compared as a
/// sorted set (the lines themselves, and everything around a run, stay
/// exact).
fn canonical_read_runs(lines: Vec<String>) -> Vec<String> {
    let is_read = |l: &str| l.starts_with("DEBUG Inventory folder read ");
    let mut out: Vec<String> = Vec::new();
    let mut run: Vec<String> = Vec::new();
    for l in lines {
        if is_read(&l) {
            run.push(l);
            continue;
        }
        run.sort();
        out.append(&mut run);
        out.push(l);
    }
    run.sort();
    out.append(&mut run);
    out
}

fn v4_lines(logs: &Value) -> (Vec<String>, Vec<Value>) {
    let mut lines = Vec::new();
    let mut publishes = Vec::new();
    for log in logs.as_array().unwrap() {
        let message = log["message"].as_str().unwrap();
        let fields: Map<String, Value> = log["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|kv| {
                let k = kv[0].as_str().unwrap().to_string();
                let v = match &kv[1] {
                    Value::String(s) if k == "error" => {
                        Value::String(common::normalize_v4_sqlite(s))
                    }
                    other => other.clone(),
                };
                (k, v)
            })
            .collect();
        if message == "Realtime publish queued" {
            publishes.push(json!({ "topic": fields.get("topic"), "id": fields.get("id").cloned().unwrap_or(Value::Null) }));
            continue;
        }
        if !owned(message) {
            continue;
        }
        let line = common::v4_log_as_capture_line(
            "T",
            &json!({ "level": log["level"], "message": message, "fields": Value::Object(fields) }),
        );
        lines.push(line.replacen(" T ", " ", 1));
    }
    (lines, publishes)
}

fn v5_lines(captured: &[String]) -> (Vec<String>, Vec<Value>) {
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
            let id = get("id").filter(|s| !s.is_empty());
            publishes.push(json!({ "topic": get("topic"), "id": id }));
            continue;
        }
        let message_owned = rest.starts_with("[Inventory")
            && !rest.starts_with("[InventoryTitleLint]")
            && !rest.starts_with("[InventoryEffects]")
            || DB_OWNED
                .iter()
                .any(|m| rest == *m || rest.starts_with(&format!("{m} ")));
        if message_owned {
            lines.push(format!("{level} {rest}"));
        }
    }
    (lines, publishes)
}

// ---------------------------------------------------------------------------
// The ops, on v5
// ---------------------------------------------------------------------------

fn refusal_class(r: &InventoryRefusal) -> &'static str {
    match r.code {
        InventoryRefusalCode::OffRoster => "InventoryOffRosterError",
        InventoryRefusalCode::ArchetypeInUse => "InventoryArchetypeInUseError",
        InventoryRefusalCode::ArchivedDependents => "InventoryArchivedDependentsError",
        InventoryRefusalCode::NotCopyable => "InventoryNotCopyableError",
        InventoryRefusalCode::CapExceeded => "InventoryCapExceededError",
        InventoryRefusalCode::InvalidParent => "InventoryInvalidParentError",
        InventoryRefusalCode::KitNesting => "InventoryKitNestingError",
        InventoryRefusalCode::KitStranded => "InventoryKitStrandedError",
        InventoryRefusalCode::NotEquippable => "InventoryNotEquippableError",
        InventoryRefusalCode::InvalidShape
            if r.message == "Source and destination are the same" =>
        {
            "InventoryTransferSameLocationError"
        }
        InventoryRefusalCode::InvalidShape => "InventoryShapeError",
        InventoryRefusalCode::NotAParticipant => "InventoryNotAParticipantError",
        InventoryRefusalCode::TitleShadow => "InventoryTitleShadowError",
        InventoryRefusalCode::TitleCheckUnavailable => "InventoryTitleCheckUnavailableError",
    }
}

fn error_json(e: &InventoryError) -> Value {
    match e {
        InventoryError::Refusal(r) => json!({
            "name": refusal_class(r),
            "message": r.message,
            "code": r.code.as_str(),
            "status": r.status,
            "details": Value::Object(r.details.clone()),
        }),
        InventoryError::Svg(s) => json!({ "name": "InventorySvgError", "message": s.0 }),
        InventoryError::Folder(f) => json!({
            "name": match f {
                InventoryFolderError::Path(_) => "InventoryFolderPathError",
                InventoryFolderError::NotFound(_) => "InventoryFolderNotFoundError",
                InventoryFolderError::Conflict(_) => "InventoryFolderConflictError",
                InventoryFolderError::NotInLocation(_) => "InventoryItemNotInLocationError",
            },
            "message": f.to_string(),
        }),
        InventoryError::ForeignImage { .. } => {
            json!({ "name": "ForeignWardrobeImageError", "message": e.to_string() })
        }
        InventoryError::CharacterArchived { .. } => {
            json!({ "name": "CharacterArchivedError", "message": e.to_string() })
        }
        InventoryError::Db(d) => json!({ "name": "Error", "message": error_text(d) }),
        InventoryError::Other(m) => json!({ "name": "Error", "message": m }),
    }
}

fn other(m: String) -> InventoryError {
    InventoryError::Other(m)
}

struct Ops<'a> {
    main: &'a rusqlite::Connection,
    mount: &'a rusqlite::Connection,
    user_id: &'a str,
    webp: &'a [String],
}

fn scope_of(at: &Value) -> InventoryScope {
    InventoryScope::parse(at["scope"].as_str().unwrap()).unwrap()
}

fn str_list(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn to_text(v: &impl serde::Serialize) -> String {
    serde_json::to_string(v).unwrap()
}

fn pool_summary(p: &InventoryPool) -> Value {
    let own: Vec<&str> = p.own.iter().map(|r| r.id()).collect();
    let by_id: Vec<&str> = p.by_id.values().map(|r| r.id()).collect();
    json!({
        "reachable": p.resolved(),
        "byId": by_id,
        "own": own,
        "consultedMounts": p.consulted_mounts,
        "unreadMounts": p.unread_mounts,
    })
}

fn record_value(r: &InventoryRecord) -> Value {
    r.to_value()
}

impl Ops<'_> {
    fn loc(&self, at: &Value) -> Result<InventoryLocation, InventoryError> {
        let id = at["id"].as_str();
        resolve_inventory_location(
            self.main,
            self.mount,
            scope_of(at),
            id,
            self.user_id,
            ResolveInventoryLocationOptions {
                ensure: at["ensure"].as_bool().unwrap_or(false),
            },
        )?
        .ok_or_else(|| {
            other(format!(
                "no location for {}:{}",
                at["scope"].as_str().unwrap(),
                id.unwrap_or("null")
            ))
        })
    }

    fn record_of(
        &self,
        at: &Value,
        item_id: &str,
    ) -> Result<(InventoryLocation, InventoryRecord), InventoryError> {
        let l = self.loc(at)?;
        let r = l
            .find_item(self.mount, item_id)?
            .ok_or_else(|| other(format!("no record {item_id}")))?;
        Ok((l, r))
    }

    fn chat_row(&self, chat_id: &str) -> Result<Value, InventoryError> {
        quilltap_core::db::chats_read::find_by_id_or_none(self.main, chat_id)
            .ok_or_else(|| other(format!("no chat {chat_id}")))
    }

    fn picture_id(&self, item_id: &str, n: usize) -> String {
        list_inventory_item_images(self.main, item_id)
            .unwrap()
            .get(n)
            .map(|f| f.id.clone())
            .unwrap_or_else(|| format!("no-picture-{n}"))
    }

    fn file_id_of(&self, op: &Value, item_id: &str) -> String {
        match op["fileId"].as_str() {
            Some(f) => f.to_string(),
            None => self.picture_id(item_id, op["picture"].as_u64().unwrap() as usize),
        }
    }

    fn zod(issues: Vec<quilltap_core::api::zod_issues::ZodIssue>) -> InventoryError {
        other(format!("zod: {issues:?}"))
    }

    fn run(&self, op: &Value) -> Result<String, InventoryError> {
        let s = |k: &str| op[k].as_str().unwrap_or_default().to_string();
        let provenance = match op["provenance"].as_str() {
            Some("character") => InventoryChangeProvenance::Character,
            Some("pascal") => InventoryChangeProvenance::Pascal,
            _ => InventoryChangeProvenance::Operator,
        };
        let ctx = InventoryChangeContext {
            user_id: self.user_id,
            provenance,
            announce: false,
            pending: None,
        };
        let (main, mount) = (self.main, self.mount);
        Ok(match op["kind"].as_str().unwrap() {
            "resolveLocation" => {
                let l = resolve_inventory_location(
                    main,
                    mount,
                    scope_of(op),
                    op["id"].as_str(),
                    self.user_id,
                    ResolveInventoryLocationOptions {
                        ensure: op["ensure"].as_bool().unwrap_or(false),
                    },
                )?;
                to_text(&l.map(|l| {
                    json!({
                        "scope": l.scope.as_str(),
                        "id": l.id,
                        "mountPointId": l.mount_point_id,
                        "characterId": l.character_id,
                        "origin": l.origin.to_value(),
                    })
                }))
            }
            "readItems" => {
                let l = self.loc(&op["at"])?;
                let items: Vec<Value> = l
                    .read_items(mount, op["includeArchived"].as_bool().unwrap_or(false))?
                    .iter()
                    .map(record_value)
                    .collect();
                to_text(&items)
            }
            "listFolders" => to_text(&self.loc(&op["at"])?.list_folders(mount)?),
            "loadPool" => to_text(&pool_summary(&load_inventory_pool(
                main,
                mount,
                LoadInventoryPoolOptions {
                    character_id: op["characterId"].as_str().unwrap(),
                    chat_id: op["chatId"].as_str(),
                    project_id: op["projectId"].as_str(),
                    own_records: None,
                },
            )?)),
            "loadPoolForLocation" => {
                let l = self.loc(&op["at"])?;
                to_text(&pool_summary(&load_pool_for_location(
                    main,
                    mount,
                    &l,
                    LoadPoolContext {
                        chat_id: op["context"]["chatId"].as_str(),
                        project_id: op["context"]["projectId"].as_str(),
                    },
                )?))
            }
            "findDependents" => to_text(&find_dependents(main, mount, &s("archetypeId"))?),
            "findRecordsAnywhere" => {
                let found: Vec<Value> = find_records_anywhere(mount, &str_list(&op["ids"]))
                    .into_iter()
                    .map(|(id, m)| json!([id, { "record": m.record.to_value(), "mountPointId": m.mount_point_id }]))
                    .collect();
                to_text(&found)
            }
            "findKitsStranded" => {
                let t = self.loc(&op["target"])?;
                to_text(&find_kits_stranded_by_move(
                    main,
                    mount,
                    &s("itemId"),
                    &t.mount_point_id,
                    t.scope,
                )?)
            }
            "readEquippedSet" => {
                let chat = self.chat_row(&s("chatId"))?;
                let pool = load_inventory_pool(
                    main,
                    mount,
                    LoadInventoryPoolOptions {
                        character_id: op["characterId"].as_str().unwrap(),
                        chat_id: chat["id"].as_str(),
                        project_id: chat["projectId"].as_str(),
                        own_records: None,
                    },
                )?;
                to_text(&read_equipped_set(
                    chat["id"].as_str(),
                    chat.get("equippedInventory"),
                    &s("characterId"),
                    &pool,
                ))
            }
            "create" => {
                let body = parse_create_inventory_body(&op["body"]).map_err(Self::zod)?;
                let l = self.loc(&op["at"])?;
                to_text(&record_value(&mutations::create_inventory_item(
                    main,
                    mount,
                    &l,
                    &body,
                    mutations::CreateInventoryItemOptions::default(),
                )?))
            }
            "instantiate" => {
                let body = parse_instantiate_inventory_body(&op["body"]).map_err(Self::zod)?;
                let l = self.loc(&op["at"])?;
                let overrides = body.get("overrides").and_then(Value::as_object).cloned();
                let made = mutations::instantiate(
                    main,
                    mount,
                    &l,
                    body["archetypeId"].as_str().unwrap(),
                    mutations::InstantiateOptions {
                        folder: body.get("folder").and_then(Value::as_str),
                        overrides: overrides.as_ref(),
                        pool: None,
                    },
                )?;
                to_text(&json!({ "record": made.record.to_value(), "item": made.item }))
            }
            "update" => {
                let body = parse_update_inventory_body(&op["body"]).map_err(Self::zod)?;
                let (l, r) = self.record_of(&op["at"], &s("itemId"))?;
                let ctx = LoadPoolContext {
                    chat_id: op["context"]["chatId"].as_str(),
                    project_id: op["context"]["projectId"].as_str(),
                };
                to_text(
                    &mutations::update_inventory_item(main, mount, &l, &r, &body, ctx)?
                        .map(|r| r.to_value()),
                )
            }
            "clearOverrides" => {
                let (l, r) = self.record_of(&op["at"], &s("itemId"))?;
                to_text(
                    &mutations::clear_overrides(main, mount, &l, &r, &str_list(&op["fields"]))?
                        .map(|r| r.to_value()),
                )
            }
            "setArchived" => {
                let (l, r) = self.record_of(&op["at"], &s("itemId"))?;
                let out = mutations::set_inventory_item_archived(
                    main,
                    mount,
                    &l,
                    &r,
                    op["archived"].as_bool().unwrap(),
                )?;
                to_text(&json!({ "item": out.item.map(|r| r.to_value()), "changed": out.changed }))
            }
            "detach" => {
                let (l, r) = self.record_of(&op["at"], &s("itemId"))?;
                to_text(&mutations::detach(main, mount, &l, &r, None)?.map(|r| r.to_value()))
            }
            "detachAll" => to_text(&mutations::detach_all(main, mount, &s("archetypeId"))?),
            "delete" => {
                let (l, r) = self.record_of(&op["at"], &s("itemId"))?;
                let d = mutations::delete_inventory_item(
                    main,
                    mount,
                    &l,
                    &r,
                    op["force"].as_bool().unwrap_or(false),
                )?;
                to_text(&json!({ "deleted": d.deleted, "detached": d.detached }))
            }
            "duplicate" => {
                let (l, r) = self.record_of(&op["at"], &s("itemId"))?;
                to_text(&mutations::duplicate_inventory_item(main, mount, &l, &r)?.to_value())
            }
            "moveToFolder" => {
                let l = self.loc(&op["at"])?;
                let moved = mutations::move_inventory_items_to_folder(
                    main,
                    mount,
                    &l,
                    &str_list(&op["itemIds"]),
                    &s("folder"),
                )?;
                to_text(&moved.iter().map(record_value).collect::<Vec<_>>())
            }
            "copyToFolder" => {
                let l = self.loc(&op["at"])?;
                let copies = mutations::copy_inventory_items_to_folder(
                    main,
                    mount,
                    &l,
                    &str_list(&op["itemIds"]),
                    &s("folder"),
                )?;
                to_text(&copies.iter().map(record_value).collect::<Vec<_>>())
            }
            "createFolder" => to_text(&mutations::create_inventory_folder(
                main,
                mount,
                &self.loc(&op["at"])?,
                &s("path"),
            )?),
            "renameFolder" => to_text(&mutations::rename_inventory_folder(
                main,
                mount,
                &self.loc(&op["at"])?,
                &s("path"),
                &s("newName"),
            )?),
            "deleteFolder" => {
                let d = mutations::delete_inventory_folder(
                    main,
                    mount,
                    &self.loc(&op["at"])?,
                    &s("path"),
                )?;
                to_text(&json!({ "movedItems": d.moved_items, "movedFolders": d.moved_folders }))
            }
            "listImages" => {
                let home = resolve_inventory_item_home(
                    main,
                    mount,
                    self.user_id,
                    scope_of(&op["at"]),
                    op["at"]["id"].as_str(),
                    &s("itemId"),
                )?;
                match home {
                    Some(h) => to_text(&list_inventory_images_with_fallthrough(main, &h.item)?),
                    None => "null".to_string(),
                }
            }
            "addImage" | "setCurrentImage" | "deleteImage" => {
                let home = resolve_inventory_item_home(
                    main,
                    mount,
                    self.user_id,
                    scope_of(&op["at"]),
                    op["at"]["id"].as_str(),
                    &s("itemId"),
                )?
                .ok_or_else(|| other("no home".to_string()))?;
                match op["kind"].as_str().unwrap() {
                    "addImage" => {
                        let bytes =
                            base64_decode(&self.webp[op["webp"].as_u64().unwrap() as usize]);
                        let kind = WardrobeImageKind::from_file_source(
                            &op["imageKind"].as_str().unwrap().to_uppercase(),
                        );
                        let added = add_inventory_item_image(
                            main,
                            mount,
                            &home,
                            &AddInventoryItemImageInput {
                                user_id: self.user_id,
                                kind,
                                content: &bytes,
                                content_type: "image/webp",
                                width: None,
                                height: None,
                                generation_prompt: None,
                                generation_model: None,
                                generation_revised_prompt: None,
                            },
                            &ImageMint::now(),
                            &RefusingWebpTranscoder,
                        )?;
                        to_text(
                            &json!({ "fileId": added.file.id, "record": added.record.map(|r| r.to_value()) }),
                        )
                    }
                    "setCurrentImage" => {
                        let file_id = self.file_id_of(op, &s("itemId"));
                        to_text(&set_current_inventory_item_image(
                            main, mount, &home, &file_id,
                        )?)
                    }
                    _ => {
                        let file_id = self.file_id_of(op, &s("itemId"));
                        to_text(&delete_inventory_item_image(main, mount, &home, &file_id)?)
                    }
                }
            }
            "transfer" => {
                let source = self.loc(&op["source"])?;
                let destination = self.loc(&op["destination"])?;
                let record = source
                    .find_item(mount, &s("itemId"))?
                    .ok_or_else(|| other("no source record".to_string()))?;
                let action = if s("action") == "copy" {
                    InventoryTransferAction::Copy
                } else {
                    InventoryTransferAction::Move
                };
                let out = transfer_inventory_item(
                    main,
                    mount,
                    TransferInventoryItemParams {
                        action,
                        source: &source,
                        destination: &destination,
                        record: &record,
                        user_id: self.user_id,
                    },
                    &RefusingWebpTranscoder,
                )?;
                to_text(&json!({ "stored": out.stored.to_value(), "item": out.item }))
            }
            "equip" => {
                let chat = EquipChat::from_chat_row(&self.chat_row(&s("chatId"))?);
                let opts = EquipOptions {
                    lenient: op["lenient"].as_bool().unwrap_or(false),
                    known: Vec::new(),
                };
                let (result, _) = equip_items_holding_lock(
                    main,
                    mount,
                    &chat,
                    &s("characterId"),
                    &str_list(&op["ids"]),
                    &ctx,
                    &opts,
                )?;
                to_text(&result)
            }
            "unequip" => {
                let chat = EquipChat::from_chat_row(&self.chat_row(&s("chatId"))?);
                assert_participant(&chat, &s("characterId"))?;
                let (result, _) = unequip_items_holding_lock(
                    main,
                    mount,
                    &chat,
                    &s("characterId"),
                    &str_list(&op["ids"]),
                )?;
                to_text(&result)
            }
            "pickUp" => {
                let chat = EquipChat::from_chat_row(&self.chat_row(&s("chatId"))?);
                assert_participant(&chat, &s("characterId"))?;
                let opts = PickUpOptions {
                    title: op["title"].as_str().map(str::to_string),
                    equip: op["equip"].as_bool() != Some(false),
                };
                let (result, _) = pick_up_items_holding_lock(
                    main,
                    mount,
                    &chat,
                    &s("characterId"),
                    &str_list(&op["archetypeIds"]),
                    &ctx,
                    &opts,
                )?;
                to_text(&result)
            }
            "dropEverywhere" => to_text(&drop_carried_everywhere(
                main,
                &s("characterId"),
                &s("itemId"),
                &str_list(&op["also"]),
            )),
            "applyDefaults" => {
                let source = match s("source").as_str() {
                    "participant-added" => InventoryArrivalSource::ParticipantAdded,
                    "merge" => InventoryArrivalSource::Merge,
                    _ => InventoryArrivalSource::ChatStart,
                };
                apply_inventory_defaults_holding_lock(
                    main,
                    mount,
                    &s("chatId"),
                    &str_list(&op["characterIds"]),
                    &ApplyInventoryDefaultsOptions {
                        source,
                        user_id: self.user_id.to_string(),
                        source_chat_id: op["sourceChatId"].as_str().map(str::to_string),
                    },
                );
                "null".to_string()
            }
            "resetDefaults" => {
                let chat = EquipChat::from_chat_row(&self.chat_row(&s("chatId"))?);
                let (ids, _) = reset_inventory_to_defaults_holding_lock(
                    main,
                    mount,
                    &chat,
                    &s("characterId"),
                    self.user_id,
                )?;
                to_text(&ids)
            }
            other_kind => panic!("unknown op kind {other_kind}"),
        })
    }
}

fn base64_decode(s: &str) -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(s).unwrap()
}

#[test]
fn inventory_keystone_tier2_matches_oracle() {
    let (Ok(oracle_path), Ok(fixture_main), Ok(fixture_mount)) = (
        std::env::var("QT_ORACLE_INV_KEYSTONE"),
        std::env::var("QT_FIXTURE_INV_MAIN"),
        std::env::var("QT_FIXTURE_INV_MOUNT"),
    ) else {
        eprintln!("SKIP: set QT_ORACLE_INV_KEYSTONE, QT_FIXTURE_INV_MAIN and QT_FIXTURE_INV_MOUNT (see header).");
        return;
    };
    let spec_text = std::fs::read_to_string(spec_path()).unwrap();
    let spec: Value = serde_json::from_str(&spec_text).unwrap();
    let pepper = spec["testPepperBase64"].as_str().unwrap();
    let user_id = spec["userId"].as_str().unwrap();
    let webp = str_list(&spec["webp"]);
    let ops = spec["ops"].as_array().unwrap();
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(
        oracle.len(),
        ops.len(),
        "one oracle row per op (regenerate after a spec change)"
    );
    let family = std::env::var("QT_INV_FAMILY").ok();

    // What the fixture already carries — everything else an op mints.
    let known = {
        let main = Writer::open_writable(Path::new(&fixture_main), pepper).unwrap();
        let mount = Writer::open_writable(Path::new(&fixture_mount), pepper).unwrap();
        let mut texts: Vec<String> = dump_all(main.connection(), mount.connection())
            .iter()
            .flatten()
            .map(|r| r.to_string())
            .collect();
        for t in ["characters", "projects", "groups", "instance_settings"] {
            if let Ok(mut stmt) = main.connection().prepare(&format!("SELECT * FROM \"{t}\"")) {
                let n = stmt.column_count();
                let rows = stmt
                    .query_map([], |r| {
                        Ok((0..n)
                            .map(|j| {
                                r.get::<_, rusqlite::types::Value>(j)
                                    .map(|v| format!("{v:?}"))
                                    .unwrap_or_default()
                            })
                            .collect::<Vec<_>>()
                            .join(" "))
                    })
                    .unwrap();
                texts.extend(rows.map(Result::unwrap));
            }
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

    let mut failures: Vec<String> = Vec::new();
    let mut ran = 0usize;
    let mut lines_compared = 0usize;
    for (i, (op, want)) in ops.iter().zip(&oracle).enumerate() {
        if family
            .as_deref()
            .is_some_and(|f| op["family"].as_str() != Some(f))
        {
            continue;
        }
        let name = op["name"].as_str().unwrap();
        assert_eq!(
            want["name"].as_str(),
            Some(name),
            "oracle row #{i} is a different op"
        );
        let dir = tempfile::tempdir().unwrap();
        let (main_path, mount_path) = (dir.path().join("main.db"), dir.path().join("mount.db"));
        std::fs::copy(&fixture_main, &main_path).unwrap();
        std::fs::copy(&fixture_mount, &mount_path).unwrap();
        let main_w = Writer::open_writable(&main_path, pepper).unwrap();
        let mount_w = Writer::open_writable(&mount_path, pepper).unwrap();
        let (main, mount) = (main_w.connection(), mount_w.connection());
        let before = dump_all(main, mount);

        let (tx, _rx) = tokio::sync::broadcast::channel(256);
        let spawner: BusSpawner = Arc::new(|_fut| {});
        arm_realtime_bus_for_current_thread(tx, spawner, || 0);
        let ops_runner = Ops {
            main,
            mount,
            user_id,
            webp: &webp,
        };
        let (outcome, captured) = captured_with(|| match ops_runner.run(op) {
            Ok(text) => json!({ "result": text }),
            Err(e) => json!({ "error": error_json(&e) }),
        });
        disarm_realtime_bus_for_current_thread();
        let after = dump_all(main, mount);
        let got_delta = delta(&before, &after);

        let want_outcome = match want.get("error") {
            Some(e) => json!({ "error": e }),
            None => json!({ "result": want["result"] }),
        };
        let (want_lines, want_pub) = v4_lines(&want["logs"]);
        let (got_lines, got_pub) = v5_lines(&captured);
        let (want_lines, got_lines) = (
            canonical_read_runs(want_lines),
            canonical_read_runs(got_lines),
        );
        lines_compared += want_lines.len();
        let g = Norm::new(&known).op(&outcome.to_string(), &got_delta, &got_lines);
        let w = Norm::new(&known).op(&want_outcome.to_string(), &want["delta"], &want_lines);
        let label = format!("#{i} [{}] {name}", op["family"].as_str().unwrap());
        if g["outcome"] != w["outcome"] {
            failures.push(format!(
                "{label} OUTCOME\n  v5: {}\n  v4: {}",
                g["outcome"], w["outcome"]
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
    }
    eprintln!("inventory keystone: {ran} ops, {lines_compared} owned v4 lines");
    let mut by_family: HashMap<String, usize> = HashMap::new();
    for f in &failures {
        let fam = f
            .split('[')
            .nth(1)
            .and_then(|r| r.split(']').next())
            .unwrap_or("?")
            .to_string();
        *by_family.entry(fam).or_default() += 1;
    }
    eprintln!("failures by family: {by_family:?}");
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
