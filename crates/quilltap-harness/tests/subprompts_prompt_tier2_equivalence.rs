//! Tier-2 differential: the subprompts PROMPT ASSEMBLY end to end (v4
//! `2f4254b42`) — P4.D164 unit 6, the family that proves the whole chain
//! where `subprompts_storage_tier2_equivalence` (P4.D163) recorded the
//! compiler as a seam.
//!
//! Both sides run each case on a FRESH copy of the committed
//! `crates/quilltap-web/tests/fixtures/subprompts-{main,mount}.db` pair and
//! drive: `compile_all_identity_stacks` / `compile_identity_stack_for_
//! participant` (the compiler BAKE — the block present for the selecting
//! seat, absent for the user seat, the removed seat, the pre-feature seat,
//! and a seat whose character has no `Subprompts/`), `build_chat_context`
//! with the opener's resolved subprompts (the GREETING head), and the
//! P4.D163 `update`/`delete` + `fan_out_subprompt_change` with the REAL
//! compiler behind the seams (`ProductionFanoutSeams` — the recompile is
//! observable as two changed `compiledIdentityStacks` cells across the two
//! chats sharing character A; the delete also strips the id). Comparands: the
//! op's reduced result, each named chat's whole `compiledIdentityStacks`
//! envelope (`version` included; no ids remapped, no timestamps inside), and
//! its `[[seatId, selectedSubpromptIds]]`.
//!
//! Mutations (unit 6): dropping the resolver call in `build_stack_for`
//! reddens every selecting-seat cell; dropping the `remove_selection` strip
//! reddens the post-delete participants.
//!
//! Generate the oracle (Node 24, from the v4 checkout — a pinned worktree
//! while v4 HEAD is past the baseline; cp to a /tmp mirror, jest ignores
//! .claude/):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-subprompts-prompt-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/subprompts-prompt.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/subprompts.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_SP_MAIN=$V5W/crates/quilltap-web/tests/fixtures/subprompts-main.db \
//!   QT_FIXTURE_SP_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/subprompts-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-subprompts-prompt.ndjson TZ=UTC \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- subprompts-prompt
//! Run:
//!   QT_ORACLE_SUBPROMPTS_PROMPT=/tmp/oracle-subprompts-prompt.ndjson \
//!     cargo test -p quilltap-harness --test subprompts_prompt_tier2_equivalence -- --nocapture

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use quilltap_core::db::{chats_read, Writer};
use quilltap_core::services::chat_initialize::build_chat_context;
use quilltap_core::services::system_prompt_compiler::{
    compile_all_identity_stacks, compile_identity_stack_for_participant,
};
use quilltap_core::subprompts::{
    delete_character_subprompt, fan_out_subprompt_change, resolve_selected_subprompts,
    update_character_subprompt, FanoutOptions, ProductionFanoutSeams, SubpromptFanoutResult,
    SubpromptPatch,
};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    ids: BTreeMap<String, String>,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/subprompts.json")
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

fn chat_of(main: &Connection, id: &str) -> Value {
    chats_read::find_by_id(main, id)
        .expect("read chat")
        .unwrap_or_else(|| panic!("chat {id} missing from the fixture"))
}

fn compile_all(main: &Connection, mount: &Connection, id: &str) {
    let chat = chat_of(main, id);
    compile_all_identity_stacks(main, mount, &chat).expect("compileAll");
}

fn fan_json(r: SubpromptFanoutResult) -> Value {
    json!({ "chatsTouched": r.chats_touched, "seatsRecompiled": r.seats_recompiled })
}

/// The greeting head over the opener's resolved subprompts — exactly the
/// chat-create handler's two steps.
fn greeting(
    main: &Connection,
    mount: &Connection,
    a: &str,
    ids: &[&str],
    scenario: Option<&str>,
) -> Value {
    let ids: Vec<String> = ids.iter().map(|s| s.to_string()).collect();
    let subprompts = resolve_selected_subprompts(main, mount, a, &ids);
    let ctx = build_chat_context(main, mount, a, None, scenario, None, Some(&subprompts))
        .expect("buildChatContext");
    json!({ "systemPrompt": ctx.system_prompt, "firstMessage": ctx.first_message })
}

