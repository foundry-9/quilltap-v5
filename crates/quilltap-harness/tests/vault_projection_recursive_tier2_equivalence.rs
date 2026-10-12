//! Tier-2 differential: the RECURSIVE vault projection —
//! `db::vault_wardrobe_write::{project_array_into_vault_folder,
//! project_vault_wardrobe}` vs v4's REAL `vault-projection.ts` /
//! `wardrobe-sync.ts` (P4.D279, C1 §4; v4 `4a38222e6`).
//!
//! Both sides open COPIES of the keystone fixture pair and run the SAME ten
//! projections over the pre-seeded "Projection Scratch" store (pinned id),
//! mirroring `vault-projection-recursive.test.ts`'s beats and
//! `archived-survives-projection.test.ts`'s "never sweeps a file that is not an
//! item" over a REAL store: the wardrobe projection with a casing-only rewrite
//! (`Formal/Coat.md` ← `formal/Coat.md`, NOT deleted), per-folder `-1`
//! suffixing (`Casual/Coat.md` keeps its name; the second `formal` Coat lands on
//! `Coat-1.md`), an item titled `Instructions` beside the preserved
//! `Formal/Instructions.md`, the stale files swept at depth, the note / half-
//! written garment / `images/` caption / both `instructions.md` surviving, the
//! legacy `wardrobe.json` removed; a move + drop; generic recursive projections
//! into an EMPTY `Lab/` (the ensured empty subfolder with its DEBUG, per-folder
//! suffixing with `preserveFileNames` claimed in every folder, `owns_file`
//! narrowing the sweep, `excludeSubfolders` sparing a subtree, a casing-only
//! rewrite kept); the FLAT callers unchanged (one ensure, a slashless name
//! as-is, every stale `.md` swept; an empty projection ensures nothing); and the
//! wardrobe emptied with the non-items left standing.
//!
//! After every op: the store's folder-row paths and link paths (as stored) and
//! the captured `Recursive vault projection ensured subfolders` lines; at the
//! end the three doc-store tables for the scratch store (documents joined
//! through the store's links) in the minted-values remap form — content, sha,
//! folder parentage and the P4.6BK `chunkCount` compared exactly, timestamps
//! masked.
//!
//! Build the fixture + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_WKS_MAIN=/tmp/qt-wks-main.db QT_FIXTURE_WKS_MOUNT=/tmp/qt-wks-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-wardrobe-keystone-fixture.ts
//!   QT_FIXTURE_WKS_MAIN=/tmp/qt-wks-main.db QT_FIXTURE_WKS_MOUNT=/tmp/qt-wks-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/cases/vault-projection-recursive-tier2.ts \
//!     > /tmp/oracle-vault-projection-recursive.ndjson
//! Run:
//!   QT_ORACLE_VAULT_PROJECTION_RECURSIVE=/tmp/oracle-vault-projection-recursive.ndjson \
//!   QT_FIXTURE_WKS_MAIN=/tmp/qt-wks-main.db QT_FIXTURE_WKS_MOUNT=/tmp/qt-wks-mount.db \
//!     cargo test -p quilltap-harness --test vault_projection_recursive_tier2_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::doc_mount_documents::DocMountDocumentsRepository;
use quilltap_core::db::doc_mount_file_links::DocMountFileLinksRepository;
use quilltap_core::db::vault_wardrobe_write::{
    project_array_into_vault_folder, project_vault_wardrobe, ProjectArrayOptions,
};
use quilltap_core::db::{dump_table_json_conn, DbError, Writer};
use quilltap_core::test_support::captured_with;
use quilltap_core::vault_overlay::WardrobeItem;
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "scratchMountPointId")]
    scratch_mount_point_id: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/wardrobe-keystone-tier2.json")
}

const PINNED_MESSAGES: &[&str] = &[
    "Recursive vault projection ensured subfolders",
    "Failed to delete stale vault file during folder projection",
    "Failed to delete legacy wardrobe.json after folder projection",
];

/// `preserveFileNames: ['instructions.md'], recursive: { excludeSubfolders: ['images'] }`.
const RECURSIVE: ProjectArrayOptions<'static> = ProjectArrayOptions {
    preserve_file_names: &["instructions.md"],
    recursive: Some(&["images"]),
    owns_file: None,
};

struct TableSpec {
    oracle_key: &'static str,
    view: &'static str,
    select: &'static str,
    order_by: &'static str,
    id_columns: &'static [&'static str],
    ts_columns: &'static [&'static str],
}

