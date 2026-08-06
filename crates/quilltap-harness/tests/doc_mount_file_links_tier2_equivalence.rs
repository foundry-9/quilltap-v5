//! Tier-2 differential test: the document-store STORAGE PRIMITIVE
//! (`writeDatabaseDocument` = `linkDocumentContent` / `ensureLinkFolderId` +
//! the post-write `reindexSingleFile` chunk pass — P4.6BK: v5 chunks on write).
//!
//! Both sides run the SAME op sequence (from the committed spec) against the
//! SAME mount-index fixture, then the resulting SIX tables — `doc_mount_files`,
//! `doc_mount_documents`, `doc_mount_file_links`, `doc_mount_folders`,
//! `doc_mount_chunks`, `doc_mount_blobs` — are structural-diffed. Every id and
//! timestamp is minted internally, so this is the **minted-values remap form**,
//! extended across six tables:
//!
//!   - **shared id remap.** A SINGLE first-seen-token map is built by walking all
//!     six dumps in a fixed order (files → documents → links → folders →
//!     chunks → blobs, rows in natural-key order). So a cross-table FK (e.g.
//!     `doc_mount_documents.fileId` → `doc_mount_files.id`,
//!     `doc_mount_chunks.linkId` → `doc_mount_file_links.id`) verifies the
//!     RELATIONSHIP without pinning the id. `mountPointId` is the seeded store
//!     id — pinned, identical both sides — so it is NOT remapped.
//!   - **timestamps** → `<ts>` placeholder. The `createdAt == updatedAt` create
//!     invariant is intentionally NOT asserted: an op that rewrites a path
//!     upsert-updates its link (refreshing `updatedAt`/`lastModified` while
//!     preserving `createdAt`), so the two legitimately differ.
//!   - **chunk rows** carry no natural key, so both dumps append a derived
//!     `sortKey` column (`<mount name>#<link relativePath>#<zero-padded chunkIndex>`) and
//!     order by it (the P4.6BK chunk-dump convention). Chunk `content` /
//!     `tokenCount` / `headingContext` diff EXACTLY — the chunker parity proof
//!     at the write site.
//!
//! The corpus exercises: a fresh JSON + markdown write, subfolder creation,
//! dedup-by-sha (a second path with byte-identical content reuses one file + one
//! document row), link upsert-in-place (rewriting a path — which RE-chunks), and
//! the markdown frontmatter policy cascade (`character_read: false` → all
//! `allow*` = 0), verified against v4's real yaml-based `policyFromContent`.
//!
//! **[40319484] deliberate hard-link groups**, driven through v4's REAL
//! `bindLinkGroup` / `linkBlobContent` / `deleteDatabaseDocument`: the write-path
//! fan-out and the metadata it carries (and the metadata it deliberately does
//! NOT), group extension to a third location, the group-of-one NULLing on
//! unlink, the sibling re-chunk pass (chunks are keyed by `linkId`), and the
//! orphaned-content-row GC on BOTH the grouped and the ordinary ungrouped
//! rewrite — with `doc_mount_blobs` in the dump so the GC's explicit payload
//! delete is measured. See the corpus's `_comment` for the op-by-op scenarios.
//!
//! The `doc_mount_chunks` table is a PLAIN row-for-row equality. It was a
//! both-directions divergence carve-out until bug 15 (`7bcd8515`): v4 shipped
//! `reindexLinkGroupSiblings` as dead code (`queryJoined` never selected
//! `l.linkGroupId`, so the pass early-outed and hard-linked siblings served
//! stale chunks), where v5 re-chunked them. v4 has since added the `linkGroupId`
//! projection, so both sides now re-chunk the twins to the fresh revision.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-dmfl-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-doc-mount-file-links-fixture.ts
//!   QT_FIXTURE_DOC_MOUNT_FILE_LINKS=/tmp/qt-dmfl-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/doc-mount-file-links-tier2.ts \
//!     > /tmp/oracle-dmfl.ndjson
//! Run:
//!   QT_ORACLE_DOC_MOUNT_FILE_LINKS=/tmp/oracle-dmfl.ndjson \
//!   QT_FIXTURE_DOC_MOUNT_FILE_LINKS=/tmp/qt-dmfl-fixture.db \
//!     cargo test -p quilltap-harness --test doc_mount_file_links_tier2_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::doc_mount_file_links::LinkBlobInput;
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    store: Store,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
struct Store {
    id: String,
}

