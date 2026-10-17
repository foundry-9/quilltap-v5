//! Tier-1 differential: v4's memory calendar + day-reference rules at the pin
//! (`83334b9a5`) — `crate::memory_time` (P4.D289, C1-A item 3).
//!
//! The case (`harness/oracle/cases/memory-time.ts`) runs TWICE, `TZ=UTC` and
//! `TZ=America/Chicago`, appending; every row records its leg and BOTH legs
//! are required. Every zone is explicit, so a row compares the same way on
//! either leg — except the rows that read the process zone in v4:
//! getMemoryTimeZone's system rung (the Rust side models the host fallback
//! as `valid(QUILLTAP_TIMEZONE) ? it : <leg zone>`, the reconciliation
//! `quilltap-web` does) and the `ZONELESS_PARSE_AS_UTC` rows.
//!
//! Calendar rows: zone validity (with the ruled `MEMORY_ZONE_VALIDITY`
//! table), getMemoryTimeZone, `zonedParts` / the day, hour-on-day and Monday
//! helpers / `zonedDateStamp` over 44 instants × 7 zones (both 2026
//! transitions of every DST zone, local 23:30 / 00:30, the Chicago 25-hour
//! day, the Santiago midnight gap, Lord Howe's 30-minute and Chatham's
//! 45-minute offsets), `zonedTimeToUtcMs` over gaps / overlaps / `Date.UTC`
//! overflow / the two-digit-year window, and `zonedDayBoundsIso`.
//!
//! Rules / clock / anchors rows (P4.D289 Tier-1 unit 4; the section at the
//! foot of this file): the day-reference table read both ways, the write-only
//! rules, the read scan, `normaliseTimeRange`, `clockFor`, and
//! `resolveEpisodicAnchors`, with the ruled `ZONELESS_PARSE_AS_UTC` table
//! (LR-10).
//!
//! `PROTOTYPE_KEY_MONTH_THROWS` RETIRED at `25b8fab6a` (bug 247 converged — v4
//! reads its word tables through `Object.hasOwn`); the phrases stay in the
//! corpus as plain equality rows.
//!
//! Generate the oracle output (both legs in ONE go), then run:
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   TZ=UTC $N/npx tsx "$V5W/harness/oracle/cases/memory-time.ts" > /tmp/oracle-memory-time.ndjson
//!   TZ=America/Chicago $N/npx tsx "$V5W/harness/oracle/cases/memory-time.ts" >> /tmp/oracle-memory-time.ndjson
//!   cd "$V5W"
//!   QT_ORACLE_MEMORY_TIME=/tmp/oracle-memory-time.ndjson \
//!     cargo test -p quilltap-harness --test memory_time_equivalence -- --nocapture

use std::collections::{BTreeMap, BTreeSet};

use quilltap_core::memory_time::{
    is_valid_time_zone, memory_time_zone, start_of_zoned_day_ms, start_of_zoned_week_ms,
    zoned_date_stamp, zoned_day_bounds_iso, zoned_parts, zoned_time_on_day_ms,
    zoned_time_to_utc_ms, MEMORY_CALENDAR_DAY_MS,
};
use serde_json::{json, Value};

/// The ruled divergence `MEMORY_ZONE_VALIDITY` (work order P4.D289 LR-6):
/// `(string, v4's Intl verdict, v5's jiff verdict)` for every corpus string
/// the two runtimes still judge differently once the ECMA-402 offset forms
/// are ported. Both ways: a v4 or a v5 change to either verdict trips it.
///
/// Measured 2026-10-13: v4 = Node 24.13.1 / ICU 78.2 / tz 2025c; v5 = jiff
/// 0.2 reading the HOST's system zoneinfo (`/usr/share/zoneinfo`, `+VERSION`
/// 2026d — `posixrules` resolving is the tell: no bundled tzdb carries it).
/// All three are tzdb FILES that are not zones ICU knows; a Salon timezone
/// is chosen from a list, so none is a real stored value. Every link,
/// case-folded name and offset form in the 65-string corpus agrees.
const MEMORY_ZONE_VALIDITY: &[(&str, bool, bool)] = &[
    ("Etc/Unknown", false, true),
    ("Factory", false, true),
    ("posixrules", false, true),
];

