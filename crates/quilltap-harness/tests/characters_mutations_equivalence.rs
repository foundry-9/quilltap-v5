//! P4.6f (slice 4) characters MUTATIONS differential: the create / quick-create /
//! update handlers (`api::characters::{character_create, character_quick_create,
//! character_update}`) vs v4's REAL `characters/handlers/post.ts` (create /
//! quick-create) + `characters/[id]/handlers/put.ts` (update). Each case runs on a
//! FRESH copy of the committed characters fixture; the response ECHO (a full
//! overlay re-read of the freshly-written vault) is diffed byte-exact.
//!
//! The echo diff transitively proves the vault round-trip in composition (every
//! managed field is read back through the overlay the handler just wrote); the raw
//! storage rows are already byte-proven by the standing `characters_create_tier2`
//! and `vault_character_update` tier-2 differentials, so they are not re-dumped.
//!
//! Minted ids + timestamps (the created character id, its mount-point FK, and the
//! per-array scenario/prompt/physical ids/timestamps) are blanked on both sides
//! (keys `id`/`createdAt`/`updatedAt`/`characterDocumentMountPointId`).
//!
//! The case INPUTS below MUST stay identical to `characters-mutations.test.ts`.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-characters-mutations.ndjson npx jest -- characters-mutations
//! Run:
//!   QT_ORACLE_CHARACTERS_MUTATIONS=/tmp/oracle-characters-mutations.ndjson \
//!     cargo test -p quilltap-harness --test characters_mutations_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::characters;
use quilltap_core::api::types::Response;
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::{json, Value};

const ARIA: &str = "a1000000-0000-4000-8000-000000000001";
const CONN: &str = "c0000001-0000-4000-8000-000000000001";
const ADVENTURE: &str = "70000001-0000-4000-8000-000000000001";
const MYSTERY: &str = "70000002-0000-4000-8000-000000000002";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/characters.json")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
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

fn canon_numbers(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15 {
                    *v = Value::Number((f as i64).into());
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(canon_numbers),
        Value::Object(o) => o.iter_mut().for_each(|(_, x)| canon_numbers(x)),
        _ => {}
    }
}
fn sorted(v: &Value) -> Value {
    match v {
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let mut m = serde_json::Map::new();
            for k in keys {
                m.insert(k.clone(), sorted(&o[k]));
            }
            Value::Object(m)
        }
        _ => v.clone(),
    }
}
/// Recursively blank the minted keys (id + timestamps + the vault mount FK) so the
/// per-op-minted values don't defeat the echo diff. Every OTHER field is compared.
fn blank_minted(v: &mut Value) {
    match v {
        Value::Object(o) => {
            for k in [
                "id",
                "createdAt",
                "updatedAt",
                "characterDocumentMountPointId",
            ] {
                if o.contains_key(k) {
                    o.insert(k.to_string(), Value::String(format!("<{k}>")));
                }
            }
            o.iter_mut().for_each(|(_, x)| blank_minted(x));
        }
        Value::Array(a) => a.iter_mut().for_each(blank_minted),
        _ => {}
    }
}
fn norm(v: &Value) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    blank_minted(&mut v);
    serde_json::to_string_pretty(&sorted(&v)).unwrap()
}
fn first_diff(got: &str, want: &str) -> String {
    let g: Vec<&str> = got.lines().collect();
    let w: Vec<&str> = want.lines().collect();
    for i in 0..g.len().max(w.len()) {
        let gi = g.get(i).copied().unwrap_or("<none>");
        let wi = w.get(i).copied().unwrap_or("<none>");
        if gi != wi {
            let mut ctx = String::new();
            for j in i.saturating_sub(3)..i {
                ctx.push_str(&format!("   = {}\n", g.get(j).copied().unwrap_or("")));
            }
            ctx.push_str(&format!("  GOT : {gi}\n  WANT: {wi}\n"));
            return ctx;
        }
    }
    "(identical line-by-line)".to_string()
}
fn response_data(r: &Response) -> Value {
    let v = serde_json::to_value(r).unwrap();
    v.get("data").cloned().unwrap_or(Value::Null)
}

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-char-mut-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("characters-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("characters-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db")
}

