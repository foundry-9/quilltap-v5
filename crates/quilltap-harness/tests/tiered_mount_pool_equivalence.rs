//! Read-differential (W4.1d batch 3a): the tiered mount pool
//! (`db::tiered_mount_pool::resolve_tiered_mount_pool`) vs v4's REAL
//! `resolveTieredMountPool`. Both sides READ the SAME baked fixtures (two
//! characters with vaults, a group with an official and a linked store plus charA
//! membership, a project with two stores and colliding refs, and the Quilltap
//! General singleton), run the SAME resolution matrix, and compare the resolved
//! pools EXACTLY — no
//! normalization (every id is pinned/shared; the char-vault mounts are minted but
//! read identically from the shared fixture on both sides).
//!
//! Matrix: ownership gate pass/fail, the pre-resolved fast path, the participant
//! tier + self-exclusion + flag-off, the per-RESPONDING-character group tier, the
//! character>group>global dedup dropping colliding project links, and the
//! character-less pool.
//!
//! **P4.D231 (v4 `08c49319d`)** — the NEW `resolveMountPointIdsForGroup` is
//! driven directly by the spec's `helperArms` over `helperPlants` (applied on
//! both work copies before anything reads them). The matrix is the success-
//! path NEUTRALITY leg (`08c49319d` reorders no id). Measured, against the
//! order's prediction: neither read's failure empties the group — both of
//! v4's reads are fallback-mode `safeQuery`s, so the helper's catch never
//! fires; an unreadable group row loses only its official store, and an
//! unreadable LINK row is dropped alone — and, since P4.124, on v5 too (the
//! `LINK_ROW_DIVERGENCE` pin retired by VANISHING; see below).
//! ⚠ PIN REQUIRED at `08c49319d` for the helper rows (a baseline pin records
//! none — the helper does not exist there).
//!
//! **P4.D256 (v4 `cc80dc89d`)** — the NEW grouped resolver
//! `resolveGroupMountsForCharacter` is driven by the spec's `groupedArms`
//! (each row carries `groups` AND the flat resolver's `flat`, so "flattens to
//! exactly the flat resolver, in the same order" is a comparand), over the
//! `p4d256` plants in `helperPlants`: first-group credit for a store linked to
//! two of a character's groups, a group whose every store is already claimed
//! (DROPPED), a membership whose group row is absent / unreadable but whose
//! links survive (`name: ""`). ⚠ PIN REQUIRED at `cc80dc89d` or later.
//!
//! **P4.D376 (v4 `0c47f4539`)** — `roster` / `enabled` are REQUIRED options
//! and the matrix now lives in the spec (`matrix`), each row stating both (the
//! nine pre-existing rows `open` / `ignore`, v4's old `{}`). The project tier is
//! the official store FIRST, then every link (`official_unlinked`,
//! `official_linked_late`, the `project_ids_official_*` helper rows — shift
//! (a)); a failed links read keeps the official store (shift (t)); the roster
//! gate runs INSIDE the pool with v4's ROOT DEBUG (`roster_gated_*`, its silence
//! leg on the on-roster row); `enabled: 'require'` drops a disabled store from
//! every tier and General, and a mount with no row silently, with v4's DEBUG
//! once (`enabled_*`); the grouped resolver's `includeEmpty`. Every matrix row
//! compares the pool's own lines (`POOL_MESSAGES`) too. **Measured dead:**
//! `Project official store lookup failed`, `Project mount lookup failed` and
//! `Mount lookup failed; store left out of the pool` — every read under them
//! is a fallback `safeQuery`; the repository lines are what v4 emits (pinned
//! positive on `project_ids_*`), and the three WARNs are pinned NEGATIVE by
//! name below. ⚠ PIN REQUIRED at `624d0d25c`.
//!
//! Build the fixtures + oracle (Node 24, from the v4 checkout — a pin at
//! `624d0d25c`):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_TMP_MAIN=/tmp/qt-p4d376-tmp-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-p4d376-tmp-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/fixtures/build-tiered-mount-pool-fixture.ts
//!   QT_FIXTURE_TMP_MAIN=/tmp/qt-p4d376-tmp-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-p4d376-tmp-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/cases/tiered-mount-pool.ts > /tmp/oracle-p4d376-tmp.ndjson
//! Run:
//!   QT_ORACLE_TMP=/tmp/oracle-p4d376-tmp.ndjson \
//!   QT_FIXTURE_TMP_MAIN=/tmp/qt-p4d376-tmp-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-p4d376-tmp-mount.db \
//!     cargo test -p quilltap-harness --test tiered_mount_pool_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::tiered_mount_pool::{
    resolve_group_mount_point_ids_for_character, resolve_group_mounts_for_character_with,
    resolve_mount_point_ids_for_group, resolve_tiered_mount_pool, Enabled, GroupMountsOptions,
    Roster, TierContext, TierResolveOptions,
};
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "wrongUserId")]
    wrong_user_id: String,
    #[serde(rename = "charAId")]
    char_a_id: String,
    #[serde(rename = "charBId")]
    char_b_id: String,
    #[serde(rename = "projectId")]
    project_id: String,
    #[serde(rename = "fakeMountPointId")]
    fake_mount_point_id: String,
    #[serde(rename = "charCId")]
    char_c_id: String,
    /// P4.D376: the resolution matrix (mirrored from the oracle — one source).
    matrix: Vec<MatrixRow>,
    #[serde(rename = "helperPlants")]
    helper_plants: Vec<HelperPlant>,
    #[serde(rename = "helperArms")]
    helper_arms: Vec<HelperArm>,
    /// P4.D256 (v4 `cc80dc89d`): the grouped resolver's arms.
    #[serde(rename = "groupedArms")]
    grouped_arms: Vec<GroupedArm>,
    /// P4.149 (items 6a/6b): the two project-tier helpers' arms.
    #[serde(rename = "projectTierArms")]
    project_tier_arms: Vec<ProjectTierArm>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MatrixRow {
    id: String,
    ctx: serde_json::Map<String, Value>,
    roster: String,
    enabled: String,
    #[serde(default)]
    require_ownership: bool,
    #[serde(default)]
    include_participants: bool,
    #[serde(default)]
    before: Vec<String>,
    #[serde(default)]
    after: Vec<String>,
}

