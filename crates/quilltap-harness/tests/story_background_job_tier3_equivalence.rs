//! Tier-3 differential: the W4.9c STORY_BACKGROUND_GENERATION job handler (v4
//! `lib/background-jobs/handlers/story-background.ts` `handleStoryBackgroundGeneration`,
//! ported as `quilltap_core::services::story_background_job::handle_story_background_generation`).
//!
//! Both sides run the SAME case sequence (`story-background-job.json`) on a FRESH
//! copy of the two-DB fixture. Each case's per-op result, the five mount-index store
//! tables + `files` (shared-cross-db-id-map remap form), the chat's
//! `storyBackgroundImageId` / `lastBackgroundGeneratedAt`, the project's
//! `storyBackgroundImageId` (the latest_chat properties RMW), and the Lantern
//! notification body are diffed byte-for-byte.
//!
//! MODEL SEAMS (pinned identically both sides): the completion boundary
//! ([`CannedCompletionProvider`], recorded keys prove the derive/resolve/craft task
//! prompts + the cheap-vs-uncensored retry selection), the image provider
//! ([`CannedImageProvider`], keyed in `to_key_value` order; `[]` images drive the
//! no-images no-op), the WebP transcode ([`PassthroughTranscoder`]), the API key, the
//! orientation registry (OPENAI + GROK), the project FsSeam ([`RecordingProjectUpload`],
//! deterministic like the oracle), and `now_ms` = the frozen `Date.now()`.
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout). See the oracle
//! header. Run:
//!   QT_ORACLE_STORY=/tmp/oracle-story-background-job.ndjson \
//!   QT_FIXTURE_STORY_MAIN=/tmp/qt-story-main.db QT_FIXTURE_STORY_MOUNT=/tmp/qt-story-mount.db \
//!     cargo test -p quilltap-harness --test story_background_job_tier3_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::image_gen::{
    ModelInfo, OrientationMapping, OrientationStrategy, OrientationSupport,
};
use quilltap_core::model::completion::{
    CannedCompletionProvider, CompletionMessage, CompletionRole, CompletionUsage,
};
use quilltap_core::model::image::{
    CannedImageProvider, GeneratedImageData, ImageGenResponse, PassthroughTranscoder,
};
use quilltap_core::services::cheap_llm_exec::{CheapLlmLogConfig, CheapLlmTaskExecutor};
use quilltap_core::services::dangerous_content::gatekeeper::NoModerationProvider;
use quilltap_core::services::dangerous_content::provider_routing::ApiKeyResolver;
use quilltap_core::services::image_job_common::{ProjectImageUpload, ProjectUploadResult};
use quilltap_core::services::job_runner::{HandlerRegistry, JobRunner};
use quilltap_core::services::llm_logging::LogContext;
use quilltap_core::services::story_background_job::{
    handle_story_background_generation, StoryBackgroundGenerationHandler, StoryBackgroundPayload,
    StoryJobDeps,
};
use serde::Deserialize;
use serde_json::Value;

mod common;

// ===========================================================================
// Spec + oracle rows
// ===========================================================================

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "frozenNowMs")]
    frozen_now_ms: i64,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "apiKeys")]
    api_keys: HashMap<String, String>,
    chats: HashMap<String, ChatSpec>,
}

#[derive(Deserialize, Clone)]
struct ChatSpec {
    id: String,
    #[serde(rename = "characterIds")]
    character_ids: Vec<String>,
    #[serde(rename = "imageProfileId")]
    image_profile_id: String,
    #[serde(default, rename = "projectId")]
    project_id: Option<String>,
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
    mime_type: Option<String>,
    #[serde(rename = "revisedPrompt")]
    revised_prompt: Option<String>,
}

