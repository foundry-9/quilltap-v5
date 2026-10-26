//! P4.D325 INVENTORY-ROUTES differential: the `inventory*` + `chatInventory*`
//! verb family (`api::inventory_route_factory`, `api::inventory`,
//! `api::chat_inventory`) and its REST edges vs v4's REAL route handlers at the
//! `322a5488e` pin (the route factory through its four real configs, the
//! transfers route, `actions/inventory.ts` through `handlePost`), each case
//! over a FRESH copy of the committed `inventory-routes-{main,mount}.db` pair.
//!
//! THE CASE LIST LIVES IN `inventory-routes.json#cases` — the SAME corpus the
//! oracle reads (the shared-corpus rule); the row count is asserted against the
//! NDJSON, so a half-regenerated oracle cannot partial-pass.
//!
//! v5 runs TWO ways per case: `direct` — the case as its dispatch wire
//! (`{type: "inventoryList", container: {scope, id}, …}`, deserialized into the
//! real `Request`, so C7's spellings are exercised) handed to the core fns the
//! engine arms call; and `http` — the REAL router over `common::serve_instance`
//! (`on_lock_lost` no-op), on v4's documented REST paths. Comparands: the
//! status (R-11: v4's `created(...)` is 201 on REST, every dispatch success
//! 200), the body as BYTES (key order is the contract — never `Value ==`,
//! memory note `serde-json-map-eq-ignores-order`), and the chained rows —
//! `${name}__read` (the chat's `?action=inventory` on the same copy) and
//! `${name}__dump` (the store after: `doc_mount_points` count, every
//! `Inventory/` file's path + content, the `Inventory` folders, a chat's
//! `equippedInventory` / `chat_messages` count / `background_jobs` count).
//!
//! Normalization is generic and applied to BOTH sides alike: a UUID the
//! committed pair does not contain is minted (first-seen tokens per case), a
//! timestamp the pair does not contain is `<now>`.
//!
//! Named carves (both-ways — each asserts v4 HAS the difference and v5 does
//! NOT, so it reddens the moment the owner lands):
//!   - `EFFECTS_UNWIRED_UNTIL_UNION` (R-14): the operator gesture's effects
//!     (Pascal's `chat_messages` row) are P4.D328's `notify_inventory_changed`;
//!     on this branch `ProductionInventoryEffects` is the loud unwired arm. The
//!     unifier lifts it with the one-line wiring.
//!   - `TRANSFER_ERROR_SHAPES_P4D328` (R-T): the transfers module's
//!     id-collision arm is P4.D328's shape; this branch asserts v4's status only.
//!
//! Generate the oracle (see `harness/oracle/cases/inventory-routes.test.ts`),
//! then:
//!   QT_ORACLE_INVENTORY_ROUTES=/tmp/oracle-inventory-routes.ndjson \
//!     cargo test -p quilltap-web --test inventory_routes_equivalence -- --nocapture

mod common;
#[path = "support/inventory_norm.rs"]
mod inventory_norm;

use inventory_norm::{known_of, normalize, Known};

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::http::StatusCode;
use quilltap_core::api::chat_inventory::{
    chat_inventory_equip, chat_inventory_get, chat_inventory_reset_defaults, EquipMode,
    ProductionInventoryEffects,
};
use quilltap_core::api::inventory::inventory_transfer_apply;
use quilltap_core::api::inventory_route_factory::{
    collection_action, item_action, CollectionAction, ItemAction,
};
use quilltap_core::api::types::Response;
use quilltap_core::api::{Request, SINGLE_USER_ID};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::{json, Map, Value};

/// R-14: rows whose v4 dump carries the operator gesture's effects (Pascal's
/// line) that this branch's unwired seam does not write.
const EFFECTS_UNWIRED_UNTIL_UNION: &[&str] = &[
    "chat_equip__dump",
    "chat_equip_already__dump",
    "chat_equip_kit__dump",
    "chat_equip_user_seat__dump",
    "chat_equip_operator_off_roster__dump",
    "chat_unequip__dump",
];

/// R-T: the transfers module's error shapes P4.D328 moves — v4's status only.
/// EMPTY on this pair, measured: the one object whose id a second vault also
/// claims (`vSword`) is a kit member, so v4 answers `KIT_STRANDED` before the
/// collision arm (`transfer_id_collision` compares whole); the source-delete
/// 500 needs a failing store. Both arms stay P4.D328's to pin.
const TRANSFER_ERROR_SHAPES_P4D328: &[&str] = &[];

/// Tier 3 item 24 — v4's chat POST reads the chat BEFORE its action map
/// (`handlers/post.ts:78-81`), so a bad body on a MISSING chat is v4's 404
/// where v5 (whose edge gates the action without a lookup, and whose handler
/// keeps `actions/inventory.ts`'s own order: parse, then the chat) answers the
/// Zod 400. The standing recorded gate-ORDER divergence
/// (`wardrobe_routes.rs`'s `chat_action_post`); both ways.
const CHAT_GATE_ORDER: &[&str] = &["chat_equip_zod_before_chat"];

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Case {
    name: String,
    kind: String,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    item_id: Option<String>,
    #[serde(default)]
    chat_id: Option<String>,
    #[serde(default)]
    action: Option<String>,
    #[serde(default)]
    query: Vec<(String, String)>,
    #[serde(default)]
    body: Option<Value>,
    #[serde(default)]
    raw_body: Option<String>,
    #[serde(default)]
    http_only: bool,
    #[serde(default)]
    dump: Option<Value>,
    #[serde(default)]
    then_read: Option<String>,
    #[serde(default)]
    main_sql: Vec<String>,
    /// A case only dispatch can reach: the served host's boot provisions
    /// Quilltap General, so v4's unprovisioned-General arm has no REST row.
    #[serde(default)]
    direct_only: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    cases: Vec<Case>,
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}
fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/inventory-routes.json")
}
fn load_spec() -> Spec {
    serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap()
}

