//! Tier-3 differential: the photo-trio tool handlers (W4.9b; v4
//! `lib/tools/handlers/doc-edit/photo-handlers.ts`), ported as
//! `quilltap_core::tools::photo::{handle_keep_image, handle_list_images,
//! handle_attach_image}` over `quilltap_core::photos::save_image_to_album`.
//!
//! Both sides run the SAME op sequence (`photo-tools.json` + the fixture builder's
//! `.meta.json` sidecar) on a FRESH copy of the two-DB fixture (two transparent
//! characters with vaults; a chat with participants + `sceneState` +
//! `allowCrossCharacterVaultReads`; baked photos in both vaults via v4's REAL
//! `saveImageToAlbum`; ingested FileEntries with generation metadata; seeded
//! `photos/` chunk embeddings). Each op's raw handler result (`{ success, result?,
//! error? }` JSON) + the `formatDocEditResults` string are compared against v4's;
//! the KEEP cases additionally dump the five mount-index tables + `files` and diff
//! them in the shared-id-map remap form.
//!
//! MODEL SEAMS (pinned identically both sides): the semantic `list_images` branch's
//! embedding is a `CannedEmbeddingProvider` (matching the oracle's
//! `generateEmbeddingForUser` jest.mock); `keep_image`'s image bytes come from a
//! canned in-memory `FileBytesStore` keyed by the real fileId (the STORED bytes the
//! fixture recorded — so the recomputed sha matches v4's); the mount invalidation +
//! embedding enqueue are the `NoSideEffects` no-op (the oracle jest-mocks them).
//! The minted `keptAt` is PINNED to the per-op spec value both sides (it lands in the
//! filename + attribution footer + frontmatter — one mint, three sinks, identical),
//! so the keep dumps need remap only for the minted UUIDs + `createdAt`/`updatedAt`
//! (chunkCount diffs exactly since P4.6BK — v5 chunks on write).
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout). The oracle case
//! MUST be staged OUTSIDE any `.claude/` path (v4's jest ignores `/\.claude/`):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   WT=<this worktree root>
//!   STAGE=/tmp/qt-oracle-stage
//!   rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
//!   cp $WT/harness/oracle/cases/photo-tools.test.ts $STAGE/harness/oracle/cases/
//!   cp $WT/harness/oracle/fixtures/photo-tools.json  $STAGE/harness/oracle/fixtures/
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_PHOTO_MAIN=/tmp/qt-photo-main.db QT_FIXTURE_PHOTO_MOUNT=/tmp/qt-photo-mount.db \
//!     $N/node --import tsx $WT/harness/oracle/fixtures/build-photo-tools-fixture.ts
//!   QT_FIXTURE_PHOTO_MAIN=/tmp/qt-photo-main.db QT_FIXTURE_PHOTO_MOUNT=/tmp/qt-photo-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-photo-tools.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- photo-tools
//! Run:
//!   QT_ORACLE_PHOTO=/tmp/oracle-photo-tools.ndjson \
//!   QT_FIXTURE_PHOTO_MAIN=/tmp/qt-photo-main.db QT_FIXTURE_PHOTO_MOUNT=/tmp/qt-photo-mount.db \
//!     cargo test -p quilltap-harness --test photo_tools_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::files::FileEntry;
use quilltap_core::db::Writer;
use quilltap_core::model::embedding::{
    CannedEmbeddingProvider, EmbeddingPriority, EmbeddingProvider,
};
use quilltap_core::photos::save_image_to_album::{
    FileBytesStore, IngestImageRequest, NoSideEffects,
};
use quilltap_core::tools::doc_edit::shared::{
    collect_peer_character_ids_for_reads, DocEditToolContext,
};
use quilltap_core::tools::photo::{
    handle_attach_image, handle_keep_image, handle_list_images, PhotoToolContext, PhotoToolResult,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};

// ===========================================================================
// Spec + sidecar
// ===========================================================================

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "charAId")]
    char_a_id: String,
    #[serde(rename = "chatId")]
    chat_id: String,
    #[serde(rename = "chatMalformedId")]
    chat_malformed_id: String,
    #[serde(rename = "cannedEmbeddings")]
    canned_embeddings: HashMap<String, Vec<f32>>,
    #[serde(rename = "cannedFailures")]
    canned_failures: Vec<String>,
}

#[derive(Deserialize)]
struct Meta {
    #[serde(rename = "realFileIdByKey")]
    real_file_id_by_key: HashMap<String, String>,
    #[serde(rename = "realBytesByKey")]
    real_bytes_by_key: HashMap<String, Vec<u8>>,
    #[serde(rename = "bakedByKey")]
    baked_by_key: HashMap<String, BakedInfo>,
}

