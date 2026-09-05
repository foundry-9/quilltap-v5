//! P4.73 — the `/api/v1/images` COLLECTION route differential: `api::images::*`
//! vs v4's REAL `app/api/v1/images/route.ts` (+ the `[id]` DELETE arm).
//!
//! Both sides run each case on a FRESH copy of the committed images fixture
//! (every seeded id pinned, so reads diff with no remap). Read cases diff
//! status + body with KEY ORDER preserved — the list projection's key sequence
//! is the whole comparand for the omit-null rule (v4 hydrates a NULL column to
//! `undefined` and `JSON.stringify` drops the key, so `width` / `height` /
//! `generationPrompt` / `generationModel` are ABSENT, not null, while `url` —
//! which comes from the route's own ternary — stays an explicit null). Write
//! cases additionally diff the post-mutation `files` + `characters` dumps, so a
//! refusal proves it wrote NOTHING rather than only that it answered 400.
//!
//! Coverage is asserted by SHAPE: every case name the oracle emitted must have
//! been driven here and vice versa (`harness-corpus-shape-constants-rot`).
//!
//! Generate the fixture copy + oracle (Node 24, from the v4 checkout — the full
//! recipe lives in the .test.ts header):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-images-routes-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/images-routes.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/images-collection.json" "$TMPO/fixtures/"
//!   cp "$V5W/crates/quilltap-web/tests/fixtures/images-main.db" /tmp/qt-imgcol-main.db
//!   cp "$V5W/crates/quilltap-web/tests/fixtures/images-mount.db" /tmp/qt-imgcol-mount.db
//!   cp "$V5W/crates/quilltap-web/tests/fixtures/images-main.db.meta.json" /tmp/qt-imgcol-main.db.meta.json
//!   cd ~/source/quilltap-server
//!   TZ=UTC QT_FIXTURE_IMGCOL_MAIN=/tmp/qt-imgcol-main.db QT_FIXTURE_IMGCOL_MOUNT=/tmp/qt-imgcol-mount.db QT_ORACLE_OUT=/tmp/oracle-images-routes.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- images-routes
//! Run:
//!   QT_ORACLE_IMAGES_ROUTES=/tmp/oracle-images-routes.ndjson \
//!     cargo test -p quilltap-harness --test images_routes_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::{json, Value};

use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};

/// The seeded `files` ids. A row OUTSIDE this set was minted by the case, and
/// its `sha256`/`size` are therefore codec-dependent (v4 encodes WebP through
/// sharp, v5 through the harness codec below) — the D19 rule: compare the
/// POLICY, never the WebP bytes.
const SEEDED_FILE_IDS: &[&str] = &[
    "f0000000-0000-4000-8000-000000000001",
    "f0000000-0000-4000-8000-000000000002",
    "f0000000-0000-4000-8000-000000000003",
    "f0000000-0000-4000-8000-000000000004",
    "f0000000-0000-4000-8000-000000000005",
    "f0000000-0000-4000-8000-000000000006",
    "f0000000-0000-4000-8000-000000000007",
    "f0000000-0000-4000-8000-000000000008",
    "f0000000-0000-4000-8000-000000000009",
    "f0000000-0000-4000-8000-00000000000a",
    "f0000000-0000-4000-8000-0000000000b1",
    // P4.76 item (a): referenced by CHAR_ARCH_PEER and CHAR_ARCHIVED.
    "f0000000-0000-4000-8000-00000000000c",
];

/// The Rust side's pixel codec: byte-CHANGING (so `convert_to_webp` reports
/// `was_converted` and the WebP policy is genuinely exercised) and measuring
/// 1x1, which is what REAL sharp measured for every image in this corpus —
/// both the 1x1 PNG and the 1x1 SVG, whose `width`/`height` v4 stores.
///
/// A passthrough codec would leave the PNG as `image/png` and the whole
/// transcode policy would go unmeasured; a measuring-free codec would drop the
/// dimensions v4 records.
struct FixedDimsPrefixCodec;

impl quilltap_core::services::file_storage::PixelCodec for FixedDimsPrefixCodec {
    fn encode_webp(
        &self,
        bytes: &[u8],
        _quality: i64,
        _effort: Option<i64>,
        _animated: bool,
    ) -> Result<Vec<u8>, String> {
        let mut out = b"QTAP-P473-WEBP:".to_vec();
        out.extend_from_slice(bytes);
        Ok(out)
    }

    fn measure(&self, _bytes: &[u8]) -> (Option<i64>, Option<i64>) {
        (Some(1), Some(1))
    }
}

/// The same 1x1 PNG the oracle feeds v4 — real bytes, so sharp actually
/// transcodes on that side.
fn png_1x1() -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
        )
        .expect("the corpus PNG")
}

