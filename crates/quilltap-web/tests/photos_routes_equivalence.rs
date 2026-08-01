//! P4.9a USER-PHOTO-GALLERY differential: `api::photos` (over
//! `photos::user_gallery_service`) vs v4's REAL exported service functions +
//! the REAL `app/api/v1/photos` route handlers. Both sides read a FRESH copy of
//! the committed `photos-{main,mount}.db` family per case, so the mutating
//! save/delete cases cannot contaminate their neighbours.
//!
//! Every timestamp and every entity id in the fixture is PINNED, so the payload
//! diffs EXACT for the read cases — the `check` normalizes key ORDER only (so a
//! mismatch reports values, not ordering), and [`check_key_order`] separately
//! pins the raw key sequence of the richest payload (`list_default`) against
//! v4's `JSON.stringify` bytes. That is what proves the entry DTO's field order
//! AND the omit-vs-present split on `relevanceScore` (absent in the no-query
//! branch, present in the semantic one).
//!
//! The ONE case that mints an id is `save_ok` (a fresh `doc_mount_file_links`
//! row); its `linkId` is normalized on both sides — everything else about the
//! receipt, including the resolved `relativePath` and the recomputed `sha256`,
//! diffs exact.
//!
//! ## The two injected seams, and why they are honest
//!
//! - The QUERY EMBEDDING is canned identically on both sides
//!   (`photos-web.json#cannedEmbeddings`, keyed by the query text — the oracle
//!   stubs `generateEmbeddingForUser` to the same table). Everything BELOW the
//!   model boundary — `searchDocumentChunks`, cosine similarity, the
//!   literal-phrase boost, the peak-gate/trail-band filter — runs for real on
//!   both sides over the fixture's pinned chunk embeddings.
//! - The IMAGE BYTES come from the oracle's own `*_bytes` cases, which report
//!   what v4's unmocked `fileStorageManager.downloadFile` actually read at case
//!   time. Replaying those exact bytes is what makes the save leg's recomputed
//!   sha256 — and therefore the duplicate guard — comparable at all.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-photos.ndjson npx jest -- photos-routes
//! Run:
//!   QT_ORACLE_PHOTOS=/tmp/oracle-photos.ndjson \
//!     cargo test -p quilltap-web --test photos_routes_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use quilltap_core::api::photos::{
    image_info_get, photo_gallery_entry_get, photo_gallery_entry_remove, photo_gallery_list,
    photo_gallery_save,
};
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::files::FileEntry;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::embedding::{
    EmbeddingError, EmbeddingPriority, EmbeddingProvider, EmbeddingResult,
};
use quilltap_core::photos::save_image_to_album::{FileBytesStore, IngestImageRequest};
use serde::Deserialize;
use serde_json::Value;

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    user_id: String,
    canned_embeddings: HashMap<String, Vec<f32>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    link_ids: HashMap<String, Vec<String>>,
    notes_link_id: String,
    save_ok_file_id: String,
    save_dupe_file_id: String,
    save_not_image_file_id: String,
    save_not_owned_file_id: String,
    missing_file_id: String,
    image_info_linked_id: String,
    image_info_doc_links_id: String,
    image_info_plain_id: String,
    image_info_avatar_id: String,
    image_info_no_sha_id: String,
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}
fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/photos-web.json")
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

/// A fresh two-partition `Db` over a private copy of the committed fixture.
fn fresh_db(tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-photos-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("photos-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("photos-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        TEST_PEPPER,
    )
    .expect("open db")
}

/// The canned model boundary — the same table the oracle's
/// `generateEmbeddingForUser` stub reads, keyed by query TEXT. An unregistered
/// query is an ERROR, not a zero vector: a silent default would let a case that
/// forgot its vector pass by ranking everything at 0.
struct CannedEmbedding {
    table: HashMap<String, Vec<f32>>,
}
impl EmbeddingProvider for CannedEmbedding {
    async fn generate_embedding_for_user(
        &self,
        text: &str,
        _user_id: &str,
        _profile_id: Option<&str>,
        _priority: EmbeddingPriority,
    ) -> Result<EmbeddingResult, EmbeddingError> {
        match self.table.get(text) {
            Some(v) => Ok(EmbeddingResult {
                embedding: v.clone(),
                model: "canned".to_string(),
                dimensions: v.len(),
                provider: "canned".to_string(),
            }),
            None => Err(EmbeddingError::new(format!(
                "no canned embedding registered for input: {text}"
            ))),
        }
    }
}

