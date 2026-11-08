//! P4.D376 — the REACH differential: `quilltap_core::db::reach` vs v4's REAL
//! `lib/mount-index/reach.ts` (NEW in `0c47f4539`). Both sides run
//! `harness/oracle/fixtures/reach.json` over their OWN copies of the
//! tiered-mount-pool baked pair (`build-tiered-mount-pool-fixture.ts`), after
//! the spec's plants — so the tiered family's matrix is untouched.
//!
//! - **Tier 1 (exact):** `order_mounts`, `bucket_mounts`, `group_origin_map`
//!   over literal pools.
//! - **Tier 2 (real DB):** `resolve_owner_store` for every scope × `ensure`
//!   off / swallow / throw (a BEFORE INSERT trigger plants the folder failure:
//!   the `[Reach] Failed to ensure ${folder} folder` WARN, its `context`
//!   omitted when absent, and its silence legs), archived allow / refuse (the
//!   `[Reach] Archived character refused` DEBUG), another user's / a vaultless /
//!   a missing character, a project and a group with NO store (null without
//!   `ensure` — a read never provisions — minted with it: the store row, its
//!   folder and the owner pointer are dumped, the minted id masked), General
//!   unprovisioned; `classify_mount` (General, a live / archived vault, a
//!   project / group OFFICIAL store, a LINKED-only store → null, an unknown id);
//!   `cast_group_mounts` (named groups first, `''` and duplicate ids, cast order);
//!   `resolve_cast_pool` (an archived member — DEBUG, another user's member, a
//!   USERLESS member admitted, a vaultless member, a member whose vault is also a
//!   group store — excluded from `castVaults`, the RAW `namedGroupCount` with
//!   `[g, "", g]`, General absent), compared as BYTES (`serde_json::to_string` —
//!   v4's spread key order is the wire).
//! - **R-F, measured:** ONE corrupt project row and ONE corrupt group row (a
//!   BLOB `name`) read RAW and HYDRATED on both sides (`readWayArms`): v4's
//!   `findByIdRaw` and `findById` both answer `null` after `Data validation
//!   failed` + `Error finding entity by ID`, and `resolve_owner_store` answers
//!   `None` for both (with and without `ensure`).
//! - **Dead WARNs (T3.3):** `[Reach] Cast vault lookup failed; tier dropped for
//!   this character` and `[Reach] Quilltap General lookup failed; global tier
//!   dropped` — every read under them is a fallback `safeQuery` — pinned
//!   NEGATIVE by name on every arm.
//!
//! Every `[Reach]` line is compared on every arm (level, message, fields by
//! name and order, `error` omitted). `character` rows are projected to v4's
//! `ReachCharacterRow` keys (`null` for an absent key) on both sides.
//!
//! ⚠ PIN REQUIRED at `624d0d25c`. Regen (Node 24, from the pinned v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_TMP_MAIN=/tmp/qt-p4d376-tmp-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-p4d376-tmp-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/fixtures/build-tiered-mount-pool-fixture.ts
//!   QT_FIXTURE_TMP_MAIN=/tmp/qt-p4d376-tmp-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-p4d376-tmp-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/cases/reach.ts > /tmp/oracle-p4d376-reach.ndjson
//! Run:
//!   QT_ORACLE_REACH=/tmp/oracle-p4d376-reach.ndjson \
//!   QT_FIXTURE_TMP_MAIN=/tmp/qt-p4d376-tmp-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-p4d376-tmp-mount.db \
//!     cargo test -p quilltap-harness --test reach_equivalence -- --nocapture

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use quilltap_core::db::reach::{
    bucket_mounts, cast_group_mounts, classify_mount, group_origin_map, order_mounts,
    resolve_cast_pool, resolve_owner_store, Archived, BucketScope, CastPoolOptions, EnsureFolder,
    OwnerFolderFailure, OwnerScope, ResolveOwnerStoreOptions,
};
use quilltap_core::db::tiered_mount_pool::{GroupMounts, GroupRef, TieredMountPool};
use quilltap_core::db::Writer;
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Map, Value};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Tiered {
    test_pepper_base64: String,
}