/// The SVG the oracle feeds v4 — a passthrough on both sides, so its stored
/// bytes (and sha) ARE comparable.
const SVG_BYTES: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"></svg>"#;

/// The ingest seams, matching what the production engine hands these arms
/// (`Engine::images_ingest_deps`) except that the codec is the harness's
/// byte-changing one.
fn ingest_deps() -> quilltap_core::api::images::IngestDeps {
    quilltap_core::api::images::IngestDeps {
        codec: std::sync::Arc::new(FixedDimsPrefixCodec),
        backend: std::sync::Arc::new(
            quilltap_core::services::file_storage::NotConfiguredStorageBackend,
        ),
    }
}

/// A canned `fetch`, the row's own wire — the P4.D138 shape.
struct CannedFetch {
    status: u16,
    status_text: String,
    content_type: String,
    bytes: Vec<u8>,
}

impl CannedFetch {
    fn ok(content_type: &str, bytes: Vec<u8>) -> Self {
        Self {
            status: 200,
            status_text: "OK".to_string(),
            content_type: content_type.to_string(),
            bytes,
        }
    }
}

/// The seam's future type, named so the impl below reads (clippy's
/// `type_complexity`).
type FetchFuture<'a> = std::pin::Pin<
    Box<
        dyn std::future::Future<
                Output = Result<quilltap_core::api::images::ImageFetchResponse, String>,
            > + Send
            + 'a,
    >,
>;

impl quilltap_core::api::images::ImageImportFetch for CannedFetch {
    fn get(&self, _url: &str) -> FetchFuture<'_> {
        let resp = quilltap_core::api::images::ImageFetchResponse {
            status: self.status,
            status_text: self.status_text.clone(),
            content_type: self.content_type.clone(),
            bytes: self.bytes.clone(),
        };
        Box::pin(async move { Ok(resp) })
    }
}

/// v5's `SINGLE_USER_ID` — the fixture's owner.
const USER_A: &str = "11111111-1111-4111-8111-111111111111";

// The pinned ids the fixture bakes (lockstep with the builder + the .test.ts).
const CHAR_TAG: &str = "c1000000-0000-4000-8000-000000000003";
const THEME_TAG: &str = "ee000000-0000-4000-8000-000000000001";
/// The literal bytes the fixture builder stores behind F_INUSE
/// (`build-images-collection-fixture.ts:270`), as `image/webp` — a
/// PASSTHROUGH type on both sides, so re-uploading them is the dedup arm.
const IN_USE_BYTES: &[u8] = b"RIFFfixture-webp-bytes-for-F_INUSE";
const F_TAGGED: &str = "f0000000-0000-4000-8000-000000000001";
const F_INUSE: &str = "f0000000-0000-4000-8000-000000000005";
const F_ORPHAN: &str = "f0000000-0000-4000-8000-000000000006";
const F_PLAIN: &str = "f0000000-0000-4000-8000-000000000007";
const F_NOKEY_INUSE: &str = "f0000000-0000-4000-8000-00000000000a";
/// P4.76 item (a): referenced by an ARCHIVED character and by a normal peer.
const F_ORPHAN_ARCH: &str = "f0000000-0000-4000-8000-00000000000c";
const F_DOC: &str = "f0000000-0000-4000-8000-000000000009";
const MISSING: &str = "f0000000-0000-4000-8000-00000000dead";

#[derive(serde::Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/images-collection.json")
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-imgcol-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("images-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("images-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db")
}

fn status_of(kind: ErrorKind) -> u16 {
    match kind {
        ErrorKind::BadRequest => 400,
        ErrorKind::Unauthorized => 401,
        ErrorKind::Forbidden => 403,
        ErrorKind::NotFound => 404,
        ErrorKind::Conflict => 409,
        ErrorKind::Unprocessable => 422,
        ErrorKind::Locked => 503,
        ErrorKind::Unavailable => 503,
        ErrorKind::Internal => 500,
    }
}

/// Integer-valued floats → integers (JS-number parity). Applied WITHOUT
/// re-sorting keys: this family's whole point is that the key SEQUENCE matches.
fn canon_numbers(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15 {
                    *v = Value::Number((f as i64).into());
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(canon_numbers),
        Value::Object(o) => o.iter_mut().for_each(|(_, x)| canon_numbers(x)),
        _ => {}
    }
}

fn canon(v: &Value) -> Value {
    let mut c = v.clone();
    canon_numbers(&mut c);
    c
}