/// The case input bodies — kept identical to `characters-mutations.test.ts`.
fn create_full_body() -> Value {
    json!({
        "name": "Fenwick",
        "title": "The Cartographer",
        "identity": "A wandering mapmaker.",
        "description": "Ink-stained and curious.",
        "manifesto": "Every coast deserves a name.",
        "personality": "Meticulous, wry.",
        "firstMessage": "Ah — a fellow traveller.",
        "exampleDialogues": "User: Where to?\nFenwick: North, always north.",
        "controlledBy": "llm",
        "npc": false,
        "defaultConnectionProfileId": CONN,
        "scenarios": [
            { "title": "The Harbor", "content": "Fog rolls over the docks." },
            { "title": "The Summit", "content": "The last ridge before the map ends." }
        ],
        "systemPrompts": [
            {
                "id": "d0000001-0000-4000-8000-000000000001",
                "name": "Terse",
                "content": "Answer in few words.",
                "isDefault": true,
                "createdAt": "2020-01-01T00:00:00.000Z",
                "updatedAt": "2020-01-01T00:00:00.000Z"
            }
        ],
        "physicalDescription": {
            "id": "e0000001-0000-4000-8000-000000000001",
            "name": "Fenwick body",
            "shortPrompt": "weathered coat",
            "mediumPrompt": "a weathered coat and a brass spyglass",
            "fullDescription": "Tall, lean, ink on the fingers.",
            "createdAt": "2020-01-01T00:00:00.000Z",
            "updatedAt": "2020-01-01T00:00:00.000Z"
        }
    })
}
fn update_managed_body() -> Value {
    json!({
        "title": "Renamed Title",
        "identity": "A rewritten identity.",
        "description": "A rewritten description.",
        "personality": "Newly bold.",
        "firstMessage": "A fresh greeting.",
        "scenarios": [{ "title": "Only Scenario", "content": "The reprojected scene." }],
        "physicalDescription": {
            "name": "Aria body v2",
            "shortPrompt": "silver cloak",
            "longPrompt": "a flowing silver cloak with embroidered stars"
        }
    })
}
fn update_slim_body() -> Value {
    json!({
        "name": "Aria Reforged",
        "controlledBy": "user",
        "npc": true,
        "canBeCarina": false
    })
}
fn wardrobe_create_body() -> Value {
    json!({
        "title": "Storm Cloak",
        "description": "An oilskin cloak for rough weather.",
        "imagePrompt": "heavy grey oilskin storm cloak",
        "types": ["top", "accessories"],
        "isDefault": false
    })
}
fn wardrobe_update_body() -> Value {
    json!({ "title": "Weathered Flight Jacket", "description": null, "isDefault": false })
}

/// The six taggable tables' (id, tags) + the tags table (id, name) — the DELETE
/// fan-out verification dump (matches the oracle's `dumpTagTables`).
fn dump_tag_tables(db: &Db) -> Value {
    db.read_main(|main| {
        let mut out = serde_json::Map::new();
        for t in [
            "characters",
            "chats",
            "connection_profiles",
            "image_profiles",
            "embedding_profiles",
            "files",
        ] {
            let mut stmt = main.prepare(&format!("SELECT id, tags FROM {t} ORDER BY id"))?;
            let rows = stmt.query_map([], |r| {
                Ok(json!({
                    "id": r.get::<_, String>(0)?,
                    "tags": r.get::<_, Option<String>>(1)?,
                }))
            })?;
            let mut arr = Vec::new();
            for row in rows {
                arr.push(row?);
            }
            out.insert(t.to_string(), Value::Array(arr));
        }
        let mut stmt = main.prepare("SELECT id, name FROM tags ORDER BY id")?;
        let rows = stmt.query_map([], |r| {
            Ok(json!({ "id": r.get::<_, String>(0)?, "name": r.get::<_, String>(1)? }))
        })?;
        let mut arr = Vec::new();
        for row in rows {
            arr.push(row?);
        }
        out.insert("tags".to_string(), Value::Array(arr));
        Ok(Value::Object(out))
    })
    .unwrap()
}

/// Sorted stable form WITHOUT the minted-key blanking (the fan-out dump rows carry
/// baked ids identical on both sides).
fn norm_tables(v: &Value) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    serde_json::to_string_pretty(&sorted(&v)).unwrap()
}

