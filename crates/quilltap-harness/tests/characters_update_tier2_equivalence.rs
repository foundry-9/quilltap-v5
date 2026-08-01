//! Tier-2 differential test: v4's `CharactersRepository.update` (Phase-2, the
//! store-backed capstone sub-unit 4a — the update vault integration).
//!
//! Both sides start from the SAME baked fixture (a character + vault created by
//! v4's real `repos.characters.create`), apply the SAME update ops, then SIX
//! tables are structural-diffed: the main slim `characters` row + the mount-index
//! store tables (`doc_mount_points` / `_folders` / `_files` / `_documents` /
//! `_file_links`). The Rust port drives
//! [`vault_character_update::update_character`] over two writers; v4 drives the
//! real `repos.characters.update` (see the oracle).
//!
//! The character id is read back from the fixture so both sides target the same
//! minted id. Minted-values remap with ONE shared id-map across all six tables
//! (FKs verify by relationship); timestamps → `<ts>`; the link `chunkCount`
//! diffs EXACTLY since P4.6BK (v5 chunks on write); `doc_mount_chunks` excluded.
//!
//! Banks: markdown routing (`identity.md` rewritten), the **properties.json
//! read-modify-write** (a `title`-only change preserves pronouns/aliases/
//! firstMessage/talkativeness), a DB-only field update (`isFavorite` true→false →
//! the slim `_update`), a `systemPrompts` reprojection (sweep the old
//! `Prompts/Default.md`, write the new one) on a managed-only update (slim row not
//! re-touched), and — P4.22 (dogfood finding #47) — the vault-properties arms:
//!
//!   - **The genuine-absence arm (equality):** `deleteProperties` removes
//!     `properties.json` outright, and the next property patch seeds the
//!     empty-managed defaults on BOTH sides (P4.D29's Epsilon arm for the
//!     character vault — a fix that refused on absence would break
//!     provisioning). Covered by the `afterOps` snapshot diff.
//!   - **The corrupt arm (A PINNED DIVERGENCE, both directions):** malformed
//!     bytes are planted through the REAL `write_database_document`
//!     (`postPlant` snapshot still matches), then the corrupt patch runs:
//!       * v5 REFUSES (`OverlayError::Unavailable`, entity label `character`,
//!         the `properties.json unparseable: <detail>` message form) and writes
//!         NOTHING — its final state is byte-diffed against the oracle's
//!         `postPlant` snapshot, and the planted bytes are asserted to still be
//!         the vault's `properties.json`.
//!       * v4 TODAY does not throw: its `readCharacterVaultProperties` collapses
//!         the parse failure into `null` and the `??`-seed CLOBBERS all six
//!         property fields to defaults. The oracle records `message: null` and
//!         the clobbered `finalProperties`, and THIS TEST ASSERTS BOTH — so the
//!         arm goes RED the moment v4 lands its own #47 fix. When that happens,
//!         reclassify: the divergence becomes an ordinary drift re-port
//!         (regenerate the oracle, compare the refusals byte-for-byte with the
//!         parse-detail tail elided per the `UNPARSEABLE_MARKER` convention).
//!
//! Generate the oracle output + fixtures (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_CHARUPD_MAIN=/tmp/qt-charupd-main.db \
//!   QT_FIXTURE_CHARUPD_MOUNT=/tmp/qt-charupd-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-characters-update-fixture.ts
//!   QT_FIXTURE_CHARUPD_MAIN=/tmp/qt-charupd-main.db \
//!   QT_FIXTURE_CHARUPD_MOUNT=/tmp/qt-charupd-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/characters-update.ts > /tmp/oracle-charupd.ndjson
//! Run:
//!   QT_ORACLE_CHARUPD=/tmp/oracle-charupd.ndjson \
//!   QT_FIXTURE_CHARUPD_MAIN=/tmp/qt-charupd-main.db \
//!   QT_FIXTURE_CHARUPD_MOUNT=/tmp/qt-charupd-mount.db \
//!     cargo test -p quilltap-harness --test characters_update_tier2_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::doc_mount_file_links::DocMountFileLinksRepository;
use quilltap_core::db::document_store_overlay::OverlayError;
use quilltap_core::db::vault_character_update::update_character;
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::{Map, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    ops: Vec<Op>,
    #[serde(rename = "plantedProperties")]
    planted_properties: String,
    #[serde(rename = "corruptPatch")]
    corrupt_patch: Map<String, Value>,
}

