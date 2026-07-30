//! Tier-2 differential test: the `groups` STORE-BACKED pilot (document-store
//! overlay slice, build steps 2-3).
//!
//! Both sides run the SAME create/update sequence (from the committed spec)
//! against the SAME pair of fixtures (a MAIN db with the slim `groups` table + a
//! MOUNT-INDEX db with the store tables), then SEVEN tables are structural-diffed:
//! the main slim `groups` row plus `doc_mount_points` / `doc_mount_files` /
//! `doc_mount_documents` / `doc_mount_file_links` / `doc_mount_folders` /
//! `group_doc_mount_links`. The Rust port drives [`GroupsRepository`] over two
//! writers (one per DB); v4 drives the real `repos.groups` (see the oracle).
//!
//! This is the minted-values remap form, extended across two databases with ONE
//! shared id-map: a single first-seen-token map is built by walking all tables in
//! a fixed order (points → groups → files → documents → links → folders →
//! groupLinks, rows in natural-key order). So every cross-DB / cross-table FK —
//! `groups.officialMountPointId` → `doc_mount_points.id`, `link.fileId` →
//! `file.id`, `link.mountPointId` → the store, `groupLink.groupId` → the group —
//! verifies by RELATIONSHIP without pinning a literal id. Timestamps →
//! `<ts>`; the link `chunkCount` diffs EXACTLY since P4.6BK (v5 chunks on
//! write, matching v4's post-write `reindexSingleFile`); the
//! `doc_mount_chunks` rows are dumped and diffed too (shared remap).
//!
//! The corpus banks: the 5-step create (slim row + provision + four files +
//! overlay re-read), `properties.json` byte-exact (both keys + the empty bag),
//! a store-only update (`description`/`color` rewritten, the slim row's
//! `updatedAt` NOT bumped) with a properties read-modify-write that PRESERVES the
//! untouched `icon`, a DB-only update (`name` → slim row bumped, store untouched),
//! dedup-by-sha (`"{}"` shared by three links across two stores; `""` shared by
//! two), and orphan-on-rewrite (the pre-update content rows persist).
//!
//! Since P4.D29 (v4 `dcd9440a`) it also banks the `read_properties` refusal/seed
//! arms through THREE more stores: Gamma's `properties.json` is overwritten with
//! malformed bytes and Delta's with a schema-invalid body, and each then takes a
//! property-only patch that must be REFUSED with the planted bytes left intact
//! (the unchanged post-state IS the "wrote nothing" proof); Epsilon's file is
//! DELETED, the one arm that may seed defaults from the slim row. The thrown
//! messages are recorded oracle-side into an `errors` array and diffed here, ids
//! remapped through each side's own token map and only the parse-detail tail
//! elided (see `UNPARSEABLE_MARKER`).
//!
//! Generate the oracle output + fixtures (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_GROUPS_MAIN=/tmp/qt-groups-main.db \
//!   QT_FIXTURE_GROUPS_MOUNT=/tmp/qt-groups-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-groups-tier2-fixture.ts
//!   QT_FIXTURE_GROUPS_MAIN=/tmp/qt-groups-main.db \
//!   QT_FIXTURE_GROUPS_MOUNT=/tmp/qt-groups-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/groups-tier2.ts > /tmp/oracle-groups.ndjson
//! Run:
//!   QT_ORACLE_GROUPS=/tmp/oracle-groups.ndjson \
//!   QT_FIXTURE_GROUPS_MAIN=/tmp/qt-groups-main.db \
//!   QT_FIXTURE_GROUPS_MOUNT=/tmp/qt-groups-mount.db \
//!     cargo test -p quilltap-harness --test groups_tier2_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::doc_mount_file_links::DocMountFileLinksRepository;
use quilltap_core::db::groups::{
    find_official_mount_point_id_raw, GroupCreateInput, GroupCreateOptions, GroupsRepository,
};
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::{json, Map, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Op {
    #[serde(rename = "create")]
    Create { label: String, input: CreateInput },
    #[serde(rename = "update")]
    Update {
        label: String,
        patch: Map<String, Value>,
    },
    /// Overwrite `properties.json` with raw bytes, bypassing the overlay's
    /// serializer (the corrupt-store arms). v4 drives `writeDatabaseDocument`.
    #[serde(rename = "plantProperties")]
    PlantProperties { label: String, content: String },
    /// Remove `properties.json` entirely — v4's genuine-absence (NOT_FOUND) arm,
    /// the ONLY one that may seed defaults.
    #[serde(rename = "deleteProperties")]
    DeleteProperties { label: String },
    /// Run the update expecting a refusal; the message is recorded and diffed.
    #[serde(rename = "updateExpectError")]
    UpdateExpectError {
        label: String,
        patch: Map<String, Value>,
    },
}

#[derive(Deserialize)]
struct CreateInput {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    instructions: Option<String>,
    #[serde(default)]
    state: Option<Value>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    icon: Option<String>,
}

/// Per-table normalization spec. `from_mount` = read from the mount-index writer
/// (else the main writer); `oracle_key` = the JSON key the oracle emits it under.
/// The slice order here is the canonical walk order for the shared id-remap.
struct TableSpec {
    table: &'static str,
    oracle_key: &'static str,
    order_by: &'static str,
    id_columns: &'static [&'static str],
    ts_columns: &'static [&'static str],
    from_mount: bool,
    pin_chunk_count: bool,
}