/// The corpus's op kinds. `write` is the original storage-primitive op; the other
/// three arrived with v4 `40319484` (deliberate hard-link groups).
#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Op {
    #[serde(rename = "write")]
    Write {
        #[serde(rename = "relativePath")]
        relative_path: String,
        content: String,
    },
    #[serde(rename = "write-blob")]
    WriteBlob {
        #[serde(rename = "relativePath")]
        relative_path: String,
        #[serde(rename = "dataHex")]
        data_hex: String,
        #[serde(rename = "storedMimeType")]
        stored_mime_type: String,
        #[serde(rename = "originalFileName")]
        original_file_name: String,
        #[serde(rename = "originalMimeType")]
        original_mime_type: String,
        description: String,
        #[serde(rename = "extractedText")]
        extracted_text: String,
    },
    #[serde(rename = "bind-group")]
    BindGroup {
        #[serde(rename = "sourcePath")]
        source_path: String,
        #[serde(rename = "destPath")]
        dest_path: String,
    },
    #[serde(rename = "delete")]
    Delete {
        #[serde(rename = "relativePath")]
        relative_path: String,
    },
}

/// Per-table (natural-key order_by, minted-id columns, timestamp columns). The
/// table walk order here is the canonical order the shared id-remap follows.
struct TableSpec {
    table: &'static str,
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
        // mountPointId is the pinned seeded store id — NOT remapped.
        // `linkGroupId` IS minted (randomUUID inside bindLinkGroup), so it
        // remaps too — which is what proves two links share ONE group and that
        // extending a group reuses the anchor's id rather than minting a second.
        id_columns: &["id", "fileId", "folderId", "linkGroupId"],
        ts_columns: &[
            "lastModified",
            "descriptionUpdatedAt",
            "createdAt",
            "updatedAt",
        ],
    },
    TableSpec {
        table: "doc_mount_folders",
        order_by: "path",
        id_columns: &["id", "parentId"],
        ts_columns: &["createdAt", "updatedAt"],
    },
    TableSpec {
        // Dumped via the custom JOIN (see `dump_chunks_json`) — `order_by` here
        // names the derived sort column both sides append.
        table: "doc_mount_chunks",
        order_by: "sortKey",
        id_columns: &["id", "linkId"],
        ts_columns: &["createdAt", "updatedAt"],
    },
    TableSpec {
        // [40319484] The blob payload, so `gc_orphaned_file_row`'s explicit
        // `DELETE FROM doc_mount_blobs` is measured rather than reasoned about
        // (the FK cascade it replaces does not exist on schema-generated tables).
        table: "doc_mount_blobs",
        order_by: "sha256",
        id_columns: &["id", "fileId"],
        ts_columns: &["createdAt", "updatedAt"],
    },
];

const TABLE_COUNT: usize = 6;