fn num(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap(),
        Value::String(s) if s == "NaN" => f64::NAN,
        other => panic!("not a number: {other}"),
    }
}

fn same_ms(got: f64, want: &Value, ctx: &str) {
    let w = num(want);
    if w.is_nan() {
        assert!(got.is_nan(), "{ctx}: rust={got} oracle=NaN");
    } else {
        assert_eq!(got, w, "{ctx}");
    }
}

#[test]
fn memory_time_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_MEMORY_TIME") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_MEMORY_TIME to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut counts: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut validity_disagreements: Vec<(String, bool, bool)> = Vec::new();
    let mut unit4_rows: Vec<Value> = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let leg = row["leg"].as_str().unwrap().to_string();
        let kind = row["kind"].as_str().unwrap().to_string();
        *counts.entry((leg.clone(), kind.clone())).or_default() += 1;
        let ctx = format!("[{leg}] {kind} {}", row);
        match kind.as_str() {
            "valid" => {
                let tz = row["tz"].as_str().unwrap();
                let v4 = row["out"].as_bool().unwrap();
                let v5 = is_valid_time_zone(Some(tz));
                if v4 != v5 && leg == "UTC" {
                    validity_disagreements.push((tz.to_string(), v4, v5));
                }
            }
            "valid-null" => {
                assert_eq!(row["out"], json!([false, false]), "{ctx}");
                assert!(!is_valid_time_zone(None));
            }
            "mtz" => {
                let env = row["env"].as_str();
                let system = row["system"].as_str().unwrap();
                let host_fallback = match env {
                    Some(e) if is_valid_time_zone(Some(e)) => e,
                    _ => system,
                };
                let got = memory_time_zone(row["instance"].as_str(), host_fallback);
                assert_eq!(got, row["out"].as_str().unwrap(), "{ctx}");
            }
            "zoned" => {
                let tz = row["tz"].as_str().unwrap();
                let ms = quilltap_core::memory_scoring::date_ms(row["at"].as_str().unwrap());
                let p = zoned_parts(ms, tz);
                let parts = json!({
                    "year": p.year, "month": p.month, "day": p.day, "hour": p.hour,
                    "minute": p.minute, "second": p.second, "weekday": p.weekday,
                });
                assert_eq!(parts, row["parts"], "{ctx} parts");
                let day: Vec<f64> = [-2, -1, 0, 1, 2]
                    .iter()
                    .map(|o| start_of_zoned_day_ms(ms, tz, *o))
                    .collect();
                for (g, w) in day.iter().zip(row["dayStart"].as_array().unwrap()) {
                    same_ms(*g, w, &format!("{ctx} dayStart"));
                }
                let on: Vec<f64> = [(-1, 18), (0, 6), (0, 0), (1, 23), (-14, 0)]
                    .iter()
                    .map(|(o, h)| zoned_time_on_day_ms(ms, tz, *o, *h))
                    .collect();
                for (g, w) in on.iter().zip(row["onDay"].as_array().unwrap()) {
                    same_ms(*g, w, &format!("{ctx} onDay"));
                }
                let week: Vec<f64> = [-1, 0, 1]
                    .iter()
                    .map(|w| start_of_zoned_week_ms(ms, tz, *w))
                    .collect();
                for (g, w) in week.iter().zip(row["weekStart"].as_array().unwrap()) {
                    same_ms(*g, w, &format!("{ctx} weekStart"));
                }
                assert_eq!(
                    zoned_date_stamp(ms, tz),
                    row["stamp"].as_str().unwrap(),
                    "{ctx} stamp"
                );
            }
            "toUtc" => {
                let tz = row["tz"].as_str().unwrap();
                let w: Vec<i64> = row["wall"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_i64().unwrap())
                    .collect();
                same_ms(
                    zoned_time_to_utc_ms(w[0], w[1], w[2], w[3], tz),
                    &row["out"],
                    &ctx,
                );
            }
            "bounds" => {
                let tz = row["tz"].as_str().unwrap();
                let got = zoned_day_bounds_iso(row["date"].as_str().unwrap(), tz)
                    .map(|b| json!({"from": b.from, "to": b.to}))
                    .unwrap_or(Value::Null);
                assert_eq!(got, row["out"], "{ctx}");
            }
            "constants" => {
                assert_eq!(
                    MEMORY_CALENDAR_DAY_MS,
                    row["MEMORY_CALENDAR_DAY_MS"].as_i64().unwrap()
                );
            }
            k if UNIT4_KINDS.contains(&k) => unit4_rows.push(row.clone()),
            other => panic!("unknown row kind {other}"),
        }
    }

    // MEMORY_ZONE_VALIDITY, both ways: the measured disagreements ARE the table.
    let table: BTreeSet<(String, bool, bool)> = MEMORY_ZONE_VALIDITY
        .iter()
        .map(|(t, a, b)| (t.to_string(), *a, *b))
        .collect();
    let measured: BTreeSet<(String, bool, bool)> = validity_disagreements.into_iter().collect();
    assert_eq!(
        measured, table,
        "MEMORY_ZONE_VALIDITY drifted — (string, v4 Intl, v5 jiff) disagreements measured vs ruled"
    );

    check_rules_clock_anchors(&unit4_rows);

    let legs: BTreeSet<&str> = counts.keys().map(|(l, _)| l.as_str()).collect();
    assert_eq!(
        legs,
        BTreeSet::from(["America/Chicago", "UTC"]),
        "both TZ legs are required (regenerate in ONE go)"
    );
    for leg in ["UTC", "America/Chicago"] {
        let per: Vec<(&str, usize)> = counts
            .iter()
            .filter(|((l, _), _)| l == leg)
            .map(|((_, k), n)| (k.as_str(), *n))
            .collect();
        assert_eq!(
            per,
            vec![
                ("anchors", 3841), // P4.D312: +384 — `constructor 5` (bug 247)
                ("bounds", 161),
                ("clock", 126),
                ("constants", 1),
                ("event", 4630), // P4.D312: +175 — v4 `25b8fab6a`'s five phrases
                ("mtz", 24),
                ("normalise", 177),
                ("read", 1715),
                ("rule-both", 1428),
                ("rule-match", 48),
                ("rule-table", 1),
                ("toUtc", 273),
                ("valid", 65),
                ("valid-null", 1),
                ("when", 296),
                ("zoned", 308),
            ],
            "[{leg}] oracle row counts drifted from the corpus"
        );
    }
    eprintln!("OK: memory-time matched oracle ({counts:?}).");
}

