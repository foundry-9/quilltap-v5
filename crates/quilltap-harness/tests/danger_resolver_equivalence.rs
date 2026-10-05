//! W4.2 differential #1: the dangerous-content **mode resolver + per-chat
//! override truth table** (tier-1, pure) and the **manual Concierge flip**
//! (tier-2, chat-row dump).
//!
//! Two independent sections, each gated on its own env var (skips if unset):
//!
//!   1. `QT_ORACLE_DANGER_RESOLVER` — the pure NDJSON from
//!      `harness/oracle/cases/danger-resolver.ts` (drives v4's REAL
//!      `resolveDangerousContentSettings` + every `chat-override` export).
//!      P4.D226 (v4 `4d370a90f`, #75 — the three states) rewrote it: the
//!      resolver's per-chat arms are `chat-locked` / `chat-unmoderated` over
//!      `conciergeMode` (the legacy pair proven IGNORED), and the override rows
//!      ask all nine questions over v4's own 3x2 TABLE + the payload-key and
//!      hydrated-row edges; `derive` / `withLegacy` / `states` rows pin the
//!      legacy mapping, the spread's key order and `CONCIERGE_STATES`.
//!   2. `QT_ORACLE_DANGER_MANUAL_FLIP` + `QT_FIXTURE_MANUAL_FLIP` — the tier-2
//!      NDJSON from `harness/oracle/cases/danger-manual-flip.test.ts` (drives
//!      v4's REAL `applyConciergeFlip`) + the baked seed fixture. P4.D226: v4's
//!      `manual-flip.test.ts` cases as planted rows, the `chats` +
//!      `chat_messages` dumps AND every op's `ConciergeManualFlip` lines
//!      compared in order (v4's recorder also keeps the repository's
//!      `Concierge state write` DEBUG, which v5 logs on the writer thread and
//!      `db::chats`'s tests pin directly — filtered out here).
//!
//! Generate (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-danger-manual-flip-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/harness/oracle/cases" "$TMPO/harness/oracle/fixtures"
//!   cp "$V5W/harness/oracle/cases/danger-manual-flip.test.ts" "$TMPO/harness/oracle/cases/"
//!   cp "$V5W/harness/oracle/fixtures/danger-manual-flip.json" "$TMPO/harness/oracle/fixtures/"
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/danger-resolver.ts > /tmp/oracle-danger-resolver.ndjson
//!   QT_FIXTURE_OUT=/tmp/qt-danger-manual-flip.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-danger-manual-flip-fixture.ts
//!   QT_FIXTURE_MANUAL_FLIP=/tmp/qt-danger-manual-flip.db \
//!   QT_ORACLE_OUT=/tmp/oracle-danger-manual-flip.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$TMPO/harness/oracle/cases" \
//!       -- "cases/danger-manual-flip\.test\.ts$"
//! Run:
//!   QT_ORACLE_DANGER_RESOLVER=/tmp/oracle-danger-resolver.ndjson \
//!   QT_ORACLE_DANGER_MANUAL_FLIP=/tmp/oracle-danger-manual-flip.ndjson \
//!   QT_FIXTURE_MANUAL_FLIP=/tmp/qt-danger-manual-flip.db \
//!     cargo test -p quilltap-harness --test danger_resolver_equivalence

use quilltap_core::db::runtime::Db;
use quilltap_core::db::{chats_read, dump_table_json_conn};
use quilltap_core::services::dangerous_content::chat_override::{
    concierge_state_may_fail_over, concierge_state_uses_uncensored_route,
    derive_concierge_mode_from_legacy, get_concierge_provenance, get_concierge_reason,
    get_concierge_state, is_classifier_on_duty, may_fail_over, should_show_danger_styling,
    should_use_uncensored_route, with_concierge_mode_from_legacy, ConciergeState, CONCIERGE_STATES,
};
use quilltap_core::services::dangerous_content::legacy_concierge_settings::{
    map_legacy_concierge_settings, with_concierge_settings_from_legacy, LegacyConciergeSources,
};
use quilltap_core::services::dangerous_content::manual_flip::{
    apply_concierge_flip_with, RealConciergeAnnouncer,
};
use quilltap_core::services::dangerous_content::resolver::{
    default_concierge_settings, read_concierge_settings, resolve_concierge_settings,
    resolve_configured_concierge_desk, DEFAULT_AUTO_SWITCH_AFTER_REFUSALS,
};
use serde::Deserialize;
use serde_json::Value;