/// The ordered key sequence of every object in a value, depth-first. A plain
/// value equality would pass on a re-ordered projection because `serde_json`
/// is built with `preserve_order` — this makes the ordering an EXPLICIT
/// assertion rather than an implicit one, so a future map-collecting rewrite
/// cannot silently lose it.
fn key_paths(v: &Value, at: String, out: &mut Vec<(String, Vec<String>)>) {
    match v {
        Value::Object(o) => {
            out.push((at.clone(), o.keys().cloned().collect()));
            for (k, x) in o {
                key_paths(x, format!("{at}.{k}"), out);
            }
        }
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                key_paths(x, format!("{at}[{i}]"), out);
            }
        }
        _ => {}
    }
}

/// Dump the two tables the write cases mutate, matching the oracle's
/// `dumpTables()` column-for-column.
fn dump_tables(db: &Db) -> Value {
    let files = db
        .read_main(|c| {
            dump_query(
                c,
                "SELECT id, userId, sha256, originalFilename, mimeType, size, width, height, \
                 source, category, linkedTo, tags, description, generationPrompt, \
                 generationModel, generationRevisedPrompt, storageKey, fileStatus \
                 FROM files ORDER BY id",
            )
        })
        .expect("dump files");
    let characters = db
        .read_main(|c| {
            dump_query(
                c,
                "SELECT id, defaultImageId, avatarOverrides FROM characters ORDER BY id",
            )
        })
        .expect("dump characters");
    let links = db
        .read_mount_index(|c| {
            dump_query(
                c,
                "SELECT l.relativePath, l.fileName, l.originalMimeType, f.sha256, \
                 f.fileSizeBytes, f.fileType \
                 FROM doc_mount_file_links l JOIN doc_mount_files f ON f.id = l.fileId \
                 WHERE l.mountPointId IN ('80000000-0000-4000-8000-000000000001', \
                 '80000000-0000-4000-8000-000000000002') ORDER BY l.relativePath",
            )
        })
        .expect("dump links");
    serde_json::json!({ "files": files, "characters": characters, "links": links })
}

/// Normalize what a CASE minted. Two separate reasons, kept distinct:
///
/// * the row id and its blob id are fresh UUIDs on both sides — nondeterminism,
///   not divergence;
/// * a minted row that was TRANSCODED carries WebP bytes the two
///   implementations encode differently (v4 through sharp, v5 through
///   [`FixedDimsPrefixCodec`]), so its `sha256` and byte length are not
///   comparable — the POLICY around them is. Applied to BOTH sides.
///
/// Deliberately narrow: a SEEDED row keeps its exact sha (both sides read the
/// same fixture bytes), and a minted row that was NOT transcoded — the SVG
/// passthrough — keeps its sha too, which is what makes `upload_svg` a real
/// byte-level equality rather than a blanked one.
fn blank_codec_dependent(v: &mut Value) {
    if let Some(files) = v.get_mut("files").and_then(Value::as_array_mut) {
        for row in files {
            let seeded = row
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| SEEDED_FILE_IDS.contains(&id));
            let webp = row.get("mimeType").and_then(Value::as_str) == Some("image/webp");
            if !seeded {
                // The row id and the blob id inside its storage key are minted
                // per run on both sides.
                blank(row, "id");
                blank(row, "storageKey");
            }
            if !seeded && webp {
                blank(row, "sha256");
                blank(row, "size");
            }
        }
    }
    // Both dumps are `ORDER BY id`, and the id a case minted is a fresh UUID —
    // so the SORT POSITION of the new row is a coin flip once the id itself is
    // blanked (its neighbours are pinned `f0000000-…` ids, and a minted UUID
    // lands either side of them). Re-sort on a remap-invariant key: seeded rows
    // keep their pinned id order, and the minted row goes last.
    //
    // That is a TOTAL order only because at most ONE row per case is minted,
    // which the assertion below holds (`a-canonicalizer-sort-key-must-be-a-
    // total-order`).
    if let Some(files) = v.get_mut("files").and_then(Value::as_array_mut) {
        let minted = files
            .iter()
            .filter(|r| r.get("id").and_then(Value::as_str) == Some("<id>"))
            .count();
        assert!(
            minted <= 1,
            "the sort key assumes at most one minted row per case; found {minted}"
        );
        files.sort_by_key(|r| {
            r.get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        });
    }
    if let Some(links) = v.get_mut("links").and_then(Value::as_array_mut) {
        for row in links {
            let path = row
                .get("relativePath")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            // `images/in-use.webp` is the fixture's own seeded blob.
            if path != "images/in-use.webp" && path.ends_with(".webp") {
                blank(row, "sha256");
                blank(row, "fileSizeBytes");
            }
        }
    }
}

fn blank(row: &mut Value, key: &str) {
    if let Some(o) = row.as_object_mut() {
        if o.contains_key(key) {
            o.insert(key.to_string(), Value::String(format!("<{key}>")));
        }
    }
}

