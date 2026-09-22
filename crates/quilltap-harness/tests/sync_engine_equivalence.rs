//! Tier-2 differential: `sync_mount_point` END TO END — a real database-backed
//! store (real SQLite, real repositories, the real write chokepoints) and a real
//! directory — against v4's real `syncMountPoint`, over the shared scenario
//! corpus `harness/oracle/fixtures/sync-engine-scenarios.json`.
//!
//! The corpus is DATA, read by this driver and by the tsx/jest one, so the two
//! sides run the same script rather than two transcriptions of it. What it holds
//! that a planner test cannot reach:
//!
//!   - **A second run is a no-op.** This is the whole contract. If the appliers
//!     and the walks disagree about a single timestamp or a single trailing
//!     newline, the sync oscillates for ever and nobody notices until it has
//!     been running nightly for a month. Six scenarios run `sync` twice and the
//!     second report's summary is the assertion.
//!   - **The bytes that actually land**, on both sides, hashed.
//!   - **The manifest's own bytes**, parsed and compared key by key — it is the
//!     next run's base, so a field dropped here is a deletion next time.
//!   - The refusals, the walks' invisibility rules, and `--direction`.
//!
//! ## The one declared seam: the post-write re-index
//!
//! v4's own integration test mocks `reindexSingleFile` /
//! `reindexLinkGroupSiblings`, and the oracle mocks them the same way — the real
//! hook drags in the chunker and the embedding scheduler, none of which is what
//! the sync is being measured on. v5 has no mock seam there and runs the real
//! hook, so `doc_mount_chunks` and the links' `chunkCount` are OUT of the
//! comparand on both sides. [`the_sync_issues_no_chunk_sql_of_its_own`] carries
//! the v5-side half of what v4's own "never touches a chunk row itself" case
//! asserts; the hook's own behaviour is P4.D209's differential.
//!
//! ## Two normalizers, identical on both sides
//!
//! Every clock the corpus cares about is explicit, so an ISO instant that is NOT
//! one of the corpus's constants is `now` — a folder row's `updatedAt`, a blob's
//! `descriptionUpdatedAt` — and becomes `<minted>`. Minted UUIDs become
//! `<id-N>` in first-seen order, per scenario, which keeps the IDENTITY
//! relationships while dropping the values.
//!
//! Regenerate the oracle (Node 24, from the v4 checkout — cp to a /tmp mirror;
//! jest ignores `.claude/` paths):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-sync-engine-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/sync-engine.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/sync-engine-scenarios.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-sync-engine.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=300000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- sync-engine
//! Run:
//!   QT_ORACLE_SYNC_ENGINE=/tmp/oracle-sync-engine.ndjson cargo test -p quilltap-harness --test sync_engine_equivalence -- --nocapture

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use quilltap_core::db::database_store::delete_database_folder;
use quilltap_core::db::doc_mount_blobs::DocMountBlobsRepository;
use quilltap_core::db::doc_mount_documents::DocMountDocumentsRepository;
use quilltap_core::db::doc_mount_file_links::{
    sha256_of_string, DocMountFileLinksRepository, LinkBlobInput, LinkDocumentInput,
};
use quilltap_core::db::doc_mount_folders::DocMountFoldersRepository;
use quilltap_core::services::mount_index::sync::types::{
    SyncDirection, SyncOptions, SyncPreference,
};
use quilltap_core::services::mount_index::sync::{
    sync_mount_point, SyncDeps, SyncError, SyncMountPoint,
};

const MOUNT_ID: &str = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";

/// A SECOND store id for [`the_sync_issues_no_chunk_sql_of_its_own`].
///
/// The engine's `in_flight` set is a process global — one run per store at a
/// time, which is exactly what it is for — and cargo runs this file's two tests
/// in parallel threads. Sharing [`MOUNT_ID`] made the standalone test's run hold
/// the slot while the differential's first two scenarios asked for it, and they
/// came back `SYNC_IN_PROGRESS`. That is the mutex working, not a defect; the
/// two tests simply must not name the same store.
const CHUNK_MOUNT_ID: &str = "bbbbbbbb-cccc-4ddd-8eee-ffffffffffff";

fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/sync-engine-scenarios.json")
}

// ===========================================================================
// The store: an in-memory mount index with the tables the sync touches
// ===========================================================================

/// The mount-index DDL the corpus needs, taken from the provisioned schema so
/// the columns are the ones production writes.
fn open_store() -> Connection {
    open_store_for(MOUNT_ID)
}

fn open_store_for(mount_id: &str) -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory db");
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    let ddl = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../quilltap-core/src/services/provisioning/fresh_schema.json"),
    )
    .expect("read fresh_schema.json");
    let schema: Value = serde_json::from_str(&ddl).expect("parse fresh_schema.json");
    let mut applied = 0usize;
    for statement in collect_ddl(&schema) {
        if statement.contains("doc_mount_") {
            conn.execute_batch(&statement).unwrap_or_else(|e| {
                panic!("apply `{}`: {e}", &statement[..statement.len().min(80)])
            });
            applied += 1;
        }
    }
    assert!(
        applied >= 6,
        "only {applied} `doc_mount_*` statements applied — the schema dump's shape moved"
    );
    // The store's OWN row. Without it the post-write re-index cannot tell a
    // database store from a filesystem one and goes looking on disk — which is
    // how the first draft of this family produced a page of
    // `No such file or directory` and no chunks at all. The oracle mocks the
    // hook away and so never needed the row; v5 runs it for real and does.
    conn.execute(
        "INSERT INTO doc_mount_points (id, name, basePath, mountType, storeType, \
           includePatterns, excludePatterns, enabled, scanStatus, conversionStatus, \
           fileCount, chunkCount, totalSizeBytes, createdAt, updatedAt) \
         VALUES (?1, 'Lore', '', 'database', 'documents', '[]', '[\"node_modules\"]', 1, \
           'idle', 'idle', 0, 0, 0, '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
        [mount_id],
    )
    .expect("seed the mount-point row");
    conn
}

/// The MAIN partition the archived-vault guard reads. It needs the real
/// `characters` DDL, not a three-column stub: the guard goes through
/// `characters_read::find_all_raw`, which selects the whole row.
fn open_main(archived_vault: Option<&str>) -> Connection {
    let conn = Connection::open_in_memory().expect("main");
    let schema: Value = serde_json::from_str(
        &std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../quilltap-core/src/services/provisioning/fresh_schema.json"),
        )
        .expect("read fresh_schema.json"),
    )
    .expect("parse fresh_schema.json");
    let mut applied = false;
    for statement in collect_ddl(&schema) {
        if statement.contains("TABLE \"characters\"") || statement.contains("TABLE characters") {
            conn.execute_batch(&statement)
                .expect("apply the characters DDL");
            applied = true;
        }
    }
    assert!(applied, "no `characters` CREATE TABLE in the schema dump");
    if let Some(vault) = archived_vault {
        conn.execute(
            "INSERT INTO characters \
               (id, userId, name, createdAt, updatedAt, archivedAt, characterDocumentMountPointId) \
             VALUES ('c1', 'u1', 'Archived', '2026-01-01T00:00:00.000Z', \
               '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z', ?1)",
            [vault],
        )
        .expect("seed an archived character");
    }
    conn
}

/// Every `CREATE …` string anywhere in the dump, in document order.
fn collect_ddl(value: &Value) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::String(s) if s.trim_start().to_uppercase().starts_with("CREATE ") => {
                out.push(s.clone())
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            Value::Object(o) => o.values().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    walk(value, &mut out);
    out
}

// ===========================================================================
// Normalizers — the mirror of the oracle's `makeNormalizers`
// ===========================================================================

struct Scrubber {
    known_clocks: Vec<String>,
    ids: BTreeMap<String, String>,
    next_id: usize,
}

impl Scrubber {
    fn new(constants: &Map<String, Value>) -> Self {
        let mut known: Vec<String> = constants
            .values()
            .filter_map(|v| v.as_str())
            .map(str::to_string)
            .collect();
        // The store row's own fixed clocks, which the oracle's constants do not
        // carry but both sides hard-code identically.
        known.push("2026-01-01T00:00:00.000Z".to_string());
        Self {
            known_clocks: known,
            ids: BTreeMap::new(),
            next_id: 1,
        }
    }

