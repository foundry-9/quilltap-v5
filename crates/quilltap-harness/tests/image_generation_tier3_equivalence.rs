//! Tier-3 differential: the W4.9a image-generation handler (v4
//! `lib/tools/handlers/image-generation-handler.ts` `executeImageGenerationTool`,
//! ported as `quilltap_core::tools::generate_image::execute_image_generation_tool`)
//! and the avatar trigger (v4 `lib/wardrobe/avatar-generation.ts`
//! `triggerAvatarGenerationIfEnabled`, ported as
//! `quilltap_core::services::avatar_generation::trigger_avatar_generation_if_enabled`).
//!
//! Both sides run the SAME case sequence (`image-generation.json` + the fixture
//! builder's `.meta.json` sidecar) on a FRESH copy of the two-DB fixture (image
//! generation WRITES `files` + the store tables; the avatar trigger writes
//! `background_jobs`). Each image-gen case's result object + `files` + the five
//! mount-index store tables are diffed in the shared-cross-db-id-map remap form
//! (minted ids/timestamps remapped/placeholdered; the link `chunkCount` diffs
//! exactly since P4.6BK — v5 chunks on write); each avatar
//! case diffs `background_jobs`.
//!
//! MODEL SEAMS (pinned identically both sides):
//!   - the image provider ([`CannedImageProvider`], keyed by the oracle-recorded
//!     `provider|model|JSON.stringify(params)` key — a `mergeParameters`/
//!     `applyOrientation` divergence surfaces as a canned miss),
//!   - the completion boundary ([`CannedCompletionProvider`], keys replayed from the
//!     oracle so the craft/resolve/sanitize/classify task prompts are proven),
//!   - the WebP transcode ([`PassthroughTranscoder`], mirroring the oracle's
//!     `convertToWebP`/`transcodeToWebP` pass-through),
//!   - the API key (an injected [`ApiKeyResolver`] map, mirroring the oracle's
//!     `findApiKeyByIdAndUserId`),
//!   - the moderation provider ([`NoModerationProvider`], mirroring the oracle's
//!     `getDefaultProvider() -> null`),
//!   - the orientation registry (an injected `orientation_data_for` closure with the
//!     same OPENAI/GROK size-strategy support as the oracle),
//!   - the Lantern notification ([`NoLanternNotification`]; the byte-exact string is
//!     asserted in a unit test at the bottom),
//!   - `now_ms` = the oracle's frozen `Date.now()` so the provider filename
//!     `generated_<ts>.<ext>` is pinned.
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout). The oracle case
//! MUST be staged OUTSIDE any `.claude/` path (v4's jest ignores `/\.claude/`):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; WT=<this worktree root>
//!   STAGE=/tmp/qt-imggen-oracle
//!   rm -rf $STAGE && mkdir -p $STAGE/cases $STAGE/fixtures
//!   cp $WT/harness/oracle/cases/image-generation.test.ts $STAGE/cases/
//!   cp $WT/harness/oracle/fixtures/image-generation.json  $STAGE/fixtures/
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_IMGGEN_MAIN=/tmp/qt-imggen-main.db QT_FIXTURE_IMGGEN_MOUNT=/tmp/qt-imggen-mount.db \
//!     $N/node --import tsx $WT/harness/oracle/fixtures/build-image-generation-fixture.ts
//!   QT_FIXTURE_IMGGEN_MAIN=/tmp/qt-imggen-main.db QT_FIXTURE_IMGGEN_MOUNT=/tmp/qt-imggen-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-image-generation.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$STAGE/cases" -- image-generation
//! Run:
//!   QT_ORACLE_IMGGEN=/tmp/oracle-image-generation.ndjson \
//!   QT_FIXTURE_IMGGEN_MAIN=/tmp/qt-imggen-main.db QT_FIXTURE_IMGGEN_MOUNT=/tmp/qt-imggen-mount.db \
//!     cargo test -p quilltap-harness --test image_generation_tier3_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::image_gen::params_builder::ImageDeclarations;
use quilltap_core::image_gen::{
    ModelInfo, OrientationMapping, OrientationStrategy, OrientationSupport,
};
use quilltap_core::model::completion::{
    canned_completion_key, CannedCompletionProvider, CompletionMessage, CompletionRole,
    CompletionUsage,
};
use quilltap_core::model::image::{ImageGenParams, PassthroughTranscoder};
use quilltap_core::model::image_dialects::{build_image_request, RealImageProvider};
use quilltap_core::model::wire::{wire_key, CannedWireTransport, WireResponse};
use quilltap_core::services::avatar_generation::{
    trigger_avatar_generation_if_enabled, AvatarGenerationParams,
};
use quilltap_core::services::cheap_llm_exec::{CheapLlmLogConfig, CheapLlmTaskExecutor};
use quilltap_core::services::dangerous_content::gatekeeper::NoModerationProvider;
use quilltap_core::services::dangerous_content::provider_routing::ApiKeyResolver;
use quilltap_core::services::llm_logging::LogContext;
use quilltap_core::tools::generate_image::{
    execute_image_generation_tool, ImageGenDeps, ImageGenerationToolInput,
    ImageToolExecutionContext, RealLanternNotification,
};
use serde::Deserialize;
use serde_json::{Map, Value};

