//! Character progressions — the schema (v4 `lib/progressions/schema.ts`, 231
//! lines at `25f534c0b`).
//!
//! A **progression** is a named, bounded span of time carried by a character: a
//! start instant, an end instant, and the rules for how — and how often — its
//! state is reported back to that character at the top of a turn. A pregnancy
//! that began on 1 August and is due 1 May; a ship's cannon that takes ten
//! minutes to recharge; a fermentation that finishes in three weeks.
//!
//! ## Where they live
//!
//! Under **one reserved top-level key, `progressions`**, in the character
//! vault's `metadata.json`. That file is Pascal's one character-scoped store,
//! and the whole point of progressions is that Pascal's custom tools can read
//! and change them. This amends the metadata spec's "no reserved keys":
//! `progressions` is reserved and its shape is validated; every other key stays
//! freeform. **No table, no column, no D23 re-dump.**
//!
//! ## Fail-soft at the point of use
//!
//! Nothing validates at hydration. [`crate::progressions::parse_progressions`]
//! drops an entry that fails this schema, naming the id and the issue through a
//! sink, and keeps the rest. A broken entry must never hollow a character or
//! fail a turn.
//!
//! ## The Zod 4.5.4 sentence table (MEASURED, not transcribed)
//!
//! v4 joins each refused entry's issues as ``format!("{}: {}", path.join(".")
//! or "(root)", message)`` with `"; "`, and that joined string is what the
//! `on_issue` sink receives — P4.D168 logs it, P4.D169 warns it, so it is a
//! comparand, not a diagnostic. The sentences below were measured on
//! 2026-09-08 by running v4's REAL `ProgressionSchema.safeParse` at
//! `25f534c0b` against `zod` **4.5.4** (the version
//! `crates/quilltap-harness/tests/zod_version_guard.rs` pins; when v4's `zod`
//! moves, that guard fails the gate and this table is what a future lane
//! re-measures):
//!
//! | shape | sentence |
//! |---|---|
//! | one unknown key | `(root): Unrecognized key: "stages"` |
//! | several unknown keys | `(root): Unrecognized keys: "zeta", "alpha"` (input key order) |
//! | wrong type | `<path>: Invalid input: expected string, received number` |
//! | missing required | `<path>: Invalid input: expected string, received undefined` |
//! | string too short | `<path>: Too small: expected string to have >=1 characters` |
//! | string too long | `<path>: Too big: expected string to have <=80 characters` |
//! | number too small | `quantity.total: Too small: expected number to be >0` |
//! | number too big | `quantity.precision: Too big: expected number to be <=6` |
//! | non-integer | `quantity.precision: Invalid input: expected int, received number` |
//! | NaN | `quantity.precision: Invalid input: expected number, received NaN` |
//! | non-finite | `quantity.total: Invalid input: expected number, received Infinity` |
//! | bad enum | `timeIncrement: Invalid option: expected one of "second"\|…\|"year"` |
//! | bad regex | `reportFrequency: Invalid string: must match pattern /^(turn\|increment\|[1-9]\d{0,4}[smhdw])$/` |
//! | bad instant | `startTime: must be an ISO-8601 date-time with an offset or Z (e.g. 2026-08-01T00:00:00Z)` |
//! | the cross-field refine | `endTime: must be strictly after startTime` |
//! | a non-object entry | `(root): Invalid input: expected object, received null` |
//!
//! **The measured ISSUE ORDER** (v4 joins in Zod's own check order, and the
//! order is a comparand): field issues in **schema declaration order**, then
//! the single `(root)` unrecognized-key(s) issue, then the `superRefine`
//! issue. A non-object entry aborts: one issue, and the refine never runs —
//! and so does any `invalid_type` / `invalid_option` field issue (the refine
//! is skipped, the other field issues are not; see `parse_progression`).
//! Every length bound is Zod 4.5's **code-point** length
//! ([`crate::jsstr::zod_len_min_ok`] / [`crate::jsstr::zod_len_max_ok`]) —
//! measured: 500 astral characters (1,000 UTF-16 units) PASS `.max(500)`.
//!
//! **Measured seam:** a non-finite `total` is unreachable through the JSON
//! door on BOTH sides — `JSON.parse` has no `NaN`/`Infinity` literal and
//! `serde_json::Value` cannot hold one either — so v4's `.finite()` arm and
//! its twin below can only be reached from an in-memory object. The check is
//! ported anyway (it is free and faithful) and its sentence is recorded above;
//! the corpus exercises the reachable neighbours (`0`, `-1`, `null`, `"1"`).
//!
//! CLIENT-SAFE in v4's sense: no DB, no tracing, no I/O.