/// The comparand the blanking above gives up, restored WITHIN each tree: the
/// `files` row's `sha256` must name the bytes actually stored for it. v4 and
/// v5 disagree on WHAT those bytes are; both must agree that the row describes
/// them (`within-tree-equality-comparand`).
fn within_tree_sha_agrees(tables: &Value) -> Result<(), String> {
    let files = tables["files"].as_array().ok_or("no files")?;
    let links = tables["links"].as_array().ok_or("no links")?;
    for f in files {
        let Some(key) = f.get("storageKey").and_then(Value::as_str) else {
            continue;
        };
        if !key.starts_with("mount-blob:") {
            continue;
        }
        let name = f
            .get("originalFilename")
            .and_then(Value::as_str)
            .unwrap_or_default();
        // The link row is found by the stored basename (the bridge renames a
        // transcoded upload to .webp, and `originalFilename` follows).
        let Some(link) = links
            .iter()
            .find(|l| l.get("fileName").and_then(Value::as_str) == Some(name))
        else {
            continue;
        };
        let fs = f.get("sha256").and_then(Value::as_str).unwrap_or_default();
        let ls = link
            .get("sha256")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if fs != ls {
            return Err(format!(
                "the files row for {name} carries sha {fs} but the stored blob is {ls}"
            ));
        }
    }
    Ok(())
}

fn dump_query(conn: &rusqlite::Connection, sql: &str) -> Result<Value, quilltap_core::db::DbError> {
    let mut stmt = conn.prepare(sql)?;
    let cols: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let rows = stmt.query_map([], |row| {
        let mut m = serde_json::Map::new();
        for (i, name) in cols.iter().enumerate() {
            let v = match row.get_ref(i)? {
                rusqlite::types::ValueRef::Null => Value::Null,
                rusqlite::types::ValueRef::Integer(x) => Value::from(x),
                // A REAL cell is a JS Number on v4's side; `canon_numbers`
                // collapses the integer-valued ones on BOTH sides afterwards.
                rusqlite::types::ValueRef::Real(f) => Value::from(f),
                rusqlite::types::ValueRef::Text(t) => {
                    Value::String(String::from_utf8_lossy(t).to_string())
                }
                rusqlite::types::ValueRef::Blob(_) => Value::String("<blob>".to_string()),
            };
            m.insert(name.clone(), v);
        }
        Ok(Value::Object(m))
    })?;
    Ok(Value::Array(rows.collect::<Result<Vec<_>, _>>()?))
}

/// What a case produced on the v5 side.
struct Got {
    status: u16,
    body: Value,
    tables: Option<Value>,
}

fn from_response(resp: Response, success_status: u16, tables: Option<Value>) -> Got {
    match resp {
        Response::Images(v) => Got {
            status: success_status,
            body: v,
            tables,
        },
        // v4's `badRequest(message, details)` puts BOTH keys on the wire; the
        // details-bearing refusals render through `validation_wire_body`
        // exactly as `images_routes.rs`'s edge does. Rendering only `{error}`
        // here would have made the `Image is in use` bag unmeasurable.
        Response::Error(e) => Got {
            status: status_of(e.kind),
            body: e
                .validation_wire_body()
                .unwrap_or_else(|| serde_json::json!({ "error": e.message })),
            tables,
        },
        other => panic!("unexpected response variant: {other:?}"),
    }
}