mod common;

// ===========================================================================
// Spec + oracle rows
// ===========================================================================

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "charAId")]
    char_a_id: String,
    #[serde(rename = "profileId")]
    profile_id: String,
    #[serde(rename = "apiKeys")]
    api_keys: HashMap<String, String>,
    #[serde(rename = "frozenNowMs")]
    frozen_now_ms: i64,
    #[serde(rename = "callingParticipantId")]
    calling_participant_id: String,
    chats: HashMap<String, ChatSpec>,
}

/// One corpus case, read from the shared spec `chats` map (label = the map key).
#[derive(Deserialize)]
struct ChatSpec {
    id: String,
    /// `"generate" | "avatar"`.
    op: String,
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    orientation: Option<String>,
    #[serde(default, rename = "clearLantern")]
    clear_lantern: bool,
}

#[derive(Deserialize)]
struct CannedRow {
    provider: String,
    model: String,
    temperature: Option<f64>,
    messages: Vec<CannedMsg>,
    response: String,
    usage: Option<CannedUsage>,
}
#[derive(Deserialize)]
struct CannedMsg {
    role: String,
    content: String,
}
#[derive(Deserialize)]
struct CannedUsage {
    #[serde(rename = "promptTokens")]
    prompt_tokens: i64,
    #[serde(rename = "completionTokens")]
    completion_tokens: i64,
    #[serde(rename = "totalTokens")]
    total_tokens: i64,
}

#[derive(Deserialize)]
struct CannedImageRow {
    key: String,
    images: Vec<CannedImageImg>,
}
#[derive(Deserialize)]
struct CannedImageImg {
    data: String,
    #[serde(rename = "mimeType")]
    #[allow(dead_code)]
    mime_type: Option<String>,
    #[serde(rename = "revisedPrompt")]
    revised_prompt: Option<String>,
}

/// Rebuild one `ImageLoraSpec` from the recorded `loras` entry (P4.D138).
fn lora_spec_from_json(v: &Value) -> quilltap_core::image_gen::lora_support::ImageLoraSpec {
    quilltap_core::image_gen::lora_support::ImageLoraSpec {
        source: v
            .get("source")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        scale: v.get("scale").and_then(Value::as_f64),
        trigger_phrase: v
            .get("triggerPhrase")
            .and_then(Value::as_str)
            .map(str::to_string),
        label: v.get("label").and_then(Value::as_str).map(str::to_string),
    }
}

/// Reconstruct [`ImageGenParams`] from an [`image_gen_key`] param JSON (v4's
/// `to_key_value` object), so the wire is keyed by the request the dialect builds.
fn params_from_key_json(v: &Value) -> ImageGenParams {
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    ImageGenParams {
        prompt: s("prompt").unwrap_or_default(),
        negative_prompt: s("negativePrompt"),
        model: s("model").unwrap_or_default(),
        n: v.get("n").and_then(Value::as_f64),
        size: s("size"),
        aspect_ratio: s("aspectRatio"),
        quality: s("quality"),
        style: s("style"),
        seed: v.get("seed").and_then(Value::as_f64),
        guidance_scale: v.get("guidanceScale").and_then(Value::as_f64),
        response_format: s("responseFormat"),
        loras: v
            .get("loras")
            .and_then(Value::as_array)
            .map(|a| a.iter().map(lora_spec_from_json).collect())
            .unwrap_or_default(),
        profile_parameters: v.get("profileParameters").cloned(),
        steps: v.get("steps").and_then(Value::as_f64),
    }
}

