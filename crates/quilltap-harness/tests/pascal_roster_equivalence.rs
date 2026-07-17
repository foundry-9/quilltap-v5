//! Tier-2 structural differential (P4.6ay unit 2): Pascal custom-tool DISCOVERY,
//! tier shadowing, and roster resolution against v4's real `resolveCustomToolRoster`.
//!
//! Each oracle row is a scenario `{ pool, mounts }`; v4 ran it through its real
//! function with the pool + store mocked (exactly as v4's own discovery test
//! does), and this side replays the same scenario through
//! `resolve_roster_from_pool` + `load_definitions` — the same real parse path via
//! `safe_parse`. The DB/disk IO wrappers are thin and are what v4 mocks here too.
//!
//! The malformed-JSON `reason` differs one layer below this format (serde_json's
//! message vs V8's), a documented cross-language seam, so that one reason is
//! compared by its `is not valid JSON:` prefix; every other reason (the schema
//! sentence, the duplicate-name sentence) is byte-exact.
//!
//! Generate the oracle output (v4 @ d68638b4, Node 24; /tmp mirror, jest ignores
//! `.claude/`):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
//!   TMPO=/tmp/qt-pascal-discovery-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
//!   cp "$V5W/harness/oracle/cases/pascal-custom-tools-discovery.test.ts" "$TMPO/cases/"
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-pascal-discovery.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- pascal-custom-tools-discovery
//! Run:
//!   QT_ORACLE_PASCAL_DISCOVERY=/tmp/oracle-pascal-discovery.ndjson \
//!     cargo test -p quilltap-harness --test pascal_roster_equivalence

use quilltap_core::db::tiered_mount_pool::{MountTier, TieredMountPool};
use quilltap_core::pascal::roster::{
    is_root_tool_file, load_definitions, resolve_roster_from_pool, CustomToolRoster,
    DiscoveredCustomTool, ToolFileContent,
};
use serde_json::{json, Map, Value};

fn str_list(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn opt_str(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(String::from)
}

fn pool_of(input: &Value) -> TieredMountPool {
    let p = &input["pool"];
    TieredMountPool {
        character_mount_point_id: opt_str(p, "characterMountPointId"),
        participant_mount_point_ids: str_list(p, "participantMountPointIds"),
        group_mount_point_ids: str_list(p, "groupMountPointIds"),
        project_mount_point_ids: str_list(p, "projectMountPointIds"),
        global_mount_point_id: opt_str(p, "globalMountPointId"),
    }
}

fn tier_str(tier: MountTier) -> &'static str {
    match tier {
        MountTier::Character => "character",
        MountTier::Participant => "participant",
        MountTier::Group => "group",
        MountTier::Project => "project",
        MountTier::Global => "global",
    }
}

/// v5 roster → the oracle's serialized shape (for structural comparison).
fn serialize(roster: &CustomToolRoster) -> Value {
    let mut tools = Map::new();
    for (name, t) in &roster.tools {
        tools.insert(
            name.clone(),
            json!({
                "tier": tier_str(t.tier),
                "definitionPath": t.definition_path,
                "mountPointId": t.mount_point_id,
                "mountName": t.mount_name,
                "description": t.definition.description,
            }),
        );
    }
    json!({
        "toolKeys": roster.tools.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>(),
        "tools": Value::Object(tools),
        "errors": roster.errors.iter().map(|e| json!({
            "definitionPath": e.definition_path,
            "mountPointId": e.mount_point_id,
            "mountName": e.mount_name,
            "tier": tier_str(e.tier),
            "reason": e.reason,
        })).collect::<Vec<_>>(),
        "droppedForCap": roster.dropped_for_cap.clone(),
    })
}

/// Compare two serialized rosters, normalizing only the malformed-JSON reason.
fn assert_roster_eq(got: &Value, want: &Value, id: &str) {
    assert_eq!(got["toolKeys"], want["toolKeys"], "case '{id}': toolKeys");
    assert_eq!(got["tools"], want["tools"], "case '{id}': tools");
    assert_eq!(
        got["droppedForCap"], want["droppedForCap"],
        "case '{id}': droppedForCap"
    );

    let ge = got["errors"].as_array().unwrap();
    let we = want["errors"].as_array().unwrap();
    assert_eq!(ge.len(), we.len(), "case '{id}': error count");
    for (g, w) in ge.iter().zip(we.iter()) {
        for field in ["definitionPath", "mountPointId", "mountName", "tier"] {
            assert_eq!(g[field], w[field], "case '{id}': error {field}");
        }
        let gr = g["reason"].as_str().unwrap();
        let wr = w["reason"].as_str().unwrap();
        if wr.starts_with("is not valid JSON:") {
            // serde_json's parser message differs from V8's — a documented seam.
            assert!(
                gr.starts_with("is not valid JSON:"),
                "case '{id}': v5 reason {gr:?} is not a JSON-parse error"
            );
        } else {
            assert_eq!(gr, wr, "case '{id}': error reason");
        }
    }
}

#[test]
fn pascal_roster_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_PASCAL_DISCOVERY") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_PASCAL_DISCOVERY to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    let mut saw_is_root_reject = false;

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        assert_eq!(row["kind"], "roster");
        let id = row["id"].as_str().unwrap().to_string();
        let input = &row["input"];
        let pool = pool_of(input);
        let mounts = input["mounts"].as_object().cloned().unwrap_or_default();

        let roster = resolve_roster_from_pool(&pool, |mount_point_id, tier| {
            let Some(spec) = mounts.get(mount_point_id) else {
                return (Vec::new(), Vec::new());
            };
            if !spec["enabled"].as_bool().unwrap_or(false) {
                return (Vec::new(), Vec::new());
            }
            let mount_name = format!("store-{mount_point_id}");
            // Mirror `list_tool_files_from_database`: drop folders + non-root
            // files, so `is_root_tool_file` is exercised over the raw listing.
            let files: Vec<(String, ToolFileContent)> = spec["files"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|f| {
                    let rel = f["relativePath"].as_str().unwrap();
                    let is_folder = f["kind"].as_str() == Some("folder");
                    let ok = !is_folder && is_root_tool_file(rel);
                    if !ok {
                        saw_is_root_reject = true;
                    }
                    ok
                })
                .map(|f| {
                    (
                        f["relativePath"].as_str().unwrap().to_string(),
                        ToolFileContent::Content(f["content"].as_str().unwrap().to_string()),
                    )
                })
                .collect();
            load_definitions(&files, mount_point_id, &mount_name, tier)
        });

        let got = serialize(&roster);
        assert_roster_eq(&got, &row["output"], &id);
        count += 1;
    }

    assert!(count >= 15, "oracle looked short: only {count} rows");
    assert!(
        saw_is_root_reject,
        "no case exercised is_root_tool_file rejection"
    );
    // Spot-check the predicate directly.
    assert!(is_root_tool_file("Tools/x.tool.json"));
    assert!(!is_root_tool_file("Tools/sub/x.tool.json"));
    assert!(!is_root_tool_file("Tools/notes.md"));
    let _ = DiscoveredCustomTool {
        // touch the type so a field rename can't silently drift the test
        definition: quilltap_core::pascal::custom_tool_types::safe_parse(&json!({
            "name": "x", "description": "d", "outcomes": [{"when": true, "message": "-", "state": "info"}]
        }))
        .unwrap(),
        tier: MountTier::Global,
        mount_point_id: String::new(),
        mount_name: String::new(),
        definition_path: String::new(),
    };
    eprintln!("OK: pascal roster matched oracle ({count} scenarios).");
}
