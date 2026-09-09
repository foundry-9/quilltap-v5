//! P4.D174 CHAT-GALLERY differential: `photos::chat_gallery` +
//! `api::chat_media::{chat_gallery, chat_save_gallery_image, chat_files_list,
//! message_save_image}` vs v4's REAL `lib/photos/chat-gallery.ts` and its two
//! route actions (`86d59660c`). Both sides read a FRESH copy of the committed
//! `chat-gallery-{main,mount}.db` fixture per case.
//!
//! The `gallery_key_order` case is the comparand every other family is blind
//! to: v4's per-entry key order is its JS CONSTRUCTION order — different per
//! pass, and `EntryCollector.noteMessage` APPENDS `messageId` to an entry a
//! previous pass already built, landing it after `deletable`/`linkSummary`
//! rather than in a declaration slot. `norm()` sorts keys, so the raw sequence
//! is pinned separately.
//!
//! Save-image reads a `files` id through a canned [`FileBytesStore`] returning
//! the bytes the oracle recorded (jest.setup's storage-manager stub); a
//! `doc_mount_file_links` id resolves its bytes from the fixture's own blob, on
//! both sides.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-chat-gallery.ndjson npx jest -- "cases/chat-gallery\.test\.ts$"
//! Run:
//!   QT_ORACLE_CHAT_GALLERY=/tmp/oracle-chat-gallery.ndjson \
//!     cargo test -p quilltap-harness --test chat_gallery_equivalence

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use quilltap_core::api::chat_media;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::files::FileEntry;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::photos::save_image_to_album::{
    FileBytesStore, IngestImageRequest, NoSideEffects, SaveImageSideEffects,
};
use serde::Deserialize;
use serde_json::{json, Value};

const USER: &str = "f0000000-0000-4000-8000-00000000000f";
const CHAT: &str = "c1000000-0000-4000-8000-000000000001";
const PORTRAIT_CHAT: &str = "c1000000-0000-4000-8000-000000000002";
const NO_CHAT: &str = "c9000000-0000-4000-8000-0000000000ff";
const M_GEN: &str = "d1000000-0000-4000-8000-000000000001";
const F_GEN: &str = "f1000000-0000-4000-8000-000000000007";
const F_UPLOAD: &str = "f1000000-0000-4000-8000-000000000008";
const F_MISSING: &str = "99999999-9999-4999-8999-999999999999";
const NOW_MS: i64 = 1_777_939_200_000; // 2026-05-05T00:00:00.000Z

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    alda_vault: String,
    cora_vault: String,
    general_mp: String,
    kept_link_id: String,
    twin_link_id: String,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/chat-gallery-web.json")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
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

/// A canned bytes store mirroring the oracle's jest.setup storage stub: every
/// `files` id reads the same recorded bytes, and ingest is refused.
struct CannedBytes {
    bytes: Vec<u8>,
}
impl FileBytesStore for CannedBytes {
    fn read_image_buffer(&self, _entry: &FileEntry) -> Result<Option<Vec<u8>>, String> {
        Ok(Some(self.bytes.clone()))
    }
    fn ingest_image_buffer(&self, _req: &IngestImageRequest) -> Result<FileEntry, String> {
        Err("ingest not available in the differential".to_string())
    }
}

/// Minimal standard-alphabet base64 decode (the oracle emits the image bytes b64).
fn base64_decode(s: &str) -> Vec<u8> {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut lut = [255u8; 256];
    for (i, &c) in T.iter().enumerate() {
        lut[c as usize] = i as u8;
    }
    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0;
    for &c in s.as_bytes() {
        if c == b'=' || c == b'\n' || c == b'\r' {
            continue;
        }
        let v = lut[c as usize];
        if v == 255 {
            continue;
        }
        buf = (buf << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    out
}

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
fn sorted(v: &Value) -> Value {
    match v {
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let mut m = serde_json::Map::new();
            for k in keys {
                m.insert(k.clone(), sorted(&o[k]));
            }
            Value::Object(m)
        }
        _ => v.clone(),
    }
}
/// Blank the keys the SAVE path mints (the album link + its content-row file id).
fn blank_minted(v: &mut Value) {
    if let Value::Object(o) = v {
        for k in ["linkId", "fileId"] {
            if o.contains_key(k) {
                o.insert(k.to_string(), Value::String(format!("<{k}>")));
            }
        }
        o.iter_mut().for_each(|(_, x)| blank_minted(x));
    } else if let Value::Array(a) = v {
        a.iter_mut().for_each(blank_minted);
    }
}
fn norm(v: &Value) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    serde_json::to_string_pretty(&sorted(&v)).unwrap()
}
/// Blank every ISO-8601 timestamp in a string. Used ONLY on the ALREADY_SAVED
/// arm, whose sentence quotes the EXISTING link row's `createdAt` — stamped by
/// `link_blob_content`'s wall clock on both sides (v4's is frozen inside the
/// oracle; a Rust `Db` write has no clock to freeze). The rest of the sentence —
/// the album name and the existing relative path, both derived from the injected
/// `kept_at` — stays compared.
fn blank_timestamps(v: &mut Value) {
    match v {
        Value::String(s) => {
            let bytes = s.as_bytes();
            let mut out = String::with_capacity(s.len());
            let mut i = 0usize;
            while i < bytes.len() {
                if i + 24 <= bytes.len() && is_iso_timestamp(&bytes[i..i + 24]) {
                    out.push_str("<ts>");
                    i += 24;
                } else {
                    out.push(bytes[i] as char);
                    i += 1;
                }
            }
            if out != *s {
                *s = out;
            }
        }
        Value::Array(a) => a.iter_mut().for_each(blank_timestamps),
        Value::Object(o) => o.iter_mut().for_each(|(_, x)| blank_timestamps(x)),
        _ => {}
    }
}