struct Case {
    name: &'static str,
    /// The fixture KEYS of the chats whose cells + seats are dumped after the op.
    chats: &'static [&'static str],
}
const CASES: &[Case] = &[
    Case {
        name: "compile_all_chatLlm_block_present",
        chats: &["chatLlm"],
    },
    Case {
        name: "compile_all_chatUser_no_cell",
        chats: &["chatUser"],
    },
    Case {
        name: "compile_all_chatRemoved_no_cell",
        chats: &["chatRemoved"],
    },
    Case {
        name: "compile_all_chatNoKey_cell_without_block",
        chats: &["chatNoKey"],
    },
    Case {
        name: "compile_all_chatTwoSeats_A_block_B_none",
        chats: &["chatTwoSeats"],
    },
    Case {
        name: "compile_participant_pLlm",
        chats: &["chatLlm"],
    },
    Case {
        name: "compile_participant_pUser_writes_nothing",
        chats: &["chatUser"],
    },
    Case {
        name: "greeting_A_terse_VERSE",
        chats: &[],
    },
    Case {
        name: "greeting_A_none",
        chats: &[],
    },
    Case {
        name: "greeting_A_dangling_and_terse_with_scenario",
        chats: &[],
    },
    Case {
        name: "fanout_update_terse_recompiles_both_chats",
        chats: &["chatLlm", "chatTwoSeats"],
    },
    Case {
        name: "fanout_delete_terse_strips_and_recompiles",
        chats: &["chatLlm", "chatTwoSeats"],
    },
    Case {
        name: "fanout_delete_verse_strips_VERSE_recompiles_chatLlm",
        chats: &["chatLlm", "chatTwoSeats"],
    },
];

