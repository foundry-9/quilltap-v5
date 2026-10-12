//! Tier-2 differential: WARDROBE LOCATIONS — `db::wardrobe_location::
//! {resolve_wardrobe_location, group_locations_for_character, location_key}`
//! and the location's read / folder / write-refusal operations vs v4's REAL
//! `lib/wardrobe/location.ts` (P4.D279, C1 §2; v4 `4a38222e6`).
//!
//! Both sides open COPIES of the keystone fixture pair (the round's ONE shared
//! four-tier seed, `build-wardrobe-keystone-fixture.ts`) and run the SAME probe
//! matrix — four scopes × ensure on/off, the ownership gate both ways, the
//! archived character (reads, every write refuses with the tombstone), Quilltap
//! General, a provisioned project with NO `Wardrobe/` (R-C: the read probe finds
//! the store and reads `[]` WITHOUT writing; `ensure:true` creates exactly the
//! folder), a project whose `officialMountPointId` is NULL (both arms THROW the
//! overlay's unavailability sentence — through real repositories the unit test's
//! `officialMountPointId: null` owner never reaches the ensure; nothing is
//! provisioned), the provisioned project and group, missing / empty-string /
//! null ids, `groupLocationsForCharacter` and `locationKey`. Every probe
//! compares the location shape (minted mount ids remapped to owner tokens), what
//! it read, the `doc_mount_points` row count before/after, the bare project's
//! folder rows, and the captured `[WardrobeLocation] Resolved wardrobe location`
//! DEBUG line (one per resolve that RETURNS; a throw escapes before it).
//!
//! Build the fixture + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_WKS_MAIN=/tmp/qt-wks-main.db QT_FIXTURE_WKS_MOUNT=/tmp/qt-wks-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-wardrobe-keystone-fixture.ts
//!   QT_FIXTURE_WKS_MAIN=/tmp/qt-wks-main.db QT_FIXTURE_WKS_MOUNT=/tmp/qt-wks-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/cases/wardrobe-location-tier2.ts \
//!     > /tmp/oracle-wardrobe-location.ndjson
//! Run:
//!   QT_ORACLE_WARDROBE_LOCATION=/tmp/oracle-wardrobe-location.ndjson \
//!   QT_FIXTURE_WKS_MAIN=/tmp/qt-wks-main.db QT_FIXTURE_WKS_MOUNT=/tmp/qt-wks-mount.db \
//!     cargo test -p quilltap-harness --test wardrobe_location_tier2_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::vault_wardrobe_public::{WardrobePatch, WardrobeScope};
use quilltap_core::db::wardrobe_location::{
    group_locations_for_character, location_key, resolve_wardrobe_location,
    ResolveWardrobeLocationOptions, WardrobeLocation,
};
use quilltap_core::db::Writer;
use quilltap_core::test_support::captured_with;
use quilltap_core::vault_overlay::WardrobeItem;
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
    #[serde(rename = "userAId")]
    user_a_id: String,
    #[serde(rename = "userBId")]
    user_b_id: String,
    #[serde(rename = "characterAId")]
    character_a_id: String,
    #[serde(rename = "characterBId")]
    character_b_id: String,
    #[serde(rename = "archivedCharacterId")]
    archived_character_id: String,
    #[serde(rename = "generalMountPointId")]
    general_mount_point_id: String,
    #[serde(rename = "bareProjectId")]
    bare_project_id: String,
    #[serde(rename = "nullProjectId")]
    null_project_id: String,
    #[serde(rename = "fullProjectId")]
    full_project_id: String,
    #[serde(rename = "groupId")]
    group_id: String,
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
    "[WardrobeLocation] Resolved wardrobe location",
    "[SharedWardrobe] Failed to ensure Wardrobe folder",
];

/// Owner-derived tokens for the minted mount ids (the v4 case builds the same
/// map from the same rows).
struct Tokens(Vec<(String, String)>);

impl Tokens {
    fn remap(&self, s: &str) -> String {
        let mut out = s.to_string();
        for (minted, token) in &self.0 {
            out = out.replace(minted, token);
        }
        out
    }
}

