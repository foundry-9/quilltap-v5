//! P4.1b differential — the images-v2 ingest engine (v4 `lib/images-v2.ts`
//! `ingestImageBuffer`/`createFile` vs
//! `quilltap_core::services::file_storage::ingest_image_buffer_conns`).
//!
//! Both sides run the committed op sequence from `image-ingest.json` on a
//! fresh copy of the seeded two-DB fixture: fresh ingest → hash-dedup
//! linkedTo-merge → dedup no-op → (raw blob delete) → the orphaned-metadata
//! recheck re-ingest → webp passthrough → svg passthrough → gif convert. The
//! pixel seam is a deterministic passthrough on both sides (the oracle mocks
//! sharp to `mockDims` + identity encodes; the Rust side injects
//! `PassthroughPixelCodec`), so the WebP POLICY (convertToWebP's
//! mime/filename rewrite, blob-transcode's stored-as-is rules), the dedup /
//! orphan-recheck engine, the user-uploads bridge write, and the `files` row
//! marshaling are what the diff proves.
//!
//! Normalization: shared-cross-table UUID remap (any UUID substring not in the
//! pinned set collapses to a first-seen token; op records walked first, then
//! the tables in a fixed order with remap-invariant sort keys), minted ISO
//! timestamps → `<ts>` (the 2020 seed sentinel survives), `mtime` ints →
//! `<mtime>`, and the `doc_mount_points` aggregates
//! (fileCount/chunkCount/totalSizeBytes — v4's `refreshStats`, unported by
//! standing precedent) + its `updatedAt` pinned.
//!
//! Generate (Node 24, from the v4 checkout — full recipe in the oracle case
//! header `harness/oracle/cases/image-ingest.test.ts`), then run:
//!   QT_ORACLE_IMAGE_INGEST=/tmp/oracle-image-ingest.ndjson \
//!   QT_FIXTURE_INGEST_MAIN=/tmp/qt-ingest-main.db \
//!   QT_FIXTURE_INGEST_MOUNT=/tmp/qt-ingest-mount.db \
//!     cargo test -p quilltap-harness --test image_ingest_tier2_equivalence

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{json, Map, Value};

use quilltap_core::db::doc_mount_blobs::DocMountBlobsRepository;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::file_storage::{
    ingest_image_buffer_conns, IngestParams, NotConfiguredStorageBackend, PassthroughPixelCodec,
};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "uploadsMountPointId")]
    uploads_mount_point_id: String,
    #[serde(rename = "seedTs")]
    seed_ts: String,
    #[serde(rename = "mockDims")]
    mock_dims: Dims,
    #[serde(rename = "linkA")]
    link_a: String,
    #[serde(rename = "linkB")]
    link_b: String,
    payloads: HashMap<String, String>,
    ops: Vec<SpecOp>,
}

#[derive(Deserialize)]
struct Dims {
    width: i64,
    height: i64,
}

#[derive(Deserialize)]
struct SpecOp {
    op: String,
    key: String,
    payload: Option<String>,
    filename: Option<String>,
    #[serde(rename = "mimeType")]
    mime_type: Option<String>,
    #[serde(rename = "linkedTo")]
    linked_to: Option<Vec<String>>,
    target: Option<String>,
}

fn fixtures_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures")
}

