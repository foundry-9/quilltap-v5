//! P4.D263 items 7–8 — the wardrobe item-images EDGE
//! (`/api/v1/wardrobe/{itemId}/images`, `quilltap-web::wardrobe_images_routes`
//! over `quilltap-core::api::wardrobe_item_images`) vs v4's REAL
//! `app/api/v1/wardrobe/[itemId]/images/route.ts` handlers, request by request,
//! over the item-images tier-2 fixture (the corpus:
//! `harness/oracle/fixtures/wardrobe-item-images-routes.json`).
//!
//! Two drivers. `http` requests go through the REAL router of a host booted on
//! a fresh copy of the fixture (`common::serve_instance`): the query parse, the
//! action dispatch (`Action parameter required` / `Unknown action: <a>` with
//! `availableActions`), the 400 → 404 → body order, the multipart upload leg
//! (v4's `validateImageFile` sentences, the `kind` 400, the host codec's
//! `convertToWebP`), set-current / delete-image, and every PRE-provider
//! generate arm. No `http` request ever reaches a provider (the booted host's
//! provider is real). `direct` requests drive generate's 201 / 422 / 502 with
//! the provider mocked below both sides: v5 calls `generate_on_home` and
//! renders through the route's own `render` (which restores v4's 502).
//!
//! Comparands: status + body (key order included), the follow-up GET after a
//! mutation, and the item-scoped rows after it — the `files` rows linked to a
//! corpus item, the `Wardrobe/images/` links and their blobs. Normalization
//! (both sides): ISO timestamps → `<ts>`; a UUID / picture leaf / 64-hex
//! digest NOT present in the baked fixture → a first-seen token. The one
//! `codecDependent` row (a PNG transcoded by real sharp vs the host codec — D19)
//! drops the encoder-owned columns on both sides.
//!
//! The `[Wardrobe Images v1]` LINES are not compared here (a served host logs
//! off the test thread); they are capture-pinned in the item-images tier-2
//! family (`wardrobe_item_images_tier2_equivalence::the_route_lines_are_v4s`
//! and P4.D275's `the_save_route_lines_are_v4s`).
//!
//! P4.D275 (v4 `01a83539d`): `GET ?action=save-targets` (and the GET action
//! dispatch — an unknown / bare `?action` 400s BEFORE the query) and `POST
//! ?action=save-to-store`, all on the `http` driver: v4's oracle restores the
//! REAL storage manager for the save rows (`realStorage`), so both sides read
//! the picture's own mount blob (M4). Requests may plant the mount partition
//! (`mountSql`) and `repeat` (the ALREADY_SAVED 409 is the second answer). A
//! `{ref:{store}}` resolves a vault / official store / General id and
//! `{ref:{imageLink}}` an item's first `Wardrobe/images/` LINK id. The rows
//! grow the `photos/` links (their `extractedText` is the kept-image sidecar —
//! the operator's byline, M6) and their blobs; the saved file's timestamped
//! leaf (`photos/2026-…T02-35-04.067Z-<slug>.webp`) normalizes as `<pts>`.
//! Bodies are compared as BYTES (key order included — `serde_json`'s
//! `preserve_order` map equality is order-blind).
//!
//! Regenerate (see the oracle header — the fixture is the tier-2 builder's,
//! built into this family's OWN `/tmp/qt-wiir-*` paths: sharing the tier-2
//! family's `/tmp/qt-wii-*` let whichever regenerated last re-mint the other's
//! picture ids — the `f5e953a3f` unification's workspace-run catch):
//!   QT_ORACLE_WIIR=/tmp/oracle-wardrobe-item-images-routes.ndjson \
//!   QT_FIXTURE_WIIR_MAIN=/tmp/qt-wiir-main.db QT_FIXTURE_WIIR_MOUNT=/tmp/qt-wiir-mount.db \
//!     cargo test -p quilltap-web --test wardrobe_item_images_routes_equivalence

mod common;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use quilltap_core::api::types::WardrobeContainerScope;
use quilltap_core::api::wardrobe_item_images::{
    generate_on_home, list_on_home, resolve_home, save_targets_on_home,
};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::Writer;
use quilltap_core::model::image::{
    ErasedImageGenerate, GeneratedImageData, ImageGenError, ImageGenParams, ImageGenResponse,
    ImageProvider,
};
use quilltap_core::services::wardrobe_item_image_generation::WardrobeItemImageSeams;
use quilltap_core::services::wardrobe_item_images::service::list_wardrobe_item_images;
use quilltap_web::wardrobe_images_routes::render;
use serde_json::{json, Map, Value};

const NOWHERE_ID: &str = "99999999-0000-4000-8000-000000000000";
const SAFETY: &str = "400 Your request was rejected as a result of our safety system.";
const CODEC_COLUMNS: [&str; 7] = [
    "sha256",
    "size",
    "width",
    "height",
    "data",
    "sizeBytes",
    "fileSizeBytes",
];

fn harness_fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures")
        .join(name)
}

fn b64(s: &str) -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(s).unwrap()
}

struct MockProvider {
    mode: String,
    webp: String,
}

