//! Read-differential (P4.4 unit 2, sub-unit 2): buildChatContext
//! (`services::chat_initialize::build_chat_context`) vs v4's REAL
//! `buildChatContext`. Both sides READ the SAME baked fixture (three characters
//! with vaults — Aria/llm, Sam/user, Bob/llm-with-defaultPartner=Sam) and run
//! the SAME matrix, comparing the computed `systemPrompt` + `firstMessage` + the
//! resolved character id/name + the resolved user-character id/name EXACTLY (no
//! normalization — this path mints no clock/id).
//!
//! Matrix: a bare responding character (no user char); an explicit user
//! character + a scenario override (aliases/pronouns/description rendered into
//! the prompt); a selected non-default system prompt; the `defaultPartnerId`
//! user-character resolution.
//!
//! **P4.D164 (v4 `2f4254b42`) — the greeting's `## Additional Instructions`.**
//! Five `sp_*` cases pass the FIFTH argument (the opener's subprompts,
//! resolved on both sides through the REAL resolver from Aria's planted
//! `Subprompts/`): two ids without a scenario and with one + a user character
//! (`scene.md` renders `{{scenario}}`/`{{persona}}` — the greeting-local RAW
//! context, no active-scenario fallback), a dangling id, an empty selection,
//! and ids alongside a selected system prompt.
//!
//! **P4.D168 (v4 `0587d1e96`) — the greeting's FORCED progressions report.**
//! Three `prog_*` cases: Sam carries progressions in their vault
//! `metadata.json` (an active recharge, a COMPLETE `once` oath, and one the
//! schema refuses), Aria carries none. The section is composed into the
//! greeting's own flat builder AFTER the `## Additional Instructions` block and
//! BEFORE `You are roleplaying as` — the greeting is built once at chat
//! creation and never cached, which is the whole reason a per-turn clock is
//! allowed in it. Both sides read the SAME frozen instant: the oracle case
//! installs a `FakeDate` at `FIXED_NOW_MS` and the Rust side passes the same
//! constant, or no span could agree.
//!
//! ⚠ A case added to `chat-context-init.ts` alone is NEVER RUN — this family is
//! LIST-DRIVEN (the Rust `cases` array below drives it, and the oracle is a
//! lookup table). The count guard passes either way; add every new case to
//! BOTH lists.
//!
//! Build the fixtures + oracle (Node 24, from the v4 checkout). `TZ=UTC` on
//! BOTH stages: the progressions renderer's `formatInstant` falls back to the
//! host zone in v4 and to UTC in the port, so the two agree only when the
//! oracle's host zone IS UTC:
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   TZ=UTC QT_FIXTURE_CCTX_MAIN=/tmp/qt-cctx-main.db QT_FIXTURE_CCTX_MOUNT=/tmp/qt-cctx-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/fixtures/build-chat-context-init-fixture.ts
//!   TZ=UTC QT_FIXTURE_CCTX_MAIN=/tmp/qt-cctx-main.db QT_FIXTURE_CCTX_MOUNT=/tmp/qt-cctx-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/cases/chat-context-init.ts > /tmp/oracle-cctx.ndjson
//! Run:
//!   QT_ORACLE_CCTX=/tmp/oracle-cctx.ndjson \
//!   QT_FIXTURE_CCTX_MAIN=/tmp/qt-cctx-main.db QT_FIXTURE_CCTX_MOUNT=/tmp/qt-cctx-mount.db \
//!     cargo test -p quilltap-harness --test chat_context_init_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::Writer;
use quilltap_core::services::chat_initialize::build_chat_context;
use quilltap_core::subprompts::resolve_selected_subprompts;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "ariaId")]
    aria_id: String,
    #[serde(rename = "samId")]
    sam_id: String,
    #[serde(rename = "bobId")]
    bob_id: String,
    #[serde(rename = "ariaSp2")]
    aria_sp2: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/chat-context-init.json")
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

/// The oracle's frozen wall clock (`chat-context-init.ts`) — 2024-06-15T12:00:00Z.
/// The greeting's forced progressions report is the only reader.
const FIXED_NOW_MS: i64 = 1_718_452_800_000;

