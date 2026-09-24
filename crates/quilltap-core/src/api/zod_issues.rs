//! The ONE home for Zod 4 issue objects — the type, its constructors, the
//! `parsedType` word, the uuid gate, and the two renderers (`ZodError.message`
//! and the `details` array).
//!
//! # Why the bytes here are contractual
//!
//! v4 answers a failed `schema.parse` in one of two shapes, and both put Zod's
//! own issue objects on the wire verbatim:
//!
//! - `validationError(err)` → `400 {error: 'Validation error', details:
//!   err.issues}` (`lib/api/responses.ts:108-119`), and
//! - a `ZodError` that escapes to `getErrorMessage`, whose `.message` IS
//!   `JSON.stringify(err.issues, null, 2)` and whose `.includes('Invalid')`
//!   status test turns it into a 400 carrying that whole string.
//!
//! So an issue's KEY ORDER is part of the response, not an implementation
//! detail — and each code orders its keys DIFFERENTLY. The variants below are
//! untagged structs rather than one struct of optional keys precisely because
//! `Option` skipping cannot reorder: `invalid_type` leads with `expected`,
//! `invalid_format` and the size issues with `origin`, `invalid_value` with
//! `code`, and the safe-integer bounds with `code` before `origin`.
//!
//! # Provenance of every shape
//!
//! Measured against the v4 checkout's real `zod` **4.5.4** at the `89fcc3c0d`
//! baseline pin, not inferred from Zod's source. The command, verbatim:
//!
//! ```text
//! cd /tmp/qt-v4-pin-p4101-89fcc3c0d && node --input-type=module -e '
//! import { z } from "zod";
//! const show = (label, schema, input) => {
//!   const r = schema.safeParse(input);
//!   for (const i of r.error.issues)
//!     console.log(label, JSON.stringify(Object.keys(i)), JSON.stringify(i));
//! };
//! show("invalid_type/string",   z.object({ a: z.string() }),         { a: 42 });
//! show("invalid_type/int",      z.object({ a: z.int() }),            { a: 1.5 });
//! show("invalid_value/enum",    z.object({ a: z.enum(["x","y"]) }),  { a: "z" });
//! show("invalid_value/literal", z.object({ a: z.literal("C") }),     { a: "n" });
//! show("invalid_format/uuid",   z.object({ a: z.uuid() }),           { a: "n" });
//! show("too_small/string",      z.object({ a: z.string().min(1) }),  { a: "" });
//! show("too_big/string",        z.object({ a: z.string().max(2) }),  { a: "abcd" });
//! show("too_small/number",      z.object({ a: z.number().min(0) }),  { a: -1 });
//! show("too_big/number",        z.object({ a: z.number().max(1) }),  { a: 2 });
//! show("too_big/int-safeint",   z.object({ a: z.int() }),            { a: 1e300 });
//! show("too_small/int-safeint", z.object({ a: z.int() }),            { a: -1e300 });
//! '
//! ```
//!
//! Its output is transcribed into [`tests::render_table_matches_real_zod_454`],
//! one row per code — so a future Zod bump that reorders a key set reddens here
//! first rather than on a route's wire.
//!
//! # The eight copies this replaces (P4.101)
//!
//! `api/settings.rs`, `api/generators_detail.rs`, `api/generators_wizard.rs`,
//! `api/prompt_templates.rs`, `api/subprompts.rs`, `services/chat_create.rs`,
//! `image_gen/lora_validation.rs` and `api/image_profiles.rs` each grew their
//! own `invalid_type` as their route was ported, in three carrier shapes
//! (a typed enum, a bare `serde_json::Value`, and two differently-named typed
//! enums) and three `path` representations (`Vec<String>`, `Vec<Value>`,
//! `&[Value]`). **All eight agreed on key order** — measured before the fold,
//! against the table above — so this fold moved no byte on any wire; the proof
//! is every family that pins those envelopes re-run unchanged at the baseline
//! pin. The remaining definitions are the Tier-3 remainder recorded in
//! `quilltap-harness/tests/zod_issues_home_guard.rs`: `pascal/
//! custom_tool_types.rs` and `progressions/schema.rs` render STRING sentences
//! for different v4 surfaces (and the latter's shape is mirrored 1:1 by the
//! SPA's `pascal/zod-shim.ts`, so converging it would desynchronize the twin).
//!
//! Zod's `path` is `(string | number)[]` — array indices ride as JSON numbers —
//! so the home carries `Vec<Value>` and lets `serde_json` render each element
//! as JSON does.