#[derive(Deserialize, Clone)]
struct BakedInfo {
    #[serde(rename = "linkId")]
    link_id: String,
    #[allow(dead_code)]
    vault: String,
    #[allow(dead_code)]
    #[serde(rename = "relativePath")]
    relative_path: String,
}

#[derive(Deserialize)]
struct OracleRow {
    label: String,
    #[serde(rename = "resultJson")]
    result_json: String,
    formatted: String,
    #[serde(default)]
    dumps: Option<HashMap<String, Value>>,
}

// ===========================================================================
// The canned image-byte store (the differential's FileBytesStore seam)
// ===========================================================================

/// The image bytes a `keep_image` reads — the fixture-recorded STORED bytes keyed
/// by fileId, matching the oracle's `readImageBuffer` override (which returns the
/// SAME per-fileId bytes). So a keep hashes the SAME bytes the baked photo did: a
/// keep of an already-baked image collides (`ALREADY_SAVED`), a fresh image doesn't.
struct CannedBytes {
    by_file_id: HashMap<String, Vec<u8>>,
}

impl FileBytesStore for CannedBytes {
    fn read_image_buffer(&self, entry: &FileEntry) -> Result<Option<Vec<u8>>, String> {
        Ok(self.by_file_id.get(&entry.id).cloned())
    }
    fn ingest_image_buffer(&self, _req: &IngestImageRequest) -> Result<FileEntry, String> {
        Err("mount-blob fallback ingest not used by the photo-tools corpus".to_string())
    }
}

// ===========================================================================
// Helpers
// ===========================================================================

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/photo-tools.json")
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

fn load_oracle(path: &str) -> HashMap<String, OracleRow> {
    let mut map = HashMap::new();
    for line in std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read oracle {path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let row: OracleRow = serde_json::from_str(line).expect("oracle line parses");
        map.insert(row.label.clone(), row);
    }
    map
}

