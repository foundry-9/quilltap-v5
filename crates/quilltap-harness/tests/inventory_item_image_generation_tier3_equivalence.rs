//! Tier-3 differential (P4.D324, the Inventory keystone — v4 `4fbdad186` at
//! `322a5488e`): the Inventory object picture GENERATION —
//! `quilltap_core::inventory::item_image_generation::generate_inventory_item_image`
//! vs v4's REAL `generateInventoryItemImage`
//! (`lib/inventory/item-image-generation.ts`), the image provider mocked BELOW
//! both (the wardrobe twin's shape, `wardrobe_item_image_generation_tier3_
//! equivalence`, over the twin's OWN pair).
//!
//! Per scenario both sides take a fresh copy of the pair, write the scenario's
//! Inventory object into its home through their REAL Inventory writer
//! (`createInventoryInMount` / `create_inventory_in_mount` — the writer, so an
//! archived owner's vault can hold one), apply the scenario's `sql`, optionally
//! write the project store's Lantern aesthetic doc (`writeDatabaseDocument` /
//! `database_store::write_database_document`), and run ONE generation; then
//! compare the result or typed error (`NoInventoryImageProfileError` — the ONE
//! sentence for no profile AND a gone key row; `InventoryImageGenerationError`
//! with `trail` and `refused`; `CharacterArchivedError` before any provider
//! call), the ordered provider calls `{provider, apiKey, params}` (`n: 1`,
//! `style: natural`, `square`), the generation's own `[InventoryItemImage]`
//! lines, the object's pointer after, the main `files` table and the six
//! mount-index tables (the tier-2 normalizer), and the `llm_logs` rows (ONE
//! `INVENTORY_ITEM_IMAGE` row per provider attempt).
//!
//! Regenerate (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-iiig-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/inventory-item-image-generation.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/inventory-item-image-generation-tier3.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_WIIG_MAIN=/tmp/qt-iiig-main.db QT_FIXTURE_WIIG_MOUNT=/tmp/qt-iiig-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-wardrobe-item-image-generation-fixture.ts
//!   QT_FIXTURE_WIIG_MAIN=/tmp/qt-iiig-main.db QT_FIXTURE_WIIG_MOUNT=/tmp/qt-iiig-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-inventory-item-image-generation.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=600000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- "cases/inventory-item-image-generation\.test\.ts$"
//!
//! Run:
//!   QT_ORACLE_IIIG=/tmp/oracle-inventory-item-image-generation.ndjson \
//!   QT_FIXTURE_IIIG_MAIN=/tmp/qt-iiig-main.db QT_FIXTURE_IIIG_MOUNT=/tmp/qt-iiig-mount.db \
//!     cargo test -p quilltap-harness --test inventory_item_image_generation_tier3_equivalence

mod common;
mod wardrobe_images_support;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use quilltap_core::db::item_folder_ops::{VaultFolderMount, VaultFolderMountScope};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::Writer;
use quilltap_core::inventory::errors::InventoryError;
use quilltap_core::inventory::item_image_generation::{
    generate_inventory_item_image, GenerateInventoryItemImageParams,
    InventoryImageGenerationFailure, InventoryItemImageGenerationResult,
    NO_INVENTORY_IMAGE_PROFILE_MESSAGE,
};
use quilltap_core::inventory::item_images::resolve_inventory_item_home;
use quilltap_core::inventory::types::{InventoryFields, InventoryRecord, InventoryScope};
use quilltap_core::model::image::{
    ErasedImageGenerate, GeneratedImageData, ImageGenError, ImageGenParams, ImageGenResponse,
    ImageProvider,
};
use quilltap_core::services::wardrobe_item_image_generation::WardrobeItemImageSeams;
use quilltap_core::test_support::captured_with;
use serde_json::{json, Map, Value};
use wardrobe_images_support::{baked_tokens, dump_all, oracle_lines, rust_lines, Normalizer};

const LOG_PREFIXES: [&str; 1] = ["[InventoryItemImage]"];
const SAFETY: &str = "400 Your request was rejected as a result of our safety system.";