/// An op: `kind` absent/`"update"` → apply `patch`; `"deleteProperties"` →
/// remove the vault's `properties.json` (the genuine-absence arm).
#[derive(Deserialize)]
struct Op {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    patch: Option<Map<String, Value>>,
}

struct TableSpec {
    table: &'static str,
    oracle_key: &'static str,
    order_by: &'static str,
    id_columns: &'static [&'static str],
    ts_columns: &'static [&'static str],
    from_mount: bool,
}

const TABLES: &[TableSpec] = &[
    TableSpec {
        table: "characters",
        oracle_key: "characters",
        order_by: "name",
        id_columns: &["id", "characterDocumentMountPointId"],
        ts_columns: &["createdAt", "updatedAt"],
        from_mount: false,
    },
    TableSpec {
        table: "doc_mount_points",
        oracle_key: "points",
        order_by: "name",
        id_columns: &["id"],
        ts_columns: &["createdAt", "updatedAt", "lastScannedAt"],
        from_mount: true,
    },
    TableSpec {
        table: "doc_mount_folders",
        oracle_key: "folders",
        order_by: "path",
        id_columns: &["id", "parentId", "mountPointId"],
        ts_columns: &["createdAt", "updatedAt"],
        from_mount: true,
    },
    TableSpec {
        table: "doc_mount_files",
        oracle_key: "files",
        order_by: "sha256",
        id_columns: &["id"],
        ts_columns: &["createdAt", "updatedAt"],
        from_mount: true,
    },
    TableSpec {
        table: "doc_mount_documents",
        oracle_key: "documents",
        order_by: "contentSha256",
        id_columns: &["id", "fileId"],
        ts_columns: &["createdAt", "updatedAt"],
        from_mount: true,
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
    },
];

/// The bytes v4's clobber writes for `corruptPatch` = `{"title": "clobber probe"}`
/// over the defaults seed (`JSON.stringify(next, null, 2)`, literal key order).
/// This is the MEASURED divergence pin, direction v4: the oracle's
/// `finalProperties` must equal it exactly. When v4 lands its #47 fix the
/// planted bytes survive instead and this assertion goes red — reclassify.
const V4_CLOBBERED_BAG: &str = "{\n  \"pronouns\": null,\n  \"aliases\": [],\n  \"title\": \"clobber probe\",\n  \"firstMessage\": null,\n  \"talkativeness\": 0.5,\n  \"canChooseOutfit\": false\n}";

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/characters-update-tier2.json")
}

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

fn normalize_all(dumps: &mut [Value]) {
    let mut id_map: HashMap<String, String> = HashMap::new();
    for (i, spec) in TABLES.iter().enumerate() {
        normalize_table(&mut dumps[i], spec, &mut id_map);
    }
}

/// Dump the six tables from the two writers, in `TABLES` order.
fn dump_snapshot(main: &Writer, mount: &Writer) -> Vec<Value> {
    TABLES
        .iter()
        .map(|s| {
            let w = if s.from_mount { mount } else { main };
            w.dump_table_json(s.table, s.order_by)
                .unwrap_or_else(|e| panic!("dump {}: {e}", s.table))
        })
        .collect()
}

