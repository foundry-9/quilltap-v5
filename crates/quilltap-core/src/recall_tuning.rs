//! Recall tuning — port of v4 `lib/memory/recall-tuning.ts` (`f7d8064be`, the
//! recall multiplier retuning; pinned at `01a83539d`): the knobs R1–R6 add to
//! the ranking, resolved into one object the ranking code reads.
//!
//! Every knob defaults to the values the retuning chose, so
//! [`DEFAULT_RECALL_TUNING`] and "no tuning at all" rank identically
//! ([`crate::recall_tags::recall_tuning_of`] falls back to it). The
//! pre-retuning ranking is reachable as a tuning input: both gate terms 0,
//! `boostCap` 4, fresh 1.6 / 1.35, `specificAnchors` false.
//!
//! The `chatRecallReplay` harness accepts a [`RecallTuningInput`] on its new
//! path (R7), so each constant can be tried against the probe set without a
//! code change; its saved-signals input ([`RecallReplaySignals`]) lives here
//! too, so the route can validate both without loading the replay engine.
//!
//! Pure + I/O-free, like `recall_tags`. The two validators reproduce v4's Zod
//! schemas (`RecallTuningInputSchema` — `.strict()` at both levels — and
//! `RecallReplaySignalsSchema` — unknown keys stripped) byte for byte: the
//! issue objects ride the `validationError` envelope's `details`, so their key
//! order and messages are contractual (measured against the pin's real zod,
//! `recall_tuning_equivalence`).

use serde_json::{Map, Value};

use crate::api::zod_issues::{key, ZodIssue, MAX_SAFE_INTEGER};
use crate::recall_tags::{
    BoostGate, ContextTag, RecallMultiplierTable, TemporalTag, TimeWindow, CONTEXT_VALUES,
    RECALL_MULTIPLIERS, RECALL_MULTIPLIER_KEYS, TEMPORAL_VALUES,
};

/// R4 — order among the eligible entity anchors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnchorOrder {
    /// Fewest memories first (ties keep the distiller's order).
    Rarest,
    /// The distiller's own order, which leads with the turn's subject.
    Distiller,
}

impl AnchorOrder {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rarest => "rarest",
            Self::Distiller => "distiller",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "rarest" => Some(Self::Rarest),
            "distiller" => Some(Self::Distiller),
            _ => None,
        }
    }
}

/// The resolved tuning the ranking code reads (v4 `ResolvedRecallTuning`).
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedRecallTuning {
    pub multipliers: RecallMultiplierTable,
    /// R1 — `None` when the gate is off.
    pub boost_gate: Option<BoostGate>,
    /// R2 — cap on the product of boosts (before R1 scales it).
    pub boost_cap: f64,
    /// R1 variant — the fresh-event boost applies in full whatever the gate
    /// says; only the other boosts are gated. Its job is to rescue a
    /// cross-chat memory the query embeds weakly, which is exactly what a
    /// relevance gate would strip. The R2 cap still bounds the total.
    pub fresh_bypasses_gate: bool,
    /// R1 variant — the time-window boost applies in full whatever the gate
    /// says. Like the fresh boost, it marks a memory as being from the time
    /// the turn asks about, which the query's cosine does not measure.
    pub window_bypasses_gate: bool,
    /// R4 — choose entity anchors by specificity instead of the first three.
    pub specific_anchors: bool,
    /// R4 — names in fewer memories than this are not anchors (1 drops only
    /// zero-hit names).
    pub anchor_min_hits: i64,
    /// R4 — order among the eligible names.
    pub anchor_order: AnchorOrder,
    /// R6 — fraction of the head reserved for out-of-window candidates that
    /// clear the R1 gate at full strength, when the time window is a hard
    /// filter. 0 = off. Inert while the gate is off (the gate defines
    /// "clears").
    pub background_reserve: f64,
}