    fn is_iso(s: &str) -> bool {
        let b = s.as_bytes();
        b.len() >= 20
            && b[4] == b'-'
            && b[7] == b'-'
            && b[10] == b'T'
            && s.ends_with('Z')
            && b[..4].iter().all(u8::is_ascii_digit)
    }

    fn is_uuid(s: &str) -> bool {
        let b = s.as_bytes();
        b.len() == 36
            && b.iter().enumerate().all(|(i, &c)| match i {
                8 | 13 | 18 | 23 => c == b'-',
                _ => c.is_ascii_hexdigit(),
            })
    }

    fn scrub(&mut self, v: &Value) -> Value {
        match v {
            Value::String(s) => {
                if Self::is_iso(s) && !self.known_clocks.iter().any(|k| k == s) {
                    return Value::String("<minted>".into());
                }
                if Self::is_uuid(s) {
                    if !self.ids.contains_key(s) {
                        self.ids.insert(s.clone(), format!("<id-{}>", self.next_id));
                        self.next_id += 1;
                    }
                    return Value::String(self.ids[s].clone());
                }
                v.clone()
            }
            Value::Array(a) => Value::Array(a.iter().map(|x| self.scrub(x)).collect()),
            Value::Object(o) => {
                let mut out = Map::new();
                for (k, val) in o {
                    out.insert(k.clone(), self.scrub(val));
                }
                Value::Object(out)
            }
            other => other.clone(),
        }
    }
}

// ===========================================================================
// The driver
// ===========================================================================

fn set_disk_mtime(path: &Path, iso: &str) {
    let ms = quilltap_core::clock::iso_to_ms(iso).expect("a corpus clock parses");
    let when = std::time::UNIX_EPOCH + std::time::Duration::from_millis(ms as u64);
    let times = std::fs::FileTimes::new()
        .set_accessed(when)
        .set_modified(when);
    std::fs::File::options()
        .write(true)
        .open(path)
        .expect("open for utimes")
        .set_times(times)
        .expect("set times");
}

fn sha_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

