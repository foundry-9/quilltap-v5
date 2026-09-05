//! P4.76 — `POST /api/v1/images?action=generate`: `api::images::images_generate`
//! vs v4's REAL `handleGenerateImage` (`app/api/v1/images/route.ts:177-408`).
//!
//! Tier 3 (the image provider and the Concierge classifier are mocked BELOW
//! v4's real route handler), over the committed images fixture — a FRESH copy
//! per case, with this case's `dangerousContentSettings` patched in by raw SQL
//! on BOTH sides.
//!
//! ## What is actually compared
//!
//! * **status + body**, key order included (`serde_json` is built with
//!   `preserve_order`, so an object equality is order-insensitive — the
//!   `key_paths` walk makes the sequence an EXPLICIT assertion);
//! * **the ordered provider calls** — `{provider, apiKey, params}`. The params
//!   are the shared builder's output, so a `mergeParameters` divergence shows
//!   as a diff rather than as a canned-key miss; the `apiKey` is what proves an
//!   AUTO_ROUTE reroute switched PROFILES and not merely names;
//! * **the ordered classification calls** — `{content, selection, userId,
//!   settings}`. The `CheapLLMSelection` is otherwise invisible: the route
//!   builds it from `allProfiles` + `cheapLLMSettings` and hands it straight to
//!   the classifier, so recording it is the only way to pin
//!   `build_cheap_llm_selection` on this path. An EMPTY array is the comparand
//!   for the two gate conjuncts (`mode != 'OFF'`, `scanImagePrompts`);
//! * **the post-mutation `files` rows and Lantern mount links** — so a refusal
//!   proves it wrote NOTHING, and the store write's path/mime/sha are pinned.
//!
//! Provider bytes are `image/webp`, which `convertToWebP` passes through: the
//! stored bytes, their sha, the `_<sha8>_` inside the filename and the byte
//! length are therefore identical on both sides and are REAL comparands. The
//! one `image/png` case exercises the transcode POLICY instead (D19 — compare
//! the mime, never sharp's bytes), with the codec-dependent fields blanked on
//! both sides.
//!
//! Generate the oracle (Node 24, from the v4 checkout — the full recipe lives
//! in the .test.ts header):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-images-generate-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/images-generate-route.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/images-collection.json" "$TMPO/fixtures/"
//!   cp "$V5W/crates/quilltap-web/tests/fixtures/images-main.db"  /tmp/qt-imgcol-main.db
//!   cp "$V5W/crates/quilltap-web/tests/fixtures/images-mount.db" /tmp/qt-imgcol-mount.db
//!   cd ~/source/quilltap-server
//!   TZ=UTC QT_FIXTURE_IMGCOL_MAIN=/tmp/qt-imgcol-main.db QT_FIXTURE_IMGCOL_MOUNT=/tmp/qt-imgcol-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-images-generate.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=300000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- images-generate-route
//! Run:
//!   QT_ORACLE_IMAGES_GENERATE=/tmp/oracle-images-generate.ndjson \
//!     cargo test -p quilltap-harness --test images_generate_route_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde_json::{json, Map, Value};

use quilltap_core::api::images::{
    ErasedImagePromptClassifier, ImagePromptClassifier, ImagesGenerateSeams,
};
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::cheap_llm::CheapLlmSelection;
use quilltap_core::db::chat_settings::DangerousContentSettings;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::image::{
    ErasedImageGenerate, GeneratedImageData, ImageGenError, ImageGenParams, ImageGenResponse,
    ImageProvider,
};
use quilltap_core::services::dangerous_content::gatekeeper::{
    DangerCategory, DangerClassificationResult,
};

/// v5's `SINGLE_USER_ID` — the fixture's owner.
const USER_A: &str = "11111111-1111-4111-8111-111111111111";

