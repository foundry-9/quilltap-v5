//! Tier-3 differential for the wardrobe item-image GENERATION (P4.D263 item
//! 6): `quilltap-core::services::wardrobe_item_image_generation::
//! generate_wardrobe_item_image` vs v4's REAL `generateWardrobeItemImage`
//! (`lib/wardrobe/item-image-generation.ts`, `7c8572869`), the image provider
//! mocked BELOW both (the P4.76 shape; the oracle header names every seam).
//!
//! Per scenario both sides take a fresh copy of the baked fixture, apply the
//! scenario's `sql` to main, resolve the item's home and run ONE generation;
//! then compare:
//!   - the result (`fileId` / `url` / `prompt` / `subject` / `profile` /
//!     `rerouted` / `trail` / the item's new pointer) or the typed error
//!     (`NoWardrobeImageProfileError` — the ONE sentence for no profile AND a
//!     gone key row; `WardrobeImageGenerationError` with `trail` and
//!     `refused`; `CharacterArchivedError` before any provider call);
//!   - the ordered provider calls `{provider, apiKey, params}` — the params are
//!     the shared builder's output with the prompt's ORIENTATION resolved per
//!     provider, `n: 1`, `style: 'natural'`; the `apiKey` proves which profile
//!     (and, on a reroute, the understudy) was asked;
//!   - the generation's own lines (`[WardrobeItemImage]`, `[hydrateComponent
//!     Graph]`; `durationMs` placeholdered) — the item-image write's lines are
//!     `wardrobe_item_images_tier2_equivalence`'s;
//!   - every item's pointer, the main `files` table and the six mount-index
//!     tables (the tier-2 normalizer);
//!   - the `llm_logs` rows — ONE `WARDROBE_ITEM_IMAGE` row per provider
//!     attempt, its `response.error` shape on a throw (id / timestamps /
//!     `durationMs` placeholdered by the shared helpers);
//!   - P4.D263 R-B's NEGATIVE pin: the `concierge_refusals` and
//!     `chat_messages` row counts — a refused wardrobe picture passes no chat,
//!     so v4 writes neither (both absent tables on this fixture, on both
//!     sides).
//!
//! Regenerate (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-wiig-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/wardrobe-item-image-generation.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/wardrobe-item-image-generation-tier3.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_WIIG_MAIN=/tmp/qt-wiig-main.db QT_FIXTURE_WIIG_MOUNT=/tmp/qt-wiig-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-wardrobe-item-image-generation-fixture.ts
//!   QT_FIXTURE_WIIG_MAIN=/tmp/qt-wiig-main.db QT_FIXTURE_WIIG_MOUNT=/tmp/qt-wiig-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-wardrobe-item-image-generation.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=600000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- "cases/wardrobe-item-image-generation\.test\.ts$"
//! Run:
//!   QT_ORACLE_WIIG=/tmp/oracle-wardrobe-item-image-generation.ndjson \
//!   QT_FIXTURE_WIIG_MAIN=/tmp/qt-wiig-main.db QT_FIXTURE_WIIG_MOUNT=/tmp/qt-wiig-mount.db \
//!     cargo test -p quilltap-harness --test wardrobe_item_image_generation_tier3_equivalence

mod common;
mod wardrobe_images_support;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use quilltap_core::api::types::WardrobeContainerScope;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::Writer;
use quilltap_core::model::image::{
    ErasedImageGenerate, GeneratedImageData, ImageGenError, ImageGenParams, ImageGenResponse,
    ImageProvider,
};
use quilltap_core::services::wardrobe_container::resolve_wardrobe_item_home;
use quilltap_core::services::wardrobe_item_image_generation::{
    generate_wardrobe_item_image, GenerateWardrobeItemImageArgs, WardrobeImageGenerationFailure,
    WardrobeItemImageSeams,
};
use quilltap_core::services::wardrobe_item_images::service::ItemImageError;
use quilltap_core::test_support::captured_with;
use serde_json::{json, Map, Value};
use wardrobe_images_support::{baked_tokens, dump_all, oracle_lines, rust_lines, Normalizer};

const LOG_PREFIXES: [&str; 2] = ["[WardrobeItemImage]", "[hydrateComponentGraph]"];
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

fn scope_of(s: &str) -> WardrobeContainerScope {
    serde_json::from_value(json!(s)).unwrap()
}

fn trail_json(t: &Option<Vec<quilltap_core::services::route_trail::RouteAttempt>>) -> Value {
    t.as_ref()
        .map(|t| serde_json::to_value(t).unwrap())
        .unwrap_or(Value::Null)
}

