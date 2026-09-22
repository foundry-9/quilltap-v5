//! Tier-2 differential test: the document-store WRITE-METADATA contract
//! (P4.D209 — v4 bugs 155, 156 and 157, plus `23da0b322`'s seam 4).
//!
//! Both sides run the SAME op sequence (from the committed corpus) against the
//! SAME mount-index fixture, driving the REPOSITORIES DIRECTLY —
//! `linkDocumentContent`, `linkBlobContent`, `setLinkTimestamps`,
//! `bindLinkGroup`, `updateDescription`, `updateExtractedText` — never through
//! `writeDatabaseDocument`. **That distinction is the whole instrument.** The
//! database-store writer re-chunks the moment it returns, so it puts
//! `chunkCount` straight back and bug 156 leaves NO trace in a final-state dump;
//! `doc_mount_file_links_tier2` is green before and after the fix for exactly
//! that reason (measured, P4.D209). Every byte-preserving writer — file-ops, the
//! sync applier, every in-child `doc_write_file` — calls the repository
//! directly, and that is where the three defects live.
//!
//! What each group proves, all of it comparand-visible in the dump:
//!
//!   - **bug 155** (`23da0b322`). `art/logo.bin` is written three times: with
//!     metadata, then with NEW BYTES AND NOT A WORD about the metadata, then
//!     with an explicit `''`. Its `extractedText` / `extractionStatus` survive
//!     the silent overwrite and its `description` is cleared only by the
//!     explicit empty string — the two-arm proof. `art/fresh.bin` takes the
//!     blank insert defaults. `art/nulled.bin` adds the third arm: an explicit
//!     JS `null` on `extractedText` CLEARS where an omission KEEPS, and the
//!     omitted `description` beside it is kept in the same write.
//!   - **bug 156** (`23da0b322`). `notes/one.md` is chunked and then repointed:
//!     `chunkCount` returns to 0 and its `doc_mount_chunks` row is GONE.
//!     `notes/two.md` is chunked and rewritten with IDENTICAL bytes: both
//!     survive, and it is the only chunk row left. `twins/left.md` and
//!     `twins/right.md` are bound into a hard-link group, both chunked, and
//!     then the left one is rewritten — the SIBLING's chunks are retired too,
//!     because chunks are keyed by `linkId`. The `needs-rechunk-probe` result
//!     reads the rescan's own predicate back over every link, so "the overwrite
//!     announces itself to the rescan" is a comparand and not an argument.
//!   - **seam 4** (`23da0b322`). `dated/doc.md` is inserted with both
//!     timestamps pinned and then updated with `lastModified` alone —
//!     `createdAt` must reach the dump still carrying its INSERT value, which
//!     is why no later op touches it (a set would be indistinguishable from a
//!     blank-then-set). `dated/blob.bin` pins `lastModified` on the blob
//!     writer. `dated/plain.md` has no opinion and gets `now`.
//!     `setLinkTimestamps` moves each of the two columns on a path of its own
//!     and reports `false` for an unknown link AND for an empty ask.
//!   - **bug 157** (`0c14fd61f`). `vault/photos/a.bin` and `vault/history/a.bin`
//!     are byte-identical, so they dedup onto ONE `doc_mount_files` row with ONE
//!     blob — and TWO links. Each path resolves to its OWN link id
//!     (`resolve-links`, with `distinct`), each caption lands on the asked path
//!     and leaves the twin's blank, and the extracted text is per location.
//!     Under the deleted `WHERE fileId = ? LIMIT 1` fallback both captions
//!     landed on whichever link SQLite handed back first.
//!
//! NORMALIZATION (identical on both dumps): a SINGLE first-seen id map walked
//! across the five tables in `TABLES` order, so cross-table FKs verify the
//! RELATIONSHIP without pinning an id; `mountPointId` is the seeded store id and
//! is left literal. Timestamps become `<ts>` **except the corpus's
//! `pinnedTimestamps`**, which stay verbatim — without that carve-out "honoured
//! the caller's clock" and "stamped now" are the same string, and the whole
//! timestamp group is unprovable.
//!
//! The fixture is the one `build-doc-mount-file-links-fixture.ts` builds (same
//! seeded store id), so there is no second builder to drift.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-dmwm-fixture.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-doc-mount-file-links-fixture.ts
//!   QT_FIXTURE_DOC_MOUNT_WRITE_METADATA=/tmp/qt-dmwm-fixture.db \
//!     $N/npx tsx $V5W/harness/oracle/cases/doc-mount-write-metadata.ts \
//!     > /tmp/oracle-dmwm.ndjson
//! Run:
//!   QT_ORACLE_DOC_MOUNT_WRITE_METADATA=/tmp/oracle-dmwm.ndjson \
//!   QT_FIXTURE_DOC_MOUNT_WRITE_METADATA=/tmp/qt-dmwm-fixture.db \
//!     cargo test -p quilltap-harness --test doc_mount_write_metadata_equivalence -- --nocapture

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use quilltap_core::db::doc_mount_blobs::DocMountBlobsRepository;
use quilltap_core::db::doc_mount_file_links::{LinkBlobInput, LinkDocumentInput};
use quilltap_core::db::Writer;
use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// Absent / explicit `null` / set — bug 155's whole distinction, and serde's
/// only way to keep it (`Option<Option<T>>` collapses `null` into `None`
/// without this).
fn double_option<'de, D, T>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Deserialize::deserialize(de).map(Some)
}

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    store: Store,
    #[serde(rename = "pinnedTimestamps")]
    pinned_timestamps: Vec<String>,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
