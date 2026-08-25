//! P4.D115 SCENARIO-CHANGE route-surface differential:
//! `services::chat_scenario::chat_set_scenario` + `services::scenario_selection`
//! vs v4's REAL `handleSetScenario` / `resolveScenarioSelection` (both NEW at v4
//! `44a8137e`). Both sides read a FRESH copy of the committed
//! `chat-scenario-{main,mount}.db` fixture per case (baked ids identical → no
//! remap), then diff the response body AND the post-mutation table dumps.
//!
//! The TABLES are the discriminator, not the body: a no-op re-pick and a real
//! change both answer 200, and only the chat row + the event list say whether
//! anything was written (`dogfood-verify-through-persisted-state`).
//!
//! Coverage is asserted by SHAPE, not by a hand-written count: every case name
//! the oracle emitted must have been driven here, and vice versa
//! (`harness-corpus-shape-constants-rot`).
//!
//! ## Decoding through the `Request` enum
//!
//! Every `verb_*` case builds its body as raw JSON and decodes it with
//! `serde_json::from_value::<Request>` before dispatching, so a field typed too
//! narrowly in `api::types` fails HERE rather than silently collapsing a shape
//! the corpus exists to measure (`edge-must-decode-through-the-request-enum`).
//! That is also why the six fields are `Option<Value>`: v4 validates with Zod
//! inside the handler, so a wrong-TYPED field must reach v4's flat
//! `Validation error` rather than the transport's own decode 400.
//!
//! ## Recorded, direction-asserted differences
//!
//! 1. **[`VALIDATION_DETAILS_GAP`]** — v4's 400 body carries a Zod `details`
//!    array that v5's error envelope does not model (the standing, named P4.6bb
//!    deferral). Asserted in both directions, then dropped before the compare.
//!
//! ## Minted values
//!
//! The Host `scenario-change` bubble mints a UUID and a `createdAt` on both
//! sides, and the chat row's `updatedAt` moves. Those keys are blanked ONLY for
//! the cases that actually write ([`WRITING_CASES`]); the no-op cases keep them
//! fully diffed, which is precisely how "nothing was written" is proven.
//! `compiledIdentityStacks` is keyed by participant id — the ids are baked in
//! the fixture, so the map is compared verbatim (the recompile's whole effect,
//! including the new scene inside each compiled stack).
//!
//! Fixture (Node 24, from the v4 checkout — see the builder's header):
//!   TZ=UTC QT_FIXTURE_CS_MAIN=…/chat-scenario-main.db \
//!   QT_FIXTURE_CS_MOUNT=…/chat-scenario-mount.db \
//!     node --import tsx harness/oracle/fixtures/build-chat-scenario-fixture.ts
//! Generate the oracle (see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-chat-scenario.ndjson npx jest -- chat-scenario-routes
//! Run:
//!   QT_ORACLE_CHAT_SCENARIO=/tmp/oracle-chat-scenario.ndjson \
//!     cargo test -p quilltap-harness --test chat_scenario_routes_equivalence -- --nocapture

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use quilltap_core::api::types::{ErrorKind, Request, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::chat_scenario::{chat_set_scenario, SetScenarioBody};
use quilltap_core::services::scenario_selection::{
    resolve_scenario_selection, ResolveScenarioSelectionOptions, ScenarioSelectionFields,
};
use serde::Deserialize;
use serde_json::{json, Value};

const BEA: &str = "a1000000-0000-4000-8000-000000000002";

const PROJ: &str = "71000000-0000-4000-8000-000000000001";
const PROJ_BARE: &str = "71000000-0000-4000-8000-000000000002";
const GRP: &str = "72000000-0000-4000-8000-000000000001";
const GRP_DANGLING: &str = "72000000-0000-4000-8000-0000000000de";

const CHAT: &str = "c1000000-0000-4000-8000-000000000001";
const NULL_CHAT: &str = "c1000000-0000-4000-8000-000000000002";
const BARE_CHAT: &str = "c1000000-0000-4000-8000-000000000003";
const BROKEN_CHAT: &str = "c1000000-0000-4000-8000-000000000004";
const MISSING_ID: &str = "99999999-9999-4999-8999-999999999999";
/// A well-formed uuid that is on nobody's `scenarios`.
const SCEN_DANGLING: &str = "51000000-0000-4000-8000-0000000000de";

const GENERAL_PATH: &str = "Scenarios/welcome.md";
const GENERAL_BLANK_PATH: &str = "Scenarios/blank.md";
const PROJECT_PATH: &str = "Scenarios/tavern.md";
const GROUP_PATH: &str = "Scenarios/quay.md";
const GONE_PATH: &str = "Scenarios/gone.md";

/// Cases whose 400 body carries v4's Zod `details` array (the standing P4.6bb
/// deferral). Asserted in both directions, then dropped before the compare.
const VALIDATION_DETAILS_GAP: &[&str] = &[
    "verb_bad_scenario_id",
    "verb_bad_group_id",
    "verb_path_too_long",
    "verb_wrong_type_scenario",
    "verb_wrong_type_path",
];

/// Cases that WRITE: a Host bubble is minted (fresh id + `createdAt` on both
/// sides) and the chat row's `updatedAt` moves off the fixture seed. Those keys
/// are asserted-then-stripped here; every other case keeps them, so a no-op that
/// secretly wrote would fail on the untouched-timestamp compare.
const WRITING_CASES: &[&str] = &[
    "verb_general_preset",
    "verb_general_path_body_blank_clears",
    "verb_project_preset",
    "verb_group_preset",
    "verb_character_preset",
    "verb_character_preset_on_removed_participant",
    "verb_free_text_only",
    "verb_free_text_beneath_preset",
    "verb_precedence_character_over_all",
    "verb_empty_payload_clears",
    "verb_blank_free_text_clears",
    "verb_all_fields_null_clears",
    "verb_project_path_without_project",
    "verb_project_without_official_store",
    "verb_project_path_unresolvable",
    "verb_group_path_without_group_id",
    "verb_dangling_scenario_id",
    "verb_unreadable_character_skipped",
];

/// The three cases that export a transcript AFTER a `?action=scenario` call.
const EXPORT_CASES: &[&str] = &[
    "export_after_scenario_change",
    "export_after_scenario_cleared",
    "export_after_noop",
];

/// The fixture's seed timestamp — what an untouched `updatedAt` still reads.
const SEED_ISO: &str = "2026-05-01T00:00:00.000Z";
/// The instant the oracle's TICKING clock starts from (`chat-scenario-web.json`).
const FROZEN_NOW_ISO: &str = "2026-05-05T00:00:00.000Z";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    general_body: String,
    free_text_notes: String,
}

