//! Tier-2 differential for wardrobe FOLDER FILING (P4.D309 — v4 `bbbcd2388`):
//! `quilltap_core::services::wardrobe_folder_filing::file_items_to_folder` vs
//! v4's REAL `fileItemsToFolder` (`lib/wardrobe/folder-filing.ts`), each side
//! over its REAL `resolveWardrobeLocation` (no provisioning — the route's
//! `resolveLocation(ctx, params, false)`).
//!
//! Both sides start from the SAME baked fixture (`fixtures/folder-filing-
//! tier2.json` + `build-folder-filing-fixture.ts`: a character vault with a
//! nested outfit [Outfit → Suit → Shirt + Trousers, plus a borrowed General
//! hat], a diamond, an outfit over an ARCHIVED piece, a default item, an
//! archived item, a pre-existing `Shirt (copy)` in another folder, a filed and
//! a hand-made folder, pictures on four items, a wear row; an ARCHIVED
//! character; General, a project store and a group store with one composite
//! each). Every CHAIN runs against a FRESH copy; after every op both record
//! `{ result | error { name, message }, items: readItems(true), folders,
//! logs, tables }` — `tables` the six mount-index tables + MAIN `files` + MAIN
//! `wardrobe_wear_stats`, `ORDER BY rowid`; `logs` the `[FolderFiling]` /
//! `[ItemMutations]` / `[FolderMutations]` / `[WardrobeImages]` lines and the
//! bare `Wardrobe item created in folder`, compared as an ordered LIST
//! (rendered `LEVEL message k=v …`; an array field as `k=<JSON>`, the
//! keystone's `types=` convention). The service runs on the TEST thread, so its
//! DEBUG lines land in `captured_with` (the writer-thread blindness does not
//! apply here).
//!
//! NORMALISATION (both sides, the same function; R-3: a lane-private
//! extension of the transfers family's `normalize_all` / `normalize_item`,
//! duplicated rather than hoisted — that family's file stays closed):
//!   1. every item id a chain MINTED (a post-op item absent from the baked
//!      fixture) → `<minted:<its title>>` — copy titles are unique by
//!      construction, so the token is deterministic; every minted MAIN `files`
//!      id → `<minted-file:<its item's title>#<n>>` (n in rowid order). Both
//!      replaced as text everywhere — results, items, `componentItemIds`, log
//!      fields, `.md` content, `linkedTo` / `tags`, the
//!      `Wardrobe/images/<id>/…` link paths;
//!   2. every ISO stamp not in `pinnedStamps` → `<ts>`; every remaining UUID the
//!      baked fixture does not carry (the mount index's own row ids, blob ids)
//!      → `<id:N>` by first appearance in one fixed traversal; a 64-hex sha
//!      the fixture does not carry → `<sha>`, then each `.md` document's
//!      `contentSha256` (and its file's `sha256`) RECOMPUTED from the
//!      normalised content;
//!   3. item object keys canonicalised (the service seam compares VALUES — the
//!      three wire key orders are byte-pinned at the route family,
//!      `wardrobe_folders_routes_equivalence`); items and table rows sorted.
//!
//! Plus Rust-side asserts the oracle cannot make: every copy of one op shares
//! ONE `createdAt == updatedAt` (v4's single `now`), and a COPY never inherits
//! the original's wear rows.
//!
//! The service is fed `RefusingWebpTranscoder` (it stores the original bytes —
//! v4's own encode-failure fallback; the fixture's three `webp` samples are the
//! transfers family's, whose v4 transcode is a pass-through).
//!
//! Generate the fixtures + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_FFL_MAIN=/tmp/qt-ffl-main.db QT_FIXTURE_FFL_MOUNT=/tmp/qt-ffl-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-folder-filing-fixture.ts
//!   QT_FIXTURE_FFL_MAIN=/tmp/qt-ffl-main.db QT_FIXTURE_FFL_MOUNT=/tmp/qt-ffl-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/cases/folder-filing-tier2.ts \
//!     > /tmp/oracle-folder-filing.ndjson
//! Run:
//!   QT_ORACLE_FOLDER_FILING=/tmp/oracle-folder-filing.ndjson \
//!   QT_FIXTURE_FFL_MAIN=/tmp/qt-ffl-main.db QT_FIXTURE_FFL_MOUNT=/tmp/qt-ffl-mount.db \
//!     cargo test -p quilltap-harness --test folder_filing_tier2_equivalence