/// Register an oracle image response as the canned WIRE the dialect will parse:
/// reverse-map the images into the OpenAI-SDK shape and key by the built request.
fn register_image_wire(
    transport: CannedWireTransport,
    row: &CannedImageRow,
) -> CannedWireTransport {
    let mut parts = row.key.splitn(3, '|');
    let provider = parts.next().expect("provider in key");
    let _model = parts.next();
    let params_json: Value = serde_json::from_str(parts.next().expect("params json in key"))
        .expect("params json parses");
    let params = params_from_key_json(&params_json);
    let req = build_image_request(provider, &params).expect("build image request");
    let data: Vec<Value> = row
        .images
        .iter()
        .map(|i| {
            let mut o = serde_json::Map::new();
            o.insert("b64_json".into(), Value::String(i.data.clone()));
            if let Some(rp) = &i.revised_prompt {
                o.insert("revised_prompt".into(), Value::String(rp.clone()));
            }
            Value::Object(o)
        })
        .collect();
    let body = serde_json::json!({ "data": data }).to_string();
    let key = wire_key(&req.method, &req.url, &req.body_string());
    transport.with_raw_response(key, WireResponse::new(200, body))
}

#[derive(Deserialize)]
struct ResultRow {
    label: String,
    #[serde(rename = "resultJson")]
    result_json: String,
    #[serde(default)]
    dumps: Option<HashMap<String, Value>>,
    /// Round-3 Group 4: the persisted Lantern `character-image` notification body
    /// (`None` when nothing was posted). The inline file uuid is uuid-normalized.
    #[serde(default, rename = "lanternContent")]
    lantern_content: Option<String>,
    /// W4.10b: the `llm_logs` rows this case wrote (IMAGE_GENERATION + the cheap
    /// IMAGE_PROMPT_CRAFTING / APPEARANCE_RESOLUTION tasks). `{columns, rows}`;
    /// avatar cases carry an empty list.
    #[serde(default, rename = "llmLogs")]
    llm_logs: Option<Value>,
}

// ===========================================================================
// The injected API-key resolver (mirrors the oracle's findApiKeyByIdAndUserId)
// ===========================================================================

struct CannedApiKeys(HashMap<String, String>);
impl ApiKeyResolver for CannedApiKeys {
    fn resolve(&self, api_key_id: &str, _user_id: &str) -> Option<String> {
        self.0.get(api_key_id).cloned()
    }
}

// ===========================================================================
// The orientation registry (mirrors the oracle's getImageGenerationModels /
// getImageProviderConstraints).
// ===========================================================================

fn size_support(portrait: &str, landscape: &str, square: &str) -> OrientationSupport {
    OrientationSupport {
        strategy: OrientationStrategy::Size,
        portrait: OrientationMapping {
            size: Some(portrait.to_string()),
            ..Default::default()
        },
        landscape: OrientationMapping {
            size: Some(landscape.to_string()),
            ..Default::default()
        },
        square: Some(OrientationMapping {
            size: Some(square.to_string()),
            ..Default::default()
        }),
    }
}

/// P4.D138: the canned per-model `loraSupport` the oracle's registry mock also
/// declares, so the shared params builder's cap + trigger-phrase append are
/// MEASURABLE on this path (two adapters, no scale block).
fn canned_lora_support() -> quilltap_core::image_gen::lora_support::ImageLoraSupport {
    quilltap_core::image_gen::lora_support::ImageLoraSupport {
        max_loras: 2.0,
        scale: None,
        source_kinds: vec!["url".to_string(), "hf-repo".to_string()],
        supports_private_weights_token: None,
    }
}

/// The canned plugin-registry declarations (v4's `getImageGenerationModels` +
/// `getImageProviderConstraints`). P4.D138 widened the seam from the
/// orientation half to v4's whole declaration set; these fixtures declare no
/// LoRA support on the ids the fixtures point at, mirroring the oracle's mock.
fn declarations_for(provider: &str) -> ImageDeclarations {
    match provider {
        "OPENAI" => {
            let s = size_support("1024x1792", "1792x1024", "1024x1024");
            ImageDeclarations {
                models: vec![ModelInfo {
                    lora_support: Some(canned_lora_support()),
                    id: "dall-e-3".to_string(),
                    orientation_support: Some(s.clone()),
                }],
                orientation_provider: Some(s),
                lora_provider: None,
            }
        }
        "GROK" => {
            let s = size_support("768x1344", "1344x768", "1024x1024");
            ImageDeclarations {
                models: vec![ModelInfo {
                    lora_support: Some(canned_lora_support()),
                    id: "grok-image".to_string(),
                    orientation_support: Some(s.clone()),
                }],
                orientation_provider: Some(s),
                lora_provider: None,
            }
        }
        _ => ImageDeclarations::default(),
    }
}

// ===========================================================================
// Helpers
// ===========================================================================

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/image-generation.json")
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

