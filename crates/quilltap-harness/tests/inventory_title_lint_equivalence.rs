//! Differential (P4.D340, the Inventory title lint — v4 `7263105c6` at
//! `322a5488e`): ONE family, two tiers inside it (R-5).
//!
//! `cases/inventory-title-lint.ts` runs every input of
//! `inventory-title-lint.json` through v4's REAL code; here each runs through
//! `quilltap_core::inventory::{lint_scope, title_lint, title_lint_documents}`
//! and `db::database_store`'s four hooks:
//!   - `path-*` / `norm-*` / `msg-*` (tier 1, exact): the three path
//!     predicates, `normalize_title`, and the two refusals' `{name, message,
//!     code, status, details}` (details as BYTES);
//!   - the `ops` (DB substrate), each on its own fresh copy of the pair
//!     (`build-inventory-title-lint-fixture.ts`): `lint-*` call
//!     `lint_inventory_title` directly with v4's injected census (R-12);
//!     `doc-*` call `lint_inventory_document_change` and then the pending
//!     rows' `write()` (a marker line between); `store-*` call the four
//!     hooks. Per op: the OUTCOME (`serde_json::to_string` — key order is the
//!     contract — or the error), every `[InventoryTitleLint]` /
//!     `[ToolGateKeys]` line (rendered as the capture rig renders it, field
//!     order included), the PRESENCE by tag of the pool's own `[InventoryPool]`
//!     lines (§R.5 — the keystone owns their bytes), and the mount tables'
//!     delta, minted ids / timestamps normalised identically on both sides.
//!
//! **Carved on this branch, by NAME** (`CARVED`): the rows whose census holds
//! a key that normalises to the title (`UNTIL_UNION_S10` — the gate-key
//! resolution is P4.D328's `resolve_inventory_ref`) and the rows that keep
//! General's KEYED tool (`UNTIL_UNION_S9_C3_C4` — the census walk is P4.D328 /
//! P4.D329 / P4.D333's). Each carved row is still a PIN: it asserts v5's
//! branch answer is the fail-closed stand-in (R-3 / R-4). The unifier lifts
//! the carve at §S.5.
//!
//! Generate (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-itl-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-itl-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-inventory-title-lint-fixture.ts
//!   LOG_LEVEL=debug QT_FIXTURE_ITL_MAIN=/tmp/qt-itl-main.db QT_FIXTURE_ITL_MOUNT=/tmp/qt-itl-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/cases/inventory-title-lint.ts \
//!     > /tmp/oracle-inventory-title-lint.ndjson
//! Run:
//!   QT_ORACLE_INV_TITLE_LINT=/tmp/oracle-inventory-title-lint.ndjson \
//!   QT_FIXTURE_ITL_MAIN=/tmp/qt-itl-main.db QT_FIXTURE_ITL_MOUNT=/tmp/qt-itl-mount.db \
//!     cargo test -p quilltap-harness --test inventory_title_lint_equivalence -- --nocapture

mod common;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use quilltap_core::db::database_store::{
    delete_database_document, move_database_document, move_database_folder,
    write_database_document, StoreError,
};
use quilltap_core::db::fallback::error_text;
use quilltap_core::db::Writer;
use quilltap_core::inventory::errors::{
    InventoryRefusal, InventoryRefusalCode, InventoryTitleCollision, InventoryTitleLintUnread,
};
use quilltap_core::inventory::lint_scope::{
    is_title_lint_folder, is_title_lint_path, is_tool_definition_path, TitleLintScope,
};
use quilltap_core::inventory::location::{
    resolve_inventory_location, InventoryLocation, ResolveInventoryLocationOptions,
};
use quilltap_core::inventory::pool::{
    load_pool_for_location, LoadPoolContext, UnreadInventoryMount,
};
use quilltap_core::inventory::title_lint::{
    lint_inventory_title, normalize_title, TitleLintDoor, TitleLintOptions, TitleLintRequest,
};
use quilltap_core::inventory::title_lint_documents::{
    lint_inventory_document_change, InventoryDocumentChange,
};
use quilltap_core::inventory::types::{InventoryRecord, InventoryScope};
use quilltap_core::pascal::tool_gate_keys::{
    GateKeyClause, GateKeySubject, ToolGateKeyCensus, ToolGateKeyUnread, ToolGateKeyUse,
};
use quilltap_core::test_support::captured_with;
use regex::Regex;
use serde_json::{json, Map, Value};

