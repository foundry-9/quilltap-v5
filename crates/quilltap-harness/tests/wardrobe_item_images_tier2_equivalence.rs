//! Tier-2 differential for the wardrobe ITEM-IMAGES module (P4.D263; v4's
//! `lib/wardrobe/item-images.ts`, `lib/file-storage/wardrobe-image-bridge.ts`
//! and `lib/wardrobe/resolve-container.ts`, `7c8572869`): the port in
//! `quilltap-core::services` (`wardrobe_container`, `wardrobe_image_bridge`,
//! `wardrobe_item_images::{service, carry, primitives}`) vs v4's REAL exports,
//! driven by `harness/oracle/cases/wardrobe-item-images.test.ts`.
//!
//! Both sides start from the SAME baked two-DB fixture (every picture seeded
//! through v4's REAL `addWardrobeItemImage`; the planted dangling, empty-string
//! and DOCUMENT shapes; an archived character; a stranger's character). Per
//! scenario each side runs ONE operation on a fresh copy, with its log lines
//! captured, then:
//!   - the operation's RESULT (the home, the list, the added file's summary,
//!     the new current id, the carry map + pending move, or the typed error
//!     `{error, message}`) — compared exactly after normalization;
//!   - every `[WardrobeImages]` / `[WardrobeImageBridge]` / `[WardrobeContainer]`
//!     / cleanup-tag LINE, in order, `LEVEL message k=v…` in v4's meta key order
//!     (§R.5 — each line's presence AND each silence: a scenario with no line
//!     on one side and one on the other fails);
//!   - every item's frontmatter `imageFileId`, read back through the container
//!     resolver;
//!   - the main `files` table and the six mount-index tables (points, folders,
//!     links, files, documents, blobs — `doc_mount_chunks` excluded, as the
//!     transfers family does), in rowid order.
//!
//! NORMALIZATION (applied to each side's whole scenario JSON independently):
//! ISO timestamps → `<ts>`; a minted picture leaf
//! (`yyyymmdd-hhmmss-<kind>-<8 hex>`) not present in the baked fixture →
//! `<leaf:kind:N>`; a UUID not present in the baked fixture → `<id:N>`, both
//! numbered first-seen in a fixed traversal (result → logs → pointers → tables
//! in rowid order), so a minted id must land in the SAME relational places on
//! both sides; a 64-hex digest that is neither baked nor the sha of a fixture
//! picture → `<sha>` (only the documents' content-addressed shas move, and the
//! normalized content itself is compared). Baked ids, picture shas and blob
//! bytes compare LITERALLY.
//!
//! Generate the fixtures + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-wii-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/wardrobe-item-images.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/wardrobe-item-images-tier2.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_WII_MAIN=/tmp/qt-wii-main.db QT_FIXTURE_WII_MOUNT=/tmp/qt-wii-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-wardrobe-item-images-fixture.ts
//!   QT_FIXTURE_WII_MAIN=/tmp/qt-wii-main.db QT_FIXTURE_WII_MOUNT=/tmp/qt-wii-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-wardrobe-item-images.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=600000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- "cases/wardrobe-item-images\.test\.ts$"
//! Run:
//!   QT_ORACLE_WII=/tmp/oracle-wardrobe-item-images.ndjson \
//!   QT_FIXTURE_WII_MAIN=/tmp/qt-wii-main.db QT_FIXTURE_WII_MOUNT=/tmp/qt-wii-mount.db \
//!     cargo test -p quilltap-harness --test wardrobe_item_images_tier2_equivalence

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use quilltap_core::api::types::WardrobeContainerScope;
use quilltap_core::db::Writer;
use quilltap_core::services::mount_index::blob_transcode::RefusingWebpTranscoder;
use quilltap_core::services::wardrobe_container::{resolve_wardrobe_item_home, scope_str};
use quilltap_core::services::wardrobe_image_bridge::WardrobeImageKind;
use quilltap_core::services::wardrobe_item_images::carry::{
    carry_item_images, commit_moved_images, drop_source_image_links, CarryArgs, CarryMode,
    SourceImageLink,
};
use quilltap_core::services::wardrobe_item_images::primitives::{
    cleanup_item_images, ItemImageCleanupMeta,
};
use quilltap_core::services::wardrobe_item_images::service::{
    add_wardrobe_item_image, delete_wardrobe_item_image, list_wardrobe_item_images,
    set_current_wardrobe_item_image, to_wardrobe_image_summary, AddWardrobeItemImageInput,
    ImageMint, ItemImageError,
};
use quilltap_core::test_support::captured_with;
use regex::Regex;
use rusqlite::Connection;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