#[derive(Deserialize)]
struct ResultRow {
    label: String,
    threw: Option<String>,
    #[serde(default)]
    dumps: Option<HashMap<String, Value>>,
    #[serde(default, rename = "storyImageId")]
    story_image_id: Option<String>,
    #[serde(default, rename = "lastBackgroundAt")]
    last_background_at: Option<String>,
    #[serde(default, rename = "projectStoryImageId")]
    project_story_image_id: Option<String>,
    #[serde(default, rename = "lanternContent")]
    lantern_content: Option<String>,
    #[serde(default, rename = "lanternOpaque")]
    lantern_opaque: Option<String>,
    /// W4.10b: the `llm_logs` rows this case wrote (SUMMARIZATION [derive-scene] +
    /// IMAGE_PROMPT_CRAFTING [craft] + IMAGE_GENERATION; empty for a skipped case).
    #[serde(default, rename = "llmLogs")]
    llm_logs: Option<Value>,
}

// ===========================================================================
// Seams
// ===========================================================================

#[derive(Clone)]
struct CannedApiKeys(HashMap<String, String>);
impl ApiKeyResolver for CannedApiKeys {
    fn resolve(&self, api_key_id: &str, _user_id: &str) -> Option<String> {
        self.0.get(api_key_id).cloned()
    }
}

/// Deterministic project upload seam, mirroring the oracle's `uploadFile` mock:
/// `storageKey = project-upload:<projectId>:<filename>`.
#[derive(Clone, Copy)]
struct RecordingProjectUpload;
impl ProjectImageUpload for RecordingProjectUpload {
    async fn upload(
        &self,
        filename: &str,
        content: &[u8],
        content_type: &str,
        project_id: &str,
        _folder_path: &str,
    ) -> Result<ProjectUploadResult, String> {
        Ok(ProjectUploadResult {
            storage_key: format!("project-upload:{project_id}:{filename}"),
            stored_mime_type: content_type.to_string(),
            size_bytes: content.len(),
        })
    }
}

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

fn orientation_data_for(provider: &str) -> (Vec<ModelInfo>, Option<OrientationSupport>) {
    match provider {
        "OPENAI" => {
            let s = size_support("1024x1792", "1792x1024", "1024x1024");
            (
                vec![ModelInfo {
                    id: "dall-e-3".to_string(),
                    orientation_support: Some(s.clone()),
                }],
                Some(s),
            )
        }
        "GROK" => {
            let s = size_support("768x1344", "1344x768", "1024x1024");
            (
                vec![ModelInfo {
                    id: "grok-2-image".to_string(),
                    orientation_support: Some(s.clone()),
                }],
                Some(s),
            )
        }
        _ => (Vec::new(), None),
    }
}