/// A captured v5 line in the v4 case's rendering: `LEVEL message k=v …` (the
/// capture rig renders `LEVEL target message k=v …`; the target is v5's alone).
fn normalize_lines(lines: Vec<String>, tokens: &Tokens) -> Vec<String> {
    lines
        .into_iter()
        .filter_map(|l| {
            let (level, rest) = l.split_once(' ')?;
            let (_target, rest) = rest.split_once(' ')?;
            if !PINNED_MESSAGES.iter().any(|m| rest.starts_with(m)) {
                return None;
            }
            Some(format!("{level} {}", tokens.remap(rest)))
        })
        .collect()
}

fn opt_str(v: &Option<Option<String>>) -> Value {
    match v {
        Some(Some(s)) => Value::String(s.clone()),
        _ => Value::Null,
    }
}

fn item_shape(mut items: Vec<WardrobeItem>) -> Value {
    items.sort_by(|a, b| a.id.cmp(&b.id));
    Value::Array(
        items
            .iter()
            .map(|i| {
                json!({
                    "id": i.id,
                    "title": i.title,
                    "folder": i.folder,
                    "archivedAt": opt_str(&i.archived_at),
                    "characterId": opt_str(&i.character_id),
                    "componentItemIds": i.component_item_ids,
                })
            })
            .collect(),
    )
}

fn location_shape(loc: Option<&WardrobeLocation>, tokens: &Tokens) -> Value {
    match loc {
        None => Value::Null,
        Some(l) => json!({
            "scope": l.scope.as_str(),
            "id": l.id,
            "mountPointId": tokens.remap(&l.mount_point_id),
            "characterId": l.character_id,
            "origin": serde_json::to_value(&l.origin).expect("origin"),
            "key": tokens.remap(&l.key()),
        }),
    }
}

fn ok<T: serde::Serialize>(v: T) -> Value {
    json!({ "ok": v })
}
fn threw(message: String, tokens: &Tokens) -> Value {
    json!({ "threw": tokens.remap(&message) })
}

