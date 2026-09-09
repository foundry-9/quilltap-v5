//! Tier-1 differential for the character-progressions ENGINE (P4.D167) —
//! `quilltap_core::progressions` against v4's REAL `lib/progressions/
//! {schema,engine}.ts` at `25f534c0b`.
//!
//! Both sides read the SAME committed corpus
//! (`harness/oracle/fixtures/progressions-engine.json`, 549 rows across
//! seventeen ops); the oracle case drives v4's real exports and emits one
//! NDJSON row per (op, label), and this family recomputes each row in Rust and
//! compares field for field. Strings are byte-exact; the integer-valued JS
//! numbers (`startMs`, `endMs`, `nowMs`, `elapsedMs`, `remainingMs`, the sheet's
//! epoch times) compare EXACTLY — and so, in effect, do the three genuine
//! floats (`percent`, `percentClamped`, `quantityCurrent`): the `derived` /
//! `sheet` subtrees are compared whole by `assert_row` BEFORE the 1e-12
//! re-comparison below them runs, so the tolerance is not in force (the
//! unification review of 2026-09-09 measured this). Both sides do the same
//! IEEE operations and have agreed exactly through every regen; the day a
//! last-ULP difference appears, this family reds and the tolerance path is
//! the fix — until then the claim is "exact", stated honestly.
//!
//! What only a differential can answer here: Zod 4.5.4's issue sentences AND
//! their order, its code-point string lengths on astral text, `Date.parse`'s V8
//! subset behind v4's stricter regex, `toFixed`'s decimal half-up rounding
//! (`0.125` → `0.13`, where a Rust `{:.2}` gives `0.12`), and
//! `Intl.DateTimeFormat`'s exact `en-US` `dateStyle: 'medium'` bytes.
//!
//! ⚠ `TZ=UTC` is REQUIRED when generating the oracle: v4's `formatInstant`
//! falls back to the HOST zone for an absent or unresolvable timezone, and the
//! Rust twin pins that fallback to UTC — the documented harness seam shared with
//! `context_feeders_leaves_equivalence`.
//!
//! Generate (Node 24, from the v4 checkout; a pinned worktree while v4 HEAD is
//! past the baseline — the sweep driver rewrites the `cd`):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   TZ=UTC $N/node --import tsx $V5W/harness/oracle/cases/progressions-engine.ts \
//!     > /tmp/oracle-progressions-engine.ndjson
//! Run:
//!   QT_ORACLE_PROGRESSIONS_ENGINE=/tmp/oracle-progressions-engine.ndjson \
//!     cargo test -p quilltap-harness --test progressions_engine_equivalence

use std::collections::HashMap;

use serde_json::{json, Map, Value};

use quilltap_core::progressions::{
    default_in_progress_template, derive_progression, flatten_progressions, format_span,
    format_span_whole, infer_increment, is_progression_id, is_report_frequency,
    is_writable_progression_field, join_issues, parse_iso_instant, parse_progress_key,
    parse_progression, parse_progressions, parse_report_period_ms, progression_placeholders,
    render_progression_report, should_report_progression, Progression, RenderProgressionOptions,
    TimeIncrement,
};

/// The oracle rows, keyed `(op, label)`.
type Oracle = HashMap<(String, String), Value>;

fn corpus() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../harness/oracle/fixtures/progressions-engine.json"
    );
    let raw = std::fs::read_to_string(path).expect("the committed corpus is readable");
    serde_json::from_str(&raw).expect("the committed corpus is valid JSON")
}

fn oracle() -> Option<Oracle> {
    let path = std::env::var("QT_ORACLE_PROGRESSIONS_ENGINE").ok()?;
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("QT_ORACLE_PROGRESSIONS_ENGINE={path} is unreadable: {e}"));
    assert!(
        !raw.trim().is_empty(),
        "QT_ORACLE_PROGRESSIONS_ENGINE={path} is EMPTY — the generation step failed and its \
         stdout redirect truncated the file (the empty-file trap); regenerate before believing \
         any diff"
    );
    let mut map = Oracle::new();
    for line in raw.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("each oracle line is JSON");
        let op = row["op"]
            .as_str()
            .expect("every row names its op")
            .to_string();
        let label = row["label"]
            .as_str()
            .expect("every row names its case")
            .to_string();
        assert!(
            map.insert((op.clone(), label.clone()), row).is_none(),
            "duplicate oracle row for ({op}, {label}) — corpus labels must be unique within an op"
        );
    }
    Some(map)
}

/// The unit named by a corpus row.
fn unit(v: &Value) -> TimeIncrement {
    TimeIncrement::parse(v.as_str().expect("the corpus names a unit as a string"))
        .expect("the corpus names one of v4's seven units")
}