const PROFILE_MAIN: &str = "aaaa0000-0000-4000-8000-000000000001";
const PROFILE_UNCENSORED: &str = "aaaa0000-0000-4000-8000-000000000002";
const PROFILE_NOIMAGE: &str = "aaaa0000-0000-4000-8000-000000000003";
const MISSING_PROFILE: &str = "aaaa0000-0000-4000-8000-0000000000ff";
const CHAR_TAG: &str = "c1000000-0000-4000-8000-000000000003";
const THEME_TAG: &str = "ee000000-0000-4000-8000-000000000001";
const LANTERN_MP: &str = "80000000-0000-4000-8000-000000000002";

/// The same 1x1 PNG the oracle feeds v4 — real bytes, so sharp transcodes on
/// that side and [`PrefixCodec`] transcodes on this one.
fn png_1x1() -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
        )
        .expect("the corpus PNG")
}

/// A byte-CHANGING codec, so the PNG case genuinely exercises `convertToWebP`'s
/// convert branch rather than falling through it. Every other case feeds
/// `image/webp`, which the conversion PASSES THROUGH on both sides — which is
/// what makes their shas comparable.
struct PrefixCodec;

impl quilltap_core::services::file_storage::PixelCodec for PrefixCodec {
    fn encode_webp(
        &self,
        bytes: &[u8],
        _quality: i64,
        _effort: Option<i64>,
        _animated: bool,
    ) -> Result<Vec<u8>, String> {
        let mut out = b"QTAP-P476-WEBP:".to_vec();
        out.extend_from_slice(bytes);
        Ok(out)
    }

    fn measure(&self, _bytes: &[u8]) -> (Option<i64>, Option<i64>) {
        (Some(1), Some(1))
    }
}

#[derive(Clone, Copy, PartialEq)]
enum ProviderMode {
    Webp,
    Png,
    Throw,
    NoData,
}

/// The image-provider seam: behavioural (answers `params.n` images built from
/// the index) and RECORDING. Deliberately not the keyed [`CannedImageProvider`]
/// — a keyed map answers "miss" where this shows the divergence.
struct RecordingImageProvider {
    mode: ProviderMode,
    calls: Arc<Mutex<Vec<Value>>>,
}

impl ImageProvider for RecordingImageProvider {
    async fn generate_image(
        &self,
        provider: &str,
        api_key: &str,
        params: &ImageGenParams,
    ) -> Result<ImageGenResponse, ImageGenError> {
        self.calls.lock().unwrap().push(json!({
            "provider": provider,
            // v4's factory takes `profile.baseUrl ?? undefined`; no v5 image
            // dialect threads it, so the oracle records `null` and this records
            // the same constant. Recorded, not asserted-away: the day a dialect
            // learns base URLs, this row is where it shows up.
            "baseUrl": Value::Null,
            "apiKey": api_key,
            "params": params.to_key_value(),
        }));
        if self.mode == ProviderMode::Throw {
            return Err(ImageGenError::new("canned provider failure"));
        }
        let n = params.n.unwrap_or(1.0).max(1.0) as usize;
        let images = (0..n)
            .map(|i| match self.mode {
                ProviderMode::NoData => GeneratedImageData {
                    data: None,
                    url: None,
                    mime_type: Some("image/webp".into()),
                    revised_prompt: Some(format!("revised {i}")),
                },
                ProviderMode::Png => GeneratedImageData {
                    data: Some({
                        use base64::Engine;
                        base64::engine::general_purpose::STANDARD.encode(png_1x1())
                    }),
                    url: None,
                    mime_type: Some("image/png".into()),
                    revised_prompt: Some(format!("revised {i}")),
                },
                _ => GeneratedImageData {
                    data: Some({
                        use base64::Engine;
                        base64::engine::general_purpose::STANDARD
                            .encode(format!("QTAP-P476-WEBP-{i}").as_bytes())
                    }),
                    url: None,
                    mime_type: Some("image/webp".into()),
                    // Index 1 carries NO revision, so the receipt's
                    // undefined-drop (an ABSENT key, never null) is measured.
                    revised_prompt: if i == 1 {
                        None
                    } else {
                        Some(format!("revised {i}"))
                    },
                },
            })
            .collect();
        Ok(ImageGenResponse { images })
    }
}