// =============================================================================
// rules / clock / anchors (P4.D289 Tier-1 unit 4)
// =============================================================================
//
// Row kinds: `rule-table` (the nine rules, the number words, the cap, v4's scan
// source + flags), `rule-match` (`matchDayReferenceRule`, anchored, ASCII-only
// case folding — LR-9), `rule-both` (rule-table.test.ts's past phrases + the
// two future rules: the write stamps the START of the window the read
// returns), `event` (`resolveEventTime`: every write-only rule, the `the `
// strip, whitespace runs incl. U+00A0, eleven…fourteen, `a few weeks
// earlier`, `97 days ago`, the four LR-9 rows), `when` (`resolveWhenPhrase`),
// `read` (`resolveReferenceWindow`), `normalise` (`normaliseTimeRange`),
// `clock` (`clockFor` — the zone chain composed as `memory_time_zone(instance,
// host_fallback)`), `anchors` (`resolveEpisodicAnchors`, anchors.test.ts
// crossed). All over the seven zones of the calendar section, anchors around
// the DST transitions.
//
// Legs: every row's zone is explicit EXCEPT `clock` and `anchors` without a
// `timeZone`, where v4 falls back to the process zone — the Rust side passes
// the leg zone as the host fallback there and compares per leg. Every other
// row must produce the SAME v4 output on both legs, unless it is a
// `ZONELESS_PARSE_AS_UTC` row (below); v5's output equals the UTC leg on both.

