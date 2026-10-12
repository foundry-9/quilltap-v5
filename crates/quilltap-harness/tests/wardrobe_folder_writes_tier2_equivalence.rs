//! Tier-2 differential: WARDROBE FOLDER WRITES — the mount primitives of
//! `db::vault_wardrobe_public` (`list_folders_in_mount` /
//! `list_folder_paths_in_mount` / `create_folder_in_mount` /
//! `rename_folder_in_mount` / `delete_folder_in_mount` / `update_many_in_mount`
//! / `create_in_mount` / `delete_in_mount`) vs v4's REAL `wardrobe-writes.ts`
//! (P4.D279, C1 §4; v4 `4a38222e6`).
//!
//! Both sides open COPIES of the keystone fixture pair and run the SAME op
//! sequence on Ada's vault (`resolve_wardrobe_mount(A)`), mirroring
//! `wardrobe-folder-writes.test.ts`'s beats over a REAL store: the rename that
//! STAMPS the id-less `Wardrobe/Loose/no-id.md` before the store moves the
//! folder (FIRST write on the mount — every projection stamps every item, so it
//! has to come before any other write; the `Loose Scarf` keeps the id its OLD
//! path derived), the case-insensitive folder ensure, the nested create, the
//! root refusals, the batch move with one unknown id skipped, `requireAll`
//! refusing before anything is written (the borrowed group item), a `../x`
//! folder tamed to the root, a case-only rename, the sibling conflict, the 404,
//! the two reachable 409s of `deleteFolderInMount` (strays — a note and a
//! `.txt`; a same-named subfolder) and its send-up of items + subfolders. The
//! third 409 (`NOT_EMPTY`) is unreachable by construction on both sides and is
//! not posed.
//!
//! After every op: the mount's folder-row paths and link paths (LOWER-CASED),
//! `readMountItems` (id / title / folder / archivedAt — `updatedAt` is minted),
//! the op's result or thrown message, and the captured `Wardrobe …` /
//! `Stamping …` / `Recursive vault projection …` DEBUG lines (v4's keys and
//! order; the minted vault id remapped to `<ada.mount>`).
//!
//! Build the fixture + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_WKS_MAIN=/tmp/qt-wks-main.db QT_FIXTURE_WKS_MOUNT=/tmp/qt-wks-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-wardrobe-keystone-fixture.ts
//!   QT_FIXTURE_WKS_MAIN=/tmp/qt-wks-main.db QT_FIXTURE_WKS_MOUNT=/tmp/qt-wks-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/cases/wardrobe-folder-writes-tier2.ts \
//!     > /tmp/oracle-wardrobe-folder-writes.ndjson
//! Run:
//!   QT_ORACLE_WARDROBE_FOLDER_WRITES=/tmp/oracle-wardrobe-folder-writes.ndjson \
//!   QT_FIXTURE_WKS_MAIN=/tmp/qt-wks-main.db QT_FIXTURE_WKS_MOUNT=/tmp/qt-wks-mount.db \
//!     cargo test -p quilltap-harness --test wardrobe_folder_writes_tier2_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::vault_wardrobe_public::{
    create_folder_in_mount, create_in_mount, delete_folder_in_mount, delete_in_mount,
    list_folder_paths_in_mount, list_folders_in_mount, read_mount_items, rename_folder_in_mount,
    resolve_wardrobe_mount, update_many_in_mount, UpdateManyOptions, WardrobeItemPatch,
    WardrobeMount, WardrobePatch,
};
use quilltap_core::db::wardrobe_location::WardrobeLocationError;
use quilltap_core::db::Writer;
use quilltap_core::test_support::captured_with;
use quilltap_core::vault_overlay::{has_stable_wardrobe_item_id, WardrobeItem};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct SeedItem {
    id: String,
}

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "characterAId")]
    character_a_id: String,
    #[serde(rename = "missingId")]
    missing_id: String,
    items: HashMap<String, SeedItem>,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/wardrobe-keystone-tier2.json")
}

/// The messages this family pins (v4's exact wording) — the v4 case captures
/// the same set.
const PINNED_MESSAGES: &[&str] = &[
    "Wardrobe write normalised an incoming folder",
    "Wardrobe item created in folder",
    "Wardrobe items updated in folder",
    "Wardrobe folders listed",
    "Wardrobe folder ensured",
    "Wardrobe folder renamed",
    "Wardrobe folder deleted; contents sent up",
    "Wardrobe item deleted from folder",
    "Stamping ids into wardrobe files before a folder rename",
    "Wardrobe cycle check could not load peer tiers; checking the folder alone",
    "Recursive vault projection ensured subfolders",
];