/// Discover a baked wardrobe item's minted id (identical on both sides) by title.
fn discover_item_id(db: &Db, character_id: &str, title: &str) -> String {
    let cid = character_id.to_string();
    let needle = title.to_string();
    db.read_main(move |main| {
        db.read_mount_index(move |mount| {
            let docs =
                quilltap_core::db::doc_mount_documents::DocMountDocumentsRepository::new(mount);
            let items =
                quilltap_core::db::wardrobe_read::find_by_character_id(main, &docs, &cid, false)?;
            Ok(items
                .iter()
                .find(|i| i.get("title").and_then(Value::as_str) == Some(needle.as_str()))
                .and_then(|i| i.get("id").and_then(Value::as_str))
                .map(str::to_string))
        })
    })
    .unwrap()
    .unwrap_or_else(|| panic!("wardrobe item titled {title} not found"))
}

/// Resolve a vault link id by its relativePath (both sides read the same baked
/// .db, so the minted link id matches). Reads the character's mount FK off the
/// slim row, then the link.
fn discover_photo_link_id(db: &Db, character_id: &str, relative_path: &str) -> String {
    let cid = character_id.to_string();
    let rel = relative_path.to_string();
    db.read_main(move |main| {
        let mp_id: String = main
            .query_row(
                "SELECT characterDocumentMountPointId FROM characters WHERE id = ?1",
                rusqlite::params![cid],
                |r| r.get(0),
            )
            .expect("aria mount fk");
        db.read_mount_index(move |mount| {
            let id: String = mount
                .query_row(
                    "SELECT id FROM doc_mount_file_links WHERE mountPointId = ?1 AND relativePath = ?2",
                    rusqlite::params![mp_id, rel],
                    |r| r.get(0),
                )
                .expect("avatar link");
            Ok(id)
        })
    })
    .unwrap()
}

/// Dump the mount-index photo GC tables + the character's defaultImageId, in the
/// oracle's shape.
fn dump_photo_tables(db: &Db, character_id: &str) -> Value {
    let cid = character_id.to_string();
    db.read_main(move |main| {
        let mp_id: String = main
            .query_row(
                "SELECT characterDocumentMountPointId FROM characters WHERE id = ?1",
                rusqlite::params![cid],
                |r| r.get(0),
            )
            .expect("aria mount fk");
        let character: Value = main
            .query_row(
                "SELECT id, defaultImageId FROM characters WHERE id = ?1",
                rusqlite::params![cid],
                |r| {
                    Ok(json!({
                        "id": r.get::<_, String>(0)?,
                        "defaultImageId": r.get::<_, Option<String>>(1)?,
                    }))
                },
            )
            .expect("aria row");
        db.read_mount_index(move |mount| {
            let mut links = Vec::new();
            {
                let mut stmt = mount.prepare(
                    "SELECT id, relativePath FROM doc_mount_file_links WHERE mountPointId = ?1 ORDER BY id",
                )?;
                let rows = stmt.query_map(rusqlite::params![mp_id], |r| {
                    Ok(json!({ "id": r.get::<_, String>(0)?, "relativePath": r.get::<_, String>(1)? }))
                })?;
                for row in rows {
                    links.push(row?);
                }
            }
            let mut files = Vec::new();
            {
                let mut stmt = mount.prepare("SELECT id, sha256 FROM doc_mount_files ORDER BY id")?;
                let rows = stmt.query_map([], |r| {
                    Ok(json!({ "id": r.get::<_, String>(0)?, "sha256": r.get::<_, String>(1)? }))
                })?;
                for row in rows {
                    files.push(row?);
                }
            }
            let mut blobs = Vec::new();
            {
                let mut stmt = mount.prepare("SELECT fileId FROM doc_mount_blobs ORDER BY fileId")?;
                let rows =
                    stmt.query_map([], |r| Ok(json!({ "fileId": r.get::<_, String>(0)? })))?;
                for row in rows {
                    blobs.push(row?);
                }
            }
            Ok(json!({
                "links": links,
                "files": files,
                "blobs": blobs,
                "character": character,
            }))
        })
    })
    .unwrap()
}