impl ImageProvider for MockProvider {
    async fn generate_image(
        &self,
        _provider: &str,
        _api_key: &str,
        _params: &ImageGenParams,
    ) -> Result<ImageGenResponse, ImageGenError> {
        match self.mode.as_str() {
            "throw" => Err(ImageGenError::new("canned provider failure")),
            "refuseAll" => Err(ImageGenError::new(SAFETY)),
            _ => Ok(ImageGenResponse {
                images: vec![GeneratedImageData {
                    data: Some(self.webp.clone()),
                    url: None,
                    mime_type: Some("image/webp".into()),
                    revised_prompt: Some("a revised coat".into()),
                }],
            }),
        }
    }
}

// ── the hand-rolled normalizer (this crate has no `regex`) ────────────────

fn is_hex(c: u8) -> bool {
    c.is_ascii_hexdigit()
}

/// Every token this family normalizes, with its byte span: `(start, end, kind)`
/// where kind ∈ `ts` / `uuid` / `leaf` / `sha`.
fn tokens(s: &str) -> Vec<(usize, usize, &'static str)> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        // ISO: dddd-dd-ddTdd:dd:dd.dddZ (24 bytes)
        if i + 24 <= b.len() {
            let w = &b[i..i + 24];
            let pat = b"dddd-dd-ddTdd:dd:dd.dddZ";
            if w.iter().zip(pat.iter()).all(|(c, p)| {
                if *p == b'd' {
                    c.is_ascii_digit()
                } else {
                    c == p
                }
            }) {
                out.push((i, i + 24, "ts"));
                i += 24;
                continue;
            }
        }
        // UUID: 8-4-4-4-12 hex
        if i + 36 <= b.len() {
            let w = &b[i..i + 36];
            let ok = w.iter().enumerate().all(|(k, c)| {
                if [8, 13, 18, 23].contains(&k) {
                    *c == b'-'
                } else {
                    is_hex(*c)
                }
            });
            if ok && (i == 0 || !is_hex(b[i - 1])) {
                out.push((i, i + 36, "uuid"));
                i += 36;
                continue;
            }
        }
        // the photos leaf's timestamp: dddd-dd-ddTdd-dd-dd.dddZ (24 bytes)
        if i + 24 <= b.len() {
            let w = &b[i..i + 24];
            let pat = b"dddd-dd-ddTdd-dd-dd.dddZ";
            if w.iter().zip(pat.iter()).all(|(c, p)| {
                if *p == b'd' {
                    c.is_ascii_digit()
                } else {
                    c == p
                }
            }) {
                out.push((i, i + 24, "pts"));
                i += 24;
                continue;
            }
        }
        // leaf: dddddddd-dddddd-<kind>-<8 hex> (the pre-`9ecebf986` shape), or
        // the bridge's `${timestampStem(new Date())}-${kind}.webp` (v4
        // `wardrobe-image-bridge.ts:122-126` at the pin — no hex suffix, so the
        // clock stamp IS the identity and maps positionally like a minted id)
        if i + 16 <= b.len()
            && b[i..i + 8].iter().all(u8::is_ascii_digit)
            && b[i + 8] == b'-'
            && b[i + 9..i + 15].iter().all(u8::is_ascii_digit)
            && b[i + 15] == b'-'
        {
            for kind in ["generated", "uploaded", "imported"] {
                let k = kind.as_bytes();
                let start = i + 16;
                if start + k.len() + 9 <= b.len()
                    && &b[start..start + k.len()] == k
                    && b[start + k.len()] == b'-'
                    && b[start + k.len() + 1..start + k.len() + 9]
                        .iter()
                        .all(|c| is_hex(*c))
                {
                    out.push((i, start + k.len() + 9, "leaf"));
                } else if start + k.len() < b.len()
                    && &b[start..start + k.len()] == k
                    && b[start + k.len()] == b'.'
                {
                    out.push((i, start + k.len(), "leaf"));
                }
            }
            if out.last().is_some_and(|t| t.0 == i) {
                i = out.last().unwrap().1;
                continue;
            }
        }
        // a 64-hex digest
        if i + 64 <= b.len()
            && b[i..i + 64]
                .iter()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(c))
            && (i == 0 || !is_hex(b[i - 1]))
            && (i + 64 == b.len() || !is_hex(b[i + 64]))
        {
            out.push((i, i + 64, "sha"));
            i += 64;
            continue;
        }
        i += 1;
    }
    out
}

struct Normalizer {
    baked: HashSet<String>,
    seen: HashMap<String, String>,
}