/// A corpus `ms` field: a number, or one of the three non-finite spellings JSON
/// cannot hold.
fn ms_field(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().expect("a JSON number converts"),
        Value::String(s) => match s.as_str() {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            other => panic!("unknown non-finite spelling {other:?} in the corpus"),
        },
        other => panic!("a corpus `ms` is a number or a sentinel string, got {other}"),
    }
}

/// A parsed progression for a corpus row known valid, with v4's own
/// post-parse `updatedAt` cast where the row asks for it.
fn parsed(entry: &Value, updated_at_override: Option<&str>) -> Progression {
    let mut p = parse_progression(entry).unwrap_or_else(|issues| {
        panic!(
            "this corpus row must parse; it did not: {}",
            join_issues(&issues)
        )
    });
    if let Some(v) = updated_at_override {
        p.updated_at = Some(v.to_string());
    }
    p
}

/// A JS number as a `Value` the way `JSON.stringify` writes one — integral
/// values without a decimal point, non-finite as `null`.
fn num(x: f64) -> Value {
    if !x.is_finite() {
        return Value::Null;
    }
    if x.fract() == 0.0 && x.abs() < 9_007_199_254_740_992.0 {
        return Value::from(x as i64);
    }
    Value::from(x)
}

/// Compare two JSON values, allowing 1e-12 on the named float paths.
fn assert_row(op: &str, label: &str, got: &Value, want: &Value, float_keys: &[&str]) {
    match (got, want) {
        (Value::Object(g), Value::Object(w)) => {
            let g_keys: Vec<_> = g.keys().collect();
            let w_keys: Vec<_> = w.keys().collect();
            assert_eq!(
                g_keys, w_keys,
                "{op}/{label}: key set or ORDER differs\n  v5: {g_keys:?}\n  v4: {w_keys:?}"
            );
            for (k, wv) in w {
                let gv = &g[k];
                if float_keys.contains(&k.as_str()) {
                    match (gv.as_f64(), wv.as_f64()) {
                        (Some(a), Some(b)) => assert!(
                            (a - b).abs() <= 1e-12 * b.abs().max(1.0),
                            "{op}/{label}.{k}: {a} != {b}"
                        ),
                        _ => assert_eq!(gv, wv, "{op}/{label}.{k}"),
                    }
                } else {
                    assert_eq!(gv, wv, "{op}/{label}.{k}");
                }
            }
        }
        _ => assert_eq!(got, want, "{op}/{label}"),
    }
}