use serde_json::{json, Value};

/// One Zod 4 issue, serialized in Zod's own per-code key order.
///
/// Untagged: the wire shape is the variant's fields, nothing more.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(untagged)]
pub enum ZodIssue {
    /// `z.string()` / `z.boolean()` / `z.number()` / `z.array()` / `z.object()`
    /// — `expected, code, path, message`.
    InvalidType {
        expected: &'static str,
        code: &'static str,
        path: Vec<Value>,
        message: String,
    },
    /// `z.number().int()`'s own type failure — Zod 4 reports the `safeint`
    /// format alongside `expected: "int"`, and puts `format` BEFORE `code`.
    InvalidIntType {
        expected: &'static str,
        format: &'static str,
        code: &'static str,
        path: Vec<Value>,
        message: String,
    },
    /// `z.enum([...])` and `z.literal(...)` — both report `invalid_value`, and
    /// both do so for a wrong TYPE as well as an out-of-domain value (the enum
    /// compares values; it has no separate type gate).
    InvalidValue {
        code: &'static str,
        values: Vec<Value>,
        path: Vec<Value>,
        message: String,
    },
    /// `z.uuid()` / `z.string().uuid()` — `origin, code, format, pattern, path,
    /// message`, the source pattern echoed verbatim as a VALUE.
    InvalidFormat {
        origin: &'static str,
        code: &'static str,
        format: &'static str,
        pattern: &'static str,
        path: Vec<Value>,
        message: String,
    },
    /// `.min(n)` on a string or a number — `origin, code, minimum, inclusive,
    /// path, message`.
    TooSmall {
        origin: &'static str,
        code: &'static str,
        minimum: Value,
        inclusive: bool,
        path: Vec<Value>,
        message: String,
    },
    /// `.max(n)` on a string or a number.
    TooBig {
        origin: &'static str,
        code: &'static str,
        maximum: Value,
        inclusive: bool,
        path: Vec<Value>,
        message: String,
    },
    /// The safe-integer FLOOR of `z.number().int()`, which carries Zod's `note`
    /// and puts `code` before `origin`. It does NOT abort the checks that
    /// follow it.
    TooSmallInt {
        code: &'static str,
        minimum: Value,
        note: &'static str,
        origin: &'static str,
        inclusive: bool,
        path: Vec<Value>,
        message: String,
    },
    /// The safe-integer CEILING — the mirror of [`Self::TooSmallInt`].
    TooBigInt {
        code: &'static str,
        maximum: Value,
        note: &'static str,
        origin: &'static str,
        inclusive: bool,
        path: Vec<Value>,
        message: String,
    },
    // === P4.D217 ===
    /// A `.refine(fn, { message })` failure — `code, path, message`, nothing
    /// else (measured against real zod 4.6.5 at the `d1c06cd9d` pin through
    /// `scenarioBuildRequestSchema`'s root refine: `{"code":"custom","path":[],
    /// "message":"priorDraft and revision travel together"}`).
    Custom {
        code: &'static str,
        path: Vec<Value>,
        message: String,
    },
    // === end P4.D217 ===
}

/// JS `Number.MAX_SAFE_INTEGER` — the bound `z.number().int()` reports as
/// `format: "safeint"`. The `f64` form is what the range CHECK compares
/// against; the `i64` form is what the issue BODY carries (JSON renders it
/// without a fractional part, as v8 does).
pub const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;
/// [`MAX_SAFE_INTEGER`] as the integer the issue body and its sentence carry.
pub const MAX_SAFE_INTEGER_I64: i64 = 9_007_199_254_740_991;