// ===========================================================================
// Normalization — minted UUIDs and fresh timestamps, on both sides alike
// ===========================================================================

// ===========================================================================
// The two v5 drivers
// ===========================================================================

/// One emitted row: `(status, body)`.
type Row = (u16, Value);

fn container_wire(case: &Case) -> Value {
    let mut c = Map::new();
    c.insert(
        "scope".into(),
        json!(case.scope.as_deref().unwrap_or("general")),
    );
    if let Some(id) = &case.id {
        c.insert("id".into(), json!(id));
    }
    Value::Object(c)
}

fn q<'a>(case: &'a Case, key: &str) -> Option<&'a str> {
    case.query
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

/// The case as its dispatch wire (C7's spellings), `None` for a case only REST
/// can express.
fn dispatch_wire(case: &Case) -> Option<Value> {
    if case.http_only {
        return None;
    }
    let mut w = Map::new();
    let set = |w: &mut Map<String, Value>, k: &str, v: Value| {
        w.insert(k.to_string(), v);
    };
    let flatten = |w: &mut Map<String, Value>, body: &Option<Value>| {
        if let Some(Value::Object(o)) = body {
            for (k, v) in o {
                w.insert(k.clone(), v.clone());
            }
        }
    };
    let opt_q = |w: &mut Map<String, Value>, wire: &str, key: &str| {
        if let Some(v) = q(case, key) {
            w.insert(wire.to_string(), json!(v));
        }
    };
    match (case.kind.as_str(), case.action.as_deref()) {
        ("collectionGet", None) => {
            set(&mut w, "type", json!("inventoryList"));
            set(&mut w, "container", container_wire(case));
            let ia = matches!(q(case, "includeArchived"), Some("true") | Some(""));
            if ia {
                set(&mut w, "includeArchived", json!(true));
            }
            if q(case, "shared") == Some("true") {
                set(&mut w, "shared", json!(true));
            }
            opt_q(&mut w, "projectId", "projectId");
            opt_q(&mut w, "chatId", "chatId");
        }
        ("collectionGet", Some("folders")) => {
            set(&mut w, "type", json!("inventoryFolders"));
            set(&mut w, "container", container_wire(case));
        }
        ("collectionGet", Some("archetypes")) => {
            set(&mut w, "type", json!("inventoryArchetypes"));
            set(&mut w, "container", container_wire(case));
            opt_q(&mut w, "projectId", "projectId");
            opt_q(&mut w, "chatId", "chatId");
        }
        ("collectionPost", None) => {
            set(&mut w, "type", json!("inventoryCreate"));
            set(&mut w, "container", container_wire(case));
            opt_q(&mut w, "projectId", "projectId");
            opt_q(&mut w, "chatId", "chatId");
            set(&mut w, "body", case.body.clone().unwrap_or(json!({})));
        }
        ("collectionPost", Some(action)) => {
            let ty = match action {
                "instantiate" => "inventoryInstantiate",
                "create-folder" => "inventoryFolderCreate",
                "rename-folder" => "inventoryFolderRename",
                "delete-folder" => "inventoryFolderDelete",
                "move-items" => "inventoryItemsMove",
                "copy-items" => "inventoryItemsCopy",
                "duplicate" => "inventoryDuplicate",
                other => panic!("{}: no verb for {other}", case.name),
            };
            set(&mut w, "type", json!(ty));
            set(&mut w, "container", container_wire(case));
            flatten(&mut w, &case.body);
        }
        ("itemGet", action) => {
            let ty = match action {
                None => "inventoryItemGet",
                Some("dependents") => "inventoryDependents",
                Some(other) => panic!("{}: no verb for {other}", case.name),
            };
            set(&mut w, "type", json!(ty));
            set(&mut w, "container", container_wire(case));
            set(&mut w, "itemId", json!(case.item_id));
        }
        ("itemPut", action) => {
            set(&mut w, "container", container_wire(case));
            set(&mut w, "itemId", json!(case.item_id));
            match action {
                None => {
                    set(&mut w, "type", json!("inventoryUpdate"));
                    opt_q(&mut w, "projectId", "projectId");
                    opt_q(&mut w, "chatId", "chatId");
                    set(&mut w, "body", case.body.clone().unwrap_or(json!({})));
                }
                Some("clear-override") => {
                    set(&mut w, "type", json!("inventoryClearOverride"));
                    flatten(&mut w, &case.body);
                }
                Some("detach") => set(&mut w, "type", json!("inventoryDetach")),
                Some(other) => panic!("{}: no verb for {other}", case.name),
            }
        }
        ("itemDelete", _) => {
            set(&mut w, "type", json!("inventoryDelete"));
            set(&mut w, "container", container_wire(case));
            set(&mut w, "itemId", json!(case.item_id));
            if q(case, "force") == Some("true") {
                set(&mut w, "force", json!(true));
            }
        }
        ("transfer", _) => {
            set(&mut w, "type", json!("inventoryTransfer"));
            flatten(&mut w, &case.body);
        }
        ("chatGet", _) => {
            set(&mut w, "type", json!("chatInventoryGet"));
            set(&mut w, "chatId", json!(case.chat_id));
        }
        ("chatPost", Some(action)) => {
            let ty = match action {
                "inventory-equip" => "chatInventoryEquip",
                "inventory-unequip" => "chatInventoryUnequip",
                "inventory-reset-defaults" => "chatInventoryResetDefaults",
                other => panic!("{}: no verb for {other}", case.name),
            };
            set(&mut w, "type", json!(ty));
            set(&mut w, "chatId", json!(case.chat_id));
            flatten(&mut w, &case.body);
        }
        (kind, action) => panic!("{}: unhandled {kind} / {action:?}", case.name),
    }
    Some(Value::Object(w))
}