fn run_op(
    name: &str,
    main: &Connection,
    mount: &Connection,
    i: &BTreeMap<String, String>,
) -> Value {
    let id = |k: &str| i[k].as_str();
    let a = id("charA");
    let seams = ProductionFanoutSeams;
    match name {
        "compile_all_chatLlm_block_present" => {
            compile_all(main, mount, id("chatLlm"));
            Value::Null
        }
        "compile_all_chatUser_no_cell" => {
            compile_all(main, mount, id("chatUser"));
            Value::Null
        }
        "compile_all_chatRemoved_no_cell" => {
            compile_all(main, mount, id("chatRemoved"));
            Value::Null
        }
        "compile_all_chatNoKey_cell_without_block" => {
            compile_all(main, mount, id("chatNoKey"));
            Value::Null
        }
        "compile_all_chatTwoSeats_A_block_B_none" => {
            compile_all(main, mount, id("chatTwoSeats"));
            Value::Null
        }
        "compile_participant_pLlm" => {
            let chat = chat_of(main, id("chatLlm"));
            compile_identity_stack_for_participant(main, mount, &chat, id("pLlm"))
                .expect("compile");
            Value::Null
        }
        "compile_participant_pUser_writes_nothing" => {
            let chat = chat_of(main, id("chatUser"));
            compile_identity_stack_for_participant(main, mount, &chat, id("pUser"))
                .expect("compile");
            Value::Null
        }
        "greeting_A_terse_VERSE" => greeting(main, mount, a, &["terse", "VERSE"], None),
        "greeting_A_none" => greeting(main, mount, a, &[], None),
        "greeting_A_dangling_and_terse_with_scenario" => {
            greeting(main, mount, a, &["gone", "terse"], Some("A rainy quay."))
        }
        "fanout_update_terse_recompiles_both_chats" => {
            compile_all(main, mount, id("chatLlm"));
            compile_all(main, mount, id("chatTwoSeats"));
            let updated = update_character_subprompt(
                main,
                mount,
                a,
                "terse",
                &SubpromptPatch {
                    title: None,
                    content: Some("Two lines at most.".to_string()),
                },
            )
            .expect("update");
            let fan =
                fan_out_subprompt_change(main, mount, a, "terse", FanoutOptions::default(), &seams);
            json!({ "updated": { "id": updated.id, "title": updated.title }, "fan": fan_json(fan) })
        }
        "fanout_delete_terse_strips_and_recompiles" => {
            compile_all(main, mount, id("chatLlm"));
            compile_all(main, mount, id("chatTwoSeats"));
            let deleted = delete_character_subprompt(main, mount, a, "terse").expect("delete");
            let fan = fan_out_subprompt_change(
                main,
                mount,
                a,
                "terse",
                FanoutOptions {
                    remove_selection: true,
                },
                &seams,
            );
            json!({ "deleted": deleted, "fan": fan_json(fan) })
        }
        "fanout_delete_verse_strips_VERSE_recompiles_chatLlm" => {
            compile_all(main, mount, id("chatLlm"));
            compile_all(main, mount, id("chatTwoSeats"));
            let deleted = delete_character_subprompt(main, mount, a, "Verse").expect("delete");
            let fan = fan_out_subprompt_change(
                main,
                mount,
                a,
                "verse",
                FanoutOptions {
                    remove_selection: true,
                },
                &seams,
            );
            json!({ "deleted": deleted, "fan": fan_json(fan) })
        }
        other => panic!("unknown case {other}"),
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
fn first_diff(a: &Value, b: &Value) -> String {
    let ga = serde_json::to_string_pretty(a).unwrap();
    let gb = serde_json::to_string_pretty(b).unwrap();
    let la: Vec<&str> = ga.lines().collect();
    let lb: Vec<&str> = gb.lines().collect();
    for i in 0..la.len().max(lb.len()) {
        let x = la.get(i).copied().unwrap_or("<none>");
        let y = lb.get(i).copied().unwrap_or("<none>");
        if x != y {
            return format!("  line {i}\n  GOT : {x}\n  WANT: {y}");
        }
    }
    "(identical line-by-line)".to_string()
}

#[test]
fn subprompts_prompt_matches_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_SUBPROMPTS_PROMPT") else {
        return;
    };
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let main_fixture = fixtures_dir().join("subprompts-main.db");
    let mount_fixture = fixtures_dir().join("subprompts-mount.db");

    let oracle: HashMap<String, Value> = std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Value = serde_json::from_str(l).expect("oracle row");
            (v["name"].as_str().unwrap().to_string(), v)
        })
        .collect();
    assert!(!oracle.is_empty(), "empty oracle (the empty-file trap)");
    assert_eq!(
        oracle.len(),
        CASES.len(),
        "oracle case count != corpus (shape)"
    );

    let mut failed: Vec<String> = Vec::new();
    let mut block_cells = 0usize;
    for case in CASES {
        let name = case.name;
        let want = oracle
            .get(name)
            .unwrap_or_else(|| panic!("oracle missing {name}"));
        let scratch =
            std::env::temp_dir().join(format!("qt-spp-rust-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).unwrap();
        let main_work = scratch.join("main.db");
        let mount_work = scratch.join("mount.db");
        std::fs::copy(&main_fixture, &main_work).unwrap();
        std::fs::copy(&mount_fixture, &mount_work).unwrap();
        let main_w =
            Writer::open_writable(&main_work, &spec.test_pepper_base64).expect("open main");
        let mount_w =
            Writer::open_writable(&mount_work, &spec.test_pepper_base64).expect("open mount");
        let main = main_w.connection();
        let mount = mount_w.connection();

        let result = run_op(name, main, mount, &spec.ids);
        let mut stacks = serde_json::Map::new();
        let mut participants = serde_json::Map::new();
        for key in case.chats {
            let cid = spec.ids[*key].clone();
            let chat = chat_of(main, &cid);
            let cell = chat
                .get("compiledIdentityStacks")
                .cloned()
                .unwrap_or(Value::Null);
            if cell.to_string().contains("## Additional Instructions") {
                block_cells += 1;
            }
            stacks.insert(cid.clone(), cell);
            let seats: Vec<Value> = chat
                .get("participants")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .map(|p| {
                            json!([
                                p.get("id").cloned().unwrap_or(Value::Null),
                                p.get("selectedSubpromptIds")
                                    .cloned()
                                    .unwrap_or(Value::Null)
                            ])
                        })
                        .collect()
                })
                .unwrap_or_default();
            participants.insert(cid, Value::Array(seats));
        }
        let got = sorted(&json!({
            "result": result,
            "stacks": Value::Object(stacks),
            "participants": Value::Object(participants),
        }));
        let want_r = sorted(&json!({
            "result": want["result"],
            "stacks": want["stacks"],
            "participants": want["participants"],
        }));
        if got != want_r {
            failed.push(format!("{name}:\n{}", first_diff(&got, &want_r)));
            eprintln!("[{name}] MISMATCH");
        } else {
            eprintln!("[{name}] OK");
        }
        drop((main_w, mount_w));
        let _ = std::fs::remove_dir_all(&scratch);
    }
    // The block must actually be measured somewhere (a stale oracle / a
    // vacuous corpus cannot pass by simply carrying no block anywhere).
    assert!(
        block_cells >= 4,
        "expected the block in at least four dumped cells, saw {block_cells}"
    );
    assert!(
        failed.is_empty(),
        "subprompts-prompt tier-2 mismatches:\n{}",
        failed.join("\n")
    );
}
