//! Tier-2 (real-DB) differential (P4.D279, keystone C1 §1): the Wearable Pool
//! LOADER — `quilltap_core::wearable_pool::{load_wearable_pool,
//! resolve_project_tier_for_chat, load_cast_pools, component_graph}` +
//! `db::wardrobe_read::read_shared_tiers` vs v4's REAL `lib/wardrobe/pool.ts`
//! and `WardrobeRepository.readSharedTiers` at `4a38222e6`, over the SAME baked
//! four-tier fixture (`build-wearable-pool-fixture.ts`: Ada in The Guild and on
//! the ADMIT roster, Bea in no group; items in General / two project stores /
//! the group store / both vaults — the unmask id, the project-over-General
//! duplicate, a General composite reaching a GROUP sash, a filed gown).
//!
//! Every op compares the pool's ORDERED `byId`, `wearable()`, `tiers`, the
//! probe `get`s, `getMany`, `componentGraph`, `owns`, `findByTitle` EXACTLY (no
//! normalisation — every id and stamp is pinned, the store ids are read back
//! from the owner rows and carried in the oracle's meta row), AND the
//! wardrobe-family log lines: v4's `[WearablePool] …` + `Read shared wardrobe
//! tiers` lines (level, message, fields by name and value) against v5's
//! `quilltap_core::wearable_pool` / `quilltap_core::db::wardrobe_read` targets.
//! Lines are compared as a SORTED multiset per op: v4 reads General and the
//! project tier under `Promise.all` and a cast's pools concurrently, so its
//! emission order is an interleaving v5's sequential reads need not reproduce.
//!
//! Pinned NEGATIVE on both sides (unreachable through v4's real code): the
//! repository's `Failed to read shared wardrobe tier; skipping` WARN — v4's
//! `readSharedWardrobe` answers `[]` for an unknown mount (the BOGUS id in the
//! `read-shared-tiers` ops) rather than throwing; `[WearablePool] Project lookup
//! for chat failed` (the chat read is a fallback `safeQuery`); `[WearablePool]
//! Failed to read the group wardrobe tier; skipping it` (`readSharedTiers`
//! catches per mount). The test asserts the oracle never recorded them and v5
//! never emits them.
//!
//! RED-FIRST: cannot be red on pre-keystone main (no `load_wearable_pool`);
//! recorded, not measured.
//!
//! Build the fixtures + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   rm -f /tmp/qt-wpool-main.db /tmp/qt-wpool-mount.db
//!   QT_FIXTURE_WPOOL_MAIN=/tmp/qt-wpool-main.db QT_FIXTURE_WPOOL_MOUNT=/tmp/qt-wpool-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-wearable-pool-fixture.ts
//!   QT_FIXTURE_WPOOL_MAIN=/tmp/qt-wpool-main.db QT_FIXTURE_WPOOL_MOUNT=/tmp/qt-wpool-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/cases/wearable-pool-tier2.ts \
//!     > /tmp/oracle-wearable-pool-t2.ndjson
//! Run:
//!   QT_ORACLE_WEARABLE_POOL_T2=/tmp/oracle-wearable-pool-t2.ndjson \
//!   QT_FIXTURE_WPOOL_MAIN=/tmp/qt-wpool-main.db QT_FIXTURE_WPOOL_MOUNT=/tmp/qt-wpool-mount.db \
//!     cargo test -p quilltap-harness --test wearable_pool_tier2_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use quilltap_core::db::archetype_wardrobe::{
    general_wardrobe_origin, WardrobeOrigin, WardrobeOriginScope,
};
use quilltap_core::db::wardrobe_read::read_shared_tiers;
use quilltap_core::db::Writer;
use quilltap_core::wearable_pool::{
    component_graph, load_cast_pools, load_wearable_pool, resolve_project_tier_for_chat,
    LoadWearablePoolOptions, WearablePool,
};
use serde::Deserialize;
use serde_json::{json, Value};