/// The engine arms' calls, for the variants this family drives.
async fn run_direct(db: &Db, req: Request) -> Response {
    let user = SINGLE_USER_ID;
    match req {
        Request::InventoryList {
            container,
            include_archived,
            shared,
            project_id,
            chat_id,
        } => {
            collection_action(
                db,
                CollectionAction::List {
                    include_archived,
                    shared,
                    project_id,
                    chat_id,
                },
                &container,
                user,
            )
            .await
        }
        Request::InventoryFolders { container } => {
            collection_action(db, CollectionAction::Folders, &container, user).await
        }
        Request::InventoryArchetypes {
            container,
            project_id,
            chat_id,
        } => {
            collection_action(
                db,
                CollectionAction::Archetypes {
                    project_id,
                    chat_id,
                },
                &container,
                user,
            )
            .await
        }
        Request::InventoryCreate {
            container,
            project_id,
            chat_id,
            body,
        } => {
            collection_action(
                db,
                CollectionAction::Create {
                    project_id,
                    chat_id,
                    body,
                },
                &container,
                user,
            )
            .await
        }
        Request::InventoryInstantiate { container, body } => {
            collection_action(db, CollectionAction::Instantiate { body }, &container, user).await
        }
        Request::InventoryDuplicate { container, body } => {
            collection_action(db, CollectionAction::Duplicate { body }, &container, user).await
        }
        Request::InventoryFolderCreate { container, body } => {
            collection_action(
                db,
                CollectionAction::FolderCreate { body },
                &container,
                user,
            )
            .await
        }
        Request::InventoryFolderRename { container, body } => {
            collection_action(
                db,
                CollectionAction::FolderRename { body },
                &container,
                user,
            )
            .await
        }
        Request::InventoryFolderDelete { container, body } => {
            collection_action(
                db,
                CollectionAction::FolderDelete { body },
                &container,
                user,
            )
            .await
        }
        Request::InventoryItemsMove { container, body } => {
            collection_action(db, CollectionAction::ItemsMove { body }, &container, user).await
        }
        Request::InventoryItemsCopy { container, body } => {
            collection_action(db, CollectionAction::ItemsCopy { body }, &container, user).await
        }
        Request::InventoryItemGet { container, item_id } => {
            item_action(db, ItemAction::Get, &container, &item_id, user).await
        }
        Request::InventoryDependents { container, item_id } => {
            item_action(db, ItemAction::Dependents, &container, &item_id, user).await
        }
        Request::InventoryUpdate {
            container,
            item_id,
            project_id,
            chat_id,
            body,
        } => {
            item_action(
                db,
                ItemAction::Update {
                    project_id,
                    chat_id,
                    body,
                },
                &container,
                &item_id,
                user,
            )
            .await
        }
        Request::InventoryClearOverride {
            container,
            item_id,
            body,
        } => {
            item_action(
                db,
                ItemAction::ClearOverride { body },
                &container,
                &item_id,
                user,
            )
            .await
        }
        Request::InventoryDetach { container, item_id } => {
            item_action(db, ItemAction::Detach, &container, &item_id, user).await
        }
        Request::InventoryDelete {
            container,
            item_id,
            force,
        } => item_action(db, ItemAction::Delete { force }, &container, &item_id, user).await,
        Request::InventoryTransfer { body } => inventory_transfer_apply(db, user, body).await,
        Request::ChatInventoryGet { chat_id } => chat_inventory_get(db, &chat_id),
        Request::ChatInventoryEquip { chat_id, body } => {
            chat_inventory_equip(
                db,
                user,
                &chat_id,
                &body,
                EquipMode::Equip,
                &ProductionInventoryEffects,
            )
            .await
        }
        Request::ChatInventoryUnequip { chat_id, body } => {
            chat_inventory_equip(
                db,
                user,
                &chat_id,
                &body,
                EquipMode::Unequip,
                &ProductionInventoryEffects,
            )
            .await
        }
        Request::ChatInventoryResetDefaults { chat_id, body } => {
            chat_inventory_reset_defaults(db, user, &chat_id, &body).await
        }
        other => panic!("not an inventory verb: {other:?}"),
    }
}

fn row_of(resp: Response) -> Row {
    let (status, body) = quilltap_web::inventory_routes::render(resp, StatusCode::OK);
    (status.as_u16(), body)
}

/// Copy the pair into `dir` (as `main` / `mount` file names), run the case's
/// `mainSql`, and plant P4.D315's one v5-side table (the oracle plants v4's).
fn materialize(dir: &Path, main_name: &str, mount_name: &str, case: &Case, pepper: &str) {
    std::fs::copy(
        fixtures_dir().join("inventory-routes-main.db"),
        dir.join(main_name),
    )
    .unwrap();
    std::fs::copy(
        fixtures_dir().join("inventory-routes-mount.db"),
        dir.join(mount_name),
    )
    .unwrap();
    let w = Writer::open_writable(&dir.join(main_name), pepper).unwrap();
    for sql in &case.main_sql {
        w.connection()
            .execute_batch(sql)
            .unwrap_or_else(|e| panic!("{}: mainSql `{sql}`: {e}", case.name));
    }
    quilltap_core::test_support::ensure_chat_wardrobe_history_on(w.connection());
}