/// Dump the mount's `photos/` links (raw columns; deterministic under the fixed
/// clock — the link id is excluded as the only minted value), in the oracle's shape.
fn dump_saved_photo_links(db: &Db, mount_point_id: &str) -> Value {
    let mp = mount_point_id.to_string();
    db.read_mount_index(move |mount| {
        let mut stmt = mount.prepare(
            "SELECT relativePath, fileId, originalMimeType, extractedText, description \
             FROM doc_mount_file_links WHERE mountPointId = ?1 AND relativePath LIKE 'photos/%' \
             ORDER BY relativePath",
        )?;
        let rows = stmt.query_map(rusqlite::params![mp], |r| {
            Ok(json!({
                "relativePath": r.get::<_, String>(0)?,
                "fileId": r.get::<_, String>(1)?,
                "originalMimeType": r.get::<_, Option<String>>(2)?,
                "extractedText": r.get::<_, Option<String>>(3)?,
                "description": r.get::<_, Option<String>>(4)?,
            }))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(json!({ "savedLinks": out }))
    })
    .unwrap()
}

#[test]
fn characters_mutations_match_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_CHARACTERS_MUTATIONS") else {
        return;
    };
    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).expect("spec");
    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let uid = spec.user_id.clone();
    let mut failed: Vec<String> = Vec::new();
    // Manual (non-`run`) checks push here — `run` holds a mutable borrow of
    // `failed` for its whole scope, so direct pushes would conflict.
    let mut extra: Vec<String> = Vec::new();

    let mut run = |name: &str, resp: Response| {
        let want = &oracle[name];
        if let Response::Error(e) = &resp {
            eprintln!("[{name}] expected success, got error: {}", e.message);
            failed.push(name.to_string());
            return;
        }
        let got = norm(&response_data(&resp));
        let wnt = norm(&want["body"]);
        if got != wnt {
            eprintln!("[{name}] MISMATCH:\n{}", first_diff(&got, &wnt));
            failed.push(name.to_string());
        } else {
            eprintln!("[{name}] OK.");
        }
    };

    {
        let db = fresh_db(&spec, "create_full");
        let r = rt.block_on(characters::character_create(&db, &uid, create_full_body()));
        run("create_full", r);
    }
    {
        let db = fresh_db(&spec, "create_min");
        let r = rt.block_on(characters::character_create(
            &db,
            &uid,
            json!({ "name": "Mimsy" }),
        ));
        run("create_minimal", r);
    }
    {
        let db = fresh_db(&spec, "quick");
        let r = rt.block_on(characters::character_quick_create(&db, &uid, "Quill"));
        run("quick_create", r);
    }
    {
        let db = fresh_db(&spec, "upd_mgd");
        let r = rt.block_on(characters::character_update(
            &db,
            &uid,
            ARIA,
            update_managed_body(),
        ));
        run("update_managed", r);
    }
    {
        let db = fresh_db(&spec, "upd_slim");
        let r = rt.block_on(characters::character_update(
            &db,
            &uid,
            ARIA,
            update_slim_body(),
        ));
        run("update_slim", r);
    }
    {
        let db = fresh_db(&spec, "wc");
        let r = rt.block_on(characters::character_wardrobe_create(
            &db,
            &uid,
            ARIA,
            wardrobe_create_body(),
        ));
        run("wardrobe_create", r);
    }
    {
        let db = fresh_db(&spec, "wg");
        let iid = discover_item_id(&db, ARIA, "Flight Jacket");
        let r = characters::character_wardrobe_get(&db, &uid, ARIA, &iid);
        run("wardrobe_get", r);
    }
    {
        let db = fresh_db(&spec, "wu");
        let iid = discover_item_id(&db, ARIA, "Flight Jacket");
        let r = rt.block_on(characters::character_wardrobe_update(
            &db,
            &uid,
            ARIA,
            &iid,
            wardrobe_update_body(),
        ));
        run("wardrobe_update", r);
    }
    {
        let db = fresh_db(&spec, "wd");
        let iid = discover_item_id(&db, ARIA, "Goggles");
        let r = rt.block_on(characters::character_wardrobe_delete(&db, &uid, ARIA, &iid));
        run("wardrobe_delete", r);
    }
    {
        let db = fresh_db(&spec, "depget");
        run(
            "depiction_get_empty",
            characters::character_depiction_guidelines(&db, &uid, ARIA),
        );
    }
    {
        let db = fresh_db(&spec, "depput");
        let r = rt.block_on(characters::character_depiction_guidelines_update(
            &db,
            &uid,
            ARIA,
            "Keep depictions tasteful and in-period.",
        ));
        run("depiction_put_write", r);
        let rb = response_data(&characters::character_depiction_guidelines(&db, &uid, ARIA));
        let got_rb = rb.get("content").cloned().unwrap_or(Value::Null);
        if norm(&got_rb) != norm(&oracle["depiction_put_write"]["readback"]) {
            eprintln!("[depiction_put_write readback] MISMATCH: {got_rb:?}");
            extra.push("depiction_put_write_readback".to_string());
        }
    }
    {
        let db = fresh_db(&spec, "depclr");
        let r = rt.block_on(characters::character_depiction_guidelines_update(
            &db, &uid, ARIA, "   ",
        ));
        run("depiction_put_clear", r);
        let rb = response_data(&characters::character_depiction_guidelines(&db, &uid, ARIA));
        let got_rb = rb.get("content").cloned().unwrap_or(Value::Null);
        if norm(&got_rb) != norm(&oracle["depiction_put_clear"]["readback"]) {
            eprintln!("[depiction_put_clear readback] MISMATCH: {got_rb:?}");
            extra.push("depiction_put_clear_readback".to_string());
        }
    }
    {
        let db = fresh_db(&spec, "tl");
        run("tag_list", characters::tag_list(&db, &uid, None));
    }
    {
        let db = fresh_db(&spec, "tg");
        run("tag_get", characters::tag_get(&db, &uid, ADVENTURE));
    }
    {
        let db = fresh_db(&spec, "tcn");
        run(
            "tag_create_new",
            rt.block_on(characters::tag_create(&db, &uid, "Voyage")),
        );
    }
    {
        let db = fresh_db(&spec, "tcd");
        run(
            "tag_create_dedup",
            rt.block_on(characters::tag_create(&db, &uid, "adventure")),
        );
    }
    {
        let db = fresh_db(&spec, "tu");
        run(
            "tag_update",
            rt.block_on(characters::tag_update(
                &db,
                &uid,
                MYSTERY,
                json!({ "name": "Enigma", "quickHide": true }),
            )),
        );
    }
    {
        let db = fresh_db(&spec, "tdel");
        let r = rt.block_on(characters::tag_delete(&db, &uid, ADVENTURE));
        run("tag_delete", r);
        // Verify the multi-entity fan-out: diff all six taggable tables + the
        // tags table against the oracle's post-delete dump.
        let got = dump_tag_tables(&db);
        let want = &oracle["tag_delete"]["tables"];
        if norm_tables(&got) != norm_tables(want) {
            eprintln!(
                "[tag_delete tables] MISMATCH:\n{}",
                first_diff(&norm_tables(&got), &norm_tables(want))
            );
            extra.push("tag_delete_tables".to_string());
        } else {
            eprintln!("[tag_delete tables] OK.");
        }
    }

    {
        // P4.6i: ST import (JSON leg). Diff the slim create echo AND the overlay
        // readback of the created character (proving the ST-derived vault fields
        // round-tripped). Minted ids/timestamps blanked by `norm`.
        let db = fresh_db(&spec, "stimport");
        let card = json!({
            "spec": "chara_card_v2",
            "spec_version": "2.0",
            "data": {
                "name": "Fable",
                "description": "A traveling storyteller.",
                "personality": "Whimsical and wry.",
                "scenario": "The caravan halts at dusk.",
                "first_mes": "Gather round, friends.",
                "mes_example": "User: A tale?\nFable: Always a tale.",
                "system_prompt": "You are Fable, a wandering storyteller.",
                "creator_notes": "imported for the differential",
                "tags": ["bard"],
                "creator": "TestSuite",
                "character_version": "2.1",
                "extensions": { "origin": "sillytavern" },
                "title": "The Wandering Bard",
            },
        });
        let resp = rt.block_on(characters::character_import(&db, &uid, card));
        let echo = response_data(&resp);
        // Echo diff.
        if norm(&echo) != norm(&oracle["st_import_card"]["body"]) {
            eprintln!(
                "[st_import_card] MISMATCH:\n{}",
                first_diff(&norm(&echo), &norm(&oracle["st_import_card"]["body"]))
            );
            extra.push("st_import_card".to_string());
        } else {
            eprintln!("[st_import_card] OK.");
        }
        // Readback: the created character's overlay.
        let created_id = echo
            .get("character")
            .and_then(|c| c.get("id"))
            .and_then(Value::as_str)
            .expect("created id in echo")
            .to_string();
        let readback = response_data(&characters::character_get(&db, &uid, &created_id));
        if norm(&readback) != norm(&oracle["st_import_card"]["readback"]) {
            eprintln!(
                "[st_import_card readback] MISMATCH:\n{}",
                first_diff(
                    &norm(&readback),
                    &norm(&oracle["st_import_card"]["readback"])
                )
            );
            extra.push("st_import_card_readback".to_string());
        } else {
            eprintln!("[st_import_card readback] OK.");
        }
    }

    {
        // P4.6i: save Aria's avatar link into photos/ (the linkId save-by-id leg).
        // A fixed keptAt (matching the oracle's frozen Date) makes the minted path
        // + kept-image markdown deterministic; only the link id is minted (blanked).
        const FIXED_KEPT_AT: &str = "2026-04-01T12:00:00.000Z";
        let db = fresh_db(&spec, "photosave");
        let source_link = discover_photo_link_id(&db, ARIA, "images/avatar.webp");
        let saved: Value = rt
            .block_on(db.write(move |writers| {
                let mount = writers.mount_index().unwrap().connection();
                let main = writers.main().connection();
                let r = quilltap_core::photos::character_gallery_service::save_link_to_character_gallery(
                    main, mount, ARIA, &source_link, None, &[], FIXED_KEPT_AT,
                );
                Ok(r.map_err(|e| format!("{e:?}")))
            }))
            .unwrap()
            .expect("save_link ok");
        // Return-value diff (linkId minted → blank on both sides).
        let mut got_body = saved.clone();
        let mut want_body = oracle["photo_save_link"]["body"].clone();
        got_body["linkId"] = Value::String("<linkId>".into());
        want_body["linkId"] = Value::String("<linkId>".into());
        if norm_tables(&got_body) != norm_tables(&want_body) {
            eprintln!(
                "[photo_save_link] MISMATCH:\n{}",
                first_diff(&norm_tables(&got_body), &norm_tables(&want_body))
            );
            extra.push("photo_save_link".to_string());
        } else {
            eprintln!("[photo_save_link] OK.");
        }
        // Dump the freshly-written photos/ link (deterministic under the fixed clock).
        let mp_id = saved
            .get("mountPointId")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
        let saved_links = dump_saved_photo_links(&db, &mp_id);
        let want_tables = &oracle["photo_save_link"]["tables"];
        if norm_tables(&saved_links) != norm_tables(want_tables) {
            eprintln!(
                "[photo_save_link tables] MISMATCH:\n{}",
                first_diff(&norm_tables(&saved_links), &norm_tables(want_tables))
            );
            extra.push("photo_save_link_tables".to_string());
        } else {
            eprintln!("[photo_save_link tables] OK.");
        }
    }

    {
        // P4.6i: remove Aria's avatar from the gallery. Diff the {deleted,fileGC}
        // body AND the GC-table dump (links/files/blobs + defaultImageId) — all
        // baked ids, so no remap.
        let db = fresh_db(&spec, "photorm");
        let link_id = discover_photo_link_id(&db, ARIA, "images/avatar.webp");
        let r = rt.block_on(characters::character_photo_remove(
            &db, &uid, ARIA, &link_id,
        ));
        run("photo_remove_avatar", r);
        let got = dump_photo_tables(&db, ARIA);
        let want = &oracle["photo_remove_avatar"]["tables"];
        if norm_tables(&got) != norm_tables(want) {
            eprintln!(
                "[photo_remove_avatar tables] MISMATCH:\n{}",
                first_diff(&norm_tables(&got), &norm_tables(want))
            );
            extra.push("photo_remove_avatar_tables".to_string());
        } else {
            eprintln!("[photo_remove_avatar tables] OK.");
        }
    }

    failed.extend(extra);
    assert!(failed.is_empty(), "characters-mutations FAILED: {failed:?}");
}
