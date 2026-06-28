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
//!   QT_ORACLE_RECALL_TAGS=/tmp/oracle-recall-tags.ndjson cargo test -p quilltap-harness

// Data-driven case tables use multi-field tuples by design; the complexity lint
// is noise here.
#![allow(clippy::type_complexity)]

use std::collections::{HashMap, HashSet};

use quilltap_core::recall_tags::{
    combine_recall_multipliers, context_multiplier, parse_targeting_tags, participant_multiplier,
    recently_whispered_multiplier, scope_project_multiplier, temporal_multiplier, ContextTag,
    MemoryTagView, RecallContext, RecallMultiplier, ScopePolicy, ScopeTag, TargetingTags,
    TemporalTag,
};
use serde::Deserialize;

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
        let got = scope_project_multiplier(t, mp, cp, pol);
        assert_mult(&idx, &format!("scope/{id}"), &got);
    }

    // ---- temporalMultiplier ----
    for t in [
        TemporalTag::Past,
        TemporalTag::Moment,
        TemporalTag::Present,
        TemporalTag::Future,
    ] {
        let got = temporal_multiplier(tags(t, ScopeTag::Wide, ContextTag::Information));
        assert_mult(&idx, &format!("temporal/{}", t.as_str()), &got);
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
        let got = context_multiplier(tags(TemporalTag::Present, ScopeTag::Wide, mem_ctx), turn);
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
        let got = participant_multiplier(&mem, &present);
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
        let got = recently_whispered_multiplier(&mem, Some(&set));
        assert_mult(&idx, &format!("recent/{id}"), &got);
    }

    // ---- combineRecallMultipliers ----
    // Each case: (id, keywords, projectId, aboutCharacterId,
    //             currentProjectId, scopePolicy, turnContext, present[], recent[])
    struct CC {
        id: &'static str,     // oracle row key (case label)
        mem_id: &'static str, // the memory's own id (matters for anti-repetition)
        keywords: Vec<&'static str>,
        project_id: Option<&'static str>,
        about: Option<&'static str>,
        current_project_id: Option<&'static str>,
        policy: ScopePolicy,
        turn_context: Option<ContextTag>,
        present: Vec<&'static str>,
        recent: Option<Vec<&'static str>>,
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
        },
    ];
    for cc in &combine_cases {
        let keywords: Vec<String> = s(&cc.keywords);
        let present: Vec<String> = s(&cc.present);
        let recent_set: Option<HashSet<String>> = cc
            .recent
            .as_ref()
            .map(|r| r.iter().map(|x| x.to_string()).collect());
        let mem = MemoryTagView {
            id: Some(cc.mem_id),
            project_id: cc.project_id,
            keywords: &keywords,
            about_character_id: cc.about,
        };
        let ctx = RecallContext {
            current_project_id: cc.current_project_id,
            scope_policy: cc.policy,
            present_about_character_ids: &present,
            turn_context: cc.turn_context,
            recently_whispered_ids: recent_set.as_ref(),
        };
        let got = combine_recall_multipliers(&mem, &ctx);
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
