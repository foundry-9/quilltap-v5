//! Tier-3 differential: the W4.9c CHARACTER_AVATAR_GENERATION job handler (v4
//! `lib/background-jobs/handlers/character-avatar.ts` `handleCharacterAvatarGeneration`,
//! ported as `quilltap_core::services::character_avatar_job::handle_character_avatar_generation`).
//!
//! Both sides run the SAME case sequence (`avatar-job.json`) on a FRESH copy of the
//! two-DB fixture (the handler WRITES the character vault store tables + `files` +
//! `chats` + `characters` + a `chat_messages` Lantern notification row). Each case's
//! per-op result (threw / ok), the five mount-index store tables + `files` (in the
//! shared-cross-db-id-map remap form: minted ids remapped, timestamps placeholdered,
//! `chunkCount` + the mount aggregates pinned — the reindex/aggregate layer is
//! unported, per the groups/projects precedent), the Lantern notification body, and
//! the `characterAvatars` / `avatarOverrides` JSON updates are diffed byte-for-byte.
//!
//! MODEL SEAMS (pinned identically both sides): the image provider
//! ([`CannedImageProvider`], keyed by the oracle-recorded key in `to_key_value` field
//! order; a moderation reroute is driven by an oracle-recorded FAILURE key), the WebP
//! transcode ([`PassthroughTranscoder`]), the API key (an injected [`ApiKeyResolver`]
//! map), the moderation provider ([`NoModerationProvider`]), the orientation registry
//! (an OPENAI size-strategy closure), and `now_ms` = the oracle's frozen `Date.now()`
//! so the provider filename + the frozen `characterAvatars.generatedAt` are pinned.
//! Avatars fire NO completion calls (an empty [`CannedCompletionProvider`] is passed,
//! never consulted).
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout; stage OUTSIDE any
//! `.claude/` path). See the oracle header for the exact recipe. Run:
//!   QT_ORACLE_AVATAR=/tmp/oracle-avatar-job.ndjson \
//!   QT_FIXTURE_AVATAR_MAIN=/tmp/qt-avatar-main.db QT_FIXTURE_AVATAR_MOUNT=/tmp/qt-avatar-mount.db \
//!     cargo test -p quilltap-harness --test avatar_job_tier3_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::image_gen::{
    ModelInfo, OrientationMapping, OrientationStrategy, OrientationSupport,
};
use quilltap_core::model::completion::CannedCompletionProvider;
use quilltap_core::model::image::{
    CannedImageProvider, GeneratedImageData, ImageGenResponse, PassthroughTranscoder,
};
use quilltap_core::services::character_avatar_job::{
    handle_character_avatar_generation, AvatarJobDeps, CharacterAvatarGenerationHandler,
    CharacterAvatarPayload,
};
use quilltap_core::services::dangerous_content::gatekeeper::NoModerationProvider;
use quilltap_core::services::dangerous_content::provider_routing::ApiKeyResolver;
use quilltap_core::services::image_job_common::NoProjectImageUpload;
use quilltap_core::services::job_runner::{HandlerRegistry, JobRunner};
use serde::Deserialize;
use serde_json::Value;

// ===========================================================================
// Spec + oracle rows
// ===========================================================================

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "frozenNowMs")]
    frozen_now_ms: i64,
    #[serde(rename = "apiKeys")]
    api_keys: HashMap<String, String>,
    chats: HashMap<String, ChatSpec>,
}

#[derive(Deserialize)]
struct ChatSpec {
    id: String,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "characterId")]
    character_id: String,
    #[serde(rename = "imageProfileId")]
    image_profile_id: String,
    #[serde(default, rename = "equippedSlotsOverride")]
    equipped_slots_override: Option<Value>,
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
struct CannedImageFailureRow {
    key: String,
    message: String,
}

#[derive(Deserialize)]
struct ResultRow {
    label: String,
    threw: Option<String>,
    #[serde(default)]
    dumps: Option<HashMap<String, Value>>,
    #[serde(default, rename = "lanternContent")]
    lantern_content: Option<String>,
    #[serde(default, rename = "lanternOpaque")]
    lantern_opaque: Option<String>,
    #[serde(default, rename = "characterAvatars")]
    character_avatars: Option<String>,
    #[serde(default, rename = "avatarOverrides")]
    avatar_overrides: Option<String>,
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
        _ => (Vec::new(), None),
    }
}

// ===========================================================================
// Helpers
// ===========================================================================

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/avatar-job.json")
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