/// New items this case creates (pinned ids — the v4 case uses the same).
const STRAY_SCARF_ID: &str = "d9000000-0000-4000-8000-000000000051";
const RAIN_HAT_ID: &str = "d9000000-0000-4000-8000-000000000052";

fn normalize_lines(lines: Vec<String>, ada_mount: &str) -> Vec<String> {
    lines
        .into_iter()
        .filter_map(|l| {
            let (level, rest) = l.split_once(' ')?;
            let (_target, rest) = rest.split_once(' ')?;
            if !PINNED_MESSAGES.iter().any(|m| rest.starts_with(m)) {
                return None;
            }
            Some(format!(
                "{level} {}",
                rest.replace(ada_mount, "<ada.mount>")
            ))
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
        .map(|r| r.expect("row").to_lowercase())
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
        .map(|r| r.expect("row").to_lowercase())
        .collect();
    paths.sort();
    paths
}

/// `(lower-cased relativePath, content)` of every linked document on the mount.
fn doc_contents(mount: &Connection, mount_point_id: &str) -> Vec<(String, String)> {
    let mut stmt = mount
        .prepare(
            "SELECT l.relativePath, d.content FROM doc_mount_file_links l \
             JOIN doc_mount_documents d ON d.fileId = l.fileId WHERE l.mountPointId = ?1",
        )
        .expect("prepare");
    let mut rows: Vec<(String, String)> = stmt
        .query_map([mount_point_id], |r| {
            Ok((
                r.get::<_, String>(0)?.to_lowercase(),
                r.get::<_, String>(1)?,
            ))
        })
        .expect("query")
        .map(|r| r.expect("row"))
        .collect();
    rows.sort();
    rows
}

fn item_rows(mount: &Connection, loc: &WardrobeMount) -> Value {
    let mut items = read_mount_items(mount, loc).expect("readMountItems");
    items.sort_by(|a, b| a.id.cmp(&b.id));
    Value::Array(
        items
            .iter()
            .map(|i| {
                json!({
                    "id": i.id,
                    "title": i.title,
                    "folder": i.folder,
                    "archivedAt": match &i.archived_at { Some(Some(s)) => Value::String(s.clone()), _ => Value::Null },
                })
            })
            .collect(),
    )
}

fn new_item(
    character_id: &str,
    id: &str,
    title: &str,
    folder: &str,
    types: &[&str],
) -> WardrobeItem {
    WardrobeItem {
        id: id.to_string(),
        character_id: Some(Some(character_id.to_string())),
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
        archived_at: Some(None),
        folder: folder.to_string(),
        created_at: "2026-02-01T00:00:00.000Z".to_string(),
        updated_at: "2026-02-01T00:00:00.000Z".to_string(),
    }
}

fn folder_patch(id: &str, folder: &str) -> WardrobeItemPatch {
    WardrobeItemPatch {
        id: id.to_string(),
        patch: WardrobePatch {
            folder: Some(folder.to_string()),
            ..Default::default()
        },
    }
}

fn moved(items: Vec<WardrobeItem>) -> Value {
    Value::Array(
        items
            .iter()
            .map(|i| json!({ "id": i.id, "folder": i.folder }))
            .collect(),
    )
}

type OpResult = Result<Value, WardrobeLocationError>;

#[test]
fn wardrobe_folder_writes_match_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_WARDROBE_FOLDER_WRITES") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_WARDROBE_FOLDER_WRITES to the oracle NDJSON (see header)."
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

    let scratch = tempfile::Builder::new()
        .prefix("qt-wks-folder-writes-rust-")
        .tempdir()
        .expect("tempdir");
    let main_work = scratch.path().join("main.db");
    let mount_work = scratch.path().join("mount.db");
    std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));
    let main_w = Writer::open_writable(&main_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open main: {e}"));
    let mount_w = Writer::open_writable(&mount_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open mount: {e}"));
    let main = main_w.connection();
    let mount = mount_w.connection();

    let loc = resolve_wardrobe_mount(main, Some(&spec.character_a_id))
        .expect("resolve")
        .expect("Ada has a wardrobe mount");
    let ada_mount = loc.mount_point_id.clone();
    let a = spec.character_a_id.as_str();
    let i = |key: &str| spec.items[key].id.clone();

    let mut ops: Vec<Value> = Vec::new();
    let mut op = |label: &str, f: &dyn Fn() -> OpResult, extra: Option<&dyn Fn() -> Value>| {
        let (result, lines) = captured_with(f);
        let result = match result {
            Ok(v) => json!({ "ok": v }),
            Err(e) => json!({ "threw": e.message().replace(&ada_mount, "<ada.mount>") }),
        };
        let mut record = json!({
            "label": label,
            "result": result,
            "folders": folder_paths(mount, &ada_mount),
            "docs": link_paths(mount, &ada_mount),
            "items": item_rows(mount, &loc),
        });
        if let Some(extra) = extra {
            record["extra"] = extra();
        }
        record["lines"] = Value::Array(
            normalize_lines(lines, &ada_mount)
                .into_iter()
                .map(Value::String)
                .collect(),
        );
        ops.push(record);
    };

    let list_folders =
        || -> OpResult { Ok(serde_json::to_value(list_folders_in_mount(mount, &loc)?).unwrap()) };
    let list_paths = || -> OpResult {
        let mut paths = list_folder_paths_in_mount(mount, &loc);
        paths.sort();
        Ok(json!(paths))
    };
    let create_folder = |folder: &str| -> OpResult {
        Ok(serde_json::to_value(create_folder_in_mount(mount, &loc, folder)?).unwrap())
    };
    let rename_folder = |from: &str, to: &str| -> OpResult {
        Ok(serde_json::to_value(rename_folder_in_mount(mount, &loc, from, to)?).unwrap())
    };
    let delete_folder = |folder: &str| -> OpResult {
        let d = delete_folder_in_mount(main, mount, &loc, folder)?;
        Ok(
            json!({ "movedItems": d.moved_items, "movedFolders": d.moved_folders, "parent": d.parent }),
        )
    };
    let update_many = |patches: &[WardrobeItemPatch], require_all: bool| -> OpResult {
        Ok(moved(update_many_in_mount(
            main,
            mount,
            &loc,
            patches,
            UpdateManyOptions { require_all },
        )?))
    };

    op("listFolders (initial)", &list_folders, None);
    op("listFolderPaths (initial)", &list_paths, None);
    op(
        "renameFolder Loose→Odds (stamps the id-less file first)",
        &|| rename_folder("Loose", "Odds"),
        Some(&|| {
            let docs = doc_contents(mount, &ada_mount);
            json!({
                "oddsDocs": docs
                    .iter()
                    .filter(|(p, _)| p.starts_with("wardrobe/odds/"))
                    .map(|(p, c)| json!({ "path": p, "stableId": has_stable_wardrobe_item_id(Some(c)) }))
                    .collect::<Vec<_>>(),
                "oldPathGone": !docs.iter().any(|(p, _)| p == "wardrobe/loose/no-id.md"),
            })
        }),
    );
    op(
        "createFolder formal (existing, case-insensitive)",
        &|| create_folder("formal"),
        None,
    );
    op(
        "createFolder Rainy/Gear (nested, with ancestor)",
        &|| create_folder("Rainy/Gear"),
        None,
    );
    op("createFolder '' (the root)", &|| create_folder(""), None);
    op(
        "updateMany: coat→Formal, composite→Hats: Summer, unknown skipped",
        &|| {
            update_many(
                &[
                    folder_patch(&i("rootCoat"), "Formal"),
                    folder_patch(&i("composite"), "Hats: Summer"),
                    folder_patch(&spec.missing_id, "Formal"),
                ],
                false,
            )
        },
        None,
    );
    op(
        "updateMany requireAll with a borrowed id (nothing written)",
        &|| {
            update_many(
                &[
                    folder_patch(&i("filedGown"), "Formal"),
                    folder_patch(&i("groupCloak"), "Formal"),
                    folder_patch(&i("groupCloak"), "Formal"),
                ],
                true,
            )
        },
        None,
    );
    op(
        "updateMany: a patch with no folder change (title only)",
        &|| {
            update_many(
                &[WardrobeItemPatch {
                    id: i("filedGown"),
                    patch: WardrobePatch {
                        title: Some("Evening Gown".to_string()),
                        ..Default::default()
                    },
                }],
                false,
            )
        },
        None,
    );
    op(
        "createInMount with folder '../x' (tamed to the root)",
        &|| {
            let stored = create_in_mount(
                main,
                mount,
                &loc,
                &new_item(a, STRAY_SCARF_ID, "Stray Scarf", "../x", &["accessories"]),
            )?;
            Ok(json!({
                "id": stored.id,
                "folder": stored.folder,
                "characterId": match &stored.character_id { Some(Some(s)) => Value::String(s.clone()), _ => Value::Null },
            }))
        },
        None,
    );
    op(
        "createInMount into Rainy/Gear",
        &|| {
            let stored = create_in_mount(
                main,
                mount,
                &loc,
                &new_item(a, RAIN_HAT_ID, "Rain Hat", "Rainy/Gear", &["accessories"]),
            )?;
            Ok(json!({ "id": stored.id, "folder": stored.folder }))
        },
        None,
    );
    op(
        "renameFolder Formal→formal (case-only)",
        &|| rename_folder("Formal", "formal"),
        None,
    );
    op(
        "renameFolder formal→Rainy (sibling conflict)",
        &|| rename_folder("formal", "Rainy"),
        None,
    );
    op(
        "renameFolder Nope→X (404)",
        &|| rename_folder("Nope", "X"),
        None,
    );
    op(
        "renameFolder '' (the root)",
        &|| rename_folder("", "X"),
        None,
    );
    op(
        "deleteFolder Notes (strays: a note and a .txt)",
        &|| delete_folder("Notes"),
        None,
    );
    op(
        "createFolder formal/formal (the same-name plant)",
        &|| create_folder("formal/formal"),
        None,
    );
    op(
        "deleteFolder formal (same-named subfolder)",
        &|| delete_folder("formal"),
        None,
    );
    op(
        "deleteFolder formal/formal (empty)",
        &|| delete_folder("formal/formal"),
        None,
    );
    op(
        "deleteFolder Hats: Summer (one item sent to the root)",
        &|| delete_folder("hats: summer"),
        None,
    );
    op(
        "deleteFolder Rainy (item + subfolder sent up)",
        &|| delete_folder("Rainy"),
        None,
    );
    op("deleteFolder Nope (404)", &|| delete_folder("Nope"), None);
    op("deleteFolder '' (the root)", &|| delete_folder(""), None);
    op(
        "deleteInMount stray scarf",
        &|| Ok(json!(delete_in_mount(main, mount, &loc, STRAY_SCARF_ID)?)),
        None,
    );
    op(
        "deleteInMount unknown",
        &|| Ok(json!(delete_in_mount(main, mount, &loc, &spec.missing_id)?)),
        None,
    );
    op("listFolders (final)", &list_folders, None);
    op("listFolderPaths (final)", &list_paths, None);

    let want_ops = oracle["ops"].as_array().expect("oracle ops");
    assert_eq!(
        ops.len(),
        want_ops.len(),
        "op count: v5 {} vs oracle {}",
        ops.len(),
        want_ops.len()
    );
    let mut reds: Vec<String> = Vec::new();
    for (got, want) in ops.iter().zip(want_ops) {
        assert_eq!(got["label"], want["label"], "op order drifted");
        if got != want {
            let mut parts = Vec::new();
            for key in ["result", "folders", "docs", "items", "extra", "lines"] {
                if got[key] != want[key] {
                    parts.push(format!(
                        "    {key}:\n      rust:   {}\n      oracle: {}",
                        serde_json::to_string(&got[key]).unwrap(),
                        serde_json::to_string(&want[key]).unwrap()
                    ));
                }
            }
            reds.push(format!("op {}:\n{}", want["label"], parts.join("\n")));
        }
    }
    assert!(
        reds.is_empty(),
        "{} divergent op(s):\n{}",
        reds.len(),
        reds.join("\n")
    );

    // Shape guards: the beats must have fired, not merely agreed.
    let find = |label: &str| -> &Value {
        ops.iter()
            .find(|p| p["label"] == label)
            .unwrap_or_else(|| panic!("no op {label}"))
    };
    let stamp = find("renameFolder Loose→Odds (stamps the id-less file first)");
    assert!(
        stamp["lines"].as_array().unwrap().iter().any(|l| l
            .as_str()
            .unwrap()
            .contains("Stamping ids into wardrobe files")),
        "the rename must stamp the id-less file first"
    );
    assert_eq!(
        stamp["extra"]["oddsDocs"][0]["stableId"],
        Value::Bool(true),
        "the moved item file must carry its id"
    );
    assert!(
        find("deleteFolder Notes (strays: a note and a .txt)")["result"]["threw"]
            .as_str()
            .unwrap()
            .contains("holds 2 file(s)"),
        "the strays refusal must count both files"
    );
    assert!(
        find("createInMount with folder '../x' (tamed to the root)")["lines"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l
                .as_str()
                .unwrap()
                .contains("normalised an incoming folder")),
        "the raw folder must be tamed with the DEBUG"
    );
    assert_eq!(
        find("deleteFolder Rainy (item + subfolder sent up)")["result"]["ok"],
        json!({ "movedItems": 1, "movedFolders": 1, "parent": "" })
    );

    eprintln!(
        "OK: wardrobe folder writes matched oracle ({} ops).",
        ops.len()
    );
}