/// v4 `DEFAULT_RECALL_TUNING` (`{ multipliers: {...RECALL_MULTIPLIERS},
/// ...RECALL_TUNING_DEFAULTS }`) — and, value-equal, `recallTuningOf`'s
/// fallback: ONE constant serves both. The `cap14` probe-set candidate.
pub const DEFAULT_RECALL_TUNING: ResolvedRecallTuning = ResolvedRecallTuning {
    multipliers: RECALL_MULTIPLIERS,
    boost_gate: Some(BoostGate {
        abs: Some(0.45),
        margin: Some(0.15),
        ramp: 0.1,
    }),
    boost_cap: 1.4,
    fresh_bypasses_gate: false,
    window_bypasses_gate: false,
    specific_anchors: true,
    anchor_min_hits: 1,
    anchor_order: AnchorOrder::Rarest,
    background_reserve: 0.0,
};

/// A validated `tuning` body field (v4 `RecallTuningInput`). The three gate
/// constants are TRI-STATE — absent (`None`) / `null` (`Some(None)`) / a
/// number — because v4's `gateTerm` reads absent as "the default" and `null`
/// as "off". `multipliers` keeps the override keys (validated table keys).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecallTuningInput {
    pub boost_gate_abs: Option<Option<f64>>,
    pub boost_gate_margin: Option<Option<f64>>,
    pub boost_gate_ramp: Option<Option<f64>>,
    pub boost_cap: Option<f64>,
    pub fresh_bypasses_gate: Option<bool>,
    pub window_bypasses_gate: Option<bool>,
    pub multipliers: Option<Vec<(String, f64)>>,
    pub specific_anchors: Option<bool>,
    pub anchor_min_hits: Option<i64>,
    pub anchor_order: Option<AnchorOrder>,
    pub background_reserve: Option<f64>,
}

/// A validated `signals` body field (v4 `RecallReplaySignalsSchema`'s output
/// — a saved distillation, as a previous replay returned it). Absent keys stay
/// `None`; `time_range` is tri-state (`Some(None)` is a pinned `null`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecallReplaySignals {
    pub keywords: Vec<String>,
    pub temporal: Option<TemporalTag>,
    pub context: Option<ContextTag>,
    pub paraphrase: Option<String>,
    pub retrospective: Option<bool>,
    pub time_range: Option<Option<TimeWindow>>,
    pub entities: Option<Vec<String>>,
}

/// A gate constant: absent → the default's value; 0 or `null` → "this term is
/// off" (v4 `gateTerm`).
fn gate_term(value: Option<Option<f64>>, fallback: Option<f64>) -> Option<f64> {
    match value {
        None => fallback,
        Some(Some(v)) if v > 0.0 => Some(v),
        Some(_) => None,
    }
}

/// Resolve a (validated) tuning input against the defaults (v4
/// `resolveRecallTuning`). Absent input → [`DEFAULT_RECALL_TUNING`]. The gate
/// is on when either its absolute floor or its margin is set; pass both as 0
/// to turn it off. ⚠ An EMPTY input (`{}`) is truthy in v4 and resolves to a
/// value-equal copy — the caller's "was a tuning given" test reads the input,
/// not this result.
pub fn resolve_recall_tuning(input: Option<&RecallTuningInput>) -> ResolvedRecallTuning {
    let Some(input) = input else {
        return DEFAULT_RECALL_TUNING;
    };
    let default_gate = DEFAULT_RECALL_TUNING.boost_gate;
    let abs = gate_term(input.boost_gate_abs, default_gate.and_then(|g| g.abs));
    let margin = gate_term(input.boost_gate_margin, default_gate.and_then(|g| g.margin));
    let mut multipliers = DEFAULT_RECALL_TUNING.multipliers;
    for (k, v) in input.multipliers.iter().flatten() {
        multipliers.set(k, *v);
    }
    let boost_gate = if abs.is_none() && margin.is_none() {
        None
    } else {
        Some(BoostGate {
            abs,
            margin,
            ramp: gate_term(input.boost_gate_ramp, default_gate.map(|g| g.ramp)).unwrap_or(0.0),
        })
    };
    ResolvedRecallTuning {
        multipliers,
        boost_gate,
        boost_cap: input.boost_cap.unwrap_or(DEFAULT_RECALL_TUNING.boost_cap),
        fresh_bypasses_gate: input
            .fresh_bypasses_gate
            .unwrap_or(DEFAULT_RECALL_TUNING.fresh_bypasses_gate),
        window_bypasses_gate: input
            .window_bypasses_gate
            .unwrap_or(DEFAULT_RECALL_TUNING.window_bypasses_gate),
        anchor_min_hits: input
            .anchor_min_hits
            .unwrap_or(DEFAULT_RECALL_TUNING.anchor_min_hits),
        anchor_order: input
            .anchor_order
            .unwrap_or(DEFAULT_RECALL_TUNING.anchor_order),
        specific_anchors: input
            .specific_anchors
            .unwrap_or(DEFAULT_RECALL_TUNING.specific_anchors),
        background_reserve: input
            .background_reserve
            .unwrap_or(DEFAULT_RECALL_TUNING.background_reserve),
    }
}