#[allow(clippy::too_many_lines)]
#[test]
fn sync_engine_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_SYNC_ENGINE") else {
        eprintln!("SKIP: set QT_ORACLE_SYNC_ENGINE to the oracle NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("cannot read oracle {oracle_path}: {e}"));
    let corpus: Value =
        serde_json::from_str(&std::fs::read_to_string(corpus_path()).expect("read the corpus"))
            .expect("parse the corpus");
    let constants = corpus["constants"].as_object().expect("constants").clone();
    let scenarios = corpus["scenarios"].as_array().expect("scenarios").clone();

    let want_by_id: BTreeMap<String, Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Value = serde_json::from_str(l).expect("parse oracle row");
            (v["id"].as_str().expect("id").to_string(), v)
        })
        .collect();
    assert_eq!(
        want_by_id.len(),
        scenarios.len(),
        "the oracle has {} rows for {} corpus scenarios — stale oracle?",
        want_by_id.len(),
        scenarios.len()
    );

    let clock = |token: &Value| -> Option<String> {
        let t = token.as_str()?;
        Some(
            constants
                .get(t)
                .and_then(Value::as_str)
                .unwrap_or(t)
                .to_string(),
        )
    };
    let bytes_of = |token: &Value| -> Vec<u8> {
        use base64::Engine;
        let t = token.as_str().expect("a bytes token");
        let b64 = constants
            .get(&format!("{t}_BASE64"))
            .and_then(Value::as_str)
            .unwrap_or(t);
        base64::engine::general_purpose::STANDARD
            .decode(b64)
            .expect("decode a corpus payload")
    };

    let mut mismatches: Vec<String> = Vec::new();
    let mut second_run_noops = 0usize;

    for scenario in &scenarios {
        let id = scenario["id"].as_str().expect("id").to_string();
        let over = scenario.get("store").cloned().unwrap_or(json!({}));
        let s = |k: &str, d: &str| -> String {
            over.get(k).and_then(Value::as_str).unwrap_or(d).to_string()
        };
        let archived = over
            .get("archived")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mount = SyncMountPoint {
            id: MOUNT_ID.to_string(),
            name: "Lore".to_string(),
            mount_type: s("mountType", "database"),
            base_path: s("basePath", ""),
            store_type: s("storeType", "documents"),
            scan_status: s("scanStatus", "idle"),
            conversion_status: s("conversionStatus", "idle"),
            exclude_patterns: vec!["node_modules".to_string()],
        };

        let conn = open_store();
        let main = open_main(if archived { Some(MOUNT_ID) } else { None });

        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().to_path_buf();
        let links = DocMountFileLinksRepository::new(&conn);
        let blobs = DocMountBlobsRepository::new(&conn);
        let mut reports: Vec<Value> = Vec::new();

        for step in scenario["steps"].as_array().expect("steps") {
            let op = step["op"].as_str().expect("op");
            let rel = step.get("path").and_then(Value::as_str).unwrap_or("");
            let abs = target.join(rel);
            match op {
                "seedDoc" => {
                    let content = step["content"].as_str().expect("content").to_string();
                    links
                        .link_document_content(&LinkDocumentInput {
                            mount_point_id: MOUNT_ID.to_string(),
                            relative_path: rel.to_string(),
                            file_name: rel.rsplit('/').next().unwrap().to_string(),
                            file_type: "markdown".to_string(),
                            content_sha256: sha256_of_string(&content),
                            plain_text_length: quilltap_core::jsstr::utf16_len(&content) as i64,
                            file_size_bytes: content.len() as i64,
                            content,
                            allow_embed: None,
                            allow_character_read: None,
                            allow_character_write: None,
                            last_modified: step.get("lastModified").and_then(&clock),
                            created_at: step.get("createdAt").and_then(&clock),
                        })
                        .expect("seedDoc");
                }
                "seedBlob" => {
                    let data = bytes_of(&step["bytes"]);
                    let name = rel.rsplit('/').next().unwrap().to_string();
                    links
                        .link_blob_content(&LinkBlobInput {
                            mount_point_id: MOUNT_ID.to_string(),
                            relative_path: rel.to_string(),
                            file_name: name.clone(),
                            file_type: Some("blob".to_string()),
                            original_file_name: Some(name),
                            original_mime_type: Some("image/png".to_string()),
                            stored_mime_type: "image/png".to_string(),
                            sha256: sha_hex(&data),
                            data,
                            // The seed mirrors the oracle's `linkBlobContent`
                            // call, which passes no `normalizeImages` either.
                            normalize_images: true,
                            description: step
                                .get("description")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            conversion_status: None,
                            extracted_text: None,
                            extracted_text_sha256: None,
                            extraction_status: None,
                            last_modified: step.get("lastModified").and_then(&clock),
                            created_at: step.get("createdAt").and_then(&clock),
                        })
                        .expect("seedBlob");
                }
                "seedFolder" => {
                    links.ensure_folder_path(MOUNT_ID, rel).expect("seedFolder");
                }
                "setDescription" => {
                    let link = links
                        .find_by_mount_point_and_path(MOUNT_ID, rel)
                        .expect("find")
                        .expect("a link");
                    let blob = blobs
                        .find_by_file_id(&link.file_id)
                        .expect("find blob")
                        .expect("a blob");
                    blobs
                        .update_description(&blob.id, step["text"].as_str().unwrap(), &link.id)
                        .expect("setDescription");
                }
                "deleteLink" => {
                    let link = links
                        .find_by_mount_point_and_path(MOUNT_ID, rel)
                        .expect("find")
                        .expect("a link");
                    links.delete_with_gc(&link.id).expect("deleteLink");
                }
                "deleteFolder" => {
                    delete_database_folder(&conn, MOUNT_ID, rel).expect("deleteFolder");
                }
                "writeDisk" | "writeDiskBytes" => {
                    let body: Vec<u8> = if op == "writeDisk" {
                        step["body"].as_str().expect("body").as_bytes().to_vec()
                    } else {
                        bytes_of(&step["bytes"])
                    };
                    if let Some(parent) = abs.parent() {
                        std::fs::create_dir_all(parent).expect("mkdir -p");
                    }
                    std::fs::write(&abs, &body).expect("writeDisk");
                    set_disk_mtime(&abs, &clock(&step["mtime"]).expect("mtime"));
                }
                "rmDisk" => {
                    if abs.is_dir() {
                        let _ = std::fs::remove_dir_all(&abs);
                    } else {
                        let _ = std::fs::remove_file(&abs);
                    }
                }
                "symlinkDisk" => {
                    std::os::unix::fs::symlink(step["target"].as_str().unwrap(), &abs)
                        .expect("symlink");
                }
                "sync" => {
                    let o = step.get("options").cloned().unwrap_or(json!({}));
                    let options = SyncOptions {
                        target_path: target.to_string_lossy().into_owned(),
                        dry_run: o.get("dryRun").and_then(Value::as_bool).unwrap_or(false),
                        direction: o
                            .get("direction")
                            .and_then(Value::as_str)
                            .and_then(SyncDirection::parse)
                            .unwrap_or(SyncDirection::Both),
                        prefer: o
                            .get("prefer")
                            .and_then(Value::as_str)
                            .and_then(SyncPreference::parse)
                            .unwrap_or(SyncPreference::Newer),
                        propagate_deletes: o
                            .get("propagateDeletes")
                            .and_then(Value::as_bool)
                            .unwrap_or(true),
                        use_manifest: o
                            .get("useManifest")
                            .and_then(Value::as_bool)
                            .unwrap_or(true),
                    };
                    // No transcoder: the corpus's image payload is not a
                    // decodable image (deliberately — see `apply_store.rs`), so
                    // the normalizer declines on both sides either way, and a
                    // host codec here would put the `image` crate's decisions
                    // into a comparand about the SYNC.
                    match sync_mount_point(
                        &conn,
                        &main,
                        &mount,
                        &options,
                        &SyncDeps { transcoder: None },
                    ) {
                        Ok(report) => {
                            let mut v =
                                serde_json::to_value(&report).expect("serialize the report");
                            v["targetPath"] = json!("<target>");
                            v["elapsedMs"] = json!("<elapsed>");
                            let shown = target.to_string_lossy().into_owned();
                            v["warnings"] = json!(report
                                .warnings
                                .iter()
                                .map(|w| w.replace(&shown, "<target>"))
                                .collect::<Vec<_>>());
                            reports.push(v);
                        }
                        Err(e) => {
                            let (name, code) = match &e {
                                SyncError::Refused(r) => {
                                    ("SyncRefusedError", Some(r.code.as_str().to_string()))
                                }
                                SyncError::ManifestMismatch(_) => ("ManifestMismatchError", None),
                                _ => ("Error", None),
                            };
                            reports.push(json!({
                                "refused": true,
                                "name": name,
                                "code": code,
                                "message": e.to_string(),
                            }));
                        }
                    }
                }
                other => panic!("unknown corpus op `{other}`"),
            }
        }

        let got = json!({
            "id": id,
            "reports": reports,
            "failure": Value::Null,
            "store": dump_store(&conn),
            "disk": dump_disk(&target),
        });
        let mut scrubber = Scrubber::new(&constants);
        let got = scrubber.scrub(&got);
        let want = &want_by_id[&id];

        if &got != want {
            mismatches.push(format!(
                "[{id}]\n      v4: {}\n      v5: {}",
                serde_json::to_string(want).unwrap(),
                serde_json::to_string(&got).unwrap(),
            ));
        }

        // The contract, asserted on v5's own side as well as diffed: where a
        // scenario runs `sync` twice with nothing between, the SECOND report
        // must be empty of work. A differential alone would pass if BOTH sides
        // oscillated the same way.
        let sync_steps: Vec<&Value> = scenario["steps"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["op"] == "sync")
            .collect();
        if sync_steps.len() >= 2 {
            let last_two_adjacent = scenario["steps"]
                .as_array()
                .unwrap()
                .windows(2)
                .any(|w| w[0]["op"] == "sync" && w[1]["op"] == "sync");
            if last_two_adjacent {
                let final_report = reports.last().expect("a report");
                if let Some(summary) = final_report.get("summary") {
                    let busy: i64 = ["created", "modified", "deleted", "touched", "described"]
                        .iter()
                        .map(|k| summary[*k].as_i64().unwrap_or(0))
                        .sum();
                    if busy != 0 {
                        mismatches.push(format!(
                            "[{id}] the SECOND run found work to do ({summary}) — the sync \
                             oscillates, which is the one defect this family exists for"
                        ));
                    } else {
                        second_run_noops += 1;
                    }
                }
            }
        }
    }

    assert!(
        mismatches.is_empty(),
        "{} scenarios disagree with v4:\n  {}",
        mismatches.len(),
        mismatches.join("\n  ")
    );
    assert_eq!(
        scenarios.len(),
        47,
        "expected the full corpus (47 scenarios), saw {}",
        scenarios.len()
    );
    assert!(
        second_run_noops >= 5,
        "only {second_run_noops} scenarios proved a second run is a no-op — the convergence \
         half of the corpus has shrunk"
    );
    eprintln!(
        "OK: sync_mount_point matched v4 on {} scenarios ({second_run_noops} of them proving a \
         second run is a no-op).",
        scenarios.len()
    );
}