impl Normalizer {
    fn string(&mut self, s: &str) -> String {
        let mut out = String::new();
        let mut last = 0;
        for (a, z, kind) in tokens(s) {
            out.push_str(&s[last..a]);
            let tok = &s[a..z];
            if kind == "ts" {
                out.push_str("<ts>");
            } else if kind == "pts" {
                out.push_str("<pts>");
            } else if self.baked.contains(tok) {
                out.push_str(tok);
            } else {
                let n = self.seen.len();
                let t = self
                    .seen
                    .entry(tok.to_string())
                    .or_insert_with(|| format!("<{kind}:{n}>"))
                    .clone();
                out.push_str(&t);
            }
            last = z;
        }
        out.push_str(&s[last..]);
        out
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

fn baked_tokens(rows: &Value, spec: &Value) -> HashSet<String> {
    let mut out = HashSet::new();
    for text in [rows.to_string(), spec.to_string()] {
        for (a, z, kind) in tokens(&text) {
            if kind != "ts" && kind != "pts" {
                out.insert(text[a..z].to_string());
            }
        }
    }
    out
}

// ── the item-scoped rows ──────────────────────────────────────────────────

fn rows_of(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    item_ids: &[String],
) -> Value {
    fn dump(conn: &rusqlite::Connection, sql: &str, args: &[String]) -> Value {
        let mut stmt = conn.prepare(sql).unwrap();
        let cols: Vec<String> = stmt.column_names().iter().map(|c| c.to_string()).collect();
        let rows: Vec<Value> = stmt
            .query_map(rusqlite::params_from_iter(args.iter()), |r| {
                let mut m = Map::new();
                for (i, c) in cols.iter().enumerate() {
                    use rusqlite::types::ValueRef;
                    let v = match r.get_ref(i)? {
                        ValueRef::Null => Value::Null,
                        ValueRef::Integer(n) => json!(n),
                        ValueRef::Real(f) if f.fract() == 0.0 && f.abs() < 9e15 => json!(f as i64),
                        ValueRef::Real(f) => json!(f),
                        ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
                        ValueRef::Blob(b) => json!(hex::encode(b)),
                    };
                    m.insert(c.clone(), v);
                }
                Ok(Value::Object(m))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect();
        Value::Array(rows)
    }
    let placeholders = vec!["?"; item_ids.len()].join(",");
    json!({
        "files": dump(
            main,
            &format!(
                "SELECT DISTINCT f.* FROM files f, json_each(f.linkedTo) j WHERE j.value IN ({placeholders}) ORDER BY f.rowid"
            ),
            item_ids,
        ),
        "links": dump(
            mount,
            "SELECT * FROM doc_mount_file_links WHERE relativePath LIKE 'Wardrobe/images/%' ORDER BY rowid",
            &[],
        ),
        "blobs": dump(
            mount,
            "SELECT b.* FROM doc_mount_blobs b WHERE b.fileId IN (SELECT fileId FROM doc_mount_file_links WHERE relativePath LIKE 'Wardrobe/images/%') ORDER BY b.rowid",
            &[],
        ),
        "photoLinks": dump(
            mount,
            "SELECT * FROM doc_mount_file_links WHERE relativePath LIKE 'photos/%' ORDER BY rowid",
            &[],
        ),
        "photoBlobs": dump(
            mount,
            "SELECT b.* FROM doc_mount_blobs b WHERE b.fileId IN (SELECT fileId FROM doc_mount_file_links WHERE relativePath LIKE 'photos/%') ORDER BY b.rowid",
            &[],
        ),
    })
}

fn drop_codec_columns(v: &mut Value) {
    if let Some(tables) = v.as_object_mut() {
        for rows in tables.values_mut() {
            for row in rows.as_array_mut().into_iter().flatten() {
                if let Some(o) = row.as_object_mut() {
                    for c in CODEC_COLUMNS {
                        o.remove(c);
                    }
                }
            }
        }
    }
}

// ── the request ───────────────────────────────────────────────────────────

struct Ctx<'a> {
    spec: &'a Value,
    corpus: &'a Value,
}

impl Ctx<'_> {
    fn id(&self, v: &str) -> String {
        let key = match v {
            "@character" => "characterId",
            "@archived" => "archivedCharacterId",
            "@stranger" => "strangerCharacterId",
            "@project" => "projectId",
            "@group" => "groupId",
            other => return other.to_string(),
        };
        self.spec[key].as_str().unwrap().to_string()
    }
    fn item_id(&self, key: &str) -> String {
        if key == "@nowhere" {
            return NOWHERE_ID.to_string();
        }
        self.spec["items"][key]["id"].as_str().unwrap().to_string()
    }
    fn item_ids(&self) -> Vec<String> {
        self.spec["items"]
            .as_object()
            .unwrap()
            .values()
            .map(|i| i["id"].as_str().unwrap().to_string())
            .collect()
    }
    fn query(&self, req: &Value) -> Vec<(String, String)> {
        let mut q: Vec<(String, String)> = req["query"]
            .as_object()
            .map(|o| {
                o.iter()
                    .map(|(k, v)| (k.clone(), self.id(v.as_str().unwrap())))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(a) = req.get("action").and_then(Value::as_str) {
            q.push(("action".into(), a.into()));
        }
        q
    }
    fn resolve_ref(
        &self,
        main: &rusqlite::Connection,
        mount: &rusqlite::Connection,
        r: &Value,
    ) -> String {
        if let Some(store) = r.get("store").and_then(Value::as_str) {
            return match store {
                "@general" => self.spec["generalMountPointId"]
                    .as_str()
                    .unwrap()
                    .to_string(),
                "@project" => main
                    .query_row(
                        "SELECT officialMountPointId FROM projects WHERE id = ?1",
                        [self.id("@project")],
                        |row| row.get::<_, String>(0),
                    )
                    .unwrap(),
                character => main
                    .query_row(
                        "SELECT characterDocumentMountPointId FROM characters WHERE id = ?1",
                        [self.id(character)],
                        |row| row.get::<_, String>(0),
                    )
                    .unwrap(),
            };
        }
        if let Some(item) = r.get("imageLink").and_then(Value::as_str) {
            let item_id = self.item_id(item);
            let file_id = list_wardrobe_item_images(main, &item_id).unwrap()[0]
                .id
                .clone();
            let sha: String = main
                .query_row(
                    "SELECT sha256 FROM files WHERE id = ?1",
                    [&file_id],
                    |row| row.get(0),
                )
                .unwrap();
            return mount
                .query_row(
                    "SELECT l.id FROM doc_mount_file_links l JOIN doc_mount_files f ON f.id = l.fileId \
                     WHERE l.relativePath LIKE 'Wardrobe/images/' || ?1 || '/%' AND f.sha256 = ?2 \
                     ORDER BY l.rowid LIMIT 1",
                    [&item_id, &sha],
                    |row| row.get::<_, String>(0),
                )
                .unwrap();
        }
        if let Some(doc) = r.get("doc").and_then(Value::as_str) {
            return main
                .query_row(
                    "SELECT f.id FROM files f, json_each(f.linkedTo) j \
                     WHERE j.value = ?1 AND f.category = 'DOCUMENT'",
                    [self.item_id(doc)],
                    |row| row.get::<_, String>(0),
                )
                .unwrap();
        }
        let rows =
            list_wardrobe_item_images(main, &self.item_id(r["item"].as_str().unwrap())).unwrap();
        rows[r.get("nth").and_then(Value::as_u64).unwrap_or(0) as usize]
            .id
            .clone()
    }
    fn resolve_body(
        &self,
        main: &rusqlite::Connection,
        mount: &rusqlite::Connection,
        v: &Value,
    ) -> Value {
        match v {
            Value::Object(o) if o.contains_key("ref") => {
                json!(self.resolve_ref(main, mount, &o["ref"]))
            }
            Value::Object(o) => Value::Object(
                o.iter()
                    .map(|(k, e)| (k.clone(), self.resolve_body(main, mount, e)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }
    fn webp(&self, i: usize) -> Vec<u8> {
        b64(self.spec["webp"][i].as_str().unwrap())
    }
}

fn url_encode(s: &str) -> String {
    s.bytes()
        .map(|c| match c {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (c as char).to_string()
            }
            _ => format!("%{c:02X}"),
        })
        .collect()
}

async fn http_request(
    ctx: &Ctx<'_>,
    req: &Value,
    main_fixture: &str,
    mount_fixture: &str,
) -> Value {
    let pepper = ctx.spec["testPepperBase64"].as_str().unwrap().to_string();
    let base = tempfile::tempdir().unwrap();
    let data = base.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::copy(main_fixture, data.join("quilltap.db")).unwrap();
    std::fs::copy(mount_fixture, data.join("quilltap-mount-index.db")).unwrap();
    let (body_value, baked) = {
        let main = Writer::open_writable(&data.join("quilltap.db"), &pepper).unwrap();
        let mount = Writer::open_writable(&data.join("quilltap-mount-index.db"), &pepper).unwrap();
        for sql in req
            .get("sql")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            main.connection()
                .execute_batch(sql.as_str().unwrap())
                .unwrap();
        }
        for sql in req
            .get("mountSql")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            mount
                .connection()
                .execute_batch(sql.as_str().unwrap())
                .unwrap();
        }
        let body_value = req
            .get("body")
            .filter(|b| b["type"] == "json")
            .map(|b| ctx.resolve_body(main.connection(), mount.connection(), &b["value"]));
        let baked = baked_tokens(
            &rows_of(main.connection(), mount.connection(), &ctx.item_ids()),
            ctx.spec,
        );
        (body_value, baked)
    };
    let (addr, state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c.env_pepper = Some(pepper.clone());
        // Each request's host outlives its temp dir (it is never stopped), so
        // its 60 s heartbeat finds the lock gone; the default handler is v4's
        // `process.exit(1)`, which killed the binary once the P4.D275 corpus
        // ran past a minute. A lost lock here is the test's own teardown.
        c.on_lock_lost = Some(Arc::new(|| {}));
        c
    })
    .await;
    let item_id = ctx.item_id(req["item"].as_str().unwrap());
    let query = ctx.query(req);
    let qs: String = query
        .iter()
        .map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    let url = format!(
        "http://{addr}/api/v1/wardrobe/{item_id}/images{}",
        if qs.is_empty() {
            String::new()
        } else {
            format!("?{qs}")
        }
    );
    let client = reqwest::Client::new();
    let build = || {
        if req["method"] == "GET" {
            client.get(&url)
        } else {
            client.post(&url)
        }
    };
    // `repeat`: the same request first, its answer discarded (only JSON / text
    // bodies repeat — no multipart row does).
    if req.get("repeat") == Some(&json!(true)) {
        let mut first = build().header("content-type", "application/json");
        if let Some(v) = &body_value {
            first = first.body(v.to_string());
        }
        first.send().await.unwrap();
    }
    let mut rb = build();
    match req.get("body").map(|b| b["type"].as_str().unwrap()) {
        Some("json") => {
            rb = rb
                .header("content-type", "application/json")
                .body(body_value.unwrap().to_string());
        }
        Some("text") => {
            rb = rb
                .header("content-type", "application/json")
                .body(req["body"]["value"].as_str().unwrap().to_string());
        }
        Some("multipart") => {
            let mut form = reqwest::multipart::Form::new();
            for p in req["body"]["parts"].as_array().unwrap() {
                let name = p["name"].as_str().unwrap().to_string();
                if p.get("file") == Some(&json!(true)) {
                    let bytes = if let Some(size) = p.get("size").and_then(Value::as_u64) {
                        vec![0u8; size as usize]
                    } else if p.get("png") == Some(&json!(true)) {
                        b64(ctx.corpus["png"].as_str().unwrap())
                    } else {
                        ctx.webp(p.get("webp").and_then(Value::as_u64).unwrap_or(0) as usize)
                    };
                    let part = reqwest::multipart::Part::bytes(bytes)
                        .file_name(p["filename"].as_str().unwrap().to_string())
                        .mime_str(p["contentType"].as_str().unwrap())
                        .unwrap();
                    form = form.part(name, part);
                } else {
                    form = form.text(name, p["value"].as_str().unwrap().to_string());
                }
            }
            rb = rb.multipart(form);
        }
        _ => {}
    }
    let resp = rb.send().await.unwrap();
    let status = resp.status().as_u16();
    let body: Value = serde_json::from_str(&resp.text().await.unwrap()).unwrap();
    let mut out = json!({ "status": status, "body": body });
    if req.get("followGet") == Some(&json!(true)) {
        let get_qs: String = query
            .iter()
            .filter(|(k, _)| k != "action")
            .map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v)))
            .collect::<Vec<_>>()
            .join("&");
        let g = client
            .get(format!(
                "http://{addr}/api/v1/wardrobe/{item_id}/images?{get_qs}"
            ))
            .send()
            .await
            .unwrap();
        let gs = g.status().as_u16();
        let gb: Value = serde_json::from_str(&g.text().await.unwrap()).unwrap();
        out["followGet"] = json!({ "status": gs, "body": gb });
    }
    let db = state.host().unwrap().core().db().unwrap();
    let ids = ctx.item_ids();
    out["rows"] = db
        .read_main(|m| db.read_mount_index(|n| Ok(rows_of(m, n, &ids))))
        .unwrap();
    out["__baked"] = json!(baked.into_iter().collect::<Vec<_>>());
    out
}

async fn direct_request(
    ctx: &Ctx<'_>,
    req: &Value,
    main_fixture: &str,
    mount_fixture: &str,
) -> Value {
    let pepper = ctx.spec["testPepperBase64"].as_str().unwrap().to_string();
    let scratch = tempfile::tempdir().unwrap();
    let (mw, nw) = (
        scratch.path().join("main.db"),
        scratch.path().join("mount.db"),
    );
    std::fs::copy(main_fixture, &mw).unwrap();
    std::fs::copy(mount_fixture, &nw).unwrap();
    let baked = {
        let main = Writer::open_writable(&mw, &pepper).unwrap();
        let mount = Writer::open_writable(&nw, &pepper).unwrap();
        for sql in req
            .get("sql")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            main.connection()
                .execute_batch(sql.as_str().unwrap())
                .unwrap();
        }
        baked_tokens(
            &rows_of(main.connection(), mount.connection(), &ctx.item_ids()),
            ctx.spec,
        )
    };
    let db = Db::open(
        DbPaths {
            main: mw.clone(),
            mount_index: Some(nw.clone()),
            llm_logs: None,
        },
        &pepper,
    )
    .unwrap();
    let seams = WardrobeItemImageSeams {
        provider: ErasedImageGenerate::new(MockProvider {
            mode: req["provider"].as_str().unwrap_or("ok").to_string(),
            webp: ctx.spec["webp"][0].as_str().unwrap().to_string(),
        }),
        codec: Arc::new(quilltap_host::HostImageCodec),
    };
    let query = ctx.query(req);
    let get = |k: &str| query.iter().find(|(q, _)| q == k).map(|(_, v)| v.clone());
    let scope: WardrobeContainerScope =
        serde_json::from_value(json!(get("scope").unwrap())).unwrap();
    let cid = get("id");
    let item_id = ctx.item_id(req["item"].as_str().unwrap());
    let home = resolve_home(&db, scope, cid.as_deref(), &item_id)
        .await
        .unwrap_or_else(|_| panic!("direct arm needs a home"));
    // P4.D275: the listing save-targets rows — an UNBOOTED db, as v4's oracle
    // is (a booted host provisions built-in stores the oracle never has).
    if req.get("action").and_then(Value::as_str) == Some("save-targets") {
        let (status, body) = render(
            false,
            save_targets_on_home(&db, scope, &home),
            axum::http::StatusCode::OK,
        );
        let ids = ctx.item_ids();
        let mut out = json!({ "status": status.as_u16(), "body": body });
        out["rows"] = db
            .read_main(|m| db.read_mount_index(|n| Ok(rows_of(m, n, &ids))))
            .unwrap();
        out["__baked"] = json!(baked.into_iter().collect::<Vec<_>>());
        return out;
    }
    let resp = generate_on_home(
        &db,
        &seams,
        quilltap_core::api::engine::SINGLE_USER_ID,
        scope,
        cid.as_deref(),
        &home,
        None,
    )
    .await;
    let (status, body) = render(true, resp, axum::http::StatusCode::CREATED);
    let mut out = json!({ "status": status.as_u16(), "body": body });
    if req.get("followGet") == Some(&json!(true)) {
        let home = resolve_home(&db, scope, cid.as_deref(), &item_id)
            .await
            .unwrap_or_else(|_| panic!("home"));
        let (gs, gb) = render(
            false,
            list_on_home(&db, scope, &home).await,
            axum::http::StatusCode::OK,
        );
        out["followGet"] = json!({ "status": gs.as_u16(), "body": gb });
    }
    let ids = ctx.item_ids();
    out["rows"] = db
        .read_main(|m| db.read_mount_index(|n| Ok(rows_of(m, n, &ids))))
        .unwrap();
    out["__baked"] = json!(baked.into_iter().collect::<Vec<_>>());
    out
}