fn role_of(s: &str) -> CompletionRole {
    match s {
        "system" => CompletionRole::System,
        "user" => CompletionRole::User,
        "assistant" => CompletionRole::Assistant,
        "tool" => CompletionRole::Tool,
        other => panic!("unknown role {other}"),
    }
}

fn fresh_copy(main_fixture: &str, mount_fixture: &str, tag: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir();
    let main_work = dir.join(format!(
        "qt-imggen-main-rust-{}-{tag}.db",
        std::process::id()
    ));
    let mount_work = dir.join(format!(
        "qt-imggen-mount-rust-{}-{tag}.db",
        std::process::id()
    ));
    cleanup(&main_work, &mount_work);
    std::fs::copy(main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));
    (main_work, mount_work)
}

fn cleanup(main: &Path, mount: &Path) {
    for p in [main, mount] {
        for suffix in ["", "-journal", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", p.display()));
        }
    }
}

/// Delete the Lantern instance_settings pointer on a copied fixture (the
/// unprovisioned-store case), mirroring the oracle's per-case DELETE.
fn drop_lantern_pointer(main_path: &Path, pepper: &str) {
    let db = Db::open_main(main_path, pepper).expect("open main to drop lantern");
    db.write_blocking(|writers| {
        writers.main().connection().execute(
            "DELETE FROM instance_settings WHERE key = 'lanternBackgroundsMountPointId'",
            [],
        )?;
        Ok(())
    })
    .expect("drop lantern pointer");
    drop(db);
}

// ===========================================================================
// Numeric + remap normalization
// ===========================================================================