/// The store's rows, in the oracle's shape and order.
fn dump_store(conn: &Connection) -> Value {
    let links = DocMountFileLinksRepository::new(conn);
    let blobs = DocMountBlobsRepository::new(conn);
    let documents = DocMountDocumentsRepository::new(conn);
    let mut all = links
        .find_by_mount_point_id(MOUNT_ID)
        .expect("list the links");
    all.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));

    let mut rows = Vec::new();
    for link in &all {
        let doc = documents
            .find_content_by_file_id(&link.file_id)
            .expect("read the document");
        let blob = blobs.find_by_file_id(&link.file_id).expect("read the blob");
        let data = if blob.is_some() {
            blobs
                .read_data_by_file_id(&link.file_id)
                .expect("read the bytes")
        } else {
            None
        };
        rows.push(json!({
            "relativePath": link.relative_path,
            "fileName": link.file_name,
            "fileType": link.file_type,
            "sha256": link.sha256,
            "fileSizeBytes": link.file_size_bytes,
            "description": link.description.clone().unwrap_or_default(),
            "lastModified": link.last_modified,
            "createdAt": link.created_at,
            "originalMimeType": link.original_mime_type,
            "storedMimeType": blob.as_ref().map(|b| b.stored_mime_type.clone()),
            "contentSha256": match (&doc, &data) {
                (Some(content), _) => Value::String(sha_hex(content.as_bytes())),
                (None, Some(bytes)) => Value::String(sha_hex(bytes)),
                _ => Value::Null,
            },
            "kind": if doc.is_some() { "document" } else if blob.is_some() { "blob" } else { "none" },
        }));
    }

    let mut folders: Vec<String> = DocMountFoldersRepository::new(conn)
        .find_by_mount_point_id(MOUNT_ID)
        .expect("list the folders")
        .into_iter()
        .map(|f| f.path)
        .filter(|p| !p.is_empty())
        .collect();
    folders.sort();

    json!({ "links": rows, "folders": folders })
}