const TABLES: &[TableSpec] = &[
    TableSpec {
        oracle_key: "documents",
        view: "wks_documents",
        select: "SELECT t.* FROM doc_mount_documents t JOIN doc_mount_file_links l ON l.fileId = t.fileId WHERE l.mountPointId = ",
        order_by: "contentSha256",
        id_columns: &["id", "fileId"],
        ts_columns: &["createdAt", "updatedAt"],
    },
    TableSpec {
        oracle_key: "links",
        view: "wks_links",
        select: "SELECT t.* FROM doc_mount_file_links t WHERE t.mountPointId = ",
        order_by: "relativePath",
        id_columns: &["id", "fileId", "linkGroupId", "folderId", "mountPointId"],
        ts_columns: &[
            "lastModified",
            "descriptionUpdatedAt",
            "createdAt",
            "updatedAt",
        ],
    },
    TableSpec {
        oracle_key: "folders",
        view: "wks_folders",
        select: "SELECT t.* FROM doc_mount_folders t WHERE t.mountPointId = ",
        order_by: "path",
        id_columns: &["id", "parentId", "mountPointId"],
        ts_columns: &["createdAt", "updatedAt"],
    },
];

fn normalize_lines(lines: Vec<String>) -> Vec<String> {
    lines
        .into_iter()
        .filter_map(|l| {
            let (level, rest) = l.split_once(' ')?;
            let (_target, rest) = rest.split_once(' ')?;
            if !PINNED_MESSAGES.iter().any(|m| rest.starts_with(m)) {
                return None;
            }
            Some(format!("{level} {rest}"))
        })
        .collect()
}

fn folder_paths(mount: &Connection, mount_point_id: &str) -> Vec<String> {
    let mut stmt = mount
        .prepare("SELECT path FROM doc_mount_folders WHERE mountPointId = ?1")
        .expect("prepare");
    let mut paths: Vec<String> = stmt
        .query_map([mount_point_id], |r| r.get::<_, String>(0))
        .expect("query")
        .map(|r| r.expect("row"))
        .collect();
    paths.sort();
    paths
}

fn link_paths(mount: &Connection, mount_point_id: &str) -> Vec<String> {
    let mut stmt = mount
        .prepare("SELECT relativePath FROM doc_mount_file_links WHERE mountPointId = ?1")
        .expect("prepare");
    let mut paths: Vec<String> = stmt
        .query_map([mount_point_id], |r| r.get::<_, String>(0))
        .expect("query")
        .map(|r| r.expect("row"))
        .collect();
    paths.sort();
    paths
}

fn item(
    id: &str,
    title: &str,
    folder: &str,
    types: &[&str],
    archived_at: Option<&str>,
) -> WardrobeItem {
    WardrobeItem {
        id: id.to_string(),
        character_id: Some(None),
        title: title.to_string(),
        description: Some(None),
        image_prompt: Some(None),
        types: types.iter().map(|s| s.to_string()).collect(),
        component_item_ids: Vec::new(),
        appropriateness: Some(None),
        is_default: false,
        replace: false,
        migrated_from_clothing_record_id: Some(None),
        image_file_id: None,
        archived_at: Some(archived_at.map(str::to_string)),
        folder: folder.to_string(),
        created_at: "2026-01-01T00:00:00.000Z".to_string(),
        updated_at: "2026-01-01T00:00:00.000Z".to_string(),
    }
}