/// The MINTED character-scenario ids, from the fixture's sidecar.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureMeta {
    bea_scenario_ids: HashMap<String, String>,
    drew_scenario_ids: HashMap<String, String>,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/chat-scenario-web.json")
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

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-cs-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("chat-scenario-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("chat-scenario-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db")
}

// ---------------------------------------------------------------------------
// Normalization
// ---------------------------------------------------------------------------

fn canon_numbers(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15 {
                    *v = Value::Number((f as i64).into());
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(canon_numbers),
        Value::Object(o) => o.iter_mut().for_each(|(_, x)| canon_numbers(x)),
        _ => {}
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
/// Blank the Host bubble's minted `id` / `createdAt` (both sides mint fresh
/// ones). Scoped to message objects that carry BOTH keys, so the fixture's
/// baked message ids stay fully diffed.
fn blank_minted_messages(v: &mut Value) {
    if let Some(a) = v.as_array_mut() {
        for m in a.iter_mut() {
            let is_host = m.get("systemKind").and_then(Value::as_str) == Some("scenario-change");
            if is_host {
                if let Some(o) = m.as_object_mut() {
                    for k in ["id", "createdAt", "updatedAt"] {
                        if o.contains_key(k) {
                            o.insert(k.to_string(), Value::String(format!("<{k}>")));
                        }
                    }
                }
            }
        }
    }
}
fn norm(v: &Value) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    serde_json::to_string_pretty(&sorted(&v)).unwrap()
}
fn first_diff(got: &str, want: &str) -> String {
    let g: Vec<&str> = got.lines().collect();
    let w: Vec<&str> = want.lines().collect();
    for i in 0..g.len().max(w.len()) {
        let gi = g.get(i).copied().unwrap_or("<none>");
        let wi = w.get(i).copied().unwrap_or("<none>");
        if gi != wi {
            let mut ctx = String::new();
            for j in i.saturating_sub(3)..i {
                ctx.push_str(&format!("   = {}\n", g.get(j).copied().unwrap_or("")));
            }
            ctx.push_str(&format!("  GOT : {gi}\n  WANT: {wi}\n"));
            return ctx;
        }
    }
    "(identical line-by-line)".to_string()
}

/// Assert-then-strip the wall-clock-minted `updatedAt` on ONE chat object: v4's
/// must be at-or-after the frozen instant and v5's must differ from the fixture
/// seed — i.e. BOTH sides really wrote — before the key is dropped.
fn assert_and_strip_clock(
    name: &str,
    chat: &mut serde_json::Map<String, Value>,
    v4: bool,
) -> Vec<String> {
    let mut failed = Vec::new();
    let label = if v4 { "v4" } else { "v5" };
    for key in ["updatedAt", "lastMessageAt"] {
        let got = chat
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let ok = if v4 {
            got.as_str() >= FROZEN_NOW_ISO
        } else {
            got != SEED_ISO && !got.is_empty()
        };
        if !ok {
            eprintln!(
                "[{name}] {label} {key} = {got:?} — the write did not stamp it (expected {})",
                if v4 {
                    FROZEN_NOW_ISO
                } else {
                    "anything but the seed"
                }
            );
            failed.push(format!("{name}_{label}_{key}"));
        }
        chat.remove(key);
    }
    failed
}

/// The web-edge (status, body) for a Response.
fn status_body(r: &Response) -> (u16, Value) {
    match r {
        Response::ChatAdmin(v) => (200, v.clone()),
        Response::Error(e) => {
            let status = match e.kind {
                ErrorKind::BadRequest => 400,
                ErrorKind::Unauthorized => 401,
                ErrorKind::Forbidden => 403,
                ErrorKind::NotFound => 404,
                ErrorKind::Conflict => 409,
                ErrorKind::Unprocessable => 422,
                ErrorKind::Locked | ErrorKind::Unavailable => 503,
                ErrorKind::Internal => 500,
            };
            (status, json!({ "error": e.message }))
        }
        other => (500, serde_json::to_value(other).unwrap()),
    }
}

fn dump_chat(db: &Db, chat_id: &str) -> Value {
    let cid = chat_id.to_string();
    db.read_main(move |c| quilltap_core::db::chats_read::find_by_id(c, &cid))
        .unwrap()
        .unwrap_or(Value::Null)
}
fn dump_messages(db: &Db, chat_id: &str) -> Value {
    let cid = chat_id.to_string();
    Value::Array(
        db.read_main(move |c| quilltap_core::db::chats_messages_read::get_messages(c, &cid))
            .unwrap(),
    )
}

/// Decode a `?action=scenario` body THROUGH the `Request` enum, then hand the
/// variant's fields to the service. A field typed too narrowly in `api::types`
/// fails here.
fn decode(chat_id: &str, body: &Value) -> SetScenarioBody {
    let mut req = serde_json::Map::new();
    req.insert("type".into(), json!("chatSetScenario"));
    req.insert("chatId".into(), json!(chat_id));
    if let Some(o) = body.as_object() {
        for (k, v) in o {
            req.insert(k.clone(), v.clone());
        }
    }
    let decoded: Request = serde_json::from_value(Value::Object(req))
        .unwrap_or_else(|e| panic!("chatSetScenario body must decode through Request: {e}"));
    match decoded {
        Request::ChatSetScenario {
            chat_id: decoded_id,
            scenario,
            scenario_id,
            project_scenario_path,
            group_scenario_path,
            group_scenario_group_id,
            general_scenario_path,
        } => {
            assert_eq!(decoded_id, chat_id);
            SetScenarioBody {
                scenario,
                scenario_id,
                project_scenario_path,
                group_scenario_path,
                group_scenario_group_id,
                general_scenario_path,
            }
        }
        other => panic!("decoded the wrong variant: {other:?}"),
    }
}

/// The exported transcript's Host heading carries the bubble's `createdAt`,
/// which both sides MINT — v4 from the oracle's frozen clock, v5 from the real
/// one. The timestamp is replaced with a sentinel AFTER asserting the heading's
/// shape, so the notice's PRESENCE (and every byte of its body) still diffs.
/// Returns `None` when a line looks like a Host heading but does not parse,
/// which fails the case rather than silently normalizing something else away.
fn normalize_host_heading(markdown: &str) -> String {
    markdown
        .lines()
        .map(|line| {
            if let Some(rest) = line.strip_prefix("## The Host — ") {
                assert!(
                    !rest.is_empty(),
                    "a Host heading with no timestamp: {line:?}"
                );
                "## The Host — <ts>".to_string()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The `resolver_*` shape the oracle emits (`undefined` does not survive JSON).
fn resolver_body(resolved: Option<String>) -> Value {
    json!({
        "resolved": resolved.clone().map_or(Value::Null, Value::String),
        "wasUndefined": resolved.is_none(),
    })
}

#[test]
fn chat_scenario_routes_match_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_CHAT_SCENARIO") else {
        return;
    };
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let meta: FixtureMeta = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("chat-scenario-main.db.meta.json")).unwrap(),
    )
    .unwrap();
    let scen_bea_1 = meta.bea_scenario_ids["The parlour"].clone();
    let scen_bea_2 = meta.bea_scenario_ids["Watch-change"].clone();
    let scen_drew = meta.drew_scenario_ids["The vacated room"].clone();

    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }
    assert!(
        !oracle.is_empty(),
        "the oracle NDJSON is empty — regenerate it (an erroring builder leaves a stale file)"
    );
    // The export rows' Host heading carries a minted timestamp on both sides —
    // normalize v4's the same way v5's is normalized below.
    for name in EXPORT_CASES {
        let Some(row) = oracle.get_mut(*name) else {
            continue;
        };
        let text = row["body"]["bodyText"].as_str().unwrap_or("").to_string();
        row["body"]["bodyText"] = json!(normalize_host_heading(&text));
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut failed: Vec<String> = Vec::new();
    let mut driven: BTreeSet<String> = BTreeSet::new();

    let mut check = |name: &str, status: u16, body: Value, tables: Option<Value>| {
        driven.insert(name.to_string());
        let Some(want) = oracle.get(name) else {
            failed.push(format!("{name}_MISSING_FROM_ORACLE"));
            return;
        };
        let want_status = want["status"].as_u64().unwrap() as u16;
        let mut want_body = want["body"].clone();
        if status != want_status {
            eprintln!("[{name}] STATUS {status} != {want_status}");
            failed.push(format!("{name}_status"));
        }

        // The Zod `details` gap, asserted in both directions.
        if VALIDATION_DETAILS_GAP.contains(&name) {
            let v4_details_ok = want_body
                .get("details")
                .and_then(Value::as_array)
                .is_some_and(|a| !a.is_empty());
            let v5_has_details = body.get("details").is_some();
            if !v4_details_ok || v5_has_details {
                eprintln!(
                    "[{name}] the recorded validation-details gap no longer holds \
                     (v4 details present={v4_details_ok}, v5 details present={v5_has_details}) \
                     — re-rule it or drop the entry"
                );
                failed.push(format!("{name}_details_gap"));
            }
            if let Some(o) = want_body.as_object_mut() {
                o.remove("details");
            }
        }

        if norm(&body) != norm(&want_body) {
            eprintln!(
                "[{name}] BODY MISMATCH:\n{}",
                first_diff(&norm(&body), &norm(&want_body))
            );
            failed.push(name.to_string());
        } else {
            eprintln!("[{name}] body OK.");
        }

        if let Some(mut got_tables) = tables {
            let mut want_tables = want["tables"].clone();
            if WRITING_CASES.contains(&name) {
                for (is_v4, tbl) in [(true, &mut want_tables), (false, &mut got_tables)] {
                    match tbl.get_mut("chat").and_then(Value::as_object_mut) {
                        Some(chat) => failed.extend(assert_and_strip_clock(name, chat, is_v4)),
                        None => failed.push(format!("{name}_clock_shape")),
                    }
                    if let Some(msgs) = tbl.get_mut("messages") {
                        blank_minted_messages(msgs);
                    }
                }
            }
            if norm(&got_tables) != norm(&want_tables) {
                eprintln!(
                    "[{name} tables] MISMATCH:\n{}",
                    first_diff(&norm(&got_tables), &norm(&want_tables))
                );
                failed.push(format!("{name}_tables"));
            } else {
                eprintln!("[{name} tables] OK.");
            }
        }
    };

    // ── the resolver, in isolation ──────────────────────────────────────────
    struct ResolverCase {
        name: &'static str,
        fields: Value,
        project_id: Option<String>,
        character_id: Option<&'static str>,
    }
    let r = |name: &'static str,
             fields: Value,
             project_id: Option<String>,
             character_id: Option<&'static str>| ResolverCase {
        name,
        fields,
        project_id,
        character_id,
    };
    let resolver_cases = vec![
        r("resolver_nothing_chosen", json!({}), None, None),
        r(
            "resolver_character_scenario",
            json!({ "scenarioId": scen_bea_2 }),
            None,
            Some(BEA),
        ),
        r(
            "resolver_character_beats_every_file_tier",
            json!({
                "scenarioId": scen_bea_1,
                "projectScenarioPath": PROJECT_PATH,
                "groupScenarioPath": GROUP_PATH,
                "groupScenarioGroupId": GRP,
                "generalScenarioPath": GENERAL_PATH,
            }),
            Some(PROJ.to_string()),
            Some(BEA),
        ),
        r(
            "resolver_project_beats_group_and_general",
            json!({
                "projectScenarioPath": PROJECT_PATH,
                "groupScenarioPath": GROUP_PATH,
                "groupScenarioGroupId": GRP,
                "generalScenarioPath": GENERAL_PATH,
            }),
            Some(PROJ.to_string()),
            None,
        ),
        r(
            "resolver_group_beats_general",
            json!({
                "groupScenarioPath": GROUP_PATH,
                "groupScenarioGroupId": GRP,
                "generalScenarioPath": GENERAL_PATH,
            }),
            None,
            None,
        ),
        r(
            "resolver_general_alone",
            json!({ "generalScenarioPath": GENERAL_PATH }),
            None,
            None,
        ),
        r(
            "resolver_free_text_alone",
            json!({ "scenario": "Just the notes." }),
            None,
            None,
        ),
        r(
            "resolver_free_text_beneath_preset",
            json!({ "generalScenarioPath": GENERAL_PATH, "scenario": "It is raining." }),
            None,
            None,
        ),
        r(
            "resolver_dangling_scenario_id_falls_through",
            json!({ "scenarioId": SCEN_DANGLING, "generalScenarioPath": GENERAL_PATH }),
            None,
            Some(BEA),
        ),
        r(
            "resolver_project_path_without_project_id",
            json!({ "projectScenarioPath": PROJECT_PATH, "generalScenarioPath": GENERAL_PATH }),
            None,
            None,
        ),
        r(
            "resolver_project_without_official_store",
            json!({ "projectScenarioPath": PROJECT_PATH, "generalScenarioPath": GENERAL_PATH }),
            Some(PROJ_BARE.to_string()),
            None,
        ),
        r(
            "resolver_group_path_without_group_id",
            json!({ "groupScenarioPath": GROUP_PATH, "generalScenarioPath": GENERAL_PATH }),
            None,
            None,
        ),
        r(
            "resolver_group_id_unknown",
            json!({
                "groupScenarioPath": GROUP_PATH,
                "groupScenarioGroupId": GRP_DANGLING,
                "generalScenarioPath": GENERAL_PATH,
            }),
            None,
            None,
        ),
        r(
            "resolver_project_path_unresolvable",
            json!({ "projectScenarioPath": GONE_PATH, "generalScenarioPath": GENERAL_PATH }),
            Some(PROJ.to_string()),
            None,
        ),
        r(
            "resolver_general_path_body_blank",
            json!({ "generalScenarioPath": GENERAL_BLANK_PATH, "scenario": "Notes survive." }),
            None,
            None,
        ),
        r(
            "resolver_every_tier_fails_free_text_survives",
            json!({ "generalScenarioPath": GONE_PATH, "scenario": "Notes survive." }),
            None,
            None,
        ),
        r(
            "resolver_empty_string_scenario_id",
            json!({ "scenarioId": "", "generalScenarioPath": GENERAL_PATH }),
            None,
            Some(BEA),
        ),
        r(
            "resolver_empty_string_project_id",
            json!({ "projectScenarioPath": PROJECT_PATH, "generalScenarioPath": GENERAL_PATH }),
            Some(String::new()),
            None,
        ),
        r(
            "resolver_empty_string_paths",
            json!({
                "projectScenarioPath": "",
                "groupScenarioPath": "",
                "generalScenarioPath": GENERAL_PATH,
            }),
            None,
            None,
        ),
    ];
    for c in &resolver_cases {
        let db = fresh_db(&spec, c.name);
        let f = &c.fields;
        let s = |k: &str| f.get(k).and_then(Value::as_str).map(str::to_string);
        let character_id = c.character_id.map(str::to_string);
        let project_id = c.project_id.clone();
        let resolved = db
            .read_main(|main| {
                db.read_mount_index(|mount| {
                    let character = match &character_id {
                        Some(id) => {
                            quilltap_core::db::characters_read::find_by_id(main, mount, id)?
                        }
                        None => None,
                    };
                    resolve_scenario_selection(
                        main,
                        mount,
                        &ScenarioSelectionFields {
                            scenario: s("scenario").as_deref(),
                            scenario_id: s("scenarioId").as_deref(),
                            project_scenario_path: s("projectScenarioPath").as_deref(),
                            group_scenario_path: s("groupScenarioPath").as_deref(),
                            group_scenario_group_id: s("groupScenarioGroupId").as_deref(),
                            general_scenario_path: s("generalScenarioPath").as_deref(),
                        },
                        &ResolveScenarioSelectionOptions {
                            project_id: project_id.as_deref(),
                            character: character.as_ref(),
                            log_tag: Some("[Chats v1]"),
                        },
                    )
                })
            })
            .expect("resolver read");
        check(c.name, 200, resolver_body(resolved), None);
    }

    // ── POST ?action=scenario ───────────────────────────────────────────────
    let verb_cases: Vec<(&str, &str, Value, bool)> = vec![
        (
            "verb_general_preset",
            NULL_CHAT,
            json!({ "generalScenarioPath": GENERAL_PATH }),
            true,
        ),
        (
            "verb_general_path_body_blank_clears",
            CHAT,
            json!({ "generalScenarioPath": GENERAL_BLANK_PATH }),
            true,
        ),
        (
            "verb_project_preset",
            CHAT,
            json!({ "projectScenarioPath": PROJECT_PATH }),
            true,
        ),
        (
            "verb_group_preset",
            CHAT,
            json!({ "groupScenarioPath": GROUP_PATH, "groupScenarioGroupId": GRP }),
            true,
        ),
        (
            "verb_character_preset",
            CHAT,
            json!({ "scenarioId": scen_bea_1 }),
            true,
        ),
        (
            "verb_character_preset_on_removed_participant",
            CHAT,
            json!({ "scenarioId": scen_drew }),
            true,
        ),
        (
            "verb_free_text_only",
            CHAT,
            json!({ "scenario": "A rooftop in the rain." }),
            true,
        ),
        (
            "verb_free_text_beneath_preset",
            CHAT,
            json!({ "projectScenarioPath": PROJECT_PATH, "scenario": "It is raining." }),
            true,
        ),
        (
            "verb_precedence_character_over_all",
            CHAT,
            json!({
                "scenarioId": scen_bea_2,
                "projectScenarioPath": PROJECT_PATH,
                "groupScenarioPath": GROUP_PATH,
                "groupScenarioGroupId": GRP,
                "generalScenarioPath": GENERAL_PATH,
            }),
            true,
        ),
        ("verb_empty_payload_clears", CHAT, json!({}), true),
        (
            "verb_blank_free_text_clears",
            CHAT,
            json!({ "scenario": "   " }),
            true,
        ),
        (
            "verb_all_fields_null_clears",
            CHAT,
            json!({
                "scenario": null,
                "scenarioId": null,
                "projectScenarioPath": null,
                "groupScenarioPath": null,
                "groupScenarioGroupId": null,
                "generalScenarioPath": null,
            }),
            true,
        ),
        (
            "verb_noop_same_general_preset",
            CHAT,
            json!({ "generalScenarioPath": GENERAL_PATH }),
            true,
        ),
        (
            "verb_noop_same_custom_text",
            CHAT,
            json!({ "scenario": spec.general_body }),
            true,
        ),
        ("verb_noop_already_cleared", NULL_CHAT, json!({}), true),
        (
            "verb_project_path_without_project",
            NULL_CHAT,
            json!({ "projectScenarioPath": PROJECT_PATH, "generalScenarioPath": GENERAL_PATH }),
            true,
        ),
        (
            "verb_project_without_official_store",
            BARE_CHAT,
            json!({ "projectScenarioPath": PROJECT_PATH, "generalScenarioPath": GENERAL_PATH }),
            true,
        ),
        (
            "verb_project_path_unresolvable",
            CHAT,
            json!({
                "projectScenarioPath": GONE_PATH,
                "generalScenarioPath": GENERAL_BLANK_PATH,
                "scenario": spec.free_text_notes,
            }),
            true,
        ),
        (
            "verb_group_path_without_group_id",
            CHAT,
            json!({
                "groupScenarioPath": GROUP_PATH,
                "generalScenarioPath": GENERAL_BLANK_PATH,
                "scenario": spec.free_text_notes,
            }),
            true,
        ),
        (
            "verb_dangling_scenario_id",
            CHAT,
            json!({ "scenarioId": SCEN_DANGLING, "projectScenarioPath": PROJECT_PATH }),
            true,
        ),
        (
            "verb_unreadable_character_skipped",
            BROKEN_CHAT,
            json!({ "scenarioId": SCEN_DANGLING, "generalScenarioPath": GENERAL_PATH }),
            true,
        ),
        (
            "verb_bad_scenario_id",
            CHAT,
            json!({ "scenarioId": "not-a-uuid" }),
            false,
        ),
        (
            "verb_bad_group_id",
            CHAT,
            json!({ "groupScenarioPath": GROUP_PATH, "groupScenarioGroupId": "nope" }),
            false,
        ),
        (
            "verb_path_too_long",
            CHAT,
            json!({ "generalScenarioPath": "S".repeat(501) }),
            false,
        ),
        (
            "verb_wrong_type_scenario",
            CHAT,
            json!({ "scenario": 123 }),
            false,
        ),
        (
            "verb_wrong_type_path",
            CHAT,
            json!({ "generalScenarioPath": ["a"] }),
            false,
        ),
        (
            "verb_chat_missing",
            MISSING_ID,
            json!({ "generalScenarioPath": GENERAL_PATH }),
            false,
        ),
        // The COMPOSITE guard order: the route 404s before it dispatches, so an
        // invalid body against a missing chat is a 404, not a 400.
        (
            "verb_bad_body_chat_missing",
            MISSING_ID,
            json!({ "scenarioId": "not-a-uuid" }),
            false,
        ),
    ];
    for (name, chat_id, body, dump) in &verb_cases {
        let db = fresh_db(&spec, name);
        let r = rt.block_on(chat_set_scenario(&db, chat_id, decode(chat_id, body)));
        let (status, out) = status_body(&r);
        let tables = dump.then(
            || json!({ "chat": dump_chat(&db, chat_id), "messages": dump_messages(&db, chat_id) }),
        );
        check(name, status, out, tables);
    }

    // ── the transcript carry (`scenario-change` ∈ HOST_LINK_KINDS) ──────────
    for (name, body) in [
        (
            EXPORT_CASES[0],
            json!({ "projectScenarioPath": PROJECT_PATH }),
        ),
        (EXPORT_CASES[1], json!({})),
        (
            EXPORT_CASES[2],
            json!({ "generalScenarioPath": GENERAL_PATH }),
        ),
    ] {
        let db = fresh_db(&spec, name);
        rt.block_on(chat_set_scenario(&db, CHAT, decode(CHAT, &body)));
        let r = quilltap_core::services::markdown_transcript::chat_export_markdown(
            &db,
            "f0000000-0000-4000-8000-00000000000f",
            CHAT,
        );
        let (status, body_out) = match &r {
            Response::ChatMarkdownTranscriptPayload { markdown, .. } => (
                200u16,
                json!({ "bodyText": normalize_host_heading(markdown) }),
            ),
            other => status_body(other),
        };
        check(name, status, body_out, None);
    }

    // Coverage by SHAPE: the oracle's names and the driven names must agree.
    let oracle_names: BTreeSet<String> = oracle.keys().cloned().collect();
    let missing: Vec<&String> = oracle_names.difference(&driven).collect();
    assert!(
        missing.is_empty(),
        "oracle cases never driven by the Rust side: {missing:?}"
    );

    assert!(
        failed.is_empty(),
        "{} case(s) failed: {failed:?}",
        failed.len()
    );
}