mod wardrobe_images_support;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use quilltap_core::db::vault_wardrobe_public::WardrobeScope;
use quilltap_core::db::wardrobe_location::{
    resolve_wardrobe_location, ResolveWardrobeLocationOptions, WardrobeLocation,
};
use quilltap_core::db::Writer;
use quilltap_core::services::mount_index::blob_transcode::RefusingWebpTranscoder;
use quilltap_core::services::wardrobe_folder_filing::{
    file_items_to_folder, FileItemsToFolderRequest, FolderComponentMode, FolderFilingAction,
};
use quilltap_core::test_support::captured_with;
use regex::Regex;
use rusqlite::Connection;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use wardrobe_images_support::{baked_tokens, dump, rust_lines, MOUNT_TABLES};

/// Any fixed ISO string — normalised to `<ts>` either way; the Rust-side
/// single-`now` assert reads it literally.
const FROZEN_NOW: &str = "2026-05-05T05:05:05.005Z";

const PREFIXES: [&str; 5] = [
    "[FolderFiling]",
    "[ItemMutations]",
    "[FolderMutations]",
    "[WardrobeImages]",
    "Wardrobe item created in folder",
];

/// The table keys in the one fixed traversal order (the oracle's keys).
const TABLE_KEYS: [&str; 8] = [
    "files",
    "wearStats",
    "points",
    "folders",
    "links",
    "mountFiles",
    "documents",
    "blobs",
];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/folder-filing-tier2.json")
}

fn env_or_skip(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(p) => Some(p),
        Err(_) => {
            eprintln!("SKIP: set {name} (see header).");
            None
        }
    }
}

fn tables_of(main: &Connection, mount: &Connection) -> Value {
    let mut t = Map::new();
    t.insert("files".into(), dump(main, "files"));
    t.insert("wearStats".into(), dump(main, "wardrobe_wear_stats"));
    for (key, table) in MOUNT_TABLES {
        t.insert(key.into(), dump(mount, table));
    }
    Value::Object(t)
}

/// v4's `{ level, message, meta }` → `LEVEL message k=v …` (meta keys in
/// order; an array/object as compact JSON, as v5's `types=` renders).
fn oracle_lines(logs: &Value) -> Vec<String> {
    logs.as_array()
        .expect("logs array")
        .iter()
        .map(|l| {
            let mut s = format!(
                "{} {}",
                l["level"].as_str().unwrap().to_uppercase(),
                l["message"].as_str().unwrap()
            );
            if let Some(meta) = l["meta"].as_object() {
                for (k, v) in meta {
                    match v {
                        Value::String(t) => s.push_str(&format!(" {k}={t}")),
                        Value::Null => s.push_str(&format!(" {k}=null")),
                        other => s.push_str(&format!(" {k}={other}")),
                    }
                }
            }
            s
        })
        .collect()
}

struct Norm {
    baked: HashSet<String>,
    pinned: HashSet<String>,
    ids: HashMap<String, String>,
    uuid: Regex,
    iso: Regex,
    sha: Regex,
}

