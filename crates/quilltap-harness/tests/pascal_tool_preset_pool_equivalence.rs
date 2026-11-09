//! Tier-1½ differential (P4.D379, v4 `fd165fce0` + `0c47f4539` at the
//! `624d0d25c` pin): Pascal custom-tool RUN PRESETS — the tier walk, against v4's
//! real `resolveToolPresetPool`.
//!
//! Each oracle row is a scenario `{ pool, mounts, broken }` that v4 ran through
//! its real function with the pool, the mount row and the database listing
//! mocked (v4's own `tool-preset-pool.test.ts` mocks, which are the spine of the
//! corpus). This side replays the scenario through `resolve_tool_preset_pool_from`
//! — the same walk the production wrapper runs — under the thread-scoped capture
//! rig, and byte-compares:
//!
//! - the result's `serde_json::to_string` (key ORDER included: `tier,
//!   mountPointId, mountName[, name, path, shadows | , reason]`);
//! - the WARN `Preset listing failed for store` and the DEBUG `Tool presets
//!   resolved`, line for line (a `null` field renders absent on the rig);
//! - v4's pool-call options against the wrapper's [`PRESET_POOL_OPTIONS`].
//!
//! The listing mock answers the database shape, so the replay's `list` drops
//! nothing the real `list_tools_folder_files` would keep for these root-level
//! paths (the root test is the roster family's, the disk branch the route
//! family's).
//!
//! Generate the oracle output (v4 @ the `624d0d25c` pin, Node 24; /tmp mirror,
//! jest ignores `.claude/`):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
//!   TMPO=/tmp/qt-pascal-preset-pool-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
//!   cp "$V5W/harness/oracle/cases/pascal-tool-preset-pool.test.ts" "$TMPO/cases/"
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-pascal-tool-preset-pool.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- pascal-tool-preset-pool
//! Run:
//!   QT_ORACLE_PASCAL_TOOL_PRESET_POOL=/tmp/oracle-pascal-tool-preset-pool.ndjson \
//!     cargo test -p quilltap-harness --test pascal_tool_preset_pool_equivalence

use quilltap_core::db::tiered_mount_pool::{Enabled, Roster, TieredMountPool};
use quilltap_core::pascal::tool_preset_pool::{resolve_tool_preset_pool_from, PRESET_POOL_OPTIONS};
use serde_json::Value;

fn str_list(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn opt_str(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(String::from)
}

fn pool_of(v: &Value) -> TieredMountPool {
    TieredMountPool {
        character_mount_point_id: opt_str(v, "characterMountPointId"),
        participant_mount_point_ids: Vec::new(),
        group_mount_point_ids: str_list(v, "groupMountPointIds"),
        project_mount_point_ids: str_list(v, "projectMountPointIds"),
        global_mount_point_id: opt_str(v, "globalMountPointId"),
    }
}

/// v4's logger line rendered as the capture rig renders a tracing line: the
/// level, the target, the message, then each NON-null field as `k=v`.
fn v4_line(line: &Value) -> String {
    let level = line["level"].as_str().unwrap().to_uppercase();
    let mut out = format!(
        "{level} quilltap::pascal {}",
        line["message"].as_str().unwrap()
    );
    for (k, v) in line["bag"].as_object().into_iter().flatten() {
        match v {
            Value::Null => {}
            Value::String(s) => out.push_str(&format!(" {k}={s}")),
            other => out.push_str(&format!(" {k}={other}")),
        }
    }
    out
}

#[test]
fn pascal_tool_preset_pool_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_PASCAL_TOOL_PRESET_POOL") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_PASCAL_TOOL_PRESET_POOL to the oracle NDJSON (see header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    // The wrapper's options, as v4's strings.
    let (roster, enabled) = PRESET_POOL_OPTIONS;
    let wrapper_options = serde_json::json!({
        "roster": match roster { Roster::Open => "open", Roster::Gated => "gated" },
        "enabled": match enabled { Enabled::Require => "require", Enabled::Ignore => "ignore" },
    });

    let (mut count, mut warns, mut shadows, mut unread_rows) = (0usize, 0usize, 0usize, 0usize);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let id = row["id"].as_str().unwrap().to_string();
        let scenario = &row["scenario"];
        let pool = pool_of(&scenario["pool"]);
        let mounts = scenario["mounts"].as_object().cloned().unwrap_or_default();
        let broken = scenario["broken"].as_object().cloned().unwrap_or_default();
        let ctx = &row["ctx"];
        let tool_name = ctx["toolName"].as_str().unwrap();
        let character_id = ctx["characterId"].as_str();

        let (result, lines) = quilltap_core::test_support::captured_with(|| {
            resolve_tool_preset_pool_from(
                &pool,
                tool_name,
                character_id,
                |mount_point_id| {
                    Ok::<_, ()>(mounts.contains_key(mount_point_id).then(|| {
                        (
                            format!("store-{mount_point_id}"),
                            mount_point_id.to_string(),
                        )
                    }))
                },
                |mount_point_id: &String| {
                    if let Some(message) = broken.get(mount_point_id) {
                        return Err(message.as_str().unwrap().to_string());
                    }
                    Ok(str_list(&Value::Object(mounts.clone()), mount_point_id))
                },
            )
        });
        let result = result.expect("the replay's reads never fail");

        assert_eq!(
            serde_json::to_string(&result.to_value()).unwrap(),
            serde_json::to_string(&row["result"]).unwrap(),
            "case '{id}': the pool (bytes, key order included)"
        );

        let got: Vec<&str> = lines
            .iter()
            .filter(|l| {
                l.contains("Preset listing failed for store") || l.contains("Tool presets resolved")
            })
            .map(String::as_str)
            .collect();
        let want: Vec<String> = row["logs"]
            .as_array()
            .unwrap()
            .iter()
            .map(v4_line)
            .collect();
        assert_eq!(
            got,
            want.iter().map(String::as_str).collect::<Vec<_>>(),
            "case '{id}': the pool's lines"
        );
        // The DEBUG fires exactly once per walk (its silence leg is the WARN's:
        // a clean listing logs no WARN — `v4-every-tier-nearest-wins`).
        assert_eq!(
            got.iter()
                .filter(|l| l.contains("Tool presets resolved"))
                .count(),
            1,
            "case '{id}': one DEBUG"
        );

        assert_eq!(
            row["poolCallOptions"], wrapper_options,
            "case '{id}': v4 resolves the pool with {{ roster: 'open', enabled: 'require' }}"
        );

        warns += want.iter().filter(|l| l.starts_with("WARN ")).count();
        shadows += result
            .presets
            .iter()
            .map(|p| p.shadows.len())
            .sum::<usize>();
        unread_rows += usize::from(!result.unread.is_empty());
        count += 1;
    }

    assert!(count >= 12, "oracle looked short: only {count} rows");
    assert!(warns >= 3, "only {warns} listing WARNs compared");
    assert!(shadows >= 4, "only {shadows} shadows compared");
    assert!(
        unread_rows >= 3,
        "only {unread_rows} rows reported unread stores"
    );
    eprintln!("OK: pascal tool preset pool matched oracle ({count} scenarios).");
}