struct Store {
    id: String,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Op {
    #[serde(rename = "write-doc", rename_all = "camelCase")]
    WriteDoc {
        relative_path: String,
        content: String,
        last_modified: Option<String>,
        created_at: Option<String>,
    },
    #[serde(rename = "write-blob", rename_all = "camelCase")]
    WriteBlob {
        relative_path: String,
        data_hex: String,
        stored_mime_type: String,
        original_file_name: String,
        original_mime_type: String,
        description: Option<String>,
        #[serde(default, deserialize_with = "double_option")]
        extracted_text: Option<Option<String>>,
        #[serde(default, deserialize_with = "double_option")]
        extracted_text_sha256: Option<Option<String>>,
        extraction_status: Option<String>,
        last_modified: Option<String>,
        created_at: Option<String>,
    },
    #[serde(rename = "chunk", rename_all = "camelCase")]
    Chunk { relative_path: String },
    #[serde(rename = "bind-group", rename_all = "camelCase")]
    BindGroup {
        source_path: String,
        dest_path: String,
    },
    #[serde(rename = "set-timestamps", rename_all = "camelCase")]
    SetTimestamps {
        relative_path: String,
        last_modified: Option<String>,
        created_at: Option<String>,
    },
    #[serde(rename = "set-timestamps-unknown", rename_all = "camelCase")]
    SetTimestampsUnknown { link_id: String },
    #[serde(rename = "resolve-links", rename_all = "camelCase")]
    ResolveLinks { paths: Vec<String> },
    #[serde(rename = "update-description", rename_all = "camelCase")]
    UpdateDescription {
        relative_path: String,
        description: String,
    },
    #[serde(rename = "update-extracted-text", rename_all = "camelCase")]
    UpdateExtractedText {
        relative_path: String,
        extracted_text: Option<String>,
        extracted_text_sha256: Option<String>,
        extraction_status: String,
        extraction_error: Option<String>,
    },
    #[serde(rename = "needs-rechunk-probe")]
    NeedsRechunkProbe,
}

struct TableSpec {
    table: &'static str,
    /// The natural key both sides sort by. Carried here so the spec and the
    /// dump call cannot drift apart silently; the dumps themselves do the
    /// ordering.
    #[allow(dead_code)]
    order_by: &'static str,
    id_columns: &'static [&'static str],
    ts_columns: &'static [&'static str],
}

const TABLES: &[TableSpec] = &[
    TableSpec {
        table: "doc_mount_files",
        order_by: "sha256",
        id_columns: &["id"],
        ts_columns: &["createdAt", "updatedAt"],
    },
    TableSpec {
        table: "doc_mount_documents",
        order_by: "contentSha256",
        id_columns: &["id", "fileId"],
        ts_columns: &["createdAt", "updatedAt"],
    },
    TableSpec {
        table: "doc_mount_file_links",
        order_by: "relativePath",
        id_columns: &["id", "fileId", "folderId", "linkGroupId"],
        ts_columns: &[
            "lastModified",
            "descriptionUpdatedAt",
            "createdAt",
            "updatedAt",
        ],
    },
    TableSpec {
        table: "doc_mount_chunks",
        order_by: "sortKey",
        id_columns: &["id", "linkId"],
        ts_columns: &["createdAt", "updatedAt"],
    },
    TableSpec {
        table: "doc_mount_blobs",
        order_by: "sha256",
        id_columns: &["id", "fileId"],
        ts_columns: &["createdAt", "updatedAt"],
    },
];