// ===========================================================================
// Helpers
// ===========================================================================

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/story-background-job.json")
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
        "qt-story-main-rust-{}-{tag}.db",
        std::process::id()
    ));
    let mount_work = dir.join(format!(
        "qt-story-mount-rust-{}-{tag}.db",
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

// ===========================================================================
// Normalization
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

fn norm_uuids_in_string(s: &str) -> String {
    let bytes = s.as_bytes();
    let is_hex = |b: u8| b.is_ascii_hexdigit();
    let mut out = String::with_capacity(s.len());
    let mut map: HashMap<String, String> = HashMap::new();
    let mut i = 0;
    while i < bytes.len() {
        if i + 36 <= bytes.len() && s.is_char_boundary(i) && s.is_char_boundary(i + 36) {
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

struct TableSpec {
    table: &'static str,
    order_by: &'static str,
    from_mount: bool,
    id_columns: &'static [&'static str],
    ts_columns: &'static [&'static str],
    pin_chunk_count: bool,
    pin_columns: &'static [&'static str],
    norm_string_columns: &'static [&'static str],
}

const STORY_TABLES: &[TableSpec] = &[
    TableSpec {
        table: "doc_mount_points",
        order_by: "id",
        from_mount: true,
        id_columns: &["id"],
        ts_columns: &["createdAt", "updatedAt", "lastScannedAt"],
        pin_chunk_count: false,
        pin_columns: &["fileCount", "chunkCount", "totalSizeBytes"],
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
        // storageKey = mount-blob:<lantern>:<blobId> (or project-upload:<pid>:<file>);
        // linkedTo = [chatId, ...characterIds, <notification msgId>].
        pin_columns: &[],
        norm_string_columns: &["storageKey", "linkedTo"],
    },
    TableSpec {
        // The legacy `folders` find-or-create on the project storage branch.
        table: "folders",
        order_by: "path",
        from_mount: false,
        id_columns: &["id", "parentFolderId"],
        ts_columns: &["createdAt", "updatedAt"],
        pin_chunk_count: false,
        pin_columns: &[],
        norm_string_columns: &[],
    },
];

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

fn normalize_selected(dumps: &mut [Value], specs: &[&TableSpec]) {
    let mut id_map: HashMap<String, String> = HashMap::new();
    for (i, spec) in specs.iter().enumerate() {
        normalize_table(&mut dumps[i], spec, &mut id_map);
    }
}

fn norm_opt(s: &Option<String>) -> Option<String> {
    s.as_deref().map(norm_uuids_in_string)
}

// ===========================================================================
// Load oracle
// ===========================================================================

fn load_oracle(
    path: &str,
) -> (
    CannedCompletionProvider,
    CannedImageProvider,
    HashMap<String, ResultRow>,
) {
    let mut completion = CannedCompletionProvider::new();
    let mut image_provider = CannedImageProvider::new();
    let mut results: HashMap<String, ResultRow> = HashMap::new();
    for line in std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read oracle {path}: {e}"))
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
            }
            Some("cannedImage") => {
                let row: CannedImageRow = serde_json::from_value(v).expect("parse cannedImage row");
                let images: Vec<GeneratedImageData> = row
                    .images
                    .iter()
                    .map(|i| GeneratedImageData {
                        data: Some(i.data.clone()),
                        url: None,
                        mime_type: i.mime_type.clone(),
                        revised_prompt: i.revised_prompt.clone(),
                    })
                    .collect();
                image_provider = image_provider.with_raw_key(row.key, ImageGenResponse { images });
            }
            Some("result") => {
                let row: ResultRow = serde_json::from_value(v).expect("parse result row");
                results.insert(row.label.clone(), row);
            }
            _ => {}
        }
    }
    (completion, image_provider, results)
}

/// Read the project's overlaid `storyBackgroundImageId` (spans both DBs).
fn read_project_story_image(db: &Db, project_id: &str) -> Option<String> {
    let pid = project_id.to_string();
    db.read_main(move |main| {
        db.read_mount_index(|mount| {
            let repo = quilltap_core::db::projects::ProjectsRepository::new(main, mount);
            let img = repo
                .find_by_id(&pid)
                .map_err(|e| quilltap_core::db::DbError::Key(format!("project read: {e:?}")))?
                .and_then(|p| {
                    p.get("storyBackgroundImageId")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                });
            Ok(img)
        })
    })
    .ok()
    .flatten()
}

// ===========================================================================
// The test
// ===========================================================================