const NOWHERE_ID: &str = "99999999-0000-4000-8000-000000000000";
const LOG_PREFIXES: [&str; 4] = [
    "[WardrobeImages]",
    "[WardrobeImageBridge]",
    "[WardrobeContainer]",
    "[Wardrobe v1]",
];
const MOUNT_TABLES: [(&str, &str); 6] = [
    ("points", "doc_mount_points"),
    ("folders", "doc_mount_folders"),
    ("links", "doc_mount_file_links"),
    ("mountFiles", "doc_mount_files"),
    ("documents", "doc_mount_documents"),
    ("blobs", "doc_mount_blobs"),
];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/wardrobe-item-images-tier2.json")
}

struct Ctx<'a> {
    spec: &'a Value,
    main: &'a Connection,
    mount: &'a Connection,
}

impl Ctx<'_> {
    fn s(&self, k: &str) -> String {
        self.spec[k].as_str().unwrap().to_string()
    }
    fn item_id(&self, key: &str) -> String {
        self.spec["items"][key]["id"].as_str().unwrap().to_string()
    }
    fn container_id(&self, c: Option<&str>) -> Option<String> {
        match c {
            Some("character") => Some(self.s("characterId")),
            Some("archived") => Some(self.s("archivedCharacterId")),
            Some("stranger") => Some(self.s("strangerCharacterId")),
            Some("project") => Some(self.s("projectId")),
            Some("group") => Some(self.s("groupId")),
            Some("nowhere") => Some(NOWHERE_ID.to_string()),
            _ => None,
        }
    }
    fn mount_of(&self, m: &str) -> String {
        let q = |sql: &str, id: String| -> String {
            self.main
                .query_row(sql, [id], |r| r.get::<_, String>(0))
                .unwrap()
        };
        match m {
            "general" => self.s("generalMountPointId"),
            "character" => q(
                "SELECT characterDocumentMountPointId FROM characters WHERE id = ?1",
                self.s("characterId"),
            ),
            "project" => q(
                "SELECT officialMountPointId FROM projects WHERE id = ?1",
                self.s("projectId"),
            ),
            _ => q(
                "SELECT officialMountPointId FROM groups WHERE id = ?1",
                self.s("groupId"),
            ),
        }
    }
    fn nth_picture(&self, item: &str, nth: usize) -> (String, String) {
        let rows = list_wardrobe_item_images(self.main, &self.item_id(item)).unwrap();
        let f = &rows[nth];
        (f.id.clone(), f.original_filename.clone())
    }
    fn resolve_ref(&self, r: &Value) -> String {
        if let Some(doc) = r.get("doc").and_then(Value::as_str) {
            return self
                .main
                .query_row(
                    "SELECT f.id FROM files f, json_each(f.linkedTo) j \
                     WHERE j.value = ?1 AND f.category = 'DOCUMENT'",
                    [self.item_id(doc)],
                    |row| row.get::<_, String>(0),
                )
                .unwrap();
        }
        self.nth_picture(
            r["item"].as_str().unwrap(),
            r.get("nth").and_then(Value::as_u64).unwrap_or(0) as usize,
        )
        .0
    }
}

fn scope_of(s: &str) -> WardrobeContainerScope {
    serde_json::from_value(json!(s)).unwrap()
}

fn error_json(e: &ItemImageError) -> Value {
    let name = match e {
        ItemImageError::Archived { .. } => "CharacterArchivedError",
        ItemImageError::Foreign { .. } => "ForeignWardrobeImageError",
        ItemImageError::Failed(_) => "Error",
    };
    json!({ "error": name, "message": e.message() })
}