fn result_json(
    r: &Result<
        quilltap_core::services::wardrobe_item_image_generation::WardrobeItemImageGenerationResult,
        WardrobeImageGenerationFailure,
    >,
) -> Value {
    match r {
        Ok(r) => json!({
            "ok": true,
            "fileId": r.file_id,
            "url": r.url,
            "prompt": r.prompt,
            "subject": r.subject.as_str(),
            "profile": { "id": r.profile_id, "name": r.profile_name },
            "rerouted": r.rerouted,
            "trail": trail_json(&r.trail),
            "itemImageFileId": r.item.as_ref().and_then(|i| i.image_file_id.clone().flatten()),
        }),
        Err(WardrobeImageGenerationFailure::NoProfile) => json!({
            "ok": false,
            "error": "NoWardrobeImageProfileError",
            "message": quilltap_core::services::wardrobe_item_image_generation::NO_WARDROBE_IMAGE_PROFILE_MESSAGE,
            "trail": null,
            "refused": null,
        }),
        Err(WardrobeImageGenerationFailure::Generation {
            message,
            trail,
            refused,
        }) => json!({
            "ok": false,
            "error": "WardrobeImageGenerationError",
            "message": message,
            "trail": trail_json(trail),
            "refused": refused,
        }),
        Err(WardrobeImageGenerationFailure::Item(e)) => json!({
            "ok": false,
            "error": match e {
                ItemImageError::Archived { .. } => "CharacterArchivedError",
                ItemImageError::Foreign { .. } => "ForeignWardrobeImageError",
                ItemImageError::Failed(_) => "Error",
            },
            "message": e.message(),
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

fn container_of(spec: &Value, c: Option<&str>) -> Option<String> {
    match c {
        Some("character") => spec["characterId"].as_str().map(str::to_string),
        Some("archived") => spec["archivedCharacterId"].as_str().map(str::to_string),
        Some("project") => spec["projectId"].as_str().map(str::to_string),
        _ => None,
    }
}

async fn pointers(db: &Db, spec: &Value) -> Value {
    let mut m = Map::new();
    for (key, item) in spec["items"].as_object().unwrap() {
        let (scope, container) = match item["home"].as_str().unwrap() {
            "character" => ("character", Some("character")),
            "archived" => ("character", Some("archived")),
            "general" => ("general", None),
            _ => ("project", Some("project")),
        };
        let user = spec["userId"].as_str().unwrap().to_string();
        let cid = container_of(spec, container);
        let iid = item["id"].as_str().unwrap().to_string();
        let home = db
            .write(move |ws| {
                resolve_wardrobe_item_home(
                    ws.main().connection(),
                    ws.mount_index().unwrap().connection(),
                    &user,
                    scope_of(scope),
                    cid.as_deref(),
                    &iid,
                )
            })
            .await
            .unwrap();
        m.insert(
            key.clone(),
            match home {
                Some(h) => json!(h.image_file_id()),
                None => json!("<gone>"),
            },
        );
    }
    Value::Object(m)
}

#[test]
fn wardrobe_item_image_generation_tier3_matches_oracle() {
    let (Ok(oracle_path), Ok(main_fixture), Ok(mount_fixture)) = (
        std::env::var("QT_ORACLE_WIIG"),
        std::env::var("QT_FIXTURE_WIIG_MAIN"),
        std::env::var("QT_FIXTURE_WIIG_MOUNT"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_WIIG + QT_FIXTURE_WIIG_MAIN + QT_FIXTURE_WIIG_MOUNT (see header)."
        );
        return;
    };
    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/wardrobe-item-image-generation-tier3.json"),
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
            .prefix(&format!("qt-wiig-rust-{name}-"))
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
        let baked = {
            let main = Writer::open_writable(&mw, &pepper).unwrap();
            quilltap_core::test_support::ensure_wear_ledger_on(main.connection());
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
            let mount = Writer::open_writable(&nw, &pepper).unwrap();
            baked_tokens(&dump_all(main.connection(), mount.connection()), &spec)
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
        let container = container_of(&spec, scenario.get("container").and_then(Value::as_str));
        let item_id = spec["items"][scenario["item"].as_str().unwrap()]["id"]
            .as_str()
            .unwrap()
            .to_string();
        let scope = scope_of(scenario["scope"].as_str().unwrap());
        let home = {
            let (u, c, i) = (user.clone(), container.clone(), item_id.clone());
            rt.block_on(db.write(move |ws| {
                resolve_wardrobe_item_home(
                    ws.main().connection(),
                    ws.mount_index().unwrap().connection(),
                    &u,
                    scope,
                    c.as_deref(),
                    &i,
                )
            }))
            .unwrap()
            .expect("home")
        };
        let (result, lines) = captured_with(|| {
            rt.block_on(generate_wardrobe_item_image(
                &db,
                &seams,
                &GenerateWardrobeItemImageArgs {
                    user_id: &user,
                    home: &home,
                    container_id: container.as_deref(),
                    image_profile_id: scenario.get("imageProfileId").and_then(Value::as_str),
                },
            ))
        });
        let (tables, ptrs) = {
            let tables = db
                .read_main(|m| db.read_mount_index(|n| Ok(dump_all(m, n))))
                .unwrap();
            (tables, rt.block_on(pointers(&db, &spec)))
        };
        let got = json!({
            "result": result_json(&result),
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