#[test]
fn story_background_job_matches_oracle() {
    let (Some(oracle_path), Some(main_fixture), Some(mount_fixture)) = (
        env_or_skip("QT_ORACLE_STORY"),
        env_or_skip("QT_FIXTURE_STORY_MAIN"),
        env_or_skip("QT_FIXTURE_STORY_MOUNT"),
    ) else {
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let (completion, image_provider, results) = load_oracle(&oracle_path);
    let api_keys = CannedApiKeys(spec.api_keys.clone());
    let moderation = NoModerationProvider;
    let transcoder = PassthroughTranscoder;
    let upload = RecordingProjectUpload;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let mut chat_keys: Vec<&String> = spec.chats.keys().collect();
    chat_keys.sort();

    for label in chat_keys {
        let case = &spec.chats[label];
        let (main_work, mount_work) = fresh_copy(&main_fixture, &mount_fixture, label);

        // W4.10b: a fresh per-case llm-logs partition for the IMAGE_GENERATION +
        // cheap-task (SUMMARIZATION / IMAGE_PROMPT_CRAFTING) rows.
        let ll_work = main_work.with_file_name(format!("story-ll-{label}.db"));
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

        let want = results
            .get(label)
            .unwrap_or_else(|| panic!("oracle missing case {}", label));

        // The cheap tasks (derive-scene-context / craft-story-background-prompt /
        // resolve-appearance) log through the executor; the IMAGE_GENERATION rows
        // come from `generate_with_reroute` over `&db`.
        let executor = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
            db: db.clone(),
            user_id: spec.user_id.clone(),
            chat_id: Some(case.id.clone()),
            message_id: None,
            ctx: LogContext::none(),
        });
        let deps = StoryJobDeps {
            image_provider: &image_provider,
            completion: &completion,
            moderation: &moderation,
            api_keys: &api_keys,
            transcoder: &transcoder,
            upload: &upload,
            executor: &executor,
            now_ms: spec.frozen_now_ms,
            orientation_data_for: &orientation_data_for,
        };
        let payload = StoryBackgroundPayload {
            chat_id: case.id.clone(),
            image_profile_id: case.image_profile_id.clone(),
            character_ids: case.character_ids.clone(),
            scene_context: None,
            project_id: case.project_id.clone(),
        };

        let outcome = rt.block_on(handle_story_background_generation(
            &db,
            &deps,
            &spec.user_id,
            &payload,
        ));
        let got_threw: Option<String> = outcome.err();
        assert_eq!(
            got_threw, want.threw,
            "{}: threw diverged\n  rust:   {:?}\n  oracle: {:?}",
            label, got_threw, want.threw
        );

        // Diff the store tables + `files`/`folders`. For a PROJECT-scoped case the
        // generated image lands via the FsSeam upload (NOT a store blob), and the
        // only mount-index change is the project's `properties.json`
        // storyBackgroundImageId RMW — a content-addressed file whose content embeds
        // the minted fileId (a different sha reorders the sha-sorted dump), verified
        // instead by the `projectStoryImageId` SELECT below + the projects_tier2
        // differential. So skip the five mount tables for project cases; the main-DB
        // `files` (FsSeam storageKey) + `folders` (legacy find-or-create) still diff.
        let skip_mount = case.project_id.is_some();
        let oracle_dumps = want
            .dumps
            .as_ref()
            .unwrap_or_else(|| panic!("oracle case {} has no dumps", label));
        let selected: Vec<&TableSpec> = STORY_TABLES
            .iter()
            .filter(|s| !(skip_mount && s.from_mount))
            .collect();
        let mut got_dumps: Vec<Value> = selected
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
        let mut want_dumps: Vec<Value> = selected
            .iter()
            .map(|s| {
                oracle_dumps
                    .get(s.table)
                    .cloned()
                    .unwrap_or_else(|| panic!("oracle {} missing dump {}", label, s.table))
            })
            .collect();
        normalize_selected(&mut got_dumps, &selected);
        normalize_selected(&mut want_dumps, &selected);
        for (i, s) in selected.iter().enumerate() {
            assert_eq!(
                got_dumps[i]["rows"], want_dumps[i]["rows"],
                "{}: {} rows diverged\n  rust:   {}\n  oracle: {}",
                label, s.table, got_dumps[i]["rows"], want_dumps[i]["rows"]
            );
        }

        // chats.storyBackgroundImageId + lastBackgroundGeneratedAt.
        let cid = case.id.clone();
        let (got_img, got_at): (Option<String>, Option<String>) = db
            .read_main(move |c| {
                Ok(c.query_row(
                    "SELECT storyBackgroundImageId, lastBackgroundGeneratedAt FROM chats WHERE id = ?1",
                    [&cid],
                    |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?)),
                )?)
            })
            .expect("read chat story cols");
        assert_eq!(
            norm_opt(&got_img),
            norm_opt(&want.story_image_id),
            "{}: storyBackgroundImageId diverged",
            label
        );
        // lastBackgroundGeneratedAt is the frozen now (iso_from_unix_ms) both sides.
        assert_eq!(
            got_at, want.last_background_at,
            "{}: lastBackgroundGeneratedAt diverged\n  rust:   {:?}\n  oracle: {:?}",
            label, got_at, want.last_background_at
        );

        // project storyBackgroundImageId (the latest_chat properties RMW).
        let got_proj = case
            .project_id
            .as_deref()
            .and_then(|pid| read_project_story_image(&db, pid));
        assert_eq!(
            norm_opt(&got_proj),
            norm_opt(&want.project_story_image_id),
            "{}: project storyBackgroundImageId diverged\n  rust:   {:?}\n  oracle: {:?}",
            label,
            norm_opt(&got_proj),
            norm_opt(&want.project_story_image_id),
        );

        // The Lantern background notification (sender lantern).
        let cid = case.id.clone();
        let messages = db
            .read_main(move |c| quilltap_core::db::chats_messages_read::get_messages(c, &cid))
            .expect("get_messages");
        let got_lantern = messages.iter().find(|m| {
            m.get("systemSender").and_then(Value::as_str) == Some("lantern")
                && m.get("systemKind").and_then(Value::as_str) == Some("background")
        });
        let got_content = got_lantern
            .and_then(|m| m.get("content").and_then(Value::as_str))
            .map(str::to_string);
        let got_opaque = got_lantern
            .and_then(|m| m.get("opaqueContent").and_then(Value::as_str))
            .map(str::to_string);
        assert_eq!(
            norm_opt(&got_content),
            norm_opt(&want.lantern_content),
            "{}: lantern content diverged\n  rust:   {:?}\n  oracle: {:?}",
            label,
            norm_opt(&got_content),
            norm_opt(&want.lantern_content),
        );
        assert_eq!(
            norm_opt(&got_opaque),
            norm_opt(&want.lantern_opaque),
            "{}: lantern opaque diverged",
            label
        );

        // W4.10b: diff the `llm_logs` rows (cheap SUMMARIZATION / IMAGE_PROMPT_CRAFTING
        // + IMAGE_GENERATION; empty for a skipped case).
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

    eprintln!("OK: story-background-job differential matched the oracle across all cases.");
}

