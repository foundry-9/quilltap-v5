//! Tier-1 differential: v4's memory candidate at the pin (`83334b9a5`) —
//! `crate::memory_candidate` (P4.D289, C1-A item 4 + LR-13). Mirrors
//! `__tests__/unit/lib/memory/memory-candidate.test.ts`'s pure rows and adds
//! every issue kind `MemoryCandidateSchema` can raise. Row kinds:
//!
//! - `const` — `MEMORY_ENTITY_CAP`, `DEFAULT_TARGETING`;
//! - `normalise` — `normaliseTargeting` (`' PAST '`, unknown words, fallback
//!   axes, non-string axes, a non-object raw, JS-`trim` whitespace);
//! - `tkw` — `targetingKeywords`;
//! - `split` — `splitTargetingKeywords` (last of each axis wins, non-strings
//!   skipped, an unknown `scope:` value dropped, blanks dropped). The
//!   `targeting` half is a `Partial<TargetingTags>` whose JS key order is
//!   first-ASSIGNMENT order — not a contract — so it compares as a JSON
//!   object (order-blind `Map ==`); `free` compares byte-for-byte;
//! - `coerce` — `coerceImportance` over `−0.2 / 0 / 0.3 / 1.7 / NaN /
//!   '0.5' / undefined` and the rest;
//! - `anchor` — `anchorTimeMsFrom` (first `Date.parse`-finite entry, else
//!   the fallback);
//! - `draft` — `candidateFromDraft` (the blank-text fallbacks keeping the
//!   UNTRIMMED value, whitespace-only both → null, the importance clamp,
//!   entities 13 → 12, kind coercion, a non-finite anchor, empty ids) and
//!   `toAdmissionRow` over each accepted candidate;
//! - `parse` — `MemoryCandidateSchema.safeParse` directly: the parsed data
//!   byte-for-byte under `serde_json::to_string` (schema key order, nulls
//!   kept, defaults applied, unknown keys stripped) plus `toAdmissionRow`,
//!   or zod's `error.issues` byte-for-byte (order, `path`, `message` and
//!   Zod 4's per-code keys);
//! - `parse_nonfinite` — see below.
//!
//! **`toAdmissionRow` compares a v4 `null` and an absent key alike** for
//! `AdmissionRow`'s optional fields — that type's documented contract
//! (`admission/types.rs`: every v4 reader treats `undefined` and `null`
//! alike, so both are `None` and the serialised form omits them). v4's row
//! is an object literal whose key order is not a contract either, so the
//! row compares key SET + per-key value (numbers at 1e-12 — `AdmissionRow`
//! writes `importance` as an `f64`, `0.0` where JS writes `0`; everything
//! else byte-exact).
//!
//! **Non-finite numbers.** JSON — and so `serde_json::Value` — has no
//! spelling for NaN / ±Infinity. They ride as string tokens. Where v4 sees
//! a real NaN and v5 can only see the token, the row says `nonFinite: true`:
//! - `coerce` / `draft` importance: v5 is fed the TOKEN STRING, which takes
//!   the same "not a finite number → 0.5" arm (a model's importance arrives
//!   through `JSON.parse`, which never yields a NaN);
//! - `draft` anchor: the token becomes the `f64` in the TYPED provenance,
//!   and `candidate_from_draft` refuses it (→ `None`, v4's `null`);
//! - `parse_nonfinite`: zod's refusal carries a `received: "<NaN|Infinity>"`
//!   key no `ZodIssue` variant has, over an input no `Value` can carry. v5
//!   is fed the tokenised input and must refuse with ONE issue at the same
//!   `path`, `code` and `expected` — the `received` word in the message (and
//!   the `received` key) is the part JSON cannot express.
//!
//! Generate the oracle output, then run:
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   TZ=UTC $N/npx tsx "$V5W/harness/oracle/cases/memory-candidate.ts" > /tmp/oracle-memory-candidate.ndjson
//!   cd "$V5W"
//!   QT_ORACLE_MEMORY_CANDIDATE=/tmp/oracle-memory-candidate.ndjson \
//!     cargo test -p quilltap-harness --test memory_candidate_equivalence -- --nocapture

use quilltap_core::memory_candidate::{
    anchor_time_ms_from, candidate_from_draft, coerce_importance, normalise_targeting,
    parse_memory_candidate, split_targeting_keywords, targeting_keywords, to_admission_row,
    CandidatePass, CandidateProvenance, CandidateSubject, MemoryDraftView, MemorySource,
    PartialTargetingTags, ResolvedCandidateTime, TargetingTags, WitnessedContext,
    DEFAULT_TARGETING, MEMORY_ENTITY_CAP,
};
use quilltap_core::recall_tags::{ContextTag, ScopeTag, TemporalTag};
use serde_json::{json, Map, Value};

const EPS: f64 = 1e-12;

/// A JS number as the case writes it: a JSON number, or a non-finite token.
fn num(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap(),
        Value::String(s) => match s.as_str() {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            other => panic!("bad number token {other:?}"),
        },
        other => panic!("not a number: {other}"),
    }
}