/// `YYYY-MM-DDTHH:MM:SS.mmmZ` — exactly 24 bytes.
fn is_iso_timestamp(b: &[u8]) -> bool {
    let digits = [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18, 20, 21, 22];
    digits.iter().all(|&i| b[i].is_ascii_digit())
        && b[4] == b'-'
        && b[7] == b'-'
        && b[10] == b'T'
        && b[13] == b':'
        && b[16] == b':'
        && b[19] == b'.'
        && b[23] == b'Z'
}

fn norm_timestamps(v: &Value) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    blank_timestamps(&mut v);
    serde_json::to_string_pretty(&sorted(&v)).unwrap()
}

fn norm_minted(v: &Value) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    blank_minted(&mut v);
    serde_json::to_string_pretty(&sorted(&v)).unwrap()
}
fn first_diff(got: &str, want: &str) -> String {
    let g: Vec<&str> = got.lines().collect();
    let w: Vec<&str> = want.lines().collect();
    for i in 0..g.len().max(w.len()) {
        let gi = g.get(i).copied().unwrap_or("<none>");
        let wi = w.get(i).copied().unwrap_or("<none>");
        if gi != wi {
            let mut ctx = String::new();
            for j in i.saturating_sub(3)..i {
                ctx.push_str(&format!("   = {}\n", g.get(j).copied().unwrap_or("")));
            }
            ctx.push_str(&format!("  GOT : {gi}\n  WANT: {wi}\n"));
            return ctx;
        }
    }
    "(identical line-by-line)".to_string()
}

/// The web-edge `(status, body)` for a `Response`. The error arm renders
/// v4's body: `{error}` plus the `code` carrier and the `details` riders
/// spread beside it — the shape §C.3 pins for the 409.
fn status_body(r: &Response) -> (u16, Value) {
    match r {
        Response::ChatMedia(v) => (200, v.clone()),
        Response::Error(e) => {
            let status = match e.kind {
                ErrorKind::BadRequest => 400,
                ErrorKind::Unauthorized => 401,
                ErrorKind::Forbidden => 403,
                ErrorKind::NotFound => 404,
                ErrorKind::Conflict => 409,
                ErrorKind::Unprocessable => 422,
                ErrorKind::Locked => 423,
                ErrorKind::Unavailable => 503,
                ErrorKind::Internal => 500,
            };
            let mut body = serde_json::Map::new();
            body.insert("error".into(), json!(e.message));
            if let Some(code) = &e.code {
                body.insert("code".into(), json!(code));
            }
            if let Some(details) = &e.details {
                if let Some(obj) = details.as_object() {
                    for (k, v) in obj {
                        body.insert(k.clone(), v.clone());
                    }
                }
            }
            (status, Value::Object(body))
        }
        other => (500, serde_json::to_value(other).unwrap()),
    }
}

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-cg-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("chat-gallery-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("chat-gallery-mount.db"), &mount).unwrap();
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

/// The RAW per-entry key sequence, mirroring the oracle's `gallery_key_order`.
fn key_order_view(body: &Value) -> Value {
    let entries = body
        .get("entries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    json!({
        "entries": entries.iter().map(|e| {
            json!({
                "id": e.get("id").cloned().unwrap_or(Value::Null),
                "keys": e.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()).unwrap_or_default(),
            })
        }).collect::<Vec<_>>(),
        "countsKeys": body.get("counts").and_then(Value::as_object)
            .map(|o| o.keys().cloned().collect::<Vec<_>>()).unwrap_or_default(),
        "topKeys": body.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()).unwrap_or_default(),
    })
}