/// The §Shared-contract wire shape, pinned name-for-name so a rename or a serde
/// slip fails HERE rather than in P4.D116's SPA (the `p4_d10_wire_contract`
/// precedent). Needs no oracle or fixture.
#[test]
fn chat_set_scenario_wire_shape() {
    let decoded: Request = serde_json::from_str(
        r#"{"type":"chatSetScenario","chatId":"c1","scenario":"notes",
            "scenarioId":"11111111-1111-4111-8111-111111111111",
            "projectScenarioPath":"Scenarios/p.md",
            "groupScenarioPath":"Scenarios/g.md",
            "groupScenarioGroupId":"22222222-2222-4222-8222-222222222222",
            "generalScenarioPath":"Scenarios/gen.md"}"#,
    )
    .expect("chatSetScenario");
    assert_eq!(
        decoded,
        Request::ChatSetScenario {
            chat_id: "c1".into(),
            scenario: Some(json!("notes")),
            scenario_id: Some(json!("11111111-1111-4111-8111-111111111111")),
            project_scenario_path: Some(json!("Scenarios/p.md")),
            group_scenario_path: Some(json!("Scenarios/g.md")),
            group_scenario_group_id: Some(json!("22222222-2222-4222-8222-222222222222")),
            general_scenario_path: Some(json!("Scenarios/gen.md")),
        }
    );
    // Every field is optional: the bare verb decodes, and clears the scene.
    assert_eq!(
        serde_json::from_str::<Request>(r#"{"type":"chatSetScenario","chatId":"c1"}"#)
            .expect("bare chatSetScenario"),
        Request::ChatSetScenario {
            chat_id: "c1".into(),
            scenario: None,
            scenario_id: None,
            project_scenario_path: None,
            group_scenario_path: None,
            group_scenario_group_id: None,
            general_scenario_path: None,
        }
    );
    // A wrong-TYPED field must still DECODE, so the handler can answer v4's flat
    // `Validation error` rather than the transport's own decode 400 (the module
    // header's note).
    assert_eq!(
        serde_json::from_str::<Request>(
            r#"{"type":"chatSetScenario","chatId":"c1","scenario":123}"#
        )
        .expect("wrong-typed scenario must still decode"),
        Request::ChatSetScenario {
            chat_id: "c1".into(),
            scenario: Some(json!(123)),
            scenario_id: None,
            project_scenario_path: None,
            group_scenario_path: None,
            group_scenario_group_id: None,
            general_scenario_path: None,
        }
    );
}