/// Zod's `note` on both safe-integer bounds, verbatim.
const SAFE_INTEGER_NOTE: &str = "Integers must be within the safe integer range.";

impl ZodIssue {
    /// A plain type miss. `got` is the value AS FOUND — `None` is JS
    /// `undefined`, i.e. a missing key.
    pub fn invalid_type(expected: &'static str, path: Vec<Value>, got: Option<&Value>) -> Self {
        Self::InvalidType {
            expected,
            code: "invalid_type",
            path,
            message: format!(
                "Invalid input: expected {expected}, received {}",
                zod_parsed_type(got)
            ),
        }
    }

    /// `z.int()` over a number that is not a safe integer.
    pub fn invalid_int_type(path: Vec<Value>, got: Option<&Value>) -> Self {
        Self::InvalidIntType {
            expected: "int",
            format: "safeint",
            code: "invalid_type",
            path,
            message: format!(
                "Invalid input: expected int, received {}",
                zod_parsed_type(got)
            ),
        }
    }

    /// A `z.enum([...])` miss. Zod 4 prints the options double-quoted and
    /// pipe-joined. (v4's `createChatSchema` port called this `invalid_enum`;
    /// the rendering was byte-identical, so that spelling is now an alias.)
    pub fn invalid_value(values: &[&str], path: Vec<Value>) -> Self {
        let rendered = values
            .iter()
            .map(|v| format!("\"{v}\""))
            .collect::<Vec<_>>()
            .join("|");
        Self::InvalidValue {
            code: "invalid_value",
            values: values.iter().map(|v| json!(v)).collect(),
            path,
            message: format!("Invalid option: expected one of {rendered}"),
        }
    }

    /// `z.literal(v)` — the same `invalid_value` code as an enum, a different
    /// message.
    pub fn invalid_literal(value: &str, path: Vec<Value>) -> Self {
        Self::InvalidValue {
            code: "invalid_value",
            values: vec![json!(value)],
            path,
            message: format!("Invalid input: expected \"{value}\""),
        }
    }

    /// A `z.uuid()` / `z.string().uuid()` miss (the string type check has
    /// already passed — the two spellings report the same issue).
    pub fn invalid_uuid(path: Vec<Value>) -> Self {
        Self::InvalidFormat {
            origin: "string",
            code: "invalid_format",
            format: "uuid",
            pattern: ZOD_UUID_PATTERN,
            path,
            message: "Invalid UUID".to_string(),
        }
    }

    /// `.min(n)` on a string, with Zod's own sentence.
    pub fn too_small_string(minimum: Value, path: Vec<Value>) -> Self {
        let message = format!("Too small: expected string to have >={minimum} characters");
        Self::too_small_string_message(minimum, path, message)
    }

    /// `.min(n, '<custom>')` on a string — the schema's own message.
    pub fn too_small_string_message(
        minimum: Value,
        path: Vec<Value>,
        message: impl Into<String>,
    ) -> Self {
        Self::TooSmall {
            origin: "string",
            code: "too_small",
            minimum,
            inclusive: true,
            path,
            message: message.into(),
        }
    }

    /// `.max(n)` on a string, with Zod's own sentence.
    pub fn too_big_string(maximum: Value, path: Vec<Value>) -> Self {
        let message = format!("Too big: expected string to have <={maximum} characters");
        Self::too_big_string_message(maximum, path, message)
    }

    /// `.max(n, '<custom>')` on a string.
    pub fn too_big_string_message(
        maximum: Value,
        path: Vec<Value>,
        message: impl Into<String>,
    ) -> Self {
        Self::TooBig {
            origin: "string",
            code: "too_big",
            maximum,
            inclusive: true,
            path,
            message: message.into(),
        }
    }

    /// `.min(n)` on a number. `minimum` rides as the JSON number v4 declares
    /// (an integral bound prints as `1`, never `1.0`).
    pub fn too_small_number(minimum: Value, path: Vec<Value>) -> Self {
        let message = format!("Too small: expected number to be >={minimum}");
        Self::too_small_number_message(minimum, path, message)
    }