// ===========================================================================
// Runner-registration E2E (W4.8 pattern): enqueue → claim → dispatch → COMPLETED
// ===========================================================================

#[test]
fn story_background_job_runner_registration_e2e() {
    let (Some(oracle_path), Some(main_fixture), Some(mount_fixture)) = (
        env_or_skip("QT_ORACLE_STORY"),
        env_or_skip("QT_FIXTURE_STORY_MAIN"),
        env_or_skip("QT_FIXTURE_STORY_MOUNT"),
    ) else {
        return;
    };
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let (completion, image_provider, _results) = load_oracle(&oracle_path);
    let case = &spec.chats["stale_derive"];

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let (main_work, mount_work) = fresh_copy(&main_fixture, &mount_fixture, "e2e");
    let db = Db::open(
        DbPaths {
            main: main_work.clone(),
            mount_index: Some(mount_work.clone()),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open fixture");

    let job_id = rt
        .block_on(
            quilltap_core::services::queue_service::enqueue_story_background_generation(
                &db,
                &spec.user_id,
                &case.id,
                &case.image_profile_id,
                &case.character_ids,
                None,
                None,
            ),
        )
        .expect("enqueue")
        .0;

    let handler = StoryBackgroundGenerationHandler {
        image_provider,
        completion,
        moderation: NoModerationProvider,
        api_keys: CannedApiKeys(spec.api_keys.clone()),
        transcoder: PassthroughTranscoder,
        upload: RecordingProjectUpload,
        executor: CheapLlmTaskExecutor::new(),
        now_ms: spec.frozen_now_ms,
        orientation_data_for,
    };
    let mut reg = HandlerRegistry::new();
    reg.register("STORY_BACKGROUND_GENERATION", Box::new(handler));
    let runner = JobRunner::new(db.clone(), reg);

    let out = rt.block_on(runner.pump_claim());
    assert_eq!(out.dispatched, 1, "runner should dispatch exactly one job");

    let status: String = db
        .read_main(move |c| {
            Ok(c.query_row(
                "SELECT status FROM background_jobs WHERE id = ?1",
                [&job_id],
                |r| r.get::<_, String>(0),
            )?)
        })
        .expect("read status");
    assert_eq!(
        status, "COMPLETED",
        "the story-background job should complete"
    );

    drop(db);
    cleanup(&main_work, &mount_work);
    eprintln!("OK: story-background-job runner-registration E2E completed.");
}