use serde::{Serialize, Serializer};
use serde_json::Value;

use crate::jsstr::{js_trim, zod_len_max_ok, zod_len_min_ok};

/// The reserved key itself, so no reader has to spell it (v4
/// `PROGRESSIONS_METADATA_KEY`).
pub const PROGRESSIONS_METADATA_KEY: &str = "progressions";

/// A ceiling, not a design constraint — a character carrying 32 timed
/// conditions is already an outlier.
pub const MAX_PROGRESSIONS_PER_CHARACTER: usize = 32;

pub const MAX_REPORT_TEMPLATE_LENGTH: usize = 500;
pub const MAX_PROGRESSION_NAME_LENGTH: usize = 80;
pub const MAX_PROGRESSION_DESCRIPTION_LENGTH: usize = 500;
pub const MAX_QUANTITY_UNIT_LENGTH: usize = 16;

/// The source spelling of v4's `PROGRESSION_ID_PATTERN`, for error sentences
/// and documentation. The rule itself is [`is_progression_id`].
pub const PROGRESSION_ID_PATTERN_SOURCE: &str = r"^[a-z][a-z0-9_-]{0,63}$";

/// The source spelling of v4's `REPORT_FREQUENCY_PATTERN` — **the exact bytes
/// Zod prints in `Invalid string: must match pattern /…/`**, so this constant
/// is a comparand, not a comment.
pub const REPORT_FREQUENCY_PATTERN_SOURCE: &str = r"^(turn|increment|[1-9]\d{0,4}[smhdw])$";

/// v4 `PROGRESSION_ID_PATTERN.test(id)` — `/^[a-z][a-z0-9_-]{0,63}$/`, one to
/// 64 ASCII characters. Hand-rolled rather than compiled because JS `$` (no
/// `m` flag) is end-of-input and `\d`/`[a-z]` are ASCII-only: a byte scan says
/// exactly that with no engine-dialect question left open.
pub fn is_progression_id(id: &str) -> bool {
    let b = id.as_bytes();
    if b.is_empty() || b.len() > 64 {
        return false;
    }
    if !b[0].is_ascii_lowercase() {
        return false;
    }
    b[1..]
        .iter()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_' || *c == b'-')
}

/// v4 `REPORT_FREQUENCY_PATTERN.test(value)` — `turn`, `increment`, or
/// `<1-5 digits, no leading zero><s|m|h|d|w>`.
pub fn is_report_frequency(value: &str) -> bool {
    if value == "turn" || value == "increment" {
        return true;
    }
    let b = value.as_bytes();
    if b.len() < 2 || b.len() > 6 {
        return false;
    }
    if !(b'1'..=b'9').contains(&b[0]) {
        return false;
    }
    if !b[1..b.len() - 1].iter().all(|c| c.is_ascii_digit()) {
        return false;
    }
    matches!(b[b.len() - 1], b's' | b'm' | b'h' | b'd' | b'w')
}

/// v4's `ISO_INSTANT_PATTERN.test(trimmed)` —
/// `/^\d{4}-\d{2}-\d{2}[Tt ]\d{2}:\d{2}(:\d{2}(\.\d{1,9})?)?([Zz]|[+-]\d{2}:?\d{2})$/`.
/// A **space** separator is admitted and a zone is **required**; the colon in a
/// numeric offset is optional, so `+0530` passes.
fn matches_iso_instant_pattern(s: &str) -> bool {
    let b = s.as_bytes();
    let d = |i: usize| b.get(i).is_some_and(u8::is_ascii_digit);
    // `\d{4}-\d{2}-\d{2}`
    if !(d(0) && d(1) && d(2) && d(3) && b.get(4) == Some(&b'-') && d(5) && d(6))
        || b.get(7) != Some(&b'-')
        || !(d(8) && d(9))
    {
        return false;
    }
    // `[Tt ]`
    if !matches!(b.get(10), Some(b'T') | Some(b't') | Some(b' ')) {
        return false;
    }
    // `\d{2}:\d{2}`
    if !(d(11) && d(12)) || b.get(13) != Some(&b':') || !(d(14) && d(15)) {
        return false;
    }
    let mut i = 16;
    // `(:\d{2}(\.\d{1,9})?)?`
    if b.get(i) == Some(&b':') {
        if !(d(i + 1) && d(i + 2)) {
            return false;
        }
        i += 3;
        if b.get(i) == Some(&b'.') {
            let start = i + 1;
            let mut j = start;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            let digits = j - start;
            if !(1..=9).contains(&digits) {
                return false;
            }
            i = j;
        }
    }
    // `([Zz]|[+-]\d{2}:?\d{2})$`
    match b.get(i) {
        Some(b'Z') | Some(b'z') => i + 1 == b.len(),
        Some(b'+') | Some(b'-') => {
            if !(d(i + 1) && d(i + 2)) {
                return false;
            }
            let mut k = i + 3;
            if b.get(k) == Some(&b':') {
                k += 1;
            }
            d(k) && d(k + 1) && k + 2 == b.len()
        }
        _ => false,
    }
}