/// The P4.6BK chunk-dump convention: `doc_mount_chunks` plus a derived `sortKey`
/// column (`<mount name>#<link relativePath>#<zero-padded chunkIndex>`), rows ordered by it —
/// chunk rows have no natural key of their own. Mirrors the oracle case's dump.
/// Routed through a temp view so `dump_table_json_conn`'s canonical cell
/// rendering (JS-number collapse, hex BLOBs) applies unchanged.
fn dump_chunks_json(conn: &rusqlite::Connection) -> Value {
    conn.execute_batch(
        "CREATE TEMP VIEW IF NOT EXISTS qt_chunk_dump AS \
         SELECT c.*, COALESCE(p.name, '') || '#' || COALESCE(l.relativePath, '') || '#' || \
                printf('%05d', CAST(c.chunkIndex AS INTEGER)) AS sortKey \
         FROM doc_mount_chunks c \
         LEFT JOIN doc_mount_file_links l ON l.id = c.linkId \
         LEFT JOIN doc_mount_points p ON p.id = c.mountPointId",
    )
    .expect("create chunk dump view");
    let mut dump = quilltap_core::db::dump_table_json_conn(conn, "qt_chunk_dump", "sortKey")
        .expect("dump doc_mount_chunks");
    dump["table"] = Value::from("doc_mount_chunks");
    dump
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/doc-mount-file-links-tier2.json")
}

/// Normalize one dump's `rows` in place against the SHARED id-map: first-seen id
/// remap over the listed id columns, then timestamp placeholdering. The map is
/// shared across tables (passed by &mut) so cross-table FKs resolve to the same
/// tokens. Rows are already in natural-key order (the dump sorted them), identical
/// on both sides.
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
    }
}

/// Normalize all six dumps with one shared id-map, walking tables in `TABLES`
/// order so the first-seen token assignment is identical on both sides (links
/// before chunks, so `chunk.linkId` resolves to the link's token).
fn normalize_all(dumps: &mut [Value; TABLE_COUNT]) {
    let mut id_map: HashMap<String, String> = HashMap::new();
    for (i, spec) in TABLES.iter().enumerate() {
        normalize_table(&mut dumps[i], spec, &mut id_map);
    }
}