/// Pull a named snapshot's six tables out of the oracle line, in `TABLES` order.
fn oracle_snapshot(oracle: &Value, snapshot_key: &str) -> Vec<Value> {
    let snap = oracle
        .get(snapshot_key)
        .unwrap_or_else(|| panic!("oracle missing snapshot {snapshot_key}"));
    TABLES
        .iter()
        .map(|s| {
            snap.get(s.oracle_key).cloned().unwrap_or_else(|| {
                panic!("oracle {snapshot_key} missing dump for {}", s.oracle_key)
            })
        })
        .collect()
}

/// Normalize both sides and diff table-for-table.
fn diff_snapshot(phase: &str, mut got: Vec<Value>, mut want: Vec<Value>) {
    normalize_all(&mut got);
    normalize_all(&mut want);
    for (i, s) in TABLES.iter().enumerate() {
        assert_eq!(
            got[i]["table"], want[i]["table"],
            "{phase}/{}: table name",
            s.table
        );
        assert_eq!(
            got[i]["columns"], want[i]["columns"],
            "{phase}/{}: column set / order",
            s.table
        );
        assert_eq!(
            got[i]["rows"], want[i]["rows"],
            "{phase}/{}: remapped row state diverged\n  rust:   {}\n  oracle: {}",
            s.table, got[i]["rows"], want[i]["rows"]
        );
    }
}

/// The vault's current `properties.json` content (link → file → document).
fn read_properties_content(mount: &Writer, mount_point_id: &str) -> Option<String> {
    mount
        .connection()
        .query_row(
            "SELECT d.content FROM doc_mount_file_links l \
              JOIN doc_mount_documents d ON d.fileId = l.fileId \
             WHERE l.mountPointId = ?1 AND l.relativePath = 'properties.json'",
            [mount_point_id],
            |row| row.get::<_, String>(0),
        )
        .ok()
}