use quilltap_core::memory_time::{
    clock_for, day_number_words_pattern, day_reference_scan_source, match_day_reference_rule,
    normalise_time_range, resolve_episodic_anchors, resolve_event_time, resolve_reference_window,
    resolve_when_phrase, DayReferenceResolution, EpisodicAnchorInput, EventTime, TimeWindow,
    DAY_NUMBER_WORDS, DAY_REFERENCE_TABLE, MAX_TABLE_DAYS_AGO,
};

/// The row kinds this section owns.
const UNIT4_KINDS: &[&str] = &[
    "rule-table",
    "rule-match",
    "rule-both",
    "event",
    "when",
    "read",
    "normalise",
    "clock",
    "anchors",
];

/// The ruled divergence `ZONELESS_PARSE_AS_UTC` (work order P4.D289 LR-10):
/// `(kind, row id)` for every row whose v4 output differs between the
/// `TZ=UTC` and `TZ=America/Chicago` legs because v4's `Date.parse` reads a
/// zone-less datetime (`2026-07-14T09:30`) in the PROCESS zone, where v5's
/// `episodic::js_date_parse_ms` reads it as UTC — so v5 equals the UTC leg on
/// both. Both ways: a row that stops differing (v4 converging), or any other
/// row that starts differing, trips the assertion. `zoneless-from-same-order`
/// (a zone-less bound whose order survives either reading) is the negative
/// control and is deliberately absent.
const ZONELESS_PARSE_AS_UTC: &[(&str, &str)] = &[
    // resolveEventTime's ISO-with-time arm returns `ISO(Date.parse(raw))`.
    ("event", "zoneless-datetime"),
    ("event", "zoneless-datetime-fraction"),
    ("event", "zoneless-datetime-seconds"),
    // resolveWhenPhrase parses its anchor.
    ("when", "zoneless-anchor"),
    ("when", "zoneless-anchor-just-now"),
    // normaliseTimeRange compares the passed-through bound's parse.
    ("normalise", "zoneless-from-flips-order"),
    // resolveEpisodicAnchors → resolveWhenPhrase's anchor.
    ("anchors", "zoneless-reference"),
];

fn ms_of(v: &Value) -> f64 {
    match v.as_str() {
        Some(iso) => quilltap_core::memory_scoring::date_ms(iso),
        None => f64::NAN,
    }
}

fn window_json(w: &Option<TimeWindow>) -> Value {
    w.as_ref()
        .map(|w| json!({"from": w.from, "to": w.to}))
        .unwrap_or(Value::Null)
}

fn event_json(e: &EventTime) -> Value {
    json!({"occurredAt": e.occurred_at, "window": window_json(&e.window)})
}

fn resolution_json(r: &Option<DayReferenceResolution>) -> Value {
    match r {
        Some(r) => json!({
            "timeRange": {"from": r.time_range.from, "to": r.time_range.to},
            "pastPointing": r.past_pointing,
            "matched": r.matched,
        }),
        None => Value::Null,
    }
}

/// The fields of a row that are v4's OUTPUT (everything else is input).
fn output_fields(kind: &str) -> &'static [&'static str] {
    match kind {
        "rule-both" => &["written", "read"],
        "rule-table" => &["rules", "words", "max", "scanSource", "scanFlags"],
        _ => &["out"],
    }
}

/// True where v4 reads the PROCESS zone by design (the host fallback).
fn leg_dependent(row: &Value) -> bool {
    match row["kind"].as_str().unwrap() {
        "clock" => true,
        "anchors" => row["input"].get("timeZone").is_none(),
        _ => false,
    }
}