#[test]
fn doc_mount_file_links_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_DOC_MOUNT_FILE_LINKS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_DOC_MOUNT_FILE_LINKS to the oracle NDJSON (see header)."
            );
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_DOC_MOUNT_FILE_LINKS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_DOC_MOUNT_FILE_LINKS to the seed fixture .db (see header)."
            );
            return;
        }
    };

    let spec_text = std::fs::read_to_string(spec_path())
        .unwrap_or_else(|e| panic!("cannot read fixture spec: {e}"));
    let spec: Spec = serde_json::from_str(&spec_text).expect("parse fixture spec");

    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("cannot read oracle: {e}"));
    let oracle: Value = serde_json::from_str(oracle_text.trim()).expect("parse oracle dump");

    // Fresh copy so the shared seed fixture stays pristine.
    let work = std::env::temp_dir().join(format!("qt-dmfl-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME op sequence through the Rust port (minting our own ids/ts).
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.doc_mount_file_links();
        for op in &spec.ops {
            match op {
                Op::Write {
                    relative_path,
                    content,
                } => {
                    repo.write_database_document(&spec.store.id, relative_path, content)
                        .expect("write_database_document");
                }
                Op::WriteBlob {
                    relative_path,
                    data_hex,
                    stored_mime_type,
                    original_file_name,
                    original_mime_type,
                    description,
                    extracted_text,
                } => {
                    let data = (0..data_hex.len())
                        .step_by(2)
                        .map(|i| u8::from_str_radix(&data_hex[i..i + 2], 16).expect("hex byte"))
                        .collect::<Vec<u8>>();
                    repo.link_blob_content(&LinkBlobInput {
                        mount_point_id: spec.store.id.clone(),
                        relative_path: relative_path.clone(),
                        file_name: relative_path
                            .rsplit('/')
                            .next()
                            .expect("file name")
                            .to_string(),
                        file_type: None,
                        original_file_name: original_file_name.clone(),
                        original_mime_type: original_mime_type.clone(),
                        stored_mime_type: stored_mime_type.clone(),
                        // Advisory only — recomputed from the bytes, both sides.
                        sha256: "0".repeat(64),
                        data,
                        description: Some(description.clone()),
                        conversion_status: None,
                        extracted_text: Some(extracted_text.clone()),
                        extracted_text_sha256: None,
                        extraction_status: None,
                    })
                    .expect("link_blob_content");
                }
                Op::BindGroup {
                    source_path,
                    dest_path,
                } => {
                    let source = repo
                        .find_by_mount_point_and_path(&spec.store.id, source_path)
                        .expect("resolve source link")
                        .unwrap_or_else(|| panic!("bind-group: no link at {source_path}"));
                    let dest = repo
                        .find_by_mount_point_and_path(&spec.store.id, dest_path)
                        .expect("resolve dest link")
                        .unwrap_or_else(|| panic!("bind-group: no link at {dest_path}"));
                    repo.bind_link_group(&source.id, &dest.id)
                        .expect("bind_link_group");
                }
                Op::Delete { relative_path } => {
                    repo.delete_database_document(&spec.store.id, relative_path)
                        .expect("delete_database_document");
                }
            }
        }
    }

    let mut got: [Value; TABLE_COUNT] = std::array::from_fn(|i| {
        if TABLES[i].table == "doc_mount_chunks" {
            dump_chunks_json(writer.connection())
        } else {
            writer
                .dump_table_json(TABLES[i].table, TABLES[i].order_by)
                .unwrap_or_else(|e| panic!("dump {}: {e}", TABLES[i].table))
        }
    });
    let _ = std::fs::remove_file(&work);

    let mut want: [Value; TABLE_COUNT] = std::array::from_fn(|i| {
        oracle
            .get(oracle_key(TABLES[i].table))
            .cloned()
            .unwrap_or_else(|| panic!("oracle missing dump for {}", TABLES[i].table))
    });

    // One normalization, applied to both sides with a shared cross-table id-map.
    normalize_all(&mut got);
    normalize_all(&mut want);

    for i in 0..TABLE_COUNT {
        let table = TABLES[i].table;
        assert_eq!(got[i]["table"], want[i]["table"], "{table}: table name");
        assert_eq!(
            got[i]["columns"], want[i]["columns"],
            "{table}: column set / order"
        );
        // `doc_mount_chunks` (index 4) is a PLAIN equality since bug 15
        // (`7bcd8515`): v4 now selects `l.linkGroupId` in `queryJoined`, so
        // `reindexLinkGroupSiblings` runs and re-chunks the hard-link twins to
        // the fresh revision on BOTH sides — the former both-directions
        // divergence carve-out is retired.
        assert_eq!(
            got[i]["rows"], want[i]["rows"],
            "{table}: remapped row state diverged\n  rust:   {}\n  oracle: {}",
            got[i]["rows"], want[i]["rows"]
        );
    }

    // Sanity: the corpus produced the expected shape and the remap fired.
    let files = got[0]["rows"].as_array().expect("files rows");
    let docs = got[1]["rows"].as_array().expect("documents rows");
    let links = got[2]["rows"].as_array().expect("links rows");
    let folders = got[3]["rows"].as_array().expect("folders rows");
    let blobs = got[5]["rows"].as_array().expect("blob rows");
    // Shape, not hand-counted totals (those rot as the corpus grows): the paths
    // that survive the corpus, and the two folders the writes created.
    let live_paths: Vec<&str> = links
        .iter()
        .map(|r| r["relativePath"].as_str().expect("relativePath"))
        .collect();
    assert_eq!(
        live_paths,
        vec![
            // Case-variant writes upserted in place, so no DESCRIPTION.md /
            // twins/THIRD.md rows exist; pair/two.md and pair/three.md were
            // unlinked by the two delete ops.
            "Knowledge/atlas.md",
            "Knowledge/notes.md",
            "alias.md",
            "description.md",
            "logo-copy.bin",
            "logo.bin",
            "pair/one.md",
            "properties.json",
            "secret.md",
            "solo.md",
            "twins/left.md",
            "twins/right.md",
            "twins/third.md",
        ],
        "the surviving link paths"
    );
    assert_eq!(
        folders.len(),
        3,
        "expected 3 folder rows (Knowledge + twins + pair)"
    );

    // [40319484] The GC invariant, asserted structurally rather than by count —
    // this is the whole point of the commit. After the corpus, NO content row may
    // be unreferenced, and no document/blob payload may outlive its content row.
    let live_file_ids: std::collections::HashSet<&str> = files
        .iter()
        .map(|r| r["id"].as_str().expect("file id"))
        .collect();
    let referenced_file_ids: std::collections::HashSet<&str> = links
        .iter()
        .map(|r| r["fileId"].as_str().expect("link fileId"))
        .collect();
    assert_eq!(
        live_file_ids, referenced_file_ids,
        "orphaned content rows survived (or a referenced one was collected)"
    );
    for (label, rows) in [("document", docs), ("blob", blobs)] {
        for row in rows {
            let file_id = row["fileId"].as_str().expect("payload fileId");
            assert!(
                live_file_ids.contains(file_id),
                "{label} payload outlived its content row ({file_id})"
            );
        }
    }

    // [0a0419f5] Case-preserving: DESCRIPTION.md updated description.md IN PLACE,
    // keeping its stored casing (no DESCRIPTION.md link), and KNOWLEDGE/atlas.md
    // reused the Knowledge folder under its stored casing (no KNOWLEDGE folder).
    assert!(
        links
            .iter()
            .any(|r| r["relativePath"] == Value::String("description.md".into())),
        "description.md link should keep its casing"
    );
    assert!(
        !links
            .iter()
            .any(|r| r["relativePath"] == Value::String("DESCRIPTION.md".into())),
        "no DESCRIPTION.md case-variant link should exist"
    );
    assert!(
        links
            .iter()
            .any(|r| r["relativePath"] == Value::String("Knowledge/atlas.md".into())),
        "Knowledge/atlas.md should use the stored folder casing"
    );
    assert_eq!(
        folders[0]["path"],
        Value::String("Knowledge".into()),
        "the Knowledge folder keeps its casing"
    );

    // The dedup invariant: alias.md and the FIRST description.md write share content,
    // so two link rows reference the same file id (post-remap token).
    let alias = links
        .iter()
        .find(|r| r["relativePath"] == Value::String("alias.md".into()))
        .expect("alias.md link");
    assert!(
        alias["fileId"].as_str().unwrap().starts_with("ID_"),
        "fileId was not remapped"
    );

    // The policy cascade: secret.md (character_read:false) → all allow* = 0.
    let secret = links
        .iter()
        .find(|r| r["relativePath"] == Value::String("secret.md".into()))
        .expect("secret.md link");
    assert_eq!(secret["allowEmbed"], Value::from(0), "secret allowEmbed");
    assert_eq!(
        secret["allowCharacterRead"],
        Value::from(0),
        "secret allowCharacterRead"
    );
    assert_eq!(
        secret["allowCharacterWrite"],
        Value::from(0),
        "secret allowCharacterWrite"
    );

    // A permissive file keeps all flags 1 (sanity on the default path).
    let props = links
        .iter()
        .find(|r| r["relativePath"] == Value::String("properties.json".into()))
        .expect("properties.json link");
    assert_eq!(props["allowEmbed"], Value::from(1), "properties allowEmbed");

    // ---------------------------------------------------------------------
    // [40319484] Deliberate hard-link groups.
    // ---------------------------------------------------------------------
    let link_at = |path: &str| {
        links
            .iter()
            .find(|r| r["relativePath"] == Value::String(path.into()))
            .unwrap_or_else(|| panic!("{path} link"))
    };

    // The blob pair is still grouped: ONE group id, ONE content row — the
    // fan-out moved logo-copy.bin onto logo.bin's new bytes without it ever
    // being written.
    let logo = link_at("logo.bin");
    let logo_copy = link_at("logo-copy.bin");
    assert!(
        !logo["linkGroupId"].is_null(),
        "logo.bin should still carry its group"
    );
    assert_eq!(
        logo["linkGroupId"], logo_copy["linkGroupId"],
        "the linked blob pair should share one group id"
    );
    assert_eq!(
        logo["fileId"], logo_copy["fileId"],
        "a write through logo.bin should have repointed logo-copy.bin"
    );
    // …and the textState-null asymmetry: per-link metadata is NOT propagated.
    assert_eq!(
        logo_copy["description"],
        Value::String("the right caption".into()),
        "the sibling's own description must survive the fan-out"
    );
    assert_eq!(
        logo_copy["extractedText"],
        Value::String("right extracted text".into()),
        "the sibling's own extracted text must survive the fan-out"
    );
    assert_eq!(
        logo["description"],
        Value::String("the left caption, revised".into()),
        "the written link keeps its own new description"
    );

    // The twins are one file: ONE group id across all three, and the group was
    // EXTENDED to the third location rather than a second group being minted.
    let left = link_at("twins/left.md");
    let right = link_at("twins/right.md");
    let third = link_at("twins/third.md");
    assert!(
        !left["linkGroupId"].is_null(),
        "twins/left.md should carry a group"
    );
    assert_eq!(
        left["linkGroupId"], right["linkGroupId"],
        "the bound pair should share one group id"
    );
    assert_eq!(
        left["linkGroupId"], third["linkGroupId"],
        "linking a third location should EXTEND the group, not mint a second"
    );
    // …and all three sit on the content row written through twins/THIRD.md. That
    // revision only reaches left.md and right.md through the fan-out, so this is
    // the load-bearing "a hard link stays linked" assertion.
    assert_eq!(
        left["fileId"], third["fileId"],
        "left.md followed the write"
    );
    assert_eq!(
        right["fileId"], third["fileId"],
        "right.md followed the write"
    );
    let left_doc = docs
        .iter()
        .find(|d| d["fileId"] == left["fileId"])
        .expect("twins/left.md document row");
    assert_eq!(
        left_doc["content"],
        Value::String("# Twins\n\nThird revision, written through the third path.".into()),
        "left.md should serve the revision written through its sibling"
    );
    // The same write carried the permissive flags back over the restricted ones
    // left.md had been written with directly.
    assert_eq!(
        left["allowCharacterRead"],
        Value::from(1),
        "the fan-out should have carried the new policy flags to the sibling"
    );

    // "A group of one is not a link": pair/one.md outlived both siblings, so its
    // group id was NULLed — otherwise the survivor keeps a dangling group a
    // future link could accidentally join.
    let one = link_at("pair/one.md");
    assert!(
        one["linkGroupId"].is_null(),
        "the last survivor's linkGroupId should be NULL"
    );

    // The chunk pass (P4.6BK): every write chunked, and each link's chunkCount
    // rollup equals its actual chunk-row count (keyed by the derived sortKey
    // prefix, which embeds the link's relativePath).
    let chunks = got[4]["rows"].as_array().expect("chunk rows");
    assert!(!chunks.is_empty(), "expected chunk rows after writes");
    for link in links {
        let rel = link["relativePath"].as_str().expect("link relativePath");
        let rollup = link["chunkCount"].as_i64().expect("link chunkCount");
        let actual = chunks
            .iter()
            .filter(|c| {
                c["sortKey"]
                    .as_str()
                    .map(|k| k.contains(&format!("#{rel}#")))
                    .unwrap_or(false)
            })
            .count() as i64;
        assert_eq!(
            rollup, actual,
            "{rel}: link chunkCount rollup diverges from its chunk rows"
        );
    }

    eprintln!("OK: doc_mount_file_links storage-primitive tier-2 matched oracle (6 tables).");
}

/// Map a table name to the JSON key the oracle emits it under.
fn oracle_key(table: &str) -> &'static str {
    match table {
        "doc_mount_files" => "files",
        "doc_mount_documents" => "documents",
        "doc_mount_file_links" => "links",
        "doc_mount_folders" => "folders",
        "doc_mount_chunks" => "chunks",
        "doc_mount_blobs" => "blobs",
        other => panic!("unknown table {other}"),
    }
}