    /// `.min(n, '<custom>')` on a number.
    pub fn too_small_number_message(
        minimum: Value,
        path: Vec<Value>,
        message: impl Into<String>,
    ) -> Self {
        Self::TooSmall {
            origin: "number",
            code: "too_small",
            minimum,
            inclusive: true,
            path,
            message: message.into(),
        }
    }

    /// `.max(n)` on a number.
    pub fn too_big_number(maximum: Value, path: Vec<Value>) -> Self {
        let message = format!("Too big: expected number to be <={maximum}");
        Self::too_big_number_message(maximum, path, message)
    }

    /// `.max(n, '<custom>')` on a number.
    pub fn too_big_number_message(
        maximum: Value,
        path: Vec<Value>,
        message: impl Into<String>,
    ) -> Self {
        Self::TooBig {
            origin: "number",
            code: "too_big",
            maximum,
            inclusive: true,
            path,
            message: message.into(),
        }
    }

    /// The safe-integer FLOOR of `z.int()`. The bound rides as a JSON INTEGER
    /// (`-9007199254740991`, never `-9007199254740991.0`), which is why the
    /// value and the sentence both go through [`MAX_SAFE_INTEGER_I64`].
    pub fn too_small_int(path: Vec<Value>) -> Self {
        Self::TooSmallInt {
            code: "too_small",
            minimum: json!(-MAX_SAFE_INTEGER_I64),
            note: SAFE_INTEGER_NOTE,
            origin: "int",
            inclusive: true,
            path,
            message: format!("Too small: expected int to be >=-{MAX_SAFE_INTEGER_I64}"),
        }
    }

    /// The safe-integer CEILING of `z.int()`.
    pub fn too_big_int(path: Vec<Value>) -> Self {
        Self::TooBigInt {
            code: "too_big",
            maximum: json!(MAX_SAFE_INTEGER_I64),
            note: SAFE_INTEGER_NOTE,
            origin: "int",
            inclusive: true,
            path,
            message: format!("Too big: expected int to be <={MAX_SAFE_INTEGER_I64}"),
        }
    }

    // === P4.D217 ===
    /// `z.array(...).max(n)` — the size issue with `origin: "array"` and Zod's
    /// `items` sentence (measured at 4.6.5: `Too big: expected array to have
    /// <=32 items`).
    pub fn too_big_array(maximum: Value, path: Vec<Value>) -> Self {
        let message = format!("Too big: expected array to have <={maximum} items");
        Self::TooBig {
            origin: "array",
            code: "too_big",
            maximum,
            inclusive: true,
            path,
            message,
        }
    }

    /// A `.refine(fn, { message, path? })` failure.
    pub fn custom(path: Vec<Value>, message: impl Into<String>) -> Self {
        Self::Custom {
            code: "custom",
            path,
            message: message.into(),
        }
    }
    // === end P4.D217 ===

    /// This issue as a `serde_json::Value`, for the call sites that carry issue
    /// bags as raw JSON rather than as typed values. `preserve_order` keeps the
    /// key order the variant declares.
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }

    /// The issue's `path`, for the log-line projections v4 renders from it.
    pub fn path(&self) -> &[Value] {
        match self {
            Self::InvalidType { path, .. }
            | Self::InvalidIntType { path, .. }
            | Self::InvalidValue { path, .. }
            | Self::InvalidFormat { path, .. }
            | Self::TooSmall { path, .. }
            | Self::TooBig { path, .. }
            | Self::TooSmallInt { path, .. }
            | Self::TooBigInt { path, .. }
            | Self::Custom { path, .. } => path,
        }
    }

    /// The issue's `message`.
    pub fn message(&self) -> &str {
        match self {
            Self::InvalidType { message, .. }
            | Self::InvalidIntType { message, .. }
            | Self::InvalidValue { message, .. }
            | Self::InvalidFormat { message, .. }
            | Self::TooSmall { message, .. }
            | Self::TooBig { message, .. }
            | Self::TooSmallInt { message, .. }
            | Self::TooBigInt { message, .. }
            | Self::Custom { message, .. } => message,
        }
    }
}