/// The Concierge seam: canned + recording, so the `CheapLLMSelection` the route
/// built is a comparand.
struct RecordingClassifier {
    canned: DangerClassificationResult,
    calls: Arc<Mutex<Vec<Value>>>,
}

impl ImagePromptClassifier for RecordingClassifier {
    fn classify<'a>(
        &'a self,
        _db: &'a Db,
        content: &'a str,
        selection: &'a CheapLlmSelection,
        user_id: &'a str,
        settings: &'a DangerousContentSettings,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = DangerClassificationResult> + Send + 'a>>
    {
        // v4's `CheapLLMSelection` is a plain object serialized by
        // `JSON.stringify`, so an `undefined` field is an ABSENT key.
        let mut sel = Map::new();
        sel.insert("provider".into(), json!(selection.provider));
        sel.insert("modelName".into(), json!(selection.model_name));
        if let Some(b) = &selection.base_url {
            sel.insert("baseUrl".into(), json!(b));
        }
        if let Some(p) = &selection.connection_profile_id {
            sel.insert("connectionProfileId".into(), json!(p));
        }
        sel.insert("isLocal".into(), json!(selection.is_local));
        if let Some(p) = &selection.profile_parameters {
            sel.insert("profileParameters".into(), p.clone());
        }
        self.calls.lock().unwrap().push(json!({
            "content": content,
            "selection": Value::Object(sel),
            "userId": user_id,
            "settings": serde_json::to_value(settings).expect("settings serialize"),
            // v4's route calls the four-argument form — there is no chat here.
            "chatId": Value::Null,
        }));
        let canned = self.canned.clone();
        Box::pin(async move { canned })
    }
}

fn safe() -> DangerClassificationResult {
    DangerClassificationResult {
        is_dangerous: false,
        score: 0.1,
        categories: vec![],
        usage: None,
        source: Some("llm".into()),
        provider_name: None,
    }
}

fn dangerous() -> DangerClassificationResult {
    DangerClassificationResult {
        is_dangerous: true,
        score: 0.93,
        categories: vec![DangerCategory {
            category: "sexual".into(),
            score: 0.93,
            label: "Sexual content".into(),
        }],
        usage: None,
        source: Some("llm".into()),
        provider_name: None,
    }
}

#[derive(serde::Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "frozenNowMs")]
    frozen_now_ms: i64,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/images-collection.json")
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-imggen-r-{}-{}", tag, std::process::id()));
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
        ErrorKind::Locked | ErrorKind::Unavailable => 503,
        ErrorKind::Internal => 500,
    }
}

/// Integer-valued floats → integers (JS-number parity), key order preserved.
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

/// The ordered key sequence of every object, depth-first — an EXPLICIT
/// assertion, since `preserve_order`'s `IndexMap` equality ignores order.
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

