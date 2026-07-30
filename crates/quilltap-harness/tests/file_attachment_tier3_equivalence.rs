//! Tier-3 differential: the chat file/attachment subsystem (W4.4b; v4
//! `lib/services/chat-message/context-builder.service.ts` `loadAndProcessFiles` +
//! `lib/chat/file-attachment-fallback.ts` `processFileAttachmentFallback` +
//! `lib/chat-files-v2.ts` `loadChatFilesForLLM`), ported as
//! `quilltap_core::services::chat_files::{load_and_process_files,
//! load_chat_files_for_llm}` + `quilltap_core::services::file_fallback::
//! process_file_attachment_fallback`.
//!
//! Both sides run the SAME op set over a copy of the two-DB fixture. Three
//! families:
//!   * `lap` — `loadAndProcessFiles`; compare `{ prefix, attachedFileIds,
//!     attachmentsToSend }`.
//!   * `fb` — `processFileAttachmentFallback`; compare the full `FallbackResult`
//!     (order-independent `serde_json::Value` equality).
//!   * `lcffl` — `loadChatFilesForLLM` on a Scriptorium mount link (the mount-path
//!     branch the Lantern K-seam relies on); compare the attachments.
//!
//! MODEL SEAMS (pinned identically both sides): the vision-description call is a
//! `CannedCompletionProvider` registered from the oracle's recorded `kind:"canned"`
//! rows (keyed by the exact provider|model|temperature|messages|attachments); the
//! image bytes come from a canned in-memory `FileBytesStore` keyed by fileId (the
//! fixture's per-fileId FSM bytes, mirrored by the oracle's `downloadFile` mock);
//! `logLLMCall` is a no-op both sides (no llm-logs partition opened → the Rust
//! `log_llm_call` write is inert, matching the oracle's mock) so the llm_logs table
//! is out of scope. No resize is exercised (small images), so the transcoder seam
//! (`NotConfiguredTranscoder`) is never consulted.
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout). The oracle case
//! MUST be staged OUTSIDE any `.claude/` path (v4's jest ignores `/\.claude/`):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5 ; TMPO=/tmp/qt-oracle-run
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_FILE_ATTACH_MAIN=/tmp/qt-fa-main.db QT_FIXTURE_FILE_ATTACH_MOUNT=/tmp/qt-fa-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/fixtures/build-file-attachment-fixture.ts
//!   mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp $V5/harness/oracle/cases/file-attachment-tier3.test.ts "$TMPO/cases/"
//!   cp $V5/harness/oracle/fixtures/file-attachment.json       "$TMPO/fixtures/"
//!   QT_FIXTURE_FILE_ATTACH_MAIN=/tmp/qt-fa-main.db QT_FIXTURE_FILE_ATTACH_MOUNT=/tmp/qt-fa-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-file-attachment.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- file-attachment-tier3
//! Run:
//!   QT_ORACLE_FILE_ATTACHMENT=/tmp/oracle-file-attachment.ndjson \
//!   QT_FIXTURE_FILE_ATTACH_MAIN=/tmp/qt-fa-main.db QT_FIXTURE_FILE_ATTACH_MOUNT=/tmp/qt-fa-mount.db \
//!     cargo test -p quilltap-harness --test file_attachment_tier3_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use base64::Engine;
use quilltap_core::db::files::FileEntry;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::files::image_processing::NotConfiguredTranscoder;
use quilltap_core::model::completion::{
    CannedCompletionProvider, CompletionAttachment, CompletionMessage, CompletionResponse,
    CompletionUsage,
};
use quilltap_core::services::chat_files::{
    load_and_process_files, load_chat_files_for_llm, FileBytesStore, LoadChatFilesOptions,
    ProcessFilesDeps,
};
use quilltap_core::services::file_fallback::{
    process_file_attachment_fallback, FallbackDeps, FallbackFile, IMAGE_DESCRIPTION_INSTRUCTION,
};
use serde::Deserialize;
use serde_json::{json, Value};

// ===========================================================================
// Spec + oracle rows
// ===========================================================================

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "chatId")]
    chat_id: String,
    #[serde(rename = "respProfiles")]
    resp_profiles: HashMap<String, Value>,
    files: Vec<FileSpec>,
}

#[derive(Deserialize, Clone)]
struct FileSpec {
    key: String,
    id: String,
    #[serde(rename = "originalFilename")]
    original_filename: String,
    #[serde(rename = "mimeType")]
    mime_type: String,
    #[serde(rename = "fsmBytes", default)]
    fsm_bytes: Option<Vec<u8>>,
    #[serde(rename = "fsmBytesUtf8", default)]
    fsm_bytes_utf8: Option<String>,
    #[serde(rename = "fsmMissing", default)]
    fsm_missing: Option<bool>,
}