impl Norm {
    fn new(baked: HashSet<String>, pinned: HashSet<String>) -> Self {
        Norm {
            baked,
            pinned,
            ids: HashMap::new(),
            uuid: Regex::new(
                r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}",
            )
            .unwrap(),
            iso: Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z").unwrap(),
            sha: Regex::new(r"\b[0-9a-f]{64}\b").unwrap(),
        }
    }

    fn string(&mut self, s: &str) -> String {
        let pinned = &self.pinned;
        let s = self
            .iso
            .replace_all(s, |c: &regex::Captures| {
                let m = c.get(0).unwrap().as_str();
                if pinned.contains(m) {
                    m.to_string()
                } else {
                    "<ts>".to_string()
                }
            })
            .into_owned();
        let found: Vec<String> = self
            .uuid
            .find_iter(&s)
            .map(|m| m.as_str().to_string())
            .collect();
        let mut s = s;
        for id in found {
            if self.baked.contains(&id) {
                continue;
            }
            let n = self.ids.len();
            let token = self
                .ids
                .entry(id.clone())
                .or_insert_with(|| format!("<id:{n}>"))
                .clone();
            s = s.replace(&id, &token);
        }
        let shas: Vec<String> = self
            .sha
            .find_iter(&s)
            .map(|m| m.as_str().to_string())
            .collect();
        for sha in shas {
            if !self.baked.contains(&sha) {
                s = s.replace(&sha, "<sha>");
            }
        }
        s
    }

    fn value(&mut self, v: &Value) -> Value {
        match v {
            Value::String(s) => Value::String(self.string(s)),
            Value::Array(a) => Value::Array(a.iter().map(|e| self.value(e)).collect()),
            Value::Object(o) => {
                let mut m = Map::new();
                for (k, e) in o {
                    m.insert(k.clone(), self.value(e));
                }
                Value::Object(m)
            }
            other => other.clone(),
        }
    }
}

/// Step 1: the chain's minted ids → title tokens (learned from this record's
/// post-op items and `files` rows; the map grows across a chain's ops).
fn learn_minted(record: &Value, baked: &HashSet<String>, minted: &mut Vec<(String, String)>) {
    let known = |m: &Vec<(String, String)>, id: &str| m.iter().any(|(k, _)| k == id);
    let mut titles: HashMap<String, String> = HashMap::new();
    for it in record["items"].as_array().into_iter().flatten() {
        let id = it["id"].as_str().unwrap_or_default().to_string();
        let title = it["title"].as_str().unwrap_or_default().to_string();
        titles.insert(id.clone(), title.clone());
        if !baked.contains(&id) && !known(minted, &id) {
            minted.push((id, format!("<minted:{title}>")));
        }
    }
    let mut per_item: HashMap<String, usize> = HashMap::new();
    for row in record["tables"]["files"]["rows"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let id = row["id"].as_str().unwrap_or_default().to_string();
        let linked: Vec<String> =
            serde_json::from_str(row["linkedTo"].as_str().unwrap_or("[]")).unwrap_or_default();
        let owner = linked.first().cloned().unwrap_or_default();
        let n = per_item.entry(owner.clone()).or_insert(0);
        *n += 1;
        if baked.contains(&id) || known(minted, &id) {
            continue;
        }
        let title = titles
            .get(&owner)
            .cloned()
            .unwrap_or_else(|| format!("?{owner}"));
        minted.push((id, format!("<minted-file:{title}#{n}>")));
    }
}

fn apply_minted(record: &Value, minted: &[(String, String)]) -> Value {
    let mut text = serde_json::to_string(record).unwrap();
    for (id, token) in minted {
        text = text.replace(id.as_str(), token);
    }
    serde_json::from_str(&text).unwrap()
}

fn canonical_keys(v: &mut Value) {
    match v {
        Value::Array(a) => a.iter_mut().for_each(canonical_keys),
        Value::Object(o) => {
            let mut entries: Vec<(String, Value)> = std::mem::take(o).into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            for (k, mut val) in entries {
                canonical_keys(&mut val);
                o.insert(k, val);
            }
        }
        _ => {}
    }
}