fn dump_query(conn: &rusqlite::Connection, sql: &str) -> Result<Value, quilltap_core::db::DbError> {
    let mut stmt = conn.prepare(sql)?;
    let cols: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let rows = stmt.query_map([], |row| {
        let mut m = Map::new();
        for (i, name) in cols.iter().enumerate() {
            let v = match row.get_ref(i)? {
                rusqlite::types::ValueRef::Null => Value::Null,
                rusqlite::types::ValueRef::Integer(x) => Value::from(x),
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

/// The oracle's `dumpTables`, column for column.
fn dump_tables(db: &Db) -> Value {
    let files = db
        .read_main(|c| {
            dump_query(
                c,
                "SELECT id, userId, sha256, originalFilename, mimeType, size, width, height, \
                 source, category, linkedTo, tags, description, generationPrompt, \
                 generationModel, generationRevisedPrompt, storageKey, fileStatus FROM files \
                 WHERE source = 'GENERATED' ORDER BY originalFilename",
            )
        })
        .expect("dump files");
    let links = db
        .read_mount_index(|c| {
            dump_query(
                c,
                &format!(
                    "SELECT l.relativePath, l.fileName, l.originalMimeType, f.sha256, \
                     f.fileSizeBytes, f.fileType FROM doc_mount_file_links l \
                     JOIN doc_mount_files f ON f.id = l.fileId \
                     WHERE l.mountPointId = '{LANTERN_MP}' ORDER BY l.relativePath"
                ),
            )
        })
        .expect("dump links");
    json!({ "files": files, "links": links })
}

fn blank(row: &mut Value, key: &str) {
    if let Some(o) = row.as_object_mut() {
        if o.contains_key(key) {
            o.insert(key.to_string(), Value::String(format!("<{key}>")));
        }
    }
}

/// The seeded GENERATED row (`F_GENERATED`) — never minted, never blanked.
const SEEDED_GENERATED: &str = "f0000000-0000-4000-8000-000000000003";

/// Blank what the CASE minted: the row id and the blob id inside its storage
/// key, on both sides. `transcoded` additionally blanks the codec-dependent
/// fields — the WebP bytes are sharp's on one side and [`PrefixCodec`]'s on the
/// other, and the stored FILENAME carries their sha8, so it goes with them
/// (D19: compare the POLICY, which is the `mimeType` left standing).
fn blank_minted(v: &mut Value, transcoded: bool) {
    if let Some(files) = v.get_mut("files").and_then(Value::as_array_mut) {
        for row in files.iter_mut() {
            if row.get("id").and_then(Value::as_str) == Some(SEEDED_GENERATED) {
                continue;
            }
            blank(row, "id");
            blank(row, "storageKey");
            if transcoded {
                blank(row, "sha256");
                blank(row, "size");
                blank(row, "originalFilename");
            }
        }
    }
    if transcoded {
        if let Some(links) = v.get_mut("links").and_then(Value::as_array_mut) {
            for row in links.iter_mut() {
                blank(row, "sha256");
                blank(row, "fileSizeBytes");
                blank(row, "relativePath");
                blank(row, "fileName");
            }
        }
    }
}

/// The receipt's minted values (and, for the transcoded case, the
/// codec-dependent filename/size).
fn blank_receipt(v: &mut Value, transcoded: bool) {
    let Some(data) = v.get_mut("data").and_then(Value::as_array_mut) else {
        return;
    };
    for row in data.iter_mut() {
        blank(row, "id");
        blank(row, "filepath");
        blank(row, "url");
        if transcoded {
            blank(row, "filename");
            blank(row, "size");
        }
    }
}

struct Case {
    name: &'static str,
    prompt: Option<Value>,
    profile_id: Option<Value>,
    tags: Option<Value>,
    options: Option<Value>,
    danger: Option<Value>,
    classify: DangerClassificationResult,
    provider: ProviderMode,
    drop_lantern: bool,
}

impl Case {
    fn new(name: &'static str, prompt: Option<Value>, profile_id: Option<Value>) -> Self {
        Self {
            name,
            prompt,
            profile_id,
            tags: None,
            options: None,
            danger: None,
            classify: safe(),
            provider: ProviderMode::Webp,
            drop_lantern: false,
        }
    }
}

const PROMPT: &str = "A brass observatory at dusk";

fn ok(name: &'static str) -> Case {
    Case::new(name, Some(json!(PROMPT)), Some(json!(PROFILE_MAIN)))
}

#[allow(clippy::too_many_lines)]
fn cases() -> Vec<Case> {
    let mut v = vec![
        ok("generate_ok"),
        Case {
            options: Some(json!({ "n": 3 })),
            ..ok("generate_count3")
        },
        Case {
            options: Some(json!({
                "n": 1, "size": "512x768", "quality": "hd", "style": "vivid",
                "aspectRatio": "2:3"
            })),
            ..ok("generate_options_all")
        },
        Case {
            tags: Some(json!([
                { "tagId": CHAR_TAG, "tagType": "CHARACTER", "extra": 1 },
                { "tagType": "THEME", "tagId": THEME_TAG }
            ])),
            ..ok("generate_with_tags")
        },
        Case {
            provider: ProviderMode::Png,
            ..ok("generate_png_transcode")
        },
        // ── the Concierge gate ──
        // DETECT_ONLY + dangerous: classified and logged, never rerouted.
        // ⚠ Only v4's three modes exist (`OFF` / `DETECT_ONLY` / `AUTO_ROUTE`)
        // — see the oracle case's comment for what a schema-invalid
        // `chat_settings` row does on v4's side and why that divergence is
        // recorded in the lane record rather than pinned here.
        Case {
            danger: Some(json!({ "mode": "DETECT_ONLY" })),
            classify: dangerous(),
            ..ok("generate_danger_detect_only")
        },
        Case {
            danger: Some(json!({
                "mode": "AUTO_ROUTE",
                "uncensoredImageProfileId": PROFILE_NOIMAGE
            })),
            classify: dangerous(),
            ..ok("generate_danger_autoroute")
        },
        Case {
            danger: Some(json!({ "mode": "AUTO_ROUTE" })),
            classify: dangerous(),
            ..Case::new(
                "generate_danger_autoroute_no_target",
                Some(json!(PROMPT)),
                Some(json!(PROFILE_UNCENSORED)),
            )
        },
        Case {
            danger: Some(json!({ "mode": "AUTO_ROUTE" })),
            ..ok("generate_danger_autoroute_safe")
        },
        Case {
            danger: Some(json!({ "mode": "OFF" })),
            classify: dangerous(),
            ..ok("generate_danger_off")
        },
        Case {
            danger: Some(json!({ "mode": "AUTO_ROUTE", "scanImagePrompts": false })),
            classify: dangerous(),
            ..ok("generate_scan_disabled")
        },
        // ── the refusals ──
        Case::new(
            "generate_profile_missing",
            Some(json!(PROMPT)),
            Some(json!(MISSING_PROFILE)),
        ),
        Case::new(
            "generate_provider_no_images",
            Some(json!(PROMPT)),
            Some(json!(PROFILE_NOIMAGE)),
        ),
        Case {
            provider: ProviderMode::Throw,
            ..ok("generate_provider_throws")
        },
        Case {
            provider: ProviderMode::NoData,
            ..ok("generate_no_image_data")
        },
        Case {
            drop_lantern: true,
            ..ok("generate_lantern_unprovisioned")
        },
    ];

    // ── `generateImageSchema` — every arm is v4's `Validation error` 400 ──
    let long = "x".repeat(4001);
    v.extend([
        Case::new("zod_prompt_missing", None, Some(json!(PROFILE_MAIN))),
        Case::new(
            "zod_prompt_empty",
            Some(json!("")),
            Some(json!(PROFILE_MAIN)),
        ),
        Case::new(
            "zod_prompt_too_long",
            Some(json!(long)),
            Some(json!(PROFILE_MAIN)),
        ),
        Case::new(
            "zod_prompt_wrong_type",
            Some(json!(42)),
            Some(json!(PROFILE_MAIN)),
        ),
        Case::new("zod_profile_missing", Some(json!(PROMPT)), None),
        Case::new(
            "zod_profile_not_uuid",
            Some(json!(PROMPT)),
            Some(json!("not-a-uuid")),
        ),
        Case::new("zod_profile_null", Some(json!(PROMPT)), Some(Value::Null)),
        // A body that is not an object reaches v4's Zod parse with nothing in
        // it; the web edge folds it to all-absent, which is what this drives.
        Case::new("zod_body_not_object", None, None),
        Case {
            options: Some(json!({ "n": 0 })),
            ..ok("zod_count_zero")
        },
        Case {
            options: Some(json!({ "n": 20 })),
            ..ok("zod_count_over_max")
        },
        Case {
            options: Some(json!({ "n": 1.5 })),
            ..ok("zod_count_fractional")
        },
        Case {
            options: Some(json!({ "n": "2" })),
            ..ok("zod_count_string")
        },
        Case {
            options: Some(json!({ "quality": "ultra" })),
            ..ok("zod_quality_bad")
        },
        Case {
            options: Some(json!({ "style": "painterly" })),
            ..ok("zod_style_bad")
        },
        Case {
            options: Some(json!({ "size": 512 })),
            ..ok("zod_size_wrong_type")
        },
        Case {
            options: Some(Value::Null),
            ..ok("zod_options_null")
        },
        Case {
            options: Some(json!(7)),
            ..ok("zod_options_not_object")
        },
        Case {
            tags: Some(Value::Null),
            ..ok("zod_tags_null")
        },
        Case {
            tags: Some(json!([{ "tagType": "THEME", "tagId": 5 }])),
            ..ok("zod_tags_raw_tagid")
        },
        Case {
            tags: Some(json!([{ "tagType": "PERSON", "tagId": THEME_TAG }])),
            ..ok("zod_tags_bad_tagtype")
        },
        Case {
            tags: Some(json!("nope")),
            ..ok("zod_tags_not_array")
        },
    ]);
    v
}

#[test]
fn images_generate_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_IMAGES_GENERATE") else {
        eprintln!("SKIP: set QT_ORACLE_IMAGES_GENERATE (see the test header).");
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

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut failed: Vec<String> = Vec::new();
    let mut driven: Vec<String> = Vec::new();

    for c in cases() {
        driven.push(c.name.to_string());
        let db = fresh_db(&spec, c.name);

        // This case's danger bag, merged over the stored one — the same raw
        // UPDATE the oracle makes, on a fresh copy of the same fixture.
        if let Some(patch) = &c.danger {
            let patch = patch.clone();
            db.write_blocking(move |ws| {
                let conn = ws.main().connection();
                let stored: String = conn.query_row(
                    "SELECT dangerousContentSettings FROM chat_settings WHERE userId = ?1",
                    [USER_A],
                    |r| r.get(0),
                )?;
                let mut bag: Value = serde_json::from_str(&stored).unwrap();
                for (k, v) in patch.as_object().unwrap() {
                    bag[k] = v.clone();
                }
                conn.execute(
                    "UPDATE chat_settings SET dangerousContentSettings = ?1 WHERE userId = ?2",
                    rusqlite::params![bag.to_string(), USER_A],
                )?;
                Ok(())
            })
            .expect("patch the danger bag");
        }
        if c.drop_lantern {
            db.write_blocking(|ws| {
                ws.main().connection().execute(
                    "DELETE FROM instance_settings WHERE key = 'lanternBackgroundsMountPointId'",
                    [],
                )?;
                Ok(())
            })
            .expect("drop the Lantern pointer");
        }

        let provider_calls = Arc::new(Mutex::new(Vec::new()));
        let classify_calls = Arc::new(Mutex::new(Vec::new()));
        let seams = ImagesGenerateSeams {
            provider: ErasedImageGenerate::new(RecordingImageProvider {
                mode: c.provider,
                calls: provider_calls.clone(),
            }),
            classifier: ErasedImagePromptClassifier::new(RecordingClassifier {
                canned: c.classify.clone(),
                calls: classify_calls.clone(),
            }),
            codec: Arc::new(PrefixCodec),
        };

        let resp = rt.block_on(quilltap_core::api::images::images_generate(
            &db,
            &seams,
            USER_A,
            spec.frozen_now_ms,
            c.prompt.as_ref(),
            c.profile_id.as_ref(),
            c.tags.as_ref(),
            c.options.as_ref(),
        ));

        let (status, body) = match resp {
            Response::Images(v) => (201u16, v),
            Response::Error(e) => (status_of(e.kind), json!({ "error": e.message })),
            other => {
                failed.push(format!("{}: unexpected response variant {other:?}", c.name));
                continue;
            }
        };

        let Some(want) = oracle.get(c.name) else {
            failed.push(format!("{}: MISSING from the oracle", c.name));
            continue;
        };
        let want_status = want["status"].as_u64().unwrap_or(0) as u16;
        if status != want_status {
            failed.push(format!(
                "{}: status {status} != {want_status} (body {body})",
                c.name
            ));
            continue;
        }

        let transcoded = c.provider == ProviderMode::Png;
        let mut want_body = canon(&drop_zod_details(&want["body"]));
        let mut got_body = canon(&body);
        blank_receipt(&mut want_body, transcoded);
        blank_receipt(&mut got_body, transcoded);
        if got_body != want_body {
            failed.push(format!(
                "{}: body\n  want {want_body}\n  got  {got_body}",
                c.name
            ));
            continue;
        }
        let (mut wk, mut gk) = (Vec::new(), Vec::new());
        key_paths(&want_body, "$".into(), &mut wk);
        key_paths(&got_body, "$".into(), &mut gk);
        if wk != gk {
            failed.push(format!(
                "{}: KEY ORDER\n  want {wk:?}\n  got  {gk:?}",
                c.name
            ));
            continue;
        }

        let got_provider = canon(&Value::Array(provider_calls.lock().unwrap().clone()));
        let want_provider = canon(&want["providerCalls"]);
        if got_provider != want_provider {
            failed.push(format!(
                "{}: providerCalls\n  want {want_provider}\n  got  {got_provider}",
                c.name
            ));
            continue;
        }

        let got_classify = canon(&Value::Array(classify_calls.lock().unwrap().clone()));
        let want_classify = canon(&want["classifyCalls"]);
        if got_classify != want_classify {
            failed.push(format!(
                "{}: classifyCalls\n  want {want_classify}\n  got  {got_classify}",
                c.name
            ));
            continue;
        }

        let mut got_tables = canon(&dump_tables(&db));
        let mut want_tables = canon(&want["tables"]);
        blank_minted(&mut got_tables, transcoded);
        blank_minted(&mut want_tables, transcoded);
        if got_tables != want_tables {
            failed.push(format!(
                "{}: tables\n  want {want_tables}\n  got  {got_tables}",
                c.name
            ));
        }
    }

    assert!(
        failed.is_empty(),
        "images generate FAILED ({}):\n{}",
        failed.len(),
        failed.join("\n\n")
    );

    // Coverage by SHAPE, both directions (`harness-corpus-shape-constants-rot`).
    let mut driven_sorted = driven;
    driven_sorted.sort();
    let mut oracle_names: Vec<String> = oracle.keys().cloned().collect();
    oracle_names.sort();
    assert_eq!(
        driven_sorted, oracle_names,
        "the driven case list and the oracle's disagree"
    );
    assert!(
        driven_sorted.len() >= 37,
        "the corpus shrank to {} cases — a stale oracle would go green on a subset",
        driven_sorted.len()
    );
}

/// v4's middleware answers an uncaught `ZodError` with `{error: 'Validation
/// error', details: [...issues]}`; the `details` array is the standing
/// project-wide deferral, so v5 carries the sentence alone.
fn drop_zod_details(want_body: &Value) -> Value {
    let Some(o) = want_body.as_object() else {
        return want_body.clone();
    };
    if o.get("error").and_then(Value::as_str) != Some("Validation error") {
        return want_body.clone();
    }
    json!({ "error": "Validation error" })
}