#[test]
fn characters_update_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CHARUPD") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CHARUPD to the oracle NDJSON (see header).");
            return;
        }
    };
    let main_fixture = match std::env::var("QT_FIXTURE_CHARUPD_MAIN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CHARUPD_MAIN to the main fixture .db (header).");
            return;
        }
    };
    let mount_fixture = match std::env::var("QT_FIXTURE_CHARUPD_MOUNT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CHARUPD_MOUNT to the mount fixture .db (header).");
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

    // Fresh copies so the shared baked fixtures stay pristine.
    let pid = std::process::id();
    let main_work = std::env::temp_dir().join(format!("qt-charupd-main-rust-{pid}.db"));
    let mount_work = std::env::temp_dir().join(format!("qt-charupd-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));

    let main = Writer::open_writable(&main_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open main: {e}"));
    let mount = Writer::open_writable(&mount_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open mount: {e}"));

    // Read the baked character id + vault FK (both sides target the same ones).
    let (character_id, mount_point_id): (String, String) = main
        .connection()
        .query_row(
            "SELECT id, characterDocumentMountPointId FROM characters LIMIT 1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .expect("read baked character id + vault FK");

    // Phase 1: the ops under test.
    for (i, op) in spec.ops.iter().enumerate() {
        match op.kind.as_deref() {
            Some("deleteProperties") => {
                DocMountFileLinksRepository::new(mount.connection())
                    .delete_database_document(&mount_point_id, "properties.json")
                    .unwrap_or_else(|e| panic!("op {i} deleteProperties: {e}"));
            }
            None | Some("update") => {
                let patch = op
                    .patch
                    .as_ref()
                    .unwrap_or_else(|| panic!("op {i}: update op without a patch"));
                update_character(main.connection(), mount.connection(), &character_id, patch)
                    .unwrap_or_else(|e| panic!("op {i} update_character: {e}"));
            }
            Some(other) => panic!("op {i}: unknown kind {other}"),
        }
    }
    diff_snapshot(
        "afterOps",
        dump_snapshot(&main, &mount),
        oracle_snapshot(&oracle, "afterOps"),
    );

    // Phase 2: plant the malformed bytes through the REAL write_database_document.
    DocMountFileLinksRepository::new(mount.connection())
        .write_database_document(&mount_point_id, "properties.json", &spec.planted_properties)
        .unwrap_or_else(|e| panic!("plant properties: {e}"));
    diff_snapshot(
        "postPlant",
        dump_snapshot(&main, &mount),
        oracle_snapshot(&oracle, "postPlant"),
    );

    // Phase 3: the corrupt arm — v5 MUST refuse (P4.22, dogfood finding #47).
    let refusal = match update_character(
        main.connection(),
        mount.connection(),
        &character_id,
        &spec.corrupt_patch,
    ) {
        Err(e @ OverlayError::Unavailable { .. }) => e.to_string(),
        Err(other) => panic!("corrupt arm: expected Unavailable, got: {other}"),
        Ok(_) => panic!(
            "corrupt arm: v5 accepted a patch over a present-but-unparseable \
             properties.json — the finding-#47 clobber guard is gone"
        ),
    };
    // The message head is P4.D29's shape with entity label `character`; the
    // parse-detail tail is the stated seam (serde's text) — assert non-empty,
    // don't pin its bytes.
    let head = format!(
        "Character {character_id} has no usable document store \
         (officialMountPointId={mount_point_id}): properties.json unparseable: "
    );
    assert!(
        refusal.starts_with(&head),
        "corrupt arm: refusal message head diverged\n  got:  {refusal}\n  want: {head}<parse-detail>"
    );
    assert!(
        !refusal[head.len()..].trim().is_empty(),
        "corrupt arm: empty parse detail in {refusal:?}"
    );

    // v5 wrote NOTHING: the post-refusal state is byte-equal to the oracle's
    // post-plant snapshot (the strongest "wrote nothing" proof — every table,
    // not just the planted file). Independently sensitive of the message arm:
    // a refusal that still wrote would pass the assert above and fail here.
    diff_snapshot(
        "postRefusal(v5==postPlant)",
        dump_snapshot(&main, &mount),
        oracle_snapshot(&oracle, "postPlant"),
    );
    // ... and the planted bytes are still the vault's properties.json, and no
    // defaults-seeded clobber bag exists anywhere.
    let v5_final =
        read_properties_content(&mount, &mount_point_id).expect("v5 final properties.json content");
    assert_eq!(
        v5_final, spec.planted_properties,
        "corrupt arm: v5's properties.json is no longer the planted bytes"
    );
    let clobber_rows: i64 = mount
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM doc_mount_documents WHERE content LIKE '%clobber probe%'",
            [],
            |row| row.get(0),
        )
        .expect("clobber sweep");
    assert_eq!(
        clobber_rows, 0,
        "corrupt arm: a defaults-seeded bag carrying the corrupt patch exists on the v5 side"
    );

    // ── THE DIVERGENCE PIN, v4 direction (DELIBERATE — dogfood finding #47) ──
    // v4 TODAY silently accepts the patch (message null) and clobbers the bag
    // to defaults. Both assertions go RED the moment v4 lands its own #47 fix;
    // when they do, reclassify this arm from a deliberate divergence to an
    // ordinary drift re-port (see the module header).
    let corrupt = oracle
        .get("corrupt")
        .unwrap_or_else(|| panic!("oracle missing corrupt arm"));
    assert!(
        corrupt["message"].is_null(),
        "oracle corrupt arm threw — v4 appears to have landed its finding-#47 fix; \
         RECLASSIFY this divergence as a drift re-port (message: {})",
        corrupt["message"]
    );
    assert_eq!(
        corrupt["finalProperties"].as_str(),
        Some(V4_CLOBBERED_BAG),
        "oracle corrupt arm: v4's final properties.json is not the measured clobber — \
         if it now matches the planted bytes, v4 landed its finding-#47 fix; RECLASSIFY"
    );

    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);

    eprintln!(
        "OK: characters update tier-2 matched oracle (6 tables × 3 snapshots, 2 DBs; \
         the #47 divergence pinned both directions)."
    );
}