const BASELINE: &str = "p4.d279-wearable-pool-tier2";

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "seedTimestamp")]
    seed_timestamp: String,
    #[serde(rename = "adaId")]
    ada_id: String,
    #[serde(rename = "beaId")]
    bea_id: String,
    #[serde(rename = "admitProjectId")]
    admit_project_id: String,
    #[serde(rename = "admitProjectName")]
    admit_project_name: String,
    #[serde(rename = "generalMountPointId")]
    general_mount_point_id: String,
    #[serde(rename = "bogusMountPointId")]
    bogus_mount_point_id: String,
    #[serde(rename = "admitChatId")]
    admit_chat_id: String,
    #[serde(rename = "excludeChatId")]
    exclude_chat_id: String,
    #[serde(rename = "noProjectChatId")]
    no_project_chat_id: String,
    #[serde(rename = "missingChatId")]
    missing_chat_id: String,
    items: HashMap<String, String>,
}

#[derive(Deserialize)]
struct Stores {
    admit: String,
    #[allow(dead_code)]
    exclude: String,
    #[allow(dead_code)]
    group: String,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Row {
    Meta {
        baseline: String,
        stores: Stores,
    },
    Pool {
        name: String,
        pool: Value,
        lines: Vec<Value>,
    },
    Cast {
        name: String,
        pools: Vec<CastEntry>,
        lines: Vec<Value>,
    },
    ProjectTier {
        name: String,
        #[serde(rename = "chatId")]
        chat_id: Option<String>,
        operator: bool,
        #[serde(rename = "projectMountPointIds")]
        project_mount_point_ids: Vec<String>,
        lines: Vec<Value>,
    },
    SharedTiers {
        name: String,
        #[serde(rename = "mountPointIds")]
        mount_point_ids: Vec<String>,
        #[serde(rename = "includeArchived")]
        include_archived: bool,
        items: Vec<Value>,
        lines: Vec<Value>,
    },
}

#[derive(Deserialize)]
struct CastEntry {
    #[serde(rename = "characterId")]
    character_id: String,
    pool: Value,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/wearable-pool-tier2.json")
}

// ---------------------------------------------------------------------------
// A STRUCTURAL capture of the two targets' wardrobe-family lines.
// ---------------------------------------------------------------------------

const TARGETS: &[&str] = &[
    "quilltap_core::wearable_pool",
    "quilltap_core::db::wardrobe_read",
];
/// The lines the oracle records (v4's `pool.ts` + `readSharedTiers`).
const MESSAGES: &[&str] = &[
    "[WearablePool] Loaded wearable pool",
    "[WearablePool] Character off project roster — project wardrobe withheld",
    "[WearablePool] Project lookup for chat failed",
    "[WearablePool] Failed to read shared wardrobe tiers; continuing without them",
    "[WearablePool] Failed to read the character wardrobe; using shared tiers alone",
    "[WearablePool] Failed to read the group wardrobe tier; skipping it",
    "Read shared wardrobe tiers",
    "Failed to read shared wardrobe tier; skipping",
];
/// Unreachable through v4's real code — never recorded, never emitted.
const NEGATIVE: &[&str] = &[
    "[WearablePool] Project lookup for chat failed",
    "[WearablePool] Failed to read the group wardrobe tier; skipping it",
    "Failed to read shared wardrobe tier; skipping",
];

/// `(level, message, sorted (field, value) pairs)`.
type Line = (String, String, Vec<(String, String)>);

struct LineVisitor(String, Vec<(String, String)>);
impl tracing::field::Visit for LineVisitor {
    fn record_str(&mut self, f: &tracing::field::Field, v: &str) {
        self.1.push((f.name().to_string(), v.to_string()));
    }
    fn record_i64(&mut self, f: &tracing::field::Field, v: i64) {
        self.1.push((f.name().to_string(), v.to_string()));
    }
    fn record_u64(&mut self, f: &tracing::field::Field, v: u64) {
        self.1.push((f.name().to_string(), v.to_string()));
    }
    fn record_bool(&mut self, f: &tracing::field::Field, v: bool) {
        self.1.push((f.name().to_string(), v.to_string()));
    }
    fn record_debug(&mut self, f: &tracing::field::Field, v: &dyn std::fmt::Debug) {
        if f.name() == "message" {
            self.0 = format!("{v:?}");
        } else {
            self.1.push((f.name().to_string(), format!("{v:?}")));
        }
    }
}

struct Capture(Arc<Mutex<Vec<Line>>>);
impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Capture {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let meta = event.metadata();
        if !TARGETS.contains(&meta.target()) {
            return;
        }
        let mut v = LineVisitor(String::new(), Vec::new());
        event.record(&mut v);
        if !MESSAGES.contains(&v.0.as_str()) {
            return;
        }
        v.1.sort();
        self.0
            .lock()
            .unwrap()
            .push((meta.level().to_string().to_lowercase(), v.0, v.1));
    }
}