const TABLES: &[TableSpec] = &[
    TableSpec {
        table: "doc_mount_points",
        oracle_key: "points",
        order_by: "name",
        id_columns: &["id"],
        ts_columns: &["createdAt", "updatedAt", "lastScannedAt"],
        from_mount: true,
        pin_chunk_count: false,
    },
    TableSpec {
        table: "groups",
        oracle_key: "groups",
        order_by: "name",
        id_columns: &["id", "officialMountPointId"],
        ts_columns: &["createdAt", "updatedAt"],
        from_mount: false,
        pin_chunk_count: false,
    },
    TableSpec {
        table: "doc_mount_files",
        oracle_key: "files",
        order_by: "sha256",
        id_columns: &["id"],
        ts_columns: &["createdAt", "updatedAt"],
        from_mount: true,
        pin_chunk_count: false,
    },
    TableSpec {
        table: "doc_mount_documents",
        oracle_key: "documents",
        order_by: "contentSha256",
        id_columns: &["id", "fileId"],
        ts_columns: &["createdAt", "updatedAt"],
        from_mount: true,
        pin_chunk_count: false,
    },
    TableSpec {
        table: "doc_mount_file_links",
        oracle_key: "links",
        order_by: "relativePath",
        id_columns: &["id", "fileId", "folderId", "mountPointId"],
        ts_columns: &[
            "lastModified",
            "descriptionUpdatedAt",
            "createdAt",
            "updatedAt",
        ],
        from_mount: true,
        pin_chunk_count: false, // P4.6BK: v5 chunks on write — chunkCount now diffs exactly
    },
    TableSpec {
        table: "doc_mount_folders",
        oracle_key: "folders",
        order_by: "path",
        id_columns: &["id", "parentId", "mountPointId"],
        ts_columns: &["createdAt", "updatedAt"],
        from_mount: true,
        pin_chunk_count: false,
    },
    TableSpec {
        table: "group_doc_mount_links",
        oracle_key: "groupLinks",
        order_by: "createdAt",
        id_columns: &["id", "groupId", "mountPointId"],
        ts_columns: &["createdAt", "updatedAt"],
        from_mount: true,
        pin_chunk_count: false,
    },
    TableSpec {
        // Dumped via `dump_chunks_json` (custom JOIN adds the derived sortKey).
        // Walked LAST so `linkId` resolves to the link's already-assigned token.
        table: "doc_mount_chunks",
        oracle_key: "chunks",
        order_by: "sortKey",
        id_columns: &["id", "linkId", "mountPointId"],
        ts_columns: &["createdAt", "updatedAt"],
        from_mount: true,
        pin_chunk_count: false,
    },
];

/// The P4.6BK chunk-dump convention: `doc_mount_chunks` plus a derived `sortKey`
/// column (`<mount name>#<link relativePath>#<zero-padded chunkIndex>`), rows
/// ordered by it — chunk rows have no natural key of their own. Routed through a
/// temp view so `dump_table_json_conn`'s canonical cell rendering applies.
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
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/groups-tier2.json")
}

/// Remap id columns (shared map), placeholder timestamps, pin `chunkCount`.
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

/// Normalize every table and return the shared id-map, so the recorded error
/// messages can be remapped with the SAME first-seen tokens.
fn normalize_all(dumps: &mut [Value]) -> HashMap<String, String> {
    let mut id_map: HashMap<String, String> = HashMap::new();
    for (i, spec) in TABLES.iter().enumerate() {
        normalize_table(&mut dumps[i], spec, &mut id_map);
    }
    id_map
}