/// The store after a case — the oracle's `dumpStore` projection.
fn dump_store(db: &Db, case: &Case) -> Value {
    let chat = case
        .dump
        .as_ref()
        .and_then(|d| d.get("chat"))
        .and_then(Value::as_str)
        .map(str::to_string);
    db.read_main(|main| {
        db.read_mount_index(|mount| {
            let mut out = Map::new();
            let n: i64 = mount
                .query_row("SELECT COUNT(*) FROM doc_mount_points", [], |r| r.get(0))
                .unwrap();
            out.insert("mountPoints".into(), json!(n));
            let docs: Vec<Value> = mount
                .prepare(
                    r#"SELECT l."mountPointId", l."relativePath", d."content"
                         FROM doc_mount_file_links l JOIN doc_mount_documents d ON d."fileId" = l."fileId"
                        WHERE l."relativePath" LIKE 'Inventory/%' ORDER BY l."mountPointId", l."relativePath""#,
                )
                .unwrap()
                .query_map([], |r| {
                    Ok(json!([
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?
                    ]))
                })
                .unwrap()
                .filter_map(Result::ok)
                .collect();
            out.insert("docs".into(), Value::Array(docs));
            let folders: Vec<Value> = mount
                .prepare(
                    r#"SELECT "mountPointId", "path" FROM doc_mount_folders
                        WHERE "path" = 'Inventory' OR "path" LIKE 'Inventory/%' ORDER BY "mountPointId", "path""#,
                )
                .unwrap()
                .query_map([], |r| {
                    Ok(json!([r.get::<_, String>(0)?, r.get::<_, String>(1)?]))
                })
                .unwrap()
                .filter_map(Result::ok)
                .collect();
            out.insert("folders".into(), Value::Array(folders));
            if let Some(chat) = &chat {
                let e: Option<String> = main
                    .query_row(
                        r#"SELECT "equippedInventory" FROM chats WHERE id = ?1"#,
                        [chat],
                        |r| r.get(0),
                    )
                    .unwrap_or(None);
                out.insert("equipped".into(), json!(e));
                let count_of = |table: &str, sql: &str, p: &[&str]| -> Value {
                    let present: i64 = main
                        .query_row(
                            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                            [table],
                            |r| r.get(0),
                        )
                        .unwrap();
                    if present == 0 {
                        return Value::Null;
                    }
                    let n: i64 = main
                        .query_row(sql, rusqlite::params_from_iter(p.iter()), |r| r.get(0))
                        .unwrap();
                    json!(n)
                };
                out.insert(
                    "messages".into(),
                    count_of(
                        "chat_messages",
                        r#"SELECT COUNT(*) FROM "chat_messages" WHERE "chatId" = ?1"#,
                        &[chat],
                    ),
                );
                out.insert(
                    "jobs".into(),
                    count_of("background_jobs", r#"SELECT COUNT(*) FROM "background_jobs""#, &[]),
                );
            }
            Ok(Value::Object(out))
        })
    })
    .unwrap()
}

/// `direct`: the dispatch wire → the core fns, on a fresh copy.
async fn direct_rows(case: &Case, pepper: &str) -> Option<Vec<(String, Row)>> {
    let wire = dispatch_wire(case)?;
    let req: Request = serde_json::from_value(wire.clone()).unwrap_or_else(|e| {
        panic!(
            "{}: the dispatch wire {wire} does not parse: {e}",
            case.name
        )
    });
    let scratch = tempfile::Builder::new()
        .prefix(&format!("qt-iroutes-{}-", case.name))
        .tempdir()
        .unwrap();
    materialize(scratch.path(), "main.db", "mount.db", case, pepper);
    let db = Db::open(
        DbPaths {
            main: scratch.path().join("main.db"),
            mount_index: Some(scratch.path().join("mount.db")),
            llm_logs: None,
        },
        pepper,
    )
    .unwrap();
    let mut rows = vec![(case.name.clone(), row_of(run_direct(&db, req).await))];
    if let Some(chat) = &case.then_read {
        rows.push((
            format!("{}__read", case.name),
            row_of(chat_inventory_get(&db, chat)),
        ));
    }
    if case.dump.is_some() {
        rows.push((format!("{}__dump", case.name), (200, dump_store(&db, case))));
    }
    Some(rows)
}

fn url_encode(s: &str) -> String {
    s.bytes()
        .map(|c| match c {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'*' => {
                (c as char).to_string()
            }
            _ => format!("%{c:02X}"),
        })
        .collect()
}

/// The REST path + query (the oracle's `queryString`: `action` first).
fn rest_target(case: &Case) -> (String, String) {
    let tier = match case.scope.as_deref() {
        Some("general") => "inventory".to_string(),
        Some("character") => format!("characters/{}/inventory", case.id.as_deref().unwrap()),
        Some("group") => format!("groups/{}/inventory", case.id.as_deref().unwrap()),
        Some("project") => format!("projects/{}/inventory", case.id.as_deref().unwrap()),
        _ => String::new(),
    };
    let (path, method) = match case.kind.as_str() {
        "collectionGet" => (tier, "GET"),
        "collectionPost" => (tier, "POST"),
        "itemGet" => (
            format!("{tier}/{}", case.item_id.as_deref().unwrap()),
            "GET",
        ),
        "itemPut" => (
            format!("{tier}/{}", case.item_id.as_deref().unwrap()),
            "PUT",
        ),
        "itemDelete" => (
            format!("{tier}/{}", case.item_id.as_deref().unwrap()),
            "DELETE",
        ),
        "transfer" => ("inventory/transfers".to_string(), "POST"),
        "chatGet" => (format!("chats/{}", case.chat_id.as_deref().unwrap()), "GET"),
        "chatPost" => (
            format!("chats/{}", case.chat_id.as_deref().unwrap()),
            "POST",
        ),
        other => panic!("unhandled kind {other}"),
    };
    let mut pairs: Vec<(String, String)> = Vec::new();
    if case.kind == "chatGet" {
        pairs.push(("action".into(), "inventory".into()));
    }
    if let Some(a) = &case.action {
        pairs.push(("action".into(), a.clone()));
    }
    pairs.extend(case.query.iter().cloned());
    let qs = pairs
        .iter()
        .map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    let path = if qs.is_empty() {
        format!("/api/v1/{path}")
    } else {
        format!("/api/v1/{path}?{qs}")
    };
    (path, method.to_string())
}

/// `http`: the real router over a booted host, on v4's REST paths.
async fn http_rows(case: &Case, pepper: &str) -> Vec<(String, Row)> {
    let base = tempfile::tempdir().unwrap();
    let data = base.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    materialize(
        &data,
        "quilltap.db",
        "quilltap-mount-index.db",
        case,
        pepper,
    );
    // The host's boot provisions stores of its own (General's, …), so the
    // dump's `doc_mount_points` count is reported as the pair's own count
    // plus what the REQUEST added (the boot's stores excluded).
    let pristine: i64 = {
        let w = Writer::open_writable(&data.join("quilltap-mount-index.db"), pepper).unwrap();
        w.connection()
            .query_row("SELECT COUNT(*) FROM doc_mount_points", [], |r| r.get(0))
            .unwrap()
    };
    let pepper_owned = pepper.to_string();
    let (addr, state) = common::serve_instance(base.path(), move |mut c| {
        c.terminal = false;
        c.env_pepper = Some(pepper_owned);
        // The host outlives its temp dir; a lost lock here is the test's own
        // teardown, never v4's `process.exit(1)`.
        c.on_lock_lost = Some(Arc::new(|| {}));
        c
    })
    .await;
    let mount_points = |state: &quilltap_web::SharedState| -> i64 {
        let db = state.host().unwrap().core().db().unwrap();
        db.read_mount_index(|m| {
            Ok(
                m.query_row("SELECT COUNT(*) FROM doc_mount_points", [], |r| r.get(0))
                    .unwrap(),
            )
        })
        .unwrap()
    };
    let booted = mount_points(&state);
    let client = reqwest::Client::new();
    let (path, method) = rest_target(case);
    let url = format!("http://{addr}{path}");
    let mut rb = match method.as_str() {
        "GET" => client.get(&url),
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        _ => client.delete(&url),
    };
    if method != "GET" && method != "DELETE" {
        let body = case
            .raw_body
            .clone()
            .unwrap_or_else(|| case.body.clone().unwrap_or(json!({})).to_string());
        rb = rb.header("content-type", "application/json").body(body);
    }
    let resp = rb.send().await.unwrap();
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap();
    let body: Value = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("{}: non-JSON REST body {text:?}: {e}", case.name));
    let mut rows = vec![(case.name.clone(), (status, body))];
    if let Some(chat) = &case.then_read {
        let r = client
            .get(format!(
                "http://{addr}/api/v1/chats/{chat}?action=inventory"
            ))
            .send()
            .await
            .unwrap();
        let s = r.status().as_u16();
        let b: Value = serde_json::from_str(&r.text().await.unwrap()).unwrap();
        rows.push((format!("{}__read", case.name), (s, b)));
    }
    if case.dump.is_some() {
        let db = state.host().unwrap().core().db().unwrap();
        let mut dump = dump_store(&db, case);
        dump["mountPoints"] = json!(pristine + (mount_points(&state) - booted));
        rows.push((format!("{}__dump", case.name), (200, dump)));
    }
    rows
}