#[derive(Deserialize, Clone)]
struct Sql {
    db: String,
    sql: String,
    #[serde(default)]
    params: Vec<Option<String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    plants: Vec<Sql>,
    pure: Pure,
    owner_arms: Vec<OwnerArm>,
    read_way_arms: Vec<ReadWayArm>,
    classify_arms: Vec<ClassifyArm>,
    cast_group_arms: Vec<CastGroupArm>,
    cast_pool_arms: Vec<CastPoolArm>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pure {
    order_arms: Vec<OrderArm>,
    bucket_arms: Vec<BucketArm>,
    origin_arms: Vec<OriginArm>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrderArm {
    id: String,
    pool: Value,
    include_participants: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BucketArm {
    id: String,
    pool: Value,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    participants_as_character: Option<bool>,
}

#[derive(Deserialize)]
struct OriginArm {
    id: String,
    groups: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EnsureSpec {
    folder: String,
    on_folder_failure: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OwnerArm {
    id: String,
    scope: String,
    owner_id: Option<String>,
    user_id: String,
    archived: String,
    #[serde(default)]
    ensure: Option<EnsureSpec>,
    #[serde(default)]
    context: Option<String>,
    #[serde(default)]
    before: Vec<Sql>,
    #[serde(default)]
    after: Vec<Sql>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReadWayArm {
    id: String,
    collection: String,
    owner_id: String,
    way: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClassifyArm {
    id: String,
    mount_point_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CastGroupArm {
    id: String,
    character_ids: Vec<String>,
    group_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CastPoolArm {
    id: String,
    user_id: String,
    project_id: Option<String>,
    character_ids: Vec<String>,
    group_ids: Vec<String>,
    #[serde(default)]
    before: Vec<Sql>,
    #[serde(default)]
    after: Vec<Sql>,
}

/// The two v4 WARNs measured UNREACHABLE (T3.3), pinned NEGATIVE.
const DEAD_WARNS: &[&str] = &[
    "[Reach] Cast vault lookup failed; tier dropped for this character",
    "[Reach] Quilltap General lookup failed; global tier dropped",
];
/// Two PRE-EXISTING v5 divergences this family measured (R-F's "record whether
/// they agree"), RECORDED both ways — the arm's v4 body must still be the
/// recorded v4 shape, and v5 must answer exactly the recorded v5 shape — so a
/// fix on either side reddens the row. Neither is in reach's own reads; both
/// are follow-up items in P4.D376's lane record.
///
/// - `HYDRATED_OWNER_ROW_UNVALIDATED`: v4's HYDRATED `findById` is
///   `_findById` + the overlay, so a corrupt slim row (a BLOB `name`) answers
///   `null` after `Data validation failed` + `Error finding entity by ID`;
///   v5's `ProjectsRepository::find_by_id` reads it (found) and
///   `GroupsRepository::find_by_id` fails it (`Err`), both silently. The RAW
///   reads reach uses (`*_raw_or_none`) AGREE with v4.
/// - `RAW_CHARACTER_ROW_UNVALIDATED`: a `userId: ''` character fails v4's
///   `CharacterSchema` (`userId: UUIDSchema`), so `findByIdRaw` answers `null`
///   and the cast member is skipped — v4's "a userless row is ADMITTED" branch
///   of `resolveCastPool` is therefore UNREACHABLE; v5's raw character read
///   does not validate, so v5 admits her (`liveCastCount=1`).
const RECORDED_DIVERGENCES: &[(&str, &str, &str, &[&str])] = &[
    (
        "rf_project_hydrated",
        "HYDRATED_OWNER_ROW_UNVALIDATED",
        r#"{"found":true}"#,
        &[],
    ),
    (
        "rf_group_hydrated",
        "HYDRATED_OWNER_ROW_UNVALIDATED",
        r#"{"threw":true}"#,
        &[],
    ),
    (
        "cast_pool_invalid_userless_row",
        "RAW_CHARACTER_ROW_UNVALIDATED",
        r#"{"value":{"characterMountPointId":null,"groupMountPointIds":[],"projectMountPointIds":[],"globalMountPointId":"e8000000-0000-4000-8000-0000000000a1","participantMountPointIds":[],"castVaults":[],"liveCharacterIds":["c8376000-0000-4000-8000-0000000000d2"]}}"#,
        &["DEBUG [Reach] Resolved cast pool castCount=1 liveCastCount=1 namedGroupCount=0 participants=0 groups=0 projects=0 hasGlobal=true"],
    ),
];

/// The read-way arms also compare the repository's two lines.
const READ_WAY_MESSAGES: &[&str] = &["Data validation failed", "Error finding entity by ID"];
const CHARACTER_KEYS: &[&str] = &[
    "id",
    "name",
    "userId",
    "archivedAt",
    "characterDocumentMountPointId",
    "defaultScenarioId",
];

fn spec_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures")
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

fn project_character(row: Option<&Value>) -> Option<Value> {
    let row = row?;
    let mut out = Map::new();
    for k in CHARACTER_KEYS {
        out.insert(
            (*k).to_string(),
            row.get(*k)
                .filter(|v| !v.is_null())
                .cloned()
                .unwrap_or(Value::Null),
        );
    }
    Some(Value::Object(out))
}

fn pool_of(v: &Value) -> TieredMountPool {
    let list = |k: &str| -> Vec<String> {
        v[k].as_array()
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    };
    let one = |k: &str| v[k].as_str().map(str::to_string);
    TieredMountPool {
        character_mount_point_id: one("characterMountPointId"),
        participant_mount_point_ids: list("participantMountPointIds"),
        group_mount_point_ids: list("groupMountPointIds"),
        project_mount_point_ids: list("projectMountPointIds"),
        global_mount_point_id: one("globalMountPointId"),
    }
}

/// v4's recorded lines → `LEVEL message k=v…`.
fn want_lines(logs: &Value) -> Vec<String> {
    logs.as_array()
        .map(|a| {
            a.iter()
                .map(|l| {
                    let mut line = format!(
                        "{} {}",
                        l["level"].as_str().unwrap_or_default().to_uppercase(),
                        l["message"].as_str().unwrap_or_default()
                    );
                    for pair in l["fields"].as_array().into_iter().flatten() {
                        line.push_str(&format!(
                            " {}={}",
                            pair[0].as_str().unwrap_or_default(),
                            pair[1].as_str().unwrap_or_default()
                        ));
                    }
                    line
                })
                .collect()
        })
        .unwrap_or_default()
}

/// v5's rendered lines (`LEVEL target message k=v…`) → `LEVEL message k=v…`
/// for `[Reach]` lines and the `extra` messages; the target is dropped, the
/// trailing `error` field cut.
fn got_lines(lines: &[String], extra: &[&str]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|l| {
            let at = l
                .find("[Reach]")
                .or_else(|| extra.iter().find_map(|m| l.find(m)))?;
            let level = l.split(' ').next().unwrap_or_default();
            let tail = &l[at..];
            let tail = match tail.find(" error=") {
                Some(i) => &tail[..i],
                None => tail,
            };
            Some(format!("{level} {tail}"))
        })
        .collect()
}

fn run_sql(main: &Connection, mount: &Connection, sqls: &[Sql]) {
    for s in sqls {
        let conn = if s.db == "main" { main } else { mount };
        conn.execute_batch(&s.sql)
            .unwrap_or_else(|e| panic!("sql `{}`: {e}", s.sql));
    }
}

fn scope_of(s: &str) -> OwnerScope {
    match s {
        "character" => OwnerScope::Character,
        "group" => OwnerScope::Group,
        "project" => OwnerScope::Project,
        "general" => OwnerScope::General,
        other => panic!("scope {other}"),
    }
}

#[test]
fn reach_matches_oracle() {
    let (Some(oracle_path), Some(main_fixture), Some(mount_fixture)) = (
        env_or_skip("QT_ORACLE_REACH"),
        env_or_skip("QT_FIXTURE_TMP_MAIN"),
        env_or_skip("QT_FIXTURE_TMP_MOUNT"),
    ) else {
        return;
    };
    let tiered: Tiered = serde_json::from_str(
        &std::fs::read_to_string(spec_dir().join("tiered-mount-pool.json")).unwrap(),
    )
    .unwrap();
    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_dir().join("reach.json")).unwrap())
            .expect("parse reach.json");

    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let row: Value = serde_json::from_str(line).expect("oracle line parses");
        oracle.insert(row["id"].as_str().unwrap().to_string(), row);
    }
    assert!(!oracle.is_empty(), "the oracle is empty — regenerate it");

    let scratch = tempfile::Builder::new()
        .prefix("qt-reach-rust-")
        .tempdir()
        .expect("tempdir");
    let main_work = scratch.path().join("main.db");
    let mount_work = scratch.path().join("mount.db");
    std::fs::copy(&main_fixture, &main_work).unwrap();
    std::fs::copy(&mount_fixture, &mount_work).unwrap();
    let main_w = Writer::open_writable(&main_work, &tiered.test_pepper_base64).unwrap();
    let mount_w = Writer::open_writable(&mount_work, &tiered.test_pepper_base64).unwrap();
    let main = main_w.connection();
    let mount = mount_w.connection();

    let vault_of = |cid: &str| -> String {
        main.query_row(
            "SELECT characterDocumentMountPointId FROM characters WHERE id = ?1",
            [cid],
            |r| r.get::<_, String>(0),
        )
        .unwrap_or_else(|e| panic!("vault of {cid}: {e}"))
    };
    // The plants, `@vault:<id>` resolved off the main copy (main first, as v4).
    for db in ["main", "mount"] {
        for p in spec.plants.iter().filter(|p| p.db == db) {
            let params: Vec<Option<String>> = p
                .params
                .iter()
                .map(|v| match v.as_deref() {
                    Some(t) if t.starts_with("@vault:") => Some(vault_of(&t[7..])),
                    _ => v.clone(),
                })
                .collect();
            let conn = if db == "main" { main } else { mount };
            conn.execute(&p.sql, rusqlite::params_from_iter(params.iter()))
                .unwrap_or_else(|e| panic!("plant `{}`: {e}", p.sql));
        }
    }

    let mut failures: Vec<String> = Vec::new();
    let mut driven: HashSet<String> = HashSet::new();
    let mut check = |id: &str, got: Value, lines: &[String], extra: &[&str]| {
        driven.insert(id.to_string());
        for dead in DEAD_WARNS {
            assert!(
                !lines.iter().any(|l| l.contains(dead)),
                "{id}: the unreachable WARN `{dead}` fired on v5"
            );
        }
        let want = oracle
            .get(id)
            .unwrap_or_else(|| panic!("oracle missing `{id}`"));
        // `retain`, not `remove`: under `preserve_order` a `Map::remove` is a
        // swap-remove and would reorder the keys this byte compare reads.
        let want_logs = want.get("logs").cloned().unwrap_or(Value::Null);
        let mut want_body = want.clone();
        if let Some(o) = want_body.as_object_mut() {
            o.retain(|k, _| k != "id" && k != "kind" && k != "logs");
        }
        let (w, g) = (want_lines(&want_logs), got_lines(lines, extra));
        if let Some((_, name, v5_body, v5_lines)) =
            RECORDED_DIVERGENCES.iter().find(|(arm, ..)| *arm == id)
        {
            // Both ways: v4 still differs from the recorded v5 shape, and v5
            // answers exactly that shape.
            let got_text = serde_json::to_string(&got).unwrap();
            assert_ne!(
                serde_json::to_string(&want_body).unwrap(),
                *v5_body,
                "{id}: {name} VANISHED on the v4 side — retire the row"
            );
            assert_eq!(got_text, *v5_body, "{id}: {name} moved on v5");
            assert_eq!(
                g,
                v5_lines.iter().map(|l| l.to_string()).collect::<Vec<_>>(),
                "{id}: {name} lines moved on v5"
            );
            return;
        }
        if serde_json::to_string(&got).unwrap() != serde_json::to_string(&want_body).unwrap() {
            failures.push(format!("{id}: body\n  v4: {want_body}\n  v5: {got}"));
        }
        if !want_logs.is_null() && w != g {
            failures.push(format!("{id}: lines\n  v4: {w:?}\n  v5: {g:?}"));
        }
    };

    // ── Tier 1 ──────────────────────────────────────────────────────────────
    for a in &spec.pure.order_arms {
        let got = order_mounts(&pool_of(&a.pool), a.include_participants);
        check(&a.id, json!({ "value": got }), &[], &[]);
    }
    for a in &spec.pure.bucket_arms {
        let scope = match a.scope.as_deref() {
            None | Some("all") => BucketScope::All,
            Some("character") => BucketScope::Character,
            Some("group") => BucketScope::Group,
            Some("project") => BucketScope::Project,
            Some(other) => panic!("bucket scope {other}"),
        };
        let got = bucket_mounts(
            &pool_of(&a.pool),
            scope,
            a.participants_as_character.unwrap_or(false),
        );
        check(&a.id, json!({ "value": got }), &[], &[]);
    }
    for a in &spec.pure.origin_arms {
        let groups: Vec<GroupMounts> = a
            .groups
            .iter()
            .map(|g| GroupMounts {
                group: GroupRef {
                    id: g["group"]["id"].as_str().unwrap().to_string(),
                    name: g["group"]["name"].as_str().unwrap().to_string(),
                },
                mount_point_ids: g["mountPointIds"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().to_string())
                    .collect(),
            })
            .collect();
        let entries: Vec<Value> = group_origin_map(&groups)
            .into_iter()
            .map(|(k, v)| json!([k, v]))
            .collect();
        check(&a.id, json!({ "value": entries }), &[], &[]);
    }

    // ── Tier 2 ──────────────────────────────────────────────────────────────
    let preexisting: HashSet<String> = {
        let mut stmt = mount.prepare("SELECT id FROM doc_mount_points").unwrap();
        stmt.query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    let mask = |id: &str| -> Value {
        if preexisting.contains(id) {
            json!(id)
        } else {
            json!("<minted>")
        }
    };

    for a in &spec.owner_arms {
        run_sql(main, mount, &a.before);
        let ensure = a.ensure.as_ref().map(|e| EnsureFolder {
            folder: &e.folder,
            on_folder_failure: match e.on_folder_failure.as_str() {
                "swallow" => OwnerFolderFailure::Swallow,
                "throw" => OwnerFolderFailure::Throw,
                other => panic!("onFolderFailure {other}"),
            },
        });
        let opts = ResolveOwnerStoreOptions {
            ensure,
            archived: if a.archived == "refuse" {
                Archived::Refuse
            } else {
                Archived::Allow
            },
            context: a.context.as_deref(),
        };
        let (res, lines) = quilltap_core::test_support::captured_with(|| {
            resolve_owner_store(
                main,
                mount,
                scope_of(&a.scope),
                a.owner_id.as_deref(),
                &a.user_id,
                &opts,
            )
        });
        run_sql(main, mount, &a.after);
        let got = match res {
            Err(_) => json!({ "threw": true, "dump": null }),
            Ok(None) => json!({ "value": null, "dump": null }),
            Ok(Some(store)) => {
                let mut value = json!({
                    "scope": store.scope,
                    "mountPointId": mask(&store.mount_point_id),
                    "owner": store.owner,
                });
                if let Some(c) = project_character(store.character.as_ref()) {
                    value["character"] = c;
                }
                let mp: Value = mount
                    .query_row(
                        "SELECT name, storeType, mountType, enabled FROM doc_mount_points WHERE id = ?1",
                        [&store.mount_point_id],
                        |r| {
                            Ok(json!({
                                "name": r.get::<_, String>(0)?,
                                "storeType": r.get::<_, Option<String>>(1)?,
                                "mountType": r.get::<_, String>(2)?,
                                "enabled": r.get::<_, i64>(3)?,
                            }))
                        },
                    )
                    .unwrap_or(Value::Null);
                let folders: Vec<String> = {
                    let mut stmt = mount
                        .prepare("SELECT path FROM doc_mount_folders WHERE mountPointId = ?1 ORDER BY path")
                        .unwrap();
                    stmt.query_map([&store.mount_point_id], |r| r.get::<_, String>(0))
                        .unwrap()
                        .map(Result::unwrap)
                        .collect()
                };
                let table = match a.scope.as_str() {
                    "project" => Some("projects"),
                    "group" => Some("groups"),
                    _ => None,
                };
                let points = match (table, store.owner.id.as_deref()) {
                    (Some(t), Some(oid)) => {
                        main.query_row(
                            &format!("SELECT officialMountPointId FROM {t} WHERE id = ?1"),
                            [oid],
                            |r| r.get::<_, Option<String>>(0),
                        )
                        .ok()
                        .flatten()
                        .as_deref()
                            == Some(store.mount_point_id.as_str())
                    }
                    _ => false,
                };
                json!({
                    "value": value,
                    "dump": { "store": mp, "folders": folders, "ownerPointsAtIt": points },
                })
            }
        };
        check(&a.id, got, &lines, &[]);
    }

    for a in &spec.read_way_arms {
        let (found, lines) = quilltap_core::test_support::captured_with(|| {
            match (a.collection.as_str(), a.way.as_str()) {
                ("projects", "raw") => Ok(quilltap_core::db::projects::find_by_id_raw_or_none(
                    main,
                    &a.owner_id,
                )
                .is_some()),
                ("groups", "raw") => Ok(quilltap_core::db::groups::find_by_id_raw_or_none(
                    main,
                    &a.owner_id,
                )
                .is_some()),
                ("projects", "hydrated") => {
                    quilltap_core::db::projects::ProjectsRepository::new(main, mount)
                        .find_by_id(&a.owner_id)
                        .map(|v| v.is_some())
                        .map_err(|_| ())
                }
                ("groups", "hydrated") => {
                    quilltap_core::db::groups::GroupsRepository::new(main, mount)
                        .find_by_id(&a.owner_id)
                        .map(|v| v.is_some())
                        .map_err(|_| ())
                }
                other => panic!("read way {other:?}"),
            }
        });
        let got = match found {
            Ok(f) => json!({ "found": f }),
            Err(()) => json!({ "threw": true }),
        };
        check(&a.id, got, &lines, READ_WAY_MESSAGES);
    }

    let resolve_mount = |token: &str| -> String {
        if let Some(cid) = token.strip_prefix("@vault:") {
            return vault_of(cid);
        }
        if let Some(pid) = token.strip_prefix("@projectOfficial:") {
            return main
                .query_row(
                    "SELECT officialMountPointId FROM projects WHERE id = ?1",
                    [pid],
                    |r| r.get::<_, String>(0),
                )
                .unwrap();
        }
        token.to_string()
    };
    for a in &spec.classify_arms {
        let mp = resolve_mount(&a.mount_point_id);
        let (res, lines) = quilltap_core::test_support::captured_with(|| classify_mount(main, &mp));
        let value = match res.expect("classify_mount") {
            None => Value::Null,
            Some(info) => {
                let character = project_character(info.character.as_ref());
                let mut v = serde_json::to_value(&info).unwrap();
                if let Some(c) = character {
                    v["character"] = c;
                }
                v
            }
        };
        check(&a.id, json!({ "value": value }), &lines, &[]);
    }

    for a in &spec.cast_group_arms {
        let (ids, lines) = quilltap_core::test_support::captured_with(|| {
            cast_group_mounts(main, mount, &a.character_ids, &a.group_ids)
        });
        check(&a.id, json!({ "value": ids }), &lines, &[]);
    }

    for a in &spec.cast_pool_arms {
        run_sql(main, mount, &a.before);
        let (pool, lines) = quilltap_core::test_support::captured_with(|| {
            resolve_cast_pool(
                main,
                mount,
                &CastPoolOptions {
                    user_id: &a.user_id,
                    project_id: a.project_id.as_deref(),
                    character_ids: &a.character_ids,
                    group_ids: &a.group_ids,
                },
            )
        });
        run_sql(main, mount, &a.after);
        check(&a.id, json!({ "value": pool }), &lines, &[]);
    }

    let expected: HashSet<String> = oracle.keys().cloned().collect();
    assert_eq!(
        driven, expected,
        "case-set drift between reach.json's v5 driver and the oracle"
    );
    assert!(
        failures.is_empty(),
        "reach differs from v4:\n{}",
        failures.join("\n")
    );
    eprintln!("reach: {} arms matched the oracle.", driven.len());
}