fn fresh_copy(main_fixture: &str, mount_fixture: &str, tag: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir();
    let main_work = dir.join(format!(
        "qt-photo-main-rust-{}-{tag}.db",
        std::process::id()
    ));
    let mount_work = dir.join(format!(
        "qt-photo-mount-rust-{}-{tag}.db",
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

/// Turn a `PhotoToolResult` into the RAW handler-result JSON the oracle emitted
/// (`{ success, result?, error? }` — undefined keys dropped, mirroring v4's
/// `executeDocEditTool` return before the dispatch-row wrapping).
fn raw_result_json(out: &PhotoToolResult) -> Value {
    let mut m = Map::new();
    m.insert("success".into(), Value::Bool(out.success));
    if let Some(r) = &out.result {
        m.insert("result".into(), r.clone());
    }
    if let Some(e) = &out.error {
        m.insert("error".into(), Value::String(e.clone()));
    }
    if let Some(f) = &out.formatted_text {
        m.insert("formattedText".into(), Value::String(f.clone()));
    }
    Value::Object(m)
}

/// v4 `formatDocEditResults` for a photo handler: the handler always sets
/// `formattedText` on success; on failure it is `Error: <error>`.
fn format_doc_edit_results(out: &PhotoToolResult) -> String {
    if let Some(f) = &out.formatted_text {
        return f.clone();
    }
    if !out.success {
        return format!(
            "Error: {}",
            out.error.clone().unwrap_or_else(|| "Unknown error".into())
        );
    }
    // No formattedText on success would fall to JSON.stringify(result, null, 2) —
    // the photo handlers always set formattedText, so this is unreachable here.
    serde_json::to_string_pretty(&out.result.clone().unwrap_or(Value::Null)).unwrap_or_default()
}

/// Positional UUID normalization over a string (first-appearance `<uuid-N>`, one map
/// per call). When `enabled` is false, returns the string unchanged (list/attach
/// results mint nothing, so they diff exactly — the fixture ids are pinned).
fn maybe_norm_uuids(s: &str, enabled: bool) -> String {
    if !enabled {
        return s.to_string();
    }
    // A v4 UUID: 8-4-4-4-12 lowercase hex (the minted ids the port produces too).
    let bytes = s.as_bytes();
    let is_hex = |b: u8| b.is_ascii_hexdigit();
    let mut out = String::with_capacity(s.len());
    let mut map: HashMap<String, String> = HashMap::new();
    let mut i = 0;
    while i < bytes.len() {
        // Try to match a UUID starting at i.
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
        // Not a UUID here — copy this char.
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

// The mount-index tables + `files` diffed on the keep cases.
struct TableSpec {
    table: &'static str,
    order_by: &'static str,
    from_mount: bool,
    id_columns: &'static [&'static str],
    ts_columns: &'static [&'static str],
    pin_chunk_count: bool,
}

const KEEP_TABLES: &[TableSpec] = &[
    TableSpec {
        table: "doc_mount_points",
        order_by: "id",
        from_mount: true,
        id_columns: &["id"],
        ts_columns: &["createdAt", "updatedAt", "lastScannedAt"],
        pin_chunk_count: false,
    },
    TableSpec {
        table: "doc_mount_files",
        order_by: "sha256",
        from_mount: true,
        id_columns: &["id"],
        ts_columns: &["createdAt", "updatedAt"],
        pin_chunk_count: false,
    },
    TableSpec {
        // keep_image lands the image BYTES in `doc_mount_blobs` (via
        // `link_blob_content`), not `doc_mount_documents`; the markdown sidecar is
        // the LINK's `extractedText`. Order by the content sha (stable across the
        // minted fileId) so the row order matches both sides before the positional
        // id remap — the `[[wardrobe-transfers-remap-gotcha]]` precedent (never sort
        // by a minted id). The `data` BLOB is dumped as hex both sides.
        table: "doc_mount_blobs",
        order_by: "sha256",
        from_mount: true,
        id_columns: &["id", "fileId"],
        ts_columns: &["createdAt", "updatedAt"],
        pin_chunk_count: false,
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
    },
    TableSpec {
        table: "doc_mount_folders",
        order_by: "path",
        from_mount: true,
        id_columns: &["id", "parentId"],
        ts_columns: &["createdAt", "updatedAt"],
        pin_chunk_count: false,
    },
    TableSpec {
        table: "files",
        order_by: "id",
        from_mount: false,
        id_columns: &["id"],
        ts_columns: &["createdAt", "updatedAt"],
        pin_chunk_count: false,
    },
];

/// Remap id columns through the SHARED map (so cross-table FKs verify by
/// relationship), placeholder timestamps, pin `chunkCount`.
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
    }
}

fn normalize_dumps(dumps: &mut [Value]) {
    let mut id_map: HashMap<String, String> = HashMap::new();
    for (i, spec) in KEEP_TABLES.iter().enumerate() {
        normalize_table(&mut dumps[i], spec, &mut id_map);
    }
}

// ===========================================================================
// The corpus (mirrors the oracle op list)
// ===========================================================================

struct Op {
    label: &'static str,
    tool: &'static str,
    /// keep uses the malformed chat for the malformed-scene case.
    chat: Chat,
    /// `args` is built at run time from the resolved fileId/linkId.
    build_args: fn(&Meta) -> Value,
    /// The pinned keptAt for keep ops (v4 froze Date to this).
    kept_at: Option<&'static str>,
    /// Whether this op dumps the tables (keep cases).
    dumps: bool,
}

#[derive(Clone, Copy)]
enum Chat {
    Main,
    Malformed,
}

fn corpus() -> Vec<Op> {
    vec![
        Op {
            label: "keep_fresh",
            tool: "keep_image",
            chat: Chat::Main,
            build_args: |m| json!({ "uuid": m.real_file_id_by_key["fresh"], "caption": "a fresh keep", "tags": ["test"] }),
            kept_at: Some("2026-04-01T12:00:00.000Z"),
            dumps: true,
        },
        Op {
            label: "keep_duplicate",
            tool: "keep_image",
            chat: Chat::Main,
            build_args: |m| json!({ "uuid": m.real_file_id_by_key["sunset"] }),
            kept_at: Some("2026-04-01T12:00:00.000Z"),
            dumps: false,
        },
        Op {
            label: "keep_malformed_scene",
            tool: "keep_image",
            chat: Chat::Malformed,
            build_args: |m| json!({ "uuid": m.real_file_id_by_key["fresh"] }),
            kept_at: Some("2026-04-02T12:00:00.000Z"),
            dumps: true,
        },
        Op {
            label: "list_plain",
            tool: "list_images",
            chat: Chat::Main,
            build_args: |_| json!({}),
            kept_at: None,
            dumps: false,
        },
        Op {
            label: "list_plain_tag",
            tool: "list_images",
            chat: Chat::Main,
            build_args: |_| json!({ "tags": ["sea"] }),
            kept_at: None,
            dumps: false,
        },
        Op {
            label: "list_plain_savedby",
            tool: "list_images",
            chat: Chat::Main,
            build_args: |_| json!({ "saved_by": "Aurora" }),
            kept_at: None,
            dumps: false,
        },
        Op {
            label: "list_plain_pagination",
            tool: "list_images",
            chat: Chat::Main,
            build_args: |_| json!({ "limit": 1, "offset": 1 }),
            kept_at: None,
            dumps: false,
        },
        Op {
            label: "list_semantic",
            tool: "list_images",
            chat: Chat::Main,
            build_args: |_| json!({ "query": "sea sunset lighthouse" }),
            kept_at: None,
            dumps: false,
        },
        Op {
            label: "list_semantic_peer",
            tool: "list_images",
            chat: Chat::Main,
            build_args: |_| json!({ "query": "ancient library candles" }),
            kept_at: None,
            dumps: false,
        },
        Op {
            label: "list_semantic_fail",
            tool: "list_images",
            chat: Chat::Main,
            build_args: |_| json!({ "query": "please make the embedding fail" }),
            kept_at: None,
            dumps: false,
        },
        Op {
            label: "attach_by_linkid",
            tool: "attach_image",
            chat: Chat::Main,
            build_args: |m| json!({ "uuid": m.baked_by_key["sunset"].link_id }),
            kept_at: None,
            dumps: false,
        },
        Op {
            label: "attach_by_fileid",
            tool: "attach_image",
            chat: Chat::Main,
            build_args: |m| json!({ "uuid": m.real_file_id_by_key["sunset"] }),
            kept_at: None,
            dumps: false,
        },
        Op {
            label: "attach_cross_vault",
            tool: "attach_image",
            chat: Chat::Main,
            build_args: |m| json!({ "uuid": m.baked_by_key["peer"].link_id }),
            kept_at: None,
            dumps: false,
        },
        Op {
            label: "attach_missing",
            tool: "attach_image",
            chat: Chat::Main,
            build_args: |_| json!({ "uuid": "99999999-9999-4999-8999-999999999999" }),
            kept_at: None,
            dumps: false,
        },
    ]
}

// ===========================================================================
// The test
// ===========================================================================

#[test]
fn photo_tools_matches_oracle() {
    let (Some(oracle_path), Some(main_fixture), Some(mount_fixture)) = (
        env_or_skip("QT_ORACLE_PHOTO"),
        env_or_skip("QT_FIXTURE_PHOTO_MAIN"),
        env_or_skip("QT_FIXTURE_PHOTO_MOUNT"),
    ) else {
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let meta: Meta = serde_json::from_str(
        &std::fs::read_to_string(format!("{main_fixture}.meta.json"))
            .unwrap_or_else(|e| panic!("read meta sidecar: {e}")),
    )
    .expect("parse meta sidecar");
    let oracle = load_oracle(&oracle_path);

    // The canned embedding provider (mirrors the oracle's generateEmbeddingForUser mock).
    let mut provider = CannedEmbeddingProvider::new();
    for (text, vec) in &spec.canned_embeddings {
        provider = provider.with_vector(text.clone(), vec.clone());
    }
    for text in &spec.canned_failures {
        provider = provider.with_failure(text.clone());
    }

    // The canned image-byte store (fileId -> the fixture's stored bytes, matching
    // the oracle's readImageBuffer override).
    let mut by_file_id: HashMap<String, Vec<u8>> = HashMap::new();
    for (key, id) in &meta.real_file_id_by_key {
        if let Some(b) = meta.real_bytes_by_key.get(key) {
            by_file_id.insert(id.clone(), b.clone());
        }
    }
    let bytes = CannedBytes { by_file_id };
    let side_effects = NoSideEffects;

    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");

    for op in corpus() {
        let (main_work, mount_work) = fresh_copy(&main_fixture, &mount_fixture, op.label);
        let main = Writer::open_writable(&main_work, &spec.test_pepper_base64)
            .unwrap_or_else(|e| panic!("open main {}: {e}", op.label));
        let mount = Writer::open_writable(&mount_work, &spec.test_pepper_base64)
            .unwrap_or_else(|e| panic!("open mount {}: {e}", op.label));

        let chat_id = match op.chat {
            Chat::Main => spec.chat_id.clone(),
            Chat::Malformed => spec.chat_malformed_id.clone(),
        };
        let photo_ctx = PhotoToolContext {
            chat_id: chat_id.clone(),
            user_id: spec.user_id.clone(),
            project_id: None,
            character_id: Some(spec.char_a_id.clone()),
            embedding_profile_id: None,
        };
        let doc_ctx = DocEditToolContext {
            chat_id: chat_id.clone(),
            user_id: spec.user_id.clone(),
            project_id: None,
            character_id: Some(spec.char_a_id.clone()),
            operator_override: false,
            files_dir: None,
        };
        let args = (op.build_args)(&meta);

        let out = match op.tool {
            "keep_image" => handle_keep_image(
                main.connection(),
                mount.connection(),
                &args,
                &photo_ctx,
                &bytes,
                &side_effects,
                op.kept_at.expect("keep needs kept_at"),
            ),
            "list_images" => {
                // Compute the embedding up front (as the dispatcher does).
                let query = args
                    .get("query")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_default();
                let embedding: Option<Result<Vec<f32>, String>> = if query.trim().is_empty() {
                    None
                } else {
                    Some(
                        rt.block_on(provider.generate_embedding_for_user(
                            query.trim(),
                            &spec.user_id,
                            None,
                            EmbeddingPriority::Interactive,
                        ))
                        .map(|r| r.embedding)
                        .map_err(|e| e.message),
                    )
                };
                let peers = collect_peer_character_ids_for_reads(main.connection(), &doc_ctx);
                let embed = |_q: &str| -> Result<Vec<f32>, String> {
                    match &embedding {
                        Some(Ok(v)) => Ok(v.clone()),
                        Some(Err(e)) => Err(e.clone()),
                        None => Err("no embedding (empty query)".to_string()),
                    }
                };
                handle_list_images(
                    main.connection(),
                    mount.connection(),
                    &args,
                    &photo_ctx,
                    &peers,
                    &embed,
                )
            }
            "attach_image" => {
                handle_attach_image(main.connection(), mount.connection(), &args, &photo_ctx)
            }
            other => panic!("unknown tool {other}"),
        };

        let want = oracle
            .get(op.label)
            .unwrap_or_else(|| panic!("oracle missing case {}", op.label));

        // A keep result carries the minted `link_id` (a fresh UUID differing between
        // v4 + Rust); the pinned `file_id` + kept_at + sha + path + mount name are
        // identical. Normalize every UUID positionally (first-appearance `<uuid-N>`,
        // one map per compared string) so the minted link id maps to the same token
        // while the pinned ids diff exactly. list/attach mint nothing → no normalize.
        let normalize = op.dumps;
        let got_json = maybe_norm_uuids(
            &serde_json::to_string(&raw_result_json(&out)).unwrap(),
            normalize,
        );
        let want_json = maybe_norm_uuids(&want.result_json, normalize);
        assert_eq!(got_json, want_json, "resultJson diverged for {}", op.label);
        assert_eq!(
            maybe_norm_uuids(&format_doc_edit_results(&out), normalize),
            maybe_norm_uuids(&want.formatted, normalize),
            "formatted diverged for {}",
            op.label
        );

        // Keep cases: dump + diff the 6 tables (shared-id-map remap form).
        if op.dumps {
            let mut got_dumps: Vec<Value> = KEEP_TABLES
                .iter()
                .map(|s| {
                    let w = if s.from_mount { &mount } else { &main };
                    w.dump_table_json(s.table, s.order_by)
                        .unwrap_or_else(|e| panic!("dump {}: {e}", s.table))
                })
                .collect();
            let oracle_dumps = want
                .dumps
                .as_ref()
                .unwrap_or_else(|| panic!("oracle case {} has no dumps", op.label));
            let mut want_dumps: Vec<Value> = KEEP_TABLES
                .iter()
                .map(|s| {
                    oracle_dumps
                        .get(s.table)
                        .cloned()
                        .unwrap_or_else(|| panic!("oracle {} missing dump {}", op.label, s.table))
                })
                .collect();
            normalize_dumps(&mut got_dumps);
            normalize_dumps(&mut want_dumps);
            for (i, s) in KEEP_TABLES.iter().enumerate() {
                assert_eq!(
                    got_dumps[i]["rows"], want_dumps[i]["rows"],
                    "{}: {} rows diverged\n  rust:   {}\n  oracle: {}",
                    op.label, s.table, got_dumps[i]["rows"], want_dumps[i]["rows"]
                );
            }
        }

        drop(main);
        drop(mount);
        cleanup(&main_work, &mount_work);
    }

    eprintln!("OK: photo-tools differential matched the oracle across all cases.");
}