/// v4 `parseIsoInstant(value)` → epoch milliseconds, or `NaN` (`None` here).
///
/// Deliberately stricter than `new Date(x)`: a progression's whole value is
/// that its arithmetic is deterministic, and a timestamp with no zone is not an
/// instant. Non-string → `None`; JS `trim()`
/// ([`crate::jsstr::js_trim`] — the JS whitespace set, which includes
/// U+00A0); the pattern above; then `Date.parse`
/// ([`crate::episodic::js_date_parse_ms`], the V8-faithful twin, which does not
/// take the space separator — so the space form is rewritten to `T` first, as
/// `format_time::almanack_date_parse_ms` does).
pub fn parse_iso_instant(value: &Value) -> Option<i64> {
    let s = value.as_str()?;
    parse_iso_instant_str(s)
}

/// [`parse_iso_instant`] for a value already known to be a string.
pub fn parse_iso_instant_str(value: &str) -> Option<i64> {
    let trimmed = js_trim(value);
    if !matches_iso_instant_pattern(trimmed) {
        return None;
    }
    if trimmed.as_bytes().get(10) == Some(&b' ') {
        let mut t_form = String::with_capacity(trimmed.len());
        t_form.push_str(&trimmed[..10]);
        t_form.push('T');
        t_form.push_str(&trimmed[11..]);
        return crate::episodic::js_date_parse_ms(&t_form);
    }
    crate::episodic::js_date_parse_ms(trimmed)
}

/// The unit a report *speaks in*. It never affects computation, only phrasing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TimeIncrement {
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Year,
}

impl TimeIncrement {
    /// v4's enum members, in declaration order — the order Zod prints in
    /// `Invalid option: expected one of …`.
    pub const ALL: [TimeIncrement; 7] = [
        TimeIncrement::Second,
        TimeIncrement::Minute,
        TimeIncrement::Hour,
        TimeIncrement::Day,
        TimeIncrement::Week,
        TimeIncrement::Month,
        TimeIncrement::Year,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            TimeIncrement::Second => "second",
            TimeIncrement::Minute => "minute",
            TimeIncrement::Hour => "hour",
            TimeIncrement::Day => "day",
            TimeIncrement::Week => "week",
            TimeIncrement::Month => "month",
            TimeIncrement::Year => "year",
        }
    }

    pub fn parse(s: &str) -> Option<TimeIncrement> {
        TimeIncrement::ALL.into_iter().find(|u| u.as_str() == s)
    }
}

/// What happens once `endTime` has passed: `keep` goes on reporting on the same
/// cadence; `once` reports the completion exactly on the first turn after it
/// happened, then goes silent (the entry stays in the file, still readable by
/// Pascal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OnComplete {
    Keep,
    Once,
}

impl OnComplete {
    pub const ALL: [OnComplete; 2] = [OnComplete::Keep, OnComplete::Once];

    pub fn as_str(self) -> &'static str {
        match self {
            OnComplete::Keep => "keep",
            OnComplete::Once => "once",
        }
    }

    pub fn parse(s: &str) -> Option<OnComplete> {
        OnComplete::ALL.into_iter().find(|u| u.as_str() == s)
    }
}

/// One Zod issue: the path it sits at and the message. The joined form
/// ([`join_issues`]) is what the `on_issue` sink receives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZodIssue {
    pub path: Vec<String>,
    pub message: String,
}

impl ZodIssue {
    fn root(message: impl Into<String>) -> ZodIssue {
        ZodIssue {
            path: Vec::new(),
            message: message.into(),
        }
    }