struct RecordingProvider {
    mode: String,
    webp: String,
    calls: Arc<Mutex<Vec<Value>>>,
}

impl ImageProvider for RecordingProvider {
    async fn generate_image(
        &self,
        provider: &str,
        api_key: &str,
        params: &ImageGenParams,
    ) -> Result<ImageGenResponse, ImageGenError> {
        let n = {
            let mut calls = self.calls.lock().unwrap();
            calls.push(json!({
                "provider": provider,
                "apiKey": api_key,
                "params": params.to_key_value(),
            }));
            calls.len()
        };
        match self.mode.as_str() {
            "throw" => return Err(ImageGenError::new("canned provider failure")),
            "refuseAll" => return Err(ImageGenError::new(SAFETY)),
            "refuseFirst" if n == 1 => return Err(ImageGenError::new(SAFETY)),
            _ => {}
        }
        let data = (self.mode != "nodata").then(|| self.webp.clone());
        Ok(ImageGenResponse {
            images: vec![GeneratedImageData {
                data,
                url: None,
                mime_type: Some("image/webp".into()),
                revised_prompt: Some(
                    if self.mode == "nodata" {
                        "nothing drawn"
                    } else {
                        "a revised coat"
                    }
                    .into(),
                ),
            }],
        })
    }
}

fn trail_json(t: &Option<Vec<quilltap_core::services::route_trail::RouteAttempt>>) -> Value {
    t.as_ref()
        .map(|t| serde_json::to_value(t).unwrap())
        .unwrap_or(Value::Null)
}

fn result_json(
    r: &Result<InventoryItemImageGenerationResult, InventoryImageGenerationFailure>,
) -> Value {
    match r {
        Ok(r) => json!({
            "ok": true,
            "fileId": r.file_id,
            "prompt": r.prompt,
            "profile": { "id": r.profile_id, "name": r.profile_name },
            "rerouted": r.rerouted,
            "trail": trail_json(&r.trail),
        }),
        Err(InventoryImageGenerationFailure::NoProfile) => json!({
            "ok": false,
            "error": "NoInventoryImageProfileError",
            "message": NO_INVENTORY_IMAGE_PROFILE_MESSAGE,
            "trail": null,
            "refused": null,
        }),
        Err(InventoryImageGenerationFailure::Generation {
            message,
            trail,
            refused,
        }) => json!({
            "ok": false,
            "error": "InventoryImageGenerationError",
            "message": message,
            "trail": trail_json(trail),
            "refused": refused,
        }),
        Err(InventoryImageGenerationFailure::Item(e)) => json!({
            "ok": false,
            "error": match e {
                InventoryError::CharacterArchived { .. } => "CharacterArchivedError",
                InventoryError::ForeignImage { .. } => "ForeignWardrobeImageError",
                _ => "Error",
            },
            "message": e.to_string(),
            "trail": null,
            "refused": null,
        }),
    }
}

/// `durationMs=<n>` → `durationMs=<ms>` (a wall clock).
fn blank_duration(lines: Vec<String>) -> Vec<String> {
    let re = regex::Regex::new(r"durationMs=\d+").unwrap();
    lines
        .into_iter()
        .map(|l| re.replace_all(&l, "durationMs=<ms>").into_owned())
        .collect()
}

fn count(db: &Db, table: &str) -> Value {
    let t = table.to_string();
    db.read_main(move |c| {
        let exists: bool = c
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [&t],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n > 0)?;
        if !exists {
            return Ok(Value::Null);
        }
        Ok(json!(c.query_row(
            &format!("SELECT COUNT(*) FROM {t}"),
            [],
            |r| r.get::<_, i64>(0)
        )?))
    })
    .unwrap()
}

/// A scenario object's home: (scope, owner id).
fn home_of(spec: &Value, home: &str) -> (InventoryScope, String) {
    match home {
        "character" => (
            InventoryScope::Character,
            spec["characterId"].as_str().unwrap().to_string(),
        ),
        "archived" => (
            InventoryScope::Character,
            spec["archivedCharacterId"].as_str().unwrap().to_string(),
        ),
        _ => (
            InventoryScope::Project,
            spec["projectId"].as_str().unwrap().to_string(),
        ),
    }
}

