//! Tier-1 differential test #3: recall-side targeting-tag multipliers.
//!
//! Mirrors the fixed corpus in harness/oracle/cases/recall-tags.ts and asserts
//! field-for-field equivalence against the captured oracle NDJSON: float
//! multipliers within 1e-12, the `fired` unicode debug labels EXACTLY (order +
//! bytes), the parsed enum tags exactly, and the `exclude` bool exactly.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/recall-tags.ts \
//!     > /tmp/oracle-recall-tags.ndjson
//! Run:
//!   QT_ORACLE_RECALL_TAGS=/tmp/oracle-recall-tags.ndjson \
//!     cargo test -p quilltap-harness --test recall_tags_equivalence
//!
//! P4.D267 (the recall multiplier retuning, v4 `f7d8064be`, pin `01a83539d`)
//! grew three DATA-DRIVEN row kinds whose inputs ride in the row: `gate`
//! (`boostGateThreshold` / `boostGateStrength`), `multT` (the seven fns under
//! an overridden table) and `combineT` (`combineRecallMultipliers` under a
//! tuning and / or a relevance). The tuning INPUT is validated + resolved by
//! v5's real `recall_tuning` here and by v4's real schema + resolver there.
//! The pre-existing rows' expected values moved ONLY by regeneration (fresh
//! 1.3 / 1.15; the 1.4 boost cap, which fires with no relevance).

// Data-driven case tables use multi-field tuples by design; the complexity lint
// is noise here.
#![allow(clippy::type_complexity)]

use std::collections::{HashMap, HashSet};

use quilltap_core::recall_tags::{
    boost_gate_strength, boost_gate_threshold, combine_recall_multipliers, context_multiplier,
    fresh_event_multiplier, occurred_within_multiplier, parse_targeting_tags,
    participant_multiplier, recently_whispered_multiplier, scope_project_multiplier,
    temporal_multiplier, BoostGate, ContextTag, MemoryTagView, RecallContext, RecallMultiplier,
    RecallRelevance, ScopePolicy, ScopeTag, TargetingTags, TemporalTag, TimeWindow,
    RECALL_MULTIPLIERS,
};
use quilltap_core::recall_tuning::{resolve_recall_tuning, validate_recall_tuning_input};
use serde::Deserialize;
use serde_json::Value;

const EPS: f64 = 1e-12;

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum OracleRow {
    #[serde(rename = "parse")]
    Parse {
        id: String,
        temporal: String,
        scope: String,
        context: String,
    },
    #[serde(rename = "combine")]
    Combine {
        id: String,
        multiplier: f64,
        fired: Vec<String>,
        exclude: bool,
    },
}

// The oracle's `mult` rows carry a `fn` discriminator (a Rust keyword), so we
// bind it via `#[serde(rename)]` in its own struct and key `mult` rows by the
// composite "<fn>/<id>".
#[derive(Deserialize)]
struct MultRaw {
    #[serde(rename = "fn")]
    func: String,
    id: String,
    multiplier: f64,
    fired: Vec<String>,
    #[serde(default)]
    exclude: bool,
}

struct Indexed {
    parse: HashMap<String, (String, String, String)>, // id -> (temporal, scope, context)
    mult: HashMap<String, MultRaw>,                   // "<fn>/<id>" -> row
    combine: HashMap<String, (f64, Vec<String>, bool)>, // id -> (multiplier, fired, exclude)
}

fn load(path: &str) -> Indexed {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let mut parse = HashMap::new();
    let mut mult = HashMap::new();
    let mut combine = HashMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        match v.get("kind").and_then(|k| k.as_str()) {
            Some("parse") => {
                if let OracleRow::Parse {
                    id,
                    temporal,
                    scope,
                    context,
                } = serde_json::from_value(v).unwrap()
                {
                    parse.insert(id, (temporal, scope, context));
                }
            }
            Some("mult") => {
                let row: MultRaw = serde_json::from_value(v).unwrap();
                mult.insert(format!("{}/{}", row.func, row.id), row);
            }
            Some("combine") => {
                if let OracleRow::Combine {
                    id,
                    multiplier,
                    fired,
                    exclude,
                } = serde_json::from_value(v).unwrap()
                {
                    combine.insert(id, (multiplier, fired, exclude));
                }
            }
            // P4.D267's data-driven kinds — `retuning_rows_match_oracle`.
            Some("gate" | "multT" | "combineT") => {}
            other => panic!("unexpected oracle row kind: {other:?}"),
        }
    }
    Indexed {
        parse,
        mult,
        combine,
    }
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

fn tags(temporal: TemporalTag, scope: ScopeTag, context: ContextTag) -> TargetingTags {
    TargetingTags {
        temporal,
        scope,
        context,
    }
}