/// Recursively collapse integer-valued JSON floats to integers, matching JS
/// `JSON.stringify` (which renders `1.0` as `1`). serde renders an f64 `1.0` as
/// `1.0`, so both sides are canonicalized before comparison.
fn canon_numbers(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if n.is_f64() && f.fract() == 0.0 && f.abs() < 9.007e15 {
                    *v = Value::Number(serde_json::Number::from(f as i64));
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(canon_numbers),
        Value::Object(o) => o.values_mut().for_each(canon_numbers),
        _ => {}
    }
}

fn canon(mut v: Value) -> Value {
    canon_numbers(&mut v);
    v
}

// --- section 1: pure resolver + the Concierge truth table ---

/// v4's `undefined` (rendered `"<undefined>"`) and `null` are both "absent" to
/// a Rust `Option<&Value>`; every other value is passed through.
fn undefined_as_none(v: &Value) -> Option<&Value> {
    match v {
        Value::Null => None,
        Value::String(s) if s == "<undefined>" => None,
        other => Some(other),
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum PureRow {
    /// P4.D227 (v4 `3b463d6b1`, #76): `resolveConciergeSettings` — the whole
    /// `ResolvedConciergePolicy`, over v4's `resolver.test.ts` builders.
    #[serde(rename = "resolve")]
    Resolve {
        id: String,
        /// `"<undefined>"` for v4's `undefined` carrier.
        global: Value,
        chat: Value,
        policy: Value,
    },
    /// `DEFAULT_CONCIERGE_SETTINGS` + `DEFAULT_AUTO_SWITCH_AFTER_REFUSALS`.
    #[serde(rename = "defaults")]
    Defaults {
        id: String,
        settings: Value,
        #[serde(rename = "autoSwitch")]
        auto_switch: i64,
    },
    /// `readConciergeSettings` — the merged object's BYTES.
    #[serde(rename = "readSettings")]
    ReadSettings {
        id: String,
        global: Value,
        settings: Value,
    },
    /// `mapLegacyConciergeSettings` — the migrated object's BYTES.
    #[serde(rename = "legacyMap")]
    LegacyMap {
        id: String,
        sources: Value,
        migrated: Value,
    },
    /// `withConciergeSettingsFromLegacy` — the returned record's BYTES and
    /// v4's `toBe(settings)` identity.
    #[serde(rename = "withSettingsLegacy")]
    WithSettingsLegacy {
        id: String,
        settings: Value,
        #[serde(rename = "hasUnmoderatedChats")]
        has_unmoderated_chats: bool,
        out: Value,
        identical: bool,
    },
    /// P4.D226 (v4 `4d370a90f`): every question `chat-override.ts` exports,
    /// over v4's own 3x2 TABLE + the payload-key and hydrated-row edges.
    #[serde(rename = "override")]
    Override {
        id: String,
        /// `"<undefined>"` for v4's `undefined` chat (JSON has no undefined).
        chat: Value,
        state: String,
        provenance: Option<String>,
        reason: Option<Value>,
        #[serde(rename = "uncensoredRoute")]
        uncensored_route: bool,
        #[serde(rename = "dangerStyling")]
        danger_styling: bool,
        #[serde(rename = "classifierOnDuty")]
        classifier_on_duty: bool,
        #[serde(rename = "stateUsesUncensoredRoute")]
        state_uses_uncensored_route: bool,
        #[serde(rename = "mayFailOver")]
        may_fail_over: bool,
        #[serde(rename = "stateMayFailOver")]
        state_may_fail_over: bool,
    },
    /// Both state-only twins on a literal state, with no chat anywhere.
    #[serde(rename = "stateRoute")]
    StateRoute {
        id: String,
        state: String,
        #[serde(rename = "usesUncensoredRoute")]
        uses_uncensored_route: bool,
        #[serde(rename = "mayFailOver")]
        may_fail_over: bool,
    },
    /// `CONCIERGE_STATES`, in control order.
    #[serde(rename = "states")]
    States { id: String, states: Vec<String> },
    /// `deriveConciergeModeFromLegacy`.
    #[serde(rename = "derive")]
    Derive {
        id: String,
        legacy: Value,
        columns: Value,
    },
    /// P4.D228 (v4 `ce2f1dabf`, #77): `resolveConfiguredConciergeDesk` — the
    /// desk AS CONFIGURED, off duty included (the operator's explicit "Try
    /// uncensored" is the one reader).
    #[serde(rename = "configuredDesk")]
    ConfiguredDesk {
        id: String,
        global: Value,
        desk: Value,
    },
    /// `withConciergeModeFromLegacy` — the returned object's BYTES (key order)
    /// and v4's `toBe(chat)` identity.
    #[serde(rename = "withLegacy")]
    WithLegacy {
        id: String,
        chat: Value,
        out: Value,
        identical: bool,
    },
}

#[test]
fn danger_resolver_pure_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_DANGER_RESOLVER") else {
        eprintln!("SKIP: set QT_ORACLE_DANGER_RESOLVER to the pure NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));

    let mut count = 0usize;
    let mut kinds: std::collections::BTreeMap<&'static str, usize> = Default::default();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: PureRow = serde_json::from_str(line).expect("parse pure row");
        match row {
            PureRow::Resolve {
                id,
                global,
                chat,
                policy,
            } => {
                let got = resolve_concierge_settings(
                    undefined_as_none(&global),
                    undefined_as_none(&chat),
                );
                // Bytes: the policy's key order is v4's interface order, and
                // `threshold` a JS number (`1`, never `1.0`).
                assert_eq!(
                    serde_json::to_string(&got).unwrap(),
                    serde_json::to_string(&policy).unwrap(),
                    "resolve[{id}] policy"
                );
                *kinds.entry("resolve").or_default() += 1;
            }
            PureRow::Defaults {
                id,
                settings,
                auto_switch,
            } => {
                assert_eq!(
                    serde_json::to_string(&default_concierge_settings()).unwrap(),
                    serde_json::to_string(&settings).unwrap(),
                    "defaults[{id}] DEFAULT_CONCIERGE_SETTINGS"
                );
                assert_eq!(
                    DEFAULT_AUTO_SWITCH_AFTER_REFUSALS, auto_switch,
                    "defaults[{id}]"
                );
                *kinds.entry("defaults").or_default() += 1;
            }
            PureRow::ReadSettings {
                id,
                global,
                settings,
            } => {
                let got = read_concierge_settings(undefined_as_none(&global));
                assert_eq!(
                    serde_json::to_string(&got).unwrap(),
                    serde_json::to_string(&settings).unwrap(),
                    "readSettings[{id}]"
                );
                *kinds.entry("readSettings").or_default() += 1;
            }
            PureRow::LegacyMap {
                id,
                sources,
                migrated,
            } => {
                let got = map_legacy_concierge_settings(&LegacyConciergeSources {
                    dangerous_content_settings: sources.get("dangerousContentSettings"),
                    uncensored_image_description_profile_id: sources
                        .get("uncensoredImageDescriptionProfileId"),
                    cheap_llm_settings: sources.get("cheapLLMSettings"),
                    // v4 `sources.hasUnmoderatedChats === true`.
                    has_unmoderated_chats: sources.get("hasUnmoderatedChats")
                        == Some(&Value::Bool(true)),
                });
                assert_eq!(
                    serde_json::to_string(&got).unwrap(),
                    serde_json::to_string(&migrated).unwrap(),
                    "legacyMap[{id}] bytes"
                );
                *kinds.entry("legacyMap").or_default() += 1;
            }
            PureRow::WithSettingsLegacy {
                id,
                settings,
                has_unmoderated_chats,
                out,
                identical,
            } => {
                let got = with_concierge_settings_from_legacy(&settings, has_unmoderated_chats);
                assert_eq!(
                    serde_json::to_string(&got).unwrap(),
                    serde_json::to_string(&out).unwrap(),
                    "withSettingsLegacy[{id}] bytes"
                );
                assert_eq!(
                    got == settings,
                    identical,
                    "withSettingsLegacy[{id}] untouched"
                );
                *kinds.entry("withSettingsLegacy").or_default() += 1;
            }
            PureRow::Override {
                id,
                chat,
                state,
                provenance,
                reason,
                uncensored_route,
                danger_styling,
                classifier_on_duty,
                state_uses_uncensored_route,
                may_fail_over: want_may_fail_over,
                state_may_fail_over,
            } => {
                // v4's `null` and `undefined` chats are both "no chat" here.
                let chat = match chat {
                    Value::Null => None,
                    Value::String(s) if s == "<undefined>" => None,
                    other => Some(other),
                };
                let c = chat.as_ref();
                let got_state = get_concierge_state(c);
                assert_eq!(got_state.as_str(), state, "override[{id}] state");
                assert_eq!(
                    get_concierge_provenance(c).map(|p| p.as_str().to_string()),
                    provenance,
                    "override[{id}] provenance"
                );
                assert_eq!(get_concierge_reason(c), reason, "override[{id}] reason");
                assert_eq!(
                    should_use_uncensored_route(c),
                    uncensored_route,
                    "override[{id}] uncensoredRoute"
                );
                assert_eq!(
                    should_show_danger_styling(c),
                    danger_styling,
                    "override[{id}] dangerStyling"
                );
                assert_eq!(
                    is_classifier_on_duty(c),
                    classifier_on_duty,
                    "override[{id}] classifierOnDuty"
                );
                assert_eq!(
                    may_fail_over(c),
                    want_may_fail_over,
                    "override[{id}] mayFailOver"
                );
                // The state-only twins, asked exactly as v4 asks them — through
                // the derived state — and pinned to agree with the chat-shaped
                // predicates on this row.
                assert_eq!(
                    concierge_state_uses_uncensored_route(got_state),
                    state_uses_uncensored_route,
                    "override[{id}] stateUsesUncensoredRoute"
                );
                assert_eq!(
                    concierge_state_may_fail_over(got_state),
                    state_may_fail_over,
                    "override[{id}] stateMayFailOver"
                );
                assert_eq!(
                    state_uses_uncensored_route, uncensored_route,
                    "override[{id}] twin"
                );
                assert_eq!(
                    state_may_fail_over, want_may_fail_over,
                    "override[{id}] twin"
                );
                *kinds.entry("override").or_default() += 1;
            }
            PureRow::StateRoute {
                id,
                state,
                uses_uncensored_route,
                may_fail_over: want,
            } => {
                let parsed = ConciergeState::from_wire(&state)
                    .unwrap_or_else(|| panic!("stateRoute[{id}] unknown state {state}"));
                assert_eq!(
                    concierge_state_uses_uncensored_route(parsed),
                    uses_uncensored_route,
                    "stateRoute[{id}] usesUncensoredRoute"
                );
                assert_eq!(
                    concierge_state_may_fail_over(parsed),
                    want,
                    "stateRoute[{id}] mayFailOver"
                );
                *kinds.entry("stateRoute").or_default() += 1;
            }
            PureRow::States { id, states } => {
                let got: Vec<String> = CONCIERGE_STATES
                    .iter()
                    .map(|s| s.as_str().to_string())
                    .collect();
                assert_eq!(got, states, "states[{id}] CONCIERGE_STATES");
                *kinds.entry("states").or_default() += 1;
            }
            PureRow::Derive {
                id,
                legacy,
                columns,
            } => {
                let got = derive_concierge_mode_from_legacy(&legacy).to_json();
                assert_eq!(
                    serde_json::to_string(&got).unwrap(),
                    serde_json::to_string(&columns).unwrap(),
                    "derive[{id}]"
                );
                *kinds.entry("derive").or_default() += 1;
            }
            PureRow::WithLegacy {
                id,
                chat,
                out,
                identical,
            } => {
                let got = with_concierge_mode_from_legacy(chat.clone());
                // Byte comparison: v4's `{ ...chat, ...derived }` keeps a present
                // key in place and appends a new one — `Value` equality would
                // not see a moved key.
                assert_eq!(
                    serde_json::to_string(&got).unwrap(),
                    serde_json::to_string(&out).unwrap(),
                    "withLegacy[{id}] bytes"
                );
                assert_eq!(got == chat, identical, "withLegacy[{id}] untouched");
                *kinds.entry("withLegacy").or_default() += 1;
            }
            PureRow::ConfiguredDesk { id, global, desk } => {
                let got = resolve_configured_concierge_desk(undefined_as_none(&global));
                assert_eq!(
                    serde_json::to_string(&got).unwrap(),
                    serde_json::to_string(&desk).unwrap(),
                    "configuredDesk[{id}]"
                );
                *kinds.entry("configuredDesk").or_default() += 1;
            }
        }
        count += 1;
    }
    // Shape guard: an oracle regenerated before `3b463d6b1` cannot even load the
    // case (the new exports are absent), and one from a narrower case would
    // carry none of the new kinds and pass vacuously.
    let want: [(&str, usize); 11] = [
        ("defaults", 1),
        ("readSettings", 9),
        ("resolve", 32),
        ("legacyMap", 16),
        ("withSettingsLegacy", 6),
        ("override", 25),
        ("stateRoute", 3),
        ("states", 1),
        ("derive", 11),
        ("withLegacy", 5),
        // P4.D228: absent from an oracle regenerated before `ce2f1dabf`.
        ("configuredDesk", 6),
    ];
    for (kind, n) in want {
        assert_eq!(
            kinds.get(kind).copied().unwrap_or(0),
            n,
            "row count for kind {kind}"
        );
    }
    eprintln!("OK: danger resolver/Concierge table matched oracle ({count} rows).");
}

// --- section 2: manual flip (tier-2 chat-row dump + log lines) ---
//
// P4.D226 (v4 `4d370a90f`, #75): the three-state corpus — v4's own
// `manual-flip.test.ts` cases as planted rows — and every op's
// `ConciergeManualFlip` lines plus the repository's two DEBUGs compared in
// order against v4's recorded lines.

#[derive(Deserialize)]
struct FlipSpec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    chats: Vec<Value>,
    ops: Vec<FlipOp>,
    /// P4.D225: refusal-ledger values planted before the ops (both sides).
    #[serde(rename = "ledgerPlants", default)]
    ledger_plants: Vec<LedgerPlant>,
}