fn sha_hex(s: &str) -> String {
    Sha256::digest(s.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The whole per-op record, normalised (steps 1–3).
fn normalize(
    record: &Value,
    baked: &HashSet<String>,
    pinned: &HashSet<String>,
    minted: &mut Vec<(String, String)>,
) -> Value {
    learn_minted(record, baked, minted);
    let tokenised = apply_minted(record, minted);
    // One fixed traversal: result, items, folders, logs, then the tables in
    // TABLE_KEYS order, rows in rowid order.
    let mut norm = Norm::new(baked.clone(), pinned.clone());
    let mut out = Map::new();
    out.insert("result".into(), norm.value(&tokenised["result"]));
    out.insert("items".into(), norm.value(&tokenised["items"]));
    out.insert("folders".into(), norm.value(&tokenised["folders"]));
    out.insert("logs".into(), norm.value(&tokenised["logs"]));
    let mut tables = Map::new();
    for key in TABLE_KEYS {
        tables.insert(key.into(), norm.value(&tokenised["tables"][key]));
    }
    // Recompute each document's sha from its normalised content, and its
    // file's sha with it.
    let mut by_file: HashMap<String, String> = HashMap::new();
    if let Some(rows) = tables["documents"]["rows"].as_array_mut() {
        for row in rows {
            let sha = format!(
                "SHA_{}",
                sha_hex(row["content"].as_str().unwrap_or_default())
            );
            if let Some(fid) = row["fileId"].as_str() {
                by_file.insert(fid.to_string(), sha.clone());
            }
            row["contentSha256"] = Value::String(sha);
        }
    }
    if let Some(rows) = tables["mountFiles"]["rows"].as_array_mut() {
        for row in rows {
            if let Some(sha) = row["id"].as_str().and_then(|id| by_file.get(id)) {
                row["sha256"] = Value::String(sha.clone());
            }
        }
    }
    for key in TABLE_KEYS {
        if let Some(rows) = tables[key]["rows"].as_array_mut() {
            rows.sort_by_key(|r| serde_json::to_string(r).unwrap());
        }
    }
    out.insert("tables".into(), Value::Object(tables));
    let mut out = Value::Object(out);
    for key in ["result", "items", "folders"] {
        canonical_keys(&mut out[key]);
    }
    if let Some(items) = out["items"].as_array_mut() {
        items.sort_by_key(|i| serde_json::to_string(&i["id"]).unwrap());
    }
    out
}

fn scope_and_id<'a>(spec: &'a Value, scope: &str) -> (WardrobeScope, Option<&'a str>) {
    match scope {
        "character" => (WardrobeScope::Character, spec["characterId"].as_str()),
        "archived" => (
            WardrobeScope::Character,
            spec["archivedCharacterId"].as_str(),
        ),
        "project" => (WardrobeScope::Project, spec["projectId"].as_str()),
        "group" => (WardrobeScope::Group, spec["groupId"].as_str()),
        "general" => (WardrobeScope::General, None),
        other => panic!("unknown scope {other}"),
    }
}

fn locate(main: &Connection, mount: &Connection, spec: &Value, scope: &str) -> WardrobeLocation {
    let (s, id) = scope_and_id(spec, scope);
    resolve_wardrobe_location(
        main,
        mount,
        s,
        id,
        spec["userId"].as_str().unwrap(),
        ResolveWardrobeLocationOptions { ensure: false },
    )
    .expect("resolve")
    .unwrap_or_else(|| panic!("no {scope} location"))
}

fn item_value(item: &quilltap_core::vault_overlay::WardrobeItem) -> Value {
    serde_json::to_value(item).expect("item to json")
}

#[test]
fn folder_filing_matches_oracle() {
    let (Some(oracle_path), Some(main_fixture), Some(mount_fixture)) = (
        env_or_skip("QT_ORACLE_FOLDER_FILING"),
        env_or_skip("QT_FIXTURE_FFL_MAIN"),
        env_or_skip("QT_FIXTURE_FFL_MOUNT"),
    ) else {
        return;
    };
    let spec_text = std::fs::read_to_string(spec_path()).expect("read spec");
    let spec: Value = serde_json::from_str(&spec_text).expect("parse spec");
    let pepper = spec["testPepperBase64"].as_str().unwrap().to_string();
    let user_id = spec["userId"].as_str().unwrap().to_string();
    let pinned: HashSet<String> = spec["pinnedStamps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect();
    let item_id = |r: &str| -> String {
        match r.strip_prefix("raw:") {
            Some(raw) => raw.to_string(),
            None => spec["items"][r]["id"]
                .as_str()
                .unwrap_or_else(|| panic!("unknown item key {r}"))
                .to_string(),
        }
    };

    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .expect("read oracle")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse oracle line"))
        .collect();
    let chains = spec["chains"].as_array().unwrap();
    let op_count: usize = chains
        .iter()
        .map(|c| c["ops"].as_array().unwrap().len())
        .sum();
    assert_eq!(oracle.len(), op_count, "oracle op count != corpus");

    // The baked fixture's own tokens (every id, sha and pinned value it
    // carries) compare literally.
    let baked = {
        let main = Writer::open_writable(Path::new(&main_fixture), &pepper).expect("open main");
        let mount = Writer::open_writable(Path::new(&mount_fixture), &pepper).expect("open mount");
        let mut t = tables_of(main.connection(), mount.connection());
        // Every seeded item, too (ids live in `.md` content as well as here).
        let mut items = Vec::new();
        for scope in ["character", "archived", "general", "project", "group"] {
            let loc = locate(main.connection(), mount.connection(), &spec, scope);
            for it in loc.read_items(mount.connection(), true).expect("seed read") {
                items.push(item_value(&it));
            }
        }
        t["seedItems"] = Value::Array(items);
        baked_tokens(&t, &spec)
    };

    let mut oracle_iter = oracle.iter();
    let mut failures: Vec<String> = Vec::new();
    let mut compared = 0usize;
    for chain in chains {
        let name = chain["name"].as_str().unwrap();
        let scratch = tempfile::Builder::new()
            .prefix(&format!("qt-ffl-rust-{name}-"))
            .tempdir()
            .expect("tempdir");
        let main_work = scratch.path().join("main.db");
        let mount_work = scratch.path().join("mount.db");
        std::fs::copy(&main_fixture, &main_work).expect("copy main");
        std::fs::copy(&mount_fixture, &mount_work).expect("copy mount");
        let main = Writer::open_writable(&main_work, &pepper).expect("open main");
        let mount = Writer::open_writable(&mount_work, &pepper).expect("open mount");
        let (mc, xc) = (main.connection(), mount.connection());

        let mut item_n = 0u64;
        let mut file_n = 0u64;
        let mut minted_got: Vec<(String, String)> = Vec::new();
        let mut minted_want: Vec<(String, String)> = Vec::new();
        for (i, op) in chain["ops"].as_array().unwrap().iter().enumerate() {
            let want = oracle_iter.next().expect("oracle row");
            assert_eq!(want["chain"].as_str(), Some(name), "oracle chain order");
            assert_eq!(want["op"].as_u64(), Some(i as u64), "oracle op order");

            let scope = op["scope"].as_str().unwrap();
            let location = locate(mc, xc, &spec, scope);
            let ids: Vec<String> = op["itemIds"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| item_id(r.as_str().unwrap()))
                .collect();
            // id → updatedAt before the op.
            let pre_ids: HashMap<String, String> = location
                .read_items(xc, true)
                .expect("pre-op read")
                .into_iter()
                .map(|it| (it.id, it.updated_at))
                .collect();
            let action = match op["action"].as_str().unwrap() {
                "move" => FolderFilingAction::Move,
                _ => FolderFilingAction::Copy,
            };
            let components = op["components"].as_str().map(|c| match c {
                "move" => FolderComponentMode::Move,
                "copy" => FolderComponentMode::Copy,
                _ => FolderComponentMode::None,
            });
            let request = FileItemsToFolderRequest {
                action,
                item_ids: &ids,
                folder: op["folder"].as_str().unwrap(),
                components,
            };
            let mut mint = || {
                item_n += 1;
                format!("f1f1f1f1-0000-4000-8000-{item_n:012x}")
            };
            let mut mint_file_id = || {
                file_n += 1;
                format!("f2f2f2f2-0000-4000-8000-{file_n:012x}")
            };
            let (outcome, lines) = captured_with(|| {
                file_items_to_folder(
                    mc,
                    xc,
                    &user_id,
                    &location,
                    &request,
                    &mut mint,
                    &mut mint_file_id,
                    FROZEN_NOW,
                    &RefusingWebpTranscoder,
                )
            });
            let result = match &outcome {
                Ok(out) => json!({
                    "kind": "ok",
                    "items": out.items.iter().map(item_value).collect::<Vec<_>>(),
                    "componentsCarried": out.components_carried,
                }),
                Err(e) => json!({ "kind": "error", "name": e.name(), "message": e.to_string() }),
            };
            let items: Vec<Value> = location
                .read_items(xc, true)
                .expect("read-back")
                .iter()
                .map(item_value)
                .collect();
            let folders =
                serde_json::to_value(location.list_folders(xc).expect("list folders")).unwrap();
            let got = json!({
                "result": result,
                "items": items,
                "folders": folders,
                "logs": rust_lines(&lines, &PREFIXES),
                "tables": tables_of(mc, xc),
            });

            // Rust-side: every item this op minted carries the op's ONE `now`
            // (`createdAt == updatedAt`); every item it moved was restamped; no
            // copy inherits a wear row.
            if outcome.is_ok() {
                for it in got["items"].as_array().unwrap() {
                    let id = it["id"].as_str().unwrap();
                    if !pre_ids.contains_key(id) {
                        assert!(
                            it["createdAt"] == FROZEN_NOW && it["updatedAt"] == FROZEN_NOW,
                            "{name}/{i}: a copy's stamps are not the op's one now: {it}"
                        );
                    }
                }
                if action == FolderFilingAction::Move {
                    for id in &ids {
                        let after = got["items"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .find(|it| it["id"] == id.as_str())
                            .expect("moved item still here");
                        assert_ne!(
                            after["updatedAt"].as_str(),
                            pre_ids.get(id.as_str()).map(String::as_str),
                            "{name}/{i}: a moved item was not restamped"
                        );
                    }
                }
                let wear = got["tables"]["wearStats"]["rows"].as_array().unwrap();
                assert!(
                    wear.iter()
                        .all(|w| !w["itemId"].as_str().unwrap_or("").starts_with("f1f1f1f1-")),
                    "{name}/{i}: a copy inherited wear rows"
                );
            }

            let want_record = json!({
                "result": want["result"],
                "items": want["items"],
                "folders": want["folders"],
                "logs": oracle_lines(&want["logs"]),
                "tables": want["tables"],
            });
            let g = normalize(&got, &baked, &pinned, &mut minted_got);
            let w = normalize(&want_record, &baked, &pinned, &mut minted_want);
            compared += 1;
            for key in ["result", "items", "folders", "logs"] {
                let (gs, ws) = (
                    serde_json::to_string_pretty(&g[key]).unwrap(),
                    serde_json::to_string_pretty(&w[key]).unwrap(),
                );
                if gs != ws {
                    failures.push(format!("{name}/{i} {key}:\n  rust:   {gs}\n  oracle: {ws}"));
                }
            }
            for key in TABLE_KEYS {
                let (gs, ws) = (
                    serde_json::to_string(&g["tables"][key]).unwrap(),
                    serde_json::to_string(&w["tables"][key]).unwrap(),
                );
                if gs != ws {
                    failures.push(format!(
                        "{name}/{i} tables.{key}:\n  rust:   {gs}\n  oracle: {ws}"
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} divergence(s) over {compared} ops:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    eprintln!("OK: folder filing tier-2 matched oracle ({compared} ops).");
}