/// The canned bytes seam — replays exactly what the oracle reported v4's real
/// storage manager read for each source file id.
struct CannedBytes {
    table: HashMap<String, Vec<u8>>,
}
impl FileBytesStore for CannedBytes {
    fn read_image_buffer(&self, entry: &FileEntry) -> Result<Option<Vec<u8>>, String> {
        Ok(self.table.get(&entry.id).cloned())
    }
    fn ingest_image_buffer(&self, _req: &IngestImageRequest) -> Result<FileEntry, String> {
        Err("the photos differential never re-ingests".to_string())
    }
}

// ── JSON canonicalization ──────────────────────────────────────────────────
fn sorted(v: &Value) -> Value {
    match v {
        Value::Object(o) => {
            let mut m = serde_json::Map::new();
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            for k in keys {
                m.insert(k.clone(), sorted(&o[k]));
            }
            Value::Object(m)
        }
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        other => other.clone(),
    }
}
fn norm(v: &Value) -> String {
    serde_json::to_string_pretty(&sorted(v)).unwrap()
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
        // The store-unavailable refusal (P4.23) — 503 (context.ts:176-205).
        ErrorKind::Unavailable => 503,
        ErrorKind::Internal => 500,
    }
}

fn success_body(r: &Response) -> Option<Value> {
    match r {
        Response::PhotoGallery(v) => Some(v.clone()),
        _ => None,
    }
}

/// The save receipt's `linkId` is freshly minted on both sides; blank it so the
/// REST of the receipt (mount, resolved path, recomputed sha256, keptAt) still
/// diffs exact.
fn blank_minted(name: &str, v: &Value) -> Value {
    if name != "save_ok" {
        return v.clone();
    }
    let mut out = v.clone();
    if let Some(o) = out.as_object_mut() {
        if o.contains_key("linkId") {
            o.insert("linkId".into(), Value::String("<minted>".into()));
        }
    }
    out
}

fn check(oracle: &HashMap<String, Value>, name: &str, resp: &Response, failed: &mut Vec<String>) {
    let want = oracle
        .get(name)
        .unwrap_or_else(|| panic!("oracle has no case '{name}' — regenerate the NDJSON"));
    let want_status = want["status"].as_u64().unwrap() as u16;
    if (200..300).contains(&want_status) {
        match success_body(resp) {
            Some(body) => {
                let got = blank_minted(name, &body);
                let expect = blank_minted(name, &want["body"]);
                if norm(&got) != norm(&expect) {
                    eprintln!(
                        "[{name}] BODY MISMATCH:\n got {}\n want {}",
                        norm(&got),
                        norm(&expect)
                    );
                    failed.push(name.to_string());
                }
            }
            None => {
                eprintln!("[{name}] expected success {want_status}, got {resp:?}");
                failed.push(name.to_string());
            }
        }
        return;
    }
    // The error arms: v4's body is `{error: message}`.
    match resp {
        Response::Error(e) => {
            let got_status = status_of(e.kind);
            let message = &e.message;
            let want_message = want["body"]["error"].as_str().unwrap_or_default();
            if got_status != want_status || message != want_message {
                eprintln!(
                    "[{name}] ERROR MISMATCH:\n got  {got_status} {message}\n want {want_status} {want_message}"
                );
                failed.push(name.to_string());
            }
        }
        other => {
            eprintln!("[{name}] expected error {want_status}, got {other:?}");
            failed.push(name.to_string());
        }
    }
}