/// Run `f` under the capture; hand back its value and the SORTED lines.
fn captured<T>(f: impl FnOnce() -> T) -> (T, Vec<Line>) {
    use tracing_subscriber::layer::SubscriberExt;
    let logs = Arc::new(Mutex::new(Vec::<Line>::new()));
    let subscriber = tracing_subscriber::registry().with(Capture(logs.clone()));
    let out = {
        let _guard = tracing::subscriber::set_default(subscriber);
        f()
    };
    let mut lines = logs.lock().unwrap().clone();
    lines.sort();
    (out, lines)
}

/// v4's recorded `{ level, message, fields }` lines in the capture's shape, SORTED.
fn v4_lines(v: &[Value]) -> Vec<Line> {
    let mut out: Vec<Line> = v
        .iter()
        .map(|l| {
            let mut fields: Vec<(String, String)> = l["fields"]
                .as_object()
                .map(|o| {
                    o.iter()
                        .map(|(k, v)| {
                            let text = match v {
                                Value::String(s) => s.clone(),
                                other => other.to_string(),
                            };
                            (k.clone(), text)
                        })
                        .collect()
                })
                .unwrap_or_default();
            fields.sort();
            (
                l["level"].as_str().unwrap_or_default().to_string(),
                l["message"].as_str().unwrap_or_default().to_string(),
                fields,
            )
        })
        .collect();
    out.sort();
    out
}

fn assert_lines(name: &str, got: &[Line], want: &[Line]) {
    assert_eq!(
        got, want,
        "[{name}] log lines diverged\n  rust:   {got:#?}\n  oracle: {want:#?}"
    );
    for (_, m, _) in want.iter().chain(got.iter()) {
        assert!(
            !NEGATIVE.contains(&m.as_str()),
            "[{name}] a NEGATIVE-pinned line was emitted: {m}"
        );
    }
}

// ---------------------------------------------------------------------------
// The pool summary — the oracle's `summarize(pool)` shape.
// ---------------------------------------------------------------------------

fn summarize(pool: &WearablePool, probe_ids: &[String], ensemble: &str) -> Value {
    let by_id: Vec<Value> = pool.by_id_ordered().cloned().collect();
    let titles = [
        "Guild livery",
        "my livery",
        "Project badge",
        "Retired breeches",
        "Evening gown",
    ];
    let find_by_title: Vec<Value> = titles
        .iter()
        .map(|q| json!({ "query": q, "item": pool.find_by_title(q).cloned().unwrap_or(Value::Null) }))
        .collect();
    json!({
        "characterId": pool.character_id,
        "tiers": {
            "groupMountPointIds": pool.tiers.group_mount_point_ids,
            "projectMountPointIds": pool.tiers.project_mount_point_ids,
        },
        "byId": by_id,
        "wearable": pool.wearable(),
        "get": probe_ids
            .iter()
            .map(|id| json!({ "id": id, "item": pool.get(id).cloned().unwrap_or(Value::Null) }))
            .collect::<Vec<_>>(),
        "getMany": pool.get_many(probe_ids).into_iter().cloned().collect::<Vec<_>>(),
        "ensembleGraph": component_graph(pool, &[ensemble.to_string()]),
        "owns": by_id
            .iter()
            .map(|it| json!({ "id": it["id"], "owns": pool.owns(it) }))
            .collect::<Vec<_>>(),
        "findByTitle": find_by_title,
    })
}