/// One scenario on the Rust side → `(result, raw log lines)`.
fn run_op(ctx: &Ctx<'_>, scenario: &Value) -> (Value, Vec<String>) {
    let user_id = ctx.s("userId");
    let item_key = scenario["item"].as_str().unwrap();
    let item_id = ctx.item_id(item_key);
    let op = scenario["op"].as_str().unwrap();
    let scope = scenario.get("scope").and_then(Value::as_str).map(scope_of);
    let container = ctx.container_id(scenario.get("container").and_then(Value::as_str));
    let file_id = scenario.get("file").map(|r| ctx.resolve_ref(r));
    let drop_links: Vec<SourceImageLink> = scenario
        .get("links")
        .and_then(Value::as_array)
        .map(|links| {
            links
                .iter()
                .map(|l| SourceImageLink {
                    mount_point_id: ctx.mount_of(l["mount"].as_str().unwrap()),
                    leaf_name: match l.get("leaf").and_then(Value::as_str) {
                        Some(leaf) => leaf.to_string(),
                        None => {
                            ctx.nth_picture(item_key, l["nth"].as_u64().unwrap() as usize)
                                .1
                        }
                    },
                })
                .collect()
        })
        .unwrap_or_default();
    let destination_mp = scenario
        .get("destination")
        .and_then(Value::as_str)
        .map(|d| ctx.mount_of(d));
    let webp: Vec<Vec<u8>> = ctx.spec["webp"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD
                .decode(b.as_str().unwrap())
                .unwrap()
        })
        .collect();
    let codec = RefusingWebpTranscoder;
    let (main, mount) = (ctx.main, ctx.mount);

    let home_for = || {
        resolve_wardrobe_item_home(
            main,
            mount,
            &user_id,
            scope.unwrap(),
            container.as_deref(),
            &item_id,
        )
        .unwrap()
    };

    let (result, lines) = captured_with(|| -> Value {
        match op {
            "home" => match home_for() {
                Some(h) => {
                    let mut ids: Vec<String> = h
                        .container_items
                        .iter()
                        .filter_map(|i| i.get("id").and_then(Value::as_str).map(str::to_string))
                        .collect();
                    ids.sort();
                    json!({
                        "scope": scope_str(h.scope),
                        "characterId": h.character_id,
                        "itemId": h.item_id(),
                        "imageFileId": h.image_file_id(),
                        "containerItemIds": ids,
                    })
                }
                None => Value::Null,
            },
            "list" => {
                let h = home_for().expect("home");
                let images = list_wardrobe_item_images(main, h.item_id()).unwrap();
                json!({
                    "images": images.iter().map(to_wardrobe_image_summary).collect::<Vec<_>>(),
                    "itemImageFileId": h.image_file_id(),
                })
            }
            "add" => {
                let h = home_for().expect("home");
                let kind = match scenario["kind"].as_str().unwrap() {
                    "generated" => WardrobeImageKind::Generated,
                    "imported" => WardrobeImageKind::Imported,
                    _ => WardrobeImageKind::Uploaded,
                };
                let bytes = &webp[scenario["webp"].as_u64().unwrap_or(0) as usize];
                let opt = |k: &str| scenario.get(k).and_then(Value::as_str);
                match add_wardrobe_item_image(
                    main,
                    mount,
                    &h,
                    &AddWardrobeItemImageInput {
                        user_id: &user_id,
                        kind,
                        content: bytes,
                        content_type: "image/webp",
                        width: scenario.get("width").and_then(Value::as_i64),
                        height: scenario.get("height").and_then(Value::as_i64),
                        generation_prompt: opt("generationPrompt"),
                        generation_model: opt("generationModel"),
                        generation_revised_prompt: opt("generationRevisedPrompt"),
                    },
                    &ImageMint::now(),
                    &codec,
                ) {
                    Ok(added) => json!({
                        "file": to_wardrobe_image_summary(&added.file),
                        "originalFilename": added.file.original_filename,
                        "itemImageFileId": added.item.and_then(|i| i.image_file_id.flatten()),
                    }),
                    Err(e) => error_json(&e),
                }
            }
            "setCurrent" => {
                let h = home_for().expect("home");
                match set_current_wardrobe_item_image(main, mount, &h, file_id.as_deref().unwrap())
                {
                    Ok(c) => json!({ "current": c }),
                    Err(e) => error_json(&e),
                }
            }
            "delete" => {
                let h = home_for().expect("home");
                match delete_wardrobe_item_image(main, mount, &h, file_id.as_deref().unwrap()) {
                    Ok(c) => json!({ "current": c }),
                    Err(e) => error_json(&e),
                }
            }
            "cleanup" => {
                let character_id = ctx.s("characterId");
                cleanup_item_images(
                    main,
                    mount,
                    &item_id,
                    "[Wardrobe v1]",
                    ItemImageCleanupMeta::Character {
                        character_id: &character_id,
                    },
                );
                json!({ "done": true })
            }
            "carry" => {
                let dest_key = scenario["destinationItem"].as_str().unwrap();
                let destination_item_id = if dest_key == "copy" {
                    ctx.s("copyDestinationItemId")
                } else {
                    ctx.item_id(dest_key)
                };
                let mode = if scenario["mode"] == "copy" {
                    CarryMode::Copy
                } else {
                    CarryMode::Move
                };
                let now = quilltap_core::clock::now_iso();
                let carried = carry_item_images(
                    main,
                    mount,
                    &CarryArgs {
                        mode,
                        source_item_id: &item_id,
                        destination_item_id: &destination_item_id,
                        destination_mount_point_id: destination_mp.as_deref().unwrap(),
                        user_id: &user_id,
                    },
                    &mut || ImageMint::now().file_id,
                    &now,
                    &codec,
                )
                .unwrap();
                if scenario["commit"] == true {
                    commit_moved_images(main, mount, &item_id, &carried.pending_move, &now);
                }
                json!({
                    "fileIdMap": carried.file_id_map.iter().map(|(a, b)| json!([a, b])).collect::<Vec<_>>(),
                    "pendingMove": {
                        "repoints": carried.pending_move.repoints.iter().map(|r| json!({
                            "fileId": r.file_id,
                            "storageKey": r.storage_key,
                            "sourceLink": r.source_link.as_ref().map(|l| json!({
                                "mountPointId": l.mount_point_id,
                                "leafName": l.leaf_name,
                            })),
                        })).collect::<Vec<_>>(),
                    },
                })
            }
            "drop" => {
                drop_source_image_links(mount, &item_id, &drop_links);
                json!({ "done": true })
            }
            other => panic!("unknown op {other}"),
        }
    });
    (result, lines)
}