const TABLE_COUNT: usize = 5;

/// `doc_mount_chunks` plus the derived `sortKey` both sides append — chunk rows
/// have no natural key of their own. The mount name is deliberately NOT part of
/// the key here (one store), so the oracle's join is the shorter one.
fn dump_chunks_json(conn: &rusqlite::Connection) -> Value {
    conn.execute_batch(
        "CREATE TEMP VIEW IF NOT EXISTS qt_wm_chunk_dump AS \
         SELECT c.*, COALESCE(l.relativePath, '') || '#' || \
                printf('%05d', CAST(c.chunkIndex AS INTEGER)) AS sortKey \
         FROM doc_mount_chunks c \
         LEFT JOIN doc_mount_file_links l ON l.id = c.linkId",
    )
    .expect("create chunk dump view");
    let mut dump = quilltap_core::db::dump_table_json_conn(conn, "qt_wm_chunk_dump", "sortKey")
        .expect("dump doc_mount_chunks");
    dump["table"] = Value::from("doc_mount_chunks");
    dump
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/doc-mount-write-metadata.json")
}

fn normalize_table(
    dump: &mut Value,
    spec: &TableSpec,
    id_map: &mut HashMap<String, String>,
    pinned: &HashSet<String>,
) {
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
            // A PINNED timestamp is the comparand — leave it literal. Everything
            // else is minted and becomes `<ts>`.
            match obj.get(*col) {
                Some(Value::String(s)) if pinned.contains(s) => {}
                Some(v) if !v.is_null() => {
                    obj.insert((*col).to_string(), Value::String("<ts>".to_string()));
                }
                _ => {}
            }
        }
    }
}

fn normalize_all(dumps: &mut [Value; TABLE_COUNT], pinned: &HashSet<String>) {
    let mut id_map: HashMap<String, String> = HashMap::new();
    for (i, spec) in TABLES.iter().enumerate() {
        normalize_table(&mut dumps[i], spec, &mut id_map, pinned);
    }
}

/// The `results` array carries minted link ids, which cannot be compared
/// literally. Replace every uuid-shaped string with a first-seen token built
/// from the SAME map the dumps use — so `returnedLinkId` proving "the asked
/// path's link" is a cross-check against the link table, not a coincidence.
fn normalize_results(results: &mut Value, id_map: &mut HashMap<String, String>) {
    fn walk(v: &mut Value, id_map: &mut HashMap<String, String>) {
        match v {
            Value::String(s) if looks_like_uuid(s) => {
                let next = format!("RID_{}", id_map.len());
                let token = id_map.entry(s.clone()).or_insert(next).clone();
                *v = Value::String(token);
            }
            Value::Array(a) => a.iter_mut().for_each(|x| walk(x, id_map)),
            Value::Object(o) => o.values_mut().for_each(|x| walk(x, id_map)),
            _ => {}
        }
    }
    walk(results, id_map);
}