/// Pin the RAW key sequence of the richest read payload against v4's
/// `JSON.stringify` order — the sorted-key `check` above deliberately cannot see
/// ordering, and the entry DTO's field order is part of the wire contract.
fn check_key_order(oracle: &HashMap<String, Value>, resp: &Response, failed: &mut Vec<String>) {
    let Some(got) = success_body(resp) else {
        failed.push("list_default_key_order".to_string());
        return;
    };
    let want = &oracle["list_default"]["body"];
    let keys = |v: &Value| -> Vec<String> {
        v.as_object()
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default()
    };
    if keys(&got) != keys(want) {
        eprintln!(
            "[list_default_key_order] TOP-LEVEL: got {:?} want {:?}",
            keys(&got),
            keys(want)
        );
        failed.push("list_default_key_order".to_string());
        return;
    }
    let got_entries = got["entries"].as_array().cloned().unwrap_or_default();
    let want_entries = want["entries"].as_array().cloned().unwrap_or_default();
    for (i, (g, w)) in got_entries.iter().zip(want_entries.iter()).enumerate() {
        if keys(g) != keys(w) {
            eprintln!(
                "[list_default_key_order] entry {i}: got {:?} want {:?}",
                keys(g),
                keys(w)
            );
            failed.push("list_default_key_order".to_string());
            return;
        }
        // …and the linker rows inside the link summary.
        let gl = g["linkSummary"]["linkers"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let wl = w["linkSummary"]["linkers"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for (j, (a, b)) in gl.iter().zip(wl.iter()).enumerate() {
            if keys(a) != keys(b) {
                eprintln!(
                    "[list_default_key_order] entry {i} linker {j}: got {:?} want {:?}",
                    keys(a),
                    keys(b)
                );
                failed.push("list_default_key_order".to_string());
                return;
            }
        }
    }
}

/// Pin the RAW key sequence of the richest image-info payload (P4.9a2) — the
/// `data` object's field order is v4's literal order at `route.ts:101-123`
/// (with the nullable-optional keys present here), and the nested `tags[]` /
/// `characterGalleryLinks[]` / `_count` objects each carry their own order.
fn check_image_info_key_order(
    oracle: &HashMap<String, Value>,
    resp: &Response,
    failed: &mut Vec<String>,
) {
    const CASE: &str = "imageInfo_linked_key_order";
    let Some(got) = success_body(resp) else {
        failed.push(CASE.to_string());
        return;
    };
    let want = &oracle["imageInfo_linked"]["body"];
    let keys = |v: &Value| -> Vec<String> {
        v.as_object()
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default()
    };
    for (label, g, w) in [
        ("envelope", &got, want),
        ("data", &got["data"], &want["data"]),
        ("tags[0]", &got["data"]["tags"][0], &want["data"]["tags"][0]),
        (
            "characterGalleryLinks[0]",
            &got["data"]["characterGalleryLinks"][0],
            &want["data"]["characterGalleryLinks"][0],
        ),
        ("_count", &got["data"]["_count"], &want["data"]["_count"]),
    ] {
        if keys(g) != keys(w) {
            eprintln!("[{CASE}] {label}: got {:?} want {:?}", keys(g), keys(w));
            failed.push(CASE.to_string());
            return;
        }
    }
}

fn decode_bytes(oracle: &HashMap<String, Value>, case: &str) -> Vec<u8> {
    use base64::Engine;
    let b64 = oracle[case]["body"]["base64"].as_str().unwrap_or_default();
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .expect("oracle bytes decode")
}

#[tokio::test(flavor = "multi_thread")]
async fn photos_routes_equivalence() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_PHOTOS") else {
        return;
    };
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let meta: Meta = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("photos-main.db.meta.json")).unwrap(),
    )
    .unwrap();

    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path).unwrap().lines() {
        if line.trim().is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }

    let provider = CannedEmbedding {
        table: spec.canned_embeddings.clone(),
    };
    let bytes: Arc<dyn FileBytesStore> = Arc::new(CannedBytes {
        table: HashMap::from([
            (
                meta.save_ok_file_id.clone(),
                decode_bytes(&oracle, "save_ok_bytes"),
            ),
            (
                meta.save_dupe_file_id.clone(),
                decode_bytes(&oracle, "save_dupe_bytes"),
            ),
        ]),
    });
    let user = spec.user_id.as_str();
    let mut failed: Vec<String> = Vec::new();

    // ── LIST ──────────────────────────────────────────────────────────────
    async fn list(
        db: &Db,
        provider: &CannedEmbedding,
        user: &str,
        q: Option<&str>,
        tag: Option<Vec<String>>,
        limit: Option<f64>,
        offset: Option<f64>,
    ) -> Response {
        photo_gallery_list(
            db,
            Some(provider),
            user,
            q.map(|s| s.to_string()),
            tag,
            limit,
            offset,
        )
        .await
    }

    let db = fresh_db("list");
    let default = list(&db, &provider, user, None, None, None, None).await;
    check(&oracle, "list_default", &default, &mut failed);
    check_key_order(&oracle, &default, &mut failed);

    for (name, q, tag, limit, offset) in [
        ("list_page_first", None, None, Some(2.0), Some(0.0)),
        ("list_page_second", None, None, Some(2.0), Some(2.0)),
        ("list_offset_past_end", None, None, None, Some(99.0)),
        ("list_limit_nan", None, None, Some(f64::NAN), None),
        ("list_limit_fraction", None, None, Some(1.5), None),
        ("list_limit_over_max", None, None, Some(500.0), None),
        ("list_offset_negative", None, None, None, Some(-1.0)),
        (
            "list_tag_single",
            None,
            Some(vec!["covenant".to_string()]),
            None,
            None,
        ),
        (
            "list_tag_case_insensitive",
            None,
            Some(vec!["COVENANT".to_string()]),
            None,
            None,
        ),
        (
            "list_tag_multi",
            None,
            Some(vec!["sky".to_string(), "lantern".to_string()]),
            None,
            None,
        ),
        (
            "list_tag_miss",
            None,
            Some(vec!["nonesuch".to_string()]),
            None,
            None,
        ),
        ("list_query_dawn", Some("dawn flight"), None, None, None),
        ("list_query_covenant", Some("covenant"), None, None, None),
        ("list_query_noise", Some("asdfasdf"), None, None, None),
        ("list_query_blank", Some("  "), None, None, None),
    ] {
        let resp = list(&db, &provider, user, q, tag, limit, offset).await;
        check(&oracle, name, &resp, &mut failed);
    }

    // ── ENTRY GET ─────────────────────────────────────────────────────────
    for (name, id) in [
        ("entry_primary", meta.link_ids["A"][1].clone()),
        ("entry_non_primary", meta.link_ids["A"][2].clone()),
        ("entry_disabled_mount", meta.link_ids["E"][0].clone()),
        ("entry_not_a_photo", meta.notes_link_id.clone()),
        (
            "entry_unknown",
            "00000000-0000-4000-8000-000000000bad".to_string(),
        ),
    ] {
        let resp = photo_gallery_entry_get(&db, id);
        check(&oracle, name, &resp, &mut failed);
    }

    // ── IMAGE INFO (P4.9a2 — read-only, shares the list copy) ─────────────
    let linked = image_info_get(&db, user, &meta.image_info_linked_id);
    check(&oracle, "imageInfo_linked", &linked, &mut failed);
    check_image_info_key_order(&oracle, &linked, &mut failed);
    for (name, id) in [
        ("imageInfo_doc_links_only", &meta.image_info_doc_links_id),
        ("imageInfo_plain_upload", &meta.image_info_plain_id),
        ("imageInfo_avatar_system", &meta.image_info_avatar_id),
        ("imageInfo_no_sha", &meta.image_info_no_sha_id),
        ("imageInfo_not_owned", &meta.save_not_owned_file_id),
        ("imageInfo_not_image", &meta.save_not_image_file_id),
        ("imageInfo_unknown", &meta.missing_file_id),
    ] {
        let resp = image_info_get(&db, user, id);
        check(&oracle, name, &resp, &mut failed);
    }
    drop(db);

    // ── SAVE (each on its OWN copy — the happy path mutates) ──────────────
    for (name, file_id, caption, tags) in [
        (
            "save_ok",
            Some(Some(Value::String(meta.save_ok_file_id.clone()))),
            Some("A fresh arrival".to_string()),
            Some(vec!["new".to_string()]),
        ),
        (
            "save_duplicate",
            Some(Some(Value::String(meta.save_dupe_file_id.clone()))),
            None,
            None,
        ),
        (
            "save_not_an_image",
            Some(Some(Value::String(meta.save_not_image_file_id.clone()))),
            None,
            None,
        ),
        (
            "save_not_owned",
            Some(Some(Value::String(meta.save_not_owned_file_id.clone()))),
            None,
            None,
        ),
        (
            "save_missing_file",
            Some(Some(Value::String(meta.missing_file_id.clone()))),
            None,
            None,
        ),
        // The ABSENT key (v4 Zod: "received undefined") vs an explicit null.
        ("save_absent_field", None, None, None),
        (
            "save_empty_field",
            Some(Some(Value::String(String::new()))),
            None,
            None,
        ),
    ] {
        let db = fresh_db(name);
        // The oracle's own receipt carries the keptAt v4 minted; replay it so the
        // markdown (and therefore the extractedText sha) is byte-identical.
        let kept_at = oracle[name]["body"]["keptAt"]
            .as_str()
            .unwrap_or("2026-02-01T12:00:00.000Z")
            .to_string();
        let resp = photo_gallery_save(
            &db,
            bytes.clone(),
            user,
            file_id,
            caption,
            tags,
            None,
            &kept_at,
        )
        .await;
        check(&oracle, name, &resp, &mut failed);
    }

    // ── DELETE (each on its OWN copy) ─────────────────────────────────────
    for (name, id) in [
        ("delete_link_only", meta.link_ids["A"][0].clone()),
        ("delete_last_link", meta.link_ids["D"][0].clone()),
        ("delete_not_a_photo", meta.notes_link_id.clone()),
        (
            "delete_unknown",
            "00000000-0000-4000-8000-000000000bad".to_string(),
        ),
    ] {
        let db = fresh_db(name);
        let resp = photo_gallery_entry_remove(&db, id).await;
        check(&oracle, name, &resp, &mut failed);
    }

    assert!(
        failed.is_empty(),
        "photos routes differential failures: {failed:?}"
    );
    eprintln!("photos_routes_equivalence: 40 checks green (+ the two key-order claims)");
}