/// v4 zod's own `uuid()` source pattern, verbatim — it is a VALUE in the issue
/// body (stringified with the surrounding slashes), so it is transcribed, not
/// re-derived. Note the RFC nibbles: version `1-8`, variant `89abAB`, with the
/// nil and max UUIDs allowed as literal alternatives.
pub const ZOD_UUID_PATTERN: &str = "/^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$/";

/// `true` when `s` satisfies [`ZOD_UUID_PATTERN`]. Hand-matched rather than
/// regex-compiled: the shape is fixed and the crate has no regex dependency.
pub fn zod_uuid_ok(s: &str) -> bool {
    if s.eq_ignore_ascii_case("00000000-0000-0000-0000-000000000000")
        || s.eq_ignore_ascii_case("ffffffff-ffff-ffff-ffff-ffffffffffff")
    {
        // The literal alternatives are case-SENSITIVE in the pattern; the nil
        // form has no letters, and the max form is spelled lowercase.
        return s == "00000000-0000-0000-0000-000000000000"
            || s == "ffffffff-ffff-ffff-ffff-ffffffffffff";
    }
    let b = s.as_bytes();
    if b.len() != 36 {
        return false;
    }
    let hex = |i: usize| b[i].is_ascii_hexdigit();
    for (i, &c) in b.iter().enumerate() {
        match i {
            8 | 13 | 18 | 23 => {
                if c != b'-' {
                    return false;
                }
            }
            _ => {
                if !hex(i) {
                    return false;
                }
            }
        }
    }
    matches!(b[14], b'1'..=b'8') && matches!(b[19], b'8' | b'9' | b'a' | b'b' | b'A' | b'B')
}

/// v4 zod **4.6.5**'s `z.iso.datetime()` (no options: no `offset`, no `local`,
/// unbounded `precision`) — the ONE server-side home of that check (P4.113).
/// Sourced from the compiled `pattern` of a live `z.iso.datetime()` at the
/// `d1c06cd9d` pin (`node_modules/zod/v4/core/regexes.js` `datetime()` over
/// `timeSource({ precision, seconds: true })`), with JS `\d` rewritten to ASCII
/// `[0-9]` (Rust's `\d` is Unicode-aware; JS's is ASCII). Since 4.6 the
/// seconds are REQUIRED wherever the time carries a zone (RFC 3339), so
/// `2024-01-01T10:00Z` FAILS — which is what makes this pattern differ from
/// the 4.4-era copy `vault_overlay.rs` used to carry. The zone is `Z` only
/// (case-sensitive), the fraction any length ≥ 1, and the date arm does real
/// leap-year arithmetic. `$` rejects a trailing newline in both engines.
pub const ZOD_ISO_DATETIME_PATTERN: &str = r"^(?:(?:[0-9][0-9][2468][048]|[0-9][0-9][13579][26]|[0-9][0-9]0[48]|[02468][048]00|[13579][26]00)-02-29|[0-9]{4}-(?:(?:0[13578]|1[02])-(?:0[1-9]|[12][0-9]|3[01])|(?:0[469]|11)-(?:0[1-9]|[12][0-9]|30)|(?:02)-(?:0[1-9]|1[0-9]|2[0-8])))T(?:(?:[01][0-9]|2[0-3]):[0-5][0-9]:[0-5][0-9](?:\.[0-9]+)?(?:Z))$";

static ZOD_ISO_DATETIME_RE: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(ZOD_ISO_DATETIME_PATTERN).unwrap());

/// `true` when `s` passes v4's `z.iso.datetime()` ([`ZOD_ISO_DATETIME_PATTERN`]).
pub fn zod_iso_datetime_ok(s: &str) -> bool {
    ZOD_ISO_DATETIME_RE.is_match(s)
}

/// v4 `util.parsedType` (the same table the Pascal Zod port pins). `None` is
/// JS `undefined` — a missing key.
pub fn zod_parsed_type(v: Option<&Value>) -> &'static str {
    match v {
        None => "undefined",
        Some(Value::Null) => "null",
        Some(Value::Bool(_)) => "boolean",
        Some(Value::Number(_)) => "number",
        Some(Value::String(_)) => "string",
        Some(Value::Array(_)) => "array",
        Some(Value::Object(_)) => "object",
    }
}