#[test]
fn images_routes_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_IMAGES_ROUTES") else {
        eprintln!("SKIP: set QT_ORACLE_IMAGES_ROUTES (see the test header).");
        return;
    };
    let text = std::fs::read_to_string(&oracle_path).unwrap();
    assert!(
        !text.trim().is_empty(),
        "{oracle_path} is EMPTY — the regen truncated it before failing (ledger §5.1)"
    );
    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }

    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).expect("spec");

    let mut driven: Vec<String> = Vec::new();
    let mut failed: Vec<String> = Vec::new();

    // ── GET: the list projection ────────────────────────────────────────────
    let list_cases: &[(&str, Option<&str>)] = &[
        ("list_all", None),
        // v4's `if (tagId)` is JS-falsy, so `?tagId=` never filters — the web
        // edge folds the empty value to absent, which is what this drives.
        ("list_tag_empty", None),
        ("list_tag_character", Some(CHAR_TAG)),
        ("list_tag_theme", Some(THEME_TAG)),
        ("list_tag_unmatched", Some(F_TAGGED)),
        // v4 reads `searchParams.get` — FIRST wins, so the edge passes CHAR_TAG.
        ("list_tag_duplicated", Some(CHAR_TAG)),
    ];
    for (name, tag) in list_cases {
        driven.push((*name).to_string());
        let db = fresh_db(&spec, name);
        let got = from_response(
            quilltap_core::api::images::images_list(&db, USER_A, *tag),
            200,
            None,
        );
        compare(name, &got, &oracle, &mut failed);
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    // ── POST: upload (multipart) ────────────────────────────────────────────
    // The web edge parses the multipart body and the `tags` JSON string; the
    // arms it owns alone (`No file provided`, `Invalid tags JSON`, the
    // truthy-non-array 500, `Invalid content type`, the malformed-body 500s)
    // are edge-level and are pinned v5-side in
    // `crates/quilltap-web/tests/images_edge_routes.rs` (the §3 review of the
    // follow-ups round 2 found this comment naming a driver that did not exist).
    let png = png_1x1();
    let svg = SVG_BYTES.to_vec();
    /// One multipart upload arm: case name, filename, content type, bytes, and
    /// the RAW parsed `tags` array the edge hands the verb.
    struct UploadCase {
        name: &'static str,
        filename: &'static str,
        content_type: &'static str,
        bytes: Vec<u8>,
        tags: Option<Vec<Value>>,
    }
    let up = |name, filename, content_type, bytes, tags| UploadCase {
        name,
        filename,
        content_type,
        bytes,
        tags,
    };
    let upload_cases: Vec<UploadCase> = vec![
        up("upload_png", "shot.png", "image/png", png.clone(), None),
        up("upload_svg", "mark.svg", "image/svg+xml", svg, None),
        up(
            "upload_disallowed_type",
            "note.txt",
            "text/plain",
            b"x".to_vec(),
            None,
        ),
        up(
            "upload_oversize",
            "big.png",
            "image/png",
            vec![0u8; 10 * 1024 * 1024 + 1],
            None,
        ),
        up(
            "upload_tags_raw_tagid",
            "shot.png",
            "image/png",
            png.clone(),
            Some(vec![json!({"tagType": "THEME", "tagId": 5})]),
        ),
        up(
            "upload_tags_string_tagid",
            "shot.png",
            "image/png",
            png.clone(),
            Some(vec![json!({"tagType": "CHARACTER", "tagId": CHAR_TAG})]),
        ),
        // The `?action=` fall-through: the EDGE decides, and it reaches the
        // same verb, so the core-side call is identical to `upload_png`'s.
        up(
            "upload_action_unknown",
            "shot.png",
            "image/png",
            png.clone(),
            None,
        ),
        // ── The dedup arms (the order's Tier 1 item 5 named them; the §3
        // review of the follow-ups round 2 found them missing — and the code
        // WRONG behind them). F_INUSE's blob holds literal bytes stored as
        // `image/webp`, which both implementations pass through UNTRANSCODED,
        // so the same bytes hash to F_INUSE's `sha256` on both sides and the
        // upload takes `createFile`'s dedup branch (`images-v2.ts:117-137`).
        // No growth → the existing row comes back untouched.
        up(
            "upload_dedup_existing_notag",
            "in-use-again.webp",
            "image/webp",
            IN_USE_BYTES.to_vec(),
            None,
        ),
        // Growth → v4 `repos.files.update({linkedTo})` on the EXISTING row,
        // then the `addTag` loop: the receipt names F_INUSE (its stored
        // filename, not the upload's), and no row is minted.
        up(
            "upload_dedup_existing_grows",
            "in-use-again.webp",
            "image/webp",
            IN_USE_BYTES.to_vec(),
            Some(vec![json!({"tagType": "CHARACTER", "tagId": CHAR_TAG})]),
        ),
        // Growth with a RAW id → v4's `update` re-validates against
        // `FileEntrySchema` BEFORE any write and throws: 400, row untouched,
        // no orphan (unlike the create arm, where the bridge write precedes
        // the throw). v5 merged and answered 201 until the unification fix.
        up(
            "upload_dedup_existing_raw_tagid",
            "in-use-again.webp",
            "image/webp",
            IN_USE_BYTES.to_vec(),
            Some(vec![json!({"tagType": "THEME", "tagId": 5})]),
        ),
    ];
    for c in upload_cases {
        let name = c.name;
        driven.push(name.to_string());
        let db = fresh_db(&spec, name);
        let resp = rt.block_on(quilltap_core::api::images::image_upload(
            &db,
            ingest_deps(),
            USER_A,
            c.filename,
            c.content_type,
            c.bytes,
            c.tags,
        ));
        let tables = dump_tables(&db);
        if let Err(e) = within_tree_sha_agrees(&tables) {
            failed.push(format!("{name}: {e}"));
        }
        let got = from_response(resp, 201, Some(tables));
        compare(name, &got, &oracle, &mut failed);
    }

    // ── The dedup-ORPHAN arm (two steps, the same two on both sides) ────────
    // `createFile` finds the row by sha, asks the storage manager whether the
    // bytes still exist (`mountBlobExists` → `docMountBlobs.findById`), and on
    // a miss deletes the metadata row and re-uploads (`images-v2.ts:138-146`:
    // "Cleaning up orphaned file metadata before re-upload"). The fixture's
    // F_ORPHAN carries a tag hash no bytes can produce, so the arm plants its
    // own orphan: upload the SVG (passes through untranscoded, so its sha is
    // the same on both sides), delete its blob row, upload it again.
    {
        let name = "upload_dedup_orphan_cleanup";
        driven.push(name.to_string());
        let db = fresh_db(&spec, name);
        let first = rt.block_on(quilltap_core::api::images::image_upload(
            &db,
            ingest_deps(),
            USER_A,
            "mark.svg",
            "image/svg+xml",
            SVG_BYTES.to_vec(),
            None,
        ));
        let first_id = match &first {
            quilltap_core::api::Response::Images(v) => v["data"]["id"]
                .as_str()
                .expect("the first upload minted a row")
                .to_string(),
            other => panic!("first upload of {name} did not answer Images: {other:?}"),
        };
        let key: String = db
            .read_main(|c| {
                let k: String = c.query_row(
                    "SELECT storageKey FROM files WHERE id = ?1",
                    [&first_id],
                    |r| r.get(0),
                )?;
                Ok(k)
            })
            .expect("read the minted row's storageKey");
        let blob_id = key
            .rsplit(':')
            .next()
            .expect("a mount-blob storage key")
            .to_string();
        rt.block_on(db.write(move |ws| {
            let mount = ws
                .mount_index()
                .ok_or_else(|| quilltap_core::db::DbError::Internal("no mount index".to_string()))?
                .connection();
            let n = mount.execute("DELETE FROM doc_mount_blobs WHERE id = ?1", [&blob_id])?;
            assert_eq!(n, 1, "the planted orphan must delete exactly its blob row");
            Ok(())
        }))
        .expect("plant the orphan");
        let resp = rt.block_on(quilltap_core::api::images::image_upload(
            &db,
            ingest_deps(),
            USER_A,
            "mark.svg",
            "image/svg+xml",
            SVG_BYTES.to_vec(),
            None,
        ));
        let tables = dump_tables(&db);
        if let Err(e) = within_tree_sha_agrees(&tables) {
            failed.push(format!("{name}: {e}"));
        }
        let got = from_response(resp, 201, Some(tables));
        compare(name, &got, &oracle, &mut failed);
    }

    // ── POST: import from URL (JSON) ────────────────────────────────────────
    // The URL and tags cross as RAW values: the handler Zod-validates them, so
    // the refusal arms are DRIVEN here rather than owned by the web edge.
    let import_cases: Vec<(&str, Value, CannedFetch, Option<Value>)> = vec![
        (
            "import_ok",
            json!("https://example.invalid/pics/photo.png"),
            CannedFetch::ok("image/png", png_1x1()),
            None,
        ),
        (
            "import_no_dot_filename",
            json!("https://example.invalid/pics/portrait"),
            CannedFetch::ok("image/png", png_1x1()),
            None,
        ),
        // See the oracle case's comment: the PNG arm above cannot discriminate
        // the extension rule (the transcode renames to .webp either way). An
        // SVG passes through, so `portrait.svg+xml` — v4's own
        // `split('/')[1]` quirk — is what actually gets stored.
        (
            "import_no_dot_svg",
            json!("https://example.invalid/pics/portrait"),
            CannedFetch::ok("image/svg+xml", SVG_BYTES.to_vec()),
            None,
        ),
        (
            "import_not_ok",
            json!("https://example.invalid/missing.png"),
            CannedFetch {
                status: 404,
                status_text: "Not Found".into(),
                content_type: "image/png".into(),
                bytes: vec![],
            },
            None,
        ),
        (
            "import_wrong_content_type",
            json!("https://example.invalid/page.html"),
            CannedFetch::ok("text/html", b"<html>".to_vec()),
            None,
        ),
        (
            "import_oversize",
            json!("https://example.invalid/huge.png"),
            CannedFetch::ok("image/png", vec![0u8; 10 * 1024 * 1024 + 1]),
            None,
        ),
        // The Zod refusals — the arms the handler owns.
        (
            "import_tags_raw_tagid",
            json!("https://example.invalid/pics/photo.png"),
            CannedFetch::ok("image/png", png_1x1()),
            Some(json!([{"tagType": "THEME", "tagId": 5}])),
        ),
        (
            "import_bad_url",
            json!("not-a-url"),
            CannedFetch::ok("image/png", png_1x1()),
            None,
        ),
        // P4.76 item (b): MEASURED — Zod 4's bare `z.url()` is exactly
        // "`new URL(value.trim())` does not throw" (no `://` guard without the
        // `httpProtocol` constraint), so v4 ACCEPTS an authority-less scheme and
        // goes on to fetch it. These two rows are the pin for both halves: the
        // acceptance, and the filename `importImageFromUrl` derives from an
        // OPAQUE pathname (no leading slash) — `someone@example.invalid` keeps
        // its dot-bearing last segment, `png;base64,iVBORw0KGgo=` has none and
        // takes the mime subtype. v5's `zod_url_ok` refused both until this unit.
        (
            "import_mailto_scheme",
            json!("mailto:someone@example.invalid"),
            CannedFetch::ok("image/png", png_1x1()),
            None,
        ),
        (
            "import_data_uri",
            json!("data:image/png;base64,iVBORw0KGgo="),
            CannedFetch::ok("image/png", png_1x1()),
            None,
        ),
        // v4 echoes the PARSED tags (`route.ts:445`): unknown keys stripped,
        // each object rebuilt `tagType` then `tagId`. Sent reordered and with
        // an extra key so the receipt's key order is the comparand (the §3
        // review of the follow-ups round 2 — the corpus had been blind).
        (
            "import_tags_extra_keys",
            json!("https://example.invalid/pics/photo.png"),
            CannedFetch::ok("image/png", png_1x1()),
            Some(json!([{"tagId": THEME_TAG, "tagType": "THEME", "extra": 1}])),
        ),
    ];
    for (name, url, canned, tags) in import_cases {
        driven.push(name.to_string());
        let db = fresh_db(&spec, name);
        let fetch = quilltap_core::api::images::ErasedImageImportFetch::new(canned);
        let resp = rt.block_on(quilltap_core::api::images::image_import_from_url(
            &db,
            ingest_deps(),
            &fetch,
            USER_A,
            Some(&url),
            tags.as_ref(),
        ));
        let tables = dump_tables(&db);
        if let Err(e) = within_tree_sha_agrees(&tables) {
            failed.push(format!("{name}: {e}"));
        }
        let got = from_response(resp, 201, Some(tables));
        compare(name, &got, &oracle, &mut failed);
    }

    // ── DELETE ──────────────────────────────────────────────────────────────
    // The storage backend is deliberately the not-configured one: every
    // fixture row is mount-blob keyed, so the existence probe reads
    // `doc_mount_blobs` and never the disk backend — the same dispatch v4's
    // `fileStorageManager` makes. F_INUSE's bytes are really there and
    // F_ORPHAN's key dangles, which is the whole discriminator between the
    // refusal and the cleanup.
    let backend: std::sync::Arc<dyn quilltap_core::services::file_storage::StorageBackend> =
        std::sync::Arc::new(quilltap_core::services::file_storage::NotConfiguredStorageBackend);

    let delete_cases: &[(&str, &str, bool)] = &[
        ("delete_missing", MISSING, false),
        ("delete_wrong_category", F_DOC, false),
        ("delete_in_use", F_INUSE, true),
        ("delete_orphaned_cleanup", F_ORPHAN, true),
        ("delete_ok", F_PLAIN, true),
        // NO storageKey and referenced: v4 skips the probe, so `fileExists`
        // stays FALSE and the orphan branch runs. Without this row a mutation
        // flipping the key-less arm to "bytes present" stayed GREEN.
        ("delete_nokey_in_use", F_NOKEY_INUSE, true),
        // P4.76 item (a): the orphan cleanup hits an ARCHIVED character, whose
        // repository guard throws — 500 `Failed to delete image`, with the
        // PEER's cleared `defaultImageId` already committed and its
        // `avatarOverrides` untouched (the second loop never runs). Both sides
        // keep the half-done cleanup because neither opens a transaction.
        ("delete_orphan_archived_peer", F_ORPHAN_ARCH, true),
    ];
    for (name, id, dump) in delete_cases {
        driven.push((*name).to_string());
        let db = fresh_db(&spec, name);
        let resp = rt.block_on(quilltap_core::api::images::image_delete(
            &db,
            backend.clone(),
            USER_A,
            id,
        ));
        // The dump is taken AFTER the write in every case, so a refusal proves
        // it wrote NOTHING rather than only that it answered 400.
        let tables = if *dump { Some(dump_tables(&db)) } else { None };
        let got = from_response(resp, 200, tables);
        compare(name, &got, &oracle, &mut failed);
    }

    assert!(
        failed.is_empty(),
        "images routes FAILED ({}):\n{}",
        failed.len(),
        failed.join("\n")
    );

    // Coverage by SHAPE, both directions — a case added on one side only
    // cannot pass silently (`harness-corpus-shape-constants-rot`).
    // The arms the WEB EDGE owns and this core-level family cannot reach: they
    // refuse before any verb exists to call. `No file provided` and
    // `Invalid tags JSON` are multipart-parse outcomes, and
    // `Invalid content type` is the route's final fall-through — none of them
    // has a `Request` to decode.
    //
    // ⚠ DEFERRED LOUDLY, not silently covered: proving them needs a served
    // instance, i.e. a test under `crates/quilltap-web/tests/`, whose directory
    // this round assigns to the sibling lane. Named here and in the lane record
    // so the gap is visible rather than implied by a green count.
    const EDGE_ONLY: &[&str] = &[
        "upload_no_file",
        "upload_tags_bad_json",
        "post_bad_content_type",
    ];

    let mut driven_sorted = driven.clone();
    driven_sorted.extend(EDGE_ONLY.iter().map(|s| (*s).to_string()));
    driven_sorted.sort();
    let mut oracle_names: Vec<String> = oracle.keys().cloned().collect();
    oracle_names.sort();
    assert_eq!(
        driven_sorted, oracle_names,
        "the driven case list and the oracle's disagree"
    );
}