/// R4 — pick up to `max` entity anchors that narrow the search (v4
/// `selectSpecificAnchors`): drop names of participants present this turn
/// (the participant boost already covers them, and they match a large share
/// of the corpus) and names in fewer than `min_hits` memories (a zero-hit name
/// takes a slot and adds nothing; a one-hit name is often a passing mention
/// rather than the turn's subject). Then order the rest — `rarest` first, or
/// in the distiller's own order — with ties keeping the distiller's order.
pub fn select_specific_anchors(
    candidates: &[(String, usize)],
    present_names: &[String],
    max: usize,
    min_hits: Option<i64>,
    order: Option<AnchorOrder>,
) -> Vec<String> {
    let min_hits = min_hits.unwrap_or(1).max(1);
    let present: std::collections::HashSet<String> = present_names
        .iter()
        .map(|n| crate::jsstr::js_trim(n).to_lowercase())
        .filter(|n| !n.is_empty())
        .collect();
    let mut eligible: Vec<&(String, usize)> = candidates
        .iter()
        .filter(|(phrase, count)| {
            *count as i64 >= min_hits
                && !present.contains(&crate::jsstr::js_trim(phrase).to_lowercase())
        })
        .collect();
    if order.unwrap_or(AnchorOrder::Rarest) == AnchorOrder::Rarest {
        // Stable: equal counts keep their input (distiller) order — v4's
        // `a.count - b.count || a.index - b.index`.
        eligible.sort_by_key(|(_, count)| *count);
    }
    eligible
        .into_iter()
        .take(max)
        .map(|(phrase, _)| phrase.clone())
        .collect()
}

/// One-line summary of the non-default knobs, for logs and the CLI header (v4
/// `describeRecallTuning`). Numbers render as JS `String(n)`.
pub fn describe_recall_tuning(tuning: &ResolvedRecallTuning) -> String {
    let n = crate::pascal::js_value::number_to_string;
    let d = &DEFAULT_RECALL_TUNING;
    let mut parts: Vec<String> = Vec::new();
    // v4 compares the two gates by `JSON.stringify`; over the reachable values
    // (finite numbers or null, key order fixed) that is field equality.
    if tuning.boost_gate != d.boost_gate {
        parts.push(match &tuning.boost_gate {
            Some(g) => format!(
                "gate abs={} margin={} ramp={}",
                g.abs.map(n).unwrap_or_else(|| "off".into()),
                g.margin.map(n).unwrap_or_else(|| "off".into()),
                n(g.ramp)
            ),
            None => "gate off".into(),
        });
    }
    if tuning.boost_cap != d.boost_cap {
        parts.push(format!("cap={}", n(tuning.boost_cap)));
    }
    if tuning.fresh_bypasses_gate != d.fresh_bypasses_gate {
        parts.push(format!("freshBypassesGate={}", tuning.fresh_bypasses_gate));
    }
    if tuning.window_bypasses_gate != d.window_bypasses_gate {
        parts.push(format!(
            "windowBypassesGate={}",
            tuning.window_bypasses_gate
        ));
    }
    for k in RECALL_MULTIPLIER_KEYS {
        let (Some(v), Some(dv)) = (tuning.multipliers.get(k), d.multipliers.get(k)) else {
            continue;
        };
        if v != dv {
            parts.push(format!("{k}={}", n(v)));
        }
    }
    if tuning.specific_anchors != d.specific_anchors
        || tuning.anchor_min_hits != d.anchor_min_hits
        || tuning.anchor_order != d.anchor_order
    {
        parts.push(if tuning.specific_anchors {
            format!(
                "specificAnchors(minHits={}, {})",
                tuning.anchor_min_hits,
                tuning.anchor_order.as_str()
            )
        } else {
            "specificAnchors=false".into()
        });
    }
    if tuning.background_reserve != d.background_reserve {
        parts.push(format!(
            "backgroundReserve={}",
            n(tuning.background_reserve)
        ));
    }
    if parts.is_empty() {
        "defaults".into()
    } else {
        parts.join(", ")
    }
}