    fn at(path: &[&str], message: impl Into<String>) -> ZodIssue {
        ZodIssue {
            path: path.iter().map(|s| (*s).to_string()).collect(),
            message: message.into(),
        }
    }

    /// v4 ```${i.path.join('.') || '(root)'}: ${i.message}` ``.
    pub fn rendered(&self) -> String {
        let path = self.path.join(".");
        let head = if path.is_empty() { "(root)" } else { &path };
        format!("{head}: {}", self.message)
    }
}

/// v4 ``issues.map(…).join('; ')`` — the sentence the sink is handed.
pub fn join_issues(issues: &[ZodIssue]) -> String {
    issues
        .iter()
        .map(ZodIssue::rendered)
        .collect::<Vec<_>>()
        .join("; ")
}

/// An optional scalar the span fills, so a recharge can be reported in
/// megajoules rather than only in percent. `current` is derived —
/// `total × clamp(percent) / 100` — never stored.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProgressionQuantity {
    #[serde(serialize_with = "serialize_js_number")]
    pub total: f64,
    pub unit: String,
    pub precision: u32,
}

/// Serialize an `f64` the way `JSON.stringify` writes a JS number: an integral
/// value carries no decimal point, so `{ total: 1 }` round-trips as v4 writes
/// it rather than as `1.0`.
fn serialize_js_number<S: Serializer>(x: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    if x.fract() == 0.0 && x.abs() < 9_007_199_254_740_992.0 {
        serializer.serialize_i64(*x as i64)
    } else {
        serializer.serialize_f64(*x)
    }
}

/// A parsed progression — v4's `z.infer<typeof ProgressionSchema>`, with the
/// defaults already applied. The field order below is v4's **declaration**
/// order, which is also the key order `strictObject` emits (measured) and the
/// order [`Serialize`] writes; the WRITE path (P4.D169's applier) mutates the
/// raw `serde_json::Map` instead, so a write never round-trips through this
/// struct and cannot reorder a user's file.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progression {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub start_time: String,
    pub end_time: String,
    pub time_increment: TimeIncrement,
    pub percentage_report: bool,
    pub report_frequency: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<ProgressionQuantity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_template: Option<String>,
    pub on_complete: OnComplete,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// The JS `typeof`-ish name Zod 4.5 prints after `received `.