/// `ZodError.message` for a set of issues: `JSON.stringify(issues, null, 2)`.
/// `serde_json::to_string_pretty` uses the same two-space indent and the same
/// empty-array / nested-array shapes, so this is byte-identical.
pub fn zod_error_message(issues: &[ZodIssue]) -> String {
    serde_json::to_string_pretty(issues).unwrap_or_else(|_| "Invalid input".to_string())
}

/// The `details` array of v4's `validationError(err)` body, ready for
/// [`CoreError::details`](crate::api::types::CoreError::details).
pub fn zod_issue_details(issues: &[ZodIssue]) -> Value {
    serde_json::to_value(issues).unwrap_or(Value::Null)
}

/// `issues.map(i => `${i.path.join('.')} — ${i.message}`).join(', ')` — v4's
/// TOOL-side rendering of an issue bag (`run-sql-handler.ts:247` takes
/// `issues[0]`, so the join is usually over one).
pub fn zod_issues_joined(issues: &[ZodIssue]) -> String {
    zod_issue_lines(issues, " — ").join(", ")
}

/// `issues.map(i => `${i.path.join('.')}<sep>${i.message}`)` — the projection
/// v4's tool layer and its LoRA warn line render from the issue bag.
/// `Array.prototype.join` stringifies each element, so numeric indices render
/// bare.
pub fn zod_issue_lines(issues: &[ZodIssue], separator: &str) -> Vec<String> {
    issues
        .iter()
        .map(|i| {
            let joined = join_path(i.path());
            format!("{joined}{separator}{}", i.message())
        })
        .collect()
}