fn assert_mult(idx: &Indexed, key: &str, got: &RecallMultiplier) {
    let want = idx
        .mult
        .get(key)
        .unwrap_or_else(|| panic!("oracle missing mult '{key}'"));
    assert!(
        (got.multiplier - want.multiplier).abs() <= EPS,
        "mult '{key}': rust={} oracle={}",
        got.multiplier,
        want.multiplier
    );
    let got_fired: Vec<String> = got.fired.iter().map(|x| x.to_string()).collect();
    assert_eq!(got_fired, want.fired, "mult '{key}' fired");
    assert_eq!(got.exclude, want.exclude, "mult '{key}' exclude");
}

#[test]
fn recall_tags_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_RECALL_TAGS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_RECALL_TAGS to the oracle NDJSON (see test header).");
            return;
        }
    };
    let idx = load(&path);

    // ---- parseTargetingTags ----
    let parse_cases: Vec<(&str, Vec<&str>)> = vec![
        ("empty", vec![]),
        ("temporal-past", vec!["past"]),
        ("temporal-future", vec!["future"]),
        ("scope-narrow", vec!["scope: narrow"]),
        ("scope-narrow-nospace", vec!["scope:narrow"]),
        ("scope-wide", vec!["scope: wide"]),
        ("context-history", vec!["history"]),
        ("context-banter", vec!["banter"]),
        ("all-three", vec!["past", "scope: narrow", "philosophy"]),
        (
            "case-insensitive",
            vec!["PAST", "SCOPE: NARROW", "Philosophy"],
        ),
        ("whitespace", vec!["  moment  ", " scope:  wide "]),
        ("last-wins-temporal", vec!["past", "present"]),
        ("last-wins-scope", vec!["scope: narrow", "scope: wide"]),
        ("unknown-scope", vec!["scope: sideways"]),
        ("unknown-word", vec!["banana"]),
        ("free-then-real-context", vec!["information", "philosophy"]),
    ];
    for (id, kws) in parse_cases {
        let kw: Vec<String> = s(&kws);
        let got = parse_targeting_tags(&kw);
        let (wt, ws, wc) = idx
            .parse
            .get(id)
            .unwrap_or_else(|| panic!("oracle missing parse '{id}'"));
        assert_eq!(got.temporal.as_str(), wt, "parse '{id}' temporal");
        assert_eq!(got.scope.as_str(), ws, "parse '{id}' scope");
        assert_eq!(got.context.as_str(), wc, "parse '{id}' context");
    }

    // ---- scopeProjectMultiplier ----
    let w = tags(
        TemporalTag::Present,
        ScopeTag::Wide,
        ContextTag::Information,
    );
    let n = tags(
        TemporalTag::Present,
        ScopeTag::Narrow,
        ContextTag::Information,
    );
    let scope_cases: Vec<(&str, TargetingTags, Option<&str>, Option<&str>, ScopePolicy)> = vec![
        (
            "wide-passthrough",
            w,
            Some("P1"),
            Some("P1"),
            ScopePolicy::DownWeight,
        ),
        (
            "narrow-no-memproj",
            n,
            None,
            Some("P1"),
            ScopePolicy::DownWeight,
        ),
        (
            "narrow-same-project",
            n,
            Some("P1"),
            Some("P1"),
            ScopePolicy::DownWeight,
        ),
        (
            "narrow-cross-downweight",
            n,
            Some("P1"),
            Some("P2"),
            ScopePolicy::DownWeight,
        ),
        (
            "narrow-cross-exclude",
            n,
            Some("P1"),
            Some("P2"),
            ScopePolicy::Exclude,
        ),
        (
            "narrow-memproj-no-current",
            n,
            Some("P1"),
            None,
            ScopePolicy::DownWeight,
        ),
        (
            "narrow-memproj-no-current-exclude",
            n,
            Some("P1"),
            None,
            ScopePolicy::Exclude,
        ),
    ];
    for (id, t, mp, cp, pol) in scope_cases {
        let got = scope_project_multiplier(t, mp, cp, pol, &RECALL_MULTIPLIERS);
        assert_mult(&idx, &format!("scope/{id}"), &got);
    }

    // ---- temporalMultiplier ----
    // The oracle's bare-call rows pin the default arg (retrospective = false).
    for t in [
        TemporalTag::Past,
        TemporalTag::Moment,
        TemporalTag::Present,
        TemporalTag::Future,
    ] {
        let got = temporal_multiplier(
            tags(t, ScopeTag::Wide, ContextTag::Information),
            false,
            &RECALL_MULTIPLIERS,
        );
        assert_mult(&idx, &format!("temporal/{}", t.as_str()), &got);
    }
    // Episodic overhaul: the explicit retrospective arms.
    for t in [
        TemporalTag::Past,
        TemporalTag::Moment,
        TemporalTag::Present,
        TemporalTag::Future,
    ] {
        for retro in [false, true] {
            let got = temporal_multiplier(
                tags(t, ScopeTag::Wide, ContextTag::Information),
                retro,
                &RECALL_MULTIPLIERS,
            );
            assert_mult(
                &idx,
                &format!("temporal-retro/{}-{retro}", t.as_str()),
                &got,
            );
        }
    }

    // ---- occurredWithinMultiplier ----
    let window = TimeWindow {
        from: "2026-07-13T00:00:00.000Z".to_string(),
        to: "2026-07-19T23:59:59.999Z".to_string(),
    };
    let window_cases: Vec<(&str, Option<&str>, Option<&str>, Option<TimeWindow>)> = vec![
        (
            "inside",
            Some("2026-07-14T00:00:00.000Z"),
            Some("2026-07-01T00:00:00.000Z"),
            Some(window.clone()),
        ),
        (
            "outside",
            Some("2026-01-01T00:00:00.000Z"),
            Some("2026-01-01T00:00:00.000Z"),
            Some(window.clone()),
        ),
        (
            "edge-from",
            Some("2026-07-13T00:00:00.000Z"),
            None,
            Some(window.clone()),
        ),
        (
            "edge-to",
            Some("2026-07-19T23:59:59.999Z"),
            None,
            Some(window.clone()),
        ),
        (
            "created-fallback",
            None,
            Some("2026-07-14T12:00:00.000Z"),
            Some(window.clone()),
        ),
        (
            "empty-occurred-not-nullish",
            Some(""),
            Some("2026-07-14T12:00:00.000Z"),
            Some(window.clone()),
        ),
        (
            "unparsable-event",
            Some("the third night at sea"),
            Some("2026-07-14T12:00:00.000Z"),
            Some(window.clone()),
        ),
        ("no-event-time", None, None, Some(window.clone())),
        (
            "unparsable-window-from",
            Some("2026-07-14T00:00:00.000Z"),
            None,
            Some(TimeWindow {
                from: "whenever".to_string(),
                to: window.to.clone(),
            }),
        ),
        (
            "unparsable-window-to",
            Some("2026-07-14T00:00:00.000Z"),
            None,
            Some(TimeWindow {
                from: window.from.clone(),
                to: "whenever".to_string(),
            }),
        ),
        ("null-window", Some("2026-07-14T00:00:00.000Z"), None, None),
        (
            "undefined-window",
            Some("2026-07-14T00:00:00.000Z"),
            None,
            None,
        ),
    ];
    for (id, occurred, created, w) in &window_cases {
        let mem = MemoryTagView {
            occurred_at: *occurred,
            created_at: *created,
            ..Default::default()
        };
        let got = occurred_within_multiplier(&mem, w.as_ref(), &RECALL_MULTIPLIERS);
        assert_mult(&idx, &format!("window/{id}"), &got);
    }

    // ---- freshEventMultiplier (P4.d26) ----
    // 2026-07-29T02:44:00.000Z, the diagnosed chat's instant, and the corpus's
    // `FRESH_NOW`. `ago(h)` mirrors the oracle's `freshAt(msAgo)`.
    const FRESH_NOW: f64 = 1_785_293_040_000.0;
    const HOUR: f64 = 3_600_000.0;
    const FRESH_CHAT: &str = "chat-current";
    let ago =
        |hours: f64| quilltap_core::clock::iso_from_unix_ms((FRESH_NOW - hours * HOUR) as i64);
    // (id, occurredAt, createdAt, memory chatId, nowMs, currentChatId)
    let fresh_cases: Vec<(
        &str,
        Option<String>,
        Option<String>,
        Option<&str>,
        Option<f64>,
        Option<&str>,
    )> = vec![
        (
            "inside-24h",
            Some(ago(6.0)),
            None,
            None,
            Some(FRESH_NOW),
            Some(FRESH_CHAT),
        ),
        (
            "edge-24h-inclusive",
            Some(ago(24.0)),
            None,
            None,
            Some(FRESH_NOW),
            Some(FRESH_CHAT),
        ),
        (
            "just-past-24h",
            Some(quilltap_core::clock::iso_from_unix_ms(
                (FRESH_NOW - 24.0 * HOUR - 1.0) as i64,
            )),
            None,
            None,
            Some(FRESH_NOW),
            Some(FRESH_CHAT),
        ),
        (
            "inside-48h",
            Some(ago(30.0)),
            None,
            None,
            Some(FRESH_NOW),
            Some(FRESH_CHAT),
        ),
        (
            "edge-48h-inclusive",
            Some(ago(48.0)),
            None,
            None,
            Some(FRESH_NOW),
            Some(FRESH_CHAT),
        ),
        (
            "just-past-48h",
            Some(quilltap_core::clock::iso_from_unix_ms(
                (FRESH_NOW - 48.0 * HOUR - 1.0) as i64,
            )),
            None,
            None,
            Some(FRESH_NOW),
            Some(FRESH_CHAT),
        ),
        (
            "older-than-48h",
            Some(ago(49.0)),
            None,
            None,
            Some(FRESH_NOW),
            Some(FRESH_CHAT),
        ),
        (
            "created-fallback",
            None,
            Some(ago(2.0)),
            None,
            Some(FRESH_NOW),
            Some(FRESH_CHAT),
        ),
        (
            "created-fallback-null-occurred",
            None,
            Some(ago(2.0)),
            None,
            Some(FRESH_NOW),
            Some(FRESH_CHAT),
        ),
        (
            "empty-occurred-not-nullish",
            Some(String::new()),
            Some(ago(2.0)),
            None,
            Some(FRESH_NOW),
            Some(FRESH_CHAT),
        ),
        (
            "no-clock-undefined",
            Some(ago(1.0)),
            None,
            None,
            None,
            Some(FRESH_CHAT),
        ),
        (
            "no-clock-null",
            Some(ago(1.0)),
            None,
            None,
            None,
            Some(FRESH_CHAT),
        ),
        (
            "no-clock-nan",
            Some(ago(1.0)),
            None,
            None,
            Some(f64::NAN),
            Some(FRESH_CHAT),
        ),
        (
            "no-event-time",
            None,
            None,
            None,
            Some(FRESH_NOW),
            Some(FRESH_CHAT),
        ),
        (
            "unparsable-event",
            Some("not a date".to_string()),
            None,
            None,
            Some(FRESH_NOW),
            Some(FRESH_CHAT),
        ),
        (
            "future-event",
            Some(quilltap_core::clock::iso_from_unix_ms(
                (FRESH_NOW + HOUR) as i64,
            )),
            None,
            None,
            Some(FRESH_NOW),
            Some(FRESH_CHAT),
        ),
        (
            "echo-same-chat",
            Some(ago(1.0)),
            None,
            Some(FRESH_CHAT),
            Some(FRESH_NOW),
            Some(FRESH_CHAT),
        ),
        (
            "echo-other-chat",
            Some(ago(1.0)),
            None,
            Some("chat-other"),
            Some(FRESH_NOW),
            Some(FRESH_CHAT),
        ),
        (
            "echo-null-chat",
            Some(ago(1.0)),
            None,
            None,
            Some(FRESH_NOW),
            Some(FRESH_CHAT),
        ),
        (
            "echo-empty-memory-chat",
            Some(ago(1.0)),
            None,
            Some(""),
            Some(FRESH_NOW),
            Some(""),
        ),
        (
            "echo-empty-current-chat",
            Some(ago(1.0)),
            None,
            Some("chat-other"),
            Some(FRESH_NOW),
            Some(""),
        ),
        (
            "echo-no-current-chat",
            Some(ago(1.0)),
            None,
            Some(FRESH_CHAT),
            Some(FRESH_NOW),
            None,
        ),
        (
            "echo-undefined-current-chat",
            Some(ago(1.0)),
            None,
            Some(FRESH_CHAT),
            Some(FRESH_NOW),
            None,
        ),
        (
            "no-clock-other-chat",
            Some(ago(1.0)),
            None,
            Some("chat-other"),
            None,
            Some(FRESH_CHAT),
        ),
    ];
    for (id, occurred, created, chat, now, current_chat) in &fresh_cases {
        let mem = MemoryTagView {
            occurred_at: occurred.as_deref(),
            created_at: created.as_deref(),
            chat_id: *chat,
            ..Default::default()
        };
        let got = fresh_event_multiplier(&mem, *now, *current_chat, &RECALL_MULTIPLIERS);
        assert_mult(&idx, &format!("fresh/{id}"), &got);
    }

    // ---- contextMultiplier ----
    let ctx_cases: Vec<(&str, ContextTag, Option<ContextTag>)> = vec![
        (
            "match",
            ContextTag::Philosophy,
            Some(ContextTag::Philosophy),
        ),
        ("no-match", ContextTag::Philosophy, Some(ContextTag::Banter)),
        ("null-turn", ContextTag::Philosophy, None),
    ];
    for (id, mem_ctx, turn) in ctx_cases {
        let got = context_multiplier(
            tags(TemporalTag::Present, ScopeTag::Wide, mem_ctx),
            turn,
            &RECALL_MULTIPLIERS,
        );
        assert_mult(&idx, &format!("context/{id}"), &got);
    }

    // ---- participantMultiplier ----
    let part_cases: Vec<(&str, Option<&str>, Vec<&str>)> = vec![
        ("present", Some("C1"), vec!["C1", "C2"]),
        ("absent", Some("C3"), vec!["C1"]),
        ("no-about", None, vec!["C1"]),
        ("empty-present", Some("C1"), vec![]),
    ];
    for (id, about, present) in part_cases {
        let present: Vec<String> = s(&present);
        let mem = MemoryTagView {
            about_character_id: about,
            ..Default::default()
        };
        let got = participant_multiplier(&mem, &present, &RECALL_MULTIPLIERS);
        assert_mult(&idx, &format!("participant/{id}"), &got);
    }

    // ---- recentlyWhisperedMultiplier ----
    let recent_cases: Vec<(&str, Option<&str>, Vec<&str>)> = vec![
        ("whispered", Some("M1"), vec!["M1"]),
        ("not", Some("M2"), vec!["M1"]),
        ("no-id", None, vec!["M1"]),
        ("empty-set", Some("M1"), vec![]),
    ];
    for (id, mem_id, recent) in recent_cases {
        let set: HashSet<String> = recent.iter().map(|x| x.to_string()).collect();
        let mem = MemoryTagView {
            id: mem_id,
            ..Default::default()
        };
        let got = recently_whispered_multiplier(&mem, Some(&set), false, &RECALL_MULTIPLIERS);
        assert_mult(&idx, &format!("recent/{id}"), &got);
    }
    // Episodic overhaul: the suspended (retrospective re-ask) arm.
    for (id, suspended) in [
        ("whispered-suspended", true),
        ("whispered-not-suspended", false),
    ] {
        let set: HashSet<String> = ["M1".to_string()].into_iter().collect();
        let mem = MemoryTagView {
            id: Some("M1"),
            ..Default::default()
        };
        let got = recently_whispered_multiplier(&mem, Some(&set), suspended, &RECALL_MULTIPLIERS);
        assert_mult(&idx, &format!("recent/{id}"), &got);
    }

    // ---- combineRecallMultipliers ----
    // Each case: (id, keywords, projectId, aboutCharacterId,
    //             currentProjectId, scopePolicy, turnContext, present[], recent[])
    #[derive(Default)]
    struct CC {
        id: &'static str,     // oracle row key (case label)
        mem_id: &'static str, // the memory's own id (matters for anti-repetition)
        keywords: Vec<&'static str>,
        project_id: Option<&'static str>,
        about: Option<&'static str>,
        /// Owned, because the fresh arms compute their event times from FRESH_NOW.
        occurred_at: Option<String>,
        created_at: Option<String>,
        chat_id: Option<&'static str>,
        current_project_id: Option<&'static str>,
        policy: ScopePolicy,
        turn_context: Option<ContextTag>,
        turn_temporal: Option<TemporalTag>,
        turn_retrospective: bool,
        occurred_within: Option<(String, String)>,
        present: Vec<&'static str>,
        recent: Option<Vec<&'static str>>,
        current_chat_id: Option<&'static str>,
        now_ms: Option<f64>,
    }
    let combine_cases = vec![
        CC {
            id: "plain",
            mem_id: "M1",
            keywords: vec![],
            project_id: None,
            about: None,
            current_project_id: None,
            policy: ScopePolicy::DownWeight,
            turn_context: None,
            present: vec![],
            recent: None,
            ..Default::default()
        },
        CC {
            id: "exclude-shortcircuit",
            mem_id: "M1",
            keywords: vec!["scope: narrow"],
            project_id: Some("P1"),
            about: None,
            current_project_id: Some("P2"),
            policy: ScopePolicy::Exclude,
            turn_context: None,
            present: vec![],
            recent: None,
            ..Default::default()
        },
        CC {
            id: "stacked-boosts",
            mem_id: "M1",
            keywords: vec!["scope: narrow", "philosophy"],
            project_id: Some("P1"),
            about: Some("C1"),
            current_project_id: Some("P1"),
            policy: ScopePolicy::DownWeight,
            turn_context: Some(ContextTag::Philosophy),
            present: vec!["C1"],
            recent: None,
            ..Default::default()
        },
        CC {
            id: "stacked-penalties",
            mem_id: "M1",
            keywords: vec!["scope: narrow", "past"],
            project_id: Some("P1"),
            about: Some("C9"),
            current_project_id: Some("P2"),
            policy: ScopePolicy::DownWeight,
            turn_context: None,
            present: vec![],
            recent: Some(vec!["M1"]),
            ..Default::default()
        },
        CC {
            id: "mixed",
            mem_id: "M2",
            keywords: vec!["moment"],
            project_id: None,
            about: Some("C1"),
            current_project_id: Some("P1"),
            policy: ScopePolicy::DownWeight,
            turn_context: Some(ContextTag::Banter),
            present: vec!["C1"],
            recent: Some(vec![]),
            ..Default::default()
        },
        // ---- episodic overhaul arms (P4.d13) ----
        CC {
            id: "retro-flip-past",
            mem_id: "M1",
            keywords: vec!["past", "scope: wide", "history"],
            occurred_at: Some("2026-07-14T00:00:00.000Z".to_string()),
            created_at: Some("2026-07-14T01:00:00.000Z".to_string()),
            turn_retrospective: true,
            ..Default::default()
        },
        CC {
            id: "retro-flip-moment",
            mem_id: "M1",
            keywords: vec!["moment"],
            turn_retrospective: true,
            ..Default::default()
        },
        CC {
            id: "retro-suspends-whisper",
            mem_id: "M1",
            keywords: vec!["past"],
            occurred_at: Some("2026-07-14T00:00:00.000Z".to_string()),
            created_at: Some("2026-07-14T01:00:00.000Z".to_string()),
            turn_retrospective: true,
            recent: Some(vec!["M1"]),
            occurred_within: Some((
                "2026-07-13T00:00:00.000Z".to_string(),
                "2026-07-19T23:59:59.999Z".to_string(),
            )),
            ..Default::default()
        },
        CC {
            id: "window-in",
            mem_id: "M1",
            keywords: vec![],
            occurred_at: Some("2026-07-14T00:00:00.000Z".to_string()),
            created_at: Some("2026-07-01T00:00:00.000Z".to_string()),
            occurred_within: Some((
                "2026-07-13T00:00:00.000Z".to_string(),
                "2026-07-19T23:59:59.999Z".to_string(),
            )),
            ..Default::default()
        },
        CC {
            id: "window-out",
            mem_id: "M1",
            keywords: vec![],
            occurred_at: Some("2026-01-01T00:00:00.000Z".to_string()),
            created_at: Some("2026-01-01T00:00:00.000Z".to_string()),
            occurred_within: Some((
                "2026-07-13T00:00:00.000Z".to_string(),
                "2026-07-19T23:59:59.999Z".to_string(),
            )),
            ..Default::default()
        },
        CC {
            id: "window-created-fallback-unparsable-occurred",
            mem_id: "M1",
            keywords: vec![],
            occurred_at: Some("the third night at sea".to_string()),
            created_at: Some("2026-07-14T12:00:00.000Z".to_string()),
            occurred_within: Some((
                "2026-07-13T00:00:00.000Z".to_string(),
                "2026-07-19T23:59:59.999Z".to_string(),
            )),
            ..Default::default()
        },
        CC {
            id: "retro-window-whisper-stack",
            mem_id: "M1",
            keywords: vec!["scope: narrow", "past", "history"],
            project_id: Some("P1"),
            about: Some("C1"),
            occurred_at: Some("2026-07-15T09:00:00.000Z".to_string()),
            created_at: Some("2026-07-15T10:00:00.000Z".to_string()),
            current_project_id: Some("P1"),
            turn_context: Some(ContextTag::History),
            turn_temporal: Some(TemporalTag::Past),
            turn_retrospective: true,
            present: vec!["C1"],
            recent: Some(vec!["M1"]),
            occurred_within: Some((
                "2026-07-13T00:00:00.000Z".to_string(),
                "2026-07-19T23:59:59.999Z".to_string(),
            )),
            ..Default::default()
        },
        // ---- fresh-event boost arms (P4.d26) ----
        CC {
            id: "fresh-with-moment-demotion",
            mem_id: "M1",
            keywords: vec!["moment", "scope: narrow", "history"],
            project_id: Some("P1"),
            chat_id: Some("chat-other"),
            occurred_at: Some(ago(6.0)),
            current_project_id: Some("P1"),
            current_chat_id: Some(FRESH_CHAT),
            now_ms: Some(FRESH_NOW),
            ..Default::default()
        },
        CC {
            id: "fresh-inert-without-clock",
            mem_id: "M1",
            keywords: vec![],
            project_id: Some("P1"),
            chat_id: Some("chat-other"),
            occurred_at: Some(ago(6.0)),
            current_project_id: Some("P1"),
            ..Default::default()
        },
        CC {
            id: "fresh-suppressed-by-echo-guard",
            mem_id: "M1",
            keywords: vec![],
            chat_id: Some(FRESH_CHAT),
            occurred_at: Some(ago(1.0)),
            current_chat_id: Some(FRESH_CHAT),
            now_ms: Some(FRESH_NOW),
            ..Default::default()
        },
        CC {
            id: "fresh48-band-in-combine",
            mem_id: "M1",
            keywords: vec!["past"],
            chat_id: Some("chat-other"),
            occurred_at: Some(ago(30.0)),
            current_chat_id: Some(FRESH_CHAT),
            now_ms: Some(FRESH_NOW),
            ..Default::default()
        },
        CC {
            id: "maximal-fresh-window-retro-stack",
            mem_id: "M1",
            keywords: vec!["past", "scope: narrow", "history"],
            project_id: Some("P1"),
            about: Some("C1"),
            chat_id: Some("chat-other"),
            occurred_at: Some(ago(3.0)),
            current_project_id: Some("P1"),
            current_chat_id: Some(FRESH_CHAT),
            now_ms: Some(FRESH_NOW),
            turn_retrospective: true,
            turn_context: Some(ContextTag::History),
            present: vec!["C1"],
            occurred_within: Some((ago(24.0), ago(0.0))),
            ..Default::default()
        },
        CC {
            id: "fresh-with-cross-project-penalty",
            mem_id: "M1",
            keywords: vec!["scope: narrow"],
            project_id: Some("P1"),
            chat_id: Some("chat-other"),
            occurred_at: Some(ago(1.0)),
            current_project_id: Some("P2"),
            current_chat_id: Some(FRESH_CHAT),
            now_ms: Some(FRESH_NOW),
            ..Default::default()
        },
        CC {
            id: "inert-context-unchanged",
            mem_id: "M1",
            keywords: vec!["past"],
            occurred_at: Some("2026-07-14T00:00:00.000Z".to_string()),
            created_at: Some("2026-07-14T01:00:00.000Z".to_string()),
            recent: Some(vec!["M1"]),
            ..Default::default()
        },
    ];
    for cc in &combine_cases {
        let keywords: Vec<String> = s(&cc.keywords);
        let present: Vec<String> = s(&cc.present);
        let recent_set: Option<HashSet<String>> = cc
            .recent
            .as_ref()
            .map(|r| r.iter().map(|x| x.to_string()).collect());
        let window = cc.occurred_within.as_ref().map(|(from, to)| TimeWindow {
            from: from.clone(),
            to: to.clone(),
        });
        let mem = MemoryTagView {
            id: Some(cc.mem_id),
            project_id: cc.project_id,
            keywords: &keywords,
            about_character_id: cc.about,
            occurred_at: cc.occurred_at.as_deref(),
            created_at: cc.created_at.as_deref(),
            chat_id: cc.chat_id,
        };
        let ctx = RecallContext {
            current_project_id: cc.current_project_id,
            scope_policy: cc.policy,
            present_about_character_ids: &present,
            turn_context: cc.turn_context,
            turn_temporal: cc.turn_temporal,
            turn_retrospective: cc.turn_retrospective,
            occurred_within: window.as_ref(),
            expand_related: false,
            recently_whispered_ids: recent_set.as_ref(),
            current_chat_id: cc.current_chat_id,
            now_ms: cc.now_ms,
            tuning: None,
            present_participant_names: &[],
        };
        let got = combine_recall_multipliers(&mem, &ctx, None);
        let (wm, wf, we) = idx
            .combine
            .get(cc.id)
            .unwrap_or_else(|| panic!("oracle missing combine '{}'", cc.id));
        assert!(
            (got.multiplier - wm).abs() <= EPS,
            "combine '{}': rust={} oracle={}",
            cc.id,
            got.multiplier,
            wm
        );
        let got_fired: Vec<String> = got.fired.iter().map(|x| x.to_string()).collect();
        assert_eq!(&got_fired, wf, "combine '{}' fired", cc.id);
        assert_eq!(got.exclude, *we, "combine '{}' exclude", cc.id);
    }

    eprintln!(
        "OK: recall-tags case matched oracle ({} parse, {} mult, {} combine).",
        idx.parse.len(),
        idx.mult.len(),
        idx.combine.len()
    );
}

