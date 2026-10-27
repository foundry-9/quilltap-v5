//! P4.9f1 WARDROBE-ROUTES differential: `api::wardrobe` + `api::chat_outfits`
//! vs v4's REAL route handlers (the archetype tier, the transfers pair, the
//! seven-mode equip route, regenerate-avatar, preview-avatar), each case over a
//! FRESH copy of the committed `wardrobe-routes-{main,mount}.db` family.
//!
//! THE CASE LIST LIVES IN `wardrobe-routes.json#cases` — the SAME corpus the
//! oracle reads (the p4.6ay-unit-12 shared-corpus rule). This test asserts its
//! expected row count against the oracle NDJSON's actual row count, so a
//! half-regenerated oracle cannot silently partial-pass (the p4d8 lesson).
//!
//! Chained `${name}__outfit` rows re-read the equipped outfit on the SAME db
//! copy after a mutating case — that is what pins PERSISTENCE (the equip
//! writes, the delete route's equipped-reference cleanup), not just the
//! response bytes.
//!
//! Normalization is declared per-case in the corpus (`normalize` dot-paths —
//! minted ids, minted timestamps, the preview's fileId/url) and applied to
//! BOTH sides; everything else diffs exact after a key-sort. Two key-order
//! claims pin the raw key sequences of the richest read (`outfit_preset`) and
//! the equip echo (`eq_set_all`) against v4's `JSON.stringify` bytes.
//!
//! The ONE injected seam: the preview render. The oracle canned v4's
//! `createImageProvider` and captured the REAL `convertToWebP` output as
//! `pv_ok_bytes`; the [`CannedRenderer`] replays exactly that post-transcode
//! image, so the sha256, the vault write, the `files` row, and the response
//! run real on both sides.
//!
//! P4.D256 (v4 `cc80dc89d` + `3ee3b1342`): every case reads the pair as a
//! BOOTED instance would — `wardrobe_wear_stats` ensured through
//! `test_support::ensure_wear_ledger_on` (the oracle runs v4's migration
//! statements) — unless the case is `preRound` (the absent-table shape);
//! `plants` (raw SQL on the main copy, both sides) follow, before the `Db`
//! opens. `action: "wear-history"` on an item GET drives the dispatch verb
//! (`wardrobe_item_wear_history`) — the web edge's own arm is pinned in
//! `quilltap-web`'s `wardrobe_routes` unit tests.
//!
//! Generate the oracle (see the .ts header), then:
//!   QT_ORACLE_WARDROBE_ROUTES=/tmp/oracle-wardrobe-routes.ndjson \
//!     cargo test -p quilltap-web --test wardrobe_routes_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::characters::{character_wardrobe_get, character_wardrobe_list};
use quilltap_core::api::chat_outfits::{
    chat_equip, chat_outfit_get, chat_regenerate_avatar, chat_wardrobe_rollback,
};
use quilltap_core::api::types::{ErrorKind, Response, WardrobeContainerScope};
use quilltap_core::api::wardrobe::{
    wardrobe_create, wardrobe_delete, wardrobe_item_get, wardrobe_list, wardrobe_preview_avatar,
    wardrobe_transfer_apply, wardrobe_transfer_destinations, wardrobe_update,
    AvatarPreviewRenderer, ErasedAvatarPreview,
};
use quilltap_core::api::wardrobe_wear_history::{
    wardrobe_item_wear_history, wardrobe_item_wear_history_as,
};
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::Value;

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
/// The injected "now" for the mint-stamping arms (normalized where visible).
const NOW: &str = "2026-02-01T12:00:00.000Z";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaseEntry {
    name: String,
    kind: String,
    #[serde(default)]
    body: Option<Value>,
    #[serde(default)]
    item_id: Option<String>,
    #[serde(default)]
    chat_id: Option<String>,
    #[serde(default)]
    character_id: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    /// [P4.D120 / v4 `d25dacc1`] `?includeArchived=true` on a collection GET.
    #[serde(default)]
    include_archived: Option<bool>,
    /// [P4.D120] Paths whose value is CLASSIFIED rather than blanked: a
    /// non-empty string becomes `<stamped>`, `null` stays `null`. Blanking an
    /// `archivedAt` would erase the very distinction the archive arms prove.
    #[serde(default)]
    classify: Vec<String>,
    #[serde(default)]
    then_outfit: Option<String>,
    /// The chained group-tier read (v4 `8600c83f`): after this case, re-read
    /// `characters/{id}/wardrobe?scope=group` for the named character. It is
    /// what proves an item copied INTO a group store is actually reachable —
    /// the bug that commit fixed.
    #[serde(default)]
    then_group_wardrobe: Option<String>,
    #[serde(default)]
    group_normalize: Vec<String>,
    #[serde(default)]
    emit_bytes: bool,
    #[serde(default)]
    normalize: Vec<String>,
    /// [P4.D256] Leave `wardrobe_wear_stats` ABSENT (the pre-round shape).
    #[serde(default)]
    pre_round: bool,
    /// [P4.D256] Raw SQL on the main copy, after the ledger ensure.
    #[serde(default)]
    plants: Vec<Plant>,
    /// [P4.D256] `?action=<x>` on an item GET.
    #[serde(default)]
    action: Option<String>,
    /// P4.D283: the preview provider's scripted mode — `ok` (default) |
    /// `refuse` | `refuseFirst` | `throw` (the oracle's `createImageProvider`
    /// mock answers the same way).
    #[serde(default)]
    preview_provider: Option<String>,
    /// [P4.D256] Chained General wear-history reads on the SAME copy →
    /// `${name}__wear_${i}` rows (the ledger a delete left behind).
    #[serde(default)]
    then_wear_history: Vec<String>,
    /// [P4.D316] a timeline seeded on the copy (each step's slots written,
    /// then settled with the step's provenance).
    #[serde(default)]
    history_plants: Option<HistoryPlants>,
    /// [P4.D316] after the case, this chat's `chat_wardrobe_history` rows →
    /// `${name}__history`.
    #[serde(default)]
    then_history: Option<String>,
    /// [P4.D316] after the plants, delete this entry index (a timeline gap —
    /// R-11).
    #[serde(default)]
    history_drop_index: Option<i64>,
    /// [P4.D316] a body `req.json()` cannot parse (the malformed-JSON 500).
    #[serde(default)]
    raw_body: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryPlants {
    character_id: String,
    steps: Vec<HistoryStep>,
}

#[derive(Deserialize)]
struct HistoryStep {
    slots: Value,
    provenance: Value,
}

#[derive(Deserialize)]
struct Plant {
    sql: String,
    params: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    user_id: String,
    /// P4.D283: the scripted preview provider's canned answer (a WebP both
    /// sides pass through, so `pv_ok_bytes` pins the stored bytes).
    preview: PreviewSpec,
    cases: Vec<CaseEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviewSpec {
    provider_image_base64: String,
    provider_mime_type: String,
    revised_prompt: String,
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}
fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/wardrobe-routes.json")
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
/// The scratch dir rides out with the handle — bind it for the test's life.
/// [P4.D256] The ledger is ensured (a booted instance) unless `pre_round`;
/// the case's `plants` run after it, before the `Db` opens.
fn fresh_db(case: &CaseEntry) -> (Db, tempfile::TempDir) {
    let tag = case.name.as_str();
    let scratch = tempfile::Builder::new()
        .prefix(&format!("qt-wroutes-{tag}-"))
        .tempdir()
        .expect("tempdir");
    let main = scratch.path().join("main.db");
    let mount = scratch.path().join("mount.db");
    std::fs::copy(fixtures_dir().join("wardrobe-routes-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("wardrobe-routes-mount.db"), &mount).unwrap();
    {
        let w = quilltap_core::db::Writer::open_writable(&main, TEST_PEPPER).unwrap();
        let conn = w.connection();
        if !case.pre_round {
            quilltap_core::test_support::ensure_wear_ledger_on(conn);
        }
        // [P4.D316] P4.D315's one v5-side plant, on every per-case copy (R-S).
        quilltap_core::test_support::ensure_chat_wardrobe_history_on(conn);
        for p in &case.plants {
            let params: Vec<rusqlite::types::Value> = p
                .params
                .iter()
                .map(|v| match v {
                    Value::Null => rusqlite::types::Value::Null,
                    Value::String(s) => rusqlite::types::Value::Text(s.clone()),
                    Value::Number(n) => rusqlite::types::Value::Integer(n.as_i64().unwrap()),
                    other => panic!("{tag}: unsupported plant param {other}"),
                })
                .collect();
            conn.execute(&p.sql, rusqlite::params_from_iter(params))
                .unwrap_or_else(|e| panic!("{tag}: plant `{}`: {e}", p.sql));
        }
        // [P4.D316] the timeline, through the REAL repositories.
        if let Some(h) = &case.history_plants {
            use quilltap_core::db::chat_wardrobe_history::{
                ChatWardrobeHistoryRepository, WardrobeHistoryIntent, WardrobeHistoryProvenance,
                WardrobeHistorySource,
            };
            let chat_id = case
                .chat_id
                .as_deref()
                .expect("historyPlants needs a chatId");
            for step in &h.steps {
                let mut slots = serde_json::json!({ "top": [], "bottom": [], "footwear": [], "accessories": [], "hair": [] });
                for (k, v) in step.slots.as_object().unwrap() {
                    slots[k] = v.clone();
                }
                quilltap_core::db::chats_outfits::ChatOutfitsRepository::new(conn)
                    .set_equipped_outfit(chat_id, &h.character_id, &slots)
                    .expect("plant slots");
                let source: WardrobeHistorySource =
                    serde_json::from_value(step.provenance["source"].clone()).unwrap();
                let mut prov = WardrobeHistoryProvenance::new(source);
                if let Some(a) = step.provenance["actorCharacterId"].as_str() {
                    prov = prov.with_actor(a);
                }
                ChatWardrobeHistoryRepository::new(conn).settle(
                    chat_id,
                    &h.character_id,
                    &WardrobeHistoryIntent::Append { provenance: prov },
                );
            }
            if let Some(index) = case.history_drop_index {
                conn.execute(
                    r#"DELETE FROM chat_wardrobe_history WHERE "chatId" = ?1 AND "characterId" = ?2 AND "entryIndex" = ?3"#,
                    rusqlite::params![chat_id, h.character_id, index],
                )
                .expect("drop a history entry");
            }
        }
    }
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        TEST_PEPPER,
    )
    .expect("open db");
    (db, scratch)
}

/// The scripted preview PROVIDER (P4.D283, R-G): the core's seam is now ONE raw
/// `generateImage` per Concierge attempt plus the codec, so this answers the
/// fixture's canned WebP (a passthrough on both sides' transcode — the stored
/// bytes compare for real through `pv_ok_bytes`), or throws per the case's
/// `previewProvider` mode exactly as the oracle's `createImageProvider` mock
/// does: `refuse` OpenAI's safety sentence on every call, `refuseFirst` on the
/// first only (the understudy answers), `throw` a plain failure.
struct ScriptedPreviewProvider {
    webp_b64: String,
    mime_type: String,
    revised_prompt: Option<String>,
    mode: String,
    calls: std::sync::Mutex<usize>,
}
const PREVIEW_SAFETY: &str = "400 Your request was rejected as a result of our safety system.";
impl AvatarPreviewRenderer for ScriptedPreviewProvider {
    fn generate<'a>(
        &'a self,
        _provider: &'a str,
        _api_key: &'a str,
        _params: &'a quilltap_core::model::image::ImageGenParams,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<
                        quilltap_core::model::image::ImageGenResponse,
                        quilltap_core::model::image::ImageGenError,
                    >,
                > + Send
                + 'a,
        >,
    > {
        let n = {
            let mut c = self.calls.lock().unwrap();
            *c += 1;
            *c
        };
        let out = match self.mode.as_str() {
            "throw" => Err(quilltap_core::model::image::ImageGenError::new(
                "canned provider failure",
            )),
            "refuse" => Err(quilltap_core::model::image::ImageGenError::new(
                PREVIEW_SAFETY,
            )),
            "refuseFirst" if n == 1 => Err(quilltap_core::model::image::ImageGenError::new(
                PREVIEW_SAFETY,
            )),
            _ => Ok(quilltap_core::model::image::ImageGenResponse {
                images: vec![quilltap_core::model::image::GeneratedImageData {
                    data: Some(self.webp_b64.clone()),
                    url: None,
                    mime_type: Some(self.mime_type.clone()),
                    revised_prompt: self.revised_prompt.clone(),
                }],
            }),
        };
        Box::pin(async move { out })
    }

    fn codec(&self) -> std::sync::Arc<dyn quilltap_core::services::file_storage::PixelCodec> {
        std::sync::Arc::new(quilltap_host::HostImageCodec)
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
        // `Character` is the character-wardrobe route's envelope (P4.D71's
        // `?scope=group` arm rides the characters surface, not the archetype one).
        Response::Wardrobe(v)
        | Response::ChatOutfit(v)
        | Response::Character(v)
        | Response::WardrobeWearHistory(v) => Some(v.clone()),
        _ => None,
    }
}

/// Blank the corpus-declared dot-paths (minted ids/timestamps) on a body.
/// [P4.D120] `<stamped>` for a non-empty string; `null` and absent untouched.
fn classify_paths(v: &Value, paths: &[String]) -> Value {
    fn one(v: &mut Value, segments: &[&str]) {
        let Some((first, rest)) = segments.split_first() else {
            return;
        };
        if let Ok(idx) = first.parse::<usize>() {
            let Some(arr) = v.as_array_mut() else { return };
            let Some(next) = arr.get_mut(idx) else { return };
            if rest.is_empty() {
                if next.as_str().is_some_and(|s| !s.is_empty()) {
                    *next = Value::String("<stamped>".into());
                }
            } else {
                one(next, rest);
            }
            return;
        }
        let Some(obj) = v.as_object_mut() else { return };
        if rest.is_empty() {
            if obj
                .get(*first)
                .and_then(Value::as_str)
                .is_some_and(|s| !s.is_empty())
            {
                obj.insert((*first).to_string(), Value::String("<stamped>".into()));
            }
            return;
        }
        if let Some(next) = obj.get_mut(*first) {
            one(next, rest);
        }
    }
    let mut out = v.clone();
    for p in paths {
        let segs: Vec<&str> = p.split('.').collect();
        one(&mut out, &segs);
    }
    out
}

fn blank_paths(v: &Value, paths: &[String]) -> Value {
    fn blank_one(v: &mut Value, segments: &[&str]) {
        let Some((first, rest)) = segments.split_first() else {
            return;
        };
        // A numeric segment indexes an array (`wardrobeItems.1.id`); anything
        // else is an object key.
        if let Ok(idx) = first.parse::<usize>() {
            let Some(arr) = v.as_array_mut() else { return };
            let Some(next) = arr.get_mut(idx) else { return };
            if rest.is_empty() {
                *next = Value::String("<minted>".into());
            } else {
                blank_one(next, rest);
            }
            return;
        }
        let Some(obj) = v.as_object_mut() else { return };
        if rest.is_empty() {
            if obj.contains_key(*first) {
                obj.insert((*first).to_string(), Value::String("<minted>".into()));
            }
        } else if let Some(next) = obj.get_mut(*first) {
            blank_one(next, rest);
        }
    }
    let mut out = v.clone();
    for path in paths {
        let segments: Vec<&str> = path.split('.').collect();
        blank_one(&mut out, &segments);
    }
    out
}

fn check(
    oracle: &HashMap<String, Value>,
    name: &str,
    resp: &Response,
    normalize: &[String],
    classify: &[String],
    failed: &mut Vec<String>,
) {
    let want = oracle
        .get(name)
        .unwrap_or_else(|| panic!("oracle has no case '{name}' — regenerate the NDJSON"));
    let want_status = want["status"].as_u64().unwrap() as u16;
    if (200..300).contains(&want_status) {
        match success_body(resp) {
            Some(body) => {
                let got = classify_paths(&blank_paths(&body, normalize), classify);
                let expect = classify_paths(&blank_paths(&want["body"], normalize), classify);
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
    // The error arms: v4's body is `{error: message}` (+ a `details` array on
    // the middleware validationError — the standing project-wide deferral, so
    // only `error` is compared).
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

/// P4.D283: the preview case through `preview_avatar_status_and_body` — the
/// web edge's mapping — compared whole (`error` + `details` on a failure; the
/// 200 body with `rerouted` + `trail`, `fileId` / `url` blanked).
fn check_preview(
    oracle: &HashMap<String, Value>,
    name: &str,
    resp: Response,
    normalize: &[String],
    failed: &mut Vec<String>,
) {
    let want = oracle
        .get(name)
        .unwrap_or_else(|| panic!("oracle has no case '{name}' — regenerate the NDJSON"));
    let want_status = want["status"].as_u64().unwrap() as u16;
    let Some((status, body)) = quilltap_web::wardrobe_routes::preview_avatar_status_and_body(resp)
    else {
        eprintln!("[{name}] preview answered a variant the edge never maps");
        failed.push(name.to_string());
        return;
    };
    let got = norm(&blank_paths(&body, normalize));
    let expect = norm(&blank_paths(&want["body"], normalize));
    if status.as_u16() != want_status || got != expect {
        eprintln!(
            "[{name}] PREVIEW MISMATCH:\n got  {} {got}\n want {want_status} {expect}",
            status.as_u16()
        );
        failed.push(name.to_string());
    }
}

/// P4.D283: `pv_ok_bytes` — the oracle's captured post-transcode bytes vs the
/// `files` row v5 stored (sha256 / size / mime): the model+codec boundary is a
/// passthrough WebP on both sides now, so the STORED bytes are comparable.
fn check_preview_bytes(
    oracle: &HashMap<String, Value>,
    name: &str,
    db: &Db,
    resp: &Response,
    failed: &mut Vec<String>,
) {
    let key = format!("{name}_bytes");
    let Some(bytes_row) = oracle.get(&key) else {
        eprintln!("[{key}] oracle has no bytes row — regenerate the NDJSON");
        failed.push(key);
        return;
    };
    let Some(body) = success_body(resp) else {
        eprintln!("[{key}] the preview did not succeed: {resp:?}");
        failed.push(key);
        return;
    };
    let file_id = body["fileId"].as_str().unwrap_or_default().to_string();
    let want_bytes = decode_bytes(bytes_row);
    let want_sha = {
        use sha2::Digest;
        hex::encode(sha2::Sha256::digest(&want_bytes))
    };
    let want_mime = bytes_row["body"]["mimeType"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let got: Option<(String, i64, String)> = db
        .read_main(move |c| {
            c.query_row(
                "SELECT sha256, size, mimeType FROM files WHERE id = ?1",
                [&file_id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, f64>(1)? as i64,
                        r.get::<_, String>(2)?,
                    ))
                },
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other.into()),
            })
        })
        .unwrap();
    match got {
        Some((sha, size, mime))
            if sha == want_sha && size == want_bytes.len() as i64 && mime == want_mime => {}
        other => {
            eprintln!(
                "[{key}] STORED BYTES MISMATCH: got {other:?}, want sha {want_sha} size {} mime {want_mime}",
                want_bytes.len()
            );
            failed.push(key);
        }
    }
}

/// Pin a raw key sequence (the sorted-key `check` deliberately cannot see it).
fn check_key_order(
    oracle: &HashMap<String, Value>,
    name: &str,
    resp: &Response,
    failed: &mut Vec<String>,
) {
    let Some(got) = success_body(resp) else {
        failed.push(format!("{name}_key_order"));
        return;
    };
    let want = &oracle[name]["body"];
    // [P4.D256] Arrays are walked too, so a list's per-item key sequence
    // (`…, origin, wear` LAST — v4's object spreads) is claimed.
    fn key_walk(v: &Value, out: &mut Vec<String>, prefix: &str) {
        if let Some(o) = v.as_object() {
            for (k, inner) in o {
                out.push(format!("{prefix}{k}"));
                key_walk(inner, out, &format!("{prefix}{k}."));
            }
        } else if let Some(a) = v.as_array() {
            for (i, inner) in a.iter().enumerate() {
                key_walk(inner, out, &format!("{prefix}{i}."));
            }
        }
    }
    let mut got_keys = Vec::new();
    let mut want_keys = Vec::new();
    key_walk(&got, &mut got_keys, "");
    key_walk(want, &mut want_keys, "");
    if got_keys != want_keys {
        eprintln!("[{name}_key_order] got {got_keys:?}\n want {want_keys:?}");
        failed.push(format!("{name}_key_order"));
    }
}

fn decode_bytes(v: &Value) -> Vec<u8> {
    use base64::Engine;
    let b64 = v["body"]["base64"].as_str().unwrap_or_default();
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .expect("oracle bytes decode")
}

#[tokio::test(flavor = "multi_thread")]
async fn wardrobe_routes_equivalence() {
    // [P4.D256] This binary also runs `p4d256_route_lines_are_v4s`, which
    // captures the SAME callsites: install the process-global rig first so no
    // callsite's `Interest` is cached as "never" (test_support's module doc).
    quilltap_core::test_support::global_capture::install();
    let Some(oracle_path) = env_or_skip("QT_ORACLE_WARDROBE_ROUTES") else {
        return;
    };
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();

    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path).unwrap().lines() {
        if line.trim().is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }

    // The shared-corpus completeness claim: EVERY corpus case (+ its chained
    // `__outfit` row + its `_bytes` row) must exist in the oracle, and nothing
    // else — a half-regenerated NDJSON fails loudly here.
    let expected_rows = spec.cases.len()
        + spec
            .cases
            .iter()
            .filter(|c| c.then_outfit.is_some())
            .count()
        + spec
            .cases
            .iter()
            .filter(|c| c.then_group_wardrobe.is_some())
            .count()
        + spec.cases.iter().filter(|c| c.emit_bytes).count()
        + spec
            .cases
            .iter()
            .map(|c| c.then_wear_history.len())
            .sum::<usize>()
        + spec
            .cases
            .iter()
            .filter(|c| c.then_history.is_some())
            .count();
    assert_eq!(
        oracle.len(),
        expected_rows,
        "oracle row count {} != corpus expectation {expected_rows} — regenerate the NDJSON",
        oracle.len()
    );

    // P4.D283: the preview provider is scripted PER CASE (`previewProvider`).
    let preview_for = |case: &CaseEntry| {
        ErasedAvatarPreview::new(ScriptedPreviewProvider {
            webp_b64: spec.preview.provider_image_base64.clone(),
            mime_type: spec.preview.provider_mime_type.clone(),
            revised_prompt: Some(spec.preview.revised_prompt.clone()),
            mode: case
                .preview_provider
                .clone()
                .unwrap_or_else(|| "ok".to_string()),
            calls: std::sync::Mutex::new(0),
        })
    };

    let user = spec.user_id.as_str();
    let mut failed: Vec<String> = Vec::new();
    let mut checks = 0usize;

    for case in &spec.cases {
        let (db, _scratch) = fresh_db(case);
        let body = case.body.clone().unwrap_or(Value::Null);
        let resp = match case.kind.as_str() {
            "wardrobeList" => wardrobe_list(&db, case.include_archived.unwrap_or(false)).await,
            "wardrobeCreate" => wardrobe_create(&db, body).await,
            "wardrobeItemGet" => match case.action.as_deref() {
                None => wardrobe_item_get(&db, case.item_id.as_deref().unwrap()),
                Some("wear-history") => wardrobe_item_wear_history(
                    &db,
                    WardrobeContainerScope::General,
                    None,
                    case.item_id.as_deref().unwrap(),
                ),
                Some(other) => panic!("unknown action {other}"),
            },
            "characterWardrobeUpdate" => {
                quilltap_core::api::characters::character_wardrobe_update(
                    &db,
                    user,
                    case.character_id.as_deref().unwrap(),
                    case.item_id.as_deref().unwrap(),
                    body,
                )
                .await
            }
            "characterWardrobeDelete" => {
                quilltap_core::api::characters::character_wardrobe_delete(
                    &db,
                    user,
                    case.character_id.as_deref().unwrap(),
                    case.item_id.as_deref().unwrap(),
                )
                .await
            }
            "characterWardrobeItemGet" => match case.action.as_deref() {
                None => character_wardrobe_get(
                    &db,
                    user,
                    case.character_id.as_deref().unwrap(),
                    case.item_id.as_deref().unwrap(),
                ),
                Some("wear-history") => wardrobe_item_wear_history_as(
                    &db,
                    user,
                    WardrobeContainerScope::Character,
                    case.character_id.as_deref(),
                    case.item_id.as_deref().unwrap(),
                ),
                Some(other) => panic!("unknown action {other}"),
            },
            // P4.170 C.10: an unparsable body never reaches the core — the web
            // edge probes the item first (`wardrobe_routes::wardrobe_item_put`)
            // and answers its 404; the edge itself is compared against this
            // row in `chat_interrupt_rest_routes`. Here v4's row is held to the
            // probe's answer for the same missing id.
            "wardrobeUpdate" if case.raw_body.is_some() => {
                wardrobe_item_get(&db, case.item_id.as_deref().unwrap())
            }
            "wardrobeUpdate" => wardrobe_update(&db, case.item_id.as_deref().unwrap(), body).await,
            "wardrobeDelete" => wardrobe_delete(&db, case.item_id.as_deref().unwrap()).await,
            "transferDestinations" => wardrobe_transfer_destinations(&db, user),
            "transferApply" => wardrobe_transfer_apply(&db, user, body, NOW).await,
            "outfitGet" => chat_outfit_get(&db, case.chat_id.as_deref().unwrap()),
            "equip" => chat_equip(&db, user, case.chat_id.as_deref().unwrap(), body).await,
            // [P4.D316] the rollback POST. A malformed JSON body never reaches
            // the core: the web edge's `parse_body` answers v4's 500 (pinned in
            // `wardrobe_routes.rs`'s own unit test); here v4's row is held to
            // that same answer.
            "wardrobeRollback" if case.raw_body.is_some() => Response::error(
                ErrorKind::Internal,
                "Failed to roll back the outfit".to_string(),
            ),
            "wardrobeRollback" => {
                chat_wardrobe_rollback(&db, user, case.chat_id.as_deref().unwrap(), body).await
            }
            "regenerateAvatar" => {
                chat_regenerate_avatar(&db, user, case.chat_id.as_deref().unwrap(), body).await
            }
            "previewAvatar" => {
                wardrobe_preview_avatar(&db, &preview_for(case), user, body, NOW, None).await
            }
            "characterWardrobeList" => character_wardrobe_list(
                &db,
                user,
                case.character_id.as_deref().unwrap(),
                case.scope.as_deref(),
                case.include_archived.unwrap_or(false),
            ),
            other => panic!("unknown case kind: {other}"),
        };
        if case.kind == "previewAvatar" {
            // P4.D283 (R-G): the preview's status + WHOLE body through the web
            // edge's own mapping — v4's 422 / 502 with `details { trail,
            // refused }` are comparands here, not the deferred `validationError`
            // details the generic `check` leaves out.
            check_preview(
                &oracle,
                &case.name,
                resp.clone(),
                &case.normalize,
                &mut failed,
            );
            checks += 1;
            if case.emit_bytes {
                check_preview_bytes(&oracle, &case.name, &db, &resp, &mut failed);
                checks += 1;
            }
        } else {
            check(
                &oracle,
                &case.name,
                &resp,
                &case.normalize,
                &case.classify,
                &mut failed,
            );
            checks += 1;
        }

        // The two raw key-order claims (the richest read + the equip echo).
        // [P4.D256] + the tagged / worn reads and the wear-history payload.
        const KEY_ORDER_CASES: &[&str] = &[
            "outfit_preset",
            "eq_set_all",
            "list_with_ledger",
            "cw_no_scope_with_ledger",
            "cw_group_scope_with_ledger",
            "item_get_with_ledger",
            "cw_item_get",
            "wh_general",
            "wh_character",
        ];
        if KEY_ORDER_CASES.contains(&case.name.as_str()) {
            check_key_order(&oracle, &case.name, &resp, &mut failed);
            checks += 1;
        }

        for (i, wid) in case.then_wear_history.iter().enumerate() {
            let follow =
                wardrobe_item_wear_history(&db, WardrobeContainerScope::General, None, wid);
            check(
                &oracle,
                &format!("{}__wear_{i}", case.name),
                &follow,
                &[],
                &[],
                &mut failed,
            );
            checks += 1;
        }

        if let Some(character_id) = &case.then_group_wardrobe {
            let follow = character_wardrobe_list(&db, user, character_id, Some("group"), false);
            check(
                &oracle,
                &format!("{}__group", case.name),
                &follow,
                &case.group_normalize,
                &[],
                &mut failed,
            );
            checks += 1;
        }

        if let Some(chat_id) = &case.then_history {
            let name = format!("{}__history", case.name);
            let want = oracle
                .get(&name)
                .unwrap_or_else(|| panic!("oracle has no '{name}' — regenerate the NDJSON"));
            let cid = chat_id.clone();
            let got = db
                .read_main(move |c| {
                    let mut stmt = c.prepare(
                        r#"SELECT "chatId", "characterId", "entryIndex", "slots", "slotsHash", "source", "actorCharacterId", "actorToolName", "isCurrent" FROM chat_wardrobe_history WHERE "chatId" = ?1 ORDER BY "characterId", "entryIndex""#,
                    )?;
                    let rows = stmt
                        .query_map([&cid], |r| {
                            Ok(serde_json::json!({
                                "chatId": r.get::<_, String>(0)?,
                                "characterId": r.get::<_, String>(1)?,
                                "entryIndex": r.get::<_, i64>(2)?,
                                "slots": r.get::<_, String>(3)?,
                                "slotsHash": r.get::<_, String>(4)?,
                                "source": r.get::<_, String>(5)?,
                                "actorCharacterId": r.get::<_, Option<String>>(6)?,
                                "actorToolName": r.get::<_, Option<String>>(7)?,
                                "isCurrent": r.get::<_, i64>(8)?,
                            }))
                        })?
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok(Value::Array(rows))
                })
                .expect("read the history back");
            // Byte-compared (serde_json's Map `==` ignores key order).
            if serde_json::to_string(&got).unwrap() != serde_json::to_string(&want["body"]).unwrap()
            {
                eprintln!(
                    "[{name}] HISTORY MISMATCH:\n got  {got}\n want {}",
                    want["body"]
                );
                failed.push(name);
            }
            checks += 1;
        }

        if let Some(chat_id) = &case.then_outfit {
            let follow = chat_outfit_get(&db, chat_id);
            check(
                &oracle,
                &format!("{}__outfit", case.name),
                &follow,
                &[],
                &[],
                &mut failed,
            );
            checks += 1;
        }
        drop(db);
    }

    assert!(
        failed.is_empty(),
        "wardrobe routes differential failures: {failed:?}"
    );
    eprintln!(
        "wardrobe_routes_equivalence: {checks} checks green over {} corpus cases ({} oracle rows)",
        spec.cases.len(),
        expected_rows
    );
}

/// [P4.D256] Capture pins (§R.5) for the lines this lane's routes emit on the
/// caller thread, over the committed pair with the ledger ensured + planted:
/// R-D's group-tier line with v4's FIVE camelCase keys (`groupCount` NEW in
/// `cc80dc89d`), each tier's `Read … wear history` DEBUG, the single GETs'
/// silence on `Attached wear summaries` (no `wear` on a single GET), and the
/// 404-before-ledger order — an item outside the tier never reaches the ledger
/// (`Built wear history` silent, v4 `wear-ledger-routes.test.ts:197-206`).
#[test]
fn p4d256_route_lines_are_v4s() {
    quilltap_core::test_support::global_capture::install();
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let base = spec
        .cases
        .iter()
        .find(|c| c.name == "wh_general")
        .expect("the corpus carries wh_general (its ledger plants)");
    let (db, _scratch) = fresh_db(base);
    let raw: Value = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let id = |k: &str| raw["ids"][k].as_str().unwrap().to_string();
    let (aria, bram, group, project) = (id("aria"), id("bram"), id("group"), id("project"));
    let (g_coat, w_top, g_livery, p_scarf) = (id("gCoat"), id("wTop"), id("gLivery"), id("pScarf"));
    // The process-global rig (thread-armed): a thread-scoped `set_default`
    // loses callsites the sibling oracle test reached first (the gate's
    // `Interest`-cache race — red in the workspace run, green alone).
    let cap = |f: &dyn Fn() -> Response| quilltap_core::test_support::global_capture::capture(f);
    let has = |lines: &[String], needle: &str| lines.iter().any(|l| l.contains(needle));

    // R-D: the group-tier read's one line, v4's keys in v4's order.
    let (resp, lines) =
        cap(&|| character_wardrobe_list(&db, &spec.user_id, &aria, Some("group"), false));
    let items = success_body(&resp).unwrap()["wardrobeItems"]
        .as_array()
        .unwrap()
        .len();
    let line = lines
        .iter()
        .find(|l| l.contains("[Wardrobe v1] Group-tier wardrobe read"))
        .unwrap_or_else(|| panic!("no group-tier line: {lines:?}"));
    let tail = line
        .split("[Wardrobe v1] Group-tier wardrobe read")
        .nth(1)
        .unwrap();
    let keys: Vec<&str> = tail
        .split_whitespace()
        .map(|kv| kv.split('=').next().unwrap())
        .collect();
    assert_eq!(
        keys,
        [
            "characterId",
            "groupCount",
            "groupMountCount",
            "itemCount",
            "context"
        ],
        "{line}"
    );
    assert!(
        tail.contains(&format!("characterId={aria}"))
            && tail.contains(&format!("itemCount={items}"))
    );
    assert!(!lines
        .iter()
        .any(|l| l.contains("character_id=") || l.contains("group_mount_count")));
    assert!(has(&lines, "Attached wear summaries to wardrobe read"));

    // Each tier's wear-history DEBUG, after the payload — since `9ecebf986`
    // the ONE factory line `<tag> Read wardrobe item wear history` keyed by the
    // tier's `logIdKey` (`generalId: null` on General), `context` last.
    for (scope, container, item, needle) in [
        (WardrobeContainerScope::General, None, g_coat.as_str(),
         format!("[Wardrobe Archetypes v1] Read wardrobe item wear history generalId=null itemId={g_coat} wearCount=6 context=wardrobe")),
        (WardrobeContainerScope::Character, Some(aria.as_str()), w_top.as_str(),
         format!("[Wardrobe v1] Read wardrobe item wear history characterId={aria} itemId={w_top} wearCount=2 context=wardrobe")),
        (WardrobeContainerScope::Group, Some(group.as_str()), g_livery.as_str(),
         format!("[Groups v1] Read wardrobe item wear history groupId={group} itemId={g_livery} wearCount=1 context=wardrobe")),
        (WardrobeContainerScope::Project, Some(project.as_str()), p_scarf.as_str(),
         format!("[Projects v1] Read wardrobe item wear history projectId={project} itemId={p_scarf} wearCount=0 context=wardrobe")),
    ] {
        let (resp, lines) =
            cap(&|| wardrobe_item_wear_history_as(&db, &spec.user_id, scope, container, item));
        assert!(matches!(resp, Response::WardrobeWearHistory(_)), "{scope:?}: {resp:?}");
        let built = lines.iter().position(|l| l.contains("Built wear history"));
        let read = lines.iter().position(|l| l.ends_with(&needle));
        assert!(built.is_some() && read.is_some() && built < read, "{scope:?}: {lines:#?}");
        // R-H: the keystone's resolve line rides the same request — by tag.
        assert!(has(&lines, "[WardrobeLocation] Resolved wardrobe location"), "{lines:#?}");
        // The OLD per-tier titles are gone.
        assert!(
            !has(&lines, "Read archetype item wear history")
                && !has(&lines, "Read group wardrobe item wear history")
                && !has(&lines, "Read project wardrobe item wear history"),
            "{lines:#?}"
        );
    }

    // 404 BEFORE the ledger: never `Built wear history`, never a tier line.
    for (scope, container, item) in [
        (WardrobeContainerScope::General, None, w_top.as_str()),
        (
            WardrobeContainerScope::Character,
            Some(bram.as_str()),
            w_top.as_str(),
        ),
        (
            WardrobeContainerScope::Group,
            Some(group.as_str()),
            g_coat.as_str(),
        ),
        (
            WardrobeContainerScope::Project,
            Some(project.as_str()),
            g_coat.as_str(),
        ),
    ] {
        let (resp, lines) =
            cap(&|| wardrobe_item_wear_history_as(&db, &spec.user_id, scope, container, item));
        assert!(
            matches!(&resp, Response::Error(e) if e.kind == ErrorKind::NotFound),
            "{scope:?}: {resp:?}"
        );
        assert!(
            !has(&lines, "Built wear history") && !has(&lines, "wear history"),
            "{scope:?}: {lines:#?}"
        );
    }
    // A container scope without its id has no v4 analog: a 400, no ledger read.
    let (resp, lines) =
        cap(&|| wardrobe_item_wear_history(&db, WardrobeContainerScope::Group, None, &g_livery));
    assert!(
        matches!(&resp, Response::Error(e) if e.kind == ErrorKind::BadRequest),
        "{resp:?}"
    );
    assert!(lines.is_empty(), "{lines:?}");

    // Single GETs: since `9ecebf986` every item body is `serializeOne` —
    // tagged AND worn, so the one `attachWear` line fires (the reverse of the
    // pre-factory "tagged, never worn" pin, retired by measurement).
    let (_, lines) = cap(&|| wardrobe_item_get(&db, &g_coat));
    assert!(has(&lines, "Attached wear summaries"), "{lines:?}");
    let (_, lines) = cap(&|| character_wardrobe_get(&db, &spec.user_id, &aria, &w_top));
    assert!(has(&lines, "Attached wear summaries"), "{lines:?}");
}

/// P4.D281 (v4 `9ecebf986`): the ONE route factory's lines — `<tag> Listed /
/// Created / Updated / Deleted wardrobe item`, `<tag> Read dressing
/// instructions`, `<tag> Dressing instructions updated` — keyed by the tier's
/// `logIdKey` (`generalId: null` on General), v4's keys in v4's order, on the
/// calling thread; the PUT's `archivedAt` key ONLY when the body carried
/// `archived` (the silence leg); and none of the pre-factory per-tier titles.
#[test]
fn p4d281_factory_lines_are_v4s() {
    quilltap_core::test_support::global_capture::install();
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let base = spec
        .cases
        .iter()
        .find(|c| c.name == "wh_general")
        .expect("the corpus carries wh_general");
    let (db, _scratch) = fresh_db(base);
    let raw: Value = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let id = |k: &str| raw["ids"][k].as_str().unwrap().to_string();
    let (aria, gmp, g_coat, project) = (
        id("aria"),
        id("generalMountPointId"),
        id("gCoat"),
        id("project"),
    );
    const SINGLE: &str = "ffffffff-ffff-ffff-ffff-ffffffffffff";
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let cap = |f: &dyn Fn() -> Response| quilltap_core::test_support::global_capture::capture(f);
    let line_ending = |lines: &[String], needle: &str| -> String {
        lines
            .iter()
            .find(|l| l.ends_with(needle))
            .cloned()
            .unwrap_or_else(|| panic!("no line ending {needle:?}: {lines:#?}"))
    };
    let old_titles = [
        "Archetype item created",
        "Archetype item deleted",
        "Wardrobe item deleted",
        "Created project wardrobe item",
        "Updated project wardrobe item",
        "Deleted project wardrobe item",
        "Read General dressing instructions",
        "General dressing instructions updated",
        "Read character dressing instructions",
        "Read project dressing instructions",
    ];
    let no_old = |lines: &[String]| {
        for t in old_titles {
            assert!(!lines.iter().any(|l| l.contains(t)), "{t}: {lines:#?}");
        }
    };

    // General list: `generalId: null` leads, `mountPointId`, `count`, `context`.
    let (resp, lines) = cap(&|| rt.block_on(wardrobe_list(&db, false)));
    let n = success_body(&resp).unwrap()["wardrobeItems"]
        .as_array()
        .unwrap()
        .len();
    let line = line_ending(
        &lines,
        &format!(
            "[Wardrobe Archetypes v1] Listed wardrobe items generalId=null mountPointId={gmp} count={n} context=wardrobe"
        ),
    );
    assert!(line.starts_with("DEBUG "), "{line}");
    no_old(&lines);

    // A character list keys the line by `characterId`.
    let (resp, lines) = cap(&|| character_wardrobe_list(&db, &spec.user_id, &aria, None, false));
    let body = success_body(&resp).unwrap();
    let mp = body["mountPointId"].as_str().unwrap().to_string();
    let n = body["wardrobeItems"].as_array().unwrap().len();
    line_ending(
        &lines,
        &format!(
            "[Wardrobe v1] Listed wardrobe items characterId={aria} mountPointId={mp} count={n} context=wardrobe"
        ),
    );
    // R-H: a READ resolves on the calling thread, so the keystone's resolve
    // line is visible here — asserted by tag only (the store tiers' resolve
    // runs inside the write, on the writer thread).
    assert!(lines
        .iter()
        .any(|l| l.contains("[WardrobeLocation] Resolved wardrobe location")));

    // Create (INFO, `userId` second, `title` before `context`).
    let (resp, lines) = cap(&|| {
        rt.block_on(wardrobe_create(
            &db,
            serde_json::json!({ "title": "Opera Cloak", "types": ["top"] }),
        ))
    });
    let created = success_body(&resp).unwrap()["wardrobeItem"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let line = line_ending(
        &lines,
        &format!(
            "[Wardrobe Archetypes v1] Created wardrobe item generalId=null userId={SINGLE} mountPointId={gmp} itemId={created} title=Opera Cloak context=wardrobe"
        ),
    );
    assert!(line.starts_with("INFO "), "{line}");
    no_old(&lines);

    // Update WITHOUT `archived`: no `archivedAt` key (the silence leg).
    let (_, lines) = cap(&|| {
        rt.block_on(wardrobe_update(
            &db,
            &created,
            serde_json::json!({ "title": "Opera Cape" }),
        ))
    });
    line_ending(
        &lines,
        &format!(
            "[Wardrobe Archetypes v1] Updated wardrobe item generalId=null userId={SINGLE} mountPointId={gmp} itemId={created} context=wardrobe"
        ),
    );
    assert!(
        !lines.iter().any(|l| l.contains("archivedAt=")),
        "{lines:#?}"
    );
    // Update WITH `archived`: `archivedAt` rides LAST, after `context`.
    let (_, lines) = cap(&|| {
        rt.block_on(wardrobe_update(
            &db,
            &created,
            serde_json::json!({ "archived": false }),
        ))
    });
    line_ending(
        &lines,
        &format!(
            "[Wardrobe Archetypes v1] Updated wardrobe item generalId=null userId={SINGLE} mountPointId={gmp} itemId={created} context=wardrobe archivedAt=null"
        ),
    );

    // Delete: `{ ...meta, mountPointId }` — `context` BEFORE `mountPointId`.
    let (_, lines) = cap(&|| rt.block_on(wardrobe_delete(&db, &created)));
    let line = line_ending(
        &lines,
        &format!(
            "[Wardrobe Archetypes v1] Deleted wardrobe item generalId=null itemId={created} context=wardrobe mountPointId={gmp}"
        ),
    );
    assert!(line.starts_with("INFO "), "{line}");
    line_ending(
        &lines,
        &format!(
            "[Wardrobe Archetypes v1] Dropped wear-ledger rows for deleted item generalId=null itemId={created} context=wardrobe"
        ),
    );
    no_old(&lines);

    // A 404 logs no factory line at all.
    let (_, lines) = cap(&|| rt.block_on(wardrobe_delete(&db, &g_coat.replace('e', "f"))));
    assert!(
        !lines.iter().any(|l| l.contains("Deleted wardrobe item")),
        "{lines:#?}"
    );

    // Instructions — the read and the write, General and project.
    let (_, lines) =
        cap(&|| rt.block_on(quilltap_core::api::wardrobe::wardrobe_instructions_get(&db)));
    line_ending(
        &lines,
        &format!(
            "[Wardrobe Archetypes v1] Read dressing instructions generalId=null mountPointId={gmp} present=false context=wardrobe"
        ),
    );
    no_old(&lines);
    let (_, lines) = cap(&|| {
        rt.block_on(
            quilltap_core::api::projects::project_wardrobe_instructions_set(
                &db,
                &project,
                &Some(Some(Value::String("Hats indoors are fine.".into()))),
            ),
        )
    });
    let line = lines
        .iter()
        .find(|l| l.contains("[Projects v1] Dressing instructions updated"))
        .unwrap_or_else(|| panic!("{lines:#?}"));
    assert!(line.starts_with("INFO "), "{line}");
    let tail = line.split("Dressing instructions updated").nth(1).unwrap();
    let keys: Vec<&str> = tail
        .split_whitespace()
        .map(|kv| kv.split('=').next().unwrap())
        .collect();
    assert_eq!(
        keys,
        ["projectId", "userId", "mountPointId", "cleared", "context"],
        "{line}"
    );
    assert!(tail.contains(&format!("projectId={project}")) && tail.contains("cleared=false"));
    no_old(&lines);
}

/// P4.D281 (v4 `9ecebf986` + `4a38222e6`): the transfers route's success INFO
/// `[WardrobeTransfers v1] Wardrobe item transferred` — v4's thirteen keys in
/// v4's order, `folder` (`stored.folder ?? ''`) NEW at the pin, a null
/// character id rendered `null` — on the calling thread; and the silence leg:
/// a refused transfer logs no such line.
#[test]
fn p4d281_transfer_line_is_v4s() {
    quilltap_core::test_support::global_capture::install();
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let cap = |f: &dyn Fn() -> Response| quilltap_core::test_support::global_capture::capture(f);
    let case = |name: &str| {
        spec.cases
            .iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("the corpus carries {name}"))
    };

    let ok = case("tr_move_char_to_char");
    let (db, _scratch) = fresh_db(ok);
    let body = ok.body.clone().unwrap();
    let (resp, lines) = cap(&|| {
        rt.block_on(wardrobe_transfer_apply(
            &db,
            &spec.user_id,
            body.clone(),
            NOW,
        ))
    });
    let item = success_body(&resp).expect("the move succeeds")["wardrobeItem"].clone();
    let line = lines
        .iter()
        .find(|l| l.contains("[WardrobeTransfers v1] Wardrobe item transferred"))
        .unwrap_or_else(|| panic!("no transfer line: {lines:#?}"));
    assert!(line.starts_with("INFO "), "{line}");
    let tail = line.split("Wardrobe item transferred").nth(1).unwrap();
    let keys: Vec<&str> = tail
        .split_whitespace()
        .filter(|kv| kv.contains('='))
        .map(|kv| kv.split('=').next().unwrap())
        .collect();
    assert_eq!(
        keys,
        [
            "userId",
            "action",
            "componentMode",
            "itemId",
            "resultItemId",
            "folder",
            "componentsTransferred",
            "sourceScope",
            "destinationScope",
            "sourceCharacterId",
            "destinationCharacterId",
            "sourceMountPointId",
            "destinationMountPointId",
        ],
        "{line}"
    );
    let item_id = item["id"].as_str().unwrap();
    assert!(
        tail.contains(&format!(" userId={} action=move componentMode=none itemId={item_id} resultItemId={item_id} folder= componentsTransferred=0 sourceScope=character destinationScope=character ", spec.user_id)),
        "{line}"
    );
    // The read-back passed, so its ERROR stayed silent.
    assert!(
        !lines.iter().any(|l| l.contains("did not read back")),
        "{lines:#?}"
    );

    // Silence: a 404 source logs no transfer line.
    let missing = case("tr_source_missing");
    let (db, _scratch) = fresh_db(missing);
    let body = missing.body.clone().unwrap();
    let (_, lines) = cap(&|| {
        rt.block_on(wardrobe_transfer_apply(
            &db,
            &spec.user_id,
            body.clone(),
            NOW,
        ))
    });
    assert!(
        !lines
            .iter()
            .any(|l| l.contains("Wardrobe item transferred")),
        "{lines:#?}"
    );
}

/// [P4.D316] Capture pins (§R.5) for the rollback POST's caller-thread lines,
/// each with its silence leg: the success INFO `[Chats v1] Wardrobe rolled
/// back` (v4's fields, `context` LAST), the module's DEBUG `Rolled a wardrobe
/// back` AFTER the effects with the route's `sourceContext` and the operator
/// provenance (the resolver's own DEBUG `Resolved operator for an outfit
/// change` is a WRITER-thread line since the `e8e17b2d9` unification — see
/// the body); a refusal's INFO `[Chats v1] Wardrobe rollback
/// refused` with no success line; and the ERROR `[Chats v1] Error rolling back
/// wardrobe` + 500 when the history read fails (the table dropped — v4's
/// `findEntries` throws into the same catch). The in-write `Rollback left a
/// stale item off` runs on the writer thread here; its bytes are pinned on
/// the tool path (`wardrobe_tools_equivalence`, where the handler runs on the
/// test thread).
#[tokio::test(flavor = "current_thread")]
async fn p4d316_rollback_route_lines_are_v4s() {
    use quilltap_core::test_support::global_capture;
    global_capture::install();
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let case = |n: &str| spec.cases.iter().find(|c| c.name == n).unwrap();
    let ok = case("rb_ok_skipped_speaking_as");
    let (db, _s1) = fresh_db(ok);
    let chat = ok.chat_id.clone().unwrap();
    let (resp, lines) = global_capture::capture_async(chat_wardrobe_rollback(
        &db,
        &spec.user_id,
        &chat,
        ok.body.clone().unwrap(),
    ))
    .await;
    assert!(success_body(&resp).is_some(), "{resp:?}");
    let find = |lines: &[String], needle: &str| -> String {
        let hits: Vec<&String> = lines.iter().filter(|l| l.contains(needle)).collect();
        assert_eq!(hits.len(), 1, "{needle}: {lines:#?}");
        hits[0].clone()
    };
    let aria = "a1000000-0000-4000-8000-000000000001";
    let bram = "a1000000-0000-4000-8000-000000000002";
    let done = find(&lines, "[Chats v1] Wardrobe rolled back");
    assert!(
        done.ends_with(&format!(
            "[Chats v1] Wardrobe rolled back chatId={chat} characterId={aria} previousIndex=2 restoredIndex=1 skipped=2 context=wardrobe"
        )),
        "{done}"
    );
    let rolled = find(&lines, "Rolled a wardrobe back");
    assert!(
        rolled.ends_with(&format!(
            "Rolled a wardrobe back context=[Chats v1] wardrobe-rollback chatId={chat} characterId={aria} previousIndex=2 restoredIndex=1 skipped=2 provenance=operator"
        )),
        "{rolled}"
    );
    // The resolver's DEBUG `Resolved operator for an outfit change` runs INSIDE
    // the write since the `e8e17b2d9` unification (after `load_wearable_pool`,
    // before the rollback's first read — v4 `outfit.ts:383-389`'s statement
    // order, which the lane had inverted by resolving before the write), so
    // it is a writer-thread line this per-thread rig cannot see — like the
    // in-write `Rollback left a stale item off` above. Its bytes (the
    // Speaking-As `actorCharacterId`) are pinned by `operator_provenance`'s
    // unit tests; its ORDER is pinned by construction (one closure: pool →
    // provenance → `rollback_in_write`). Its ABSENCE here is the tripwire
    // against moving it back onto the caller thread (and out of v4's order).
    assert!(
        !lines
            .iter()
            .any(|l| l.contains("Resolved operator for an outfit change")),
        "the resolver's line is a writer-thread line now: {lines:#?}"
    );
    let order = |needle: &str| lines.iter().position(|l| l.contains(needle)).unwrap();
    assert!(
        order("Rolled a wardrobe back") < order("[Chats v1] Wardrobe rolled back"),
        "v4's order: {lines:#?}"
    );
    let _ = bram;
    assert!(!lines
        .iter()
        .any(|l| l.contains("Wardrobe rollback refused")));
    assert!(!lines
        .iter()
        .any(|l| l.contains("Error rolling back wardrobe")));

    // A refusal: the INFO with v4's fields, nothing else of the route's.
    let refuse = case("rb_refuse_already_current");
    let (db, _s2) = fresh_db(refuse);
    let (resp, lines) = global_capture::capture_async(chat_wardrobe_rollback(
        &db,
        &spec.user_id,
        &chat,
        refuse.body.clone().unwrap(),
    ))
    .await;
    assert!(matches!(resp, Response::Error(ref e) if e.kind == ErrorKind::BadRequest));
    let refused = find(&lines, "[Chats v1] Wardrobe rollback refused");
    assert!(
        refused.ends_with(&format!(
            "[Chats v1] Wardrobe rollback refused chatId={chat} characterId={aria} index=2 reason=already_current context=wardrobe"
        )),
        "{refused}"
    );
    assert!(!lines.iter().any(|l| l.contains("Wardrobe rolled back")));
    assert!(!lines.iter().any(|l| l.contains("Rolled a wardrobe back")));

    // The catch: a failed history read → the ERROR + 500, no success line.
    db.write(|w| {
        w.main()
            .connection()
            .execute_batch("DROP TABLE chat_wardrobe_history;")
            .map_err(quilltap_core::db::DbError::from)
    })
    .await
    .unwrap();
    let (resp, lines) = global_capture::capture_async(chat_wardrobe_rollback(
        &db,
        &spec.user_id,
        &chat,
        refuse.body.clone().unwrap(),
    ))
    .await;
    match &resp {
        Response::Error(e) => {
            assert_eq!(e.kind, ErrorKind::Internal);
            assert_eq!(e.message, "Failed to roll back the outfit");
        }
        other => panic!("expected the 500: {other:?}"),
    }
    let error = find(&lines, "[Chats v1] Error rolling back wardrobe");
    assert!(
        error.ends_with(&format!(
            "[Chats v1] Error rolling back wardrobe chatId={chat} error=no such table: chat_wardrobe_history"
        )),
        "{error}"
    );
    assert!(!lines.iter().any(|l| l.contains("Wardrobe rolled back")));
}