/// v4's middleware answers an uncaught `ZodError` with `{error: 'Validation
/// error', details: [...issues]}`; that `details` array is the standing
/// project-wide deferral, so v5 carries the sentence alone and this drops it.
///
/// ⚠ Scoped to the ZodError sentence ON PURPOSE. `details` is not a ZodError
/// marker — v4's `badRequest(message, details)` uses the same key for the
/// images DELETE's `{message, code: 'IMAGE_IN_USE', associations}` bag, which
/// is a fixed literal v5 reproduces in full. Stripping by key presence would
/// have thrown that whole bag away, taking the `associations` counts with it —
/// the very numbers the `charactersUsingAsDefault` / `chatAvatarOverrides`
/// split exists to pin. So only the deferred Zod issues are dropped, and
/// everything else is compared byte for byte.
fn drop_zod_details(want_body: &Value) -> Value {
    let Some(o) = want_body.as_object() else {
        return want_body.clone();
    };
    if !o.contains_key("details") {
        return want_body.clone();
    }
    if o.get("error").and_then(Value::as_str) != Some("Validation error") {
        return want_body.clone();
    }
    serde_json::json!({ "error": "Validation error" })
}

/// The ingest receipt's minted values: the file id (and the `filepath` built
/// from it), the ROUTE-stamped timestamps, and — for a transcoded upload — the
/// byte length, which is codec-dependent for the same D19 reason the stored
/// sha is. Blanked on BOTH sides.
fn blank_receipt(v: &mut Value) {
    let Some(data) = v.get_mut("data").and_then(Value::as_object_mut) else {
        return;
    };
    for k in ["id", "filepath", "createdAt", "updatedAt"] {
        if data.contains_key(k) {
            data.insert(k.to_string(), Value::String(format!("<{k}>")));
        }
    }
    if data.get("mimeType").and_then(Value::as_str) == Some("image/webp")
        && data.contains_key("size")
    {
        data.insert("size".to_string(), Value::String("<size>".to_string()));
    }
}