/// `path.join('.')` the way JS does it.
pub fn join_path(path: &[Value]) -> String {
    path.iter()
        .map(|p| match p {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// A schema key as a `path` element.
pub fn key(k: &str) -> Value {
    Value::String(k.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P4.113 — every expected value MEASURED by running the `d1c06cd9d` pin's
    /// real `z.iso.datetime().safeParse(row)` (zod 4.6.5) through a tsx probe
    /// (the recipe is in P4.113's lane record), never reasoned from the regex.
    #[test]
    fn iso_datetime_table_matches_real_zod_465() {
        let rows: &[(&str, bool)] = &[
            ("2024-01-01T10:00:00Z", true),
            ("2024-01-01T10:00:00.5Z", true),
            ("2024-01-01T10:00:00.123Z", true),
            ("2024-01-01T10:00:00.123456789Z", true),
            ("2024-01-01T10:00Z", false),
            ("2024-01-01T10:00:00+01:00", false),
            ("2024-01-01T10:00:00", false),
            ("2024-01-01", false),
            ("2024-01-01T23:59:60Z", false),
            ("2024-01-01T24:00:00Z", false),
            ("2024-01-01t10:00:00Z", false),
            ("2024-01-01T10:00:00z", false),
            ("2024-02-29T10:00:00Z", true),
            ("2023-02-29T10:00:00Z", false),
            ("1900-02-29T00:00:00Z", false),
            ("2000-02-29T00:00:00Z", true),
            ("yesterday", false),
            ("2024-01-01T10:00:00.Z", false),
            ("2024-01-01T10:00:00Z\n", false),
            ("2024-04-31T00:00:00Z", false),
            ("2024-01-01T10:00:00.1234567890123Z", true),
            ("2024-13-01T00:00:00Z", false),
            ("\u{662}\u{660}\u{662}\u{664}-01-01T10:00:00Z", false),
            ("", false),
        ];
        for (row, want) in rows {
            assert_eq!(zod_iso_datetime_ok(row), *want, "{row:?}");
        }
    }

    /// The render table: one row per Zod code, each transcribed from the real
    /// `zod` 4.5.4 output recorded in this module's header. The whole point is
    /// KEY ORDER, so each row compares the serialized STRING, not a `Value`
    /// equality (which would be order-blind for a `Map`-backed comparison).
    #[test]
    fn render_table_matches_real_zod_454() {
        let rows: Vec<(&str, ZodIssue, &str)> = vec![
            (
                "invalid_type/string",
                ZodIssue::invalid_type("string", vec![key("a")], Some(&json!(42))),
                r#"{"expected":"string","code":"invalid_type","path":["a"],"message":"Invalid input: expected string, received number"}"#,
            ),
            (
                "invalid_type/object-root",
                ZodIssue::invalid_type("object", vec![], Some(&json!(42))),
                r#"{"expected":"object","code":"invalid_type","path":[],"message":"Invalid input: expected object, received number"}"#,
            ),
            (
                "invalid_type/int",
                ZodIssue::invalid_int_type(vec![key("a")], Some(&json!(1.5))),
                r#"{"expected":"int","format":"safeint","code":"invalid_type","path":["a"],"message":"Invalid input: expected int, received number"}"#,
            ),
            (
                "invalid_value/enum",
                ZodIssue::invalid_value(&["x", "y"], vec![key("a")]),
                r#"{"code":"invalid_value","values":["x","y"],"path":["a"],"message":"Invalid option: expected one of \"x\"|\"y\""}"#,
            ),
            (
                "invalid_value/literal",
                ZodIssue::invalid_literal("CHARACTER", vec![key("a")]),
                r#"{"code":"invalid_value","values":["CHARACTER"],"path":["a"],"message":"Invalid input: expected \"CHARACTER\""}"#,
            ),
            (
                "invalid_format/uuid",
                ZodIssue::invalid_uuid(vec![key("a")]),
                r#"{"origin":"string","code":"invalid_format","format":"uuid","pattern":"/^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$/","path":["a"],"message":"Invalid UUID"}"#,
            ),
            (
                "too_small/string",
                ZodIssue::too_small_string(json!(1), vec![key("a")]),
                r#"{"origin":"string","code":"too_small","minimum":1,"inclusive":true,"path":["a"],"message":"Too small: expected string to have >=1 characters"}"#,
            ),
            (
                "too_big/string",
                ZodIssue::too_big_string(json!(2), vec![key("a")]),
                r#"{"origin":"string","code":"too_big","maximum":2,"inclusive":true,"path":["a"],"message":"Too big: expected string to have <=2 characters"}"#,
            ),
            (
                "too_small/number",
                ZodIssue::too_small_number(json!(0), vec![key("a")]),
                r#"{"origin":"number","code":"too_small","minimum":0,"inclusive":true,"path":["a"],"message":"Too small: expected number to be >=0"}"#,
            ),
            (
                "too_big/number",
                ZodIssue::too_big_number(json!(1), vec![key("a")]),
                r#"{"origin":"number","code":"too_big","maximum":1,"inclusive":true,"path":["a"],"message":"Too big: expected number to be <=1"}"#,
            ),
            (
                "too_big/int-safeint",
                ZodIssue::too_big_int(vec![key("a")]),
                r#"{"code":"too_big","maximum":9007199254740991,"note":"Integers must be within the safe integer range.","origin":"int","inclusive":true,"path":["a"],"message":"Too big: expected int to be <=9007199254740991"}"#,
            ),
            (
                "too_small/int-safeint",
                ZodIssue::too_small_int(vec![key("a")]),
                r#"{"code":"too_small","minimum":-9007199254740991,"note":"Integers must be within the safe integer range.","origin":"int","inclusive":true,"path":["a"],"message":"Too small: expected int to be >=-9007199254740991"}"#,
            ),
            // P4.D217 — measured at 4.6.5 through `scenarioBuildRequestSchema`
            // (the `scenario_build_request_schema_equivalence` oracle).
            (
                "too_big/array",
                ZodIssue::too_big_array(json!(32), vec![key("characterIds")]),
                r#"{"origin":"array","code":"too_big","maximum":32,"inclusive":true,"path":["characterIds"],"message":"Too big: expected array to have <=32 items"}"#,
            ),
            (
                "custom/refine-root",
                ZodIssue::custom(vec![], "priorDraft and revision travel together"),
                r#"{"code":"custom","path":[],"message":"priorDraft and revision travel together"}"#,
            ),
        ];
        for (label, issue, want) in rows {
            assert_eq!(
                serde_json::to_string(&issue).unwrap(),
                want,
                "{label}: the serialized issue must match real zod byte for byte, key \
                 order included (recorded at 4.5.4; re-verified unchanged at 4.6.5 by \
                 P4.D211, whose read found `core/util.js`'s `finalizeIssue` rewritten \
                 from object-rest to an own-key loop with the SAME key order)"
            );
        }
    }

    /// A numeric `path` element rides as a JSON number, not a string — Zod's
    /// `path` is `(string | number)[]` and array indices are numbers.
    #[test]
    fn a_numeric_path_element_rides_as_a_number() {
        let issue = ZodIssue::invalid_type("object", vec![json!(0)], Some(&json!(7)));
        assert_eq!(
            serde_json::to_string(&issue.to_value()).unwrap(),
            r#"{"expected":"object","code":"invalid_type","path":[0],"message":"Invalid input: expected object, received number"}"#
        );
        assert_eq!(join_path(&[json!(0), key("source")]), "0.source");
    }

    /// `ZodError.message` is `JSON.stringify(issues, null, 2)`.
    #[test]
    fn zod_error_message_is_pretty_stringify() {
        let msg = zod_error_message(&[ZodIssue::invalid_type(
            "boolean",
            vec![key("dashes")],
            Some(&json!("yes")),
        )]);
        assert_eq!(
            msg,
            "[\n  {\n    \"expected\": \"boolean\",\n    \"code\": \"invalid_type\",\n    \"path\": [\n      \"dashes\"\n    ],\n    \"message\": \"Invalid input: expected boolean, received string\"\n  }\n]"
        );
    }

    #[test]
    fn parsed_type_covers_every_json_shape() {
        assert_eq!(zod_parsed_type(None), "undefined");
        assert_eq!(zod_parsed_type(Some(&Value::Null)), "null");
        assert_eq!(zod_parsed_type(Some(&json!(true))), "boolean");
        assert_eq!(zod_parsed_type(Some(&json!(1))), "number");
        assert_eq!(zod_parsed_type(Some(&json!("s"))), "string");
        assert_eq!(zod_parsed_type(Some(&json!([]))), "array");
        assert_eq!(zod_parsed_type(Some(&json!({}))), "object");
    }

    #[test]
    fn uuid_gate_matches_the_pattern() {
        assert!(zod_uuid_ok("a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11"));
        assert!(zod_uuid_ok("00000000-0000-0000-0000-000000000000"));
        assert!(zod_uuid_ok("ffffffff-ffff-ffff-ffff-ffffffffffff"));
        // The max form is spelled lowercase in the pattern's literal arm.
        assert!(!zod_uuid_ok("FFFFFFFF-FFFF-FFFF-FFFF-FFFFFFFFFFFF"));
        // version 9 and variant 'c' are both out of the RFC nibbles.
        assert!(!zod_uuid_ok("a0eebc99-9c0b-9ef8-bb6d-6bb9bd380a11"));
        assert!(!zod_uuid_ok("a0eebc99-9c0b-4ef8-cb6d-6bb9bd380a11"));
        assert!(!zod_uuid_ok("nope"));
    }

    /// The joined projection v4's tool layer renders (` — `) and the LoRA warn
    /// line renders (`: `) come off the same bag.
    #[test]
    fn issue_lines_join_path_and_message() {
        let issues = vec![
            ZodIssue::invalid_type("string", vec![key("max_rows")], Some(&json!(true))),
            ZodIssue::too_small_string_message(json!(1), vec![json!(0), key("source")], "required"),
        ];
        assert_eq!(
            zod_issue_lines(&issues, " — "),
            vec![
                "max_rows — Invalid input: expected string, received boolean",
                "0.source — required"
            ]
        );
        assert_eq!(
            zod_issue_lines(&issues, ": ")[1],
            "0.source: required".to_string()
        );
    }
}