fn tags_json(t: &TargetingTags) -> Value {
    json!({ "temporal": t.temporal.as_str(), "scope": t.scope.as_str(), "context": t.context.as_str() })
}

fn partial_from(v: Option<&Value>) -> PartialTargetingTags {
    let s = |k: &str| v.and_then(|v| v.get(k)).and_then(Value::as_str);
    PartialTargetingTags {
        temporal: s("temporal").map(|x| TemporalTag::from_kw(x).expect("temporal")),
        scope: s("scope").map(|x| ScopeTag::from_kw(x).expect("scope")),
        context: s("context").map(|x| ContextTag::from_kw(x).expect("context")),
    }
}

fn partial_json(p: &PartialTargetingTags) -> Value {
    let mut m = Map::new();
    if let Some(t) = p.temporal {
        m.insert("temporal".into(), json!(t.as_str()));
    }
    if let Some(s) = p.scope {
        m.insert("scope".into(), json!(s.as_str()));
    }
    if let Some(c) = p.context {
        m.insert("context".into(), json!(c.as_str()));
    }
    Value::Object(m)
}

fn opt_string(m: &Value, k: &str) -> Option<String> {
    match m.get(k) {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) => panic!("{k}: expected a string, got {other}"),
    }
}

fn opt_array(m: &Value, k: &str) -> Option<Vec<Value>> {
    m.get(k).map(|v| {
        v.as_array()
            .unwrap_or_else(|| panic!("{k}: not an array"))
            .clone()
    })
}

fn subject_of(v: &Value) -> CandidateSubject {
    CandidateSubject {
        holder_id: v["holderId"].as_str().unwrap().to_string(),
        about_id: opt_string(v, "aboutId"),
    }
}

fn provenance_of(v: &Value) -> CandidateProvenance {
    CandidateProvenance {
        source: MemorySource::from_wire(v["source"].as_str().unwrap()).unwrap(),
        witnessed_context: v["witnessedContext"]
            .as_str()
            .map(|s| WitnessedContext::from_wire(s).unwrap()),
        chat_id: opt_string(v, "chatId"),
        project_id: opt_string(v, "projectId"),
        source_message_id: opt_string(v, "sourceMessageId"),
        anchor_time_ms: num(&v["anchorTimeMs"]),
        pass: CandidatePass::from_wire(v["pass"].as_str().unwrap()).unwrap(),
    }
}

fn resolved_of(v: &Value) -> ResolvedCandidateTime {
    ResolvedCandidateTime {
        occurred_at: opt_string(v, "occurredAt"),
        narrative_time: opt_string(v, "narrativeTime"),
        source_message_timestamp: opt_string(v, "sourceMessageTimestamp"),
    }
}

/// `toAdmissionRow`: null ≡ absent; key set + per-key value.
fn same_row(got: &quilltap_core::admission::types::AdmissionRow, want: &Value, ctx: &str) {
    let got = serde_json::to_value(got).unwrap();
    let got = got.as_object().unwrap();
    let want: Map<String, Value> = want
        .as_object()
        .unwrap()
        .iter()
        .filter(|(_, v)| !v.is_null())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let mut gk: Vec<&String> = got.keys().collect();
    let mut wk: Vec<&String> = want.keys().collect();
    gk.sort();
    wk.sort();
    assert_eq!(gk, wk, "{ctx}: row key set");
    for (k, w) in &want {
        let g = &got[k];
        match (g, w) {
            (Value::Number(a), Value::Number(b)) => {
                let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
                assert!((a - b).abs() <= EPS, "{ctx}: row.{k} rust={a} oracle={b}");
            }
            _ => assert_eq!(
                serde_json::to_string(g).unwrap(),
                serde_json::to_string(w).unwrap(),
                "{ctx}: row.{k}"
            ),
        }
    }
}