fn fresh_copy(main_fixture: &str, mount_fixture: &str, tag: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir();
    let main_work = dir.join(format!(
        "qt-avatar-main-rust-{}-{tag}.db",
        std::process::id()
    ));
    let mount_work = dir.join(format!(
        "qt-avatar-mount-rust-{}-{tag}.db",
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

/// Positional UUID normalization over a string (JSON columns embedding minted ids):
/// each distinct uuid → a first-appearance `<uuid-N>` token, per string.
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

const AVATAR_TABLES: &[TableSpec] = &[
    TableSpec {
        table: "doc_mount_points",
        order_by: "id",
        from_mount: true,
        id_columns: &["id"],
        ts_columns: &["createdAt", "updatedAt", "lastScannedAt"],
        pin_chunk_count: false,
        // v4's reindex/storeMountFile maintains the mount aggregates
        // (fileCount/chunkCount/totalSizeBytes); the Rust storage primitive leaves
        // them at the baked/DDL value (the reindex/aggregate layer is unported, the
        // groups/projects/image-generation precedent).
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
        pin_chunk_count: true,
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
        // storageKey = mount-blob:<vault>:<blobId>; <blobId> minted → normalize.
        // linkedTo = [chatId, characterId, <notification msgId>]; msgId minted.
        pin_columns: &[],
        norm_string_columns: &["storageKey", "linkedTo"],
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

fn normalize_dumps(dumps: &mut [Value]) {
    let mut id_map: HashMap<String, String> = HashMap::new();
    for (i, spec) in AVATAR_TABLES.iter().enumerate() {
        normalize_table(&mut dumps[i], spec, &mut id_map);
    }
}

fn norm_opt(s: &Option<String>) -> Option<String> {
    s.as_deref().map(norm_uuids_in_string)
}

// ===========================================================================
// Build the canned providers from the oracle
// ===========================================================================

fn load_oracle(path: &str) -> (CannedImageProvider, HashMap<String, ResultRow>) {
    let mut image_provider = CannedImageProvider::new();
    let mut results: HashMap<String, ResultRow> = HashMap::new();
    for line in std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read oracle {path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        match v.get("kind").and_then(Value::as_str) {
            Some("cannedImage") => {
                let row: CannedImageRow = serde_json::from_value(v).expect("parse cannedImage row");
                let images: Vec<GeneratedImageData> = row
                    .images
                    .iter()
                    .map(|i| GeneratedImageData {
                        data: i.data.clone(),
                        mime_type: i.mime_type.clone(),
                        revised_prompt: i.revised_prompt.clone(),
                    })
                    .collect();
                image_provider = image_provider.with_raw_key(row.key, ImageGenResponse { images });
            }
            Some("cannedImageFailure") => {
                let row: CannedImageFailureRow =
                    serde_json::from_value(v).expect("parse cannedImageFailure row");
                image_provider = image_provider.with_raw_failure(row.key, row.message);
            }
            Some("result") => {
                let row: ResultRow = serde_json::from_value(v).expect("parse result row");
                results.insert(row.label.clone(), row);
            }
            _ => {}
        }
    }
    (image_provider, results)
}

// ===========================================================================
// The test
// ===========================================================================

#[test]
fn avatar_job_matches_oracle() {
    let (Some(oracle_path), Some(main_fixture), Some(mount_fixture)) = (
        env_or_skip("QT_ORACLE_AVATAR"),
        env_or_skip("QT_FIXTURE_AVATAR_MAIN"),
        env_or_skip("QT_FIXTURE_AVATAR_MOUNT"),
    ) else {
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let (image_provider, results) = load_oracle(&oracle_path);
    let completion = CannedCompletionProvider::new();
    let api_keys = CannedApiKeys(spec.api_keys.clone());
    let moderation = NoModerationProvider;
    let transcoder = PassthroughTranscoder;
    let upload = NoProjectImageUpload;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let mut chat_keys: Vec<&String> = spec.chats.keys().collect();
    chat_keys.sort();

    for label in chat_keys {
        let case = &spec.chats[label];
        let (main_work, mount_work) = fresh_copy(&main_fixture, &mount_fixture, label);

        let db = Db::open(
            DbPaths {
                main: main_work.clone(),
                mount_index: Some(mount_work.clone()),
                llm_logs: None,
            },
            &spec.test_pepper_base64,
        )
        .unwrap_or_else(|e| panic!("open fixture {}: {e}", label));

        let want = results
            .get(label)
            .unwrap_or_else(|| panic!("oracle missing case {}", label));

        let deps = AvatarJobDeps {
            image_provider: &image_provider,
            completion: &completion,
            moderation: &moderation,
            api_keys: &api_keys,
            transcoder: &transcoder,
            upload: &upload,
            now_ms: spec.frozen_now_ms,
            orientation_data_for: &orientation_data_for,
        };
        let payload = CharacterAvatarPayload {
            chat_id: case.id.clone(),
            character_id: case.character_id.clone(),
            image_profile_id: case.image_profile_id.clone(),
            equipped_slots_override: case.equipped_slots_override.clone(),
        };

        let outcome = rt.block_on(handle_character_avatar_generation(
            &db,
            &deps,
            &case.user_id,
            &payload,
        ));
        let got_threw: Option<String> = outcome.err();
        assert_eq!(
            got_threw, want.threw,
            "{}: threw diverged\n  rust:   {:?}\n  oracle: {:?}",
            label, got_threw, want.threw
        );

        // Diff the 6 tables (shared-id-map remap form).
        let mut got_dumps: Vec<Value> = AVATAR_TABLES
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
        let mut want_dumps: Vec<Value> = AVATAR_TABLES
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
        for (i, s) in AVATAR_TABLES.iter().enumerate() {
            assert_eq!(
                got_dumps[i]["rows"], want_dumps[i]["rows"],
                "{}: {} rows diverged\n  rust:   {}\n  oracle: {}",
                label, s.table, got_dumps[i]["rows"], want_dumps[i]["rows"]
            );
        }

        // The Lantern avatar notification (sender aurora), byte-exact (uuid-normed).
        let cid = case.id.clone();
        let messages = db
            .read_main(move |c| quilltap_core::db::chats_messages_read::get_messages(c, &cid))
            .expect("get_messages");
        let got_lantern = messages.iter().find(|m| {
            m.get("systemSender").and_then(Value::as_str) == Some("aurora")
                && m.get("systemKind").and_then(Value::as_str) == Some("avatar")
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
            "{}: lantern content diverged",
            label
        );
        assert_eq!(
            norm_opt(&got_opaque),
            norm_opt(&want.lantern_opaque),
            "{}: lantern opaque diverged",
            label
        );

        // characterAvatars + avatarOverrides JSON updates.
        let cid = case.id.clone();
        let got_avatars: Option<String> = db
            .read_main(move |c| {
                Ok(c.query_row(
                    "SELECT characterAvatars FROM chats WHERE id = ?1",
                    [&cid],
                    |r| r.get::<_, Option<String>>(0),
                )?)
            })
            .expect("read characterAvatars");
        assert_eq!(
            norm_opt(&got_avatars),
            norm_opt(&want.character_avatars),
            "{}: characterAvatars diverged\n  rust:   {:?}\n  oracle: {:?}",
            label,
            norm_opt(&got_avatars),
            norm_opt(&want.character_avatars),
        );
        let chid = case.character_id.clone();
        let got_overrides: Option<String> = db
            .read_main(move |c| {
                Ok(c.query_row(
                    "SELECT avatarOverrides FROM characters WHERE id = ?1",
                    [&chid],
                    |r| r.get::<_, Option<String>>(0),
                )?)
            })
            .expect("read avatarOverrides");
        assert_eq!(
            norm_opt(&got_overrides),
            norm_opt(&want.avatar_overrides),
            "{}: avatarOverrides diverged\n  rust:   {:?}\n  oracle: {:?}",
            label,
            norm_opt(&got_overrides),
            norm_opt(&want.avatar_overrides),
        );

        drop(db);
        cleanup(&main_work, &mount_work);
    }

    eprintln!("OK: avatar-job differential matched the oracle across all cases.");
}

// ===========================================================================
// Runner-registration E2E (W4.8 pattern): enqueue → claim → dispatch → COMPLETED
// ===========================================================================

/// A single canned-seam `CharacterAvatarGenerationHandler` registered in a
/// `HandlerRegistry`; the runner claims the enqueued job and dispatches it to the
/// REAL handler over the canned seams, marking it COMPLETED (removing the type from
/// the loud fallback in effect).
#[test]
fn avatar_job_runner_registration_e2e() {
    let (Some(oracle_path), Some(main_fixture), Some(mount_fixture)) = (
        env_or_skip("QT_ORACLE_AVATAR"),
        env_or_skip("QT_FIXTURE_AVATAR_MAIN"),
        env_or_skip("QT_FIXTURE_AVATAR_MOUNT"),
    ) else {
        return;
    };
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let (image_provider, _results) = load_oracle(&oracle_path);
    let case = &spec.chats["happy_vault"];

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

    // Enqueue a real CHARACTER_AVATAR_GENERATION job for the happy_vault chat.
    let job_id = rt
        .block_on(
            quilltap_core::services::queue_service::enqueue_character_avatar_generation(
                &db,
                &case.user_id,
                &case.id,
                &case.character_id,
                &case.image_profile_id,
                None,
            ),
        )
        .expect("enqueue")
        .0;

    let handler = CharacterAvatarGenerationHandler {
        image_provider,
        completion: CannedCompletionProvider::new(),
        moderation: NoModerationProvider,
        api_keys: CannedApiKeys(spec.api_keys.clone()),
        transcoder: PassthroughTranscoder,
        upload: NoProjectImageUpload,
        now_ms: spec.frozen_now_ms,
        orientation_data_for,
    };
    let mut reg = HandlerRegistry::new();
    reg.register("CHARACTER_AVATAR_GENERATION", Box::new(handler));
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
    assert_eq!(status, "COMPLETED", "the avatar job should complete");

    drop(db);
    cleanup(&main_work, &mount_work);
    eprintln!("OK: avatar-job runner-registration E2E completed.");
}