/// `LEVEL target message k=v…` → `LEVEL message k=v…` for the lines this
/// family pins.
fn rust_lines(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|l| {
            let mut parts = l.splitn(3, ' ');
            let level = parts.next()?;
            let _target = parts.next()?;
            let rest = parts.next()?;
            LOG_PREFIXES
                .iter()
                .any(|p| rest.starts_with(p))
                .then(|| format!("{level} {rest}"))
        })
        .collect()
}

/// v4's `{level, message, meta}` → the same rendering (meta keys in order,
/// `null` for null, numbers/booleans bare).
fn oracle_lines(logs: &Value) -> Vec<String> {
    logs.as_array()
        .unwrap()
        .iter()
        .map(|l| {
            let mut s = format!(
                "{} {}",
                l["level"].as_str().unwrap().to_uppercase(),
                l["message"].as_str().unwrap()
            );
            if let Some(meta) = l["meta"].as_object() {
                for (k, v) in meta {
                    let v = match v {
                        Value::String(s) => s.clone(),
                        Value::Null => "null".to_string(),
                        other => other.to_string(),
                    };
                    s.push_str(&format!(" {k}={v}"));
                }
            }
            s
        })
        .collect()
}

fn dump(conn: &Connection, table: &str) -> Value {
    let columns: Vec<String> = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap()
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let mut stmt = conn
        .prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))
        .unwrap();
    let rows: Vec<Value> = stmt
        .query_map([], |r| {
            let mut m = Map::new();
            for (i, c) in columns.iter().enumerate() {
                use rusqlite::types::ValueRef;
                let v = match r.get_ref(i)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(n) => json!(n),
                    // JS has one number type: a whole REAL reads as an integer.
                    ValueRef::Real(f) if f.fract() == 0.0 && f.abs() < 9e15 => json!(f as i64),
                    ValueRef::Real(f) => json!(f),
                    ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
                    ValueRef::Blob(b) => {
                        json!(b.iter().map(|x| format!("{x:02x}")).collect::<String>())
                    }
                };
                m.insert(c.clone(), v);
            }
            Ok(Value::Object(m))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    json!({ "table": table, "columns": columns, "rows": rows })
}