#[test]
fn chat_context_init_matches_oracle() {
    let (Some(oracle_path), Some(main_fixture), Some(mount_fixture)) = (
        env_or_skip("QT_ORACLE_CCTX"),
        env_or_skip("QT_FIXTURE_CCTX_MAIN"),
        env_or_skip("QT_FIXTURE_CCTX_MOUNT"),
    ) else {
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let row: Value = serde_json::from_str(line).expect("oracle line parses");
        oracle.insert(row["id"].as_str().unwrap().to_string(), row);
    }

    let pid = std::process::id();
    let main_work = std::env::temp_dir().join(format!("qt-cctx-main-rust-{pid}.db"));
    let mount_work = std::env::temp_dir().join(format!("qt-cctx-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));

    let main_w = Writer::open_writable(&main_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open main: {e}"));
    let mount_w = Writer::open_writable(&mount_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open mount: {e}"));
    let main = main_w.connection();
    let mount = mount_w.connection();

    // (id, characterId, userCharacterId, scenario, selectedSystemPromptId,
    //  selectedSubpromptIds — P4.D164: `None` = the pre-feature call shape;
    //  `Some(ids)` resolves through `resolve_selected_subprompts` exactly as
    //  the chat-create handler does before the fifth argument.)
    type Case = (
        &'static str,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<Vec<&'static str>>,
    );
    let cases: Vec<Case> = vec![
        ("basic", spec.aria_id.clone(), None, None, None, None),
        (
            "with_user_scenario",
            spec.aria_id.clone(),
            Some(spec.sam_id.clone()),
            Some("A misty harbor at dawn.".to_string()),
            None,
            None,
        ),
        (
            "selected_prompt",
            spec.aria_id.clone(),
            None,
            None,
            Some(spec.aria_sp2.clone()),
            None,
        ),
        (
            "default_partner",
            spec.bob_id.clone(),
            None,
            None,
            None,
            None,
        ),
        // P4.D164 / v4 `2f4254b42`: the greeting's `## Additional Instructions`.
        (
            "sp_two_no_scenario",
            spec.aria_id.clone(),
            None,
            None,
            None,
            Some(vec!["terse", "scene"]),
        ),
        (
            "sp_two_with_scenario_and_user",
            spec.aria_id.clone(),
            Some(spec.sam_id.clone()),
            Some("A misty harbor at dawn.".to_string()),
            None,
            Some(vec!["scene", "TERSE"]),
        ),
        (
            "sp_dangling_only_renders_nothing",
            spec.aria_id.clone(),
            None,
            None,
            None,
            Some(vec!["gone"]),
        ),
        (
            "sp_empty_renders_nothing",
            spec.aria_id.clone(),
            None,
            None,
            None,
            Some(vec![]),
        ),
        (
            "sp_with_selected_prompt",
            spec.aria_id.clone(),
            None,
            None,
            Some(spec.aria_sp2.clone()),
            Some(vec!["terse"]),
        ),
        // P4.D168 / v4 `25f534c0b`: the greeting's FORCED progressions report.
        // Sam carries them; Aria carries none.
        ("prog_forced_greeting", spec.sam_id.clone(), None, None, None, None),
        (
            "prog_forced_with_scenario",
            spec.sam_id.clone(),
            None,
            Some("A misty harbor at dawn.".to_string()),
            None,
            None,
        ),
        ("prog_absent_on_aria", spec.aria_id.clone(), None, None, None, None),
    ];

    let mut subprompt_hits = 0usize;
    for (id, character_id, ucid, scenario, sel, sp_ids) in cases {
        let subprompts = sp_ids.as_ref().map(|ids| {
            let ids: Vec<String> = ids.iter().map(|s| s.to_string()).collect();
            resolve_selected_subprompts(main, mount, &character_id, &ids)
        });
        let ctx = build_chat_context(
            main,
            mount,
            &character_id,
            ucid.as_deref(),
            scenario.as_deref(),
            sel.as_deref(),
            subprompts.as_deref(),
            // P4.D168: the greeting's FORCED progressions report. The oracle
            // freezes `Date.now()` to this same instant.
            FIXED_NOW_MS,
        )
        .unwrap_or_else(|e| panic!("case {id}: build_chat_context: {e}"));
        if id.starts_with("sp_") {
            subprompt_hits += 1;
        }

        let got = json!({
            "id": id,
            "selectedSubpromptIds": sp_ids,
            "systemPrompt": ctx.system_prompt,
            "firstMessage": ctx.first_message,
            "characterId": ctx.character.get("id").and_then(Value::as_str).unwrap_or_default(),
            "characterName": ctx.character.get("name").and_then(Value::as_str).unwrap_or_default(),
            "userCharacterId": ctx.user_character.as_ref().map(|u| u.id.clone()),
            "userCharacterName": ctx.user_character.as_ref().map(|u| u.name.clone()),
        });

        let want = oracle
            .get(id)
            .unwrap_or_else(|| panic!("oracle missing case {id}"));
        assert_eq!(&got, want, "case {id}: rust != oracle");
    }
    // 9 before P4.D168; +3 for the greeting's FORCED progressions report
    // (`prog_forced_greeting`, `prog_forced_with_scenario`,
    // `prog_absent_on_aria`).
    assert_eq!(oracle.len(), 12, "oracle case count drifted");
    assert_eq!(subprompt_hits, 5, "the five P4.D164 `sp_*` cases must run");
    // P4.D168: the forced section must actually appear. These read the ORACLE,
    // so they guard the FIXTURE (a builder that stopped seeding Sam's
    // `metadata.json` would otherwise pass in silence) — they cannot catch a v5
    // regression. What catches THAT is the row-by-row `assert_eq!(&got, want)`
    // above, which is why `prog_forced_greeting` had to join the Rust case list
    // too: a case present only in the oracle satisfies the count guard and is
    // never run.
    let forced = oracle
        .get("prog_forced_greeting")
        .expect("prog_forced_greeting in the oracle")
        .to_string();
    assert!(
        forced.contains("Time-bound conditions you are carrying, as of this moment:"),
        "the greeting must carry the forced progressions section"
    );
    assert!(
        forced.contains("Lantern recharge"),
        "the active progression reports"
    );
    assert!(
        forced.contains("The oath: complete;"),
        "a COMPLETE `once` progression reports in the greeting. MEASURED: this is \
         rule 1 (no last turn ⇒ report everything), not the `force` flag — the \
         greeting supplies no event loader, so a mutation flipping `force` to \
         `false` here stays green on BOTH sides. What the arm pins is that the \
         opener reports unconditionally, with nothing silenced"
    );
    assert!(
        !forced.contains("Broken"),
        "the schema-refused entry is dropped while its siblings survive"
    );
    let absent = oracle
        .get("prog_absent_on_aria")
        .expect("prog_absent_on_aria in the oracle")
        .to_string();
    assert!(
        !absent.contains("Time-bound conditions"),
        "a character carrying none sees a greeting indistinguishable from before"
    );
}