impl FileSpec {
    /// The bytes the FSM download seam returns (`None` = a read failure).
    fn fsm(&self) -> Option<Vec<u8>> {
        if self.fsm_missing == Some(true) {
            return None;
        }
        if let Some(s) = &self.fsm_bytes_utf8 {
            return Some(s.as_bytes().to_vec());
        }
        Some(self.fsm_bytes.clone().unwrap_or_default())
    }

    /// The base64 payload a (B) `processFileAttachmentFallback` case feeds as the
    /// fileAttachment `data` (mirrors the oracle's `dataByKey`).
    fn data_base64(&self) -> String {
        match self.fsm() {
            Some(b) => base64::engine::general_purpose::STANDARD.encode(&b),
            None => String::new(),
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum OracleRow {
    #[serde(rename = "case")]
    Case {
        family: String,
        label: String,
        result: Value,
    },
    #[serde(rename = "canned")]
    Canned {
        provider: String,
        model: String,
        temperature: Option<f64>,
        filename: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
        content: String,
        #[serde(rename = "finishReason")]
        finish_reason: Option<String>,
        usage: Option<OracleUsage>,
    },
}

#[derive(Deserialize)]
struct OracleUsage {
    #[serde(rename = "promptTokens")]
    prompt_tokens: i64,
    #[serde(rename = "completionTokens")]
    completion_tokens: i64,
    #[serde(rename = "totalTokens")]
    total_tokens: i64,
}

// ===========================================================================
// The canned FSM byte store
// ===========================================================================

struct CannedBytes {
    by_file_id: HashMap<String, Option<Vec<u8>>>,
}

impl FileBytesStore for CannedBytes {
    fn download_file(&self, entry: &FileEntry) -> Result<Vec<u8>, String> {
        match self.by_file_id.get(&entry.id) {
            Some(Some(b)) => Ok(b.clone()),
            Some(None) => Err(format!("storage read failed for fileId {}", entry.id)),
            None => Err(format!("no canned bytes for fileId {}", entry.id)),
        }
    }
}

// ===========================================================================
// Helpers
// ===========================================================================

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/file-attachment.json")
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

fn fresh_copy(main_fixture: &str, mount_fixture: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir();
    let main_work = dir.join(format!("qt-fa-main-rust-{}.db", std::process::id()));
    let mount_work = dir.join(format!("qt-fa-mount-rust-{}.db", std::process::id()));
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
// The test
// ===========================================================================

#[test]
fn file_attachment_matches_oracle() {
    let (Some(oracle_path), Some(main_fixture), Some(mount_fixture)) = (
        env_or_skip("QT_ORACLE_FILE_ATTACHMENT"),
        env_or_skip("QT_FIXTURE_FILE_ATTACH_MAIN"),
        env_or_skip("QT_FIXTURE_FILE_ATTACH_MOUNT"),
    ) else {
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let file_by_key: HashMap<String, FileSpec> = spec
        .files
        .iter()
        .map(|f| (f.key.clone(), f.clone()))
        .collect();

    // Parse the oracle: canned rows → the completion provider; case rows by label.
    let mut cases: HashMap<String, (String, Value)> = HashMap::new();
    let mut provider = CannedCompletionProvider::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle {oracle_path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let row: OracleRow = serde_json::from_str(line).expect("oracle line parses");
        match row {
            OracleRow::Case {
                family,
                label,
                result,
            } => {
                cases.insert(label, (family, result));
            }
            OracleRow::Canned {
                provider: prov,
                model,
                temperature,
                filename,
                mime_type,
                content,
                finish_reason,
                usage,
            } => {
                let messages = vec![CompletionMessage::user(IMAGE_DESCRIPTION_INSTRUCTION)];
                let attachments = vec![CompletionAttachment {
                    // Off the canned key (P4.21) — the oracle's recorded keys carry no id.
                    id: String::new(),
                    filename,
                    mime_type,
                    data: String::new(),
                }];
                provider = provider.with_response_full(
                    &prov,
                    &model,
                    temperature,
                    &messages,
                    &attachments,
                    CompletionResponse {
                        content,
                        usage: usage.map(|u| CompletionUsage {
                            prompt_tokens: u.prompt_tokens,
                            completion_tokens: u.completion_tokens,
                            total_tokens: u.total_tokens,
                        }),
                        finish_reason,
                    },
                );
            }
        }
    }

    // The FSM byte store (fileId -> Option<bytes>).
    let mut by_file_id: HashMap<String, Option<Vec<u8>>> = HashMap::new();
    for f in &spec.files {
        by_file_id.insert(f.id.clone(), f.fsm());
    }
    let bytes = CannedBytes { by_file_id };
    let transcoder = NotConfiguredTranscoder;

    let (main_work, mount_work) = fresh_copy(&main_fixture, &mount_fixture);
    let db = Db::open(
        DbPaths {
            main: main_work.clone(),
            mount_index: Some(mount_work.clone()),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db");

    let deps = ProcessFilesDeps {
        db: &db,
        bytes: &bytes,
        transcoder: &transcoder,
        completion: &provider,
        user_id: &spec.user_id,
        now_ms: 0,
    };

    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");

    // ---- (A) loadAndProcessFiles ----
    let lap_cases: &[(&str, &str, &[&str])] = &[
        ("lap_text", "noImg", &["text"]),
        ("lap_image_kept", "imgOK", &["keptImage"]),
        ("lap_image_desc", "noImg", &["descImage"]),
        ("lap_refusal_retry", "noImg", &["refusalImage"]),
        ("lap_unsupported", "noImg", &["zip"]),
        ("lap_load_skip", "noImg", &["missing"]),
        ("lap_multi", "noImg", &["text", "zip"]),
    ];
    for (label, profile_key, file_keys) in lap_cases {
        let profile = &spec.resp_profiles[*profile_key];
        let file_ids: Vec<String> = file_keys
            .iter()
            .map(|k| file_by_key[*k].id.clone())
            .collect();
        let out = rt.block_on(load_and_process_files(
            &deps,
            &spec.chat_id,
            profile,
            &file_ids,
        ));
        let got = json!({
            "prefix": out.message_content_prefix.clone().unwrap_or_default(),
            "attachedFileIds": out.attached_file_ids,
            "attachmentsToSend": out.attachments_to_send,
        });
        let (_family, want) = cases
            .get(*label)
            .unwrap_or_else(|| panic!("oracle missing case {label}"));
        assert_eq!(&got, want, "lap case {label} diverged");
    }

    // ---- (B) processFileAttachmentFallback ----
    let fb_cases: &[(&str, &str, &str)] = &[
        ("fb_text", "noImg", "text"),
        ("fb_kept_raw", "imgOK", "keptImage"),
        ("fb_desc", "noImg", "descImage"),
        ("fb_refusal_retry", "noImg", "refusalImage"),
        ("fb_unsupported", "noImg", "zip"),
        ("fb_reuse_revised", "noImg", "reuseRevised"),
        ("fb_reuse_prompt", "noImg", "reusePrompt"),
        ("fb_reuse_desc", "noImg", "reuseDesc"),
        ("fb_reuse_whitespace", "noImg", "reuseWhitespace"),
    ];
    let fb_deps = FallbackDeps {
        db: &db,
        completion: &provider,
        transcoder: &transcoder,
        user_id: &spec.user_id,
        now_ms: 0,
    };
    for (label, profile_key, file_key) in fb_cases {
        let profile = &spec.resp_profiles[*profile_key];
        let f = &file_by_key[*file_key];
        let file = FallbackFile {
            id: f.id.clone(),
            filename: f.original_filename.clone(),
            mime_type: f.mime_type.clone(),
            data: Some(f.data_base64()),
        };
        let result = rt.block_on(process_file_attachment_fallback(&fb_deps, &file, profile));
        let got = serde_json::to_value(&result).expect("serialize FallbackResult");
        let (_family, want) = cases
            .get(*label)
            .unwrap_or_else(|| panic!("oracle missing case {label}"));
        assert_eq!(&got, want, "fb case {label} diverged");
    }

    // ---- (C) loadChatFilesForLLM (mount-path branch) ----
    {
        let meta: Value = serde_json::from_str(
            &std::fs::read_to_string(format!("{main_fixture}.meta.json"))
                .unwrap_or_else(|e| panic!("read meta sidecar: {e}")),
        )
        .expect("parse meta sidecar");
        let mount_link_id = meta
            .get("mountLinkId")
            .and_then(Value::as_str)
            .expect("mountLinkId")
            .to_string();
        let options = LoadChatFilesOptions::with_provider(Some("DEEPSEEK".to_string()));
        let attachments =
            load_chat_files_for_llm(&db, &bytes, &transcoder, &[mount_link_id], &options);
        let got = Value::Array(attachments);
        let (_family, want) = cases
            .get("lcffl_mount")
            .expect("oracle missing case lcffl_mount");
        assert_eq!(&got, want, "lcffl_mount diverged");
    }

    drop(db);
    cleanup(&main_work, &mount_work);
    eprintln!("OK: file-attachment differential matched the oracle across all cases.");
}