#[test]
fn chat_gallery_equivalence() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_CHAT_GALLERY") else {
        return;
    };
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let meta: Meta = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("chat-gallery-main.db.meta.json")).unwrap(),
    )
    .unwrap();

    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }
    assert!(
        oracle.len() >= 18,
        "stale oracle: {} cases (expected the family's 18+)",
        oracle.len()
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut failed: Vec<String> = Vec::new();
    let kept_at = quilltap_core::clock::iso_from_unix_ms(NOW_MS);

    let img_b64 = oracle["image_bytes"]["body"]["base64"].as_str().unwrap();
    let bytes: Arc<dyn FileBytesStore> = Arc::new(CannedBytes {
        bytes: base64_decode(img_b64),
    });
    let no_fx: Arc<dyn SaveImageSideEffects + Send + Sync> = Arc::new(NoSideEffects);

    /// How a case's body is normalized before comparison.
    #[derive(Clone, Copy, PartialEq)]
    enum Norm {
        /// Byte-for-byte (keys sorted, numbers canonicalized).
        Exact,
        /// …plus the save path's minted `linkId` / `fileId` blanked.
        Minted,
        /// …plus every ISO timestamp blanked (the ALREADY_SAVED sentence).
        Timestamps,
    }

    fn check(
        failed: &mut Vec<String>,
        oracle: &HashMap<String, Value>,
        name: &str,
        resp: &Response,
        mode: Norm,
    ) {
        let (status, body) = status_body(resp);
        let want = &oracle[name];
        let want_status = want["status"].as_u64().unwrap() as u16;
        if status != want_status {
            eprintln!("[{name}] STATUS {status} != {want_status}");
            failed.push(format!("{name}_status"));
        }
        let (got_s, want_s) = match mode {
            Norm::Exact => (norm(&body), norm(&want["body"])),
            Norm::Minted => (norm_minted(&body), norm_minted(&want["body"])),
            Norm::Timestamps => (norm_timestamps(&body), norm_timestamps(&want["body"])),
        };
        if got_s != want_s {
            eprintln!("[{name}] BODY MISMATCH:\n{}", first_diff(&got_s, &want_s));
            failed.push(name.to_string());
        } else {
            eprintln!("[{name}] body OK.");
        }
    }

    // --- The roll ---
    let gallery_body = {
        let db = fresh_db(&spec, "roll");
        let r = chat_media::chat_gallery(&db, CHAT);
        check(&mut failed, &oracle, "gallery", &r, Norm::Exact);
        status_body(&r).1
    };
    {
        let db = fresh_db(&spec, "portrait");
        check(
            &mut failed,
            &oracle,
            "gallery_portrait_only",
            &chat_media::chat_gallery(&db, PORTRAIT_CHAT),
            Norm::Exact,
        );
    }
    {
        let db = fresh_db(&spec, "nochat");
        check(
            &mut failed,
            &oracle,
            "gallery_missing_chat",
            &chat_media::chat_gallery(&db, NO_CHAT),
            Norm::Exact,
        );
    }
    // The raw key sequence — the comparand `norm`'s key sort cannot see.
    {
        let got = key_order_view(&gallery_body);
        let want = &oracle["gallery_key_order"]["body"];
        if norm(&got) != norm(want) {
            eprintln!(
                "[gallery_key_order] MISMATCH:\n{}",
                first_diff(&norm(&got), &norm(want))
            );
            failed.push("gallery_key_order".to_string());
        } else {
            eprintln!("[gallery_key_order] OK.");
        }
        // ... and the ORDER of the key list itself, which `sorted()` would also
        // flatten if the lists were objects. They are arrays, so `norm` keeps
        // them — this assert is the belt on that braces.
        let got_raw = serde_json::to_string(&got).unwrap();
        let want_raw = serde_json::to_string(want).unwrap();
        if got_raw != want_raw {
            eprintln!(
                "[gallery_key_order] RAW SEQUENCE MISMATCH:\n  GOT : {got_raw}\n  WANT: {want_raw}"
            );
            failed.push("gallery_key_order_raw".to_string());
        }
    }

    // --- The listing that shares the walk ---
    {
        let db = fresh_db(&spec, "flist");
        check(
            &mut failed,
            &oracle,
            "files_list",
            &chat_media::chat_files_list(&db, CHAT),
            Norm::Exact,
        );
    }

    // --- Save-image, in v4's ladder order ---
    let save =
        |failed: &mut Vec<String>, name: &str, tag: &str, chat: &str, body: Value, mode: Norm| {
            let db = fresh_db(&spec, tag);
            let r = rt.block_on(chat_media::chat_save_gallery_image(
                &db,
                USER,
                chat,
                &body,
                bytes.clone(),
                no_fx.clone(),
                &kept_at,
            ));
            check(failed, &oracle, name, &r, mode);
        };
    save(
        &mut failed,
        "save_image_file",
        "savefile",
        CHAT,
        json!({ "fileId": F_GEN, "mountPointId": meta.general_mp, "caption": "The alley" }),
        Norm::Minted,
    );
    save(
        &mut failed,
        "save_image_vault",
        "savevault",
        CHAT,
        json!({ "fileId": F_UPLOAD, "mountPointId": meta.alda_vault, "tags": ["alley"] }),
        Norm::Minted,
    );
    save(
        &mut failed,
        "save_image_link",
        "savelink",
        CHAT,
        json!({ "fileId": meta.kept_link_id, "mountPointId": meta.general_mp }),
        Norm::Minted,
    );
    // The ALREADY_SAVED 409: the SAME save runs twice; the second is the answer.
    {
        let db = fresh_db(&spec, "savealready");
        let body = json!({ "fileId": F_GEN, "mountPointId": meta.general_mp });
        let _ = rt.block_on(chat_media::chat_save_gallery_image(
            &db,
            USER,
            CHAT,
            &body,
            bytes.clone(),
            no_fx.clone(),
            &kept_at,
        ));
        let r = rt.block_on(chat_media::chat_save_gallery_image(
            &db,
            USER,
            CHAT,
            &body,
            bytes.clone(),
            no_fx.clone(),
            &kept_at,
        ));
        check(
            &mut failed,
            &oracle,
            "save_image_already",
            &r,
            Norm::Timestamps,
        );
    }
    save(
        &mut failed,
        "save_image_not_member",
        "savenm",
        CHAT,
        json!({ "fileId": F_MISSING, "mountPointId": meta.general_mp }),
        Norm::Exact,
    );
    // The sha twin's LINK id: aliased in the collector, but no ENTRY carries it.
    save(
        &mut failed,
        "save_image_twin_alias",
        "savetwin",
        CHAT,
        json!({ "fileId": meta.twin_link_id, "mountPointId": meta.general_mp }),
        Norm::Exact,
    );
    save(
        &mut failed,
        "save_image_empty_body",
        "saveeb",
        CHAT,
        json!({}),
        Norm::Exact,
    );
    save(
        &mut failed,
        "save_image_blank_ids",
        "savebi",
        CHAT,
        json!({ "fileId": "", "mountPointId": "" }),
        Norm::Exact,
    );
    save(
        &mut failed,
        "save_image_wrong_types",
        "savewt",
        CHAT,
        json!({ "fileId": 42, "mountPointId": true, "tags": "nope" }),
        Norm::Exact,
    );
    save(
        &mut failed,
        "save_image_missing_chat",
        "savenc",
        NO_CHAT,
        json!({ "fileId": F_GEN, "mountPointId": meta.general_mp }),
        Norm::Exact,
    );

    // --- The MESSAGE-scoped leg on the newly-shared schema ---
    let msg_save = |failed: &mut Vec<String>, name: &str, tag: &str, body: Value| {
        let db = fresh_db(&spec, tag);
        let r = rt.block_on(chat_media::message_save_image(
            &db,
            USER,
            CHAT,
            M_GEN,
            &body,
            bytes.clone(),
            no_fx.clone(),
            &kept_at,
        ));
        check(failed, &oracle, name, &r, Norm::Exact);
    };
    msg_save(
        &mut failed,
        "message_save_non_uuid",
        "msgnu",
        json!({ "fileId": "not-a-uuid", "mountPointId": "also-not-a-uuid" }),
    );
    msg_save(&mut failed, "message_save_empty_body", "msgeb", json!({}));

    // The vault-attribution arm is only meaningful if the fixture HAS a
    // character-vault album in the roster — a silent renaming of the meta key
    // would otherwise make `save_image_vault` a second General save.
    assert_ne!(
        meta.alda_vault, meta.general_mp,
        "the vault-attribution arm needs a distinct vault mount"
    );
    assert!(
        !meta.cora_vault.is_empty(),
        "the fixture's persona vault id must be present"
    );

    assert!(failed.is_empty(), "chat-gallery differences: {failed:?}");
}