// ===========================================================================
// The comparison
// ===========================================================================

/// v4 statuses that are `created(...)` (R-11): REST answers 201, dispatch 200.
fn status_matches(driver: &str, v4: u16, v5: u16) -> bool {
    if driver == "direct" && v4 == 201 {
        return v5 == 200;
    }
    v4 == v5
}

fn drop_effects(v: &Value) -> Value {
    let mut v = v.clone();
    if let Some(o) = v.as_object_mut() {
        o.remove("messages");
        o.remove("jobs");
    }
    v
}

#[allow(clippy::too_many_arguments)]
fn compare(
    driver: &str,
    name: &str,
    want: &Value,
    got: &Row,
    known: &Known,
    v4_minted: &mut Vec<String>,
    v5_minted: &mut Vec<String>,
    failed: &mut Vec<String>,
) {
    let want_status = want["status"].as_u64().unwrap() as u16;
    let (got_status, got_body) = got;
    if CHAT_GATE_ORDER.contains(&name) {
        if !(want_status == 404 && *got_status == 400) {
            eprintln!("[{driver}:{name}] CHAT_GATE_ORDER no longer holds: v4 {want_status} v5 {got_status}");
            failed.push(format!("{driver}:{name}"));
        }
        return;
    }
    if !status_matches(driver, want_status, *got_status) {
        eprintln!("[{driver}:{name}] STATUS v4 {want_status} v5 {got_status}: {got_body}");
        failed.push(format!("{driver}:{name}"));
        return;
    }
    if TRANSFER_ERROR_SHAPES_P4D328.contains(&name) {
        // R-T: v4's status only — the body is P4.D328's shape.
        return;
    }
    let (want_body, got_body) = if EFFECTS_UNWIRED_UNTIL_UNION.contains(&name) {
        // Both ways: v4 wrote Pascal's line; this branch's seam did not.
        let v4_msgs = want["body"]["messages"].as_i64().unwrap_or(0);
        let v5_msgs = got_body["messages"].as_i64().unwrap_or(0);
        if v4_msgs <= v5_msgs {
            eprintln!(
                "[{driver}:{name}] EFFECTS_UNWIRED_UNTIL_UNION no longer holds (v4 {v4_msgs}, v5 {v5_msgs}) — lift the carve"
            );
            failed.push(format!("{driver}:{name}"));
            return;
        }
        (drop_effects(&want["body"]), drop_effects(got_body))
    } else {
        (want["body"].clone(), got_body.clone())
    };
    let w = normalize(
        &serde_json::to_string(&want_body).unwrap(),
        known,
        v4_minted,
    );
    let g = normalize(&serde_json::to_string(&got_body).unwrap(), known, v5_minted);
    if w != g {
        eprintln!("[{driver}:{name}] BODY MISMATCH\n  v4 {w}\n  v5 {g}");
        failed.push(format!("{driver}:{name}"));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn inventory_routes_equivalence() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_INVENTORY_ROUTES") else {
        eprintln!("SKIP: set QT_ORACLE_INVENTORY_ROUTES (see test header).");
        return;
    };
    let spec = load_spec();
    let pepper = spec.test_pepper_base64.clone();
    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path).unwrap().lines() {
        if line.trim().is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }
    let expected_rows: usize = spec
        .cases
        .iter()
        .map(|c| 1 + usize::from(c.then_read.is_some()) + usize::from(c.dump.is_some()))
        .sum();
    assert_eq!(
        oracle.len(),
        expected_rows,
        "oracle row count {} != corpus expectation {expected_rows} — regenerate the NDJSON",
        oracle.len()
    );
    let only: Option<Vec<String>> = std::env::var("QT_IROUTES_ONLY")
        .ok()
        .map(|s| s.split(',').map(str::to_string).collect());
    let known = known_of(&pepper);
    let mut failed: Vec<String> = Vec::new();
    let (mut direct_ran, mut http_ran) = (0usize, 0usize);
    for case in &spec.cases {
        if only.as_ref().is_some_and(|o| !o.contains(&case.name)) {
            continue;
        }
        for driver in ["direct", "http"] {
            let rows = if driver == "direct" {
                match direct_rows(case, &pepper).await {
                    Some(r) => {
                        direct_ran += 1;
                        r
                    }
                    None => continue,
                }
            } else {
                if case.direct_only {
                    continue;
                }
                http_ran += 1;
                http_rows(case, &pepper).await
            };
            let (mut v4_minted, mut v5_minted) = (Vec::new(), Vec::new());
            for (name, got) in &rows {
                let want = oracle
                    .get(name)
                    .unwrap_or_else(|| panic!("oracle has no row '{name}' — regenerate"));
                compare(
                    driver,
                    name,
                    want,
                    got,
                    &known,
                    &mut v4_minted,
                    &mut v5_minted,
                    &mut failed,
                );
            }
        }
    }
    eprintln!(
        "inventory_routes_equivalence: {} cases, {direct_ran} direct + {http_ran} http drives, {} failed",
        spec.cases.len(),
        failed.len()
    );
    assert!(
        failed.is_empty(),
        "{} rows differ: {failed:#?}",
        failed.len()
    );
    if only.is_none() {
        let direct_only = spec.cases.iter().filter(|c| c.direct_only).count();
        assert!(direct_ran > 100 && http_ran == spec.cases.len() - direct_only);
    }
}

