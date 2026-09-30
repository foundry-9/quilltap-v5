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
    match row.get("name") {
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
    zod_uuid_issues(row, "officialMountPointId", true, &mut issues);
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

/// v4 `RouteAttemptSchema` (`lib/schemas/chat.types.ts`) — the STRICT twin of
/// one `routeTrail` element as v4's per-row `ChatEventSchema.safeParse` meets
/// it (`MessageEventSchema.routeTrail` is `RouteAttemptSchema.array()
/// .nullable().optional()`). `None` when the element passes; otherwise the
/// first failing key and zod's sentence for it (the stand-in for the issue
/// list the per-row WARN reports — P4.130, P4.D228 re-premised).
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
/// refused. v5's lenient reader `RouteAttempt::from_value`
/// (`services/route_trail.rs`) had read `null` as absent and checked neither
/// the uuid nor the length, so a row v4 skips was KEPT (and re-saved by the
/// "Try uncensored" picture route).
pub fn zod_route_attempt_failure(v: &Value) -> Option<String> {
    let Some(o) = v.as_object() else {
        return Some(format!(
            "Invalid input: expected object, received {}",
            zod_parsed_type(Some(v))
        ));
    };
    let fail = |k: &str, why: String| Some(format!("{k}: {why}"));
    match o.get("profileId") {
        Some(Value::String(id)) if zod_uuid_ok(id) => {}
        Some(Value::String(_)) => return fail("profileId", "Invalid UUID".into()),
        got => {
            return fail(
                "profileId",
                format!(
                    "Invalid input: expected string, received {}",
                    zod_parsed_type(got)
                ),
            )
        }
    }
    for k in ["profileName", "provider", "modelName"] {
        if !o.get(k).is_some_and(Value::is_string) {
            return fail(
                k,
                format!(
                    "Invalid input: expected string, received {}",
                    zod_parsed_type(o.get(k))
                ),
            );
        }
    }
    let member = |k: &str, ok: &dyn Fn(&str) -> bool, required: bool| -> Option<String> {
        match o.get(k) {
            None if !required => None,
            Some(Value::String(s)) if ok(s) => None,
            _ => fail(k, "Invalid option".into()),
        }
    };
    const VIA: [&str; 5] = ["primary", "retry", "concierge", "understudy", "tier-pick"];
    const OUTCOME: [&str; 3] = ["answered", "failed", "refused"];
    member("via", &|s| VIA.contains(&s), true)
        .or_else(|| member("outcome", &|s| OUTCOME.contains(&s), true))
        .or_else(|| {
            member(
                "trigger",
                &|s| crate::llm_fallback::FallbackTrigger::from_wire(s).is_some(),
                false,
            )
        })
        .or_else(|| {
            member(
                "evidence",
                &|s| {
                    crate::services::dangerous_content::refusal::RefusalEvidence::from_wire(s)
                        .is_some()
                },
                false,
            )
        })
        .or_else(|| {
            member(
                "profileKind",
                &|s| matches!(s, "connection" | "image"),
                false,
            )
        })
        .or_else(|| match o.get("detail") {
            None => None,
            Some(Value::String(d)) if crate::jsstr::zod_len_max_ok(d, 200) => None,
            Some(Value::String(_)) => fail(
                "detail",
                "Too big: expected string to have <=200 characters".into(),
            ),
            got => fail(
                "detail",
                format!(
                    "Invalid input: expected string, received {}",
                    zod_parsed_type(got)
                ),
            ),
        })
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