fn looks_like_uuid(s: &str) -> bool {
    s.len() == 36
        && s.as_bytes()[8] == b'-'
        && s.as_bytes()[13] == b'-'
        && s.as_bytes()[18] == b'-'
        && s.as_bytes()[23] == b'-'
        && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

#[test]
fn doc_mount_write_metadata_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_DOC_MOUNT_WRITE_METADATA") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_DOC_MOUNT_WRITE_METADATA to the oracle NDJSON (see header)."
            );
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_DOC_MOUNT_WRITE_METADATA") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_DOC_MOUNT_WRITE_METADATA to the seed fixture .db (see header)."
            );
            return;
        }
    };

    let spec_text = std::fs::read_to_string(spec_path())
        .unwrap_or_else(|e| panic!("cannot read corpus: {e}"));
    let spec: Spec = serde_json::from_str(&spec_text).expect("parse corpus");
    let pinned: HashSet<String> = spec.pinned_timestamps.iter().cloned().collect();

    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("cannot read oracle: {e}"));
    let oracle: Value = serde_json::from_str(oracle_text.trim()).expect("parse oracle dump");

    let work = std::env::temp_dir().join(format!("qt-dmwm-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    let store_id = spec.store.id.clone();
    let mut results: Vec<Value> = Vec::new();

    {
        let conn = writer.connection();
        let repo = writer.doc_mount_file_links();
        let blobs = DocMountBlobsRepository::new(conn);

        let link_at = |rel: &str| {
            repo.find_by_mount_point_and_path(&store_id, rel)
                .unwrap_or_else(|e| panic!("find link {rel}: {e}"))
                .unwrap_or_else(|| panic!("no link at {rel}"))
        };
        let blob_at = |rel: &str| {
            blobs
                .find_by_mount_point_and_path(&store_id, rel)
                .unwrap_or_else(|e| panic!("find blob {rel}: {e}"))
                .unwrap_or_else(|| panic!("no blob at {rel}"))
        };

        for (i, op) in spec.ops.iter().enumerate() {
            match op {
                Op::WriteDoc {
                    relative_path,
                    content,
                    last_modified,
                    created_at,
                } => {
                    repo.link_document_content(&LinkDocumentInput {
                        mount_point_id: store_id.clone(),
                        relative_path: relative_path.clone(),
                        file_name: relative_path.rsplit('/').next().expect("name").to_string(),
                        file_type: "markdown".to_string(),
                        content: content.clone(),
                        content_sha256: hex::encode(Sha256::digest(content.as_bytes())),
                        plain_text_length: content.encode_utf16().count() as i64,
                        file_size_bytes: content.len() as i64,
                        allow_embed: None,
                        allow_character_read: None,
                        allow_character_write: None,
                        last_modified: last_modified.clone(),
                        created_at: created_at.clone(),
                    })
                    .expect("link_document_content");
                }
                Op::WriteBlob {
                    relative_path,
                    data_hex,
                    stored_mime_type,
                    original_file_name,
                    original_mime_type,
                    description,
                    extracted_text,
                    extracted_text_sha256,
                    extraction_status,
                    last_modified,
                    created_at,
                } => {
                    let data = (0..data_hex.len())
                        .step_by(2)
                        .map(|i| u8::from_str_radix(&data_hex[i..i + 2], 16).expect("hex byte"))
                        .collect::<Vec<u8>>();
                    repo.link_blob_content(&LinkBlobInput {
                        mount_point_id: store_id.clone(),
                        relative_path: relative_path.clone(),
                        file_name: relative_path.rsplit('/').next().expect("name").to_string(),
                        file_type: None,
                        original_file_name: Some(original_file_name.clone()),
                        original_mime_type: Some(original_mime_type.clone()),
                        stored_mime_type: stored_mime_type.clone(),
                        // Advisory only — recomputed from the bytes, both sides.
                        sha256: "0".repeat(64),
                        data,
                        // The corpus's mime is `application/octet-stream`, which
                        // no codec decodes, so the chokepoint is a measured
                        // no-op here on both sides — this family is about the
                        // metadata, not the pixels.
                        normalize_images: true,
                        description: description.clone(),
                        conversion_status: None,
                        extracted_text: extracted_text.clone(),
                        extracted_text_sha256: extracted_text_sha256.clone(),
                        extraction_status: extraction_status.clone(),
                        last_modified: last_modified.clone(),
                        created_at: created_at.clone(),
                    })
                    .expect("link_blob_content");
                }
                Op::Chunk { relative_path } => {
                    quilltap_core::services::mount_index::reindex_file::reindex_single_file(
                        conn,
                        &store_id,
                        relative_path,
                        "",
                        &quilltap_core::services::mount_index::converters::RefusingTextExtractor,
                    );
                }
                Op::BindGroup {
                    source_path,
                    dest_path,
                } => {
                    let source = link_at(source_path);
                    let dest = link_at(dest_path);
                    repo.bind_link_group(&source.id, &dest.id)
                        .expect("bind_link_group");
                }
                Op::SetTimestamps {
                    relative_path,
                    last_modified,
                    created_at,
                } => {
                    let link = link_at(relative_path);
                    let moved = repo
                        .set_link_timestamps(
                            &link.id,
                            last_modified.as_deref(),
                            created_at.as_deref(),
                        )
                        .expect("set_link_timestamps");
                    results.push(json!({
                        "op": i, "kind": "set-timestamps",
                        "path": relative_path, "moved": moved,
                    }));
                }
                Op::SetTimestampsUnknown { link_id } => {
                    let moved = repo
                        .set_link_timestamps(link_id, Some("2000-01-01T00:00:00.000Z"), None)
                        .expect("set_link_timestamps");
                    results.push(json!({
                        "op": i, "kind": "set-timestamps-unknown", "moved": moved,
                    }));
                }
                Op::ResolveLinks { paths } => {
                    let ids: Vec<String> = paths.iter().map(|p| blob_at(p).link_id).collect();
                    let distinct =
                        ids.iter().collect::<HashSet<_>>().len() == ids.len();
                    results.push(json!({
                        "op": i, "kind": "resolve-links",
                        "paths": paths, "linkIds": ids, "distinct": distinct,
                    }));
                }
                Op::UpdateDescription {
                    relative_path,
                    description,
                } => {
                    let b = blob_at(relative_path);
                    let updated = blobs
                        .update_description(&b.id, description, &b.link_id)
                        .expect("update_description");
                    results.push(json!({
                        "op": i, "kind": "update-description", "path": relative_path,
                        "returnedLinkId": updated.as_ref().map(|u| u.link_id.clone()),
                        "returnedDescription": updated.as_ref().map(|u| u.description.clone()),
                    }));
                }
                Op::UpdateExtractedText {
                    relative_path,
                    extracted_text,
                    extracted_text_sha256,
                    extraction_status,
                    extraction_error,
                } => {
                    let b = blob_at(relative_path);
                    blobs
                        .update_extracted_text(
                            &b.id,
                            extracted_text.as_deref(),
                            extracted_text_sha256.as_deref(),
                            extraction_status,
                            extraction_error.as_deref(),
                            &b.link_id,
                        )
                        .expect("update_extracted_text");
                    // v5's `update_extracted_text` returns a bool, not the
                    // joined view (a pre-existing shape difference bug 157 does
                    // not move), so the returned row is read back explicitly.
                    let after = blob_at(relative_path);
                    results.push(json!({
                        "op": i, "kind": "update-extracted-text", "path": relative_path,
                        "returnedLinkId": after.link_id,
                        "returnedExtractedText": after.extracted_text,
                    }));
                }
                Op::NeedsRechunkProbe => {
                    let mut stmt = conn
                        .prepare(
                            "SELECT relativePath, chunkCount, conversionStatus \
                             FROM doc_mount_file_links WHERE mountPointId = ?1 \
                             ORDER BY relativePath",
                        )
                        .expect("prepare probe");
                    let needs: Vec<String> = stmt
                        .query_map(rusqlite::params![store_id], |row| {
                            // `chunkCount` is REAL on the generated schema
                            // (every numeric Zod column is), so it is read as
                            // f64 — the same value v4's JS sees.
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, f64>(1)?,
                                row.get::<_, String>(2)?,
                            ))
                        })
                        .expect("probe")
                        .filter_map(|r| {
                            let (path, cc, status) = r.expect("probe row");
                            (cc == 0.0 || status != "converted").then_some(path)
                        })
                        .collect();
                    results.push(json!({
                        "op": i, "kind": "needs-rechunk-probe", "needsRechunk": needs,
                    }));
                }
            }
        }
    }

    let conn = writer.connection();
    let mut ours: [Value; TABLE_COUNT] = [
        quilltap_core::db::dump_table_json_conn(conn, "doc_mount_files", "sha256").expect("files"),
        quilltap_core::db::dump_table_json_conn(conn, "doc_mount_documents", "contentSha256")
            .expect("documents"),
        quilltap_core::db::dump_table_json_conn(conn, "doc_mount_file_links", "relativePath")
            .expect("links"),
        dump_chunks_json(conn),
        quilltap_core::db::dump_table_json_conn(conn, "doc_mount_blobs", "sha256").expect("blobs"),
    ];
    drop(writer);
    let _ = std::fs::remove_file(&work);

    let keys = ["files", "documents", "links", "chunks", "blobs"];
    let mut theirs: [Value; TABLE_COUNT] = std::array::from_fn(|i| oracle[keys[i]].clone());

    normalize_all(&mut ours, &pinned);
    normalize_all(&mut theirs, &pinned);

    let mut our_results = Value::Array(results);
    let mut their_results = oracle["results"].clone();
    normalize_results(&mut our_results, &mut HashMap::new());
    normalize_results(&mut their_results, &mut HashMap::new());

    assert_eq!(
        serde_json::to_string_pretty(&their_results).unwrap(),
        serde_json::to_string_pretty(&our_results).unwrap(),
        "op results diverged (setLinkTimestamps verdicts / resolved link ids / \
         the rescan's needs-rechunk predicate)"
    );

    for (i, key) in keys.iter().enumerate() {
        assert_eq!(
            serde_json::to_string_pretty(&theirs[i]).unwrap(),
            serde_json::to_string_pretty(&ours[i]).unwrap(),
            "{key} diverged"
        );
    }

    println!(
        "OK: doc_mount write-metadata tier-2 matched oracle ({} tables + {} op results).",
        TABLE_COUNT,
        our_results.as_array().map(Vec::len).unwrap_or(0)
    );
}