/// Byte-compare two JSON values (`serde_json::Value` equality is key-order
/// blind on maps; the oracle's key order is v4's, so compare the text too).
fn assert_json(name: &str, got: &Value, want: &Value) {
    assert_eq!(
        got, want,
        "[{name}] diverged\n  rust:   {got}\n  oracle: {want}"
    );
    assert_eq!(
        serde_json::to_string(got).unwrap(),
        serde_json::to_string(want).unwrap(),
        "[{name}] key order diverged"
    );
}

#[test]
fn wearable_pool_tier2_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_WEARABLE_POOL_T2") else {
        eprintln!("SKIP: set QT_ORACLE_WEARABLE_POOL_T2 to the oracle NDJSON (see header).");
        return;
    };
    let Ok(main_fixture) = std::env::var("QT_FIXTURE_WPOOL_MAIN") else {
        eprintln!("SKIP: set QT_FIXTURE_WPOOL_MAIN to the main fixture .db (header).");
        return;
    };
    let Ok(mount_fixture) = std::env::var("QT_FIXTURE_WPOOL_MOUNT") else {
        eprintln!("SKIP: set QT_FIXTURE_WPOOL_MOUNT to the mount fixture .db (header).");
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let text = std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let rows: Vec<Row> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<Row>(l).expect("oracle row"))
        .collect();

    let Some(Row::Meta { baseline, stores }) = rows.first() else {
        panic!("the oracle's first row must be the meta row");
    };
    assert_eq!(
        baseline, BASELINE,
        "the oracle NDJSON predates this lane — regenerate it"
    );
    let admit_store = stores.admit.clone();

    // Fresh copies so the shared seed fixtures stay pristine (TRUNCATE-mode
    // `-journal` files land beside each copy; the scratch dir goes on drop).
    let scratch = tempfile::Builder::new()
        .prefix("qt-wpool-rust-")
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
    let docs = mount_w.doc_mount_documents();

    let item = |k: &str| -> String {
        spec.items
            .get(k)
            .unwrap_or_else(|| panic!("spec item {k}"))
            .clone()
    };
    let probe_ids: Vec<String> = [
        "generalTopper",
        "duplicateProjectGeneral",
        "generalEnsemble",
        "generalArchivedBreeches",
        "projectApron",
        "excludeVeil",
        "groupSash",
        "unmaskLivery",
        "adaBoots",
        "adaEveningGown",
        "beaCoat",
        "handedCloak",
        "noSuch",
    ]
    .iter()
    .map(|k| item(k))
    .collect();
    let ensemble = item("generalEnsemble");
    let handed: Value = json!({
        "id": item("handedCloak"),
        "characterId": null,
        "title": "Handed cloak",
        "description": null,
        "imagePrompt": null,
        "types": ["top"],
        "componentItemIds": [item("groupSash")],
        "appropriateness": null,
        "isDefault": true,
        "replace": false,
        "migratedFromClothingRecordId": null,
        "archivedAt": null,
        "folder": "",
        "createdAt": spec.seed_timestamp,
        "updatedAt": spec.seed_timestamp,
    });
    let admit_ids = vec![admit_store.clone()];
    let empty_ids: Vec<String> = Vec::new();

    let chat_of = |label: &str| -> Option<&str> {
        match label {
            "admit" => Some(spec.admit_chat_id.as_str()),
            "exclude" => Some(spec.exclude_chat_id.as_str()),
            "no-project" => Some(spec.no_project_chat_id.as_str()),
            "missing" => Some(spec.missing_chat_id.as_str()),
            "null" => None,
            "empty" => Some(""),
            other => panic!("unknown chat label {other}"),
        }
    };

    let (mut pools, mut casts, mut tiers_ops, mut shared_ops) = (0, 0, 0, 0);
    let (mut withheld, mut unmask_seen, mut cross_tier_graph, mut filed_seen) = (0, 0, 0, 0);

    for row in rows.iter().skip(1) {
        match row {
            Row::Meta { .. } => panic!("a second meta row"),

            Row::Pool {
                name,
                pool: want,
                lines,
            } => {
                let ada = spec.ada_id.as_str();
                let (got, got_lines) = captured(|| match name.as_str() {
                    "explicit-project" => load_wearable_pool(
                        main,
                        mount,
                        ada,
                        Some(&admit_ids),
                        LoadWearablePoolOptions::default(),
                    ),
                    "explicit-empty-project" => load_wearable_pool(
                        main,
                        mount,
                        ada,
                        Some(&empty_ids),
                        LoadWearablePoolOptions {
                            chat_id: Some(&spec.exclude_chat_id),
                            ..Default::default()
                        },
                    ),
                    "chat-admit" => load_wearable_pool(
                        main,
                        mount,
                        ada,
                        None,
                        LoadWearablePoolOptions {
                            chat_id: Some(&spec.admit_chat_id),
                            ..Default::default()
                        },
                    ),
                    "chat-exclude" => load_wearable_pool(
                        main,
                        mount,
                        ada,
                        None,
                        LoadWearablePoolOptions {
                            chat_id: Some(&spec.exclude_chat_id),
                            ..Default::default()
                        },
                    ),
                    "chat-exclude-operator" => load_wearable_pool(
                        main,
                        mount,
                        ada,
                        None,
                        LoadWearablePoolOptions {
                            chat_id: Some(&spec.exclude_chat_id),
                            operator: true,
                            ..Default::default()
                        },
                    ),
                    "no-chat" => load_wearable_pool(
                        main,
                        mount,
                        ada,
                        None,
                        LoadWearablePoolOptions::default(),
                    ),
                    "chat-no-project" => load_wearable_pool(
                        main,
                        mount,
                        ada,
                        None,
                        LoadWearablePoolOptions {
                            chat_id: Some(&spec.no_project_chat_id),
                            ..Default::default()
                        },
                    ),
                    "chat-missing" => load_wearable_pool(
                        main,
                        mount,
                        ada,
                        None,
                        LoadWearablePoolOptions {
                            chat_id: Some(&spec.missing_chat_id),
                            ..Default::default()
                        },
                    ),
                    "own-items-handed" => load_wearable_pool(
                        main,
                        mount,
                        ada,
                        Some(&admit_ids),
                        LoadWearablePoolOptions {
                            own_items: Some(std::slice::from_ref(&handed)),
                            ..Default::default()
                        },
                    ),
                    "bea-no-groups" => load_wearable_pool(
                        main,
                        mount,
                        &spec.bea_id,
                        Some(&admit_ids),
                        LoadWearablePoolOptions::default(),
                    ),
                    other => panic!("unknown pool op {other}"),
                });
                let summary = summarize(&got, &probe_ids, &ensemble);
                assert_json(name, &summary, want);
                assert_lines(name, &got_lines, &v4_lines(lines));

                if got_lines.iter().any(|(_, m, _)| {
                    m == "[WearablePool] Character off project roster — project wardrobe withheld"
                }) {
                    withheld += 1;
                }
                // The unmask witness: byId holds the archived personal copy,
                // wearable() the group's live one.
                let unmask = item("unmaskLivery");
                if let Some(b) = got.get(&unmask) {
                    let w = got.wearable().iter().find(|i| i["id"] == unmask);
                    if quilltap_core::wearable_pool::is_archived_truthy(b)
                        && w.is_some_and(|w| !quilltap_core::wearable_pool::is_archived_truthy(w))
                    {
                        unmask_seen += 1;
                    }
                }
                // The cross-tier walk: General composite → GROUP sash.
                let graph = component_graph(&got, std::slice::from_ref(&ensemble));
                if graph.len() == 2
                    && graph[1]["origin"]["scope"] == "group"
                    && graph[0]["origin"]["scope"] == "general"
                {
                    cross_tier_graph += 1;
                }
                if got
                    .get(&item("adaEveningGown"))
                    .is_some_and(|g| g["folder"] == "Formal/Evening")
                {
                    filed_seen += 1;
                }
                pools += 1;
            }

            Row::Cast {
                name,
                pools: want,
                lines,
            } => {
                let ids = vec![spec.ada_id.clone(), spec.bea_id.clone()];
                let (got, got_lines) =
                    captured(|| load_cast_pools(main, mount, Some(&spec.admit_project_id), &ids));
                assert_eq!(got.len(), want.len(), "[{name}] cast size");
                for ((gid, gpool), w) in got.iter().zip(want.iter()) {
                    assert_eq!(gid, &w.character_id, "[{name}] cast order");
                    assert_json(
                        &format!("{name}/{gid}"),
                        &summarize(gpool, &probe_ids, &ensemble),
                        &w.pool,
                    );
                }
                assert_lines(name, &got_lines, &v4_lines(lines));
                // One shared read for the cast: exactly ONE General line and
                // ONE project line across both pools.
                let shared_reads = got_lines
                    .iter()
                    .filter(|(_, m, f)| {
                        m == "Read shared wardrobe tiers"
                            && f.iter()
                                .any(|(k, v)| k == "itemCount" && (v == "4" || v == "2"))
                    })
                    .count();
                assert!(shared_reads >= 2, "[{name}] the shared tiers were read");
                casts += 1;
            }

            Row::ProjectTier {
                name,
                chat_id,
                operator,
                project_mount_point_ids: want,
                lines,
            } => {
                let label = name
                    .trim_start_matches("resolve-project-tier-")
                    .trim_end_matches("-operator");
                let chat = chat_of(label);
                // The oracle's own chatId must agree with the spec's.
                assert_eq!(
                    chat.map(str::to_string),
                    chat_id.clone(),
                    "[{name}] the oracle's chatId"
                );
                let (got, got_lines) = captured(|| {
                    resolve_project_tier_for_chat(main, mount, chat, Some(&spec.ada_id), *operator)
                });
                assert_eq!(&got, want, "[{name}] projectMountPointIds");
                assert_lines(name, &got_lines, &v4_lines(lines));
                if got_lines.iter().any(|(_, m, _)| m.contains("withheld")) {
                    withheld += 1;
                }
                tiers_ops += 1;
            }

            Row::SharedTiers {
                name,
                mount_point_ids,
                include_archived,
                items: want,
                lines,
            } => {
                let general = spec.general_mount_point_id.clone();
                let admit = admit_store.clone();
                let project_origin = WardrobeOrigin::new(
                    WardrobeOriginScope::Project,
                    &spec.admit_project_id,
                    &spec.admit_project_name,
                );
                let bogus_origin =
                    WardrobeOrigin::new(WardrobeOriginScope::Project, "bogus", "Bogus");
                let origin_of = |mp: &str| -> WardrobeOrigin {
                    if mp == general {
                        general_wardrobe_origin()
                    } else if mp == admit {
                        project_origin.clone()
                    } else {
                        bogus_origin.clone()
                    }
                };
                if !mount_point_ids.is_empty() {
                    assert!(
                        mount_point_ids.contains(&spec.bogus_mount_point_id),
                        "[{name}] the bogus mount rides the list"
                    );
                }
                let (got, got_lines) = captured(|| {
                    read_shared_tiers(&docs, mount_point_ids, *include_archived, &origin_of)
                });
                assert_json(name, &Value::Array(got), &Value::Array(want.clone()));
                assert_lines(name, &got_lines, &v4_lines(lines));
                shared_ops += 1;
            }
        }
    }

    drop(main_w);
    drop(mount_w);

    assert_eq!(pools, 10, "every pool op ran");
    assert_eq!(casts, 1, "the cast op ran");
    assert_eq!(tiers_ops, 12, "every resolveProjectTierForChat op ran");
    assert_eq!(shared_ops, 3, "every readSharedTiers op ran");
    assert!(
        withheld >= 2,
        "the roster withheld the project tier ({withheld})"
    );
    assert!(
        unmask_seen >= 8,
        "the archived own copy unmasked the group's ({unmask_seen})"
    );
    assert!(
        cross_tier_graph >= 8,
        "the General composite reached the GROUP sash ({cross_tier_graph})"
    );
    assert!(
        filed_seen >= 8,
        "the filed gown kept its folder ({filed_seen})"
    );

    eprintln!(
        "OK: wearable-pool tier-2 matched the oracle on {} ops.",
        pools + casts + tiers_ops + shared_ops
    );
}