/// The mount tables a delta covers (the oracle's `TABLES`).
const TABLES: &[&str] = &[
    "doc_mount_documents",
    "doc_mount_files",
    "doc_mount_file_links",
    "doc_mount_folders",
];

/// Rows carved until the union (§S.5), by NAME — each still pinned to the
/// branch's fail-closed stand-in.
const CARVED: &[(&str, &str)] = &[
    ("lint-crafted-create", "UNTIL_UNION_S10"),
    ("lint-crafted-rename", "UNTIL_UNION_S10"),
    ("lint-crafted-diacritics", "UNTIL_UNION_S10"),
    ("lint-naming-tape", "UNTIL_UNION_S10"),
    ("lint-two-objects-shadow", "UNTIL_UNION_S10"),
    ("lint-two-tools-sorted", "UNTIL_UNION_S10"),
    ("lint-leaving-dark", "UNTIL_UNION_S10"),
    ("lint-leaving-redirected", "UNTIL_UNION_S10"),
    ("lint-archive-dark", "UNTIL_UNION_S10"),
    ("lint-delete-dark", "UNTIL_UNION_S10"),
    ("lint-leaving-unverifiable-mount", "UNTIL_UNION_S10"),
    ("doc-keyed-naming", "UNTIL_UNION_S9_C3_C4"),
    ("doc-keyed-tool-delete", "UNTIL_UNION_S9_C3_C4"),
];

/// v4's repository lines a lint read can reach (the R-7 fallbacks).
const REPO_LINES: &[&str] = &[
    "Error finding document by mount point and path",
    "Error finding documents by mount point IDs and folder",
];

const MARKER: &str = "MARKER --- after the change ---";

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/inventory-title-lint.json")
}

fn compared(message: &str) -> bool {
    message.starts_with("[InventoryTitleLint]")
        || message.starts_with("[ToolGateKeys]")
        || REPO_LINES.contains(&message)
}

// ---------------------------------------------------------------------------
// The dumps and the normalisation (the keystone family's, narrowed to mount)
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

fn dump_all(mount: &rusqlite::Connection) -> Vec<Vec<Value>> {
    TABLES.iter().map(|t| dump_table(mount, t)).collect()
}