fn dump_all(main: &Connection, mount: &Connection) -> Value {
    let mut tables = Map::new();
    tables.insert("files".into(), dump(main, "files"));
    for (key, table) in MOUNT_TABLES {
        tables.insert(key.into(), dump(mount, table));
    }
    Value::Object(tables)
}

fn pointers(ctx: &Ctx<'_>) -> Value {
    let mut m = Map::new();
    for (key, item) in ctx.spec["items"].as_object().unwrap() {
        let (scope, container) = match item["home"].as_str().unwrap() {
            "character" => ("character", Some(ctx.s("characterId"))),
            "archived" => ("character", Some(ctx.s("archivedCharacterId"))),
            "general" => ("general", None),
            "project" => ("project", Some(ctx.s("projectId"))),
            _ => ("group", Some(ctx.s("groupId"))),
        };
        let home = resolve_wardrobe_item_home(
            ctx.main,
            ctx.mount,
            &ctx.s("userId"),
            scope_of(scope),
            container.as_deref(),
            item["id"].as_str().unwrap(),
        )
        .unwrap();
        m.insert(
            key.clone(),
            match home {
                Some(h) => json!(h.image_file_id()),
                None => json!("<gone>"),
            },
        );
    }
    Value::Object(m)
}

/// The scenario-wide normalizer (see the header).
struct Normalizer {
    baked: HashSet<String>,
    ids: HashMap<String, String>,
    leaves: HashMap<String, String>,
    uuid: Regex,
    iso: Regex,
    leaf: Regex,
    sha: Regex,
}