/// v4 `helpers.ts:512-519`, carried: `updateChatSchema` DELIBERATELY does not
/// expose `scenarioText`, because a bare field update would skip the identity-
/// stack recompile AND the Host's revision announcement. v5's dispatch
/// `chatUpdate` bag must stay equally closed.
///
/// Two pins, because either alone is weak: a BEHAVIORAL one (a `chatUpdate`
/// carrying `scenarioText` leaves the column exactly as it was) and a SOURCE
/// CENSUS (`ChatUpdate.scenario_text` is constructed at exactly ONE site, the
/// scenario verb) — the `db_error_key_guard` idiom, since a new writer would
/// pass the behavioral test by simply being somewhere else.
#[test]
fn chat_update_surface_cannot_reach_scenario_text() {
    let Ok(raw) = std::fs::read_to_string(spec_path()) else {
        eprintln!("SKIP: the chat-scenario spec is missing.");
        return;
    };
    let spec: Spec = serde_json::from_str(&raw).unwrap();
    let db = fresh_db(&spec, "update_surface_guard");
    let before = dump_chat(&db, CHAT);
    let seeded = before
        .get("scenarioText")
        .and_then(Value::as_str)
        .expect("the fixture chat must start with a scene")
        .to_string();
    assert_eq!(seeded, spec.general_body);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let r = rt.block_on(quilltap_core::api::salon::chat_update(
        &db,
        "f0000000-0000-4000-8000-00000000000f",
        CHAT,
        &json!({ "scenarioText": "hijacked", "title": "A New Title" }),
        None,
        None,
        None,
    ));
    assert!(
        !matches!(r, Response::Error(_)),
        "the update itself must succeed: {r:?}"
    );
    let after = dump_chat(&db, CHAT);
    assert_eq!(
        after.get("scenarioText").and_then(Value::as_str),
        Some(seeded.as_str()),
        "chatUpdate reached scenarioText — v4 keeps that field off the update \
         surface on purpose (helpers.ts:512-519); scenario changes must go \
         through ?action=scenario so the recompile and the announcement happen"
    );
    // …and the sibling key in the same bag DID land, so the assertion above is
    // not passing because the whole update was ignored.
    assert_eq!(
        after.get("title").and_then(Value::as_str),
        Some("A New Title"),
        "the update did not apply at all — the guard above proves nothing"
    );

    // The census: exactly one construction site for the setter.
    let core_src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-core/src");
    let mut sites: Vec<String> = Vec::new();
    let mut stack = vec![core_src];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                // `ChatUpdate` literals only — `scenario_text: Some(` also
                // appears in unrelated builder structs (a system-prompt test
                // fixture), so the census scopes on the surrounding literal.
                let n = text
                    .split("ChatUpdate {")
                    .skip(1)
                    .filter(|tail| {
                        tail.split("..Default::default()")
                            .next()
                            .is_some_and(|body| body.contains("scenario_text: Some("))
                    })
                    .count();
                if n > 0 {
                    sites.push(format!(
                        "{}:{n}",
                        path.file_name().unwrap().to_string_lossy()
                    ));
                }
            }
        }
    }
    sites.sort();
    assert_eq!(
        sites,
        vec!["chat_scenario.rs:1".to_string()],
        "`ChatUpdate.scenario_text` gained a writer outside the scenario verb — \
         every write to that column must recompile the identity stacks and post \
         the Host's revision notice (v4 helpers.ts:512-519)"
    );
}