#[test]
fn inventory_item_image_generation_tier3_matches_oracle() {
    let (Ok(oracle_path), Ok(main_fixture), Ok(mount_fixture)) = (
        std::env::var("QT_ORACLE_IIIG"),
        std::env::var("QT_FIXTURE_IIIG_MAIN"),
        std::env::var("QT_FIXTURE_IIIG_MOUNT"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_IIIG + QT_FIXTURE_IIIG_MAIN + QT_FIXTURE_IIIG_MOUNT (see header)."
        );
        return;
    };
    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/inventory-item-image-generation-tier3.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let pepper = spec["testPepperBase64"].as_str().unwrap().to_string();
    let oracle: HashMap<String, Value> = std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Value = serde_json::from_str(l).unwrap();
            (v["name"].as_str().unwrap().to_string(), v)
        })
        .collect();
    let scenarios = spec["scenarios"].as_array().unwrap();
    assert_eq!(
        oracle.len(),
        scenarios.len(),
        "oracle scenario count != corpus"
    );

    let mut failures: Vec<String> = Vec::new();
    for scenario in scenarios {
        let name = scenario["name"].as_str().unwrap();
        let want = &oracle[name];
        let scratch = tempfile::Builder::new()
            .prefix(&format!("qt-iiig-rust-{name}-"))
            .tempdir()
            .unwrap();
        let (mw, nw, lw) = (
            scratch.path().join("main.db"),
            scratch.path().join("mount.db"),
            scratch.path().join("llm-logs.db"),
        );
        std::fs::copy(&main_fixture, &mw).unwrap();
        std::fs::copy(&mount_fixture, &nw).unwrap();
        common::materialize_llm_logs(&lw, &pepper);
        let object = &spec["objects"][scenario["object"].as_str().unwrap()];
        let (scope, owner_id) = home_of(&spec, object["home"].as_str().unwrap());
        let record = InventoryRecord(object["record"].as_object().unwrap().clone());
        let baked = {
            let main = Writer::open_writable(&mw, &pepper).unwrap();
            quilltap_core::test_support::ensure_wear_ledger_on(main.connection());
            let mount = Writer::open_writable(&nw, &pepper).unwrap();
            // The FIXTURE's literals, taken BEFORE the plant: the planted rows'
            // ids are minted on each side, so they must tokenise on both.
            let mut baked = dump_all(main.connection(), mount.connection());
            baked.as_object_mut().unwrap().insert(
                "chats".into(),
                wardrobe_images_support::dump(main.connection(), "chats"),
            );
            let baked = baked_tokens(&baked, &spec);
            // The scenario's object, through the REAL writer (v4's
            // `createInventoryInMount`), then the Lantern doc.
            let mount_point_id: String = if scope == InventoryScope::Project {
                main.connection()
                    .query_row(
                        "SELECT officialMountPointId FROM projects WHERE id = ?1",
                        [&owner_id],
                        |r| r.get(0),
                    )
                    .unwrap()
            } else {
                main.connection()
                    .query_row(
                        "SELECT characterDocumentMountPointId FROM characters WHERE id = ?1",
                        [&owner_id],
                        |r| r.get(0),
                    )
                    .unwrap()
            };
            quilltap_core::db::inventory_writes::create_inventory_in_mount(
                mount.connection(),
                &VaultFolderMount {
                    mount_point_id: mount_point_id.clone(),
                    scope: if scope == InventoryScope::Project {
                        VaultFolderMountScope::Project
                    } else {
                        VaultFolderMountScope::Character
                    },
                    character_id: (scope == InventoryScope::Character).then(|| owner_id.clone()),
                },
                &record,
            )
            .unwrap();
            if let Some(aesthetic) = scenario.get("lanternAesthetic").and_then(Value::as_str) {
                quilltap_core::db::database_store::write_database_document(
                    mount.connection(),
                    &mount_point_id,
                    spec["lanternAestheticsFilename"].as_str().unwrap(),
                    aesthetic,
                    quilltap_core::inventory::lint_scope::TitleLintScope::Linted,
                )
                .unwrap();
            }
            for sql in scenario
                .get("sql")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
            {
                main.connection()
                    .execute_batch(sql.as_str().unwrap())
                    .unwrap();
            }
            baked
        };
        let db = Db::open(
            DbPaths {
                main: mw.clone(),
                mount_index: Some(nw.clone()),
                llm_logs: Some(lw.clone()),
            },
            &pepper,
        )
        .unwrap();
        let calls = Arc::new(Mutex::new(Vec::<Value>::new()));
        let seams = WardrobeItemImageSeams {
            provider: ErasedImageGenerate::new(RecordingProvider {
                mode: scenario["provider"].as_str().unwrap().to_string(),
                webp: spec["webp"].as_str().unwrap().to_string(),
                calls: Arc::clone(&calls),
            }),
            codec: Arc::new(quilltap_host::HostImageCodec),
        };
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let user = spec["userId"].as_str().unwrap().to_string();
        let item_id = record.id().to_string();
        let home_of_object = |db: &Db| {
            let (u, o, i) = (user.clone(), owner_id.clone(), item_id.clone());
            db.read_main(|m| {
                db.read_mount_index(|n| resolve_inventory_item_home(m, n, &u, scope, Some(&o), &i))
            })
            .unwrap()
        };
        let home = home_of_object(&db).expect("home");
        let (result, lines) = captured_with(|| {
            rt.block_on(generate_inventory_item_image(
                &db,
                &seams,
                GenerateInventoryItemImageParams {
                    user_id: &user,
                    home: &home,
                    image_profile_id: scenario.get("imageProfileId").and_then(Value::as_str),
                },
            ))
        });
        let result = result_json(&result);
        let tables = db
            .read_main(|m| {
                db.read_mount_index(|n| {
                    // The oracle's key order (`files`, then the six mount
                    // tables) — the normalizer numbers minted ids by first
                    // sight, so both sides walk the dump in one order.
                    let all = dump_all(m, n);
                    let mut tables = Map::new();
                    tables.insert("files".into(), all["files"].clone());
                    for (k, v) in all.as_object().unwrap() {
                        if k != "files" {
                            tables.insert(k.clone(), v.clone());
                        }
                    }
                    Ok(Value::Object(tables))
                })
            })
            .unwrap();
        let ptrs = json!({
            "object": match home_of_object(&db) {
                Some(h) => json!(h.record.image_file_id()),
                None => json!("<gone>"),
            }
        });
        let got = json!({
            "result": result,
            "providerCalls": calls.lock().unwrap().clone(),
            "logs": blank_duration(rust_lines(&lines, &LOG_PREFIXES)),
            "pointers": ptrs,
            "tables": tables,
            "llmLogs": common::dump_llm_logs(&db),
            "conciergeRefusals": count(&db, "concierge_refusals"),
            "chatMessages": count(&db, "chat_messages"),
        });
        let want_s = json!({
            "result": want["result"],
            "providerCalls": want["providerCalls"],
            "logs": blank_duration(oracle_lines(&want["logs"])),
            "pointers": want["pointers"],
            "tables": want["tables"],
            "llmLogs": common::oracle_llm_logs(&want["llmLogs"]),
            "conciergeRefusals": want["conciergeRefusals"],
            "chatMessages": want["chatMessages"],
        });
        let got_n = Normalizer::new(baked.clone()).value(&got);
        let want_n = Normalizer::new(baked).value(&want_s);
        for part in [
            "result",
            "providerCalls",
            "logs",
            "pointers",
            "llmLogs",
            "conciergeRefusals",
            "chatMessages",
        ] {
            if got_n[part] != want_n[part] {
                failures.push(format!(
                    "{name}: {part} diverged\n  rust:   {}\n  oracle: {}",
                    got_n[part], want_n[part]
                ));
            }
        }
        for (k, t) in want_n["tables"].as_object().unwrap() {
            if &got_n["tables"][k] != t {
                failures.push(format!(
                    "{name}: table {k} diverged\n  rust:   {}\n  oracle: {}",
                    got_n["tables"][k], t
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} divergence(s):\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