fn f(n: u32) -> String {
    format!("f9000000-0000-4000-8000-{n:012}")
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn mapper(file: &String) -> (String, String) {
    (file.clone(), format!("body of {file}"))
}

fn normalize_table(dump: &mut Value, spec: &TableSpec, id_map: &mut HashMap<String, String>) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .unwrap_or_else(|| panic!("{}: dump has no rows array", spec.oracle_key));
    for row in rows.iter_mut() {
        let obj = row.as_object_mut().expect("row object");
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

#[test]
fn vault_projection_recursive_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_VAULT_PROJECTION_RECURSIVE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_VAULT_PROJECTION_RECURSIVE to the oracle NDJSON (see header)."
            );
            return;
        }
    };
    let main_fixture = match std::env::var("QT_FIXTURE_WKS_MAIN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_WKS_MAIN to the keystone main fixture .db (header).");
            return;
        }
    };
    let mount_fixture = match std::env::var("QT_FIXTURE_WKS_MOUNT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_WKS_MOUNT to the keystone mount fixture .db (header).");
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
    .expect("parse oracle");
    let s = spec.scratch_mount_point_id.as_str();

    let scratch = tempfile::Builder::new()
        .prefix("qt-wks-projection-rust-")
        .tempdir()
        .expect("tempdir");
    let main_work = scratch.path().join("main.db");
    let mount_work = scratch.path().join("mount.db");
    std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));
    let mount_w = Writer::open_writable(&mount_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open mount: {e}"));
    let mount = mount_w.connection();
    let links = DocMountFileLinksRepository::new(mount);
    let docs = DocMountDocumentsRepository::new(mount);

    let mut ops: Vec<Value> = Vec::new();
    let mut op = |label: &str, run: &dyn Fn() -> Result<(), DbError>| {
        let (result, lines) = captured_with(run);
        let result = match result {
            Ok(()) => json!({ "ok": true }),
            Err(e) => json!({ "threw": e.to_string() }),
        };
        ops.push(json!({
            "label": label,
            "result": result,
            "folders": folder_paths(mount, s),
            "links": link_paths(mount, s),
            "lines": normalize_lines(lines),
        }));
    };

    let wardrobe = |items: &[WardrobeItem]| -> Result<(), DbError> {
        project_vault_wardrobe(&links, &docs, s, s, items)
    };
    let generic = |folder: &str,
                   files: &[&str],
                   opts: ProjectArrayOptions<'_>|
     -> Result<(), DbError> {
        project_array_into_vault_folder(&links, &docs, s, folder, &strings(files), mapper, s, opts)
    };

    op(
        "wardrobe: initial (casing, per-folder suffix, preserved, note kept, sweep)",
        &|| {
            wardrobe(&[
                item(&f(1), "Coat", "formal", &["top"], None),
                item(&f(7), "Gown", "Formal/Evening", &["top", "bottom"], None),
                item(&f(8), "Gloves", "Formal/Evening", &["accessories"], None),
                item(&f(4), "Coat", "Casual", &["top"], None),
                item(&f(5), "Coat", "formal", &["top"], None),
                item(&f(6), "Instructions", "Formal", &["top"], None),
                item(
                    &f(9),
                    "Shelved hat",
                    "",
                    &["accessories"],
                    Some("2026-02-01T00:00:00.000Z"),
                ),
            ])
        },
    );
    op(
        "wardrobe: move Gown to the root, drop Gloves (old path swept, folder row stays)",
        &|| {
            wardrobe(&[
                item(&f(1), "Coat", "formal", &["top"], None),
                item(&f(7), "Gown", "", &["top", "bottom"], None),
                item(&f(4), "Coat", "Casual", &["top"], None),
                item(&f(5), "Coat", "formal", &["top"], None),
                item(&f(6), "Instructions", "Formal", &["top"], None),
                item(
                    &f(9),
                    "Shelved hat",
                    "",
                    &["accessories"],
                    Some("2026-02-01T00:00:00.000Z"),
                ),
            ])
        },
    );
    op(
        "Lab recursive: an empty target subfolder is ensured",
        &|| generic("Lab", &["Rainy/Mac.md", "Rainy/Hat.md"], RECURSIVE),
    );
    op(
        "Lab recursive: per-folder suffixing, preserved names claimed in every folder",
        &|| {
            generic(
                "Lab",
                &[
                    "Rainy/Mac.md",
                    "rainy/mac.md",
                    "Formal/Mac.md",
                    "Formal/instructions.md",
                    "Mac.md",
                    "instructions.md",
                ],
                RECURSIVE,
            )
        },
    );
    let not_mac =
        |path: &str, _content: Option<&str>| -> bool { !path.to_lowercase().ends_with("mac.md") };
    op("Lab recursive: ownsFile narrows the sweep", &|| {
        generic(
            "Lab",
            &[],
            ProjectArrayOptions {
                owns_file: Some(&not_mac),
                ..RECURSIVE
            },
        )
    });
    op("Lab recursive: excludeSubfolders spares a subtree", &|| {
        generic(
            "Lab",
            &[],
            ProjectArrayOptions {
                preserve_file_names: &["instructions.md"],
                recursive: Some(&["rainy"]),
                owns_file: None,
            },
        )
    });
    op(
        "Lab recursive: a casing-only rewrite is not deleted",
        &|| generic("Lab", &["rainy/mac.md"], RECURSIVE),
    );
    op(
        "Prompts flat: top-level only, one ensure, slashless name as-is, every stale .md swept",
        &|| {
            generic(
                "Prompts",
                &["Default.md", "default.md"],
                ProjectArrayOptions::FLAT,
            )
        },
    );
    op(
        "Prompts2 flat: an empty projection ensures nothing",
        &|| generic("Prompts2", &[], ProjectArrayOptions::FLAT),
    );
    op(
        "wardrobe: empty projection — items swept, non-items stay",
        &|| wardrobe(&[]),
    );

    // The final table state for the scratch store, through TEMP VIEWs so the
    // shared canonical dumper (`dump_table_json_conn`) shapes the rows exactly
    // as the oracle's `canonicalizeRows` does.
    let mut got: Vec<Value> = Vec::new();
    for t in TABLES {
        mount
            .execute_batch(&format!(
                "CREATE TEMP VIEW {} AS {}'{}'",
                t.view,
                t.select,
                s.replace('\'', "''")
            ))
            .expect("create view");
        got.push(dump_table_json_conn(mount, t.view, t.order_by).expect("dump view"));
    }
    let mut want: Vec<Value> = TABLES
        .iter()
        .map(|t| {
            oracle["tables"]
                .get(t.oracle_key)
                .cloned()
                .unwrap_or_else(|| panic!("oracle missing table {}", t.oracle_key))
        })
        .collect();
    let mut got_map = HashMap::new();
    let mut want_map = HashMap::new();
    for (i, t) in TABLES.iter().enumerate() {
        normalize_table(&mut got[i], t, &mut got_map);
        normalize_table(&mut want[i], t, &mut want_map);
    }

    let want_ops = oracle["ops"].as_array().expect("oracle ops");
    assert_eq!(
        ops.len(),
        want_ops.len(),
        "op count: v5 {} vs oracle {}",
        ops.len(),
        want_ops.len()
    );
    let mut reds: Vec<String> = Vec::new();
    for (g, w) in ops.iter().zip(want_ops) {
        assert_eq!(g["label"], w["label"], "op order drifted");
        if g != w {
            let mut parts = Vec::new();
            for key in ["result", "folders", "links", "lines"] {
                if g[key] != w[key] {
                    parts.push(format!(
                        "    {key}:\n      rust:   {}\n      oracle: {}",
                        serde_json::to_string(&g[key]).unwrap(),
                        serde_json::to_string(&w[key]).unwrap()
                    ));
                }
            }
            reds.push(format!("op {}:\n{}", w["label"], parts.join("\n")));
        }
    }
    for (i, t) in TABLES.iter().enumerate() {
        if got[i]["columns"] != want[i]["columns"] {
            reds.push(format!(
                "{}: column set / order\n  rust:   {}\n  oracle: {}",
                t.oracle_key, got[i]["columns"], want[i]["columns"]
            ));
        }
        if got[i]["rows"] != want[i]["rows"] {
            reds.push(format!(
                "{}: remapped row state diverged\n  rust:   {}\n  oracle: {}",
                t.oracle_key,
                serde_json::to_string(&got[i]["rows"]).unwrap(),
                serde_json::to_string(&want[i]["rows"]).unwrap()
            ));
        }
    }
    assert!(
        reds.is_empty(),
        "{} divergence(s):\n{}",
        reds.len(),
        reds.join("\n")
    );

    // Shape guards: the beats must have fired, not merely agreed.
    let find = |label: &str| -> &Value {
        ops.iter()
            .find(|p| p["label"] == label)
            .unwrap_or_else(|| panic!("no op {label}"))
    };
    let has = |op: &Value, path: &str| op["links"].as_array().unwrap().iter().any(|p| p == path);
    let initial =
        find("wardrobe: initial (casing, per-folder suffix, preserved, note kept, sweep)");
    assert!(
        has(initial, "Wardrobe/Formal/Coat.md"),
        "the casing-only rewrite must keep the stored file"
    );
    assert!(
        !has(initial, "Wardrobe/formal/Coat.md"),
        "no second spelling may appear"
    );
    assert!(
        has(initial, "Wardrobe/Formal/Coat-1.md"),
        "two Coats in one folder suffix"
    );
    assert!(
        has(initial, "Wardrobe/Casual/Coat.md"),
        "a Coat in another folder keeps its name"
    );
    assert!(
        has(initial, "Wardrobe/Formal/Instructions-1.md")
            && has(initial, "Wardrobe/Formal/Instructions.md")
    );
    assert!(
        !has(initial, "Wardrobe/Old/Hat.md") && !has(initial, "Wardrobe/Formal/Old gown.md"),
        "stale items swept at depth"
    );
    assert!(
        has(initial, "Wardrobe/Formal/notes.md")
            && has(initial, "Wardrobe/Half-done.md")
            && has(initial, "Wardrobe/images/abc/caption.md"),
        "non-items survive"
    );
    assert!(
        !has(initial, "wardrobe.json"),
        "the legacy wardrobe.json is removed"
    );
    let empty_target = find("Lab recursive: an empty target subfolder is ensured");
    assert!(
        empty_target["lines"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l.as_str().unwrap().contains("subfolderCount=1")),
        "the recursive ensure must log"
    );
    assert!(
        !find("Prompts2 flat: an empty projection ensures nothing")["folders"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p == "Prompts2"),
        "an empty flat projection ensures nothing"
    );
    let last = find("wardrobe: empty projection — items swept, non-items stay");
    assert!(has(last, "Wardrobe/Formal/notes.md") && !has(last, "Wardrobe/Gown.md"));

    eprintln!(
        "OK: recursive vault projection matched oracle ({} ops + 3 tables).",
        ops.len()
    );
}