// ---------------------------------------------------------------------------
// The two request schemas (Zod 4, byte-faithful)
// ---------------------------------------------------------------------------

/// One `z.number()[.int()].min(lo).max(hi)` check at `path`, issues pushed in
/// Zod's order (the integer gate ABORTS; the safe-integer bounds then the
/// range checks CONTINUE). `None` → the field failed.
fn zod_number(
    got: &Value,
    path: &[Value],
    int: bool,
    min: f64,
    max: f64,
    issues: &mut Vec<ZodIssue>,
) -> Option<f64> {
    let at = || path.to_vec();
    let Some(n) = got.as_f64() else {
        issues.push(ZodIssue::invalid_type("number", at(), Some(got)));
        return None;
    };
    let before = issues.len();
    if int {
        if n.fract() != 0.0 {
            issues.push(ZodIssue::invalid_int_type(at(), Some(got)));
            return None;
        }
        if n > MAX_SAFE_INTEGER {
            issues.push(ZodIssue::too_big_int(at()));
        } else if n < -MAX_SAFE_INTEGER {
            issues.push(ZodIssue::too_small_int(at()));
        }
    }
    if n < min {
        issues.push(ZodIssue::too_small_number(
            crate::db::js_number_to_json(min),
            at(),
        ));
    }
    if n > max {
        issues.push(ZodIssue::too_big_number(
            crate::db::js_number_to_json(max),
            at(),
        ));
    }
    (issues.len() == before).then_some(n)
}

/// `z.boolean()` at `path`.
fn zod_bool(got: &Value, path: &[Value], issues: &mut Vec<ZodIssue>) -> Option<bool> {
    match got {
        Value::Bool(b) => Some(*b),
        _ => {
            issues.push(ZodIssue::invalid_type("boolean", path.to_vec(), Some(got)));
            None
        }
    }
}

/// `z.string()` at `path`.
fn zod_string(got: &Value, path: &[Value], issues: &mut Vec<ZodIssue>) -> Option<String> {
    match got {
        Value::String(s) => Some(s.clone()),
        _ => {
            issues.push(ZodIssue::invalid_type("string", path.to_vec(), Some(got)));
            None
        }
    }
}

/// `z.array(z.string())` at `path` — every bad element reported.
fn zod_string_array(
    got: &Value,
    path: &[Value],
    issues: &mut Vec<ZodIssue>,
) -> Option<Vec<String>> {
    let Value::Array(items) = got else {
        issues.push(ZodIssue::invalid_type("array", path.to_vec(), Some(got)));
        return None;
    };
    let before = issues.len();
    let mut out = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let mut p = path.to_vec();
        p.push(Value::from(i));
        if let Some(s) = zod_string(item, &p, issues) {
            out.push(s);
        }
    }
    (issues.len() == before).then_some(out)
}