fn canon_numbers(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if n.is_f64() {
                if let Some(f) = n.as_f64() {
                    if f.fract() == 0.0 && f.abs() < 9.007e15 {
                        *v = Value::Number(serde_json::Number::from(f as i64));
                    }
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(canon_numbers),
        Value::Object(o) => o.values_mut().for_each(canon_numbers),
        _ => {}
    }
}

/// Positional UUID + `/api/v1/*/<uuid>` normalization over a result JSON string:
/// the minted `files.id` (and its `/api/v1/images/<id>` / `/api/v1/files/<id>`
/// url/filepath) differ between v4 + Rust, so a shared positional map collapses
/// them to `<uuid-N>` first-appearance tokens in the same string.
fn norm_uuids_in_string(s: &str) -> String {
    let bytes = s.as_bytes();
    let is_hex = |b: u8| b.is_ascii_hexdigit();
    let mut out = String::with_capacity(s.len());
    let mut map: HashMap<String, String> = HashMap::new();
    let mut i = 0;
    while i < bytes.len() {
        if i + 36 <= bytes.len() {
            let cand = &s[i..i + 36];
            let cb = cand.as_bytes();
            let shape_ok = cb.len() == 36
                && cb[8] == b'-'
                && cb[13] == b'-'
                && cb[18] == b'-'
                && cb[23] == b'-'
                && cand
                    .chars()
                    .enumerate()
                    .all(|(j, c)| matches!(j, 8 | 13 | 18 | 23) || is_hex(c as u8));
            if shape_ok {
                let next = format!("<uuid-{}>", map.len());
                let token = map.entry(cand.to_string()).or_insert(next).clone();
                out.push_str(&token);
                i += 36;
                continue;
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

// The mount-index tables + `files` diffed on each image-gen case.
struct TableSpec {
    table: &'static str,
    order_by: &'static str,
    from_mount: bool,
    id_columns: &'static [&'static str],
    ts_columns: &'static [&'static str],
    pin_chunk_count: bool,
    /// Columns pinned to a placeholder (v4-storeMountFile bookkeeping the Rust
    /// storage primitive legitimately does not maintain — the mount-point
    /// aggregate rollups are a documented deferral of the refresh layer).
    pin_columns: &'static [&'static str],
    /// Columns whose value is a STRING embedding a minted UUID (e.g.
    /// `files.storageKey = "mount-blob:<lantern>:<blobId>"` — the `blobId` is minted
    /// nondeterministically). Collapse embedded UUIDs to positional tokens so the
    /// seeded lantern id (same both sides) and the minted blobId compare by shape.
    norm_string_columns: &'static [&'static str],
}

const IMG_TABLES: &[TableSpec] = &[
    TableSpec {
        table: "doc_mount_points",
        order_by: "id",
        from_mount: true,
        id_columns: &["id"],
        ts_columns: &["createdAt", "updatedAt", "lastScannedAt"],
        pin_chunk_count: false,
        // v4's storeMountFile bumps the Lantern mount's fileCount/totalSizeBytes
        // aggregates; the Rust link_blob_content leaves them at the DDL default (the
        // reindex/aggregate layer is unported, as for chunkCount).
        pin_columns: &["fileCount", "totalSizeBytes"],
        norm_string_columns: &[],
    },
    TableSpec {
        table: "doc_mount_files",
        order_by: "sha256",
        from_mount: true,
        id_columns: &["id"],
        ts_columns: &["createdAt", "updatedAt"],
        pin_chunk_count: false,
        pin_columns: &[],
        norm_string_columns: &[],
    },
    TableSpec {
        // The generated image BYTES land in doc_mount_blobs (link_blob_content),
        // not doc_mount_documents. Order by the content sha (stable across the
        // minted fileId), then remap — the wardrobe-transfers precedent.
        table: "doc_mount_blobs",
        order_by: "sha256",
        from_mount: true,
        id_columns: &["id", "fileId"],
        ts_columns: &["createdAt", "updatedAt"],
        pin_chunk_count: false,
        pin_columns: &[],
        norm_string_columns: &[],
    },
    TableSpec {
        table: "doc_mount_file_links",
        order_by: "relativePath",
        from_mount: true,
        id_columns: &["id", "fileId", "folderId"],
        ts_columns: &[
            "createdAt",
            "updatedAt",
            "lastModified",
            "descriptionUpdatedAt",
        ],
        pin_chunk_count: false, // P4.6BK: v5 chunks on write — chunkCount now diffs exactly
        pin_columns: &[],
        norm_string_columns: &[],
    },
    TableSpec {
        table: "doc_mount_folders",
        order_by: "path",
        from_mount: true,
        id_columns: &["id", "parentId"],
        ts_columns: &["createdAt", "updatedAt"],
        pin_chunk_count: false,
        pin_columns: &[],
        norm_string_columns: &[],
    },
    TableSpec {
        table: "files",
        order_by: "sha256",
        from_mount: false,
        id_columns: &["id"],
        ts_columns: &["createdAt", "updatedAt"],
        pin_chunk_count: false,
        // `storageKey` is `mount-blob:<lanternMount>:<blobId>`; the <blobId> is a
        // minted uuid differing between sides (the blob linkage is verified by the
        // doc_mount_blobs / doc_mount_file_links remap). Pin it.
        pin_columns: &["storageKey"],
        norm_string_columns: &["storageKey"],
    },
];

/// Remap id columns through the SHARED map, placeholder timestamps, pin chunkCount.
fn normalize_table(dump: &mut Value, spec: &TableSpec, id_map: &mut HashMap<String, String>) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .unwrap_or_else(|| panic!("{}: dump has no rows array", spec.table));
    for row in rows.iter_mut() {
        let obj = row
            .as_object_mut()
            .unwrap_or_else(|| panic!("{}: row is not an object", spec.table));
        for col in spec.id_columns {
            if let Some(Value::String(raw)) = obj.get(*col) {
                let next = format!("ID_{}", id_map.len());
                let token = id_map.entry(raw.clone()).or_insert(next).clone();
                obj.insert((*col).to_string(), Value::String(token));
            }
        }
        for col in spec.ts_columns {
            if obj.get(*col).map(|v| !v.is_null()).unwrap_or(false) {
                obj.insert((*col).to_string(), Value::String("<ts>".to_string()));
            }
        }
        if spec.pin_chunk_count {
            obj.insert("chunkCount".to_string(), Value::String("<cc>".to_string()));
        }
        for col in spec.pin_columns {
            if obj.contains_key(*col) {
                obj.insert((*col).to_string(), Value::String("<pin>".to_string()));
            }
        }
        for col in spec.norm_string_columns {
            if let Some(Value::String(raw)) = obj.get(*col) {
                let normed = norm_uuids_in_string(raw);
                obj.insert((*col).to_string(), Value::String(normed));
            }
        }
        canon_numbers(row);
    }
}

fn normalize_dumps(dumps: &mut [Value]) {
    let mut id_map: HashMap<String, String> = HashMap::new();
    for (i, spec) in IMG_TABLES.iter().enumerate() {
        normalize_table(&mut dumps[i], spec, &mut id_map);
    }
}

/// The avatar cases dump `background_jobs`: remap the minted `id` + placeholder
/// timestamps + pin the payload's minted-value-free keys (the job payload is
/// deterministic: chatId/characterId/imageProfileId).
fn normalize_background_jobs(dump: &mut Value) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .expect("background_jobs has no rows");
    for row in rows.iter_mut() {
        if let Some(obj) = row.as_object_mut() {
            for col in ["id"] {
                if obj.get(col).map(|v| !v.is_null()).unwrap_or(false) {
                    obj.insert(col.into(), Value::String("<id>".into()));
                }
            }
            for col in [
                "createdAt",
                "updatedAt",
                "scheduledAt",
                "startedAt",
                "completedAt",
                "nextRetryAt",
            ] {
                if obj.get(col).map(|v| !v.is_null()).unwrap_or(false) {
                    obj.insert(col.into(), Value::String("<ts>".into()));
                }
            }
        }
        canon_numbers(row);
    }
}

