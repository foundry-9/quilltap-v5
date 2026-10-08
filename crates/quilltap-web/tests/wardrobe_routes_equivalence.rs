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
use quilltap_core::api::chat_outfits::{chat_equip, chat_outfit_get, chat_regenerate_avatar};
use quilltap_core::api::types::{ErrorKind, Response, WardrobeContainerScope};
use quilltap_core::api::wardrobe::{
    wardrobe_create, wardrobe_delete, wardrobe_item_get, wardrobe_list, wardrobe_preview_avatar,
    wardrobe_transfer_apply, wardrobe_transfer_destinations, wardrobe_update, AvatarPreviewImage,
    AvatarPreviewRenderRequest, AvatarPreviewRenderer, ErasedAvatarPreview,
};
use quilltap_core::api::wardrobe_wear_history::wardrobe_item_wear_history;
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
        let (db, _scratch) = fresh_db(case);
        let body = case.body.clone().unwrap_or(Value::Null);
        let resp = match case.kind.as_str() {
            "wardrobeList" => wardrobe_list(&db, case.include_archived.unwrap_or(false)),
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
            "characterWardrobeItemGet" => match case.action.as_deref() {
                None => character_wardrobe_get(
                    &db,
                    user,
                    case.character_id.as_deref().unwrap(),
                    case.item_id.as_deref().unwrap(),
                ),
                Some("wear-history") => wardrobe_item_wear_history(
                    &db,
                    WardrobeContainerScope::Character,
                    case.character_id.as_deref(),
                    case.item_id.as_deref().unwrap(),
                ),
                Some(other) => panic!("unknown action {other}"),
            },
            "wardrobeUpdate" => wardrobe_update(&db, case.item_id.as_deref().unwrap(), body).await,
            "wardrobeDelete" => wardrobe_delete(&db, case.item_id.as_deref().unwrap()).await,
            "transferDestinations" => wardrobe_transfer_destinations(&db, user),
            "transferApply" => wardrobe_transfer_apply(&db, user, body, NOW).await,
            "outfitGet" => chat_outfit_get(&db, case.chat_id.as_deref().unwrap()),
            "equip" => chat_equip(&db, user, case.chat_id.as_deref().unwrap(), body).await,
            "regenerateAvatar" => {
                chat_regenerate_avatar(&db, user, case.chat_id.as_deref().unwrap(), body).await
            }
            "previewAvatar" => wardrobe_preview_avatar(&db, &renderer, user, body, NOW, None).await,
            "characterWardrobeList" => character_wardrobe_list(
                &db,
                user,
                case.character_id.as_deref().unwrap(),
                case.scope.as_deref(),
                case.include_archived.unwrap_or(false),
            ),
            other => panic!("unknown case kind: {other}"),
        };
        check(
            &oracle,
            &case.name,
            &resp,
            &case.normalize,
            &case.classify,
            &mut failed,
        );
        checks += 1;

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
    let cap = |f: &dyn Fn() -> Response| quilltap_core::test_support::captured_with(f);
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

    // Each tier's wear-history DEBUG, after the payload.
    for (scope, container, item, needle) in [
        (WardrobeContainerScope::General, None, g_coat.as_str(),
         format!("[Wardrobe Archetypes v1] Read archetype item wear history itemId={g_coat} wearCount=6")),
        (WardrobeContainerScope::Character, Some(aria.as_str()), w_top.as_str(),
         format!("[Wardrobe v1] Read wardrobe item wear history characterId={aria} itemId={w_top} wearCount=2")),
        (WardrobeContainerScope::Group, Some(group.as_str()), g_livery.as_str(),
         format!("[Groups v1] Read group wardrobe item wear history groupId={group} itemId={g_livery} wearCount=1 context=wardrobe")),
        (WardrobeContainerScope::Project, Some(project.as_str()), p_scarf.as_str(),
         format!("[Projects v1] Read project wardrobe item wear history projectId={project} itemId={p_scarf} wearCount=0 context=wardrobe")),
    ] {
        let (resp, lines) = cap(&|| wardrobe_item_wear_history(&db, scope, container, item));
        assert!(matches!(resp, Response::WardrobeWearHistory(_)), "{scope:?}: {resp:?}");
        let built = lines.iter().position(|l| l.contains("Built wear history"));
        let read = lines.iter().position(|l| l.ends_with(&needle));
        assert!(built.is_some() && read.is_some() && built < read, "{scope:?}: {lines:#?}");
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
        let (resp, lines) = cap(&|| wardrobe_item_wear_history(&db, scope, container, item));
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

    // Single GETs: tagged, never worn — no `Attached wear summaries` line.
    let (_, lines) = cap(&|| wardrobe_item_get(&db, &g_coat));
    assert!(!has(&lines, "Attached wear summaries"), "{lines:?}");
    let (_, lines) = cap(&|| character_wardrobe_get(&db, &spec.user_id, &aria, &w_top));
    assert!(!has(&lines, "Attached wear summaries"), "{lines:?}");
}