fn received(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// `Invalid input: expected <want>, received <got>` — Zod 4.5's invalid-type
/// sentence, with `undefined` for an absent key.
fn invalid_type(want: &str, got: Option<&Value>) -> String {
    let received = got.map(received).unwrap_or("undefined");
    format!("Invalid input: expected {want}, received {received}")
}

/// The enum sentence: `Invalid option: expected one of "a"|"b"`.
fn invalid_option(options: &[&str]) -> String {
    let quoted = options
        .iter()
        .map(|o| format!("\"{o}\""))
        .collect::<Vec<_>>()
        .join("|");
    format!("Invalid option: expected one of {quoted}")
}

const ISO_MESSAGE: &str =
    "must be an ISO-8601 date-time with an offset or Z (e.g. 2026-08-01T00:00:00Z)";

/// Read one required/optional string field with Zod's `min`/`max` checks.
/// Returns `Ok(None)` when the key is absent and optional.
fn read_string(
    object: &serde_json::Map<String, Value>,
    key: &str,
    path: &[&str],
    required: bool,
    min: Option<usize>,
    max: Option<usize>,
    issues: &mut Vec<ZodIssue>,
) -> Option<String> {
    match object.get(key) {
        None => {
            if required {
                issues.push(ZodIssue::at(path, invalid_type("string", None)));
            }
            None
        }
        Some(Value::String(s)) => {
            if let Some(min) = min {
                if !zod_len_min_ok(s, min) {
                    issues.push(ZodIssue::at(
                        path,
                        format!("Too small: expected string to have >={min} characters"),
                    ));
                    return None;
                }
            }
            if let Some(max) = max {
                if !zod_len_max_ok(s, max) {
                    issues.push(ZodIssue::at(
                        path,
                        format!("Too big: expected string to have <={max} characters"),
                    ));
                    return None;
                }
            }
            Some(s.clone())
        }
        Some(other) => {
            issues.push(ZodIssue::at(path, invalid_type("string", Some(other))));
            None
        }
    }
}

/// An ISO-instant string field: the string checks, then v4's `refine`.
fn read_instant(
    object: &serde_json::Map<String, Value>,
    key: &str,
    required: bool,
    issues: &mut Vec<ZodIssue>,
) -> Option<String> {
    let s = read_string(object, key, &[key], required, None, None, issues)?;
    if parse_iso_instant_str(&s).is_none() {
        issues.push(ZodIssue::at(&[key], ISO_MESSAGE));
        return None;
    }
    Some(s)
}

/// A `z.number()` field with the optional `int`/`finite`/bound checks Zod runs
/// in declaration order. `serde_json` cannot hold a non-finite number, so the
/// `finite` arm is written for completeness and is unreachable from JSON — see
/// the module doc's measured seam.
#[allow(clippy::too_many_arguments)]
fn read_number(
    object: &serde_json::Map<String, Value>,
    key: &str,
    path: &[&str],
    required: bool,
    int: bool,
    finite: bool,
    bounds: &[(&str, f64)],
    issues: &mut Vec<ZodIssue>,
) -> Option<f64> {
    let value = match object.get(key) {
        None => {
            if required {
                issues.push(ZodIssue::at(path, invalid_type("number", None)));
            }
            return None;
        }
        // `unwrap_or` is unreachable without `arbitrary_precision`: a
        // `serde_json::Number` is an i64/u64/f64 and always converts. A `NaN`
        // would fall into the NaN arm below, which is v4's own answer for one.
        Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
        Some(other) => {
            issues.push(ZodIssue::at(path, invalid_type("number", Some(other))));
            return None;
        }
    };
    if value.is_nan() {
        issues.push(ZodIssue::at(
            path,
            "Invalid input: expected number, received NaN",
        ));
        return None;
    }
    if int && value.fract() != 0.0 {
        issues.push(ZodIssue::at(
            path,
            "Invalid input: expected int, received number",
        ));
        return None;
    }
    if finite && !value.is_finite() {
        let word = if value > 0.0 { "Infinity" } else { "-Infinity" };
        issues.push(ZodIssue::at(
            path,
            format!("Invalid input: expected number, received {word}"),
        ));
        return None;
    }
    for (op, bound) in bounds {
        let ok = match *op {
            ">" => value > *bound,
            ">=" => value >= *bound,
            "<=" => value <= *bound,
            _ => unreachable!("unknown numeric bound operator"),
        };
        if !ok {
            let (word, rendered) = match *op {
                ">" | ">=" => ("Too small", format!("expected number to be {op}{bound}")),
                _ => ("Too big", format!("expected number to be {op}{bound}")),
            };
            issues.push(ZodIssue::at(path, format!("{word}: {rendered}")));
            return None;
        }
    }
    Some(value)
}

/// v4's `ProgressionQuantitySchema` — a `strictObject`.
fn read_quantity(
    object: &serde_json::Map<String, Value>,
    issues: &mut Vec<ZodIssue>,
) -> Option<ProgressionQuantity> {
    let raw = match object.get("quantity") {
        None => return None,
        Some(Value::Object(map)) => map,
        Some(other) => {
            issues.push(ZodIssue::at(
                &["quantity"],
                invalid_type("object", Some(other)),
            ));
            return None;
        }
    };
    let before = issues.len();
    let total = read_number(
        raw,
        "total",
        &["quantity", "total"],
        true,
        false,
        true,
        &[(">", 0.0)],
        issues,
    );
    let unit = read_string(
        raw,
        "unit",
        &["quantity", "unit"],
        true,
        Some(1),
        Some(MAX_QUANTITY_UNIT_LENGTH),
        issues,
    );
    let precision = read_number(
        raw,
        "precision",
        &["quantity", "precision"],
        false,
        true,
        false,
        &[(">=", 0.0), ("<=", 6.0)],
        issues,
    );
    let unknown = unknown_keys(raw, &["total", "unit", "precision"]);
    if !unknown.is_empty() {
        issues.push(ZodIssue::at(&["quantity"], unrecognized(&unknown)));
    }
    if issues.len() != before {
        return None;
    }
    Some(ProgressionQuantity {
        total: total?,
        unit: unit?,
        precision: precision.unwrap_or(1.0) as u32,
    })
}

/// The keys of `object` not in `known`, in the object's own order.
fn unknown_keys(object: &serde_json::Map<String, Value>, known: &[&str]) -> Vec<String> {
    object
        .keys()
        .filter(|k| !known.contains(&k.as_str()))
        .cloned()
        .collect()
}

/// Zod 4.5's `strictObject` sentence — singular for one key, plural for more,
/// keys in the input's own order (measured).
fn unrecognized(keys: &[String]) -> String {
    let quoted = keys
        .iter()
        .map(|k| format!("\"{k}\""))
        .collect::<Vec<_>>()
        .join(", ");
    if keys.len() == 1 {
        format!("Unrecognized key: {quoted}")
    } else {
        format!("Unrecognized keys: {quoted}")
    }
}

const KNOWN_KEYS: [&str; 11] = [
    "name",
    "description",
    "startTime",
    "endTime",
    "timeIncrement",
    "percentageReport",
    "reportFrequency",
    "quantity",
    "reportTemplate",
    "onComplete",
    "updatedAt",
];

/// v4 `ProgressionSchema.safeParse(entry)`.
///
/// The issue order is Zod's own, measured at 4.5.4: the field checks in schema
/// **declaration** order, then the single `(root)` unrecognized-key(s) issue,
/// then the `superRefine`. A non-object aborts with one issue and never reaches
/// the refine.
pub fn parse_progression(entry: &Value) -> Result<Progression, Vec<ZodIssue>> {
    let object = match entry {
        Value::Object(map) => map,
        other => {
            return Err(vec![ZodIssue::root(invalid_type("object", Some(other)))]);
        }
    };

    let mut issues: Vec<ZodIssue> = Vec::new();

    let name = read_string(
        object,
        "name",
        &["name"],
        true,
        Some(1),
        Some(MAX_PROGRESSION_NAME_LENGTH),
        &mut issues,
    );
    let description = read_string(
        object,
        "description",
        &["description"],
        false,
        None,
        Some(MAX_PROGRESSION_DESCRIPTION_LENGTH),
        &mut issues,
    );
    let start_time = read_instant(object, "startTime", true, &mut issues);
    let end_time = read_instant(object, "endTime", true, &mut issues);

    let time_increment = match object.get("timeIncrement").and_then(Value::as_str) {
        Some(s) => match TimeIncrement::parse(s) {
            Some(u) => Some(u),
            None => {
                issues.push(ZodIssue::at(
                    &["timeIncrement"],
                    invalid_option(&TimeIncrement::ALL.map(TimeIncrement::as_str)),
                ));
                None
            }
        },
        None => {
            issues.push(ZodIssue::at(
                &["timeIncrement"],
                invalid_option(&TimeIncrement::ALL.map(TimeIncrement::as_str)),
            ));
            None
        }
    };

    let percentage_report = match object.get("percentageReport") {
        None => Some(true),
        Some(Value::Bool(b)) => Some(*b),
        Some(other) => {
            issues.push(ZodIssue::at(
                &["percentageReport"],
                invalid_type("boolean", Some(other)),
            ));
            None
        }
    };

    let report_frequency = match object.get("reportFrequency") {
        None => Some("turn".to_string()),
        Some(Value::String(s)) => {
            if is_report_frequency(s) {
                Some(s.clone())
            } else {
                issues.push(ZodIssue::at(
                    &["reportFrequency"],
                    format!(
                        "Invalid string: must match pattern /{REPORT_FREQUENCY_PATTERN_SOURCE}/"
                    ),
                ));
                None
            }
        }
        Some(other) => {
            issues.push(ZodIssue::at(
                &["reportFrequency"],
                invalid_type("string", Some(other)),
            ));
            None
        }
    };

    let quantity_before = issues.len();
    let quantity = read_quantity(object, &mut issues);
    let quantity_ok = issues.len() == quantity_before;

    let report_template = read_string(
        object,
        "reportTemplate",
        &["reportTemplate"],
        false,
        Some(1),
        Some(MAX_REPORT_TEMPLATE_LENGTH),
        &mut issues,
    );

    let on_complete = match object.get("onComplete") {
        None => Some(OnComplete::Keep),
        Some(Value::String(s)) => match OnComplete::parse(s) {
            Some(v) => Some(v),
            None => {
                issues.push(ZodIssue::at(
                    &["onComplete"],
                    invalid_option(&OnComplete::ALL.map(OnComplete::as_str)),
                ));
                None
            }
        },
        Some(_) => {
            issues.push(ZodIssue::at(
                &["onComplete"],
                invalid_option(&OnComplete::ALL.map(OnComplete::as_str)),
            ));
            None
        }
    };

    let updated_at = read_instant(object, "updatedAt", false, &mut issues);

    let unknown = unknown_keys(object, &KNOWN_KEYS);
    if !unknown.is_empty() {
        issues.push(ZodIssue::root(unrecognized(&unknown)));
    }

    // v4's `superRefine`, which runs after the object checks and guards on
    // both instants parsing, so an unparseable `startTime` silences it.
    //
    // MEASURED at `25f534c0b` on Zod 4.5.4 (the unification review, 2026-09-09
    // — the first shape of this port ran the refine unconditionally, and the
    // corpus was blind to it because every abort row carried a valid span):
    // Zod skips an object-level refine once the payload carries a
    // NON-CONTINUABLE issue. Non-continuable here means `invalid_type` (the
    // `Invalid input: expected …` family — wrong type, missing required,
    // `NaN`/`Infinity`, a fractional `int`) and `invalid_value` (the two
    // enums' `Invalid option: …`). `too_small`, `too_big`, `invalid_format`,
    // `custom` (a bad instant) and — since 4.5 — `unrecognized_keys` all
    // CONTINUE, and the refine still speaks beside them. Every field check
    // still runs either way; only this refine is gated.
    let aborted = issues.iter().any(|i| {
        i.message.starts_with("Invalid input: expected") || i.message.starts_with("Invalid option:")
    });
    let start_ms = object.get("startTime").and_then(parse_iso_instant);
    let end_ms = object.get("endTime").and_then(parse_iso_instant);
    if let (false, Some(start), Some(end)) = (aborted, start_ms, end_ms) {
        if end <= start {
            issues.push(ZodIssue::at(
                &["endTime"],
                "must be strictly after startTime",
            ));
        }
    }

    if !issues.is_empty() {
        return Err(issues);
    }

    Ok(Progression {
        name: name.expect("name is present when no issue was raised"),
        description,
        start_time: start_time.expect("startTime is present when no issue was raised"),
        end_time: end_time.expect("endTime is present when no issue was raised"),
        time_increment: time_increment.expect("timeIncrement parses when no issue was raised"),
        percentage_report: percentage_report.expect("percentageReport defaults when absent"),
        report_frequency: report_frequency.expect("reportFrequency defaults when absent"),
        quantity: if quantity_ok { quantity } else { None },
        report_template,
        on_complete: on_complete.expect("onComplete defaults when absent"),
        updated_at,
    })
}

/// v4 `parseProgressKey(key)` — split a `"<id>.<field>"` progress key into its
/// halves at the FIRST dot, or say why it is not one. The single parser for
/// that shape, so the identifier rule lives in exactly one place. The `<field>`
/// half is checked for shape only, deliberately: which derived fields exist is
/// the engine's business, and an unknown one fails soft at run time exactly as
/// an absent metadata key does.
pub fn parse_progress_key(key: &str) -> Result<(String, String), String> {
    let Some(dot) = key.find('.') else {
        return Err(format!(
            "\"{key}\" names no field — write \"<progression id>.<field>\", e.g. \"cannon.complete\""
        ));
    };
    let id = &key[..dot];
    let field = &key[dot + 1..];
    if !is_progression_id(id) {
        return Err(format!(
            "\"{id}\" is not a progression id — lowercase, starting with a letter, then letters, digits, _ or - (at most 64)"
        ));
    }
    if field.is_empty() || !field.bytes().all(|c| c.is_ascii_alphabetic()) {
        return Err(format!("\"{field}\" is not a progression field name"));
    }
    Ok((id.to_string(), field.to_string()))
}

/// The fields a Pascal effect may write, plus the `remove` pseudo-field, in
/// v4's order. `updatedAt` is deliberately NOT here — the applier stamps it.
pub const WRITABLE_PROGRESSION_FIELDS: [&str; 13] = [
    "name",
    "description",
    "startTime",
    "endTime",
    "timeIncrement",
    "percentageReport",
    "reportFrequency",
    "onComplete",
    "reportTemplate",
    "quantity.total",
    "quantity.unit",
    "quantity.precision",
    "remove",
];

pub fn is_writable_progression_field(field: &str) -> bool {
    WRITABLE_PROGRESSION_FIELDS.contains(&field)
}

impl Progression {
    /// The record as JSON, in v4's `strictObject` key order with absent
    /// optionals omitted — what `JSON.stringify(ProgressionSchema.parse(x))`
    /// writes. The WRITE path never uses it (P4.D169's applier mutates the raw
    /// map), so this is the READ/serialize side only.
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("a Progression always serializes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The Tier-2 key-order pin. The differential compares this order against
    /// v4's `JSON.stringify`, but it SKIPs without its oracle — and a
    /// key-sorting comparison elsewhere would leave a binding order unpinned
    /// (the P4.D163 §3 lesson). So pin the raw serialization here too.
    #[test]
    fn to_value_emits_v4s_declaration_key_order() {
        let full = parse_progression(&json!({
            "name": "Cannon recharge",
            "description": "You are carrying a child.",
            "startTime": "2026-09-08T14:00:00Z",
            "endTime": "2026-09-08T14:10:00Z",
            "timeIncrement": "minute",
            "percentageReport": false,
            "reportFrequency": "1h",
            "quantity": { "total": 1.0, "unit": "MJ", "precision": 1 },
            "reportTemplate": "t",
            "onComplete": "once",
            "updatedAt": "2026-09-08T14:02:10Z",
        }))
        .expect("the fixture parses");
        let value = full.to_value();
        let keys: Vec<&str> = value
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "name",
                "description",
                "startTime",
                "endTime",
                "timeIncrement",
                "percentageReport",
                "reportFrequency",
                "quantity",
                "reportTemplate",
                "onComplete",
                "updatedAt",
            ]
        );
        // `total: 1` — an integral JS number carries no decimal point, so a
        // round trip writes what v4 writes.
        assert_eq!(value["quantity"]["total"], json!(1));
        assert_eq!(
            serde_json::to_string(&value["quantity"]).unwrap(),
            r#"{"total":1,"unit":"MJ","precision":1}"#
        );

        // Absent optionals are OMITTED, not `null`.
        let minimal = parse_progression(&json!({
            "name": "Cannon recharge",
            "startTime": "2026-09-08T14:00:00Z",
            "endTime": "2026-09-08T14:10:00Z",
            "timeIncrement": "minute",
        }))
        .expect("the fixture parses");
        let minimal_value = minimal.to_value();
        let keys: Vec<&str> = minimal_value
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "name",
                "startTime",
                "endTime",
                "timeIncrement",
                "percentageReport",
                "reportFrequency",
                "onComplete",
            ]
        );
    }

    /// The three `parseProgressKey` sentences, byte-exact — P4.D169's
    /// load-time refusals quote them.
    #[test]
    fn parse_progress_key_sentences_are_byte_exact() {
        assert_eq!(
            parse_progress_key("cannon.complete"),
            Ok(("cannon".to_string(), "complete".to_string()))
        );
        assert_eq!(
            parse_progress_key("cannon").unwrap_err(),
            "\"cannon\" names no field — write \"<progression id>.<field>\", e.g. \"cannon.complete\""
        );
        assert_eq!(
            parse_progress_key("Cannon.complete").unwrap_err(),
            "\"Cannon\" is not a progression id — lowercase, starting with a letter, then letters, digits, _ or - (at most 64)"
        );
        assert_eq!(
            parse_progress_key("cannon.a-b").unwrap_err(),
            "\"a-b\" is not a progression field name"
        );
        // Split at the FIRST dot.
        assert_eq!(
            parse_progress_key("cannon.quantity.total").unwrap_err(),
            "\"quantity.total\" is not a progression field name"
        );
    }

    /// The writable set, and the two things deliberately outside it.
    #[test]
    fn writable_fields_exclude_updated_at_and_the_derived_names() {
        assert_eq!(WRITABLE_PROGRESSION_FIELDS.len(), 13);
        for field in WRITABLE_PROGRESSION_FIELDS {
            assert!(is_writable_progression_field(field), "{field}");
        }
        for field in [
            "updatedAt",
            "percent",
            "complete",
            "elapsed",
            "quantity",
            "id",
            "",
        ] {
            assert!(!is_writable_progression_field(field), "{field}");
        }
    }

    /// A non-finite `quantity.total` cannot arrive through the JSON door — see
    /// the module doc's measured seam. `serde_json` refuses the document where
    /// `JSON.parse` yields `Infinity`, so the `.finite()` arm below it is
    /// unreachable from a `metadata.json` on either side.
    #[test]
    fn a_non_finite_total_cannot_reach_the_parser_from_json() {
        assert!(serde_json::from_str::<Value>("1e400").is_err());
        assert!(serde_json::Number::from_f64(f64::INFINITY).is_none());
        assert!(serde_json::Number::from_f64(f64::NAN).is_none());
    }
}
