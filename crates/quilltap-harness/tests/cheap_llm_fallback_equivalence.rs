//! P4.D135 — the cheap-LLM fallback chain builder (v4 `65f5021c8`,
//! `lib/memory/cheap-llm-tasks/fallback.ts`), tier-2 over a real DB.
//!
//! Drives v4's REAL `buildCheapFallbackSelections` and diffs
//! `quilltap_core::services::cheap_llm_fallback::build_cheap_fallback_selections`
//! against it, comparing the SELECTIONS — provider, model, baseUrl,
//! connectionProfileId, isLocal and the resolved `profileParams`. That last one
//! is the point: the chain speaks connection profiles and the cheap path speaks
//! selections, and the conversion is where the two currencies meet.
//!
//! Both sides run against a copy of the same fixture and apply the SAME wiring
//! statements first (the committed settings fixture has no chain, no cheap
//! profile, and `allowCheapFallback` unset). The Rust side replays them through
//! its own repositories so the world under test is built the same way, not
//! assumed.
//!
//! ⚠ **Two vacuity traps this case had to walk around, both measured:**
//!
//! 1. The profile-less arm stands in a SYNTHETIC failed profile with no model
//!    class, and `tierMatches` reads unknown-vs-KNOWN as a non-match in both
//!    directions — so a *classified* cheap profile is never drafted and the
//!    switch's two positions both answer `[]`. The cheap profile's `modelClass`
//!    is cleared before those arms.
//! 2. v4's `jest.setup.ts` mocks the WHOLE DB stack, and `safeQuery` swallows
//!    the resulting failure: every seeding write quietly no-ops and the case
//!    goes green having measured a world it never built. The oracle un-mocks
//!    the manager, the repositories and the factory.
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout; jest ignores
//! `.claude/` paths, so the case is staged in a /tmp mirror):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-cheapfallback-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/cheap-llm-fallback.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/settings.json"           "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_SETTINGS_MAIN=/tmp/qt-cheapfallback-fixture.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-settings-fixture.ts
//!   QT_FIXTURE_CHEAP_FALLBACK=/tmp/qt-cheapfallback-fixture.db \
//!   QT_ORACLE_OUT=/tmp/oracle-cheap-llm-fallback.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- "cheap-llm-fallback\.test\.ts$"
//! Run:
//!   QT_ORACLE_CHEAP_FALLBACK=/tmp/oracle-cheap-llm-fallback.ndjson \
//!   QT_FIXTURE_CHEAP_FALLBACK=/tmp/qt-cheapfallback-fixture.db \
//!     cargo test -p quilltap-harness --test cheap_llm_fallback_equivalence -- --nocapture

use std::path::PathBuf;

use quilltap_core::cheap_llm::CheapLlmSelection;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::Writer;
use quilltap_core::services::cheap_llm_fallback::{
    build_cheap_fallback_selections, CheapFallbackRequest,
};
use serde_json::{json, Value};

/// The settings family's pepper — this case reuses its fixture, and that family
/// keys with a DIFFERENT throwaway key from the restore family's.
const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/settings.json")
}

fn selection_json(s: &CheapLlmSelection) -> Value {
    json!({
        "provider": s.provider,
        "modelName": s.model_name,
        "baseUrl": s.base_url,
        "connectionProfileId": s.connection_profile_id,
        "isLocal": s.is_local,
        "profileParameters": s.profile_parameters,
    })
}

/// v4 emits the selection object as `JSON.stringify` renders it: an `undefined`
/// key is DROPPED, and `profileParameters` is absent when `profileParams`
/// returns nothing. v5's `Option`s render as `null`, so the comparison folds
/// null and absent together on BOTH sides rather than on one.
fn normalize(mut v: Value) -> Value {
    if let Some(obj) = v.as_object_mut() {
        obj.retain(|_, val| !val.is_null());
    }
    v
}

fn selection_from_oracle(v: &Value) -> CheapLlmSelection {
    CheapLlmSelection {
        provider: v["provider"].as_str().unwrap_or("").to_string(),
        model_name: v["modelName"].as_str().unwrap_or("").to_string(),
        base_url: v["baseUrl"].as_str().map(str::to_string),
        connection_profile_id: v["connectionProfileId"].as_str().map(str::to_string),
        is_local: v["isLocal"].as_bool().unwrap_or(false),
        profile_parameters: v.get("profileParameters").cloned().filter(|p| !p.is_null()),
    }
}