/// The directory tree: every path, its kind, and its bytes' sha.
fn dump_disk(dir: &Path) -> Value {
    let mut out: Vec<Value> = Vec::new();
    walk_disk(dir, "", &mut out);
    Value::Array(out)
}

fn walk_disk(dir: &Path, rel: &str, out: &mut Vec<Value>) {
    let here = if rel.is_empty() {
        dir.to_path_buf()
    } else {
        dir.join(rel)
    };
    let Ok(read) = std::fs::read_dir(&here) else {
        return;
    };
    let mut entries: Vec<(String, std::fs::FileType)> = read
        .filter_map(Result::ok)
        .filter_map(|e| {
            e.file_type()
                .ok()
                .map(|t| (e.file_name().to_string_lossy().into_owned(), t))
        })
        .collect();
    // The oracle sorts by name (JS `<`, i.e. UTF-16 code units); these are all
    // ASCII in the corpus, where that agrees with the byte order.
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    for (name, file_type) in entries {
        let rel_path = if rel.is_empty() {
            name.clone()
        } else {
            format!("{rel}/{name}")
        };
        if file_type.is_symlink() {
            out.push(json!({ "path": rel_path, "kind": "symlink" }));
            continue;
        }
        if file_type.is_dir() {
            out.push(json!({ "path": rel_path, "kind": "dir" }));
            walk_disk(dir, &rel_path, out);
            continue;
        }
        let bytes = std::fs::read(dir.join(&rel_path)).expect("read a file");
        let mut row = Map::new();
        row.insert("path".into(), json!(rel_path));
        row.insert("kind".into(), json!("file"));
        if rel_path == ".quilltap-sync.json" {
            // The manifest is the one file whose CONTENT is part of the
            // contract, and `lastSyncAt` is the one field the engine mints.
            let mut parsed: Value = serde_json::from_slice(&bytes).expect("the manifest is JSON");
            parsed["lastSyncAt"] = json!("<lastSyncAt>");
            row.insert("manifest".into(), parsed);
        } else {
            row.insert("sha256".into(), json!(sha_hex(&bytes)));
            row.insert("sizeBytes".into(), json!(bytes.len()));
            let meta = std::fs::metadata(dir.join(&rel_path)).expect("stat");
            row.insert(
                "lastModified".into(),
                json!(quilltap_core::clock::iso_from_unix_ms(mtime_ms(&meta))),
            );
            if rel_path.to_lowercase().ends_with(".description.md") {
                row.insert(
                    "text".into(),
                    json!(String::from_utf8_lossy(&bytes).into_owned()),
                );
            }
        }
        out.push(Value::Object(row));
    }
}