fn compare(name: &str, got: &Got, oracle: &HashMap<String, Value>, failed: &mut Vec<String>) {
    let Some(want) = oracle.get(name) else {
        failed.push(format!("{name}: MISSING from the oracle"));
        return;
    };
    let want_status = want["status"].as_u64().unwrap_or(200) as u16;
    if got.status != want_status {
        failed.push(format!(
            "{name}: status {} != {want_status} (body {})",
            got.status, got.body
        ));
        return;
    }
    let mut want_body = canon(&drop_zod_details(&want["body"]));
    let mut got_body = canon(&got.body);
    if want_status == 201 {
        blank_receipt(&mut want_body);
        blank_receipt(&mut got_body);
    }
    if got_body != want_body {
        failed.push(format!(
            "{name}: body\n  want {want_body}\n  got  {got_body}"
        ));
        return;
    }
    // The key SEQUENCE, asserted explicitly (see `key_paths`).
    let (mut wk, mut gk) = (Vec::new(), Vec::new());
    key_paths(&want_body, "$".into(), &mut wk);
    key_paths(&got_body, "$".into(), &mut gk);
    if wk != gk {
        failed.push(format!("{name}: KEY ORDER\n  want {wk:?}\n  got  {gk:?}"));
        return;
    }
    if let Some(tables) = &got.tables {
        let mut want_tables = canon(&want["tables"]);
        let mut got_tables = canon(tables);
        // D19: the WebP bytes a case minted are codec-dependent, so their sha
        // and length are blanked on BOTH sides; `within_tree_sha_agrees` is
        // what keeps that from being a hole.
        blank_codec_dependent(&mut want_tables);
        blank_codec_dependent(&mut got_tables);
        if got_tables != want_tables {
            failed.push(format!(
                "{name}: tables\n  want {want_tables}\n  got  {got_tables}"
            ));
        }
    }
}