/// Canonicalize a row list to one shared form: keys sorted, integral floats
/// collapsed to ints, sorted by `order_by` (string, code-unit). Applied to
/// BOTH the Rust dump rows (from `dump_table_json_conn`, already
/// hex-blob/js-number canonical) and the oracle's `canonRows` output.
fn canon_rows(rows: &[Value], order_by: &str) -> Vec<Value> {
    let mut out: Vec<Value> = rows
        .iter()
        .map(|r| {
            let obj = r.as_object().expect("row object");
            let mut sorted = Map::new();
            let mut keys: Vec<&String> = obj.keys().collect();
            keys.sort();
            for k in keys {
                let v = &obj[k];
                let canon = match v {
                    Value::Number(n) => {
                        if let Some(f) = n.as_f64() {
                            if f.fract() == 0.0 && f.abs() < 9e15 && n.as_i64().is_none() {
                                Value::from(f as i64)
                            } else {
                                v.clone()
                            }
                        } else {
                            v.clone()
                        }
                    }
                    other => other.clone(),
                };
                sorted.insert(k.clone(), canon);
            }
            Value::Object(sorted)
        })
        .collect();
    out.sort_by_key(|r| {
        r.get(order_by)
            .map(|v| match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .unwrap_or_default()
    });
    out
}

/// The shared normalizer: UUID substrings not in `pinned` collapse to
/// first-seen `<id-N>` tokens; ISO timestamps ≠ sentinel → `<ts>`.
struct Normalizer {
    pinned: Vec<String>,
    sentinel: String,
    map: HashMap<String, String>,
    uuid_re: regex::Regex,
    iso_re: regex::Regex,
}

impl Normalizer {
    fn new(pinned: Vec<String>, sentinel: String) -> Self {
        Normalizer {
            pinned,
            sentinel,
            map: HashMap::new(),
            uuid_re: regex::Regex::new(
                r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}",
            )
            .unwrap(),
            iso_re: regex::Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$").unwrap(),
        }
    }

    fn norm_string(&mut self, s: &str) -> String {
        if self.iso_re.is_match(s) {
            return if s == self.sentinel {
                s.to_string()
            } else {
                "<ts>".to_string()
            };
        }
        // Replace UUID substrings via the shared first-seen map.
        let mut out = String::new();
        let mut last = 0;
        let matches: Vec<(usize, usize, String)> = self
            .uuid_re
            .find_iter(s)
            .map(|m| (m.start(), m.end(), m.as_str().to_string()))
            .collect();
        for (start, end, text) in matches {
            out.push_str(&s[last..start]);
            if self.pinned.contains(&text) {
                out.push_str(&text);
            } else {
                let next = self.map.len();
                let token = self
                    .map
                    .entry(text)
                    .or_insert_with(|| format!("<id-{next}>"))
                    .clone();
                out.push_str(&token);
            }
            last = end;
        }
        out.push_str(&s[last..]);
        out
    }

    fn norm_value(&mut self, v: &Value) -> Value {
        match v {
            Value::String(s) => Value::String(self.norm_string(s)),
            Value::Array(a) => Value::Array(a.iter().map(|x| self.norm_value(x)).collect()),
            Value::Object(o) => {
                let mut out = Map::new();
                for (k, val) in o {
                    out.insert(k.clone(), self.norm_value(val));
                }
                Value::Object(out)
            }
            other => other.clone(),
        }
    }

    /// Normalize one table's rows with per-table pins.
    fn norm_table(&mut self, table: &str, rows: &[Value]) -> Vec<Value> {
        rows.iter()
            .map(|row| {
                let obj = row.as_object().expect("row");
                let mut out = Map::new();
                for (k, v) in obj {
                    let pinned_cell = match (table, k.as_str()) {
                        // v4's refreshStats maintains the mount aggregates (and
                        // its `_update` mints the row's updatedAt); the Rust
                        // storage primitive leaves them at the seed — the
                        // standing groups/projects/image-generation precedent.
                        ("doc_mount_points", "fileCount" | "chunkCount" | "totalSizeBytes") => {
                            Some(Value::String("<stat>".into()))
                        }
                        ("doc_mount_points", "updatedAt") => Some(Value::String("<mnt-ts>".into())),
                        // The link mtime is a bare epoch-ms int minted at write.
                        ("doc_mount_file_links", "mtime") => Some(Value::String("<mtime>".into())),
                        _ => None,
                    };
                    out.insert(k.clone(), pinned_cell.unwrap_or_else(|| self.norm_value(v)));
                }
                Value::Object(out)
            })
            .collect()
    }
}

const TABLES: &[(&str, &str, bool)] = &[
    // (table, order_by, from_mount)
    ("files", "sha256", false),
    ("doc_mount_points", "name", true),
    ("doc_mount_folders", "path", true),
    ("doc_mount_files", "sha256", true),
    ("doc_mount_blobs", "sha256", true),
    ("doc_mount_file_links", "relativePath", true),
];

