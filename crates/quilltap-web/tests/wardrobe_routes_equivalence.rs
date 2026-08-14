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
//! Generate the oracle (see the .ts header), then:
//!   QT_ORACLE_WARDROBE_ROUTES=/tmp/oracle-wardrobe-routes.ndjson \
//!     cargo test -p quilltap-web --test wardrobe_routes_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::characters::character_wardrobe_list;
use quilltap_core::api::chat_outfits::{chat_equip, chat_outfit_get, chat_regenerate_avatar};
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::api::wardrobe::{
    wardrobe_create, wardrobe_delete, wardrobe_item_get, wardrobe_list, wardrobe_preview_avatar,
    wardrobe_transfer_apply, wardrobe_transfer_destinations, wardrobe_update, AvatarPreviewImage,
    AvatarPreviewRenderRequest, AvatarPreviewRenderer, ErasedAvatarPreview,
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
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    user_id: String,
    cases: Vec<CaseEntry>,
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
fn fresh_db(tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-wroutes-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("wardrobe-routes-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("wardrobe-routes-mount.db"), &mount).unwrap();
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

/// The canned preview render — replays the oracle's captured post-transcode
/// image (`pv_ok_bytes`), the honest model+codec boundary.
struct CannedRenderer {
    buffer: Vec<u8>,
    mime_type: String,
    filename: String,
    revised_prompt: Option<String>,
}
impl AvatarPreviewRenderer for CannedRenderer {
    fn render<'a>(
        &'a self,
        _req: &'a AvatarPreviewRenderRequest,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<
                        AvatarPreviewImage,
                        quilltap_core::api::wardrobe::AvatarPreviewError,
                    >,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async {
            Ok(AvatarPreviewImage {
                buffer: self.buffer.clone(),
                mime_type: self.mime_type.clone(),
                filename: self.filename.clone(),
                revised_prompt: self.revised_prompt.clone(),
            })
        })
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
        Response::Wardrobe(v) | Response::ChatOutfit(v) | Response::Character(v) => Some(v.clone()),
        _ => None,
    }
}

/// Blank the corpus-declared dot-paths (minted ids/timestamps) on a body.
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
    failed: &mut Vec<String>,
) {
    let want = oracle
        .get(name)
        .unwrap_or_else(|| panic!("oracle has no case '{name}' — regenerate the NDJSON"));
    let want_status = want["status"].as_u64().unwrap() as u16;
    if (200..300).contains(&want_status) {
        match success_body(resp) {
            Some(body) => {
                let got = blank_paths(&body, normalize);
                let expect = blank_paths(&want["body"], normalize);
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
    fn key_walk(v: &Value, out: &mut Vec<String>, prefix: &str) {
        if let Some(o) = v.as_object() {
            for (k, inner) in o {
                out.push(format!("{prefix}{k}"));
                key_walk(inner, out, &format!("{prefix}{k}."));
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
        + spec.cases.iter().filter(|c| c.emit_bytes).count();
    assert_eq!(
        oracle.len(),
        expected_rows,
        "oracle row count {} != corpus expectation {expected_rows} — regenerate the NDJSON",
        oracle.len()
    );

    // The canned preview renderer, replaying the oracle's captured transcode.
    let renderer = if let Some(bytes_row) = oracle.get("pv_ok_bytes") {
        ErasedAvatarPreview::new(CannedRenderer {
            buffer: decode_bytes(bytes_row),
            mime_type: bytes_row["body"]["mimeType"]
                .as_str()
                .unwrap_or("image/webp")
                .to_string(),
            filename: bytes_row["body"]["filename"]
                .as_str()
                .unwrap_or("avatar_preview.webp")
                .to_string(),
            revised_prompt: bytes_row["body"]["revisedPrompt"]
                .as_str()
                .map(str::to_string),
        })
    } else {
        ErasedAvatarPreview::none()
    };

    let user = spec.user_id.as_str();
    let mut failed: Vec<String> = Vec::new();
    let mut checks = 0usize;

    for case in &spec.cases {
        let db = fresh_db(&case.name);
        let body = case.body.clone().unwrap_or(Value::Null);
        let resp = match case.kind.as_str() {
            "wardrobeList" => wardrobe_list(&db),
            "wardrobeCreate" => wardrobe_create(&db, body).await,
            "wardrobeItemGet" => wardrobe_item_get(&db, case.item_id.as_deref().unwrap()),
            "wardrobeUpdate" => wardrobe_update(&db, case.item_id.as_deref().unwrap(), body).await,
            "wardrobeDelete" => wardrobe_delete(&db, case.item_id.as_deref().unwrap()).await,
            "transferDestinations" => wardrobe_transfer_destinations(&db, user),
            "transferApply" => wardrobe_transfer_apply(&db, user, body, NOW).await,
            "outfitGet" => chat_outfit_get(&db, case.chat_id.as_deref().unwrap()),
            "equip" => chat_equip(&db, user, case.chat_id.as_deref().unwrap(), body).await,
            "regenerateAvatar" => {
                chat_regenerate_avatar(&db, user, case.chat_id.as_deref().unwrap(), body).await
            }
            "previewAvatar" => wardrobe_preview_avatar(&db, &renderer, user, body, NOW).await,
            "characterWardrobeList" => character_wardrobe_list(
                &db,
                user,
                case.character_id.as_deref().unwrap(),
                case.scope.as_deref(),
            ),
            other => panic!("unknown case kind: {other}"),
        };
        check(&oracle, &case.name, &resp, &case.normalize, &mut failed);
        checks += 1;

        // The two raw key-order claims (the richest read + the equip echo).
        if case.name == "outfit_preset" || case.name == "eq_set_all" {
            check_key_order(&oracle, &case.name, &resp, &mut failed);
            checks += 1;
        }

        if let Some(character_id) = &case.then_group_wardrobe {
            let follow = character_wardrobe_list(&db, user, character_id, Some("group"));
            check(
                &oracle,
                &format!("{}__group", case.name),
                &follow,
                &case.group_normalize,
                &mut failed,
            );
            checks += 1;
        }

        if let Some(chat_id) = &case.then_outfit {
            let follow = chat_outfit_get(&db, chat_id);
            check(
                &oracle,
                &format!("{}__outfit", case.name),
                &follow,
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
