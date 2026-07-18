//! Integration check (P4.6ay unit 9): the roster resolving INSIDE `build_tools`
//! and reaching `run_custom`'s description. Over the committed
//! `pascal-run-custom-{main,mount}.db` fixture, `build_tools` with a
//! `custom_tool_context` on character A's vault must offer `run_custom` with a
//! description IDENTICAL to `build_run_custom_description` over the roster
//! `resolve_custom_tool_roster` returns for that context; a `None` context must
//! not offer it (the `customTools` setting off).
//!
//! The two constituents are each a v4 differential: `resolve_custom_tool_roster`
//! (`pascal_roster_equivalence`) and `build_run_custom_description`
//! (`pascal_run_custom_equivalence`); the SAME roster over the SAME fixture is
//! also pinned to v4's order by the handler oracle's `unknown-tool` case
//! (`Available: ansible, coin, whispered`). This confirms the unit-9 wiring
//! composes them correctly end-to-end on a real DB.
//!
//! No oracle env var — it drives the committed fixture directly.
//!   cargo test -p quilltap-harness --test pascal_build_tools_roster

use std::path::PathBuf;

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::pascal::roster::{resolve_custom_tool_roster, RosterContext};
use quilltap_core::services::tool_build::{build_tools, BuildToolsInput};
use quilltap_core::tools::run_custom::build_run_custom_description;

const CHAT: &str = "c1000000-0000-4000-8000-000000000001";
const CHAR_A: &str = "a1000000-0000-4000-8000-00000000000a";
const USER: &str = "e18e05bc-63e8-4539-8a85-719b7a508850";
const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

#[derive(serde::Deserialize)]
struct Meta {
    #[serde(rename = "vaultA")]
    vault_a: String,
}

fn tool_name(t: &serde_json::Value) -> Option<&str> {
    t.get("function")
        .and_then(|f| f.get("name"))
        .and_then(|n| n.as_str())
        .or_else(|| t.get("name").and_then(|n| n.as_str()))
}

fn open() -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-pascal-bt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("pascal-run-custom-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("pascal-run-custom-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        PEPPER,
    )
    .expect("open db")
}

fn base_input<'a>(ctx: Option<RosterContext>) -> BuildToolsInput<'a> {
    BuildToolsInput {
        provider: "OPENAI",
        use_native_web_search: false,
        allow_tool_use: None,
        allow_web_search: false,
        image_profile_id: None,
        image_provider_constraints: None,
        project_id: None,
        request_full_context: false,
        disabled_tools: Some(&[]),
        disabled_tool_groups: &[],
        agent_mode_enabled: false,
        is_multi_character: false,
        help_tools_enabled: false,
        can_dress_themselves: false,
        can_create_outfits: false,
        document_editing_enabled: false,
        ask_carina_enabled: false,
        include_workspace_tools: false,
        exclude_memory_search: false,
        sql_access: false,
        model_supports_native_tools: true,
        provider_supports_web_search: false,
        custom_tool_context: ctx,
    }
}

#[test]
fn build_tools_resolves_and_offers_run_custom() {
    let meta: Meta = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("pascal-run-custom-main.db.meta.json"))
            .unwrap(),
    )
    .unwrap();

    let ctx = RosterContext {
        user_id: USER.into(),
        chat_id: CHAT.into(),
        character_id: Some(CHAR_A.into()),
        character_mount_point_id: Some(meta.vault_a.clone()),
        character_ids: Some(vec![CHAR_A.into()]),
        project_id: None,
    };

    // A Some(context) → run_custom is offered, its description built from exactly
    // the resolved roster.
    let db = open();
    let built = build_tools(&db, USER, &base_input(Some(ctx.clone()))).unwrap();
    let run_custom = built
        .tools
        .iter()
        .find(|t| tool_name(t) == Some("run_custom"))
        .expect("run_custom offered for a non-empty roster");

    let roster = db
        .read_main(|main| {
            db.read_mount_index(|mount| Ok(resolve_custom_tool_roster(&ctx, main, mount)))
        })
        .unwrap();
    let expected_roster: Vec<_> = roster.tools.iter().map(|(_, t)| t.clone()).collect();
    // The handler oracle's `unknown-tool` case pins this order to v4:
    // ansible, coin, whispered.
    assert_eq!(
        expected_roster
            .iter()
            .map(|t| t.definition.name.as_str())
            .collect::<Vec<_>>(),
        vec!["ansible", "coin", "whispered"]
    );
    assert_eq!(
        run_custom["function"]["description"].as_str().unwrap(),
        build_run_custom_description(&expected_roster)
    );

    // A None context → run_custom is never built (the setting off).
    let db2 = open();
    let built2 = build_tools(&db2, USER, &base_input(None)).unwrap();
    assert!(built2
        .tools
        .iter()
        .all(|t| tool_name(t) != Some("run_custom")));

    eprintln!("OK: build_tools resolves the roster and offers run_custom.");
}