// ---------------------------------------------------------------------------
// P4.D267 — the data-driven retuning rows.
// ---------------------------------------------------------------------------

fn opt_str(v: &Value) -> Option<&str> {
    v.as_str()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn context_tag(v: &Value) -> Option<ContextTag> {
    v.as_str().and_then(ContextTag::from_kw)
}

fn temporal_tag(v: &Value) -> Option<TemporalTag> {
    v.as_str().and_then(TemporalTag::from_kw)
}

/// The owned pieces a row's `ctx` borrows into a [`RecallContext`].
struct CtxParts {
    present: Vec<String>,
    whispered: Option<HashSet<String>>,
    window: Option<TimeWindow>,
    tuning: Option<quilltap_core::recall_tuning::ResolvedRecallTuning>,
}

fn ctx_parts(ctx: &Value, tuning_input: Option<&Value>) -> CtxParts {
    CtxParts {
        present: strings(&ctx["presentAboutCharacterIds"]),
        whispered: ctx
            .get("recentlyWhisperedIds")
            .map(|v| strings(v).into_iter().collect()),
        window: ctx.get("occurredWithin").and_then(|w| {
            Some(TimeWindow {
                from: w["from"].as_str()?.to_string(),
                to: w["to"].as_str()?.to_string(),
            })
        }),
        tuning: tuning_input.map(|raw| {
            let (input, _) = validate_recall_tuning_input(raw)
                .unwrap_or_else(|e| panic!("rust refused a v4-valid tuning input: {e:?}"));
            resolve_recall_tuning(Some(&input))
        }),
    }
}

fn recall_ctx<'a>(ctx: &'a Value, parts: &'a CtxParts) -> RecallContext<'a> {
    RecallContext {
        current_project_id: opt_str(&ctx["currentProjectId"]),
        scope_policy: if ctx["scopePolicy"] == "exclude" {
            ScopePolicy::Exclude
        } else {
            ScopePolicy::DownWeight
        },
        present_about_character_ids: &parts.present,
        turn_context: context_tag(&ctx["turnContext"]),
        turn_temporal: temporal_tag(&ctx["turnTemporal"]),
        turn_retrospective: ctx["turnRetrospective"] == true,
        occurred_within: parts.window.as_ref(),
        expand_related: false,
        recently_whispered_ids: parts.whispered.as_ref(),
        current_chat_id: opt_str(&ctx["currentChatId"]),
        now_ms: ctx["nowMs"].as_f64(),
        tuning: parts.tuning.as_ref(),
        present_participant_names: &[],
    }
}

fn tag_view<'a>(mem: &'a Value, keywords: &'a [String]) -> MemoryTagView<'a> {
    MemoryTagView {
        id: opt_str(&mem["id"]),
        project_id: opt_str(&mem["projectId"]),
        keywords,
        about_character_id: opt_str(&mem["aboutCharacterId"]),
        occurred_at: opt_str(&mem["occurredAt"]),
        created_at: opt_str(&mem["createdAt"]),
        chat_id: opt_str(&mem["chatId"]),
    }
}

fn want_fired(row: &Value) -> Vec<String> {
    strings(&row["fired"])
}

#[test]
fn retuning_rows_match_oracle() {
    let path = match std::env::var("QT_ORACLE_RECALL_TAGS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_RECALL_TAGS to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let (mut n_gate, mut n_mult, mut n_combine) = (0, 0, 0);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let id = row["id"].as_str().unwrap_or("");
        match row["kind"].as_str() {
            Some("gate") => {
                let g = &row["gate"];
                let gate = BoostGate {
                    abs: g["abs"].as_f64(),
                    margin: g["margin"].as_f64(),
                    ramp: g["ramp"].as_f64().unwrap(),
                };
                let best = row["best"].as_f64().unwrap();
                let cosine = row["cosine"].as_f64().unwrap();
                let t = boost_gate_threshold(&gate, best);
                let st = boost_gate_strength(&gate, cosine, best);
                let wt = row["threshold"].as_f64().unwrap();
                assert!(
                    (t - wt).abs() <= EPS,
                    "gate '{id}' threshold: rust={t} oracle={wt}"
                );
                let ws = row["strength"].as_f64().unwrap();
                assert!(
                    (st - ws).abs() <= EPS,
                    "gate '{id}' strength: rust={st} oracle={ws}"
                );
                n_gate += 1;
            }
            Some("multT") => {
                let keywords = strings(&row["memory"]["keywords"]);
                let mem = tag_view(&row["memory"], &keywords);
                let parts = ctx_parts(&row["ctx"], row.get("tuningInput"));
                let ctx = recall_ctx(&row["ctx"], &parts);
                let m = &parts.tuning.as_ref().unwrap().multipliers;
                let t = parse_targeting_tags(&keywords);
                let got = match row["fn"].as_str().unwrap() {
                    "scope" => scope_project_multiplier(
                        t,
                        mem.project_id,
                        ctx.current_project_id,
                        ctx.scope_policy,
                        m,
                    ),
                    "temporal" => temporal_multiplier(t, ctx.turn_retrospective, m),
                    "window" => occurred_within_multiplier(&mem, ctx.occurred_within, m),
                    "fresh" => fresh_event_multiplier(&mem, ctx.now_ms, ctx.current_chat_id, m),
                    "context" => context_multiplier(t, ctx.turn_context, m),
                    "participant" => {
                        participant_multiplier(&mem, ctx.present_about_character_ids, m)
                    }
                    _ => recently_whispered_multiplier(
                        &mem,
                        ctx.recently_whispered_ids,
                        ctx.turn_retrospective,
                        m,
                    ),
                };
                let wm = row["multiplier"].as_f64().unwrap();
                assert!(
                    (got.multiplier - wm).abs() <= EPS,
                    "multT '{id}': rust={} oracle={wm}",
                    got.multiplier
                );
                let fired: Vec<String> = got.fired.iter().map(|f| f.to_string()).collect();
                assert_eq!(fired, want_fired(&row), "multT '{id}' fired");
                assert_eq!(got.exclude, row["exclude"] == true, "multT '{id}' exclude");
                n_mult += 1;
            }
            Some("combineT") => {
                let keywords = strings(&row["memory"]["keywords"]);
                let mem = tag_view(&row["memory"], &keywords);
                let parts = ctx_parts(&row["ctx"], row.get("tuningInput"));
                let ctx = recall_ctx(&row["ctx"], &parts);
                let relevance = row.get("relevance").map(|r| RecallRelevance {
                    cosine: r["cosine"].as_f64().unwrap(),
                    best_cosine: r["bestCosine"].as_f64().unwrap(),
                });
                let got = combine_recall_multipliers(&mem, &ctx, relevance);
                let wm = row["multiplier"].as_f64().unwrap();
                assert!(
                    (got.multiplier - wm).abs() <= EPS,
                    "combineT '{id}': rust={} oracle={wm}",
                    got.multiplier
                );
                assert_eq!(got.fired, want_fired(&row), "combineT '{id}' fired");
                assert_eq!(
                    got.exclude,
                    row["exclude"] == true,
                    "combineT '{id}' exclude"
                );
                n_combine += 1;
            }
            _ => {}
        }
    }
    assert!(
        n_gate > 0 && n_mult > 0 && n_combine > 0,
        "retuning rows missing — regenerate the oracle"
    );
    eprintln!("OK: recall-tags retuning rows matched oracle ({n_gate} gate, {n_mult} multT, {n_combine} combineT).");
}