#[test]
fn progressions_engine_equivalence() {
    let Some(oracle) = oracle() else {
        eprintln!(
            "SKIP: QT_ORACLE_PROGRESSIONS_ENGINE unset — see this file's header for the recipe"
        );
        return;
    };
    let corpus = corpus();
    let mut checked: HashMap<&str, usize> = HashMap::new();
    let mut check = |op: &'static str, label: &str, got: Value, floats: &[&str]| {
        let want = oracle
            .get(&(op.to_string(), label.to_string()))
            .unwrap_or_else(|| panic!("the oracle has no row for ({op}, {label}) — regenerate it"));
        let mut want = want.clone();
        let want = want.as_object_mut().expect("an oracle row is an object");
        want.shift_remove("op");
        want.shift_remove("label");
        assert_row(op, label, &got, &Value::Object(want.clone()), floats);
        *checked.entry(op).or_default() += 1;
    };

    // ---------------------------------------------------------- parseIsoInstant
    for c in corpus["parseIsoInstant"].as_array().unwrap() {
        let label = c["label"].as_str().unwrap();
        let result = parse_iso_instant(&c["value"]);
        check("parseIsoInstant", label, json!({ "result": result }), &[]);
    }

    // -------------------------------------------------------- parseProgression
    for c in corpus["parseProgression"].as_array().unwrap() {
        let label = c["label"].as_str().unwrap();
        let got = match parse_progression(&c["entry"]) {
            Ok(p) => json!({
                "ok": true,
                // The Tier-2 key-order pin: `Progression`'s `Serialize` must emit
                // v4's `strictObject` key order, absent optionals omitted.
                "parsedValue": serde_json::to_value(&p).unwrap(),
                "issues": Value::Null,
            }),
            Err(issues) => json!({
                "ok": false,
                "parsedValue": Value::Null,
                "issues": join_issues(&issues),
            }),
        };
        check("parseProgression", label, got, &[]);
    }

    // ------------------------------------------------------- parseProgressions
    for c in corpus["parseProgressions"].as_array().unwrap() {
        let label = c["label"].as_str().unwrap();
        let metadata = c.get("metadata").filter(|v| !v.is_null());
        // v4's `metadata` is `unknown`: a JSON `null` and an absent value are the
        // same "none" answer, and the corpus spells the array/string/number arms
        // explicitly.
        let mut issues: Vec<Value> = Vec::new();
        let kept = parse_progressions(metadata, &mut |id, issue| {
            issues.push(json!([id, issue]));
        });
        let ids: Vec<Value> = kept.iter().map(|(id, _)| Value::from(id.clone())).collect();
        check(
            "parseProgressions",
            label,
            json!({ "ids": ids, "issues": issues }),
            &[],
        );
    }

    // ----------------------------------------------- formatSpan/formatSpanWhole
    for c in corpus["formatSpan"].as_array().unwrap() {
        let label = c["label"].as_str().unwrap();
        let got = format_span(ms_field(&c["ms"]), unit(&c["unit"]));
        check("formatSpan", label, json!({ "result": got }), &[]);
    }
    for c in corpus["formatSpanWhole"].as_array().unwrap() {
        let label = c["label"].as_str().unwrap();
        let got = format_span_whole(ms_field(&c["ms"]), unit(&c["unit"]));
        check("formatSpanWhole", label, json!({ "result": got }), &[]);
    }

    // ------------------------------------------------------- deriveProgression
    let derived_floats = ["percent", "percentClamped", "quantityCurrent"];
    for c in corpus["deriveProgression"].as_array().unwrap() {
        let label = c["label"].as_str().unwrap();
        let p = parsed(&c["entry"], None);
        let d = derive_progression(c["id"].as_str().unwrap(), &p, c["nowMs"].as_i64().unwrap());
        check(
            "deriveProgression",
            label,
            json!({ "derived": derived_value(&d) }),
            &[],
        );
        // The float tolerance applies inside the nested object, so compare that
        // half again field-wise.
        let want = &oracle[&("deriveProgression".to_string(), label.to_string())]["derived"];
        assert_row(
            "deriveProgression",
            label,
            &derived_value(&d),
            want,
            &derived_floats,
        );
    }

    // -------------------------------------------------- shouldReportProgression
    for c in corpus["shouldReportProgression"].as_array().unwrap() {
        let label = c["label"].as_str().unwrap();
        let p = parsed(&c["entry"], c["updatedAtOverride"].as_str());
        let now = c["nowMs"].as_i64().unwrap();
        let d = derive_progression(c["id"].as_str().unwrap(), &p, now);
        let r = should_report_progression(&p, &d, c["lastTurnMs"].as_i64());
        check(
            "shouldReportProgression",
            label,
            json!({ "report": r.report, "reason": r.reason.as_str() }),
            &[],
        );
    }

    // ------------------------------------------------------ parseReportPeriodMs
    for c in corpus["parseReportPeriodMs"].as_array().unwrap() {
        let label = c["label"].as_str().unwrap();
        let got = parse_report_period_ms(c["value"].as_str().unwrap());
        check("parseReportPeriodMs", label, json!({ "result": got }), &[]);
    }

    // -------------------------------------- render + placeholders + the default
    for c in corpus["renderProgressionReport"].as_array().unwrap() {
        let label = c["label"].as_str().unwrap();
        let p = parsed(&c["entry"], None);
        let d = derive_progression(c["id"].as_str().unwrap(), &p, c["nowMs"].as_i64().unwrap());
        let opts = RenderProgressionOptions {
            timezone: c["timezone"].as_str(),
        };
        check(
            "renderProgressionReport",
            label,
            json!({ "result": render_progression_report(&p, &d, &opts) }),
            &[],
        );
    }
    for c in corpus["progressionPlaceholders"].as_array().unwrap() {
        let label = c["label"].as_str().unwrap();
        let p = parsed(&c["entry"], None);
        let d = derive_progression(c["id"].as_str().unwrap(), &p, c["nowMs"].as_i64().unwrap());
        let opts = RenderProgressionOptions {
            timezone: c["timezone"].as_str(),
        };
        let values = progression_placeholders(&p, &d, &opts);
        let keys: Vec<Value> = values.iter().map(|(k, _)| Value::from(k.clone())).collect();
        let mut object = Map::new();
        for (k, v) in &values {
            object.insert(k.clone(), Value::from(v.clone()));
        }
        check(
            "progressionPlaceholders",
            label,
            json!({ "keys": keys, "values": Value::Object(object) }),
            &[],
        );
    }
    for c in corpus["defaultInProgressTemplate"].as_array().unwrap() {
        let label = c["label"].as_str().unwrap();
        let p = parsed(&c["entry"], None);
        check(
            "defaultInProgressTemplate",
            label,
            json!({ "result": default_in_progress_template(&p) }),
            &[],
        );
    }

    // ------------------------------------------------------ flattenProgressions
    for c in corpus["flattenProgressions"].as_array().unwrap() {
        let label = c["label"].as_str().unwrap();
        let metadata = c.get("metadata").filter(|v| !v.is_null());
        let mut issues: Vec<Value> = Vec::new();
        let sheet = flatten_progressions(metadata, c["nowMs"].as_i64().unwrap(), &mut |id, i| {
            issues.push(json!([id, i]));
        });
        let keys: Vec<Value> = sheet.keys().map(|k| Value::from(k.clone())).collect();
        check(
            "flattenProgressions",
            label,
            json!({ "keys": keys, "sheet": Value::Object(sheet.clone()), "issues": issues }),
            &[],
        );
        // `<id>.percent` and `<id>.quantity` are the sheet's genuine floats.
        let want = &oracle[&("flattenProgressions".to_string(), label.to_string())]["sheet"];
        let float_keys: Vec<String> = sheet
            .keys()
            .filter(|k| k.ends_with(".percent") || k.ends_with(".quantity"))
            .cloned()
            .collect();
        let float_refs: Vec<&str> = float_keys.iter().map(String::as_str).collect();
        assert_row(
            "flattenProgressions",
            label,
            &Value::Object(sheet),
            want,
            &float_refs,
        );
    }

    // ------------------------------------------------------------ the leaf sets
    for c in corpus["inferIncrement"].as_array().unwrap() {
        let label = c["label"].as_str().unwrap();
        let got = infer_increment(c["spanMs"].as_f64().unwrap());
        check(
            "inferIncrement",
            label,
            json!({ "result": got.as_str() }),
            &[],
        );
    }
    for c in corpus["parseProgressKey"].as_array().unwrap() {
        let label = c["label"].as_str().unwrap();
        let got = match parse_progress_key(c["key"].as_str().unwrap()) {
            Ok((id, field)) => json!({ "ok": true, "id": id, "field": field, "reason": null }),
            Err(reason) => {
                json!({ "ok": false, "id": null, "field": null, "reason": reason })
            }
        };
        check("parseProgressKey", label, got, &[]);
    }
    for c in corpus["isWritableProgressionField"].as_array().unwrap() {
        let label = c["label"].as_str().unwrap();
        let got = is_writable_progression_field(c["field"].as_str().unwrap());
        check(
            "isWritableProgressionField",
            label,
            json!({ "result": got }),
            &[],
        );
    }
    for c in corpus["isProgressionId"].as_array().unwrap() {
        let label = c["label"].as_str().unwrap();
        check(
            "isProgressionId",
            label,
            json!({ "result": is_progression_id(c["id"].as_str().unwrap()) }),
            &[],
        );
    }
    for c in corpus["isReportFrequency"].as_array().unwrap() {
        let label = c["label"].as_str().unwrap();
        check(
            "isReportFrequency",
            label,
            json!({ "result": is_report_frequency(c["value"].as_str().unwrap()) }),
            &[],
        );
    }

    let mut ops: Vec<_> = checked.iter().collect();
    ops.sort();
    let total: usize = checked.values().sum();
    for (op, n) in &ops {
        println!("  {op:32} {n}");
    }
    println!(
        "progressions_engine_equivalence: {total} rows across {} ops",
        ops.len()
    );
    assert_eq!(
        total,
        oracle.len(),
        "every oracle row must be consumed — a corpus row the family forgot is a silent gap"
    );
    // Floors, so a corpus trimmed by accident cannot pass quietly.
    assert!(checked["parseIsoInstant"] >= 14, "parseIsoInstant floor");
    assert!(checked["parseProgression"] >= 30, "parseProgression floor");
    assert!(
        checked["parseProgressions"] >= 12,
        "parseProgressions floor"
    );
    assert!(checked["formatSpan"] >= 60, "formatSpan floor");
    assert!(
        checked["shouldReportProgression"] >= 20,
        "shouldReportProgression floor"
    );
    assert_eq!(ops.len(), 17, "every op is exercised");
}

/// The `DerivedProgression` fields the oracle emits, in the oracle's own key
/// order (which is this function's — the two are written together).
fn derived_value(d: &quilltap_core::progressions::DerivedProgression) -> Value {
    json!({
        "id": d.id,
        "name": d.name,
        "startMs": d.start_ms,
        "endMs": d.end_ms,
        "nowMs": d.now_ms,
        "elapsedMs": num(d.elapsed_ms),
        "remainingMs": num(d.remaining_ms),
        "percent": num(d.percent),
        "percentClamped": num(d.percent_clamped),
        "state": d.state.as_str(),
        "started": d.started,
        "complete": d.complete,
        "quantityCurrent": d.quantity_current.map(num).unwrap_or(Value::Null),
        "hasQuantityCurrent": d.quantity_current.is_some(),
        "elapsed": d.elapsed,
        "elapsedWhole": d.elapsed_whole,
        "remaining": d.remaining,
        "remainingWhole": d.remaining_whole,
    })
}