impl Normalizer {
    fn new(baked: HashSet<String>) -> Self {
        Normalizer {
            baked,
            ids: HashMap::new(),
            leaves: HashMap::new(),
            uuid: Regex::new(
                r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}",
            )
            .unwrap(),
            iso: Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z").unwrap(),
            leaf: Regex::new(r"\d{8}-\d{6}-(generated|uploaded|imported)-[0-9a-f]{8}").unwrap(),
            sha: Regex::new(r"\b[0-9a-f]{64}\b").unwrap(),
        }
    }

    fn string(&mut self, s: &str) -> String {
        let s = self.iso.replace_all(s, "<ts>").into_owned();
        let mut out = String::new();
        let mut last = 0;
        let leaves: Vec<(usize, usize, String, String)> = self
            .leaf
            .captures_iter(&s)
            .map(|c| {
                let m = c.get(0).unwrap();
                (m.start(), m.end(), m.as_str().to_string(), c[1].to_string())
            })
            .collect();
        for (start, end, whole, kind) in leaves {
            out.push_str(&s[last..start]);
            if self.baked.contains(&whole) {
                out.push_str(&whole);
            } else {
                let n = self.leaves.len();
                let token = self
                    .leaves
                    .entry(whole)
                    .or_insert_with(|| format!("<leaf:{kind}:{n}>"))
                    .clone();
                out.push_str(&token);
            }
            last = end;
        }
        out.push_str(&s[last..]);
        let s = out;
        let ids: Vec<String> = self
            .uuid
            .find_iter(&s)
            .map(|m| m.as_str().to_string())
            .collect();
        let mut s = s;
        for id in ids {
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

/// Every uuid / leaf / 64-hex token in the baked fixture's dump, plus the
/// picture shas — the values that compare literally.
fn baked_tokens(tables: &Value, spec: &Value) -> HashSet<String> {
    let text = tables.to_string();
    let mut out = HashSet::new();
    for re in [
        r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}",
        r"\d{8}-\d{6}-(generated|uploaded|imported)-[0-9a-f]{8}",
        r"\b[0-9a-f]{64}\b",
    ] {
        for m in Regex::new(re).unwrap().find_iter(&text) {
            out.insert(m.as_str().to_string());
        }
    }
    for m in
        Regex::new(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")
            .unwrap()
            .find_iter(&spec.to_string())
    {
        out.insert(m.as_str().to_string());
    }
    for b in spec["webp"].as_array().unwrap() {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b.as_str().unwrap())
            .unwrap();
        out.insert(
            Sha256::digest(&bytes)
                .iter()
                .map(|x| format!("{x:02x}"))
                .collect(),
        );
    }
    out
}

#[test]
fn wardrobe_item_images_tier2_matches_oracle() {
    let (Ok(oracle_path), Ok(main_fixture), Ok(mount_fixture)) = (
        std::env::var("QT_ORACLE_WII"),
        std::env::var("QT_FIXTURE_WII_MAIN"),
        std::env::var("QT_FIXTURE_WII_MOUNT"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_WII + QT_FIXTURE_WII_MAIN + QT_FIXTURE_WII_MOUNT (see header)."
        );
        return;
    };
    let spec: Value = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let pepper = spec["testPepperBase64"].as_str().unwrap().to_string();
    let oracle: HashMap<String, Value> = std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Value = serde_json::from_str(l).unwrap();
            (v["name"].as_str().unwrap().to_string(), v)
        })
        .collect();
    let scenarios = spec["scenarios"].as_array().unwrap();
    assert_eq!(
        oracle.len(),
        scenarios.len(),
        "oracle scenario count != corpus"
    );

    let mut failures: Vec<String> = Vec::new();
    for scenario in scenarios {
        let name = scenario["name"].as_str().unwrap();
        let want = &oracle[name];
        let scratch = tempfile::Builder::new()
            .prefix(&format!("qt-wii-rust-{name}-"))
            .tempdir()
            .unwrap();
        let (mw, nw) = (
            scratch.path().join("main.db"),
            scratch.path().join("mount.db"),
        );
        std::fs::copy(&main_fixture, &mw).unwrap();
        std::fs::copy(&mount_fixture, &nw).unwrap();
        let main = Writer::open_writable(&mw, &pepper).unwrap();
        let mount = Writer::open_writable(&nw, &pepper).unwrap();
        quilltap_core::test_support::ensure_wear_ledger_on(main.connection());
        let ctx = Ctx {
            spec: &spec,
            main: main.connection(),
            mount: mount.connection(),
        };
        let baked = baked_tokens(&dump_all(ctx.main, ctx.mount), &spec);

        let (result, lines) = run_op(&ctx, scenario);
        let got = json!({
            "result": result,
            "logs": rust_lines(&lines),
            "pointers": pointers(&ctx),
            "tables": dump_all(ctx.main, ctx.mount),
        });
        let want_scenario = json!({
            "result": want["result"],
            "logs": oracle_lines(&want["logs"]),
            "pointers": want["pointers"],
            "tables": want["tables"],
        });
        let got_n = Normalizer::new(baked.clone()).value(&got);
        let want_n = Normalizer::new(baked).value(&want_scenario);
        for part in ["result", "logs", "pointers", "tables"] {
            if got_n[part] != want_n[part] {
                if part == "tables" {
                    for (k, t) in want_n["tables"].as_object().unwrap() {
                        if &got_n["tables"][k] != t {
                            failures.push(format!(
                                "{name}: table {k} diverged\n  rust:   {}\n  oracle: {}",
                                got_n["tables"][k], t
                            ));
                        }
                    }
                } else {
                    failures.push(format!(
                        "{name}: {part} diverged\n  rust:   {}\n  oracle: {}",
                        got_n[part], want_n[part]
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} divergence(s):\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