#[test]
fn memory_candidate_matches_v4_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_MEMORY_CANDIDATE") else {
        eprintln!("SKIP: set QT_ORACLE_MEMORY_CANDIDATE to the memory-candidate.ts NDJSON");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read oracle NDJSON");
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let r: Value = serde_json::from_str(line).expect("NDJSON row");
        let kind = r["kind"].as_str().unwrap().to_string();
        *counts.entry(kind.clone()).or_default() += 1;
        let label = r.get("label").and_then(Value::as_str).unwrap_or("");
        let ctx = format!("{kind} [{label}]");
        match kind.as_str() {
            "const" => {
                assert_eq!(
                    r["MEMORY_ENTITY_CAP"].as_u64().unwrap() as usize,
                    MEMORY_ENTITY_CAP
                );
                assert_eq!(
                    serde_json::to_string(&tags_json(&DEFAULT_TARGETING)).unwrap(),
                    serde_json::to_string(&r["DEFAULT_TARGETING"]).unwrap(),
                    "{ctx}"
                );
            }
            "normalise" => {
                let fallback = partial_from(r.get("fallback"));
                let got = normalise_targeting(r.get("raw"), &fallback);
                assert_eq!(
                    serde_json::to_string(&tags_json(&got)).unwrap(),
                    serde_json::to_string(&r["out"]).unwrap(),
                    "{ctx}"
                );
            }
            "tkw" => {
                let p = partial_from(Some(&r["t"]));
                let t = TargetingTags {
                    temporal: p.temporal.unwrap(),
                    scope: p.scope.unwrap(),
                    context: p.context.unwrap(),
                };
                assert_eq!(json!(targeting_keywords(&t)), r["out"], "{ctx}");
            }
            "split" => {
                let got = split_targeting_keywords(r["keywords"].as_array().unwrap());
                assert_eq!(
                    serde_json::to_string(&got.free).unwrap(),
                    serde_json::to_string(&r["out"]["free"]).unwrap(),
                    "{ctx}: free"
                );
                // Partial<TargetingTags>: order-blind (first-assignment order).
                assert_eq!(
                    partial_json(&got.targeting),
                    r["out"]["targeting"],
                    "{ctx}: targeting"
                );
            }
            "coerce" => {
                let got = coerce_importance(r.get("raw"));
                let want = num(&r["out"]);
                assert_eq!(
                    got.to_bits(),
                    want.to_bits(),
                    "{ctx}: rust={got} oracle={want}"
                );
            }
            "anchor" => {
                let isos: Vec<Option<&str>> = r["isos"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(Value::as_str)
                    .collect();
                let got = anchor_time_ms_from(&isos, num(&r["fallback"]));
                assert_eq!(got, num(&r["out"]), "{ctx}");
            }
            "draft" => {
                let d = &r["draft"];
                let draft = MemoryDraftView {
                    content: opt_string(d, "content"),
                    summary: opt_string(d, "summary"),
                    keywords: opt_array(d, "keywords"),
                    importance: d.get("importance").cloned(),
                    targeting: d.get("targeting").cloned(),
                    kind: opt_string(d, "kind"),
                    when: opt_string(d, "when"),
                    entities: opt_array(d, "entities"),
                };
                let got = candidate_from_draft(
                    &draft,
                    subject_of(&r["subject"]),
                    provenance_of(&r["provenance"]),
                );
                match (&got, &r["out"]) {
                    (None, Value::Null) => {}
                    (Some(c), want @ Value::Object(_)) => {
                        assert_eq!(
                            serde_json::to_string(c).unwrap(),
                            serde_json::to_string(want).unwrap(),
                            "{ctx}: candidate"
                        );
                        same_row(
                            &to_admission_row(c, &resolved_of(&r["resolved"])),
                            &r["row"],
                            &ctx,
                        );
                    }
                    (g, w) => panic!("{ctx}: rust={g:?} oracle={w}"),
                }
            }
            "parse" => {
                let got = parse_memory_candidate(&r["input"]);
                match (got, r["success"].as_bool().unwrap()) {
                    (Ok(c), true) => {
                        assert_eq!(
                            serde_json::to_string(&c).unwrap(),
                            serde_json::to_string(&r["data"]).unwrap(),
                            "{ctx}: data"
                        );
                        same_row(
                            &to_admission_row(&c, &resolved_of(&r["resolved"])),
                            &r["row"],
                            &ctx,
                        );
                    }
                    (Err(issues), false) => assert_eq!(
                        serde_json::to_string(&issues).unwrap(),
                        serde_json::to_string(&r["issues"]).unwrap(),
                        "{ctx}: issues"
                    ),
                    (g, s) => panic!("{ctx}: rust={g:?} oracle success={s}"),
                }
            }
            "parse_nonfinite" => {
                assert!(
                    !r["success"].as_bool().unwrap(),
                    "{ctx}: v4 accepted a non-finite"
                );
                let want = r["issues"].as_array().unwrap();
                assert_eq!(want.len(), 1, "{ctx}");
                assert!(
                    want[0].get("received").is_some(),
                    "{ctx}: zod's `received` key"
                );
                let issues =
                    parse_memory_candidate(&r["input"]).expect_err("v5 accepted the token");
                assert_eq!(issues.len(), 1, "{ctx}");
                let g = issues[0].to_value();
                for k in ["code", "expected", "path"] {
                    assert_eq!(g[k], want[0][k], "{ctx}: {k}");
                }
            }
            other => panic!("unknown row kind {other}"),
        }
    }
    let expected: Vec<(&str, usize)> = vec![
        ("anchor", 7),
        ("coerce", 18),
        ("const", 1),
        ("draft", 37),
        ("normalise", 14),
        ("parse", 56), // P4.D311: +2 capSize rows; P4.169: +5 past-invalid_type rows
        ("parse_nonfinite", 3),
        ("split", 8),
        ("tkw", 3),
    ];
    let got: Vec<(&str, usize)> = counts.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    assert_eq!(got, expected, "oracle row counts drifted from the corpus");
    eprintln!("OK: memory-candidate matched oracle ({counts:?}).");
}