fn delta(before: &[Vec<Value>], after: &[Vec<Value>]) -> Value {
    let mut out = Map::new();
    for (i, table) in TABLES.iter().enumerate() {
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

/// Minted values (an id or timestamp the fixture does not carry), masked in
/// order of first appearance. `contentSha256` / `sha256` / `lastModified`
/// columns are content- or clock-derived and identical by construction when
/// the content is: they are masked whole.
struct Norm<'k> {
    known: &'k HashSet<String>,
    tokens: HashMap<String, String>,
}

impl<'k> Norm<'k> {
    fn new(known: &'k HashSet<String>) -> Self {
        Norm {
            known,
            tokens: HashMap::new(),
        }
    }

    fn tokenize(&mut self, s: &str) -> String {
        let s = ts_re().replace_all(s, |c: &regex::Captures| {
            if self.known.contains(&c[0]) {
                c[0].to_string()
            } else {
                "<ts>".to_string()
            }
        });
        let mut out = String::new();
        let mut last = 0;
        for m in uuid_re().find_iter(&s) {
            out.push_str(&s[last..m.start()]);
            let id = m.as_str();
            if self.known.contains(id) {
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

    fn delta(&mut self, delta: &Value) -> Value {
        let mut tables = Map::new();
        for table in TABLES {
            let mut rows: Vec<Value> = delta[*table]["changed"].as_array().unwrap().clone();
            for r in rows.iter_mut() {
                for k in [
                    "contentSha256",
                    "sha256",
                    "lastModified",
                    "updatedAt",
                    "createdAt",
                ] {
                    if r.get(k).is_some_and(|v| !v.is_null()) {
                        r[k] = json!("<masked>");
                    }
                }
            }
            let mut text: Vec<String> = rows.iter().map(|r| r.to_string()).collect();
            text.sort_by_key(|r| uuid_re().replace_all(r, "<uuid>").into_owned());
            let mut tokenized: Vec<String> = text.iter().map(|r| self.tokenize(r)).collect();
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
        Value::Object(tables)
    }
}

// ---------------------------------------------------------------------------
// The lines
// ---------------------------------------------------------------------------

/// `(compared lines, the [InventoryPool] tags seen)` off v4's log list.
fn v4_lines(logs: &Value) -> (Vec<String>, BTreeSet<String>) {
    let mut lines = Vec::new();
    let mut tags = BTreeSet::new();
    for log in logs.as_array().unwrap() {
        let message = log["message"].as_str().unwrap();
        if log["level"] == "marker" {
            lines.push(MARKER.to_string());
            continue;
        }
        if message.starts_with("[InventoryPool]") {
            tags.insert(message.to_string());
        }
        if !compared(message) {
            continue;
        }
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
        let line = common::v4_log_as_capture_line(
            "T",
            &json!({ "level": log["level"], "message": message, "fields": Value::Object(fields) }),
        );
        lines.push(line.replacen(" T ", " ", 1));
    }
    (lines, tags)
}

fn v5_lines(captured: &[String]) -> (Vec<String>, BTreeSet<String>) {
    let mut lines = Vec::new();
    let mut tags = BTreeSet::new();
    for l in captured {
        if l == MARKER {
            lines.push(l.clone());
            continue;
        }
        let mut parts = l.splitn(3, ' ');
        let (level, _target, rest) = (
            parts.next().unwrap(),
            parts.next().unwrap_or(""),
            parts.next().unwrap_or(""),
        );
        if rest.starts_with("[InventoryPool]") {
            let head = rest
                .split(" context=")
                .next()
                .unwrap_or(rest)
                .split(' ')
                .take_while(|w| !w.contains('='))
                .collect::<Vec<_>>()
                .join(" ");
            tags.insert(head);
        }
        let message_compared = rest.starts_with("[InventoryTitleLint]")
            || rest.starts_with("[ToolGateKeys]")
            || REPO_LINES
                .iter()
                .any(|m| rest == *m || rest.starts_with(&format!("{m} ")));
        if message_compared {
            lines.push(format!("{level} {rest}"));
        }
    }
    (lines, tags)
}

// ---------------------------------------------------------------------------
// v5's answers
// ---------------------------------------------------------------------------

fn refusal_json(r: &InventoryRefusal) -> Value {
    json!({
        "name": match r.code {
            InventoryRefusalCode::TitleShadow => "InventoryTitleShadowError",
            InventoryRefusalCode::TitleCheckUnavailable => "InventoryTitleCheckUnavailableError",
            _ => "InventoryRefusal",
        },
        "message": r.message,
        "code": r.code.as_str(),
        "status": r.status,
        "details": Value::Object(r.details.clone()),
    })
}

fn store_error_json(e: &StoreError) -> Value {
    match e {
        StoreError::TitleLint(r) => refusal_json(r),
        StoreError::Store(s) => json!({ "name": "DatabaseStoreError", "message": s.message }),
        StoreError::Db(d) => json!({ "name": "Error", "message": error_text(d) }),
    }
}

fn collision_of(v: &Value) -> InventoryTitleCollision {
    match v["reason"].as_str().unwrap() {
        "kind-name shadow" => InventoryTitleCollision::KindNameShadow {
            kind_name: v["kindName"].as_str().unwrap().to_string(),
        },
        _ => InventoryTitleCollision::LiveGateShadow {
            tool_name: v["toolName"].as_str().unwrap().to_string(),
            reference: v["ref"].as_str().unwrap().to_string(),
            lands_on: v["landsOn"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s.as_str().unwrap().to_string())
                .collect(),
        },
    }
}

fn census_of(v: &Value) -> ToolGateKeyCensus {
    let s = |x: &Value| x.as_str().unwrap().to_string();
    ToolGateKeyCensus {
        keys: v["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| ToolGateKeyUse {
                reference: s(&k["ref"]),
                tool_name: s(&k["toolName"]),
                clause: if k["clause"] == "availableWhen" {
                    GateKeyClause::AvailableWhen
                } else {
                    GateKeyClause::WithheldWhen
                },
                subject: if k["subject"] == "inventory" {
                    GateKeySubject::Inventory
                } else {
                    GateKeySubject::InventoryCount
                },
                mount_name: s(&k["mountName"]),
                definition_path: s(&k["definitionPath"]),
            })
            .collect(),
        unread: v["unread"]
            .as_array()
            .unwrap()
            .iter()
            .map(|u| ToolGateKeyUnread {
                mount_name: s(&u["mountName"]),
                definition_path: s(&u["definitionPath"]),
                reason: s(&u["reason"]),
            })
            .collect(),
        stores_consulted: v["storesConsulted"]
            .as_array()
            .unwrap()
            .iter()
            .map(s)
            .collect(),
    }
}

fn door_of(s: &str) -> TitleLintDoor {
    match s {
        "create" => TitleLintDoor::Create,
        "update" => TitleLintDoor::Update,
        "archive" => TitleLintDoor::Archive,
        "delete" => TitleLintDoor::Delete,
        other => panic!("door {other}"),
    }
}

struct Pair<'a> {
    main: &'a rusqlite::Connection,
    mount: &'a rusqlite::Connection,
    user_id: &'a str,
    mounts: HashMap<String, String>,
}

impl Pair<'_> {
    fn loc(&self, at: &Value) -> InventoryLocation {
        let scope = InventoryScope::parse(at["scope"].as_str().unwrap()).unwrap();
        resolve_inventory_location(
            self.main,
            self.mount,
            scope,
            at["id"].as_str(),
            self.user_id,
            ResolveInventoryLocationOptions { ensure: false },
        )
        .unwrap()
        .unwrap_or_else(|| panic!("no location for {at}"))
    }

    fn mp(&self, s: &str) -> String {
        let re = Regex::new(r"\{\{mp:(\w+)\}\}").unwrap();
        re.replace_all(s, |c: &regex::Captures| self.mounts[&c[1]].clone())
            .into_owned()
    }

    fn side(&self, l: &InventoryLocation, r: &Value) -> Option<InventoryRecord> {
        if r.is_null() {
            return None;
        }
        if let Some(rec) = r.get("record") {
            return Some(InventoryRecord(rec.as_object().unwrap().clone()));
        }
        let mut found = l
            .find_item(self.mount, r["id"].as_str().unwrap())
            .unwrap()
            .unwrap_or_else(|| panic!("no item {}", r["id"]));
        for (k, v) in r
            .get("patch")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            found.set(k, v.clone());
        }
        for k in r
            .get("remove")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            found.remove(k.as_str().unwrap());
        }
        Some(found)
    }

    fn step(&self, step: &Value) {
        match step["do"].as_str().unwrap() {
            "sql" => {
                let conn = if step["db"] == "main" {
                    self.main
                } else {
                    self.mount
                };
                conn.execute_batch(step["sql"].as_str().unwrap()).unwrap();
            }
            "plant" => {
                let l = self.loc(&step["at"]);
                let rec = InventoryRecord(step["record"].as_object().unwrap().clone());
                l.create(self.main, self.mount, &rec).unwrap();
            }
            "doc-plant" => {
                write_database_document(
                    self.mount,
                    &self.mp(step["mp"].as_str().unwrap()),
                    step["path"].as_str().unwrap(),
                    step["content"].as_str().unwrap(),
                    TitleLintScope::Linted,
                )
                .unwrap();
            }
            other => panic!("step {other}"),
        }
    }

    /// One op, on v5. Answers the outcome and the captured lines (a doc op's
    /// marker line between the lint and its rows).
    fn run(&self, op: &Value) -> (Value, Vec<String>) {
        match op["kind"].as_str().unwrap() {
            "lint" => {
                let l = self.loc(&op["at"]);
                let before = self.side(&l, &op["before"]);
                let after = self.side(&l, &op["after"]);
                let census_spec = op["census"].clone();
                let census = move || match census_spec.get("throw") {
                    Some(t) => Err(t.as_str().unwrap().to_string()),
                    None => Ok(census_of(&census_spec)),
                };
                let project_id = op["context"]["projectId"].as_str().map(str::to_string);
                let (out, lines) = captured_with(|| {
                    let ctx = LoadPoolContext {
                        chat_id: None,
                        project_id: project_id.as_deref(),
                    };
                    let pool = op.get("poolUnread").map(|unread| {
                        let mut p = load_pool_for_location(self.main, self.mount, &l, ctx).unwrap();
                        for u in unread.as_array().unwrap() {
                            p.unread_mounts.push(UnreadInventoryMount {
                                mount_point_id: self.mp(u["mountPointId"].as_str().unwrap()),
                                error: u["error"].as_str().unwrap().to_string(),
                            });
                        }
                        p
                    });
                    lint_inventory_title(
                        self.main,
                        self.mount,
                        &l,
                        TitleLintRequest {
                            door: door_of(op["door"].as_str().unwrap()),
                            before: before.as_ref(),
                            after: after.as_ref(),
                        },
                        TitleLintOptions {
                            pool: pool.as_ref(),
                            context: ctx,
                            census: Some(&census),
                        },
                    )
                });
                let outcome = match out {
                    Ok(v) => json!({ "result": serde_json::to_string(&v.to_value()).unwrap() }),
                    Err(r) => json!({ "error": refusal_json(&r) }),
                };
                (outcome, lines)
            }
            "doc" => {
                let c = &op["change"];
                let g = |k: &str| self.mp(c[k].as_str().unwrap_or(""));
                let (mp, rel, content, from, to) = (
                    g("mountPointId"),
                    g("relativePath"),
                    g("content"),
                    g("fromPath"),
                    g("toPath"),
                );
                let change = match c["kind"].as_str().unwrap() {
                    "write" => InventoryDocumentChange::Write {
                        mount_point_id: &mp,
                        relative_path: &rel,
                        content: &content,
                    },
                    "delete" => InventoryDocumentChange::Delete {
                        mount_point_id: &mp,
                        relative_path: &rel,
                    },
                    "move" => InventoryDocumentChange::Move {
                        mount_point_id: &mp,
                        from_path: &from,
                        to_path: &to,
                    },
                    _ => InventoryDocumentChange::MoveFolder {
                        mount_point_id: &mp,
                        from_path: &from,
                        to_path: &to,
                    },
                };
                let scope = if op["linted"] == true {
                    TitleLintScope::Linted
                } else {
                    TitleLintScope::Unlinted { main: self.main }
                };
                let (pending, mut lines) =
                    captured_with(|| lint_inventory_document_change(self.mount, &change, &scope));
                match pending {
                    Ok(pending) => {
                        lines.push(MARKER.to_string());
                        let ((), after) = captured_with(|| pending.write());
                        lines.extend(after);
                        (json!({ "result": r#"{"written":true}"# }), lines)
                    }
                    Err(r) => (json!({ "error": refusal_json(&r) }), lines),
                }
            }
            "store" => {
                let a = &op["args"];
                let g = |k: &str| self.mp(a[k].as_str().unwrap_or(""));
                let scope = if op["linted"] == true {
                    TitleLintScope::Linted
                } else {
                    TitleLintScope::Unlinted { main: self.main }
                };
                let (out, lines) = captured_with(|| match op["fn"].as_str().unwrap() {
                    "write" => write_database_document(
                        self.mount,
                        &g("mountPointId"),
                        &g("relativePath"),
                        &g("content"),
                        scope,
                    )
                    .map(|_| r#"{"written":true}"#.to_string()),
                    "delete" => delete_database_document(
                        self.mount,
                        &g("mountPointId"),
                        &g("relativePath"),
                        scope,
                    )
                    .map(|d| format!(r#"{{"deleted":{d}}}"#)),
                    "move" => move_database_document(
                        self.mount,
                        &g("mountPointId"),
                        &g("fromPath"),
                        &g("toPath"),
                        scope,
                    )
                    .map(|()| r#"{"moved":true}"#.to_string()),
                    _ => move_database_folder(
                        self.mount,
                        &g("mountPointId"),
                        &g("fromPath"),
                        &g("toPath"),
                        scope,
                    )
                    .map(|()| r#"{"moved":true}"#.to_string()),
                });
                let outcome = match out {
                    Ok(text) => json!({ "result": text }),
                    Err(e) => json!({ "error": store_error_json(&e) }),
                };
                (outcome, lines)
            }
            other => panic!("op kind {other}"),
        }
    }
}

/// A carved row's branch answer: the fail-closed stand-in (R-3 / R-4) — a
/// `TITLE_CHECK_UNAVAILABLE` naming the unresolvable key or the keyed file,
/// or a `gate unverifiable` row.
fn is_branch_stand_in(outcome: &Value) -> bool {
    let text = outcome.to_string();
    text.contains("not_available: resolve_inventory_ref")
        || text.contains("TITLE_CHECK_UNAVAILABLE")
        || text.contains("gate unverifiable")
        || outcome["result"] == r#"{"written":true}"#
}

#[test]
fn inventory_title_lint_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_INV_TITLE_LINT") else {
        eprintln!(
            "SKIP: set QT_ORACLE_INV_TITLE_LINT (and QT_FIXTURE_ITL_MAIN / _MOUNT) — see header."
        );
        return;
    };
    let spec_text = std::fs::read_to_string(spec_path()).unwrap();
    let spec: Value = serde_json::from_str(&spec_text).unwrap();
    let rows: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let by_op: HashMap<&str, &Value> = rows
        .iter()
        .map(|r| (r["op"].as_str().unwrap(), r))
        .collect();
    let mut failures: Vec<String> = Vec::new();

    // Tier 1 — the path predicates.
    let paths = spec["paths"].as_array().unwrap();
    for (i, p) in paths.iter().enumerate() {
        let p = p.as_str().unwrap();
        let want = by_op[format!("path-{i}").as_str()];
        let got = json!({
            "tool": is_tool_definition_path(p),
            "folder": is_title_lint_folder(p),
            "lint": is_title_lint_path(p),
        });
        let want = json!({ "tool": want["tool"], "folder": want["folder"], "lint": want["lint"] });
        if got != want {
            failures.push(format!("path-{i} {p:?}\n  v5: {got}\n  v4: {want}"));
        }
    }
    // Tier 1 — normalize_title.
    let norms = spec["norms"].as_array().unwrap();
    for (i, n) in norms.iter().enumerate() {
        let want = by_op[format!("norm-{i}").as_str()];
        let got = normalize_title(n["v"].as_str());
        if want["out"].as_str() != Some(got.as_str()) {
            failures.push(format!(
                "norm-{i} {:?}\n  v5: {got:?}\n  v4: {:?}",
                n["v"], want["out"]
            ));
        }
    }
    // Tier 1 — the two refusal messages (details as BYTES).
    let messages = spec["messages"].as_array().unwrap();
    for (i, m) in messages.iter().enumerate() {
        let want = &by_op[format!("msg-{i}").as_str()]["error"];
        let title = m["title"].as_str().unwrap();
        let r = if m["kind"] == "shadow" {
            let c: Vec<InventoryTitleCollision> = m["collisions"]
                .as_array()
                .unwrap()
                .iter()
                .map(collision_of)
                .collect();
            InventoryRefusal::title_shadow(title, &c)
        } else {
            let u: Vec<InventoryTitleLintUnread> = m["unread"]
                .as_array()
                .unwrap()
                .iter()
                .map(|u| InventoryTitleLintUnread {
                    pool: u["pool"].as_str().unwrap().to_string(),
                    reason: u["reason"].as_str().unwrap().to_string(),
                })
                .collect();
            InventoryRefusal::title_check_unavailable(title, &u)
        };
        let got = serde_json::to_string(&refusal_json(&r)).unwrap();
        let want = serde_json::to_string(want).unwrap();
        if got != want {
            failures.push(format!("msg-{i}\n  v5: {got}\n  v4: {want}"));
        }
    }

    // The DB ops.
    let (Ok(fixture_main), Ok(fixture_mount)) = (
        std::env::var("QT_FIXTURE_ITL_MAIN"),
        std::env::var("QT_FIXTURE_ITL_MOUNT"),
    ) else {
        panic!("QT_ORACLE_INV_TITLE_LINT is set but QT_FIXTURE_ITL_MAIN / _MOUNT are not");
    };
    let pepper = spec["testPepperBase64"].as_str().unwrap();
    let user_id = spec["userId"].as_str().unwrap();
    let known: HashSet<String> = {
        let mount = Writer::open_writable(Path::new(&fixture_mount), pepper).unwrap();
        let mut texts: Vec<String> = dump_all(mount.connection())
            .iter()
            .flatten()
            .map(|r| r.to_string())
            .collect();
        texts.extend(
            dump_table(mount.connection(), "doc_mount_points")
                .iter()
                .map(|r| r.to_string()),
        );
        texts.push(spec_text.clone());
        let mut k = HashSet::new();
        for t in &texts {
            k.extend(uuid_re().find_iter(t).map(|m| m.as_str().to_string()));
            k.extend(ts_re().find_iter(t).map(|m| m.as_str().to_string()));
        }
        k
    };
    let carved: HashMap<&str, &str> = CARVED.iter().copied().collect();
    let mut ran = 0;
    let mut carved_pinned = 0;
    let mut lines_compared = 0;
    for op in spec["ops"].as_array().unwrap() {
        let name = op["name"].as_str().unwrap();
        let want = by_op[name];
        let dir = tempfile::tempdir().unwrap();
        let (main_path, mount_path) = (dir.path().join("main.db"), dir.path().join("mount.db"));
        std::fs::copy(&fixture_main, &main_path).unwrap();
        std::fs::copy(&fixture_mount, &mount_path).unwrap();
        let main_w = Writer::open_writable(&main_path, pepper).unwrap();
        let mount_w = Writer::open_writable(&mount_path, pepper).unwrap();
        let (main, mount) = (main_w.connection(), mount_w.connection());
        let mut pair = Pair {
            main,
            mount,
            user_id,
            mounts: HashMap::new(),
        };
        for (k, at) in [
            ("general", json!({ "scope": "general" })),
            (
                "project",
                json!({ "scope": "project", "id": spec["project"]["id"] }),
            ),
            (
                "group",
                json!({ "scope": "group", "id": spec["group"]["id"] }),
            ),
            (
                "friday",
                json!({ "scope": "character", "id": spec["friday"]["id"] }),
            ),
        ] {
            let mp = pair.loc(&at).mount_point_id;
            assert_eq!(
                want["mounts"][k].as_str(),
                Some(mp.as_str()),
                "{name}: the {k} store resolves to a different mount on v5"
            );
            pair.mounts.insert(k.to_string(), mp);
        }
        if op["tools"] != true {
            let gone = delete_database_document(
                mount,
                &pair.mounts["general"],
                "Tools/measure_twice.tool.json",
                TitleLintScope::Linted,
            )
            .unwrap();
            assert!(gone, "{name}: the keyed tool was not there to strip");
        }
        for step in op["steps"].as_array().unwrap() {
            pair.step(step);
        }
        let before = dump_all(mount);
        let (outcome, captured) = pair.run(op);
        let after = dump_all(mount);
        let got_delta = delta(&before, &after);

        let want_outcome = match want.get("error") {
            Some(e) => json!({ "error": e }),
            None => json!({ "result": want["result"] }),
        };
        if let Some(carve) = carved.get(name) {
            assert!(
                is_branch_stand_in(&outcome),
                "{name} ({carve}): v5's branch answer is not the fail-closed stand-in: {outcome}"
            );
            carved_pinned += 1;
            continue;
        }
        let (want_lines, want_tags) = v4_lines(&want["logs"]);
        let (got_lines, got_tags) = v5_lines(&captured);
        lines_compared += want_lines.len();
        let mut gn = Norm::new(&known);
        let mut wn = Norm::new(&known);
        let g = (
            gn.tokenize(&outcome.to_string()),
            got_lines.iter().map(|l| gn.tokenize(l)).collect::<Vec<_>>(),
            gn.delta(&got_delta),
        );
        let w = (
            wn.tokenize(&want_outcome.to_string()),
            want_lines
                .iter()
                .map(|l| wn.tokenize(l))
                .collect::<Vec<_>>(),
            wn.delta(&want["delta"]),
        );
        if g.0 != w.0 {
            failures.push(format!("{name} OUTCOME\n  v5: {}\n  v4: {}", g.0, w.0));
        }
        if g.1 != w.1 {
            failures.push(format!("{name} LINES\n  v5: {:#?}\n  v4: {:#?}", g.1, w.1));
        }
        if got_tags != want_tags {
            failures.push(format!(
                "{name} [InventoryPool] TAGS\n  v5: {got_tags:?}\n  v4: {want_tags:?}"
            ));
        }
        if g.2 != w.2 {
            failures.push(format!("{name} DELTA\n  v5: {:#}\n  v4: {:#}", g.2, w.2));
        }
        ran += 1;
    }
    eprintln!(
        "inventory title lint: {} paths, {} norms, {} messages, {ran} ops compared, {carved_pinned} carved (pinned to the branch stand-in), {lines_compared} v4 lines",
        paths.len(),
        norms.len(),
        messages.len()
    );
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