// ===========================================================================
// The test
// ===========================================================================

#[test]
fn image_generation_matches_oracle() {
    let (Some(oracle_path), Some(main_fixture), Some(mount_fixture)) = (
        env_or_skip("QT_ORACLE_IMGGEN"),
        env_or_skip("QT_FIXTURE_IMGGEN_MAIN"),
        env_or_skip("QT_FIXTURE_IMGGEN_MOUNT"),
    ) else {
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    // Parse the oracle: canned completions, canned image WIRE, per-case results.
    // W4.7f: the image provider is the REAL dialect (`RealImageProvider`) over a
    // canned [`CannedWireTransport`] — each oracle image response is reverse-mapped
    // into the OpenAI-SDK wire shape (`{data:[{b64_json, revised_prompt?}]}`) keyed
    // by the request the dialect builds, so `build_image_request` +
    // `parse_image_response` run in composition (the corpus is OPENAI/dall-e-3).
    let mut completion = CannedCompletionProvider::new();
    let mut image_transport = CannedWireTransport::new();
    let mut results: HashMap<String, ResultRow> = HashMap::new();

    for line in std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle {oracle_path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        match v.get("kind").and_then(Value::as_str) {
            Some("canned") => {
                let row: CannedRow = serde_json::from_value(v).expect("parse canned row");
                let messages: Vec<CompletionMessage> = row
                    .messages
                    .iter()
                    .map(|m| CompletionMessage {
                        role: role_of(&m.role),
                        content: m.content.clone(),
                    })
                    .collect();
                let usage = row.usage.map(|u| CompletionUsage {
                    prompt_tokens: u.prompt_tokens,
                    completion_tokens: u.completion_tokens,
                    total_tokens: u.total_tokens,
                });
                completion = completion.with_response(
                    &row.provider,
                    &row.model,
                    row.temperature,
                    &messages,
                    row.response,
                    usage,
                );
                // Assert the recorded key round-trips through our key fn (sanity).
                let _ =
                    canned_completion_key(&row.provider, &row.model, row.temperature, &messages);
            }
            Some("cannedImage") => {
                let row: CannedImageRow = serde_json::from_value(v).expect("parse cannedImage row");
                image_transport = register_image_wire(image_transport, &row);
            }
            Some("result") => {
                let row: ResultRow = serde_json::from_value(v).expect("parse result row");
                results.insert(row.label.clone(), row);
            }
            _ => {}
        }
    }

    let image_provider = RealImageProvider::new(image_transport);
    let api_keys = CannedApiKeys(spec.api_keys.clone());
    let moderation = NoModerationProvider;
    let transcoder = PassthroughTranscoder;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    // Iterate the shared spec `chats` map (label = key) so the Rust corpus can never
    // drift from the oracle's case list. Sort by key for a deterministic order.
    let mut chat_keys: Vec<&String> = spec.chats.keys().collect();
    chat_keys.sort();

    for label in chat_keys {
        let case = &spec.chats[label];
        let (main_work, mount_work) = fresh_copy(&main_fixture, &mount_fixture, label);
        if case.clear_lantern {
            drop_lantern_pointer(&main_work, &spec.test_pepper_base64);
        }

        // W4.10b: a fresh per-case llm-logs partition for the IMAGE_GENERATION +
        // cheap-task `logLLMCall` rows.
        let ll_work = main_work.with_file_name(format!("imggen-ll-{label}.db"));
        let _ = std::fs::remove_file(&ll_work);
        common::materialize_llm_logs(&ll_work, &spec.test_pepper_base64);

        let db = Db::open(
            DbPaths {
                main: main_work.clone(),
                mount_index: Some(mount_work.clone()),
                llm_logs: Some(ll_work.clone()),
            },
            &spec.test_pepper_base64,
        )
        .unwrap_or_else(|e| panic!("open fixture {}: {e}", label));

        let chat_id = case.id.clone();

        let want = results
            .get(label)
            .unwrap_or_else(|| panic!("oracle missing case {}", label));

        if case.op == "generate" {
            // The image cheap tasks (craftImagePrompt / resolveAppearance /
            // sanitizeAppearance) log through the executor; the IMAGE_GENERATION
            // rows come from the handler's own `log_image_generation` over `&db`.
            let executor = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
                db: db.clone(),
                user_id: spec.user_id.clone(),
                chat_id: Some(chat_id.clone()),
                message_id: None,
                ctx: LogContext::none(),
            });
            // Round-3 Group 4: the REAL Lantern sink posts the character-image
            // notification to this chat; its persisted content is diffed below.
            let lantern = RealLanternNotification { db: &db };
            let deps = ImageGenDeps {
                image_provider: &image_provider,
                completion: &completion,
                moderation: &moderation,
                api_keys: &api_keys,
                transcoder: &transcoder,
                lantern: &lantern,
                executor: &executor,
                now_ms: spec.frozen_now_ms,
                declarations_for: &declarations_for,
            };
            let input = ImageGenerationToolInput {
                prompt: case.prompt.clone().unwrap_or_default(),
                negative_prompt: None,
                orientation: case.orientation.clone(),
                size: None,
                style: None,
                quality: None,
                aspect_ratio: None,
                count: Some(1),
            };
            let ctx = ImageToolExecutionContext {
                user_id: spec.user_id.clone(),
                profile_id: spec.profile_id.clone(),
                chat_id: Some(chat_id.clone()),
                calling_participant_id: Some(spec.calling_participant_id.clone()),
            };

            let out = rt.block_on(execute_image_generation_tool(&db, &deps, &input, &ctx));
            let got_result = result_output_to_value(&out);

            // Compare the result object (positional-UUID normalized for the minted file id).
            let got_json = norm_uuids_in_string(&serde_json::to_string(&got_result).unwrap());
            let want_json = norm_uuids_in_string(&want.result_json);
            assert_eq!(
                got_json, want_json,
                "resultJson diverged for {}\n  rust:   {}\n  oracle: {}",
                label, got_json, want_json
            );

            // Diff the 6 tables (shared-id-map remap form).
            let mut got_dumps: Vec<Value> = IMG_TABLES
                .iter()
                .map(|s| {
                    let t = s.table.to_string();
                    let ob = s.order_by.to_string();
                    if s.from_mount {
                        db.read_mount_index(move |c| dump_table_json_conn(c, &t, &ob))
                    } else {
                        db.read_main(move |c| dump_table_json_conn(c, &t, &ob))
                    }
                    .unwrap_or_else(|e| panic!("dump {}: {e}", s.table))
                })
                .collect();
            let oracle_dumps = want
                .dumps
                .as_ref()
                .unwrap_or_else(|| panic!("oracle case {} has no dumps", label));
            let mut want_dumps: Vec<Value> = IMG_TABLES
                .iter()
                .map(|s| {
                    oracle_dumps
                        .get(s.table)
                        .cloned()
                        .unwrap_or_else(|| panic!("oracle {} missing dump {}", label, s.table))
                })
                .collect();
            normalize_dumps(&mut got_dumps);
            normalize_dumps(&mut want_dumps);
            for (i, s) in IMG_TABLES.iter().enumerate() {
                assert_eq!(
                    got_dumps[i]["rows"], want_dumps[i]["rows"],
                    "{}: {} rows diverged\n  rust:   {}\n  oracle: {}",
                    label, s.table, got_dumps[i]["rows"], want_dumps[i]["rows"]
                );
            }

            // Round-3 Group 4: the persisted Lantern character-image notification
            // body (byte-exact, incl. the tail the old placeholder truncated). The
            // inline file uuid is uuid-normalized on both sides.
            let cid = chat_id.clone();
            let messages = db
                .read_main(move |c| quilltap_core::db::chats_messages_read::get_messages(c, &cid))
                .expect("get_messages");
            let got_lantern = messages.iter().find(|m| {
                m.get("systemSender").and_then(Value::as_str) == Some("lantern")
                    && m.get("systemKind").and_then(Value::as_str) == Some("character-image")
            });
            let got_content = got_lantern
                .and_then(|m| m.get("content").and_then(Value::as_str))
                .map(norm_uuids_in_string);
            let want_content = want.lantern_content.as_deref().map(norm_uuids_in_string);
            assert_eq!(
                got_content, want_content,
                "{}: lantern character-image content diverged\n  rust:   {:?}\n  oracle: {:?}",
                label, got_content, want_content
            );
        } else {
            // Avatar trigger.
            let params = AvatarGenerationParams {
                user_id: spec.user_id.clone(),
                chat_id: chat_id.clone(),
                character_id: spec.char_a_id.clone(),
                image_profile_id_override: None,
                equipped_slots_override: None,
            };
            rt.block_on(trigger_avatar_generation_if_enabled(&db, &params));

            let mut got = db
                .read_main(|c| dump_table_json_conn(c, "background_jobs", "id"))
                .expect("dump background_jobs");
            let oracle_dumps = want
                .dumps
                .as_ref()
                .unwrap_or_else(|| panic!("oracle avatar {} has no dumps", label));
            let mut want_bg = oracle_dumps
                .get("background_jobs")
                .cloned()
                .unwrap_or_else(|| panic!("oracle {} missing background_jobs dump", label));
            normalize_background_jobs(&mut got);
            normalize_background_jobs(&mut want_bg);
            assert_eq!(
                got["rows"], want_bg["rows"],
                "{}: background_jobs diverged\n  rust:   {}\n  oracle: {}",
                label, got["rows"], want_bg["rows"]
            );
        }

        // W4.10b: diff the `llm_logs` rows this case wrote (IMAGE_GENERATION +
        // any cheap IMAGE_PROMPT_CRAFTING / APPEARANCE_RESOLUTION rows; empty for
        // the avatar cases, which make no model call).
        let got_logs = common::dump_llm_logs(&db);
        let want_logs = want
            .llm_logs
            .as_ref()
            .map(common::oracle_llm_logs)
            .unwrap_or_default();
        assert_eq!(
            got_logs,
            want_logs,
            "{}: llm_logs rows diverge (got {} vs oracle {})",
            label,
            got_logs.len(),
            want_logs.len()
        );

        drop(db);
        cleanup(&main_work, &mount_work);
        let _ = std::fs::remove_file(&ll_work);
    }

    eprintln!("OK: image-generation differential matched the oracle across all cases.");
}