/// v5's answer for a row, shaped as v4's output fields.
fn v5_for(row: &Value) -> Value {
    let leg = row["leg"].as_str().unwrap();
    let kind = row["kind"].as_str().unwrap();
    let tz = row["tz"].as_str().unwrap_or("");
    match kind {
        "rule-table" => {
            let rules: Vec<Value> = DAY_REFERENCE_TABLE
                .iter()
                .map(|r| json!({"name": r.name, "pattern": r.pattern}))
                .collect();
            let words: serde_json::Map<String, Value> = DAY_NUMBER_WORDS
                .iter()
                .map(|(w, n)| (w.to_string(), json!(n)))
                .collect();
            json!({
                "rules": rules,
                "words": words,
                "max": MAX_TABLE_DAYS_AGO,
                "scanSource": day_reference_scan_source(),
                "scanFlags": "gi",
            })
        }
        "rule-match" => {
            let phrase = row["phrase"].as_str().unwrap();
            let out = match match_day_reference_rule(phrase) {
                Some((rule, m)) => json!({
                    "name": rule.name,
                    "groups": m.iter().map(|g| g.map(|g| g.as_str())).collect::<Vec<_>>(),
                }),
                None => Value::Null,
            };
            json!({ "out": out })
        }
        "rule-both" => {
            let now = ms_of(&row["at"]);
            let phrase = row["phrase"].as_str().unwrap();
            let written = resolve_event_time(Some(phrase), now, tz);
            let read = resolve_reference_window(
                &format!("we talked about it {phrase}, remember"),
                tz,
                now,
            );
            // The family's invariant, on v5's own output: a past phrase is
            // written at the START of the window it is read as, and the stamp
            // lies inside the closed window; a future one reads as a window and
            // never as an event time.
            let r = read
                .as_ref()
                .expect("every rule-both phrase reads as a window");
            if r.past_pointing {
                assert_eq!(
                    written.occurred_at.as_deref(),
                    Some(r.time_range.from.as_str()),
                    "{row}"
                );
                assert_eq!(written.window.as_ref(), Some(&r.time_range), "{row}");
                let t =
                    quilltap_core::memory_scoring::date_ms(written.occurred_at.as_deref().unwrap());
                assert!(
                    t >= quilltap_core::memory_scoring::date_ms(&r.time_range.from),
                    "{row}"
                );
                assert!(
                    t <= quilltap_core::memory_scoring::date_ms(&r.time_range.to),
                    "{row}"
                );
            } else {
                assert_eq!(written, EventTime::default(), "{row}");
            }
            json!({ "written": event_json(&written), "read": resolution_json(&read) })
        }
        "event" => {
            let e = resolve_event_time(row["phrase"].as_str(), ms_of(&row["at"]), tz);
            json!({ "out": event_json(&e) })
        }
        "when" => {
            let out = resolve_when_phrase(
                row["phrase"].as_str(),
                row["anchorIso"].as_str().unwrap(),
                tz,
            );
            json!({ "out": out })
        }
        "read" => {
            let r = resolve_reference_window(row["text"].as_str().unwrap(), tz, ms_of(&row["at"]));
            json!({ "out": resolution_json(&r) })
        }
        "normalise" => {
            let range = &row["range"];
            let (from, to) = if range.is_null() {
                (None, None)
            } else {
                (
                    range.get("from").and_then(Value::as_str),
                    range.get("to").and_then(Value::as_str),
                )
            };
            json!({ "out": window_json(&normalise_time_range(from, to, tz)) })
        }
        "clock" => {
            // v4 clockFor: `getMemoryTimeZone(instance.timeZone ?? null)` with
            // the cache never set → explicit → QUILLTAP_TIMEZONE → system; v5's
            // caller resolves the zone first (`memory_time_zone`, the host
            // fallback being the reconciled env/system name).
            let system = row["system"].as_str().unwrap();
            let host_fallback = match row["env"].as_str() {
                Some(e) if is_valid_time_zone(Some(e)) => e,
                _ => system,
            };
            let instance_tz = row["instance"].get("timeZone").and_then(Value::as_str);
            let mode = row["chat"].get("timelineMode").and_then(Value::as_str);
            let c = clock_for(mode, &memory_time_zone(instance_tz, host_fallback), 5.0);
            assert_eq!(leg, system);
            json!({ "out": {"timeZone": c.time_zone, "nowMs": c.now_ms, "timelineMode": c.timeline_mode} })
        }
        "anchors" => {
            let input = &row["input"];
            let a = resolve_episodic_anchors(&EpisodicAnchorInput {
                when: input["when"].as_str(),
                reference_iso: input["referenceIso"].as_str(),
                fallback_iso: input["fallbackIso"].as_str(),
                timeline_mode: input["timelineMode"].as_str().unwrap(),
                narrative_time: input.get("narrativeTime").and_then(Value::as_str),
                // v4's absent `timeZone` → resolveWhenPhrase's default, the
                // process zone here (QUILLTAP_TIMEZONE unset, cache unset).
                time_zone: input
                    .get("timeZone")
                    .and_then(Value::as_str)
                    .unwrap_or(row["system"].as_str().unwrap()),
            });
            json!({ "out": {"occurredAt": a.occurred_at, "narrativeTime": a.narrative_time} })
        }
        other => panic!("unit-4 kind {other} has no v5 driver"),
    }
}