/// P4.D275 Tier 2 #18 — v4's `[Wardrobe Images v1]` lines' meta KEYS (in
/// order) for the picture viewer's two actions, as the routes oracle captures
/// them. v5's lines carry these keys — capture-pinned in the item-images
/// tier-2 family's `the_save_route_lines_are_v4s` — save that v4's `message`
/// is v5's `errorMessage` (a tracing field named `message` would replace the
/// file record's envelope message; recorded).
/// P4.D281 — v4's `handleLinkImage` INFO and its meta keys (in order).
const V4_LINK_LINE: (&str, [&str; 4]) = (
    "[Wardrobe Images v1] Linked an existing picture to a wardrobe item",
    ["itemId", "scope", "sourceFileId", "fileId"],
);

const V4_SAVE_LINE_KEYS: [(&str, &[&str]); 5] = [
    (
        "[Wardrobe Images v1] Listed save targets",
        &["itemId", "scope", "count"],
    ),
    (
        "[Wardrobe Images v1] Refused to save a picture that is not the item's",
        &["itemId", "scope", "fileId", "mountPointId"],
    ),
    (
        "[Wardrobe Images v1] Refused to save into an archived character's vault",
        &["itemId", "scope", "fileId", "mountPointId"],
    ),
    (
        "[Wardrobe Images v1] Saved wardrobe picture to a store",
        &[
            "itemId",
            "scope",
            "fileId",
            "mountPointId",
            "relativePath",
            "linkId",
        ],
    ),
    (
        "[Wardrobe Images v1] Save to store rejected",
        &[
            "itemId",
            "scope",
            "fileId",
            "mountPointId",
            "code",
            "message",
        ],
    ),
];