fn mtime_ms(meta: &std::fs::Metadata) -> i64 {
    use std::os::unix::fs::MetadataExt;
    meta.mtime() * 1000 + i64::from(meta.mtime_nsec() as i32) / 1_000_000
}

/// The v5-side half of v4's "never touches a chunk row itself — only the store's
/// own reindex hook does".
///
/// The re-index hook is the declared seam (mocked on v4's side, real here), so
/// the chunk rows are out of the differential's comparand. What stays provable
/// from this side is the shape of them: a TEXT document pushed in by the sync
/// carries chunks, because `write_store_file` calls
/// `reindex_after_database_write` on that branch; a BLOB carries none, because
/// it does not. If the sync ever issued chunk SQL of its own, the blob would
/// have rows.
#[test]
fn the_sync_issues_no_chunk_sql_of_its_own() {
    let conn = open_store_for(CHUNK_MOUNT_ID);
    let main = open_main(None);
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("doc.md"),
        "# A heading\n\nA paragraph of prose.",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("blob.png"),
        b"\x89PNG\r\n\x1a\n not an image",
    )
    .unwrap();

    let mount = SyncMountPoint {
        id: CHUNK_MOUNT_ID.to_string(),
        name: "Lore".to_string(),
        mount_type: "database".to_string(),
        base_path: String::new(),
        store_type: "documents".to_string(),
        scan_status: "idle".to_string(),
        conversion_status: "idle".to_string(),
        exclude_patterns: vec![],
    };
    let report = sync_mount_point(
        &conn,
        &main,
        &mount,
        &SyncOptions::with_defaults(dir.path().to_string_lossy().into_owned()),
        &SyncDeps { transcoder: None },
    )
    .expect("the sync runs");
    assert_eq!(report.summary.failed, 0, "{:?}", report.actions);

    let links = DocMountFileLinksRepository::new(&conn);
    let count = |path: &str| -> i64 {
        let link = links
            .find_by_mount_point_and_path(CHUNK_MOUNT_ID, path)
            .unwrap()
            .expect("a link");
        conn.query_row(
            "SELECT COUNT(*) FROM doc_mount_chunks WHERE linkId = ?1",
            [&link.id],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0)
    };
    assert!(
        count("doc.md") > 0,
        "the text document has no chunks — the sync's own `reindex_after_database_write` \
         call on the document branch has been lost"
    );
    assert_eq!(
        count("blob.png"),
        0,
        "the blob has chunk rows — the sync is writing chunks of its own, which v4's \
         `apply-store.ts` never does (it re-indexes only documents and pdf/docx)"
    );
}