/// `z.enum(values)` at `path` (no separate type gate — a non-string misses
/// the domain the same way).
fn zod_enum<'v>(
    got: &'v Value,
    values: &[&str],
    path: &[Value],
    issues: &mut Vec<ZodIssue>,
) -> Option<&'v str> {
    match got.as_str() {
        Some(s) if values.contains(&s) => Some(s),
        _ => {
            issues.push(ZodIssue::invalid_value(values, path.to_vec()));
            None
        }
    }
}

/// Zod 4's strict-object tail: the keys not in the shape, in INPUT order,
/// raised after the shape's own checks (continuable).
fn unrecognized(obj: &Map<String, Value>, shape: &[&str], path: Vec<Value>) -> Option<ZodIssue> {
    let extra: Vec<String> = obj
        .keys()
        .filter(|k| !shape.contains(&k.as_str()))
        .cloned()
        .collect();
    (!extra.is_empty()).then(|| ZodIssue::unrecognized_keys(extra, path))
}

const TUNING_SHAPE: [&str; 11] = [
    "boostGateAbs",
    "boostGateMargin",
    "boostGateRamp",
    "boostCap",
    "freshBypassesGate",
    "windowBypassesGate",
    "multipliers",
    "specificAnchors",
    "anchorMinHits",
    "anchorOrder",
    "backgroundReserve",
];

/// v4 `RecallTuningInputSchema.optional().safeParse(value)` over a PRESENT
/// raw value (the route's own `undefined` arm is the caller's — absent is not
/// validated). `Ok` carries the typed input and Zod's parsed output (present
/// keys only, in SHAPE order); `Err` the issue list in Zod's order — fields in
/// shape order, a nested `multipliers` object's issues at its position, the
/// object-level `unrecognized_keys` last. A non-object (`null` included) is
/// ONE `invalid_type object` issue at `[]`.
pub fn validate_recall_tuning_input(
    value: &Value,
) -> Result<(RecallTuningInput, Value), Vec<ZodIssue>> {
    let Some(obj) = value.as_object() else {
        return Err(vec![ZodIssue::invalid_type("object", vec![], Some(value))]);
    };
    let mut issues = Vec::new();
    let mut input = RecallTuningInput::default();
    let mut parsed = Map::new();
    for k in TUNING_SHAPE {
        let Some(got) = obj.get(k) else { continue };
        let path = [key(k)];
        let ok = match k {
            "boostGateAbs" | "boostGateMargin" | "boostGateRamp" => {
                // gateConstant = z.number().min(0).max(1).nullable().optional()
                let term = if got.is_null() {
                    Some(None)
                } else {
                    zod_number(got, &path, false, 0.0, 1.0, &mut issues).map(Some)
                };
                match k {
                    "boostGateAbs" => input.boost_gate_abs = term,
                    "boostGateMargin" => input.boost_gate_margin = term,
                    _ => input.boost_gate_ramp = term,
                }
                term.is_some()
            }
            "boostCap" => {
                input.boost_cap = zod_number(got, &path, false, 1.0, 10.0, &mut issues);
                input.boost_cap.is_some()
            }
            "freshBypassesGate" => {
                input.fresh_bypasses_gate = zod_bool(got, &path, &mut issues);
                input.fresh_bypasses_gate.is_some()
            }
            "windowBypassesGate" => {
                input.window_bypasses_gate = zod_bool(got, &path, &mut issues);
                input.window_bypasses_gate.is_some()
            }
            "multipliers" => {
                let Some(inner) = got.as_object() else {
                    issues.push(ZodIssue::invalid_type("object", path.to_vec(), Some(got)));
                    continue;
                };
                let before = issues.len();
                let mut overrides = Vec::new();
                let mut inner_parsed = Map::new();
                for mk in RECALL_MULTIPLIER_KEYS {
                    let Some(mv) = inner.get(mk) else { continue };
                    let mpath = [key("multipliers"), key(mk)];
                    if let Some(n) = zod_number(mv, &mpath, false, 0.0, 10.0, &mut issues) {
                        overrides.push((mk.to_string(), n));
                        inner_parsed.insert(mk.to_string(), mv.clone());
                    }
                }
                if let Some(issue) =
                    unrecognized(inner, &RECALL_MULTIPLIER_KEYS, vec![key("multipliers")])
                {
                    issues.push(issue);
                }
                if issues.len() == before {
                    input.multipliers = Some(overrides);
                    parsed.insert(k.to_string(), Value::Object(inner_parsed));
                }
                continue;
            }
            "specificAnchors" => {
                input.specific_anchors = zod_bool(got, &path, &mut issues);
                input.specific_anchors.is_some()
            }
            "anchorMinHits" => {
                input.anchor_min_hits =
                    zod_number(got, &path, true, 1.0, 100.0, &mut issues).map(|n| n as i64);
                input.anchor_min_hits.is_some()
            }
            "anchorOrder" => {
                input.anchor_order = zod_enum(got, &["rarest", "distiller"], &path, &mut issues)
                    .and_then(AnchorOrder::parse);
                input.anchor_order.is_some()
            }
            _ => {
                input.background_reserve = zod_number(got, &path, false, 0.0, 0.5, &mut issues);
                input.background_reserve.is_some()
            }
        };
        if ok {
            parsed.insert(k.to_string(), got.clone());
        }
    }
    if let Some(issue) = unrecognized(obj, &TUNING_SHAPE, vec![]) {
        issues.push(issue);
    }
    if issues.is_empty() {
        Ok((input, Value::Object(parsed)))
    } else {
        Err(issues)
    }
}