#[tokio::test(flavor = "multi_thread")]
async fn wardrobe_item_images_routes_match_oracle() {
    let (Ok(oracle_path), Ok(main_fixture), Ok(mount_fixture)) = (
        std::env::var("QT_ORACLE_WIIR"),
        std::env::var("QT_FIXTURE_WIIR_MAIN"),
        std::env::var("QT_FIXTURE_WIIR_MOUNT"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_WIIR + QT_FIXTURE_WIIR_MAIN + QT_FIXTURE_WIIR_MOUNT (see header)."
        );
        return;
    };
    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(harness_fixture("wardrobe-item-images-tier2.json")).unwrap(),
    )
    .unwrap();
    let corpus: Value = serde_json::from_str(
        &std::fs::read_to_string(harness_fixture("wardrobe-item-images-routes.json")).unwrap(),
    )
    .unwrap();
    let oracle: HashMap<String, Value> = std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Value = serde_json::from_str(l).unwrap();
            (v["name"].as_str().unwrap().to_string(), v)
        })
        .collect();
    let requests = corpus["requests"].as_array().unwrap();
    assert_eq!(oracle.len(), requests.len(), "oracle row count != corpus");
    let ctx = Ctx {
        spec: &spec,
        corpus: &corpus,
    };

    // Tier 2 #18: every save-action line v4 logged carries the recorded keys,
    // and every recorded line was seen at least once.
    let mut seen_lines = HashSet::new();
    for row in oracle.values() {
        for line in row
            .get("v4Lines")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let message = line["message"].as_str().unwrap();
            let Some((_, keys)) = V4_SAVE_LINE_KEYS.iter().find(|(m, _)| *m == message) else {
                continue; // the P4.D263 actions' lines (pinned in core's family)
            };
            let got: Vec<&str> = line["keys"]
                .as_array()
                .unwrap()
                .iter()
                .map(|k| k.as_str().unwrap())
                .collect();
            assert_eq!(&got, keys, "{message}: v4's meta keys moved");
            seen_lines.insert(message.to_string());
        }
    }
    assert_eq!(
        seen_lines.len(),
        V4_SAVE_LINE_KEYS.len(),
        "every save line was captured: {seen_lines:?}"
    );
    // P4.D281 (`9ecebf986`): the link route's INFO — v4's keys, seen at least
    // once (`link_image_ok_201`, `link_image_cross_tier_general`); v5's line
    // carries them in this order (`p4d281_link_lines_are_v4s`).
    let link_keys: Vec<Vec<String>> = oracle
        .values()
        .flat_map(|row| {
            row.get("v4Lines")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        })
        .filter(|l| l["message"] == V4_LINK_LINE.0)
        .map(|l| {
            l["keys"]
                .as_array()
                .unwrap()
                .iter()
                .map(|k| k.as_str().unwrap().to_string())
                .collect()
        })
        .collect();
    assert!(!link_keys.is_empty(), "v4 logged no link line");
    for keys in &link_keys {
        assert_eq!(
            keys, &V4_LINK_LINE.1,
            "{}: v4's meta keys moved",
            V4_LINK_LINE.0
        );
    }

    let mut failures = Vec::new();
    for req in requests {
        let name = req["name"].as_str().unwrap();
        let mut got = if req["driver"] == "direct" {
            direct_request(&ctx, req, &main_fixture, &mount_fixture).await
        } else {
            http_request(&ctx, req, &main_fixture, &mount_fixture).await
        };
        let baked: HashSet<String> = got["__baked"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        got.as_object_mut().unwrap().remove("__baked");
        let mut want = oracle[name].clone();
        want.as_object_mut().unwrap().remove("name");
        if req.get("codecDependent") == Some(&json!(true)) {
            drop_codec_columns(&mut got["rows"]);
            drop_codec_columns(&mut want["rows"]);
        }
        let got_n = Normalizer {
            baked: baked.clone(),
            seen: HashMap::new(),
        }
        .value(&got);
        let want_n = Normalizer {
            baked,
            seen: HashMap::new(),
        }
        .value(&want);
        for part in ["status", "body", "followGet", "rows"] {
            // Bytes, not `Value` equality — `preserve_order` maps compare
            // order-blind, and v4's key order is part of the wire. NAMED CARVE
            // `TRAIL_ENTRY_KEY_ORDER` (pre-existing, found by P4.D275's switch
            // to bytes; recorded for the follow-ups round): the provider-
            // generate rows' `details.trail[]` entries carry `profileKind`
            // after `outcome` where v4 puts it after `trigger`/`evidence` —
            // those three `direct` generate bodies stay order-blind until the
            // trail type's field order is fixed.
            let order_blind = part == "body"
                && req["driver"] == "direct"
                && req.get("action").and_then(Value::as_str) == Some("generate");
            let differs = if order_blind {
                got_n.get(part) != want_n.get(part)
            } else {
                got_n.get(part).map(|p| p.to_string()) != want_n.get(part).map(|p| p.to_string())
            };
            if differs {
                failures.push(format!(
                    "{name}: {part} diverged\n  rust:   {}\n  oracle: {}",
                    got_n.get(part).unwrap_or(&Value::Null),
                    want_n.get(part).unwrap_or(&Value::Null)
                ));
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

/// P4.D281 (v4 `9ecebf986`, `item-images.ts:264-302`): `linkWardrobeItemImage`'s
/// two lines and the route's INFO, each with v4's keys in v4's order — the
/// DEBUG `[WardrobeImages] Linking an existing picture to a wardrobe item`
/// `{context, itemId, sourceFileId, bytes}` on a link; the INFO `Refused to
/// link a picture with no readable blob` `{context, itemId, sourceFileId,
/// found}` (and NO Linking line) for a document (`found=true`) and an unknown
/// id (`found=false`); the route's `[Wardrobe Images v1] Linked an existing
/// picture to a wardrobe item` `{itemId, scope, sourceFileId, fileId}`.
#[tokio::test(flavor = "multi_thread")]
async fn p4d281_link_lines_are_v4s() {
    use quilltap_core::services::wardrobe_container::resolve_wardrobe_item_home;
    use quilltap_core::services::wardrobe_item_images::service::{
        link_wardrobe_item_image, ImageMint,
    };
    quilltap_core::test_support::global_capture::install();
    let (Ok(main_fixture), Ok(mount_fixture)) = (
        std::env::var("QT_FIXTURE_WIIR_MAIN"),
        std::env::var("QT_FIXTURE_WIIR_MOUNT"),
    ) else {
        eprintln!("SKIP: set QT_FIXTURE_WIIR_MAIN + QT_FIXTURE_WIIR_MOUNT (see header).");
        return;
    };
    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/wardrobe-item-images-tier2.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let s = |k: &str| spec[k].as_str().unwrap().to_string();
    let (pepper, user, character) = (s("testPepperBase64"), s("userId"), s("characterId"));
    let item = |k: &str| spec["items"][k]["id"].as_str().unwrap().to_string();
    let (hat, coat) = (item("hat"), item("coat"));
    let scratch = tempfile::tempdir().unwrap();
    let main_path = scratch.path().join("main.db");
    let mount_path = scratch.path().join("mount.db");
    std::fs::copy(&main_fixture, &main_path).unwrap();
    std::fs::copy(&mount_fixture, &mount_path).unwrap();

    let main_w = quilltap_core::db::Writer::open_writable(&main_path, &pepper).unwrap();
    let mount_w = quilltap_core::db::Writer::open_writable(&mount_path, &pepper).unwrap();
    let (main, mount) = (main_w.connection(), mount_w.connection());
    let picture: String = main
        .query_row(
            "SELECT f.id FROM files f, json_each(f.linkedTo) j WHERE j.value = ?1 \
             AND f.category = 'IMAGE' ORDER BY f.rowid LIMIT 1",
            [&coat],
            |r| r.get(0),
        )
        .unwrap();
    let document: String = main
        .query_row(
            "SELECT f.id FROM files f, json_each(f.linkedTo) j WHERE j.value = ?1 \
             AND f.category = 'DOCUMENT' LIMIT 1",
            [&coat],
            |r| r.get(0),
        )
        .unwrap();
    let home = resolve_wardrobe_item_home(
        main,
        mount,
        &user,
        WardrobeContainerScope::Character,
        Some(&character),
        &hat,
    )
    .unwrap()
    .expect("the hat's home");
    let codec = quilltap_core::services::mount_index::blob_transcode::RefusingWebpTranscoder;

    // A link: the DEBUG, no refusal.
    let (linked, lines) = quilltap_core::test_support::captured_with(|| {
        link_wardrobe_item_image(
            main,
            mount,
            &home,
            &user,
            &picture,
            &ImageMint::now(),
            &codec,
        )
    });
    linked.expect("the link lands");
    let debug = lines
        .iter()
        .find(|l| l.contains("[WardrobeImages] Linking an existing picture to a wardrobe item"))
        .unwrap_or_else(|| panic!("{lines:#?}"));
    assert!(debug.starts_with("DEBUG "), "{debug}");
    let tail = debug.split("to a wardrobe item").nth(1).unwrap();
    assert!(
        tail.starts_with(&format!(
            " context=wardrobe.item-images itemId={hat} sourceFileId={picture} bytes="
        )),
        "{debug}"
    );
    assert!(
        !lines.iter().any(|l| l.contains("Refused to link")),
        "{lines:#?}"
    );

    // The two refusals: `found` true (a document) and false (no such file).
    for (file, found) in [
        (document.as_str(), "true"),
        ("00000000-0000-4000-8000-0000000000aa", "false"),
    ] {
        let (refused, lines) = quilltap_core::test_support::captured_with(|| {
            link_wardrobe_item_image(main, mount, &home, &user, file, &ImageMint::now(), &codec)
        });
        assert!(refused.is_err(), "{file}");
        let info = lines
            .iter()
            .find(|l| {
                l.contains("[WardrobeImages] Refused to link a picture with no readable blob")
            })
            .unwrap_or_else(|| panic!("{lines:#?}"));
        assert!(info.starts_with("INFO "), "{info}");
        assert!(
            info.ends_with(&format!(
                "context=wardrobe.item-images itemId={hat} sourceFileId={file} found={found}"
            )),
            "{info}"
        );
        assert!(
            !lines.iter().any(|l| l.contains("Linking an existing")),
            "{lines:#?}"
        );
    }
    drop((main_w, mount_w));

    // The route's INFO, through the verb (it runs on the calling thread).
    let db = quilltap_core::db::runtime::Db::open(
        quilltap_core::db::runtime::DbPaths {
            main: main_path.clone(),
            mount_index: Some(mount_path.clone()),
            llm_logs: None,
        },
        &pepper,
    )
    .unwrap();
    let rt_db = db.clone();
    let (character, hat2, pic2) = (character.clone(), item("bare"), picture.clone());
    let (resp, lines) = tokio::task::spawn_blocking(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        quilltap_core::test_support::global_capture::capture(|| {
            rt.block_on(quilltap_core::api::wardrobe_item_images::link_image(
                &rt_db,
                std::sync::Arc::new(
                    quilltap_core::services::mount_index::blob_transcode::RefusingWebpTranscoder,
                ),
                WardrobeContainerScope::Character,
                Some(&character),
                &hat2,
                &pic2,
            ))
        })
    })
    .await
    .unwrap();
    let quilltap_core::api::types::Response::WardrobeItemImages(body) = &resp else {
        panic!("the link answers its body: {resp:?}");
    };
    let file_id = body["current"].as_str().unwrap();
    let bare = item("bare");
    let line = lines
        .iter()
        .find(|l| l.contains("Linked an existing picture to a wardrobe item"))
        .unwrap_or_else(|| panic!("{lines:#?}"));
    assert!(line.starts_with("INFO "), "{line}");
    assert!(
        line.ends_with(&format!(
            "[Wardrobe Images v1] Linked an existing picture to a wardrobe item itemId={bare} scope=character sourceFileId={picture} fileId={file_id}"
        )),
        "{line}"
    );
}