#[test]
fn cheap_llm_fallback_matches_oracle() {
    let (Ok(oracle_path), Ok(fixture)) = (
        std::env::var("QT_ORACLE_CHEAP_FALLBACK"),
        std::env::var("QT_FIXTURE_CHEAP_FALLBACK"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_CHEAP_FALLBACK + QT_FIXTURE_CHEAP_FALLBACK (see the header)."
        );
        return;
    };
    let raw = std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let cases: Vec<Value> = raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle line is JSON"))
        .collect();
    assert!(!cases.is_empty(), "oracle produced no cases");

    let spec: Value =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).expect("read settings.json"))
            .expect("settings.json parses");
    let user_a = spec["userA"].as_str().expect("userA").to_string();
    let gpt = spec["profiles"]["gpt"].as_str().expect("gpt").to_string();
    let claude = spec["profiles"]["claude"]
        .as_str()
        .expect("claude")
        .to_string();

    let work = std::env::temp_dir().join(format!("qt-cheap-fallback-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // The SAME wiring statements the oracle applied, in the same order.
    {
        let w = Writer::open_writable(&work, TEST_PEPPER).expect("open fixture copy");
        let repo = quilltap_core::db::connection_profiles::ConnectionProfilesRepository::new(
            w.connection(),
        );
        repo.update(
            &gpt,
            &quilltap_core::db::connection_profiles::CpUpdate {
                fallback_profile_id: Some(Some(claude.clone())),
                allow_tier_fallback: Some(true),
                model_class: Some("Standard".into()),
                updated_at: "2020-01-01T00:00:00.000Z".into(),
                ..Default::default()
            },
        )
        .expect("wire the primary's chain");
        repo.update(
            &claude,
            &quilltap_core::db::connection_profiles::CpUpdate {
                is_cheap: Some(true),
                model_class: Some("Standard".into()),
                updated_at: "2020-01-01T00:00:00.000Z".into(),
                ..Default::default()
            },
        )
        .expect("make the understudy cheap");
    }

    let db = Db::open(
        DbPaths {
            main: work.clone(),
            mount_index: None,
            llm_logs: None,
        },
        TEST_PEPPER,
    )
    .unwrap_or_else(|e| panic!("open fixture copy: {e}"));

    let mut failed: Vec<String> = Vec::new();
    let mut nonempty = 0usize;

    for case in &cases {
        let name = case["name"].as_str().expect("case name");

        // The two statements the oracle interleaves between arms, replayed at
        // the same points: the switch, and the modelClass clear.
        if let Some(allow) = case["allowCheapFallback"].as_bool() {
            set_allow_cheap_fallback(&work, &user_a, allow);
        }
        if name == "profileless_switch_off" {
            let w = Writer::open_writable(&work, TEST_PEPPER).expect("open fixture copy");
            quilltap_core::db::connection_profiles::ConnectionProfilesRepository::new(
                w.connection(),
            )
            .update(
                &claude,
                &quilltap_core::db::connection_profiles::CpUpdate {
                    clear_model_class: true,
                    updated_at: "2020-01-01T00:00:00.000Z".into(),
                    ..Default::default()
                },
            )
            .expect("clear the cheap profile's model class");
        }

        let selection = selection_from_oracle(&case["selection"]);
        let already_tried: Vec<String> = case["alreadyTried"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();

        let got = build_cheap_fallback_selections(
            &db,
            CheapFallbackRequest {
                selection: &selection,
                user_id: &user_a,
                dangerous: case["dangerous"].as_bool().unwrap_or(false),
                already_tried,
                task_type: Some("oracle"),
            },
        );

        let got_json: Vec<Value> = got.iter().map(|s| normalize(selection_json(s))).collect();
        let want: Vec<Value> = case["selections"]
            .as_array()
            .expect("selections")
            .iter()
            .cloned()
            .map(normalize)
            .collect();
        if !want.is_empty() {
            nonempty += 1;
        }
        if got_json != want {
            failed.push(format!(
                "{name}:\n    rust   {}\n    oracle {}",
                Value::Array(got_json),
                Value::Array(want)
            ));
        }
    }

    let _ = std::fs::remove_file(&work);

    assert!(
        failed.is_empty(),
        "{} of {} case(s) failed:\n{}",
        failed.len(),
        cases.len(),
        failed.join("\n")
    );
    // Shape assertion: an all-empty corpus proves only that both sides refuse.
    assert!(
        nonempty >= 3,
        "only {nonempty} case(s) produced a stand-in — the corpus has gone vacuous \
         (see the two traps in the header)"
    );
    eprintln!(
        "OK cheap-llm fallback: {} cases ({nonempty} producing a stand-in)",
        cases.len()
    );
}

/// The oracle sets the switch through v4's repository; v5 writes the same cell.
fn set_allow_cheap_fallback(main: &std::path::Path, user_id: &str, value: bool) {
    let w = Writer::open_writable(main, TEST_PEPPER).expect("open fixture copy");
    let conn = w.connection();
    let row = quilltap_core::db::chat_settings::find_by_user_id(conn, user_id)
        .expect("read chat settings")
        .expect("the settings fixture seeds userA's chat_settings row");
    let mut bag = row
        .get("cheapLLMSettings")
        .cloned()
        .unwrap_or_else(|| json!({}));
    bag.as_object_mut()
        .expect("cheapLLMSettings is an object")
        .insert("allowCheapFallback".into(), json!(value));
    let id = row["id"].as_str().expect("settings id");
    conn.execute(
        "UPDATE chat_settings SET cheapLLMSettings = ?1 WHERE id = ?2",
        rusqlite::params![serde_json::to_string(&bag).unwrap(), id],
    )
    .expect("write the switch");
}