const SIGNALS_SHAPE: [&str; 7] = [
    "keywords",
    "temporal",
    "context",
    "paraphrase",
    "retrospective",
    "timeRange",
    "entities",
];

/// v4 `RecallReplaySignalsSchema.optional().safeParse(value)` over a PRESENT
/// raw value. NOT strict: unknown keys are STRIPPED at both levels (the top
/// object and `timeRange`). `Ok` carries the typed signals and Zod's parsed
/// echo (present keys only, SHAPE order — the result's `signals` when
/// pinned).
pub fn validate_recall_replay_signals(
    value: &Value,
) -> Result<(RecallReplaySignals, Value), Vec<ZodIssue>> {
    let Some(obj) = value.as_object() else {
        return Err(vec![ZodIssue::invalid_type("object", vec![], Some(value))]);
    };
    let mut issues = Vec::new();
    let mut out = RecallReplaySignals::default();
    let mut parsed = Map::new();
    for k in SIGNALS_SHAPE {
        let got = obj.get(k);
        let path = [key(k)];
        let ok_value: Option<Value> = match (k, got) {
            // `keywords` is REQUIRED — a missing key is `received undefined`.
            ("keywords", got) => match got {
                None => {
                    issues.push(ZodIssue::invalid_type("array", path.to_vec(), None));
                    None
                }
                Some(g) => zod_string_array(g, &path, &mut issues).map(|kw| {
                    out.keywords = kw;
                    g.clone()
                }),
            },
            (_, None) => continue,
            ("temporal", Some(g)) => zod_enum(g, &TEMPORAL_VALUES, &path, &mut issues).map(|s| {
                out.temporal = TemporalTag::from_kw(s);
                g.clone()
            }),
            ("context", Some(g)) => zod_enum(g, &CONTEXT_VALUES, &path, &mut issues).map(|s| {
                out.context = ContextTag::from_kw(s);
                g.clone()
            }),
            ("paraphrase", Some(g)) => zod_string(g, &path, &mut issues).map(|s| {
                out.paraphrase = Some(s);
                g.clone()
            }),
            ("retrospective", Some(g)) => zod_bool(g, &path, &mut issues).map(|b| {
                out.retrospective = Some(b);
                g.clone()
            }),
            ("timeRange", Some(Value::Null)) => {
                out.time_range = Some(None);
                Some(Value::Null)
            }
            ("timeRange", Some(g)) => match g.as_object() {
                None => {
                    issues.push(ZodIssue::invalid_type("object", path.to_vec(), Some(g)));
                    None
                }
                Some(tr) => {
                    let before = issues.len();
                    let mut fields = Vec::new();
                    for f in ["from", "to"] {
                        let fpath = [key("timeRange"), key(f)];
                        match tr.get(f) {
                            None => {
                                issues.push(ZodIssue::invalid_type("string", fpath.to_vec(), None))
                            }
                            Some(v) => {
                                if let Some(s) = zod_string(v, &fpath, &mut issues) {
                                    fields.push(s);
                                }
                            }
                        }
                    }
                    (issues.len() == before).then(|| {
                        let window = TimeWindow {
                            from: fields[0].clone(),
                            to: fields[1].clone(),
                        };
                        let mut echo = Map::new();
                        echo.insert("from".into(), Value::String(window.from.clone()));
                        echo.insert("to".into(), Value::String(window.to.clone()));
                        out.time_range = Some(Some(window));
                        Value::Object(echo)
                    })
                }
            },
            (_, Some(g)) => zod_string_array(g, &path, &mut issues).map(|e| {
                out.entities = Some(e);
                g.clone()
            }),
        };
        if let Some(v) = ok_value {
            parsed.insert(k.to_string(), v);
        }
    }
    if issues.is_empty() {
        Ok((out, Value::Object(parsed)))
    } else {
        Err(issues)
    }
}