/// The pool's own lines (P4.D376) — the oracle's `POOL_MESSAGES`, the three
/// unreachable WARNs included so a row would show them.
const POOL_MESSAGES: &[&str] = &[
    "Character off project roster — project tier withheld",
    "Disabled stores left out of the pool",
    "Mount lookup failed; store left out of the pool",
    "Project official store lookup failed",
    "Project mount lookup failed",
];
/// The three v4 WARNs measured UNREACHABLE (T3.3) — pinned NEGATIVE by name.
const DEAD_WARNS: &[&str] = &[
    "Mount lookup failed; store left out of the pool",
    "Project official store lookup failed",
    "Project mount lookup failed",
];

#[derive(Deserialize)]
struct ProjectTierArm {
    id: String,
    helper: String,
    arg: String,
    #[serde(default)]
    plant: Option<RenamePlant>,
}

#[derive(Deserialize)]
struct RenamePlant {
    db: String,
    table: String,
    from: String,
    to: String,
}

/// One v4 line on a project-tier arm (`error` omitted on the oracle side).
#[derive(Deserialize)]
struct ArmLog {
    level: String,
    message: String,
    fields: Vec<(String, String)>,
}

/// v4's backend-only lines (`backends/sqlite/backend.ts`), unported by
/// standing convention (the search families' `UNPORTED_BACKEND_LINES`).
const UNPORTED_BACKEND_LINES: &[&str] = &["SQLite find error", "SQLite findOne error"];

#[derive(Deserialize)]
struct HelperPlant {
    db: String,
    sql: String,
    params: Vec<Option<String>>,
}

#[derive(Deserialize)]
struct HelperArm {
    id: String,
    #[serde(rename = "groupId")]
    group_id: String,
}

#[derive(Deserialize)]
struct GroupedArm {
    id: String,
    #[serde(rename = "characterId")]
    character_id: String,
    /// P4.D376: v4's `{ includeEmpty: true }`.
    #[serde(default, rename = "includeEmpty")]
    include_empty: bool,
}

/// A matrix row carries `pool`; a P4.D231 helper row carries `ids`; a P4.149
/// project-tier row carries `ids` + `logs` under `projectTier: true`.
#[derive(Deserialize)]
struct Row {
    id: String,
    #[serde(default)]
    pool: Option<Value>,
    #[serde(default)]
    ids: Option<Value>,
    #[serde(default, rename = "projectTier")]
    project_tier: bool,
    #[serde(default)]
    logs: Option<Vec<ArmLog>>,
    /// P4.D256: a grouped row carries `groups` + `flat` under `grouped: true`.
    #[serde(default)]
    grouped: bool,
    #[serde(default)]
    groups: Option<Value>,
    #[serde(default)]
    flat: Option<Value>,
}