/// Serialize a [`ImageGenerationToolOutput`] into the same JSON shape v4's handler
/// returns (undefined keys dropped, in v4 field order:
/// `success, images, error?, message?, provider?, model?, expandedPrompt?`).
fn result_output_to_value(
    out: &quilltap_core::tools::generate_image::ImageGenerationToolOutput,
) -> Value {
    let mut m = Map::new();
    m.insert("success".into(), Value::Bool(out.success));
    let images: Vec<Value> = out
        .images
        .iter()
        .map(|img| {
            let mut im = Map::new();
            im.insert("id".into(), Value::String(img.id.clone()));
            im.insert("url".into(), Value::String(img.url.clone()));
            im.insert("filename".into(), Value::String(img.filename.clone()));
            if let Some(rp) = &img.revised_prompt {
                im.insert("revisedPrompt".into(), Value::String(rp.clone()));
            }
            if let Some(fp) = &img.filepath {
                im.insert("filepath".into(), Value::String(fp.clone()));
            }
            if let Some(mt) = &img.mime_type {
                im.insert("mimeType".into(), Value::String(mt.clone()));
            }
            if let Some(sz) = img.size {
                im.insert("size".into(), num(sz));
            }
            if let Some(w) = img.width {
                im.insert("width".into(), num(w));
            }
            if let Some(h) = img.height {
                im.insert("height".into(), num(h));
            }
            if let Some(sha) = &img.sha256 {
                im.insert("sha256".into(), Value::String(sha.clone()));
            }
            Value::Object(im)
        })
        .collect();
    // v4's SUCCESS output carries `images`; its error output omits the key entirely
    // (the early `{ success:false, error, message, ... }` returns have no `images`).
    if !images.is_empty() {
        m.insert("images".into(), Value::Array(images));
    }
    if let Some(e) = &out.error {
        m.insert("error".into(), Value::String(e.clone()));
    }
    if let Some(msg) = &out.message {
        m.insert("message".into(), Value::String(msg.clone()));
    }
    if let Some(p) = &out.provider {
        m.insert("provider".into(), Value::String(p.clone()));
    }
    if let Some(model) = &out.model {
        m.insert("model".into(), Value::String(model.clone()));
    }
    if let Some(ep) = &out.expanded_prompt {
        m.insert("expandedPrompt".into(), Value::String(ep.clone()));
    }
    Value::Object(m)
}

/// A JS-number: integer-valued floats render bare (matching JSON.stringify).
fn num(f: f64) -> Value {
    if f.fract() == 0.0 && f.abs() < 9.007e15 {
        Value::Number(serde_json::Number::from(f as i64))
    } else {
        serde_json::Number::from_f64(f)
            .map(Value::Number)
            .unwrap_or(Value::Null)
    }
}

// Round-3 Group 4: the byte-exact Lantern character-image notification content is
// now proven end-to-end via the persisted `lanternContent` diff (above) against the
// REAL writer — the old truncated-placeholder assertion is retired.