#[cfg(test)]
mod tests {
    //! Mirrors of v4 `lib/memory/__tests__/recall-tuning.test.ts` (the pure
    //! arms; the differential `recall_tuning_equivalence` is the proof).

    use super::*;
    use serde_json::json;

    #[test]
    fn resolves_no_input_and_an_empty_input_to_the_default_tuning() {
        assert_eq!(resolve_recall_tuning(None), DEFAULT_RECALL_TUNING);
        assert_eq!(
            resolve_recall_tuning(Some(&RecallTuningInput::default())),
            DEFAULT_RECALL_TUNING
        );
        assert_eq!(RECALL_MULTIPLIERS.fresh_event_24h, 1.3);
        assert_eq!(RECALL_MULTIPLIERS.fresh_event_48h, 1.15);
    }

    #[test]
    fn treats_0_and_null_gate_constants_as_off() {
        let t = resolve_recall_tuning(Some(&RecallTuningInput {
            boost_gate_abs: Some(Some(0.0)),
            boost_gate_margin: Some(None),
            boost_gate_ramp: Some(Some(0.1)),
            ..Default::default()
        }));
        assert_eq!(t.boost_gate, None);
    }

    #[test]
    fn describes_defaults_and_the_legacy_ranking() {
        assert_eq!(describe_recall_tuning(&DEFAULT_RECALL_TUNING), "defaults");
        let (legacy, _) = validate_recall_tuning_input(&json!({
            "boostGateAbs": 0, "boostGateMargin": 0, "boostCap": 4,
            "multipliers": {"freshEvent24h": 1.6, "freshEvent48h": 1.35},
            "specificAnchors": false
        }))
        .unwrap();
        assert_eq!(
            describe_recall_tuning(&resolve_recall_tuning(Some(&legacy))),
            "gate off, cap=4, freshEvent24h=1.6, freshEvent48h=1.35, specificAnchors=false"
        );
    }

    #[test]
    fn drops_present_and_zero_hit_names_then_prefers_the_rarest() {
        let c: Vec<(String, usize)> = [
            ("Amy", 40),
            ("Steinway", 0),
            ("Tessarium", 1),
            ("Prospero", 3),
        ]
        .iter()
        .map(|(p, n)| (p.to_string(), *n))
        .collect();
        assert_eq!(
            select_specific_anchors(&c, &[" amy ".to_string()], 3, None, None),
            vec!["Tessarium", "Prospero"]
        );
    }
}