/// The one seam the thrown messages cannot cross: the parse detail. v4's tail is
/// V8's `JSON.parse` text or a Zod issue array; v5's is serde's. Everything up to
/// and including `properties.json unparseable: ` IS compared byte-for-byte (that
/// is v4's `ProjectStoreUnavailableError`/`GroupStoreUnavailableError` message,
/// ids remapped); the tail is replaced with a placeholder and separately asserted
/// non-empty on both sides. This is the standing "`is not valid JSON:` wording"
/// seam, not a normalization of convenience.
const UNPARSEABLE_MARKER: &str = "properties.json unparseable: ";

/// Remap minted ids to the shared tokens, then elide the parse-detail tail.
/// Panics if a message claims `unparseable` with an empty detail (which would
/// make the placeholder hide a real difference).
fn normalize_error_message(message: &str, id_map: &HashMap<String, String>, side: &str) -> String {
    let mut out = message.to_string();
    for (raw, token) in id_map {
        if out.contains(raw.as_str()) {
            out = out.replace(raw.as_str(), token);
        }
    }
    if let Some(idx) = out.find(UNPARSEABLE_MARKER) {
        let head_end = idx + UNPARSEABLE_MARKER.len();
        assert!(
            !out[head_end..].trim().is_empty(),
            "{side}: empty parse detail in {out:?}"
        );
        out.truncate(head_end);
        out.push_str("<parse-detail>");
    }
    out
}

/// `[{label, message}]` → normalized, order preserved (the op order is the corpus
/// order on both sides).
fn normalize_errors(errors: &Value, id_map: &HashMap<String, String>, side: &str) -> Vec<Value> {
    errors
        .as_array()
        .unwrap_or_else(|| panic!("{side}: errors is not an array: {errors}"))
        .iter()
        .map(|e| {
            let label = e["label"].clone();
            let message = match e.get("message") {
                Some(Value::String(m)) => Value::String(normalize_error_message(m, id_map, side)),
                // `null` = the update did NOT throw. Kept as-is so a side that
                // silently succeeds where the other refuses goes RED.
                _ => Value::Null,
            };
            json!({ "label": label, "message": message })
        })
        .collect()
}