#[test]
fn image_ingest_matches_oracle() {
    let (oracle_path, fixture_main, fixture_mount) = match (
        std::env::var("QT_ORACLE_IMAGE_INGEST"),
        std::env::var("QT_FIXTURE_INGEST_MAIN"),
        std::env::var("QT_FIXTURE_INGEST_MOUNT"),
    ) {
        (Ok(o), Ok(m), Ok(x)) => (o, m, x),
        _ => {
            eprintln!(
                "skipping image_ingest_matches_oracle: set QT_ORACLE_IMAGE_INGEST + \
                 QT_FIXTURE_INGEST_MAIN + QT_FIXTURE_INGEST_MOUNT to run the differential"
            );
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("image-ingest.json")).expect("spec"),
    )
    .expect("parse spec");

    // Copy the fixture pair.
    let tmp = tempfile::tempdir().expect("tempdir");
    let work_main = tmp.path().join("ingest-main.db");
    let work_mount = tmp.path().join("ingest-mount.db");
    std::fs::copy(&fixture_main, &work_main).expect("copy main");
    std::fs::copy(&fixture_mount, &work_mount).expect("copy mount");

    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: Some(work_mount.clone()),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db");

    let codec = PassthroughPixelCodec {
        width: spec.mock_dims.width,
        height: spec.mock_dims.height,
    };
    let backend = NotConfiguredStorageBackend;

    // Run the op sequence; record per-op lines shaped like the oracle's.
    let mut rust_ops: Vec<Value> = Vec::new();
    let mut entry_storage_key_by_key: HashMap<String, String> = HashMap::new();

    for op in &spec.ops {
        match op.op.as_str() {
            "ingest" => {
                let params = IngestParams {
                    buffer: spec.payloads[op.payload.as_ref().unwrap()]
                        .as_bytes()
                        .to_vec(),
                    original_filename: op.filename.clone().unwrap(),
                    mime_type: op.mime_type.clone().unwrap(),
                    user_id: spec.user_id.clone(),
                    linked_to: op.linked_to.clone().unwrap_or_default(),
                    tags: Vec::new(),
                    source: "IMPORTED".to_string(),
                    description: None,
                };
                let entry_id = db
                    .write_blocking(move |ws| {
                        let mount = ws.mount_index().expect("mount partition").connection();
                        let main = ws.main().connection();
                        let (entry, _outcome) =
                            ingest_image_buffer_conns(main, mount, &codec, &backend, &params)?;
                        Ok(entry.id)
                    })
                    .expect("ingest op");

                // Project the row into the oracle's entry record shape (via
                // the canonical whole-table dump — the harness carries no
                // direct SQL dependency).
                let files_dump = db
                    .read_main(|c| quilltap_core::db::dump_table_json_conn(c, "files", "id"))
                    .expect("files dump");
                let row = files_dump
                    .get("rows")
                    .and_then(Value::as_array)
                    .and_then(|rows| {
                        rows.iter()
                            .find(|r| r.get("id").and_then(Value::as_str) == Some(&entry_id))
                    })
                    .cloned()
                    .expect("created/returned files row present");
                let parse_json_text = |v: Option<&Value>| -> Value {
                    v.and_then(Value::as_str)
                        .and_then(|s| serde_json::from_str::<Value>(s).ok())
                        .unwrap_or_else(|| json!([]))
                };
                let record = json!({
                    "id": row.get("id").cloned().unwrap_or(Value::Null),
                    "sha256": row.get("sha256").cloned().unwrap_or(Value::Null),
                    "originalFilename": row.get("originalFilename").cloned().unwrap_or(Value::Null),
                    "mimeType": row.get("mimeType").cloned().unwrap_or(Value::Null),
                    "size": row.get("size").cloned().unwrap_or(Value::Null),
                    "width": row.get("width").cloned().unwrap_or(Value::Null),
                    "height": row.get("height").cloned().unwrap_or(Value::Null),
                    "linkedTo": parse_json_text(row.get("linkedTo")),
                    "tags": parse_json_text(row.get("tags")),
                    "source": row.get("source").cloned().unwrap_or(Value::Null),
                    "category": row.get("category").cloned().unwrap_or(Value::Null),
                    "storageKey": row.get("storageKey").cloned().unwrap_or(Value::Null),
                });
                entry_storage_key_by_key.insert(
                    op.key.clone(),
                    record
                        .get("storageKey")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                );
                rust_ops.push(json!({"kind": "op", "key": op.key, "entry": record}));
            }
            "delete_blob_of" => {
                let storage_key = entry_storage_key_by_key
                    .get(op.target.as_ref().unwrap())
                    .expect("target op storage key")
                    .clone();
                let blob_id = storage_key.split(':').nth(2).unwrap().to_string();
                let deleted = db
                    .write_blocking(move |ws| {
                        let mount = ws.mount_index().expect("mount").connection();
                        DocMountBlobsRepository::new(mount).delete(&blob_id)
                    })
                    .expect("delete blob");
                rust_ops.push(json!({"kind": "op", "key": op.key, "deletedBlob": deleted}));
            }
            other => panic!("unknown op {other}"),
        }
    }

    // Final dumps (both connections read-only off the pools).
    let mut rust_tables: Map<String, Value> = Map::new();
    for (table, order_by, from_mount) in TABLES {
        let dump = if *from_mount {
            db.read_mount_index(|c| quilltap_core::db::dump_table_json_conn(c, table, order_by))
                .expect("mount dump")
        } else {
            db.read_main(|c| quilltap_core::db::dump_table_json_conn(c, table, order_by))
                .expect("main dump")
        };
        let rows = dump
            .get("rows")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        rust_tables.insert(table.to_string(), Value::Array(rows));
    }

    // Parse the oracle NDJSON.
    let oracle_text = std::fs::read_to_string(&oracle_path).expect("oracle ndjson");
    let mut oracle_ops: Vec<Value> = Vec::new();
    let mut oracle_tables: Option<Value> = None;
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("oracle line");
        match v.get("kind").and_then(Value::as_str) {
            Some("op") => oracle_ops.push(v),
            Some("dump") => oracle_tables = Some(v.get("tables").cloned().unwrap()),
            other => panic!("unexpected oracle line kind {other:?}"),
        }
    }
    let oracle_tables = oracle_tables.expect("oracle dump line");

    // Normalize both sides with independent normalizers over the SAME walk
    // order (ops first, then the tables in TABLES order).
    let pinned = vec![
        spec.user_id.clone(),
        spec.uploads_mount_point_id.clone(),
        spec.link_a.clone(),
        spec.link_b.clone(),
    ];
    let mut rust_norm = Normalizer::new(pinned.clone(), spec.seed_ts.clone());
    let mut oracle_norm = Normalizer::new(pinned, spec.seed_ts.clone());

    assert_eq!(
        rust_ops.len(),
        oracle_ops.len(),
        "op count diverged: rust {} vs oracle {}",
        rust_ops.len(),
        oracle_ops.len()
    );
    let rust_ops_n: Vec<Value> = rust_ops.iter().map(|v| rust_norm.norm_value(v)).collect();
    let oracle_ops_n: Vec<Value> = oracle_ops
        .iter()
        .map(|v| oracle_norm.norm_value(v))
        .collect();
    for (r, o) in rust_ops_n.iter().zip(oracle_ops_n.iter()) {
        assert_eq!(r, o, "ingest op diverged\nrust:   {r}\noracle: {o}");
    }

    // Structural sanity pins (vacuous-pass guard): the final state must hold
    // four files rows (orphan re-ingest replaced the first; webp/svg/gif), four
    // content-addressed store files/blobs/links, and the `images` folder.
    let count = |t: &str| {
        rust_tables
            .get(t)
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0)
    };
    assert_eq!(count("files"), 4, "expected 4 surviving files rows");
    assert_eq!(count("doc_mount_files"), 4);
    assert_eq!(count("doc_mount_blobs"), 4);
    assert_eq!(count("doc_mount_file_links"), 4);
    assert_eq!(count("doc_mount_folders"), 1);
    assert_eq!(count("doc_mount_points"), 1);

    for (table, order_by, _) in TABLES {
        let rust_rows = rust_tables
            .get(*table)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let oracle_rows_raw = oracle_tables
            .get(*table)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let oracle_rows = canon_rows(&oracle_rows_raw, order_by);
        let rust_rows = canon_rows(&rust_rows, order_by);
        let rn = rust_norm.norm_table(table, &rust_rows);
        let on = oracle_norm.norm_table(table, &oracle_rows);
        assert_eq!(
            rn.len(),
            on.len(),
            "{table}: row count diverged (rust {} vs oracle {})",
            rn.len(),
            on.len()
        );
        for (r, o) in rn.iter().zip(on.iter()) {
            assert_eq!(r, o, "{table} row diverged\nrust:   {r}\noracle: {o}");
        }
    }
}
