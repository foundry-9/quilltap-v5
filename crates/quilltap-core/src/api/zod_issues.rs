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
//! baseline pin, not inferred from Zod's source, and RE-MEASURED unchanged
//! against **4.6.5** at the `97b25fc53` pin by P4.130 — which added the last
//! rows (the datetime format issue, the `invalid_union` a `TimestampSchema`
//! reports over a missing cell, and a `Float32Array` — a hydrated BLOB cell —
//! with the length checks zod still runs over it). The command, verbatim:
//!
//! ```text
//! cd /tmp/qt-v4-pin-p4130-97b25fc53 && node --input-type=module -e '
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
//! show("invalid_format/regex", z.object({ a: z.string().regex(/^#(?:[0-9a-fA-F]{3}){1,2}$/) }), { a: "red" }); // P4.148, at 07b8f0209
//! show("too_small/string",      z.object({ a: z.string().min(1) }),  { a: "" });
//! show("too_big/string",        z.object({ a: z.string().max(2) }),  { a: "abcd" });
//! show("too_small/number",      z.object({ a: z.number().min(0) }),  { a: -1 });
//! show("too_big/number",        z.object({ a: z.number().max(1) }),  { a: 2 });
//! show("too_big/int-safeint",   z.object({ a: z.int() }),            { a: 1e300 });
//! show("too_small/int-safeint", z.object({ a: z.int() }),            { a: -1e300 });
//! show("invalid_format/datetime", z.object({ a: z.iso.datetime() }), { a: "n" });
//! show("invalid_union/timestamp", z.object({ a: z.iso.datetime().or(z.date()) }), { });
//! show("invalid_type/float32",  z.object({ a: z.string() }),         { a: new Float32Array(1) });
//! show("too_small/unknown",     z.object({ a: z.string().min(1) }),  { a: new Float32Array(0) });
//! show("too_big/unknown",       z.object({ a: z.string().max(100) }), { a: new Float32Array(101) });
//! '
//! ```
//!
//! Its output is transcribed into [`tests::render_table_matches_real_zod_465`],
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
    /// [P4.161] A `z.instanceof(Class)` miss — `code, expected, path, message`
    /// (`code` FIRST, unlike [`Self::InvalidType`]; measured at zod 4.6.5
    /// through `MemorySchema.embedding`'s `Float32Array` / `Buffer` options).
    InvalidInstance {
        code: &'static str,
        expected: &'static str,
        path: Vec<Value>,
        message: String,
    },
    /// A `z.union([...])` / `.or(...)` where every option ABORTED — `code,
    /// errors, path, message`, `errors` one issue list per option (each
    /// relative to the union's own position, so its `path` is `[]`). Measured
    /// at zod 4.6.5 (`97b25fc53`, P4.130) through `TimestampSchema` (`z.iso
    /// .datetime().or(z.date())`) over a missing or non-string cell. A union in
    /// which exactly one option did NOT abort reports that option's issues
    /// bare instead — which is why a malformed STRING stamp is a lone
    /// [`ZodIssue::invalid_datetime`].
    InvalidUnion {
        code: &'static str,
        errors: Vec<Vec<ZodIssue>>,
        path: Vec<Value>,
        message: String,
    },
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

    /// [P4.148] A `z.string().regex(re)` miss — the SAME `invalid_format` key
    /// order as the uuid issue (`origin, code, format, pattern, path,
    /// message`), `format: "regex"`, the source pattern echoed with its
    /// slashes, and Zod's sentence `Invalid string: must match pattern <re>`.
    /// Measured at zod 4.6.5 (the `07b8f0209` pin) over
    /// `z.string().regex(/^#(?:[0-9a-fA-F]{3}){1,2}$/)` and `"red"`.
    pub fn invalid_regex(pattern: &'static str, path: Vec<Value>) -> Self {
        Self::InvalidFormat {
            origin: "string",
            code: "invalid_format",
            format: "regex",
            pattern,
            path,
            message: format!("Invalid string: must match pattern {pattern}"),
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

    /// A `z.iso.datetime()` miss (the string type check has already passed) —
    /// the `invalid_format` shape with `format: "datetime"` and zod's own
    /// JS-form pattern ([`ZOD_ISO_DATETIME_JS_PATTERN`]) as a VALUE.
    pub fn invalid_datetime(path: Vec<Value>) -> Self {
        Self::InvalidFormat {
            origin: "string",
            code: "invalid_format",
            format: "datetime",
            pattern: ZOD_ISO_DATETIME_JS_PATTERN,
            path,
            message: "Invalid ISO datetime".to_string(),
        }
    }

    /// A length `.min(n)` over a value zod cannot name a length origin for (a
    /// typed array reaching a string schema) — `origin: "unknown"`.
    pub fn too_small_unknown(minimum: Value, path: Vec<Value>) -> Self {
        let message = format!("Too small: expected unknown to be >={minimum}");
        Self::TooSmall {
            origin: "unknown",
            code: "too_small",
            minimum,
            inclusive: true,
            path,
            message,
        }
    }

    /// The `.max(n)` mirror of [`Self::too_small_unknown`].
    pub fn too_big_unknown(maximum: Value, path: Vec<Value>) -> Self {
        let message = format!("Too big: expected unknown to be <={maximum}");
        Self::TooBig {
            origin: "unknown",
            code: "too_big",
            maximum,
            inclusive: true,
            path,
            message,
        }
    }

    /// [P4.161] A `z.instanceof(Class)` miss (see [`Self::InvalidInstance`]).
    pub fn invalid_instance(expected: &'static str, path: Vec<Value>, got: Option<&Value>) -> Self {
        Self::InvalidInstance {
            code: "invalid_type",
            expected,
            path,
            message: format!(
                "Invalid input: expected {expected}, received {}",
                zod_parsed_type(got)
            ),
        }
    }

    /// A union every option of which aborted (see [`Self::InvalidUnion`]).
    pub fn invalid_union(errors: Vec<Vec<ZodIssue>>, path: Vec<Value>) -> Self {
        Self::InvalidUnion {
            code: "invalid_union",
            errors,
            path,
            message: "Invalid input".to_string(),
        }
    }

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
            | Self::Custom { path, .. }
            | Self::InvalidInstance { path, .. }
            | Self::InvalidUnion { path, .. } => path,
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
            | Self::Custom { message, .. }
            | Self::InvalidInstance { message, .. }
            | Self::InvalidUnion { message, .. } => message,
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

/// v4 `HexColorSchema` (`lib/schemas/common.types.ts:77`) — its regex source,
/// verbatim, as Zod echoes it into the issue's `pattern`.
pub const ZOD_HEX_COLOR_PATTERN: &str = "/^#(?:[0-9a-fA-F]{3}){1,2}$/";

/// `true` when `s` matches [`ZOD_HEX_COLOR_PATTERN`] — `#rgb` or `#rrggbb`.
/// The pattern is anchored (`^`/`$`, no `m` flag), so it is a whole-string
/// match; its classes are ASCII-only, so a byte walk is exact. The ONE
/// predicate for v4's hex colour (P4.148; `api/groups.rs`'s private twin was
/// folded onto it at the `07b8f0209` follow-ups unification).
pub fn zod_hex_color_ok(s: &str) -> bool {
    let Some(rest) = s.strip_prefix('#') else {
        return false;
    };
    (rest.len() == 3 || rest.len() == 6) && rest.bytes().all(|b| b.is_ascii_hexdigit())
}

/// One `X.nullable().optional()` string field: absent and `null` pass, a
/// non-string is `invalid_type`, a string is handed to `check` (which pushes
/// its own issue).
fn nullable_string_issues(
    bag: &serde_json::Map<String, Value>,
    k: &str,
    issues: &mut Vec<ZodIssue>,
    check: impl Fn(&str) -> Option<ZodIssue>,
) {
    match bag.get(k) {
        None | Some(Value::Null) => {}
        Some(Value::String(v)) => issues.extend(check(v)),
        got => issues.push(ZodIssue::invalid_type("string", vec![key(k)], got)),
    }
}

/// The colour + icon pair both property schemas declare, in schema order:
/// `color: HexColorSchema.nullable().optional()`, `icon:
/// z.string().max(50).nullable().optional()` (CODE POINTS — `jsstr::
/// zod_len_max_ok`).
fn color_icon_issues(bag: &serde_json::Map<String, Value>, issues: &mut Vec<ZodIssue>) {
    nullable_string_issues(bag, "color", issues, |v| {
        (!zod_hex_color_ok(v))
            .then(|| ZodIssue::invalid_regex(ZOD_HEX_COLOR_PATTERN, vec![key("color")]))
    });
    nullable_string_issues(bag, "icon", issues, |v| {
        (!crate::jsstr::zod_len_max_ok(v, 50))
            .then(|| ZodIssue::too_big_string(json!(50), vec![key("icon")]))
    });
}

/// v4 `GroupPropertiesSchema` (`lib/schemas/group.types.ts:43-46`) over a raw
/// `properties.json` bag — zod's issue list in zod's order (empty = the bag
/// parses). A non-object is ONE `invalid_type` at `path: []`. Unknown keys are
/// stripped by `z.object`, never an issue. The ONE rule set every v5 site runs
/// through `GroupEntity::parse_properties` (P4.148 — v4 `parseProperties` at
/// every overlay site, `document-store-overlay.ts:160,295,316,384,389`).
pub fn zod_group_properties_issues(bag: &Value) -> Vec<ZodIssue> {
    let Some(obj) = bag.as_object() else {
        return vec![ZodIssue::invalid_type("object", vec![], Some(bag))];
    };
    let mut issues = Vec::new();
    color_icon_issues(obj, &mut issues);
    issues
}

/// v4 `ProjectPropertiesSchema` (`lib/schemas/project.types.ts:62-119`) over a
/// raw `properties.json` bag, in schema order — the sixteen keys:
///
/// - `allowAnyCharacter: z.boolean().default(false)` — absent defaults; `null`
///   or a non-boolean is `invalid_type` (a default replaces `undefined` only);
/// - `characterRoster: z.array(UUIDSchema).default([])` — a non-array (incl.
///   `null`) is `invalid_type array`; each element a uuid, its issue at
///   `[key, index]` (a non-string element `invalid_type string`);
/// - `color` / `icon` — [`color_icon_issues`];
/// - `defaultDisabledTools` / `defaultDisabledToolGroups: z.array(z.string())
///   .default([])`;
/// - the four `z.boolean().nullable().optional()` flags;
/// - `defaultImageProfileId`, `defaultRoleplayTemplateId`,
///   `staticBackgroundImageId`, `storyBackgroundImageId:
///   UUIDSchema.nullable().optional()`;
/// - `answerConfirmationOverride: z.enum(['ON','OFF']).nullable().optional()`
///   — an enum has no separate type gate, so a number is `invalid_value` too;
/// - `backgroundDisplayMode: z.preprocess(normalizeBackgroundDisplayMode,
///   z.enum(['latest_chat','theme'])).default('theme')` — absent defaults
///   (the default short-circuits ahead of the preprocess); `null`
///   preprocesses to `undefined` and FAILS the enum (`invalid_value`); every
///   other value normalizes to a member (measured at the pin: `5` → `theme`).
///
/// Measured against v4's REAL schema at `07b8f0209` (the `projects-tier2`
/// corpus's `propertyRefusals` cells).
pub fn zod_project_properties_issues(bag: &Value) -> Vec<ZodIssue> {
    let Some(obj) = bag.as_object() else {
        return vec![ZodIssue::invalid_type("object", vec![], Some(bag))];
    };
    let mut issues = Vec::new();
    let boolean_default = |k: &str, issues: &mut Vec<ZodIssue>| match obj.get(k) {
        None | Some(Value::Bool(_)) => {}
        got => issues.push(ZodIssue::invalid_type("boolean", vec![key(k)], got)),
    };
    let nullable_boolean = |k: &str, issues: &mut Vec<ZodIssue>| match obj.get(k) {
        None | Some(Value::Null) | Some(Value::Bool(_)) => {}
        got => issues.push(ZodIssue::invalid_type("boolean", vec![key(k)], got)),
    };
    let string_array = |k: &str, uuid: bool, issues: &mut Vec<ZodIssue>| match obj.get(k) {
        None => {}
        Some(Value::Array(items)) => {
            for (i, item) in items.iter().enumerate() {
                let path = vec![key(k), json!(i)];
                match item {
                    Value::String(v) if uuid && !zod_uuid_ok(v) => {
                        issues.push(ZodIssue::invalid_uuid(path))
                    }
                    Value::String(_) => {}
                    got => issues.push(ZodIssue::invalid_type("string", path, Some(got))),
                }
            }
        }
        got => issues.push(ZodIssue::invalid_type("array", vec![key(k)], got)),
    };
    let nullable_uuid = |k: &str, issues: &mut Vec<ZodIssue>| {
        nullable_string_issues(obj, k, issues, |v| {
            (!zod_uuid_ok(v)).then(|| ZodIssue::invalid_uuid(vec![key(k)]))
        })
    };
    boolean_default("allowAnyCharacter", &mut issues);
    string_array("characterRoster", true, &mut issues);
    color_icon_issues(obj, &mut issues);
    string_array("defaultDisabledTools", false, &mut issues);
    string_array("defaultDisabledToolGroups", false, &mut issues);
    nullable_boolean("defaultAgentModeEnabled", &mut issues);
    nullable_boolean("defaultAvatarGenerationEnabled", &mut issues);
    nullable_uuid("defaultImageProfileId", &mut issues);
    nullable_uuid("defaultRoleplayTemplateId", &mut issues);
    nullable_boolean("defaultAlertCharactersOfLanternImages", &mut issues);
    match obj.get("answerConfirmationOverride") {
        None | Some(Value::Null) => {}
        Some(Value::String(v)) if v == "ON" || v == "OFF" => {}
        Some(_) => issues.push(ZodIssue::invalid_value(
            &["ON", "OFF"],
            vec![key("answerConfirmationOverride")],
        )),
    }
    nullable_boolean("storyBackgroundsEnabled", &mut issues);
    nullable_uuid("staticBackgroundImageId", &mut issues);
    nullable_uuid("storyBackgroundImageId", &mut issues);
    if let Some(Value::Null) = obj.get("backgroundDisplayMode") {
        issues.push(ZodIssue::invalid_value(
            &["latest_chat", "theme"],
            vec![key("backgroundDisplayMode")],
        ));
    }
    issues
}

/// v4 `GroupSchema` (`lib/schemas/group.types.ts`) over a raw `groups` row —
/// the schema `groups.repository.ts` constructs its repository with, so the
/// one `groups.findByIdRaw` (`_findById`) validates before a row counts as
/// found (P4.124, P4.D231). Returns zod's issue list in zod's order (empty =
/// the row passes); `ZodError.message` is [`zod_error_message`] over it.
///
/// The row columns, in schema order: `id: UUIDSchema`, `name:
/// z.string().min(1).max(100)` (Unicode CODE POINTS — zod ≥ 4.5's
/// `$ZodCheckMaxLength` / `$ZodCheckMinLength` over `util.codePointLength`, not
/// JS `.length`; P4.130 found a UTF-16 count refusing a row v4 keeps),
/// `officialMountPointId: UUIDSchema.nullable().optional()` (absent and `null`
/// both pass — v4 reads a NULL cell as `undefined`), `createdAt`/`updatedAt:
/// TimestampSchema` ([`zod_timestamp_issues`]).
///
/// ⚠ Scope, recorded: `GroupSchema` extends `GroupRowSchema` with the five
/// store-resident fields (`description` ≤ 2000, `instructions` ≤ 10000,
/// `state` a JSON record with a `{}` default, `color` a hex colour, `icon` ≤
/// 50). v4 strips those from every row it writes (`GROUP_STORE_MANAGED_FIELDS`),
/// so their columns read NULL → `undefined` on any v4-written instance and add
/// no issue; they are not checked here. (P4.124 named the schema
/// `GroupRowSchema`; the outcome and issue order are the same on such a row.)
pub fn zod_group_issues(row: &serde_json::Map<String, Value>) -> Vec<ZodIssue> {
    let mut issues = Vec::new();
    zod_uuid_issues(row, "id", false, &mut issues);
    zod_entity_name_issues(row.get("name"), &mut issues);
    zod_uuid_issues(row, "officialMountPointId", true, &mut issues);
    zod_timestamp_issues(row, "createdAt", &mut issues);
    zod_timestamp_issues(row, "updatedAt", &mut issues);
    issues
}

/// The store-backed row schemas' `name: z.string().min(1).max(100)` (v4
/// `GroupRowSchema` / `ProjectRowSchema`) — Unicode CODE POINTS (zod ≥ 4.5's
/// `util.codePointLength`, P4.130).
fn zod_entity_name_issues(got: Option<&Value>, issues: &mut Vec<ZodIssue>) {
    match got {
        Some(Value::String(n)) => {
            if !crate::jsstr::zod_len_min_ok(n, 1) {
                issues.push(ZodIssue::too_small_string(json!(1), vec![key("name")]));
            } else if !crate::jsstr::zod_len_max_ok(n, 100) {
                issues.push(ZodIssue::too_big_string(json!(100), vec![key("name")]));
            }
        }
        got => {
            issues.push(ZodIssue::invalid_type("string", vec![key("name")], got));
            // zod runs a length check over anything with a `.length` even after
            // the type issue (`$ZodCheckMinLength.when`), so a typed array
            // outside `[1, 100]` ALSO fails its bound — `origin: "unknown"`
            // (`getLengthableOrigin`), counted in elements. Measured at 4.6.5.
            if let Some(n) = float32_array_len(got) {
                if n < 1 {
                    issues.push(ZodIssue::too_small_unknown(json!(1), vec![key("name")]));
                } else if n > 100 {
                    issues.push(ZodIssue::too_big_unknown(json!(100), vec![key("name")]));
                }
            }
        }
    }
}

/// v4 `ProjectSchema` / `GroupSchema`'s OWN keys (`lib/schemas/project.
/// types.ts:129-161`, `group.types.ts:58-89` — identical for both kinds) over
/// the entity the store-backed `create` hands `_create` to validate:
/// `{...prepareCreateData(data), officialMountPointId: null, id, createdAt,
/// updatedAt}` (`store-backed.repository.ts:136-144`, `base.repository.ts:
/// 350-368`). In schema order, AHEAD of the spread property bag's issues
/// ([`zod_project_properties_issues`] / [`zod_group_properties_issues`], which
/// the caller appends — `ProjectRowSchema.extend({…, ...PropertiesSchema.
/// shape})` keeps that order):
///
/// - `id: UUIDSchema` — `id` is the id the create will claim (`options.id ||
///   generateId()`), checked only when the caller claims one (a minted id
///   always passes);
/// - `name: z.string().min(1).max(100)` ([`zod_entity_name_issues`]);
/// - `officialMountPointId` — always `null` on create; `createdAt` /
///   `updatedAt` — always fresh stamps: no issue possible, not checked;
/// - `description: z.string().max(2000).nullable().optional()`;
/// - `instructions: z.string().max(10000).nullable().optional()`;
/// - `state: JsonSchema.default({})` — `z.record(z.string(), z.unknown())`:
///   absent defaults; anything but a plain object (`null`, an array, a
///   string) is `invalid_type` `expected record`.
///
/// Measured against v4's REAL schemas at `94fbb1ae3` (P4.155 — the
/// `system_import_state` property-refusal arms' whole-entity rows).
pub fn zod_store_entity_issues(
    entity: &serde_json::Map<String, Value>,
    claimed_id: Option<&str>,
) -> Vec<ZodIssue> {
    let mut issues = Vec::new();
    if let Some(id) = claimed_id {
        if !zod_uuid_ok(id) {
            issues.push(ZodIssue::invalid_uuid(vec![key("id")]));
        }
    }
    zod_entity_name_issues(entity.get("name"), &mut issues);
    for (k, max) in [("description", 2000), ("instructions", 10000)] {
        nullable_string_issues(entity, k, &mut issues, |v| {
            (!crate::jsstr::zod_len_max_ok(v, max))
                .then(|| ZodIssue::too_big_string(json!(max), vec![key(k)]))
        });
    }
    match entity.get("state") {
        None | Some(Value::Object(_)) => {}
        got => issues.push(ZodIssue::invalid_type("record", vec![key("state")], got)),
    }
    issues
}

/// v4 `MemorySourceEnum` (`lib/schemas/memory.types.ts:21`), in schema order.
pub const MEMORY_SOURCE: [&str; 2] = ["AUTO", "MANUAL"];
/// v4 `WitnessedContextEnum` (`memory.types.ts:35`).
pub const MEMORY_WITNESSED_CONTEXT: [&str; 3] = ["user_present", "autonomous_room", "manual"];
/// v4 `MemoryKindEnum` (`memory.types.ts:47`).
pub const MEMORY_KIND: [&str; 2] = ["semantic", "episodic"];

/// `z.array(z.string())` / `z.array(UUIDSchema)` with `.default([])` at `k`:
/// absent defaults; anything but an array (incl. `null` — a default replaces
/// `undefined` only) is `invalid_type array`; each element a string (a uuid
/// when `uuid`), its issue at `[k, index]`.
fn default_string_array_issues(
    row: &serde_json::Map<String, Value>,
    k: &str,
    uuid: bool,
    issues: &mut Vec<ZodIssue>,
) {
    match row.get(k) {
        None => {}
        Some(Value::Array(items)) => {
            for (i, item) in items.iter().enumerate() {
                let path = vec![key(k), json!(i)];
                match item {
                    Value::String(v) if uuid && !zod_uuid_ok(v) => {
                        issues.push(ZodIssue::invalid_uuid(path))
                    }
                    Value::String(_) => {}
                    got => issues.push(ZodIssue::invalid_type("string", path, Some(got))),
                }
            }
        }
        got => issues.push(ZodIssue::invalid_type("array", vec![key(k)], got)),
    }
}

/// A `z.enum([...])` at `k` — `.default(..)` when `default` (absent passes),
/// `.nullable().optional()` when `nullable` (absent and `null` pass). An enum
/// has no separate type gate, so a wrong type is `invalid_value` too.
fn enum_issues(
    row: &serde_json::Map<String, Value>,
    k: &str,
    values: &[&str],
    nullable: bool,
    issues: &mut Vec<ZodIssue>,
) {
    match row.get(k) {
        None => {}
        Some(Value::Null) if nullable => {}
        Some(Value::String(v)) if values.contains(&v.as_str()) => {}
        Some(_) => issues.push(ZodIssue::invalid_value(values, vec![key(k)])),
    }
}

/// `z.number().min(lo).max(hi).default(..)` at `k` (absent defaults).
fn default_ranged_number_issues(
    row: &serde_json::Map<String, Value>,
    k: &str,
    lo: i64,
    hi: i64,
    issues: &mut Vec<ZodIssue>,
) {
    match row.get(k) {
        None => {}
        Some(v) => match v.as_f64() {
            None => issues.push(ZodIssue::invalid_type("number", vec![key(k)], Some(v))),
            Some(n) if n < lo as f64 => {
                issues.push(ZodIssue::too_small_number(json!(lo), vec![key(k)]))
            }
            Some(n) if n > hi as f64 => {
                issues.push(ZodIssue::too_big_number(json!(hi), vec![key(k)]))
            }
            Some(_) => {}
        },
    }
}

/// A `.nullable().optional()` `z.string()` at `k`.
fn nullable_plain_string_issues(
    row: &serde_json::Map<String, Value>,
    k: &str,
    issues: &mut Vec<ZodIssue>,
) {
    nullable_string_issues(row, k, issues, |_| None);
}

/// A REQUIRED `z.string()` at `k`.
fn required_string_issues(
    row: &serde_json::Map<String, Value>,
    k: &str,
    issues: &mut Vec<ZodIssue>,
) {
    if !row.get(k).is_some_and(Value::is_string) {
        issues.push(ZodIssue::invalid_type("string", vec![key(k)], row.get(k)));
    }
}

/// `TimestampSchema.nullable().optional()` at `k`.
fn nullable_timestamp_issues(
    row: &serde_json::Map<String, Value>,
    k: &str,
    issues: &mut Vec<ZodIssue>,
) {
    if !matches!(row.get(k), None | Some(Value::Null)) {
        zod_timestamp_issues(row, k, issues);
    }
}

/// `MemorySchema.embedding` (`memory.types.ts:67-80`): `z.union([
/// z.instanceof(Float32Array), z.array(z.number()).transform(…),
/// z.instanceof(Buffer).transform(…), z.string().transform(…)])
/// .nullable().optional()`. Absent / `null` pass; a hydrated BLOB
/// ([`zod_float32_array_cell`]) is the `Float32Array` option; a number array
/// the array option; a string the string option (whose transform's
/// `JSON.parse` this twin does not run — no archive or `.qtap` writer emits a
/// string embedding, and the import drops the key before the parse). Anything
/// else fails EVERY option: one `invalid_union` carrying each option's issues
/// (the array option's are its elements' `invalid_type number` at `[index]`
/// when the value IS an array). Measured at zod 4.6.5 (P4.161).
fn memory_embedding_issues(row: &serde_json::Map<String, Value>, issues: &mut Vec<ZodIssue>) {
    let got = match row.get("embedding") {
        None | Some(Value::Null) | Some(Value::String(_)) => return,
        g if float32_array_len(g).is_some() => return,
        Some(g) => g,
    };
    let array_option: Vec<ZodIssue> = match got.as_array() {
        Some(items) => items
            .iter()
            .enumerate()
            .filter(|(_, x)| !x.is_number())
            .map(|(i, x)| ZodIssue::invalid_type("number", vec![json!(i)], Some(x)))
            .collect(),
        None => vec![ZodIssue::invalid_type("array", vec![], Some(got))],
    };
    if array_option.is_empty() {
        return;
    }
    issues.push(ZodIssue::invalid_union(
        vec![
            vec![ZodIssue::invalid_instance(
                "Float32Array",
                vec![],
                Some(got),
            )],
            array_option,
            vec![ZodIssue::invalid_instance("Buffer", vec![], Some(got))],
            vec![ZodIssue::invalid_type("string", vec![], Some(got))],
        ],
        vec![key("embedding")],
    ));
}

/// v4 `MemorySchema` (`lib/schemas/memory.types.ts:54-108`) over the WHOLE
/// entity `_create` validates (`{...data, id, createdAt, updatedAt}`,
/// `base.repository.ts:350-368`) — zod's issue list in SCHEMA KEY order
/// (empty = the row parses). Unknown keys are stripped, never an issue. The
/// ONE rule set `db::memories::parse_create_memory` runs on the `.qtap`
/// import and the backup restore (P4.161, dogfood #152); every bound is a
/// recorded row of the `repository_zod_messages` oracle (v4's REAL schema),
/// never typed from the source.
pub fn zod_memory_issues(row: &serde_json::Map<String, Value>) -> Vec<ZodIssue> {
    let mut issues = Vec::new();
    zod_uuid_issues(row, "id", false, &mut issues);
    zod_uuid_issues(row, "characterId", false, &mut issues);
    for k in ["aboutCharacterId", "chatId", "projectId"] {
        zod_uuid_issues(row, k, true, &mut issues);
    }
    required_string_issues(row, "content", &mut issues);
    required_string_issues(row, "summary", &mut issues);
    default_string_array_issues(row, "keywords", false, &mut issues);
    default_string_array_issues(row, "tags", true, &mut issues);
    default_ranged_number_issues(row, "importance", 0, 1, &mut issues);
    memory_embedding_issues(row, &mut issues);
    enum_issues(row, "source", &MEMORY_SOURCE, false, &mut issues);
    enum_issues(
        row,
        "witnessedContext",
        &MEMORY_WITNESSED_CONTEXT,
        true,
        &mut issues,
    );
    nullable_timestamp_issues(row, "occurredAt", &mut issues);
    nullable_plain_string_issues(row, "narrativeTime", &mut issues);
    default_string_array_issues(row, "entities", false, &mut issues);
    enum_issues(row, "kind", &MEMORY_KIND, false, &mut issues);
    zod_uuid_issues(row, "sourceMessageId", true, &mut issues);
    nullable_timestamp_issues(row, "lastAccessedAt", &mut issues);
    // `z.number().int().min(1).default(1)`: the number gate, then the integer
    // gate (which ABORTS — `1.5` and `0.5` answer one issue), then the
    // safe-integer bounds and the `min(1)` (which continue).
    if let Some(v) = row.get("reinforcementCount") {
        let at = || vec![key("reinforcementCount")];
        match v.as_f64() {
            None => issues.push(ZodIssue::invalid_type("number", at(), Some(v))),
            Some(n) if n.fract() != 0.0 => issues.push(ZodIssue::invalid_int_type(at(), Some(v))),
            Some(n) => {
                if n > MAX_SAFE_INTEGER {
                    issues.push(ZodIssue::too_big_int(at()));
                } else if n < -MAX_SAFE_INTEGER {
                    issues.push(ZodIssue::too_small_int(at()));
                }
                if n < 1.0 {
                    issues.push(ZodIssue::too_small_number(json!(1), at()));
                }
            }
        }
    }
    nullable_timestamp_issues(row, "lastReinforcedAt", &mut issues);
    default_string_array_issues(row, "relatedMemoryIds", true, &mut issues);
    default_ranged_number_issues(row, "reinforcedImportance", 0, 1, &mut issues);
    zod_timestamp_issues(row, "createdAt", &mut issues);
    zod_timestamp_issues(row, "updatedAt", &mut issues);
    issues
}

/// v4 `ChatInformSchema` (`lib/schemas/chat-inform.types.ts:28-66`) over the
/// WHOLE entity `chatInforms.create` hands `_create` — the whole-row inform
/// twin (§S.2 of the `94fbb1ae3` smalls round, P4.161), in schema key order:
/// `id` / `chatId` / `batchId` / `participantId` uuid; `contentMarkdown`
/// string (`""` passes); `recordMessageId` uuid `.nullable().optional()`
/// (`""` FAILS); `permanent` boolean `.default(false)` (`null` fails);
/// the two stamps; `consumedAt` `TimestampSchema.nullable().optional()`;
/// `consumedByMessageId` uuid `.nullable().optional()`. Recorded through
/// v4's REAL schema (`repository_zod_messages`).
pub fn zod_chat_inform_issues(row: &serde_json::Map<String, Value>) -> Vec<ZodIssue> {
    let mut issues = Vec::new();
    for k in ["id", "chatId", "batchId", "participantId"] {
        zod_uuid_issues(row, k, false, &mut issues);
    }
    required_string_issues(row, "contentMarkdown", &mut issues);
    zod_uuid_issues(row, "recordMessageId", true, &mut issues);
    match row.get("permanent") {
        None | Some(Value::Bool(_)) => {}
        got => issues.push(ZodIssue::invalid_type(
            "boolean",
            vec![key("permanent")],
            got,
        )),
    }
    zod_timestamp_issues(row, "createdAt", &mut issues);
    zod_timestamp_issues(row, "updatedAt", &mut issues);
    nullable_timestamp_issues(row, "consumedAt", &mut issues);
    zod_uuid_issues(row, "consumedByMessageId", true, &mut issues);
    issues
}

/// A `z.string().min(lo)` / `.max(hi)` at `k` (CODE POINTS — zod ≥ 4.5's
/// `util.codePointLength`), REQUIRED.
fn required_bounded_string_issues(
    row: &serde_json::Map<String, Value>,
    k: &str,
    min: Option<usize>,
    max: Option<usize>,
    issues: &mut Vec<ZodIssue>,
) {
    match row.get(k) {
        Some(Value::String(v)) => {
            if let Some(lo) = min.filter(|lo| !crate::jsstr::zod_len_min_ok(v, *lo)) {
                issues.push(ZodIssue::too_small_string(json!(lo), vec![key(k)]));
            } else if let Some(hi) = max.filter(|hi| !crate::jsstr::zod_len_max_ok(v, *hi)) {
                issues.push(ZodIssue::too_big_string(json!(hi), vec![key(k)]));
            }
        }
        got => issues.push(ZodIssue::invalid_type("string", vec![key(k)], got)),
    }
}

/// v4 `PromptTemplateSchema` (`lib/schemas/template.types.ts:270-282`) over
/// the WHOLE entity `promptTemplates.create` hands `_create`, in schema key
/// order: `id` uuid; `userId` uuid `.nullable().optional()`; `name`
/// `.min(1).max(100)`; `content` `.min(1)`; `description` `.max(500)
/// .nullable().optional()`; `isBuiltIn` boolean `.default(false)`;
/// `category` / `modelHint` string `.nullable().optional()`; `tags` uuid[]
/// `.default([])`; the two stamps. Every bound is a recorded row of the
/// `repository_zod_messages` oracle (P4.161 Tier 2).
pub fn zod_prompt_template_issues(row: &serde_json::Map<String, Value>) -> Vec<ZodIssue> {
    let mut issues = Vec::new();
    zod_uuid_issues(row, "id", false, &mut issues);
    zod_uuid_issues(row, "userId", true, &mut issues);
    required_bounded_string_issues(row, "name", Some(1), Some(100), &mut issues);
    required_bounded_string_issues(row, "content", Some(1), None, &mut issues);
    nullable_string_issues(row, "description", &mut issues, |v| {
        (!crate::jsstr::zod_len_max_ok(v, 500))
            .then(|| ZodIssue::too_big_string(json!(500), vec![key("description")]))
    });
    match row.get("isBuiltIn") {
        None | Some(Value::Bool(_)) => {}
        got => issues.push(ZodIssue::invalid_type(
            "boolean",
            vec![key("isBuiltIn")],
            got,
        )),
    }
    nullable_plain_string_issues(row, "category", &mut issues);
    nullable_plain_string_issues(row, "modelHint", &mut issues);
    default_string_array_issues(row, "tags", true, &mut issues);
    zod_timestamp_issues(row, "createdAt", &mut issues);
    zod_timestamp_issues(row, "updatedAt", &mut issues);
    issues
}

/// v4 `GroupDocMountLinkSchema` (`lib/schemas/mount-index.types.ts`) — three
/// required uuids and two timestamps — over a raw `group_doc_mount_links` row
/// (the shape `findByFilter` `validateSafe()`s row by row). Zod's issue list;
/// empty = the row passes.
pub fn zod_group_doc_mount_link_issues(row: &serde_json::Map<String, Value>) -> Vec<ZodIssue> {
    let mut issues = Vec::new();
    for k in ["id", "groupId", "mountPointId"] {
        zod_uuid_issues(row, k, false, &mut issues);
    }
    zod_timestamp_issues(row, "createdAt", &mut issues);
    zod_timestamp_issues(row, "updatedAt", &mut issues);
    issues
}

/// `UUIDSchema` (`z.uuid()`) at `k` — `.nullable().optional()` when
/// `nullable`: a non-string is `invalid_type` `expected string`, a non-uuid
/// string [`ZodIssue::invalid_uuid`].
fn zod_uuid_issues(
    row: &serde_json::Map<String, Value>,
    k: &str,
    nullable: bool,
    issues: &mut Vec<ZodIssue>,
) {
    match row.get(k) {
        None | Some(Value::Null) if nullable => {}
        Some(Value::String(v)) => {
            if !zod_uuid_ok(v) {
                issues.push(ZodIssue::invalid_uuid(vec![key(k)]));
            }
        }
        got => issues.push(ZodIssue::invalid_type("string", vec![key(k)], got)),
    }
}

/// v4 `TimestampSchema` (`lib/schemas/common.types.ts`: `z.iso.datetime()
/// .or(z.date()).transform(…)`) at `k`. A STRING that fails the datetime check
/// did not abort the string option, so zod reports that option's issue bare
/// ([`ZodIssue::invalid_datetime`]); anything else (absent, `null`, a number,
/// a BLOB) aborts BOTH options — `invalid_union` with an `expected string` and
/// an `expected date` issue, each at `path: []` (measured at zod 4.6.5).
fn zod_timestamp_issues(row: &serde_json::Map<String, Value>, k: &str, issues: &mut Vec<ZodIssue>) {
    match row.get(k) {
        Some(Value::String(s)) => {
            if !zod_iso_datetime_ok(s) {
                issues.push(ZodIssue::invalid_datetime(vec![key(k)]));
            }
        }
        got => issues.push(ZodIssue::invalid_union(
            vec![
                vec![ZodIssue::invalid_type("string", vec![], got)],
                vec![ZodIssue::invalid_type("date", vec![], got)],
            ],
            vec![key(k)],
        )),
    }
}

/// v4 `RouteAttemptViaEnum` (`lib/schemas/chat.types.ts`), in schema order.
const ROUTE_ATTEMPT_VIA: [&str; 5] = ["primary", "retry", "concierge", "understudy", "tier-pick"];
/// v4 `RouteAttemptOutcomeEnum`, in schema order.
const ROUTE_ATTEMPT_OUTCOME: [&str; 3] = ["answered", "failed", "refused"];
/// `RouteAttemptSchema.trigger`'s enum, in schema order — the VALUES an
/// `invalid_value` issue echoes. Membership is decided by v5's ONE union
/// ([`FallbackTrigger::from_wire`](crate::llm_fallback::FallbackTrigger::from_wire));
/// `route_attempt_enum_lists_are_the_wire_unions` pins that the two agree.
const ROUTE_ATTEMPT_TRIGGER: [&str; 7] = [
    "auth",
    "rate-limit",
    "network",
    "model-missing",
    "provider-error",
    "empty-response",
    "moderation-refusal",
];
/// `RouteAttemptSchema.evidence`'s enum, in schema order (membership:
/// [`RefusalEvidence::from_wire`](crate::services::dangerous_content::refusal::RefusalEvidence::from_wire)).
const ROUTE_ATTEMPT_EVIDENCE: [&str; 5] = [
    "typed-error",
    "provider-code",
    "finish-reason",
    "message-pattern",
    "inferred",
];
/// `RouteAttemptSchema.profileKind`'s enum.
const ROUTE_ATTEMPT_PROFILE_KIND: [&str; 2] = ["connection", "image"];

/// A `UUIDSchema` (`z.uuid()`) key that is PRESENT: a non-string is the
/// string type check's `invalid_type` (aborting), a string that is not a Zod
/// uuid the format check's `invalid_format` (continuing).
fn uuid_key_issues(got: &Value, path: Vec<Value>, issues: &mut Vec<ZodIssue>) {
    match got {
        Value::String(s) if zod_uuid_ok(s) => {}
        Value::String(_) => issues.push(ZodIssue::invalid_uuid(path)),
        other => issues.push(ZodIssue::invalid_type("string", path, Some(other))),
    }
}

/// `path` with one more element appended — Zod's `prefixIssues` builds a
/// child's path the same way, outermost first.
fn child_path(prefix: &[Value], k: Value) -> Vec<Value> {
    let mut p = prefix.to_vec();
    p.push(k);
    p
}

/// v4 `RouteAttemptSchema` (`lib/schemas/chat.types.ts`) — the STRICT twin of
/// one `routeTrail` element as v4's per-row `ChatEventSchema.safeParse` meets
/// it (`MessageEventSchema.routeTrail` is `RouteAttemptSchema.array()
/// .nullable().optional()`). Returns EVERY issue zod 4.6.5 raises for the
/// element, in schema key order, each path prefixed by `prefix` (the
/// element's own position, e.g. `["routeTrail", 1]` — `&[]` for a bare
/// element). Empty = the element passes. P4.143 made this the issue SOURCE
/// (P4.130's twin answered a pre-rendered first-failure string).
///
/// The schema, key by key: `profileId: UUIDSchema`; `profileName`,
/// `provider`, `modelName: z.string()`; `via` (`RouteAttemptViaEnum`, 5) and
/// `outcome` (`RouteAttemptOutcomeEnum`, 3); `trigger` (7 — v5's ONE
/// [`FallbackTrigger`](crate::llm_fallback::FallbackTrigger) union),
/// `evidence` (5 — the classifier's
/// [`RefusalEvidence`](crate::services::dangerous_content::refusal::RefusalEvidence)),
/// `profileKind` (`connection` | `image`) and `detail: z.string().max(200)`
/// (CODE POINTS) — all `.optional()`, so ABSENT passes and an explicit `null`
/// FAILS (`.optional()` is not `.nullable()`). Unknown keys are stripped, not
/// refused. A non-object element is the object type check's one
/// `invalid_type` (zod checks no key of it). v5's lenient reader
/// `RouteAttempt::from_value` (`services/route_trail.rs`) had read `null` as
/// absent and checked neither the uuid nor the length, so a row v4 skips was
/// KEPT (and re-saved by the "Try uncensored" picture route).
///
/// Which issues ABORT (type / enum) and which CONTINUE (the uuid format, the
/// `detail` length) is [`zod_issue_aborts`]'s; this source does not collapse —
/// the union collapse belongs to the chat event ([`zod_chat_event_issues`]).
pub fn zod_route_attempt_issues(v: &Value, prefix: &[Value]) -> Vec<ZodIssue> {
    let mut issues = Vec::new();
    let Some(o) = v.as_object() else {
        issues.push(ZodIssue::invalid_type("object", prefix.to_vec(), Some(v)));
        return issues;
    };
    let at = |k: &str| child_path(prefix, key(k));
    match o.get("profileId") {
        Some(got) => uuid_key_issues(got, at("profileId"), &mut issues),
        None => issues.push(ZodIssue::invalid_type("string", at("profileId"), None)),
    }
    for k in ["profileName", "provider", "modelName"] {
        if !o.get(k).is_some_and(Value::is_string) {
            issues.push(ZodIssue::invalid_type("string", at(k), o.get(k)));
        }
    }
    // `z.enum` compares values and has no separate type gate, so an absent
    // REQUIRED key and a wrong type are the same `invalid_value`.
    let mut member =
        |k: &str, values: &[&str], ok: &dyn Fn(&str) -> bool, required: bool| match o.get(k) {
            None if !required => {}
            Some(Value::String(s)) if ok(s) => {}
            _ => issues.push(ZodIssue::invalid_value(values, at(k))),
        };
    member(
        "via",
        &ROUTE_ATTEMPT_VIA,
        &|s| ROUTE_ATTEMPT_VIA.contains(&s),
        true,
    );
    member(
        "outcome",
        &ROUTE_ATTEMPT_OUTCOME,
        &|s| ROUTE_ATTEMPT_OUTCOME.contains(&s),
        true,
    );
    member(
        "trigger",
        &ROUTE_ATTEMPT_TRIGGER,
        &|s| crate::llm_fallback::FallbackTrigger::from_wire(s).is_some(),
        false,
    );
    member(
        "evidence",
        &ROUTE_ATTEMPT_EVIDENCE,
        &|s| crate::services::dangerous_content::refusal::RefusalEvidence::from_wire(s).is_some(),
        false,
    );
    member(
        "profileKind",
        &ROUTE_ATTEMPT_PROFILE_KIND,
        &|s| ROUTE_ATTEMPT_PROFILE_KIND.contains(&s),
        false,
    );
    match o.get("detail") {
        None => {}
        Some(Value::String(d)) if crate::jsstr::zod_len_max_ok(d, 200) => {}
        Some(Value::String(_)) => issues.push(ZodIssue::too_big_string(json!(200), at("detail"))),
        got => issues.push(ZodIssue::invalid_type("string", at("detail"), got)),
    }
    issues
}

/// [`zod_route_attempt_issues`] over a bare element, rendered — `None` when the
/// element passes, else its issue lines (`path: message`) joined with `", "`.
/// Kept for the callers that only ask WHETHER an element passes
/// (`api/chat_media.rs`'s `saved_trail` filter, `.is_none()`); its `Some` /
/// `None` set is exactly [`zod_route_attempt_issues`]'s non-empty / empty
/// (P4.143 kept the signature so no caller moved).
pub fn zod_route_attempt_failure(v: &Value) -> Option<String> {
    let issues = zod_route_attempt_issues(v, &[]);
    (!issues.is_empty()).then(|| zod_issue_lines(&issues, ": ").join(", "))
}

/// Does this issue ABORT its schema's parse in zod 4.6.5's sense — i.e. does
/// it lack `continue: true` (`util.aborted`, `core/util.js:520-529`)? Zod's
/// TYPE gates (`invalid_type`, every `invalid_value` — enum and literal alike
/// — and a nested `invalid_union`) push an issue with no `continue`; its
/// CHECKS (`invalid_format` uuid / datetime, the `too_big` / `too_small`
/// length and range bounds, a `.refine`'s `custom`) push `continue: true`
/// unless declared `abort` — none in the schemas this module twins is.
pub fn zod_issue_aborts(issue: &ZodIssue) -> bool {
    match issue {
        ZodIssue::InvalidType { .. }
        | ZodIssue::InvalidIntType { .. }
        | ZodIssue::InvalidValue { .. }
        | ZodIssue::InvalidInstance { .. }
        | ZodIssue::InvalidUnion { .. } => true,
        ZodIssue::InvalidFormat { .. }
        | ZodIssue::TooSmall { .. }
        | ZodIssue::TooBig { .. }
        | ZodIssue::TooSmallInt { .. }
        | ZodIssue::TooBigInt { .. }
        | ZodIssue::Custom { .. } => false,
    }
}

/// v4's `RoleEnum` (`lib/schemas/common.types.ts:38`), in schema order.
const ROLE_ENUM: [&str; 4] = ["SYSTEM", "USER", "ASSISTANT", "TOOL"];
/// `hostEvent.toStatus`'s enum (`MessageEventSchema`), in schema order.
const HOST_EVENT_STATUSES: [&str; 4] = ["active", "silent", "absent", "removed"];

/// v4 `ChatEventSchema.safeParse(event)`'s issue list — the `errors` v4's
/// `getMessages` WARN renders (`chats-messages.ops.ts:343-356`) — over the
/// shapes v5 checks (P4.143; the checks are P4.112 / P4.113 / P4.130's):
/// on all three members `id: UUIDSchema` and `createdAt: TimestampSchema`;
/// on a message `role: RoleEnum`, `participantId: UUIDSchema.nullable()
/// .optional()`, `routeTrail` (non-array, then each element through
/// [`zod_route_attempt_issues`]) and `hostEvent` (`z.object({ participantId?:
/// uuid, toStatus?: enum, introducedCharacterIds?: uuid[] }).nullable()
/// .optional()`). Issues come in each member's SCHEMA KEY ORDER — the
/// message's is `id`, `role`, `createdAt`, `participantId`, `routeTrail`,
/// `hostEvent`; the other two members' `id`, `createdAt`.
///
/// **The union collapse (measured, zod 4.6.5).** `ChatEventSchema` is a plain
/// `z.union([MessageEventSchema, ContextSummaryEventSchema,
/// SystemEventSchema])` (`chat.types.ts:637-641`), and `handleUnionResults`
/// (`core/schemas.js:1195-1214`) answers the lone NON-ABORTED option's issues
/// when exactly one option did not abort, else ONE `invalid_union`. The two
/// members whose `type` literal fails always abort, so: when every issue of
/// the row's own member CONTINUES ([`zod_issue_aborts`] false — uuid /
/// datetime formats, the `detail` length), v4 logs ALL of them; when ANY
/// aborts (a `null` trigger, a bad `via`, a non-array trail, a bad `role`, a
/// non-object `hostEvent` …), v4 logs the single collapsed issue (`path:
/// []`, `"Invalid input"` → the line `": Invalid input"`). This answers that
/// collapsed issue with `errors: []` — sufficient for the WARN's `path:
/// message` projection, and NOT v4's `ZodError.message` (v4's `errors` holds
/// each member's nested issues; [`zod_error_message`] over this is not v4's —
/// P4.143 Tier 3 item 13, `updateMessage`'s ERROR).
///
/// Out of scope (recorded in P4.112's lane record, never silently claimed):
/// every other member field v4 can reject (`content`, `recoveryType`,
/// `attachments`' uuids, `systemEventType` …) — a NULL `content` is a cell
/// error the read path collapses itself. A non-object event and an UNKNOWN
/// `type` answer `[]` here: the read path collapses an unknown `type` before
/// it calls this, and `updateMessage`'s merged check
/// (`db/chats_messages.rs`) has never refused one (P4.143 kept every
/// caller's pass/fail set unchanged).
pub fn zod_chat_event_issues(event: &Value) -> Vec<ZodIssue> {
    let mut issues = Vec::new();
    let Some(obj) = event.as_object() else {
        return issues;
    };
    let is_message = match obj.get("type").and_then(Value::as_str) {
        Some("message") => true,
        Some("context-summary" | "system") => false,
        _ => return issues,
    };
    match obj.get("id") {
        Some(got) => uuid_key_issues(got, vec![key("id")], &mut issues),
        None => issues.push(ZodIssue::invalid_type("string", vec![key("id")], None)),
    }
    if is_message {
        let role_ok = obj
            .get("role")
            .and_then(Value::as_str)
            .is_some_and(|r| ROLE_ENUM.contains(&r));
        if !role_ok {
            issues.push(ZodIssue::invalid_value(&ROLE_ENUM, vec![key("role")]));
        }
    }
    // `TimestampSchema` = `z.iso.datetime().or(z.date())`: a malformed STRING
    // is the lone non-aborted option's `invalid_format` (continuing), anything
    // else aborts both options (an `invalid_union`).
    zod_timestamp_issues(obj, "createdAt", &mut issues);
    if !is_message {
        return chat_event_union_collapse(issues);
    }
    if let Some(p) = obj.get("participantId").filter(|p| !p.is_null()) {
        uuid_key_issues(p, vec![key("participantId")], &mut issues);
    }
    if let Some(trail) = obj.get("routeTrail").filter(|t| !t.is_null()) {
        match trail.as_array() {
            None => issues.push(ZodIssue::invalid_type(
                "array",
                vec![key("routeTrail")],
                Some(trail),
            )),
            Some(rows) => {
                for (i, row) in rows.iter().enumerate() {
                    issues.extend(zod_route_attempt_issues(
                        row,
                        &[key("routeTrail"), json!(i)],
                    ));
                }
            }
        }
    }
    // `.nullable().optional()` on the object itself: a `null` hostEvent passes
    // (the read path never sees one — `put_opt_json` drops it — but a MERGED
    // update event does: `{ hostEvent: null }` is the repair, P4.113). Inside
    // it every key is `.optional()` only, so a PRESENT `null` fails.
    if let Some(host) = obj.get("hostEvent").filter(|h| !h.is_null()) {
        let at = |k: &str| vec![key("hostEvent"), key(k)];
        match host.as_object() {
            None => issues.push(ZodIssue::invalid_type(
                "object",
                vec![key("hostEvent")],
                Some(host),
            )),
            Some(h) => {
                if let Some(p) = h.get("participantId") {
                    uuid_key_issues(p, at("participantId"), &mut issues);
                }
                if let Some(s) = h.get("toStatus") {
                    if !s.as_str().is_some_and(|s| HOST_EVENT_STATUSES.contains(&s)) {
                        issues.push(ZodIssue::invalid_value(
                            &HOST_EVENT_STATUSES,
                            at("toStatus"),
                        ));
                    }
                }
                if let Some(ids) = h.get("introducedCharacterIds") {
                    match ids.as_array() {
                        None => issues.push(ZodIssue::invalid_type(
                            "array",
                            at("introducedCharacterIds"),
                            Some(ids),
                        )),
                        Some(a) => {
                            for (i, id) in a.iter().enumerate() {
                                uuid_key_issues(
                                    id,
                                    child_path(&at("introducedCharacterIds"), json!(i)),
                                    &mut issues,
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    chat_event_union_collapse(issues)
}

/// `handleUnionResults` over the row's own member: its issues when none
/// aborts, else the ONE collapsed `invalid_union` (see
/// [`zod_chat_event_issues`]).
fn chat_event_union_collapse(issues: Vec<ZodIssue>) -> Vec<ZodIssue> {
    if issues.iter().any(zod_issue_aborts) {
        vec![ZodIssue::invalid_union(vec![], vec![])]
    } else {
        issues
    }
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

/// The SAME `z.iso.datetime()` pattern as zod ECHOES it in an `invalid_format`
/// issue's `pattern` VALUE — JS form, `\d` rather than [`ZOD_ISO_DATETIME_PATTERN`]'s
/// `[0-9]` rewrite, wrapped in the regex literal's slashes. Transcribed
/// verbatim from the `repository_zod_messages` oracle at `97b25fc53` (zod
/// 4.6.5, P4.130); `datetime_js_pattern_is_the_matcher_in_js_form` pins that
/// the two constants are one pattern.
pub const ZOD_ISO_DATETIME_JS_PATTERN: &str = r"/^(?:(?:\d\d[2468][048]|\d\d[13579][26]|\d\d0[48]|[02468][048]00|[13579][26]00)-02-29|\d{4}-(?:(?:0[13578]|1[02])-(?:0[1-9]|[12]\d|3[01])|(?:0[469]|11)-(?:0[1-9]|[12]\d|30)|(?:02)-(?:0[1-9]|1\d|2[0-8])))T(?:(?:[01]\d|2[0-3]):[0-5]\d:[0-5]\d(?:\.\d+)?(?:Z))$/";

/// The key of the one-entry object that stands for a JS `Float32Array` of
/// `n` elements (the entry's value) in a row handed to this module
/// ([`zod_float32_array_cell`]). A NUL-led key no JSON body plausibly carries,
/// so an ordinary object never reads as one.
///
/// **Why a `Float32Array` and not a `Buffer` (MEASURED, P4.130 — the order
/// predicted `received Buffer`):** better-sqlite3 hands a BLOB back as a
/// `Buffer`, but v4's SQLite collection hydrates every row before Zod sees it,
/// and a `Buffer` in a non-BLOB, non-JSON, non-boolean column is decoded
/// `blobToEmbedding(value)` (`lib/database/backends/sqlite/backend.ts`, "Buffer
/// in non-blob column, decoding as Float32") — a `Float32Array` of the
/// header-aware element count ([`crate::embedding_blob::blob_to_float32`]).
/// The scenario-builder mount-pool oracle's BLOB-named group logs `received
/// Float32Array` and, for a zero-element array, a second `too_small` issue.
pub const ZOD_FLOAT32_ARRAY_MARKER: &str = "\u{0}Float32Array";

/// A `Float32Array` of `len` elements as this module's rows carry it
/// ([`ZOD_FLOAT32_ARRAY_MARKER`]); [`zod_parsed_type`] answers `Float32Array`.
pub fn zod_float32_array_cell(len: usize) -> Value {
    let mut o = serde_json::Map::new();
    o.insert(ZOD_FLOAT32_ARRAY_MARKER.to_string(), json!(len));
    Value::Object(o)
}

/// The element count of a [`zod_float32_array_cell`], `None` for anything else.
fn float32_array_len(v: Option<&Value>) -> Option<u64> {
    match v {
        Some(Value::Object(o)) if o.len() == 1 => {
            o.get(ZOD_FLOAT32_ARRAY_MARKER).and_then(Value::as_u64)
        }
        _ => None,
    }
}

/// v4 `util.parsedType` (the same table the Pascal Zod port pins). `None` is
/// JS `undefined` — a missing key. An object whose prototype is not
/// `Object.prototype` answers its `constructor.name` in zod
/// (`core/util.js` `parsedType`); the one such value a v4 row carries is a
/// BLOB cell, hydrated to a `Float32Array` ([`ZOD_FLOAT32_ARRAY_MARKER`]).
pub fn zod_parsed_type(v: Option<&Value>) -> &'static str {
    match v {
        None => "undefined",
        Some(Value::Null) => "null",
        Some(Value::Bool(_)) => "boolean",
        Some(Value::Number(_)) => "number",
        Some(Value::String(_)) => "string",
        Some(Value::Array(_)) => "array",
        v @ Some(Value::Object(_)) if float32_array_len(v).is_some() => "Float32Array",
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
    /// P4.124: v4 `GroupSchema` — each shape the order named, both edges (the
    /// messages themselves are `repository_zod_messages_equivalence`'s, over
    /// v4's real schema).
    #[test]
    fn group_schema_holds_the_five_columns() {
        let zod_group_row_ok = |r: &serde_json::Map<String, Value>| zod_group_issues(r).is_empty();
        let ok = serde_json::json!({
            "id": "d2310000-0000-4000-8000-0000000000c1",
            "name": "Loners",
            "officialMountPointId": "e2000000-0000-4000-8000-0000000000f3",
            "createdAt": "2026-01-02T03:04:05.000Z",
            "updatedAt": "2026-01-02T03:04:05.000Z",
        });
        let row = |patch: serde_json::Value| {
            let mut r = ok.as_object().unwrap().clone();
            for (k, v) in patch.as_object().unwrap() {
                if v == &serde_json::json!("<absent>") {
                    r.remove(k);
                } else {
                    r.insert(k.clone(), v.clone());
                }
            }
            r
        };
        assert!(zod_group_row_ok(ok.as_object().unwrap()));
        // `.nullable().optional()`: absent and null both pass.
        assert!(zod_group_row_ok(&row(
            serde_json::json!({"officialMountPointId": "<absent>"})
        )));
        assert!(zod_group_row_ok(&row(
            serde_json::json!({"officialMountPointId": null})
        )));
        // `min(1).max(100)` in CODE POINTS (zod 4.6.5): 100 passes, 101 fails;
        // an astral character counts ONE — 99 × `x` + one astral character is
        // 101 UTF-16 units and PASSES in v4 (P4.130, measured at `97b25fc53`).
        assert!(zod_group_row_ok(&row(
            serde_json::json!({"name": "x".repeat(100)})
        )));
        assert!(!zod_group_row_ok(&row(
            serde_json::json!({"name": "x".repeat(101)})
        )));
        assert!(zod_group_row_ok(&row(
            serde_json::json!({"name": format!("{}\u{1F600}", "x".repeat(99))})
        )));
        assert!(!zod_group_row_ok(&row(
            serde_json::json!({"name": format!("{}\u{1F600}", "x".repeat(100))})
        )));
        for bad in [
            serde_json::json!({"name": ""}),
            serde_json::json!({"name": zod_float32_array_cell(1)}),
            serde_json::json!({"name": "<absent>"}),
            serde_json::json!({"id": "d2310000-0000-0000-8000-0000000000c5"}),
            serde_json::json!({"officialMountPointId": "e2000000-0000-0000-8000-0000000000f3"}),
            serde_json::json!({"officialMountPointId": 7}),
            serde_json::json!({"createdAt": "2026-01-02"}),
            serde_json::json!({"updatedAt": "<absent>"}),
        ] {
            assert!(!zod_group_row_ok(&row(bad.clone())), "{bad}");
        }
    }

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
    /// `zod` output recorded in this module's header (4.5.4, re-measured at
    /// 4.6.5 by P4.130). The whole point is
    /// KEY ORDER, so each row compares the serialized STRING, not a `Value`
    /// equality (which would be order-blind for a `Map`-backed comparison).
    #[test]
    fn render_table_matches_real_zod_465() {
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
                "invalid_format/regex",
                ZodIssue::invalid_regex(ZOD_HEX_COLOR_PATTERN, vec![key("a")]),
                r#"{"origin":"string","code":"invalid_format","format":"regex","pattern":"/^#(?:[0-9a-fA-F]{3}){1,2}$/","path":["a"],"message":"Invalid string: must match pattern /^#(?:[0-9a-fA-F]{3}){1,2}$/"}"#,
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
            // P4.130 — measured at 4.6.5 by the header command.
            (
                "invalid_format/datetime",
                ZodIssue::invalid_datetime(vec![key("a")]),
                r#"{"origin":"string","code":"invalid_format","format":"datetime","pattern":"/^(?:(?:\\d\\d[2468][048]|\\d\\d[13579][26]|\\d\\d0[48]|[02468][048]00|[13579][26]00)-02-29|\\d{4}-(?:(?:0[13578]|1[02])-(?:0[1-9]|[12]\\d|3[01])|(?:0[469]|11)-(?:0[1-9]|[12]\\d|30)|(?:02)-(?:0[1-9]|1\\d|2[0-8])))T(?:(?:[01]\\d|2[0-3]):[0-5]\\d:[0-5]\\d(?:\\.\\d+)?(?:Z))$/","path":["a"],"message":"Invalid ISO datetime"}"#,
            ),
            (
                "invalid_union/timestamp",
                ZodIssue::invalid_union(
                    vec![
                        vec![ZodIssue::invalid_type("string", vec![], None)],
                        vec![ZodIssue::invalid_type("date", vec![], None)],
                    ],
                    vec![key("a")],
                ),
                r#"{"code":"invalid_union","errors":[[{"expected":"string","code":"invalid_type","path":[],"message":"Invalid input: expected string, received undefined"}],[{"expected":"date","code":"invalid_type","path":[],"message":"Invalid input: expected date, received undefined"}]],"path":["a"],"message":"Invalid input"}"#,
            ),
            (
                "invalid_type/float32array",
                ZodIssue::invalid_type("string", vec![key("a")], Some(&zod_float32_array_cell(1))),
                r#"{"expected":"string","code":"invalid_type","path":["a"],"message":"Invalid input: expected string, received Float32Array"}"#,
            ),
            (
                "too_small/unknown",
                ZodIssue::too_small_unknown(json!(1), vec![key("a")]),
                r#"{"origin":"unknown","code":"too_small","minimum":1,"inclusive":true,"path":["a"],"message":"Too small: expected unknown to be >=1"}"#,
            ),
            (
                "too_big/unknown",
                ZodIssue::too_big_unknown(json!(100), vec![key("a")]),
                r#"{"origin":"unknown","code":"too_big","maximum":100,"inclusive":true,"path":["a"],"message":"Too big: expected unknown to be <=100"}"#,
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
        // P4.130: a BLOB cell reaches v4's Zod as a `Float32Array`; an
        // ordinary one-key object (even one spelling the word) stays an object.
        assert_eq!(
            zod_parsed_type(Some(&zod_float32_array_cell(3))),
            "Float32Array"
        );
        assert_eq!(zod_parsed_type(Some(&json!({"Float32Array": 3}))), "object");
        assert_eq!(zod_parsed_type(Some(&json!({"$float32": 3}))), "object");
    }

    /// P4.130 — v4 `RouteAttemptSchema`, each shape the strict twin refuses
    /// and the lenient reader had kept (the read-side proof is
    /// `chats_messages_ops_tier2` + `retry_uncensored_tier3` over v4's real
    /// `getMessages` / route).
    #[test]
    fn route_attempt_schema_is_strict() {
        let ok = json!({
            "profileId": "a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11",
            "profileName": "Frank Desk",
            "provider": "OPENAI",
            "modelName": "gpt-image-2",
            "via": "primary",
            "outcome": "refused",
            "trigger": "moderation-refusal",
            "evidence": "provider-code",
            "profileKind": "image",
            "detail": "x".repeat(200),
            "unknownKey": "stripped, not refused",
        });
        assert_eq!(zod_route_attempt_failure(&ok), None);
        let with = |k: &str, v: Value| {
            let mut o = ok.clone();
            o[k] = v;
            o
        };
        let without = |k: &str| {
            let mut o = ok.clone();
            o.as_object_mut().unwrap().remove(k);
            o
        };
        for k in ["trigger", "evidence", "profileKind", "detail"] {
            assert_eq!(zod_route_attempt_failure(&without(k)), None, "{k} absent");
            assert!(
                zod_route_attempt_failure(&with(k, Value::Null)).is_some(),
                "{k}: null"
            );
        }
        // 199 × x + one astral = 200 code points: passes.
        assert_eq!(
            zod_route_attempt_failure(&with(
                "detail",
                json!(format!("{}\u{1F600}", "x".repeat(199)))
            )),
            None
        );
        for (k, v) in [
            ("profileId", json!("not-a-uuid")),
            ("profileId", json!("a0eebc99-9c0b-9ef8-bb6d-6bb9bd380a11")),
            ("profileName", json!(7)),
            ("via", json!("sideways")),
            ("outcome", json!("maybe")),
            ("trigger", json!("boredom")),
            ("evidence", json!("hunch")),
            ("profileKind", json!("embedding")),
            ("detail", json!("x".repeat(201))),
        ] {
            assert!(
                zod_route_attempt_failure(&with(k, v.clone())).is_some(),
                "{k}: {v}"
            );
        }
        assert!(zod_route_attempt_failure(&without("profileId")).is_some());
        assert!(zod_route_attempt_failure(&json!("row")).is_some());
    }

    /// The enum lists an `invalid_value` echoes are the wire unions v5 decides
    /// membership by — every listed value is a member, and the counts are
    /// v4's (`RouteAttemptSchema`, `lib/schemas/chat.types.ts`).
    #[test]
    fn route_attempt_enum_lists_are_the_wire_unions() {
        for t in ROUTE_ATTEMPT_TRIGGER {
            assert!(
                crate::llm_fallback::FallbackTrigger::from_wire(t).is_some(),
                "{t}"
            );
        }
        for e in ROUTE_ATTEMPT_EVIDENCE {
            assert!(
                crate::services::dangerous_content::refusal::RefusalEvidence::from_wire(e)
                    .is_some(),
                "{e}"
            );
        }
        assert!(crate::llm_fallback::FallbackTrigger::from_wire("boredom").is_none());
        assert!(
            crate::services::dangerous_content::refusal::RefusalEvidence::from_wire("hunch")
                .is_none()
        );
    }

    /// P4.143 — [`zod_chat_event_issues`] rendered as v4's WARN renders it
    /// (`issues.map(i => `${i.path.join('.')}: ${i.message}`)`) against the
    /// lines v4's REAL `ChatEventSchema.safeParse` produced for the same
    /// shapes: a throwaway `npx tsx` probe run from the `f6426e196` pin (zod
    /// 4.6.5), recorded verbatim in P4.143's lane record. `None` = v4 keeps the
    /// row. The table is the byte reference for the union collapse: every
    /// row whose member carries an aborting issue is `[": Invalid input"]`;
    /// check-only rows log every issue in schema key order.
    #[test]
    fn chat_event_issues_match_v4s_errors() {
        let trail_ok = json!({
            "profileId": "a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11", "profileName": "Frank Desk",
            "provider": "OPENAI", "modelName": "gpt-image-2", "via": "primary",
            "outcome": "refused", "trigger": "moderation-refusal",
            "evidence": "provider-code", "profileKind": "image", "detail": "declined",
        });
        let el = |patch: Value| {
            let mut o = trail_ok.clone();
            for (k, v) in patch.as_object().unwrap() {
                if v == &json!("<absent>") {
                    o.as_object_mut().unwrap().remove(k);
                } else {
                    o[k] = v.clone();
                }
            }
            o
        };
        let base = |typ: &str, n: char, extra: Value| {
            let mut o = json!({
                "type": typ, "id": format!("e00000b0-0000-4000-8000-00000000000{n}"),
                "createdAt": "2026-03-01T00:00:01.000Z",
            });
            for (k, v) in extra.as_object().unwrap() {
                o[k] = v.clone();
            }
            o
        };
        let msg = |patch: Value| {
            let mut o = base(
                "message",
                '1',
                json!({"role": "ASSISTANT", "content": "c", "attachments": []}),
            );
            for (k, v) in patch.as_object().unwrap() {
                o[k] = v.clone();
            }
            o
        };
        let cs = |patch: Value| {
            let mut o = base("context-summary", '2', json!({"context": "ctx"}));
            for (k, v) in patch.as_object().unwrap() {
                o[k] = v.clone();
            }
            o
        };
        let sys = |patch: Value| {
            let mut o = base(
                "system",
                '3',
                json!({"systemEventType": "SUMMARIZATION", "description": "d"}),
            );
            for (k, v) in patch.as_object().unwrap() {
                o[k] = v.clone();
            }
            o
        };
        let x201 = "x".repeat(201);
        let collapsed: Option<&[&str]> = Some(&[": Invalid input"]);
        let table: Vec<(&str, Value, Option<&[&str]>)> = vec![
            (
                "trail_valid_two",
                msg(json!({"routeTrail": [trail_ok.clone(),
                    el(json!({"outcome": "answered", "trigger": "<absent>", "evidence": "<absent>"}))]})),
                None,
            ),
            (
                "trail_bad_profile_id",
                msg(json!({"routeTrail": [el(json!({"profileId": "not-a-profile"}))]})),
                Some(&["routeTrail.0.profileId: Invalid UUID"]),
            ),
            (
                "trail_detail_201",
                msg(json!({"routeTrail": [el(json!({"detail": x201}))]})),
                Some(&["routeTrail.0.detail: Too big: expected string to have <=200 characters"]),
            ),
            (
                "trail_null_trigger",
                msg(json!({"routeTrail": [el(json!({"trigger": null}))]})),
                collapsed,
            ),
            (
                "trail_detail_200_astral",
                msg(
                    json!({"routeTrail": [el(json!({"detail": format!("{}\u{1F600}", "x".repeat(199))}))]}),
                ),
                None,
            ),
            (
                "trail_bad_via_second",
                msg(json!({"routeTrail": [trail_ok.clone(), el(json!({"via": "sideways"}))]})),
                collapsed,
            ),
            (
                "trail_not_array",
                msg(json!({"routeTrail": {"not": "an array"}})),
                collapsed,
            ),
            (
                "trail_element_not_object",
                msg(json!({"routeTrail": ["row"]})),
                collapsed,
            ),
            (
                "trail_detail_number",
                msg(json!({"routeTrail": [el(json!({"detail": 42}))]})),
                collapsed,
            ),
            (
                "trail_profile_id_missing",
                msg(json!({"routeTrail": [el(json!({"profileId": "<absent>"}))]})),
                collapsed,
            ),
            (
                "trail_two_checks_one_element",
                msg(
                    json!({"routeTrail": [el(json!({"profileId": "not-a-profile", "detail": x201}))]}),
                ),
                Some(&[
                    "routeTrail.0.profileId: Invalid UUID",
                    "routeTrail.0.detail: Too big: expected string to have <=200 characters",
                ]),
            ),
            ("bad_role", msg(json!({"role": "narrator"})), collapsed),
            (
                "bad_id",
                msg(json!({"id": "not-a-uuid"})),
                Some(&["id: Invalid UUID"]),
            ),
            ("host_event_42", msg(json!({"hostEvent": 42})), collapsed),
            (
                "host_event_bad_status",
                msg(json!({"hostEvent": {"toStatus": "asleep"}})),
                collapsed,
            ),
            (
                "host_event_bad_participant",
                msg(json!({"hostEvent": {"participantId": "not-a-participant"}})),
                Some(&["hostEvent.participantId: Invalid UUID"]),
            ),
            (
                "host_event_bad_introduced_1",
                msg(json!({"hostEvent": {"introducedCharacterIds":
                    ["fb000090-0000-4000-8000-000000000001", "not-a-character"]}})),
                Some(&["hostEvent.introducedCharacterIds.1: Invalid UUID"]),
            ),
            (
                "host_event_null_participant",
                msg(json!({"hostEvent": {"participantId": null}})),
                collapsed,
            ),
            (
                "host_event_valid",
                msg(
                    json!({"hostEvent": {"participantId": "fb000090-0000-4000-8000-000000000001",
                    "toStatus": "active", "introducedCharacterIds": []}}),
                ),
                None,
            ),
            (
                "message_created_no_seconds",
                msg(json!({"createdAt": "2024-01-01T10:00Z"})),
                Some(&["createdAt: Invalid ISO datetime"]),
            ),
            (
                "message_created_offset",
                msg(json!({"createdAt": "2024-01-01T10:00:00+01:00"})),
                Some(&["createdAt: Invalid ISO datetime"]),
            ),
            (
                "context_summary_created_yesterday",
                cs(json!({"createdAt": "yesterday"})),
                Some(&["createdAt: Invalid ISO datetime"]),
            ),
            (
                "system_created_yesterday",
                sys(json!({"createdAt": "yesterday"})),
                Some(&["createdAt: Invalid ISO datetime"]),
            ),
            (
                "system_created_no_seconds",
                sys(json!({"createdAt": "2024-01-01T10:00Z"})),
                Some(&["createdAt: Invalid ISO datetime"]),
            ),
            (
                "context_summary_bad_id",
                cs(json!({"id": "not-a-uuid"})),
                Some(&["id: Invalid UUID"]),
            ),
            (
                "message_created_null",
                msg(json!({"createdAt": null})),
                collapsed,
            ),
            (
                "bad_participant_id",
                msg(json!({"participantId": "not-a-participant"})),
                Some(&["participantId: Invalid UUID"]),
            ),
            (
                "null_participant_id",
                msg(json!({"participantId": null})),
                None,
            ),
            (
                "number_participant_id",
                msg(json!({"participantId": 7})),
                collapsed,
            ),
            (
                "two_checks_id_and_detail",
                msg(json!({"id": "not-a-uuid", "routeTrail": [el(json!({"detail": x201}))]})),
                Some(&[
                    "id: Invalid UUID",
                    "routeTrail.0.detail: Too big: expected string to have <=200 characters",
                ]),
            ),
            (
                "three_checks_id_created_participant",
                msg(json!({"id": "not-a-uuid", "createdAt": "yesterday", "participantId": "nope"})),
                Some(&[
                    "id: Invalid UUID",
                    "createdAt: Invalid ISO datetime",
                    "participantId: Invalid UUID",
                ]),
            ),
            (
                "check_then_abort",
                msg(json!({"id": "not-a-uuid", "role": "narrator"})),
                collapsed,
            ),
        ];
        assert_eq!(
            table.len(),
            32,
            "the probe's shapes minus its two cell-level rows"
        );
        for (name, event, want) in table {
            let issues = zod_chat_event_issues(&event);
            let got = zod_issue_lines(&issues, ": ");
            match want {
                None => assert!(
                    got.is_empty(),
                    "{name}: v4 keeps the row, v5 logged {got:?}"
                ),
                Some(lines) => assert_eq!(got, lines, "{name}"),
            }
        }
        // The probe's two cell-level rows (`null_content`, `unknown_type` —
        // both `[": Invalid input"]` in v4) are the read path's own collapse
        // (`chats_messages_read::collapsed_errors`); an unknown `type` answers
        // `[]` HERE so `updateMessage`'s merged check keeps its pass set.
        assert!(zod_chat_event_issues(&msg(json!({"type": "bogus"}))).is_empty());
    }

    /// The echoed JS pattern and the Rust matcher are ONE pattern: the matcher
    /// is the JS source with `\d` rewritten to `[0-9]` (Rust's `\d` is
    /// Unicode-aware), and the echo wraps it in the literal's slashes.
    #[test]
    fn datetime_js_pattern_is_the_matcher_in_js_form() {
        let js = ZOD_ISO_DATETIME_JS_PATTERN;
        let inner = js
            .strip_prefix('/')
            .and_then(|s| s.strip_suffix('/'))
            .expect("slashes");
        assert_eq!(inner.replace(r"\d", "[0-9]"), ZOD_ISO_DATETIME_PATTERN);
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