// P4.D231's `LINK_ROW_DIVERGENCE` pin (`helper_unreadable_link_row`: v4
// dropped the one undecodable link row, v5 failed the whole read) was RETIRED
// by P4.124 after the family measured it VANISHED — `find_by_group_id` now
// validates row by row as v4's `findByFilter` does. The arm is an ordinary
// comparand row again; `helper_zod_invalid_link_row` adds the decodes-but-
// fails-Zod shape.

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/tiered-mount-pool.json")
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

#[test]
fn tiered_mount_pool_matches_oracle() {
    let (Some(oracle_path), Some(main_fixture), Some(mount_fixture)) = (
        env_or_skip("QT_ORACLE_TMP"),
        env_or_skip("QT_FIXTURE_TMP_MAIN"),
        env_or_skip("QT_FIXTURE_TMP_MOUNT"),
    ) else {
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let mut oracle: HashMap<String, (Value, Vec<ArmLog>)> = HashMap::new();
    let mut helper_oracle: HashMap<String, Value> = HashMap::new();
    let mut project_tier_oracle: HashMap<String, (Value, Vec<ArmLog>)> = HashMap::new();
    let mut grouped_oracle: HashMap<String, (Value, Value)> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let row: Row = serde_json::from_str(line).expect("oracle line parses");
        if row.grouped {
            grouped_oracle.insert(
                row.id,
                (
                    row.groups.expect("a grouped row carries groups"),
                    row.flat.expect("a grouped row carries flat"),
                ),
            );
            continue;
        }
        if row.project_tier {
            project_tier_oracle.insert(
                row.id,
                (
                    row.ids.expect("a project-tier row carries ids"),
                    row.logs.expect("a project-tier row carries logs"),
                ),
            );
            continue;
        }
        match (row.pool, row.ids) {
            (Some(pool), None) => {
                oracle.insert(row.id, (pool, row.logs.unwrap_or_default()));
            }
            (None, Some(ids)) => {
                helper_oracle.insert(row.id, ids);
            }
            _ => panic!("oracle row '{}' carries exactly one of pool / ids", row.id),
        }
    }

    // Fresh copies so the shared seed fixtures stay pristine.
    // A scratch dir removed on drop — with the TRUNCATE-mode `-journal`
    // files the writable opens leave beside each DB.
    let scratch = tempfile::Builder::new()
        .prefix("qt-tmp-rust-")
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
    // P4.D231: the helper arms' plants, on the work copies before any read (the
    // oracle applies them pre-init) — new groups with no members, so the matrix
    // below is untouched.
    for p in &spec.helper_plants {
        let conn = if p.db == "main" { main } else { mount };
        conn.execute(&p.sql, rusqlite::params_from_iter(p.params.iter()))
            .unwrap_or_else(|e| panic!("plant `{}`: {e}", p.sql));
    }

    // The matrix — read from the spec, the oracle's own source (P4.D376).
    let subst = |v: &str| -> String {
        match v {
            "$A" => spec.char_a_id.clone(),
            "$B" => spec.char_b_id.clone(),
            "$C" => spec.char_c_id.clone(),
            "$P" => spec.project_id.clone(),
            "$U" => spec.user_id.clone(),
            "$WU" => spec.wrong_user_id.clone(),
            "$FAKE" => spec.fake_mount_point_id.clone(),
            other => other.to_string(),
        }
    };
    let text = |row: &MatrixRow, k: &str| row.ctx.get(k).and_then(Value::as_str).map(&subst);
    let mut matrix_failures: Vec<String> = Vec::new();
    for row in &spec.matrix {
        let ctx = TierContext {
            user_id: text(row, "userId"),
            character_id: text(row, "characterId"),
            character_mount_point_id: text(row, "characterMountPointId"),
            character_ids: row
                .ctx
                .get("characterIds")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).map(&subst).collect()),
            project_id: text(row, "projectId"),
        };
        let roster = match row.roster.as_str() {
            "gated" => Roster::Gated,
            "open" => Roster::Open,
            other => panic!("{}: roster {other}", row.id),
        };
        let enabled = match row.enabled.as_str() {
            "require" => Enabled::Require,
            "ignore" => Enabled::Ignore,
            other => panic!("{}: enabled {other}", row.id),
        };
        let opts = TierResolveOptions {
            require_ownership: row.require_ownership,
            include_participants: row.include_participants,
            ..TierResolveOptions::new(roster, enabled)
        };
        for sql in &row.before {
            mount
                .execute_batch(sql)
                .unwrap_or_else(|e| panic!("{}: before: {e}", row.id));
        }
        let (pool, lines) = quilltap_core::test_support::captured_with(|| {
            resolve_tiered_mount_pool(main, mount, &ctx, &opts)
        });
        for sql in &row.after {
            mount
                .execute_batch(sql)
                .unwrap_or_else(|e| panic!("{}: after: {e}", row.id));
        }
        let got = serde_json::to_value(&pool).unwrap();
        let (want, want_logs) = oracle
            .get(&row.id)
            .unwrap_or_else(|| panic!("oracle missing case '{}'", row.id));
        if &got != want {
            matrix_failures.push(format!("{}: pool\n  v4: {want}\n  v5: {got}", row.id));
        }
        let want_lines: Vec<String> = want_logs
            .iter()
            .map(|l| {
                let mut line = format!("{} {}", l.level.to_uppercase(), l.message);
                for (k, v) in &l.fields {
                    line.push_str(&format!(" {k}={v}"));
                }
                line
            })
            .collect();
        // v5's rendered line is `LEVEL target message k=v…`; the target is
        // dropped (v4's ROOT logger has none) and the `error` field cut.
        let got_lines: Vec<String> = lines
            .iter()
            .filter_map(|l| {
                let msg = POOL_MESSAGES.iter().find(|m| l.contains(*m))?;
                let level = l.split(' ').next().unwrap_or_default();
                let at = l.find(msg).unwrap();
                let tail = &l[at..];
                let tail = match tail.find(" error=") {
                    Some(i) => &tail[..i],
                    None => tail,
                };
                Some(format!("{level} {tail}"))
            })
            .collect();
        if got_lines != want_lines {
            matrix_failures.push(format!(
                "{}: lines\n  v4: {want_lines:?}\n  v5: {got_lines:?}",
                row.id
            ));
        }
        for dead in DEAD_WARNS {
            assert!(
                !lines.iter().any(|l| l.contains(dead)),
                "{}: the unreachable WARN `{dead}` fired on v5",
                row.id
            );
        }
    }
    assert!(
        matrix_failures.is_empty(),
        "tiered pool mismatches:\n{}",
        matrix_failures.join("\n")
    );
    assert_eq!(spec.matrix.len(), oracle.len(), "case count mismatch");

    // P4.D231 — `resolve_mount_point_ids_for_group` (v4 `08c49319d`
    // `resolveMountPointIdsForGroup`), driven directly.
    assert_eq!(
        helper_oracle.len(),
        spec.helper_arms.len(),
        "helper arm count — a pre-`08c49319d` pin records none (regenerate at the pin)"
    );
    let mut helper_failures: Vec<String> = Vec::new();
    for arm in &spec.helper_arms {
        let got = serde_json::to_value(resolve_mount_point_ids_for_group(
            main,
            mount,
            &arm.group_id,
        ))
        .unwrap();
        let want = &helper_oracle[&arm.id];
        if &got != want {
            helper_failures.push(format!("{}\n  v4: {want}\n  v5: {got}", arm.id));
        }
    }
    assert!(
        helper_failures.is_empty(),
        "resolve_mount_point_ids_for_group differs:\n{}",
        helper_failures.join("\n")
    );

    // P4.D256 — `resolve_group_mounts_for_character` (v4 `cc80dc89d`
    // `resolveGroupMountsForCharacter`) beside the flat resolver, which is now
    // DEFINED over it (its bytes unchanged — the matrix above stays green).
    assert_eq!(
        grouped_oracle.len(),
        spec.grouped_arms.len(),
        "grouped arm count — a pre-`cc80dc89d` pin records none (regenerate at the pin)"
    );
    let mut grouped_failures: Vec<String> = Vec::new();
    for arm in &spec.grouped_arms {
        let got_groups = serde_json::to_value(resolve_group_mounts_for_character_with(
            main,
            mount,
            &arm.character_id,
            GroupMountsOptions {
                include_empty: arm.include_empty,
            },
        ))
        .unwrap();
        let got_flat = serde_json::to_value(resolve_group_mount_point_ids_for_character(
            main,
            mount,
            &arm.character_id,
        ))
        .unwrap();
        let (want_groups, want_flat) = &grouped_oracle[&arm.id];
        if &got_groups != want_groups {
            grouped_failures.push(format!(
                "{}: groups\n  v4: {want_groups}\n  v5: {got_groups}",
                arm.id
            ));
        }
        if &got_flat != want_flat {
            grouped_failures.push(format!(
                "{}: flat\n  v4: {want_flat}\n  v5: {got_flat}",
                arm.id
            ));
        }
    }
    assert!(
        grouped_failures.is_empty(),
        "resolve_group_mounts_for_character differs:\n{}",
        grouped_failures.join("\n")
    );

    // P4.149 (items 6a/6b) — the project-tier helpers, AFTER everything above
    // (the oracle's order). v4 creates `chats` lazily on its first access; the
    // fixture has none, so the same table's statements from the D23 dump run
    // here. Each failure arm's RENAME is restored before the next arm.
    let fresh: Value = serde_json::from_str(include_str!(
        "../../quilltap-core/src/services/provisioning/fresh_schema.json"
    ))
    .expect("fresh_schema.json");
    let chats_ddl: Vec<&str> = fresh["main"]
        .as_array()
        .expect("main DDL list")
        .iter()
        .filter_map(Value::as_str)
        .filter(|d| d.starts_with("CREATE TABLE \"chats\" (") || d.contains(" ON \"chats\" "))
        .collect();
    assert!(
        !chats_ddl.is_empty(),
        "the D23 dump carries the chats table"
    );
    for ddl in chats_ddl {
        main.execute_batch(ddl)
            .unwrap_or_else(|e| panic!("chats DDL `{ddl}`: {e}"));
    }
    assert_eq!(
        project_tier_oracle.len(),
        spec.project_tier_arms.len(),
        "project-tier arm count — regenerate from THIS tree's case (P4.149)"
    );
    let mut tier_failures: Vec<String> = Vec::new();
    for arm in &spec.project_tier_arms {
        let rename = |from: &str, to: &str| {
            if let Some(p) = &arm.plant {
                let conn = if p.db == "main" { main } else { mount };
                conn.execute_batch(&format!(
                    "ALTER TABLE \"{}\" RENAME COLUMN \"{from}\" TO \"{to}\"",
                    p.table
                ))
                .unwrap_or_else(|e| panic!("{}: rename: {e}", arm.id));
            }
        };
        if let Some(p) = &arm.plant {
            rename(&p.from, &p.to);
        }
        let (ids, lines) =
            quilltap_core::test_support::captured_with(|| match arm.helper.as_str() {
                "project" => {
                    quilltap_core::tools::wardrobe_shared::resolve_project_mount_point_ids(
                        main,
                        mount,
                        Some(&arm.arg),
                    )
                }
                "chat" => {
                    quilltap_core::tools::wardrobe_shared::resolve_project_mount_point_ids_for_chat(
                        main, mount, &arm.arg,
                    )
                }
                other => panic!("unknown project-tier helper {other}"),
            });
        if let Some(p) = &arm.plant {
            rename(&p.to, &p.from);
        }
        let (want_ids, want_logs) = &project_tier_oracle[&arm.id];
        let got_ids = serde_json::to_value(&ids).unwrap();
        if &got_ids != want_ids {
            tier_failures.push(format!(
                "{}: ids\n  v4: {want_ids}\n  v5: {got_ids}",
                arm.id
            ));
        }
        let want_lines: Vec<String> = want_logs
            .iter()
            .filter(|l| !UNPORTED_BACKEND_LINES.contains(&l.message.as_str()))
            .map(|l| {
                let mut line = format!("{} quilltap::db {}", l.level.to_uppercase(), l.message);
                for (k, v) in &l.fields {
                    line.push_str(&format!(" {k}={v}"));
                }
                line
            })
            .collect();
        let got_lines: Vec<String> = lines
            .iter()
            .filter(|l| l.starts_with("ERROR ") || l.starts_with("WARN "))
            .map(|l| match l.find(" error=") {
                Some(i) => l[..i].to_string(),
                None => l.clone(),
            })
            .collect();
        for dead in DEAD_WARNS {
            assert!(
                !lines.iter().any(|l| l.contains(dead)),
                "{}: the unreachable WARN `{dead}` fired on v5",
                arm.id
            );
        }
        if got_lines != want_lines {
            tier_failures.push(format!(
                "{}: lines\n  v4: {want_lines:?}\n  v5: {got_lines:?}",
                arm.id
            ));
        }
    }
    assert!(
        tier_failures.is_empty(),
        "the project-tier helpers differ:\n{}",
        tier_failures.join("\n")
    );
    eprintln!(
        "tiered_mount_pool: {} matrix rows, {} helper, {} grouped, {} project-tier arms matched the oracle.",
        spec.matrix.len(),
        spec.helper_arms.len(),
        spec.grouped_arms.len(),
        spec.project_tier_arms.len()
    );
}