/// R-10: every route line the factory, the chat actions and the archived
/// refusal log — level, `<logTag> <message>`, then v4's keys in v4's order
/// (`[logIdKey]` first, `context` last) — capture-pinned, each with a silence
/// leg (a refused request logs no success line). v5-side LITERALS checked
/// against v4's source at the pin (`inventory-route-factory.ts`,
/// `responses.ts:36`, `actions/inventory.ts:52,85,99`); the differential runs
/// v4 at `LOG_LEVEL=error` and compares bodies, so nothing else sees them. The
/// lines run on the CALLING thread after the write returns, so the
/// process-global rig (armed on this current-thread runtime's one thread)
/// sees every one of them and none of the writer thread's keystone lines.
#[test]
fn inventory_route_lines_are_v4s() {
    use quilltap_core::api::types::{InventoryContainer, InventoryScope};
    use quilltap_core::test_support::global_capture::capture_async;
    quilltap_core::test_support::global_capture::install();
    let spec = load_spec();
    let pepper = spec.test_pepper_base64.clone();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    const V: &str = "c1000000-0000-4000-8000-000000000001";
    const Z: &str = "c3000000-0000-4000-8000-000000000003";
    const P: &str = "d1000000-0000-4000-8000-000000000001";
    const G: &str = "e1000000-0000-4000-8000-000000000001";
    const ARCH: &str = "a0000001-0000-4000-8000-000000000000";
    const SWORD: &str = "a0000010-0000-4000-8000-000000000000";
    const LANTERN: &str = "a0000012-0000-4000-8000-000000000000";
    const DEFAULT: &str = "a0000017-0000-4000-8000-000000000000";
    const KIN: &str = "a0000011-0000-4000-8000-000000000000";
    const B3: &str = "b3000000-0000-4000-8000-000000000001";
    const B5: &str = "b5000000-0000-4000-8000-000000000001";
    let fresh = || {
        let scratch = tempfile::tempdir().unwrap();
        let blank = load_spec().cases.into_iter().next().unwrap();
        let mut blank = blank;
        blank.main_sql.clear();
        materialize(scratch.path(), "main.db", "mount.db", &blank, &pepper);
        let db = Db::open(
            DbPaths {
                main: scratch.path().join("main.db"),
                mount_index: Some(scratch.path().join("mount.db")),
                llm_logs: None,
            },
            &pepper,
        )
        .unwrap();
        (db, scratch)
    };
    let c = |scope, id: Option<&str>| InventoryContainer {
        scope,
        id: id.map(str::to_string),
    };
    let line = |lines: &[String], needle: &str, level: &str| -> String {
        let l = lines
            .iter()
            .find(|l| l.contains(needle))
            .cloned()
            .unwrap_or_else(|| panic!("no line carrying {needle:?}: {lines:#?}"));
        assert!(l.starts_with(&format!("{level} ")), "{level}: {l}");
        l
    };
    let none = |lines: &[String], needle: &str| {
        assert!(
            !lines.iter().any(|l| l.contains(needle)),
            "silence: {needle:?} in {lines:#?}"
        );
    };
    let ok = |r: &Response| assert!(matches!(r, Response::Inventory(_)), "{r:?}");

    // The collection GET (character tier: `[Characters v1]` + `characterId`).
    let (db, _s) = fresh();
    let (r, lines) = rt.block_on(capture_async(collection_action(
        &db,
        CollectionAction::List {
            include_archived: false,
            shared: false,
            project_id: None,
            chat_id: None,
        },
        &c(InventoryScope::Character, Some(V)),
        SINGLE_USER_ID,
    )));
    ok(&r);
    let Response::Inventory(body) = &r else {
        unreachable!()
    };
    let (mp, n) = (
        body["mountPointId"].as_str().unwrap().to_string(),
        body["items"].as_array().unwrap().len(),
    );
    line(
        &lines,
        &format!("[Characters v1] Listed inventory characterId={V} mountPointId={mp} count={n} shared=false includeArchived=false context=inventory"),
        "DEBUG",
    );
    // Silence: a missing owner logs no listing line.
    let (_, lines) = rt.block_on(capture_async(collection_action(
        &db,
        CollectionAction::Folders,
        &c(
            InventoryScope::Character,
            Some("c9000000-0000-4000-8000-000000000009"),
        ),
        SINGLE_USER_ID,
    )));
    none(&lines, "Listed inventory folders");

    // Folders (General: `[Inventory General v1]` + `generalId=null`).
    let (r, lines) = rt.block_on(capture_async(collection_action(
        &db,
        CollectionAction::Folders,
        &c(InventoryScope::General, None),
        SINGLE_USER_ID,
    )));
    let Response::Inventory(body) = &r else {
        panic!("{r:?}")
    };
    let n = body["folders"].as_array().unwrap().len();
    line(
        &lines,
        &format!("[Inventory General v1] Listed inventory folders generalId=null count={n} context=inventory"),
        "DEBUG",
    );

    // Archetypes (project tier: `[Projects v1]` + `projectId`).
    let (r, lines) = rt.block_on(capture_async(collection_action(
        &db,
        CollectionAction::Archetypes {
            project_id: None,
            chat_id: None,
        },
        &c(InventoryScope::Project, Some(P)),
        SINGLE_USER_ID,
    )));
    let Response::Inventory(body) = &r else {
        panic!("{r:?}")
    };
    let n = body["items"].as_array().unwrap().len();
    line(
        &lines,
        &format!(
            "[Projects v1] Listed reachable archetypes projectId={P} count={n} context=inventory"
        ),
        "DEBUG",
    );

    // Create (group tier: `[Groups v1]` + `groupId`); a refused body is silent.
    let (_, lines) = rt.block_on(capture_async(collection_action(
        &db,
        CollectionAction::Create {
            project_id: None,
            chat_id: None,
            body: json!({}),
        },
        &c(InventoryScope::Group, Some(G)),
        SINGLE_USER_ID,
    )));
    none(&lines, "Created inventory object");
    let (r, lines) = rt.block_on(capture_async(collection_action(
        &db,
        CollectionAction::Create {
            project_id: None,
            chat_id: None,
            body: json!({ "name": "oil" }),
        },
        &c(InventoryScope::Group, Some(G)),
        SINGLE_USER_ID,
    )));
    let Response::Inventory(body) = &r else {
        panic!("{r:?}")
    };
    let (id, gmp) = (
        body["item"]["id"].as_str().unwrap().to_string(),
        body["item"]["mountPointId"].as_str().unwrap().to_string(),
    );
    line(
        &lines,
        &format!("[Groups v1] Created inventory object groupId={G} userId={SINGLE_USER_ID} mountPointId={gmp} itemId={id} kind=item context=inventory"),
        "INFO",
    );

    // Instantiate, move, copy, duplicate (character tier).
    let (r, lines) = rt.block_on(capture_async(collection_action(
        &db,
        CollectionAction::Instantiate {
            body: json!({ "archetypeId": ARCH }),
        },
        &c(InventoryScope::Character, Some(V)),
        SINGLE_USER_ID,
    )));
    let Response::Inventory(body) = &r else {
        panic!("{r:?}")
    };
    let id = body["item"]["id"].as_str().unwrap().to_string();
    line(
        &lines,
        &format!("[Characters v1] Instantiated archetype characterId={V} userId={SINGLE_USER_ID} itemId={id} archetypeId={ARCH} context=inventory"),
        "INFO",
    );
    for (action, message, n) in [
        ("move", "Moved inventory objects to folder", 1),
        ("copy", "Copied inventory objects to folder", 1),
    ] {
        let body = json!({ "itemIds": [DEFAULT], "folder": "Weapons" });
        let act = if action == "move" {
            CollectionAction::ItemsMove { body }
        } else {
            CollectionAction::ItemsCopy { body }
        };
        let (r, lines) = rt.block_on(capture_async(collection_action(
            &db,
            act,
            &c(InventoryScope::Character, Some(V)),
            SINGLE_USER_ID,
        )));
        ok(&r);
        line(
            &lines,
            &format!("[Characters v1] {message} characterId={V} userId={SINGLE_USER_ID} folder=Weapons count={n} context=inventory"),
            "INFO",
        );
    }
    let (r, lines) = rt.block_on(capture_async(collection_action(
        &db,
        CollectionAction::Duplicate {
            body: json!({ "itemId": KIN }),
        },
        &c(InventoryScope::Character, Some(V)),
        SINGLE_USER_ID,
    )));
    let Response::Inventory(body) = &r else {
        panic!("{r:?}")
    };
    let copy = body["item"]["id"].as_str().unwrap().to_string();
    line(
        &lines,
        &format!("[Characters v1] Duplicated inventory object characterId={V} itemId={KIN} copyId={copy} context=inventory"),
        "INFO",
    );
    // No line for the folder writes or the item GET.
    let (r, lines) = rt.block_on(capture_async(collection_action(
        &db,
        CollectionAction::FolderCreate {
            body: json!({ "path": "Quiet" }),
        },
        &c(InventoryScope::Character, Some(V)),
        SINGLE_USER_ID,
    )));
    ok(&r);
    none(&lines, "[Characters v1]");

    // The item routes.
    let (r, lines) = rt.block_on(capture_async(item_action(
        &db,
        ItemAction::Dependents,
        &c(InventoryScope::General, None),
        ARCH,
        SINGLE_USER_ID,
    )));
    let Response::Inventory(body) = &r else {
        panic!("{r:?}")
    };
    let n = body["dependents"].as_array().unwrap().len();
    line(
        &lines,
        &format!("[Inventory General v1] Listed archetype dependents generalId=null itemId={ARCH} count={n} context=inventory"),
        "DEBUG",
    );
    let (_, lines) = rt.block_on(capture_async(item_action(
        &db,
        ItemAction::Update {
            project_id: None,
            chat_id: None,
            body: json!({ "title": 5 }),
        },
        &c(InventoryScope::Character, Some(V)),
        SWORD,
        SINGLE_USER_ID,
    )));
    none(&lines, "Updated inventory object");
    let (r, lines) = rt.block_on(capture_async(item_action(
        &db,
        ItemAction::Update {
            project_id: None,
            chat_id: None,
            body: json!({ "title": "Caliburn" }),
        },
        &c(InventoryScope::Character, Some(V)),
        SWORD,
        SINGLE_USER_ID,
    )));
    ok(&r);
    line(
        &lines,
        &format!("[Characters v1] Updated inventory object characterId={V} userId={SINGLE_USER_ID} itemId={SWORD} context=inventory"),
        "INFO",
    );
    let (r, lines) = rt.block_on(capture_async(item_action(
        &db,
        ItemAction::Detach,
        &c(InventoryScope::Character, Some(V)),
        LANTERN,
        SINGLE_USER_ID,
    )));
    ok(&r);
    line(
        &lines,
        &format!(
            "[Characters v1] Detached instance characterId={V} itemId={LANTERN} context=inventory"
        ),
        "INFO",
    );
    let (_, lines) = rt.block_on(capture_async(item_action(
        &db,
        ItemAction::Delete { force: false },
        &c(InventoryScope::General, None),
        ARCH,
        SINGLE_USER_ID,
    )));
    none(&lines, "Deleted inventory object");
    let (r, lines) = rt.block_on(capture_async(item_action(
        &db,
        ItemAction::Delete { force: false },
        &c(InventoryScope::Character, Some(V)),
        KIN,
        SINGLE_USER_ID,
    )));
    ok(&r);
    line(
        &lines,
        &format!("[Characters v1] Deleted inventory object characterId={V} userId={SINGLE_USER_ID} itemId={KIN} force=false detached=0 context=inventory"),
        "INFO",
    );

    // The archived character's 409 (`responses.ts:36`) — and no success line.
    let (r, lines) = rt.block_on(capture_async(collection_action(
        &db,
        CollectionAction::Create {
            project_id: None,
            chat_id: None,
            body: json!({ "name": "ghost" }),
        },
        &c(InventoryScope::Character, Some(Z)),
        SINGLE_USER_ID,
    )));
    assert!(matches!(r, Response::Error(_)), "{r:?}");
    line(
        &lines,
        "[Inventory routes] Refused a write to an archived character context=inventory",
        "INFO",
    );
    none(&lines, "Created inventory object");

    // The chat actions.
    let (db, _s) = fresh();
    let (r, lines) = rt.block_on(capture_async(async { chat_inventory_get(&db, B3) }));
    let Response::Inventory(body) = &r else {
        panic!("{r:?}")
    };
    let carried: usize = body["inventory"]
        .as_object()
        .unwrap()
        .values()
        .map(|e| e["equipped"].as_array().unwrap().len())
        .sum();
    line(
        &lines,
        &format!("[Chats v1] Read the cast inventory chatId={B3} characters=2 carried={carried} context=inventory"),
        "DEBUG",
    );
    let (r, lines) = rt.block_on(capture_async(chat_inventory_equip(
        &db,
        SINGLE_USER_ID,
        B5,
        &json!({ "characterId": V, "ids": [SWORD] }),
        EquipMode::Equip,
        &ProductionInventoryEffects,
    )));
    ok(&r);
    line(
        &lines,
        &format!("[Chats v1] Inventory equip chatId={B5} characterId={V} idsJson=[\"{SWORD}\"] context=inventory"),
        "INFO",
    );
    // R-14's loud unwired seam, until the union wires P4.D328's effects.
    line(
        &lines,
        "Inventory change effects not wired on this branch",
        "WARN",
    );
    let (_, lines) = rt.block_on(capture_async(chat_inventory_equip(
        &db,
        SINGLE_USER_ID,
        B5,
        &json!({ "characterId": V, "ids": [] }),
        EquipMode::Unequip,
        &ProductionInventoryEffects,
    )));
    none(&lines, "Inventory unequip");
    let (r, lines) = rt.block_on(capture_async(chat_inventory_equip(
        &db,
        SINGLE_USER_ID,
        B5,
        &json!({ "characterId": V, "ids": [SWORD] }),
        EquipMode::Unequip,
        &ProductionInventoryEffects,
    )));
    ok(&r);
    line(
        &lines,
        &format!("[Chats v1] Inventory unequip chatId={B5} characterId={V} idsJson=[\"{SWORD}\"] context=inventory"),
        "INFO",
    );
    let (r, lines) = rt.block_on(capture_async(chat_inventory_reset_defaults(
        &db,
        SINGLE_USER_ID,
        B3,
        &json!({ "characterId": V }),
    )));
    let Response::Inventory(body) = &r else {
        panic!("{r:?}")
    };
    let n = body["equipped"].as_array().unwrap().len();
    line(
        &lines,
        &format!("[Chats v1] Inventory reset to defaults chatId={B3} characterId={V} count={n} context=inventory"),
        "INFO",
    );
    let (_, lines) = rt.block_on(capture_async(chat_inventory_reset_defaults(
        &db,
        SINGLE_USER_ID,
        B3,
        &json!({ "characterId": Z }),
    )));
    none(&lines, "Inventory reset to defaults");
}