fn point_count(mount: &Connection) -> i64 {
    mount
        .query_row("SELECT COUNT(*) FROM doc_mount_points", [], |r| r.get(0))
        .expect("count doc_mount_points")
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

fn mount_of(main: &Connection, table: &str, column: &str, id: &str) -> String {
    main.query_row(
        &format!("SELECT \"{column}\" FROM \"{table}\" WHERE id = ?1"),
        [id],
        |r| r.get::<_, Option<String>>(0),
    )
    .unwrap_or_else(|e| panic!("{table} {id}: {e}"))
    .unwrap_or_else(|| panic!("{table} {id} has no {column}"))
}

fn never_written(character_id: &str, id: &str) -> WardrobeItem {
    WardrobeItem {
        id: id.to_string(),
        character_id: Some(Some(character_id.to_string())),
        title: "Never Written".to_string(),
        description: Some(None),
        image_prompt: Some(None),
        types: vec!["top".to_string()],
        component_item_ids: Vec::new(),
        appropriateness: Some(None),
        is_default: false,
        replace: false,
        migrated_from_clothing_record_id: Some(None),
        image_file_id: None,
        archived_at: Some(None),
        folder: String::new(),
        created_at: "2026-02-01T00:00:00.000Z".to_string(),
        updated_at: "2026-02-01T00:00:00.000Z".to_string(),
    }
}

#[test]
fn wardrobe_location_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_WARDROBE_LOCATION") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_WARDROBE_LOCATION to the oracle NDJSON (see header).");
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
        .prefix("qt-wks-location-rust-")
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

    let tokens = Tokens(vec![
        (
            mount_of(
                main,
                "characters",
                "characterDocumentMountPointId",
                &spec.character_a_id,
            ),
            "<ada.mount>".into(),
        ),
        (
            mount_of(
                main,
                "characters",
                "characterDocumentMountPointId",
                &spec.character_b_id,
            ),
            "<bo.mount>".into(),
        ),
        (
            mount_of(
                main,
                "characters",
                "characterDocumentMountPointId",
                &spec.archived_character_id,
            ),
            "<zed.mount>".into(),
        ),
        (
            mount_of(
                main,
                "projects",
                "officialMountPointId",
                &spec.bare_project_id,
            ),
            "<bare.mount>".into(),
        ),
        (
            mount_of(
                main,
                "projects",
                "officialMountPointId",
                &spec.full_project_id,
            ),
            "<full.mount>".into(),
        ),
        (
            mount_of(main, "groups", "officialMountPointId", &spec.group_id),
            "<group.mount>".into(),
        ),
    ]);
    let bare_mount = mount_of(
        main,
        "projects",
        "officialMountPointId",
        &spec.bare_project_id,
    );
    let zed_mount = mount_of(
        main,
        "characters",
        "characterDocumentMountPointId",
        &spec.archived_character_id,
    );
    let archived_hat = spec.items["archivedHat"].id.clone();
    let archived_cape = spec.items["archivedCape"].id.clone();
    let a = spec.user_a_id.as_str();
    let b = spec.user_b_id.as_str();

    let mut probes: Vec<Value> = Vec::new();
    let mut probe = |label: &str,
                     scope: WardrobeScope,
                     id: Option<&str>,
                     user_id: &str,
                     ensure: bool,
                     extra: Option<&dyn Fn(&WardrobeLocation) -> Value>| {
        let before = point_count(mount);
        let (resolved, lines) = captured_with(|| {
            resolve_wardrobe_location(
                main,
                mount,
                scope,
                id,
                user_id,
                ResolveWardrobeLocationOptions { ensure },
            )
        });
        let (result, loc) = match resolved {
            Ok(loc) => (
                json!({ "location": location_shape(loc.as_ref(), &tokens) }),
                loc,
            ),
            Err(e) => (threw(e.to_string(), &tokens), None),
        };
        let mut record = json!({
            "label": label,
            "scope": scope.as_str(),
            "id": id,
            "ensure": ensure,
            "result": result,
            "pointsBefore": before,
            "pointsAfter": point_count(mount),
            "bareFolders": folder_paths(mount, &bare_mount),
        });
        if let (Some(loc), Some(extra)) = (loc.as_ref(), extra) {
            record["extra"] = extra(loc);
        }
        record["lines"] = Value::Array(
            normalize_lines(lines, &tokens)
                .into_iter()
                .map(Value::String)
                .collect(),
        );
        probes.push(record);
    };

    let read_only = |loc: &WardrobeLocation| -> Value {
        json!({ "readItems": item_shape(loc.read_items(mount, true).expect("read")) })
    };

    probe(
        "character A (read probe)",
        WardrobeScope::Character,
        Some(&spec.character_a_id),
        a,
        false,
        Some(&|loc: &WardrobeLocation| {
            let resolve = |req: &str| match loc.resolve_folder(mount, req) {
                Ok(s) => ok(s),
                Err(e) => threw(e.0, &tokens),
            };
            json!({
                "readItems": item_shape(loc.read_items(mount, false).expect("read")),
                "readItemsArchived": item_shape(loc.read_items(mount, true).expect("read")),
                "findArchived": loc.find_item(mount, &archived_hat).expect("find").map(|i| i.id),
                "findMissing": loc.find_item(mount, &spec.missing_id).expect("find").map(|i| i.id),
                "listFolders": serde_json::to_value(loc.list_folders(mount).expect("folders")).unwrap(),
                "resolveExisting": resolve("formal/evening"),
                "resolveHandMade": resolve("hats: summer"),
                "resolveNew": resolve("Formal/Winter Gear"),
                "resolveBlank": resolve("   "),
                "resolveClimb": resolve("../x"),
                "writable": match loc.writable_mount_point_id(main) {
                    Ok(mp) => ok(tokens.remap(&mp)),
                    Err(e) => threw(e.message(), &tokens),
                },
            })
        }),
    );
    probe(
        "character A (ensure)",
        WardrobeScope::Character,
        Some(&spec.character_a_id),
        a,
        true,
        None,
    );
    probe(
        "character B as user A (ownership gate)",
        WardrobeScope::Character,
        Some(&spec.character_b_id),
        a,
        false,
        None,
    );
    probe(
        "character A as user B (ownership gate, reversed)",
        WardrobeScope::Character,
        Some(&spec.character_a_id),
        b,
        false,
        None,
    );
    probe(
        "character B as user B",
        WardrobeScope::Character,
        Some(&spec.character_b_id),
        b,
        false,
        Some(&read_only),
    );
    probe(
        "archived Zed (reads, refuses writes)",
        WardrobeScope::Character,
        Some(&spec.archived_character_id),
        a,
        false,
        Some(&|loc: &WardrobeLocation| {
            let patch = WardrobePatch {
                title: Some("x".to_string()),
                ..Default::default()
            };
            json!({
                "readItems": item_shape(loc.read_items(mount, false).expect("read")),
                "writable": match loc.writable_mount_point_id(main) {
                    Ok(mp) => ok(tokens.remap(&mp)),
                    Err(e) => threw(e.message(), &tokens),
                },
                "create": match loc.create(main, mount, &never_written(&spec.archived_character_id, &spec.missing_id)) {
                    Ok(i) => ok(i.id),
                    Err(e) => threw(e.message(), &tokens),
                },
                "update": match loc.update(main, mount, &archived_cape, &patch) {
                    Ok(i) => ok(i.map(|i| i.id)),
                    Err(e) => threw(e.message(), &tokens),
                },
                "delete": match loc.delete(main, mount, &archived_cape) {
                    Ok(d) => ok(d),
                    Err(e) => threw(e.message(), &tokens),
                },
                "createFolder": match loc.create_folder(main, mount, "Never") {
                    Ok(f) => ok(f),
                    Err(e) => threw(e.message(), &tokens),
                },
                "readItemsAfter": item_shape(loc.read_items(mount, true).expect("read")),
                "folders": folder_paths(mount, &zed_mount),
            })
        }),
    );
    probe(
        "general (read probe)",
        WardrobeScope::General,
        None,
        a,
        false,
        Some(
            &|loc: &WardrobeLocation| json!({ "readItems": item_shape(loc.read_items(mount, false).expect("read")) }),
        ),
    );
    probe(
        "general (ensure)",
        WardrobeScope::General,
        None,
        a,
        true,
        None,
    );
    probe(
        "project Bare Manor (read probe: store, no folder)",
        WardrobeScope::Project,
        Some(&spec.bare_project_id),
        a,
        false,
        Some(&|loc: &WardrobeLocation| {
            json!({
                "readItems": item_shape(loc.read_items(mount, true).expect("read")),
                "listFolders": serde_json::to_value(loc.list_folders(mount).expect("folders")).unwrap(),
            })
        }),
    );
    probe(
        "project Bare Manor (ensure: the folder appears)",
        WardrobeScope::Project,
        Some(&spec.bare_project_id),
        a,
        true,
        Some(&read_only),
    );
    probe(
        "project Bare Manor (ensure again: idempotent)",
        WardrobeScope::Project,
        Some(&spec.bare_project_id),
        a,
        true,
        None,
    );
    probe(
        "project Null Abbey (read probe)",
        WardrobeScope::Project,
        Some(&spec.null_project_id),
        a,
        false,
        None,
    );
    probe(
        "project Null Abbey (ensure)",
        WardrobeScope::Project,
        Some(&spec.null_project_id),
        a,
        true,
        None,
    );
    probe(
        "project Full Abbey",
        WardrobeScope::Project,
        Some(&spec.full_project_id),
        a,
        false,
        Some(&|loc: &WardrobeLocation| {
            json!({
                "readItems": item_shape(loc.read_items(mount, true).expect("read")),
                "listFolders": serde_json::to_value(loc.list_folders(mount).expect("folders")).unwrap(),
            })
        }),
    );
    probe(
        "project Full Abbey (ensure)",
        WardrobeScope::Project,
        Some(&spec.full_project_id),
        a,
        true,
        None,
    );
    probe(
        "group Guild",
        WardrobeScope::Group,
        Some(&spec.group_id),
        a,
        false,
        Some(&|loc: &WardrobeLocation| {
            json!({
                "readItems": item_shape(loc.read_items(mount, true).expect("read")),
                "findShadow": loc.find_item(mount, &archived_hat).expect("find").map(|i| i.title),
            })
        }),
    );
    probe(
        "group Guild (ensure)",
        WardrobeScope::Group,
        Some(&spec.group_id),
        a,
        true,
        None,
    );
    probe(
        "project missing (ensure)",
        WardrobeScope::Project,
        Some(&spec.missing_id),
        a,
        true,
        None,
    );
    probe(
        "group missing (ensure)",
        WardrobeScope::Group,
        Some(&spec.missing_id),
        a,
        true,
        None,
    );
    probe(
        "character missing",
        WardrobeScope::Character,
        Some(&spec.missing_id),
        a,
        false,
        None,
    );
    probe(
        "project '' (falsy id)",
        WardrobeScope::Project,
        Some(""),
        a,
        true,
        None,
    );
    probe(
        "group '' (falsy id)",
        WardrobeScope::Group,
        Some(""),
        a,
        false,
        None,
    );
    probe(
        "character '' (falsy id)",
        WardrobeScope::Character,
        Some(""),
        a,
        false,
        None,
    );
    probe(
        "project null id",
        WardrobeScope::Project,
        None,
        a,
        true,
        None,
    );

    let group_locs = |character_id: &str| -> Value {
        Value::Array(
            group_locations_for_character(main, mount, character_id)
                .iter()
                .map(|l| location_shape(Some(l), &tokens))
                .collect(),
        )
    };
    let (groups, lines) = captured_with(|| {
        json!({
            "ada": group_locs(&spec.character_a_id),
            "bo": group_locs(&spec.character_b_id),
            "missing": group_locs(&spec.missing_id),
        })
    });
    let mut groups = groups;
    groups["lines"] = Value::Array(
        normalize_lines(lines, &tokens)
            .into_iter()
            .map(Value::String)
            .collect(),
    );
    let keys = json!({
        "project": location_key(WardrobeScope::Project, "mp-1"),
        "group": location_key(WardrobeScope::Group, "mp-1"),
        "general": location_key(WardrobeScope::General, &spec.general_mount_point_id),
    });

    let want_probes = oracle["probes"].as_array().expect("oracle probes");
    assert_eq!(
        probes.len(),
        want_probes.len(),
        "probe count: v5 {} vs oracle {}",
        probes.len(),
        want_probes.len()
    );
    let mut reds: Vec<String> = Vec::new();
    for (got, want) in probes.iter().zip(want_probes) {
        if got != want {
            reds.push(format!(
                "probe {}:\n  rust:   {}\n  oracle: {}",
                want["label"],
                serde_json::to_string(got).unwrap(),
                serde_json::to_string(want).unwrap()
            ));
        }
    }
    if groups != oracle["groups"] {
        reds.push(format!(
            "groupLocationsForCharacter:\n  rust:   {groups}\n  oracle: {}",
            oracle["groups"]
        ));
    }
    if keys != oracle["keys"] {
        reds.push(format!(
            "locationKey:\n  rust:   {keys}\n  oracle: {}",
            oracle["keys"]
        ));
    }
    assert!(
        reds.is_empty(),
        "{} divergence(s):\n{}",
        reds.len(),
        reds.join("\n")
    );

    // Shape guards against a corpus that could not prove its beats.
    let find = |label: &str| -> &Value {
        probes
            .iter()
            .find(|p| p["label"] == label)
            .unwrap_or_else(|| panic!("no probe {label}"))
    };
    assert_eq!(
        find("project Bare Manor (read probe: store, no folder)")["bareFolders"],
        json!([]),
        "the read probe must find Bare Manor's store with NO Wardrobe/ folder"
    );
    assert_eq!(
        find("project Bare Manor (ensure: the folder appears)")["bareFolders"],
        json!(["wardrobe"]),
        "ensure:true must create exactly the Wardrobe/ folder"
    );
    let null_probe = find("project Null Abbey (ensure)");
    assert!(
        null_probe["result"]["threw"].is_string(),
        "the NULL-pointer project must throw through the real repository"
    );
    assert_eq!(
        null_probe["pointsBefore"], null_probe["pointsAfter"],
        "nothing may be provisioned for a project the repository cannot hydrate"
    );
    assert!(
        find("archived Zed (reads, refuses writes)")["extra"]["create"]["threw"].is_string(),
        "an archived character's create must refuse with the tombstone"
    );
    assert_eq!(
        find("character B as user A (ownership gate)")["result"]["location"],
        Value::Null,
        "the ownership gate must refuse another user's character"
    );

    eprintln!(
        "OK: wardrobe locations matched oracle ({} probes + groups + keys).",
        probes.len()
    );
}