/// Numbers compare by value (`5` vs `5.0`), everything else structurally.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

fn v4_outputs(row: &Value) -> Value {
    let kind = row["kind"].as_str().unwrap();
    let mut out = serde_json::Map::new();
    for f in output_fields(kind) {
        out.insert(f.to_string(), row[*f].clone());
    }
    Value::Object(out)
}

/// The leg-independent identity of a row: its `id` when it has one, else its
/// inputs (the row minus `leg`, `system` and the output fields).
fn row_key(row: &Value) -> (String, String) {
    let kind = row["kind"].as_str().unwrap().to_string();
    if let Some(id) = row.get("id").and_then(Value::as_str) {
        return (kind, id.to_string());
    }
    let mut inputs = row.as_object().unwrap().clone();
    inputs.remove("leg");
    inputs.remove("system");
    for f in output_fields(&kind) {
        inputs.remove(*f);
    }
    (kind, serde_json::to_string(&inputs).unwrap())
}

fn check_rules_clock_anchors(rows: &[Value]) {
    // Sanity: the number-word pattern v4 joins is the one the table carries.
    assert_eq!(DAY_REFERENCE_TABLE[4].pattern, day_number_words_pattern());

    let mut by_key: BTreeMap<(String, String), BTreeMap<String, &Value>> = BTreeMap::new();
    for row in rows {
        let leg = row["leg"].as_str().unwrap().to_string();
        let prev = by_key.entry(row_key(row)).or_default().insert(leg, row);
        assert!(prev.is_none(), "duplicate row key {:?}", row_key(row));
    }

    let mut zoneless_measured: BTreeSet<(String, String)> = BTreeSet::new();
    let mut compared = 0usize;
    for ((kind, key), legs) in &by_key {
        let utc = legs
            .get("UTC")
            .unwrap_or_else(|| panic!("{kind} {key}: no UTC leg"));
        let chi = legs
            .get("America/Chicago")
            .unwrap_or_else(|| panic!("{kind} {key}: no Chicago leg"));
        let (v4_utc, v4_chi) = (v4_outputs(utc), v4_outputs(chi));

        if !leg_dependent(utc) && !same(&v4_utc, &v4_chi) {
            // A zone-less Date.parse row: v5 equals the UTC leg on both legs.
            zoneless_measured.insert((kind.clone(), key.clone()));
            for (leg, row) in legs {
                let v5 = v5_for(row);
                assert!(
                    same(&v5, &v4_utc),
                    "[{leg}] {kind} {key}: ZONELESS row — rust={v5} v4(UTC)={v4_utc}"
                );
            }
        } else {
            for (leg, row) in legs {
                let v5 = v5_for(row);
                let v4 = v4_outputs(row);
                assert!(
                    same(&v5, &v4),
                    "[{leg}] {kind} {key}:\n  rust={v5}\n  v4  ={v4}"
                );
            }
        }
        compared += legs.len();
    }

    let table: BTreeSet<(String, String)> = ZONELESS_PARSE_AS_UTC
        .iter()
        .map(|(k, i)| (k.to_string(), i.to_string()))
        .collect();
    assert_eq!(
        zoneless_measured, table,
        "ZONELESS_PARSE_AS_UTC drifted — rows whose v4 output differs between the TZ legs, measured vs ruled"
    );
    eprintln!(
        "OK: rules/clock/anchors — {compared} rows compared, {} zone-less.",
        table.len()
    );
}