#[derive(Deserialize)]
struct LedgerPlant {
    #[serde(rename = "chatId")]
    chat_id: String,
    count: i64,
    #[serde(rename = "lastAt")]
    last_at: String,
}

#[derive(Deserialize)]
struct FlipOp {
    id: String,
    #[serde(rename = "chatId")]
    chat_id: String,
    requested: String,
    /// P4.D225 (v4 `49059fb14`): `applyConciergeFlip`'s fourth argument.
    #[serde(default)]
    options: Option<Value>,
    /// P4.D226: spread over the row read before the flip (a stale snapshot —
    /// the compare-and-set miss).
    #[serde(default)]
    snapshot: Option<Value>,
}

/// v4 `ApplyConciergeFlipOptions` off the spec's JSON.
fn flip_options(
    v: Option<&Value>,
) -> quilltap_core::services::dangerous_content::manual_flip::ApplyConciergeFlipOptions {
    use quilltap_core::services::concierge_notifications::{
        ConciergeAutoFlagDetails, ConciergeCategory, ConciergeDangerDetails,
    };
    use quilltap_core::services::dangerous_content::manual_flip::{
        ApplyConciergeFlipOptions, FlipBy, FlipReason,
    };
    let Some(v) = v else {
        return ApplyConciergeFlipOptions::default();
    };
    ApplyConciergeFlipOptions {
        by: match v.get("by").and_then(Value::as_str) {
            Some("concierge") => FlipBy::Concierge,
            _ => FlipBy::Operator,
        },
        reason: match v.get("reason").and_then(Value::as_str) {
            Some("manual") => Some(FlipReason::Manual),
            Some("refusals") => Some(FlipReason::Refusals),
            Some("classifier") => Some(FlipReason::Classifier),
            _ => None,
        },
        refusals: v.get("refusals").map(|r| ConciergeAutoFlagDetails {
            count: r["count"].as_i64().unwrap_or(0),
            last_provider: r["lastProvider"].as_str().unwrap_or("").to_string(),
            last_model: r
                .get("lastModel")
                .and_then(Value::as_str)
                .map(str::to_string),
        }),
        classification: v.get("classification").map(|c| ConciergeDangerDetails {
            score: c["score"].as_f64().unwrap(),
            threshold: c["threshold"].as_f64().unwrap(),
            categories: c["categories"]
                .as_array()
                .unwrap()
                .iter()
                .map(|k| ConciergeCategory {
                    category: k["category"].as_str().unwrap().to_string(),
                    score: k["score"].as_f64().unwrap(),
                    label: k.get("label").and_then(Value::as_str).map(str::to_string),
                })
                .collect(),
            source: c.get("source").and_then(Value::as_str).map(str::to_string),
            provider_name: c
                .get("providerName")
                .and_then(Value::as_str)
                .map(str::to_string),
        }),
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum FlipOracleRow {
    #[serde(rename = "op")]
    Op {
        id: String,
        #[serde(rename = "newState")]
        new_state: String,
        changed: bool,
        logs: Vec<Value>,
    },
    #[serde(rename = "table")]
    Table { table: String, rows: Vec<Value> },
}

fn state_from_str(s: &str) -> ConciergeState {
    ConciergeState::from_wire(s).unwrap_or_else(|| panic!("unknown ConciergeState {s}"))
}

const FLIP_SEED_TS: &str = "2020-01-01T00:00:00.000Z";

fn snake(k: &str) -> String {
    let mut out = String::new();
    for c in k.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// v4's recorded line in the capture rig's rendering: the `ConciergeManualFlip`
/// service logs under `quilltap::concierge_manual_flip`, the repository's root
/// lines under `quilltap::db`; a JSON `null` renders as the literal `null`.
fn render_v4(log: &Value) -> String {
    let target = match log["service"].as_str() {
        Some("ConciergeManualFlip") => "quilltap::concierge_manual_flip",
        None => "quilltap::db",
        Some(other) => panic!("unexpected service {other}"),
    };
    let level = log["level"].as_str().unwrap().to_uppercase();
    let mut line = format!("{level} {target} {}", log["message"].as_str().unwrap());
    for (k, v) in log["bag"].as_object().unwrap() {
        let rendered = match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        line.push_str(&format!(" {}={rendered}", snake(k)));
    }
    line
}

/// The Rust lines this family compares: every `concierge_manual_flip` line.
/// The repository's `Concierge state write` DEBUG (which v4's recorder also
/// captures) runs on v5's single-writer thread, out of a thread-scoped
/// capture's sight — it is pinned by `db::chats`'s own tests, calling the
/// repository directly, and filtered out of v4's lines below.
fn keep_rust_line(line: &str) -> bool {
    line.contains(" quilltap::concierge_manual_flip ")
}

/// Placeholder the minted `dangerClassifiedAt` (present-non-null → `<ts>`) so
/// a clear compares. `updatedAt` is bumped by the Concierge bubble's
/// `addMessage` on a changed flip — sentinel-aware, so a value equal to the
/// `2020` seed stays (proving a silent path posted nothing) and a mint
/// collapses to `<ts>`. `lastMessageAt` rides the same rule but is NOT
/// expected to move (bug 112 — a Concierge bubble is system-authored).
fn normalize_chat_rows(rows: &mut [Value]) {
    for row in rows.iter_mut() {
        if let Some(obj) = row.as_object_mut() {
            for col in ["updatedAt", "lastMessageAt", "dangerClassifiedAt"] {
                let minted = obj
                    .get(col)
                    .and_then(Value::as_str)
                    .map(|s| s != FLIP_SEED_TS && !s.starts_with("2026-09-20"))
                    .unwrap_or(false);
                if minted {
                    obj.insert(col.into(), Value::String("<ts>".into()));
                }
            }
        }
    }
}

/// The Concierge bubble's minted `id` + `createdAt` are placeholdered; every
/// other column is diffed exactly against v4's REAL writer.
fn normalize_message_rows(rows: &mut [Value]) {
    for row in rows.iter_mut() {
        if let Some(obj) = row.as_object_mut() {
            if obj.contains_key("id") {
                obj.insert("id".into(), Value::String("<id>".into()));
            }
            if obj.get("createdAt").map(|v| !v.is_null()) == Some(true) {
                obj.insert("createdAt".into(), Value::String("<ts>".into()));
            }
        }
    }
}

#[test]
fn danger_manual_flip_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_DANGER_MANUAL_FLIP") else {
        eprintln!("SKIP: set QT_ORACLE_DANGER_MANUAL_FLIP to the tier-2 NDJSON (see header).");
        return;
    };
    let Ok(fixture) = std::env::var("QT_FIXTURE_MANUAL_FLIP") else {
        eprintln!("SKIP: set QT_FIXTURE_MANUAL_FLIP to the seed fixture .db (see header).");
        return;
    };

    let spec_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/danger-manual-flip.json");
    let spec: FlipSpec = serde_json::from_str(
        &std::fs::read_to_string(&spec_path).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse flip spec");
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));

    // Parse oracle: op results (by id) + the chats + chat_messages table dumps.
    let mut want_ops: std::collections::HashMap<String, (String, bool, Vec<Value>)> =
        Default::default();
    let mut want_rows: Vec<Value> = Vec::new();
    let mut want_message_rows: Vec<Value> = Vec::new();
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<FlipOracleRow>(line).expect("parse flip oracle row") {
            FlipOracleRow::Op {
                id,
                new_state,
                changed,
                logs,
            } => {
                want_ops.insert(id, (new_state, changed, logs));
            }
            FlipOracleRow::Table { table, rows } => match table.as_str() {
                "chats" => want_rows = rows,
                "chat_messages" => want_message_rows = rows,
                other => panic!("unexpected oracle table {other}"),
            },
        }
    }
    assert_eq!(want_ops.len(), spec.ops.len(), "one oracle row per op");

    let work_dir = tempfile::Builder::new()
        .prefix("qt-danger-manual-flip-rust-")
        .tempdir()
        .expect("tempdir");
    let work = work_dir.path().join("danger-manual-flip-rust.db");
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let db = Db::open_main(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));

    // P4.D225: v5's boot ensure (a no-op on a fixture whose builder ran v4's
    // `add-chat-refusal-ledger-v1`), then the planted ledgers.
    let plants: Vec<(String, i64, String)> = spec
        .ledger_plants
        .iter()
        .map(|p| (p.chat_id.clone(), p.count, p.last_at.clone()))
        .collect();
    rt.block_on(db.write(move |w| {
        quilltap_core::test_support::ensure_p4d225_columns(w.main().connection());
        for (id, count, at) in &plants {
            w.main().connection().execute(
                "UPDATE chats SET \"moderationRefusalCount\" = ?1, \"lastModerationRefusalAt\" = ?2 WHERE id = ?3",
                rusqlite::params![count, at, id],
            )?;
        }
        Ok(())
    }))
    .expect("plant the ledgers");

    let mut lines_compared = 0usize;
    for op in &spec.ops {
        let mut chat = db
            .read_main(|c| chats_read::find_by_id(c, &op.chat_id))
            .unwrap_or_else(|e| panic!("read chat {}: {e:?}", op.chat_id))
            .unwrap_or_else(|| panic!("op {}: chat {} missing", op.id, op.chat_id));
        if let Some(snapshot) = op.snapshot.as_ref().and_then(Value::as_object) {
            let obj = chat.as_object_mut().unwrap();
            for (k, v) in snapshot {
                obj.insert(k.clone(), v.clone());
            }
        }
        let options = flip_options(op.options.as_ref());
        let (result, lines) = quilltap_core::test_support::captured_with(|| {
            rt.block_on(apply_concierge_flip_with(
                &db,
                &RealConciergeAnnouncer { db: &db },
                &op.chat_id,
                state_from_str(&op.requested),
                &chat,
                &options,
            ))
        });
        let result = result.unwrap_or_else(|e| panic!("flip {}: {e:?}", op.id));

        let (want_state, want_changed, want_logs) = want_ops
            .get(&op.id)
            .unwrap_or_else(|| panic!("oracle missing op {}", op.id));
        assert_eq!(
            result.new_state.as_str(),
            want_state,
            "op {} newState",
            op.id
        );
        assert_eq!(result.changed, *want_changed, "op {} changed", op.id);
        let got: Vec<String> = lines.into_iter().filter(|l| keep_rust_line(l)).collect();
        let want: Vec<String> = want_logs
            .iter()
            .filter(|l| l["service"].as_str() == Some("ConciergeManualFlip"))
            .map(render_v4)
            .collect();
        assert_eq!(got, want, "op {}: the flip's log lines", op.id);
        lines_compared += want.len();
    }
    assert!(
        lines_compared >= 25,
        "the log comparison ran vacuously ({lines_compared})"
    );

    let mut got_dump = db
        .read_main(|c| dump_table_json_conn(c, "chats", "id"))
        .expect("dump chats");
    let mut got_message_dump = db
        .read_main(|c| dump_table_json_conn(c, "chat_messages", "chatId"))
        .expect("dump chat_messages");
    drop(db);

    let mut got_rows: Vec<Value> = got_dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .expect("dump rows")
        .clone();
    normalize_chat_rows(&mut got_rows);
    normalize_chat_rows(&mut want_rows);
    let got_rows = got_rows.into_iter().map(canon).collect::<Vec<_>>();
    let want_rows = want_rows.into_iter().map(canon).collect::<Vec<_>>();
    assert_eq!(got_rows, want_rows, "chats rows diverge after manual flips");

    let mut got_msgs: Vec<Value> = got_message_dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .expect("dump message rows")
        .clone();
    normalize_message_rows(&mut got_msgs);
    normalize_message_rows(&mut want_message_rows);
    let got_msgs = got_msgs.into_iter().map(canon).collect::<Vec<_>>();
    let want_msgs = want_message_rows.into_iter().map(canon).collect::<Vec<_>>();
    assert_eq!(
        got_msgs, want_msgs,
        "chat_messages rows diverge after manual flips"
    );

    // v4 "the legacy column is never written": every chat's `conciergeOverride`
    // is exactly as seeded, on BOTH sides — and the corpus must carry seeded
    // legacy values for the claim to bite.
    assert_legacy_override_untouched(&spec, &got_rows);
    assert_legacy_override_untouched(&spec, &want_rows);

    eprintln!(
        "OK: danger manual-flip chats + chat_messages dumps + {lines_compared} log lines matched oracle ({} ops).",
        spec.ops.len()
    );
}

/// No flip writes `conciergeOverride` (v4 `4d370a90f`), and since `3b463d6b1`
/// (#76) the fresh DDL has no such column at all — v4 deleted it from both chat
/// schemas. The spec still SEEDS legacy values (the builder's `chats.create`
/// strips them as unknown keys, as a post-#76 v4 does with a stale bundle), so
/// the pin is now: neither dump carries the key. Fails loudly if the corpus
/// stops seeding them (the probe would be vacuous).
fn assert_legacy_override_untouched(spec: &FlipSpec, rows: &[Value]) {
    let seeded_legacy = spec
        .chats
        .iter()
        .filter(|c| c.get("conciergeOverride").is_some_and(|v| !v.is_null()))
        .count();
    for row in rows {
        assert!(
            row.get("conciergeOverride").is_none(),
            "chat {:?}: the fresh DDL has no conciergeOverride column (v4 `3b463d6b1`)",
            row.get("id")
        );
    }
    assert!(seeded_legacy >= 2, "the corpus must seed legacy overrides");
}