#[test]
fn groups_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_GROUPS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_GROUPS to the oracle NDJSON (see header).");
            return;
        }
    };
    let main_fixture = match std::env::var("QT_FIXTURE_GROUPS_MAIN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_GROUPS_MAIN to the main fixture .db (see header).");
            return;
        }
    };
    let mount_fixture = match std::env::var("QT_FIXTURE_GROUPS_MOUNT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_GROUPS_MOUNT to the mount-index fixture .db (see header)."
            );
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle: Value = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle dump");

    // Fresh copies so the shared seed fixtures stay pristine.
    let pid = std::process::id();
    let main_work = std::env::temp_dir().join(format!("qt-groups-main-rust-{pid}.db"));
    let mount_work = std::env::temp_dir().join(format!("qt-groups-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));

    let main = Writer::open_writable(&main_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open main: {e}"));
    let mount = Writer::open_writable(&mount_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open mount: {e}"));

    let mut got_errors: Vec<Value> = Vec::new();
    {
        let repo = GroupsRepository::new(main.connection(), mount.connection());
        let mut id_by_label: HashMap<String, String> = HashMap::new();
        // The label's official store, read RAW — a broken overlay must not block
        // it (v4 reads it through `groups.findByIdRaw`).
        let mount_point_id = |id_by_label: &HashMap<String, String>, label: &String| -> String {
            let id = id_by_label
                .get(label)
                .unwrap_or_else(|| panic!("op references unknown label {label}"));
            find_official_mount_point_id_raw(main.connection(), id)
                .unwrap_or_else(|e| panic!("raw mount lookup {label}: {e}"))
                .flatten()
                .unwrap_or_else(|| panic!("group {label} has no officialMountPointId"))
        };
        for op in &spec.ops {
            match op {
                Op::PlantProperties { label, content } => {
                    let mp = mount_point_id(&id_by_label, label);
                    DocMountFileLinksRepository::new(mount.connection())
                        .write_database_document(&mp, "properties.json", content)
                        .unwrap_or_else(|e| panic!("plant {label}: {e}"));
                }
                Op::DeleteProperties { label } => {
                    let mp = mount_point_id(&id_by_label, label);
                    DocMountFileLinksRepository::new(mount.connection())
                        .delete_database_document(&mp, "properties.json")
                        .unwrap_or_else(|e| panic!("delete properties {label}: {e}"));
                }
                Op::UpdateExpectError { label, patch } => {
                    let id = id_by_label
                        .get(label)
                        .unwrap_or_else(|| panic!("op references unknown label {label}"));
                    let message = match repo.update(id, patch) {
                        Ok(_) => Value::Null,
                        Err(e) => Value::String(e.to_string()),
                    };
                    got_errors.push(json!({ "label": label, "message": message }));
                }
                Op::Create { label, input } => {
                    let created = repo
                        .create(
                            &GroupCreateInput {
                                name: input.name.clone(),
                                description: input.description.clone(),
                                instructions: input.instructions.clone(),
                                state: input
                                    .state
                                    .clone()
                                    .unwrap_or_else(|| Value::Object(Map::new())),
                                color: input.color.clone(),
                                icon: input.icon.clone(),
                            },
                            &GroupCreateOptions::default(),
                        )
                        .unwrap_or_else(|e| panic!("create {label}: {e}"));
                    let id = created["id"].as_str().expect("created id").to_string();
                    id_by_label.insert(label.clone(), id);
                }
                Op::Update { label, patch } => {
                    let id = id_by_label
                        .get(label)
                        .unwrap_or_else(|| panic!("update references unknown label {label}"));
                    repo.update(id, patch)
                        .unwrap_or_else(|e| panic!("update {label}: {e}"));
                }
            }
        }
    }

    let mut got: Vec<Value> = TABLES
        .iter()
        .map(|s| {
            let w = if s.from_mount { &mount } else { &main };
            if s.table == "doc_mount_chunks" {
                dump_chunks_json(w.connection())
            } else {
                w.dump_table_json(s.table, s.order_by)
                    .unwrap_or_else(|e| panic!("dump {}: {e}", s.table))
            }
        })
        .collect();
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);

    let mut want: Vec<Value> = TABLES
        .iter()
        .map(|s| {
            oracle
                .get(s.oracle_key)
                .cloned()
                .unwrap_or_else(|| panic!("oracle missing dump for {}", s.oracle_key))
        })
        .collect();

    let got_id_map = normalize_all(&mut got);
    let want_id_map = normalize_all(&mut want);

    // The P4.D29 refusal arms: v4's thrown message vs v5's, ids remapped through
    // each side's OWN first-seen map and only the parse-detail tail elided.
    let got_errs = normalize_errors(&Value::Array(got_errors.clone()), &got_id_map, "rust");
    let want_errs = normalize_errors(
        oracle
            .get("errors")
            .unwrap_or_else(|| panic!("oracle has no `errors` array — regenerate it (see header)")),
        &want_id_map,
        "oracle",
    );
    assert_eq!(
        got_errs, want_errs,
        "refusal-arm messages diverged\n  rust:   {got_errs:?}\n  oracle: {want_errs:?}"
    );

    for (i, s) in TABLES.iter().enumerate() {
        assert_eq!(got[i]["table"], want[i]["table"], "{}: table name", s.table);
        assert_eq!(
            got[i]["columns"], want[i]["columns"],
            "{}: column set / order",
            s.table
        );
        assert_eq!(
            got[i]["rows"], want[i]["rows"],
            "{}: remapped row state diverged\n  rust:   {}\n  oracle: {}",
            s.table, got[i]["rows"], want[i]["rows"]
        );
    }

    // Sanity: the corpus produced the expected shape.
    let rows = |key: &str| {
        let i = TABLES.iter().position(|t| t.oracle_key == key).unwrap();
        got[i]["rows"].as_array().unwrap().clone()
    };
    assert_eq!(rows("groups").len(), 5, "5 group rows");
    assert_eq!(rows("points").len(), 5, "5 mount-point rows");
    assert_eq!(rows("files").len(), 15, "15 deduped file rows");
    assert_eq!(rows("documents").len(), 16, "16 document rows");
    assert_eq!(rows("links").len(), 20, "20 link rows (5 stores × 4 files)");
    assert_eq!(rows("folders").len(), 0, "0 folders (all files top-level)");
    assert_eq!(rows("groupLinks").len(), 5, "5 group→store links");

    // The properties read-modify-write PRESERVED the untouched `icon` while
    // changing `color` (Alpha's final properties.json).
    let docs = rows("documents");
    let has_doc = |content: &str| {
        docs.iter()
            .any(|d| d["content"] == Value::String(content.into()))
    };
    let final_props = "{\n  \"color\": \"#445566\",\n  \"icon\": \"star\"\n}";
    assert!(
        has_doc(final_props),
        "RMW-preserved properties.json not found; documents: {docs:?}"
    );
    // The empty-bag store (Beta) wrote `{}` and the empty description.md (`""`).
    assert!(has_doc("{}"), "empty properties.json `{{}}` not found");
    assert!(has_doc(""), "empty markdown file `\"\"` not found");

    // ── P4.D29: the refusal arms wrote NOTHING ────────────────────────────
    // Gamma's malformed bytes and Delta's schema-invalid bytes are still the
    // stores' `properties.json` after their patches were refused. Before
    // `dcd9440a` both would have been REPLACED by a defaults-seeded bag
    // carrying only the one patched key (`{"color":"#010203"}` /
    // `{"icon":"moon"}`) — silently flattening everything else.
    assert!(
        has_doc("{ not json"),
        "gamma's planted malformed properties.json was overwritten — the refusal did not hold"
    );
    assert!(
        has_doc("{\n  \"color\": 123,\n  \"icon\": \"cog\"\n}"),
        "delta's planted schema-invalid properties.json was overwritten"
    );
    assert!(
        !has_doc("{\n  \"color\": \"#010203\"\n}"),
        "gamma's refused patch reached the store as a defaults-seeded bag"
    );
    assert!(
        !has_doc("{\n  \"icon\": \"moon\"\n}"),
        "delta's refused patch reached the store as a defaults-seeded bag"
    );
    // Epsilon's `properties.json` was genuinely ABSENT — the one arm that may
    // seed from the slim row (`{}`), so its icon legitimately does not survive.
    assert!(
        has_doc("{\n  \"color\": \"#0e0e0e\"\n}"),
        "the genuine-absence seed arm did not write its defaults-seeded bag"
    );

    eprintln!(
        "OK: groups store-backed tier-2 matched oracle (8 tables, 2 DBs, {} refusal arms).",
        got_errs.len()
    );
}

/// The keystone asymmetry (v4 `applyOverlayOne` THROWS, `applyOverlay` DROPS): a
/// group with a null `officialMountPointId` (or a store missing `properties.json`)
/// is unavailable. `find_by_id` must error; `find_all` must drop it. This is the
/// engine's documented failure mode mirrored from v4; it is a Rust-side
/// behavioral bank (the write-path differential above is the oracle-verified
/// part) and needs only the fixtures — a storeless row, against the empty store.
#[test]
fn groups_keystone_throw_vs_drop() {
    let (Ok(main_fixture), Ok(mount_fixture), Ok(pepper)) = (
        std::env::var("QT_FIXTURE_GROUPS_MAIN"),
        std::env::var("QT_FIXTURE_GROUPS_MOUNT"),
        // The fixtures are keyed by the committed test pepper.
        Ok::<_, std::env::VarError>("ZpjI5jcj5CYsyBA6zPH90G4frQEbv2WsAhERvEKrjJk=".to_string()),
    ) else {
        eprintln!("SKIP: set QT_FIXTURE_GROUPS_MAIN/MOUNT (see header).");
        return;
    };

    let pid = std::process::id();
    let main_work = std::env::temp_dir().join(format!("qt-groups-keystone-main-{pid}.db"));
    let mount_work = std::env::temp_dir().join(format!("qt-groups-keystone-mount-{pid}.db"));
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    std::fs::copy(&main_fixture, &main_work).unwrap();
    std::fs::copy(&mount_fixture, &mount_work).unwrap();

    let main = Writer::open_writable(&main_work, &pepper).unwrap();
    let mount = Writer::open_writable(&mount_work, &pepper).unwrap();

    // A storeless group: null officialMountPointId (the keystone-broken state).
    main.connection()
        .execute(
            "INSERT INTO groups (id, name, officialMountPointId, createdAt, updatedAt) \
             VALUES ('ghost-id', 'Ghost', NULL, '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
            [],
        )
        .unwrap();

    let repo = GroupsRepository::new(main.connection(), mount.connection());

    // find_by_id THROWS (Unavailable).
    let one = repo.find_by_id("ghost-id");
    assert!(
        one.is_err(),
        "find_by_id on a storeless group should error, got {one:?}"
    );

    // find_all DROPS it (no error, row absent).
    let all = repo.find_all().expect("find_all should not error");
    assert!(
        !all.iter()
            .any(|g| g["id"] == Value::String("ghost-id".into())),
        "find_all should drop the storeless group"
    );

    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    eprintln!("OK: groups keystone throw-vs-drop asymmetry holds.");
}
