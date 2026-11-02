//! The roleplay-templates repository — a Phase-2 repo port, after `folders`,
//! `tags`, `text_replacement_rules`, `prompt_templates`, and
//! `conversation_annotations`. Ports v4's
//! `lib/database/repositories/roleplay-templates.repository.ts` (+ the
//! `_create`/`_update`/`_delete` internals of `base.repository.ts`).
//!
//! Scope: `create`, `update`, and `delete`. The tag-mutation helpers
//! (`addTag`/`removeTag`) are out of scope; the built-in *seeding*
//! (`seedBuiltInTemplates`) lives in [`crate::services::builtin_templates`] and
//! writes through the repo-internal [`RoleplayTemplatesRepository::seed_insert`] /
//! [`RoleplayTemplatesRepository::seed_update`] paths (mirroring v4's
//! collection-level seed, which bypasses the `isBuiltIn` read-only guard on the
//! public `update`). Like `prompt_templates`, `roleplay_templates` uses the plain
//! `AbstractBaseRepository` because `userId` is nullable (built-in templates have
//! `userId = null`). **No built-in read-only guard is ported here:** v4's
//! `roleplay-templates` `update`/`delete` DO carry an `isBuiltIn` guard, but the
//! task scopes this port to the base `_create`/`_update`/`_delete` delegation
//! shape used by the corpus — the corpus never targets a built-in row for a
//! mutation, so the guard is not exercised and is left to a later widening.
//!
//! ## What this repo banks for the tier-2 marshaling surface
//!
//! It is the **first repo with an array-of-objects JSON column**
//! (`renderingPatterns`), plus a **nullable JSON-object column**
//! (`dialogueDetection`):
//!
//!   - **`renderingPatterns: z.array(RenderingPatternSchema)`** → a JSON array of
//!     objects, stored compact via v4's `prepareForStorage` `JSON.stringify`. Each
//!     element's key order is the schema's field order (Zod `.parse()` rebuilds
//!     the object in shape order), so the elements are modeled with a typed
//!     [`RenderingPattern`] struct whose fields are declared in
//!     `RenderingPatternSchema` order — `pattern`, `className`, then the three
//!     optionals `flags` / `scope` / `hideDelimiters`. The optionals carry
//!     `skip_serializing_if = "Option::is_none"`, matching Zod's omission of an
//!     absent `.optional()` field (and `JSON.stringify`'s skip of `undefined`).
//!     A typed struct, not a `serde_json::Value`, so the field order is declared
//!     at the definition site — the same typed-struct rule `tags.visualStyle`
//!     established, now over an array of objects.
//!   - **`dialogueDetection: DialogueDetectionSchema.nullable().optional()`** → a
//!     nullable JSON object. `None` → SQL NULL; `Some` → compact JSON via
//!     [`DialogueDetection`], fields in schema order (`openingChars`,
//!     `closingChars`, `className`).
//!   - **`tags: z.array(UUIDSchema)`** → a JSON array of UUID strings, exactly as
//!     `prompt_templates` (order-preserving, no key-order subtlety).
//!   - several **nullable string columns** (`userId` null-for-built-in,
//!     `description`) and the `isBuiltIn` boolean → INTEGER 0/1.
//!
//! Two more columns carry unions (P4.4u3 completed the marshaling the Phase-2
//! port deferred):
//!
//!   - **`delimiters: z.array(TemplateDelimiterSchema)`** → a JSON array of the
//!     discriminated union ([`TemplateDelimiter`]), discriminated on `kind`
//!     (`wrap` / `linePrefix` / `tagPrefix`). Each variant is a typed serde struct
//!     in **schema field order** (`kind`, the common fields `name` / `buttonName`
//!     / `style` / `hideDelimiter?` / `addOns?`, then the kind-specific tail), so
//!     `to_string` reproduces v4's `JSON.stringify(zodParsed)` byte-for-byte. The
//!     `wrap` tail (`delimiters`) and `narrationDelimiters` share the
//!     [`StringOrPair`] union (a bare string OR an `[open, close]` pair);
//!     `TemplateDelimiterSchema`'s `z.preprocess` read-side `kind:'wrap'` backfill
//!     for legacy kind-less rows is ported in [`parse_delimiters`]. The optional
//!     `hideDelimiter` / `addOns` / `tokenPattern` keys carry
//!     `skip_serializing_if`, matching Zod's omission of an absent `.optional()`.
//!   - **`narrationDelimiters: NarrationDelimitersSchema.nullable().default('*')`**
//!     → the [`StringOrPair`] union again, now NULLABLE (`null` = unmarked prose,
//!     P4.D341 / v4 `778b97b2b`). The union is `'unknown'` to the schema
//!     classifier, so the generic SQLite layer used to store a string as plain
//!     TEXT and a pair as JSON text and hand the pair BACK as that string (Quilltap
//!     RP's `["[","]"]` reached the composer as one eight-character delimiter),
//!     and a `null` came back as `undefined` → the default `'*'`. v4's repository
//!     therefore writes the field as JSON text itself and decodes it on every read
//!     (`roleplay-template-narration-codec.ts`): [`encode_narration_delimiters`] /
//!     [`decode_narration_delimiters`] port that codec exactly. Decoding accepts
//!     legacy rows too: a raw delimiter string (`*`, `+`) that is not JSON is
//!     returned as-is.
//!   - **`unmarkedStyle: UnmarkedStyleSchema.nullable().optional()`** → a nullable
//!     JSON object ([`UnmarkedStyle`]); `None` → SQL NULL (v4's `toJson(null)`).
//!   - **`markdownAsterisks: z.boolean().default(true)`** → INTEGER 0/1; a NULL
//!     cell reads as `true` (hydrate → `undefined` → the Zod default).
//!
//! Determinism: the tier-2 case pins the id and timestamps, so the persisted rows
//! match v4's byte-for-byte with no normalization — the pinned form
//! `folders`/`tags`/`text_replacement_rules`/`prompt_templates`/
//! `conversation_annotations` use.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use super::DbError;

/// One rendering pattern — an element of the `renderingPatterns` JSON array.
/// Field order here is `RenderingPatternSchema`'s definition order in v4's
/// `template.types.ts`; `serde_json` serializes struct fields in declaration
/// order, so `to_string` reproduces v4's `JSON.stringify(zodParsed)` exactly.
/// The three optionals are `skip_serializing_if = "Option::is_none"` so an absent
/// `.optional()` field is omitted from the stored object (matching Zod +
/// `JSON.stringify`), letting the corpus exercise varied optional-field presence.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderingPattern {
    pub pattern: String,
    pub class_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hide_delimiters: Option<bool>,
}

/// The `dialogueDetection` JSON object (nullable column). Field order is
/// `DialogueDetectionSchema`'s definition order (`openingChars`, `closingChars`,
/// `className`); serialized compact, reproducing v4's `JSON.stringify`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DialogueDetection {
    pub opening_chars: Vec<String>,
    pub closing_chars: Vec<String>,
    pub class_name: String,
}

/// v4 `none` default for the `underline` / `border` add-on enums.
fn addon_none() -> String {
    "none".to_string()
}

/// The `addOns` object (`DelimiterAddOnsSchema`): layered decorations on top of a
/// delimiter's base `style`. Field order is the schema's; each field carries the
/// Zod `.default(...)` so a partially-specified add-ons object deserializes with
/// the same materialized fields v4's `.parse()` produces (the enums default to
/// `'none'`/`''`, the flags to `false`). Serialized compact in field order.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DelimiterAddOns {
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub reverse: bool,
    #[serde(default = "addon_none")]
    pub underline: String,
    #[serde(default = "addon_none")]
    pub border: String,
    #[serde(default)]
    pub font: String,
}

/// The `z.union([z.string(), z.tuple([string, string])])` shared by a `wrap`
/// delimiter's `delimiters` field and the row-level `narrationDelimiters` column.
/// Untagged: a bare JSON string deserializes to [`Self::Single`]; a two-element
/// array to [`Self::Pair`]. Serialized as a string or a 2-array respectively.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StringOrPair {
    /// One string used for both open and close (e.g. `"*"`).
    Single(String),
    /// An `[open, close]` pair (e.g. `["[", "]"]`).
    Pair([String; 2]),
}

/// How a template displays text that no delimiter claims (v4
/// `UnmarkedStyleSchema`): a base `style` class plus optional add-ons. A template
/// whose narration is plain prose (`narrationDelimiters: null`) uses this to
/// style it; any template may set it. Field order is the schema's.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnmarkedStyle {
    pub style: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub add_ons: Option<DelimiterAddOns>,
}

/// v4 `encodeNarrationDelimiters(value) = JSON.stringify(value)` — the JSON text
/// the `narrationDelimiters` column stores: `'*'` → the three bytes `"*"`, a pair
/// → `["[","]"]`, `null` (unmarked prose) → the four-byte TEXT `null` (never SQL
/// NULL).
pub fn encode_narration_delimiters(v: Option<&StringOrPair>) -> String {
    match v {
        // `serde_json::to_string` of a `String` / `[String; 2]` cannot fail.
        Some(StringOrPair::Single(s)) => serde_json::to_string(s).unwrap_or_default(),
        Some(StringOrPair::Pair(pair)) => serde_json::to_string(pair).unwrap_or_default(),
        None => "null".to_string(),
    }
}

/// v4 `isPair`: an array of exactly two NON-EMPTY strings.
fn json_pair(v: &serde_json::Value) -> Option<[String; 2]> {
    let serde_json::Value::Array(items) = v else {
        return None;
    };
    match items.as_slice() {
        [serde_json::Value::String(a), serde_json::Value::String(b)]
            if !a.is_empty() && !b.is_empty() =>
        {
            Some([a.clone(), b.clone()])
        }
        _ => None,
    }
}

/// v4 `decodeNarrationDelimiters(stored)` over the column cell as the read path
/// hands it over. v4's hydrate turns a SQL NULL into `undefined`, which the
/// schema's `.default('*')` makes `'*'` before the decoder sees it — so a NULL
/// cell on a legacy row reads as the historical default `'*'` (the codec's own
/// docblock). Otherwise: an empty string → `'*'`; JSON `null` → `None`
/// (unmarked prose); a JSON non-empty string or two-string pair → it; a parse
/// failure or any other parsed value → the RAW stored string (a legacy raw
/// delimiter like `*` or `+`).
pub fn decode_narration_delimiters(stored: Option<&str>) -> Option<StringOrPair> {
    let Some(stored) = stored else {
        return Some(StringOrPair::Single("*".to_string()));
    };
    if stored.is_empty() {
        return Some(StringOrPair::Single("*".to_string()));
    }
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(stored) {
        match &parsed {
            serde_json::Value::Null => return None,
            serde_json::Value::String(s) if !s.is_empty() => {
                return Some(StringOrPair::Single(s.clone()))
            }
            other => {
                if let Some(pair) = json_pair(other) {
                    return Some(StringOrPair::Pair(pair));
                }
            }
        }
    }
    // Not JSON (or JSON of another shape) — a legacy raw delimiter string.
    Some(StringOrPair::Single(stored.to_string()))
}

/// The decoded narration as the read DTO carries it: a string, a two-string
/// array, or `null` (unmarked prose).
fn narration_value(v: Option<&StringOrPair>) -> serde_json::Value {
    match v {
        Some(StringOrPair::Single(s)) => serde_json::Value::String(s.clone()),
        Some(StringOrPair::Pair([a, b])) => serde_json::Value::Array(vec![
            serde_json::Value::String(a.clone()),
            serde_json::Value::String(b.clone()),
        ]),
        None => serde_json::Value::Null,
    }
}

/// One `delimiters` entry — v4's `TemplateDelimiterUnionSchema`, a discriminated
/// union over `kind`. Modeled as a serde internally-tagged enum (`kind` is
/// serialized first), each variant's fields in **schema field order**: the shared
/// `name` / `buttonName` / `style` / `hideDelimiter?` / `addOns?` (spread from
/// `delimiterCommonFields`, hence before the kind-specific tail), then the tail.
/// The read-side `kind:'wrap'` backfill for legacy kind-less rows lives in
/// [`parse_delimiters`] (v4's `z.preprocess`), not here — an internally-tagged
/// enum requires the discriminant to be present.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum TemplateDelimiter {
    /// `open…close` around an inline span. `delimiters` is a [`StringOrPair`].
    #[serde(rename = "wrap")]
    Wrap {
        name: String,
        #[serde(rename = "buttonName")]
        button_name: String,
        style: String,
        #[serde(
            rename = "hideDelimiter",
            skip_serializing_if = "Option::is_none",
            default
        )]
        hide_delimiter: Option<bool>,
        #[serde(rename = "addOns", skip_serializing_if = "Option::is_none", default)]
        add_ons: Option<DelimiterAddOns>,
        delimiters: StringOrPair,
        /// v4 `matchCurlyQuotes`: a straight quote in the delimiters also matches
        /// its curly forms, in any mix. Absent ⇒ the delimiters match literally.
        /// (Without this field serde DROPS the key on re-emission — Novel's
        /// Dialogue delimiter carries it.)
        #[serde(
            rename = "matchCurlyQuotes",
            skip_serializing_if = "Option::is_none",
            default
        )]
        match_curly_quotes: Option<bool>,
        /// v4 `closeOptional`: a span that opens but never closes runs to the end
        /// of its paragraph (speech carried over several paragraphs). Absent ⇒
        /// an unclosed span is not styled.
        #[serde(
            rename = "closeOptional",
            skip_serializing_if = "Option::is_none",
            default
        )]
        close_optional: Option<bool>,
    },
    /// A `marker` at the start of a line styles the whole line (e.g. `// `).
    #[serde(rename = "linePrefix")]
    LinePrefix {
        name: String,
        #[serde(rename = "buttonName")]
        button_name: String,
        style: String,
        #[serde(
            rename = "hideDelimiter",
            skip_serializing_if = "Option::is_none",
            default
        )]
        hide_delimiter: Option<bool>,
        #[serde(rename = "addOns", skip_serializing_if = "Option::is_none", default)]
        add_ons: Option<DelimiterAddOns>,
        marker: String,
    },
    /// A bracketed token (`open`/`close`) whose inner text matches `tokenPattern`
    /// styles the whole line (e.g. `[CAPTAIN] …`).
    #[serde(rename = "tagPrefix")]
    TagPrefix {
        name: String,
        #[serde(rename = "buttonName")]
        button_name: String,
        style: String,
        #[serde(
            rename = "hideDelimiter",
            skip_serializing_if = "Option::is_none",
            default
        )]
        hide_delimiter: Option<bool>,
        #[serde(rename = "addOns", skip_serializing_if = "Option::is_none", default)]
        add_ons: Option<DelimiterAddOns>,
        open: String,
        close: String,
        #[serde(
            rename = "tokenPattern",
            skip_serializing_if = "Option::is_none",
            default
        )]
        token_pattern: Option<String>,
    },
}

/// Serialize the typed delimiters to the compact JSON array text v4's
/// `prepareForStorage` (`JSON.stringify` of the Zod-parsed array) produces.
fn serialize_delimiters(delimiters: &[TemplateDelimiter]) -> Result<String, DbError> {
    serde_json::to_string(delimiters)
        .map_err(|e| DbError::Internal(format!("delimiters serialize: {e}")))
}

/// Parse the stored `delimiters` JSON array text into typed delimiters, applying
/// v4's read-side `kind:'wrap'` backfill (`TemplateDelimiterSchema`'s
/// `z.preprocess`): an object entry lacking `kind` is read as a `wrap` delimiter.
/// Used by [`RoleplayTemplatesRepository::update`] to reproduce v4's full-row
/// rewrite of a legacy row (v4 `_update` reads → validates → writes back every
/// column, so a kind-less delimiter on disk is upgraded to `kind:'wrap'` on the
/// next update).
fn parse_delimiters(json_text: &str) -> Result<Vec<TemplateDelimiter>, DbError> {
    let raw: Vec<serde_json::Value> = serde_json::from_str(json_text)
        .map_err(|e| DbError::Internal(format!("delimiters parse: {e}")))?;
    let mut out = Vec::with_capacity(raw.len());
    for mut value in raw {
        if let serde_json::Value::Object(map) = &mut value {
            map.entry("kind")
                .or_insert_with(|| serde_json::Value::String("wrap".to_string()));
        }
        let delim: TemplateDelimiter = serde_json::from_value(value)
            .map_err(|e| DbError::Internal(format!("delimiters element: {e}")))?;
        out.push(delim);
    }
    Ok(out)
}

/// Serialize an optional [`UnmarkedStyle`] to its stored text (`None` → SQL NULL,
/// v4's `toJson(null)`).
fn serialize_unmarked_style(v: Option<&UnmarkedStyle>) -> Result<Option<String>, DbError> {
    v.map(|u| {
        serde_json::to_string(u)
            .map_err(|e| DbError::Internal(format!("unmarkedStyle serialize: {e}")))
    })
    .transpose()
}

/// Fields for creating a roleplay template (the `Omit<RoleplayTemplate,'id'|
/// timestamps>` shape). `tags`/`delimiters`/`renderingPatterns` are the JSON array
/// columns; `dialogue_detection` is the nullable JSON-object column; `user_id` /
/// `description` are the nullable string columns; `narration_delimiters` is the
/// string-or-pair union (see the module header).
pub struct RtCreate {
    /// `None` => SQL NULL (the null-for-built-in `userId`).
    pub user_id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub system_prompt: String,
    pub is_built_in: bool,
    /// Stored as compact JSON text (`["id1","id2"]`, `[]` when empty).
    pub tags: Vec<String>,
    /// The discriminated-union delimiter entries → compact JSON array text.
    pub delimiters: Vec<TemplateDelimiter>,
    /// Array of pattern objects → compact JSON array of objects.
    pub rendering_patterns: Vec<RenderingPattern>,
    /// `None` => SQL NULL; `Some` => compact JSON object.
    pub dialogue_detection: Option<DialogueDetection>,
    /// The narration delimiter — a bare string or an `[open, close]` pair;
    /// `None` = unmarked prose, stored as the TEXT `null`. Always written
    /// ENCODED ([`encode_narration_delimiters`]).
    pub narration_delimiters: Option<StringOrPair>,
    /// `None` => SQL NULL; `Some` => compact JSON object.
    pub unmarked_style: Option<UnmarkedStyle>,
    /// v4 `markdownAsterisks` (Zod default `true`) → INTEGER 0/1.
    pub markdown_asterisks: bool,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A roleplay-template update patch. Mirrors v4 `update` over `_update`: provided
/// fields overwrite, id and createdAt are preserved, `updatedAt` is set
/// explicitly. Each `Some` field sets that column (JSON columns re-serialized).
#[derive(Default)]
pub struct RtUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub system_prompt: Option<String>,
    pub tags: Option<Vec<String>>,
    pub delimiters: Option<Vec<TemplateDelimiter>>,
    pub rendering_patterns: Option<Vec<RenderingPattern>>,
    pub dialogue_detection: Option<DialogueDetection>,
    /// `None` = not named (the stored value is re-encoded); `Some(None)` = set
    /// `null` (unmarked prose); `Some(Some(v))` = set `v`.
    pub narration_delimiters: Option<Option<StringOrPair>>,
    /// `None` = not named; `Some(None)` = set SQL NULL; `Some(Some(v))` = set `v`.
    pub unmarked_style: Option<Option<UnmarkedStyle>>,
    /// `None` = not named (the stored value, NULL → `true`, is rewritten).
    pub markdown_asterisks: Option<bool>,
    pub updated_at: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct RoleplayTemplatesRepository<'c> {
    conn: &'c Connection,
}

impl<'c> RoleplayTemplatesRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert a roleplay template with the given pinned id + timestamps. All 15
    /// columns are written (`narrationDelimiters` ENCODED — v4's
    /// `toPersistedRow`); JSON columns via `serde_json::to_string`, the bool via
    /// `i64::from`, nullable columns via `Option`. `delimiters` is the JSON text of
    /// an empty array (held empty across the corpus).
    pub fn create(&self, data: &RtCreate, opts: &CreateOptions) -> Result<(), DbError> {
        let tags_json = serde_json::to_string(&data.tags)
            .map_err(|e| DbError::Internal(format!("tags serialize: {e}")))?;
        let rendering_patterns_json = serde_json::to_string(&data.rendering_patterns)
            .map_err(|e| DbError::Internal(format!("renderingPatterns serialize: {e}")))?;
        let delimiters_json = serialize_delimiters(&data.delimiters)?;
        let narration_delimiters = encode_narration_delimiters(data.narration_delimiters.as_ref());
        let unmarked_style_json = serialize_unmarked_style(data.unmarked_style.as_ref())?;
        let dialogue_detection_json: Option<String> = match &data.dialogue_detection {
            Some(dd) => Some(
                serde_json::to_string(dd)
                    .map_err(|e| DbError::Internal(format!("dialogueDetection serialize: {e}")))?,
            ),
            None => None,
        };

        self.conn.execute(
            "INSERT INTO roleplay_templates \
               (id, userId, name, description, systemPrompt, isBuiltIn, tags, delimiters, \
                renderingPatterns, dialogueDetection, narrationDelimiters, unmarkedStyle, \
                markdownAsterisks, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                opts.id,
                data.user_id,
                data.name,
                data.description,
                data.system_prompt,
                i64::from(data.is_built_in),
                tags_json,
                delimiters_json,
                rendering_patterns_json,
                dialogue_detection_json,
                narration_delimiters,
                unmarked_style_json,
                i64::from(data.markdown_asterisks),
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Apply an update patch to the template `id`. Returns `Ok(false)` when no row
    /// matched (v4's "not found -> null"). id and createdAt are never touched;
    /// `updatedAt` is always set. JSON columns are re-serialized when provided.
    ///
    /// v4's `_update` reads the existing row, merges the patch, re-validates the
    /// WHOLE object, and `$set`s every column — so a column absent from the patch
    /// is still rewritten from its re-parsed self. That is a byte-level no-op for
    /// every column EXCEPT:
    ///   - `delimiters`, whose read-side `kind:'wrap'` backfill
    ///     (`parse_delimiters`) upgrades a legacy kind-less entry;
    ///   - `narrationDelimiters` — v4: "_update writes the whole merged row, so
    ///     the narration value always travels encoded — the patch's if it names
    ///     one, else the stored one" (the stored one DECODED then re-encoded, so a
    ///     legacy raw `+` lands as `"+"` and a NULL cell as `"*"`);
    ///   - `markdownAsterisks` — a NULL cell reads as the Zod default `true` and
    ///     is written back as `1`;
    ///   - `unmarkedStyle` — a stored object is re-emitted in Zod schema order with
    ///     its add-on defaults materialized (a NULL / `null` cell hydrates to
    ///     `undefined` and is left out of the `$set`).
    ///
    /// So this port always rewrites those four (from the patch, or from the
    /// re-parsed existing value) and leaves the other columns partial —
    /// byte-identical to v4.
    pub fn update(&self, id: &str, patch: &RtUpdate) -> Result<bool, DbError> {
        // v4 `_update` first `findById`s — the row must exist or it's a no-op.
        // Fetch the always-rewritten columns in the same read.
        type Existing = (String, Option<String>, Option<String>, Option<i64>);
        let existing: Option<Existing> = self
            .conn
            .query_row(
                "SELECT delimiters, narrationDelimiters, unmarkedStyle, markdownAsterisks \
                 FROM roleplay_templates WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        let Some((
            existing_delimiters,
            existing_narration,
            existing_unmarked,
            existing_markdown_asterisks,
        )) = existing
        else {
            return Ok(false);
        };

        let mut assignments: Vec<String> = Vec::new();
        let mut values: Vec<Box<dyn ToSql>> = Vec::new();

        // `delimiters` is always rewritten (v4's full-row `$set`), applying the
        // read-side backfill when the patch does not supply a replacement.
        let delimiters_json = match &patch.delimiters {
            Some(delimiters) => serialize_delimiters(delimiters)?,
            None => serialize_delimiters(&parse_delimiters(&existing_delimiters)?)?,
        };
        assignments.push(format!("delimiters = ?{}", values.len() + 1));
        values.push(Box::new(delimiters_json));

        if let Some(name) = &patch.name {
            assignments.push(format!("name = ?{}", values.len() + 1));
            values.push(Box::new(name.clone()));
        }
        if let Some(description) = &patch.description {
            assignments.push(format!("description = ?{}", values.len() + 1));
            values.push(Box::new(description.clone()));
        }
        if let Some(system_prompt) = &patch.system_prompt {
            assignments.push(format!("systemPrompt = ?{}", values.len() + 1));
            values.push(Box::new(system_prompt.clone()));
        }
        if let Some(tags) = &patch.tags {
            let tags_json = serde_json::to_string(tags)
                .map_err(|e| DbError::Internal(format!("tags serialize: {e}")))?;
            assignments.push(format!("tags = ?{}", values.len() + 1));
            values.push(Box::new(tags_json));
        }
        if let Some(rendering_patterns) = &patch.rendering_patterns {
            let json = serde_json::to_string(rendering_patterns)
                .map_err(|e| DbError::Internal(format!("renderingPatterns serialize: {e}")))?;
            assignments.push(format!("renderingPatterns = ?{}", values.len() + 1));
            values.push(Box::new(json));
        }
        if let Some(dialogue_detection) = &patch.dialogue_detection {
            let json = serde_json::to_string(dialogue_detection)
                .map_err(|e| DbError::Internal(format!("dialogueDetection serialize: {e}")))?;
            assignments.push(format!("dialogueDetection = ?{}", values.len() + 1));
            values.push(Box::new(json));
        }
        // narrationDelimiters: ALWAYS written encoded (the patch's, else the
        // stored one through decode → encode).
        let narration = match &patch.narration_delimiters {
            Some(named) => encode_narration_delimiters(named.as_ref()),
            None => encode_narration_delimiters(
                decode_narration_delimiters(existing_narration.as_deref()).as_ref(),
            ),
        };
        assignments.push(format!("narrationDelimiters = ?{}", values.len() + 1));
        values.push(Box::new(narration));
        // unmarkedStyle: the patch's (`Some(None)` → SQL NULL), else the stored
        // object re-emitted through the schema; an absent (NULL / `null`) cell is
        // left out of the `$set`, as v4's hydrate → `undefined` leaves it.
        let unmarked: Option<Option<String>> = match &patch.unmarked_style {
            Some(named) => Some(serialize_unmarked_style(named.as_ref())?),
            None => match existing_unmarked
                .as_deref()
                .map(serde_json::from_str::<serde_json::Value>)
            {
                Some(Ok(v)) if !v.is_null() => {
                    let typed: UnmarkedStyle = serde_json::from_value(v)
                        .map_err(|e| DbError::Internal(format!("unmarkedStyle parse: {e}")))?;
                    Some(serialize_unmarked_style(Some(&typed))?)
                }
                _ => None,
            },
        };
        if let Some(unmarked) = unmarked {
            assignments.push(format!("unmarkedStyle = ?{}", values.len() + 1));
            values.push(Box::new(unmarked));
        }
        // markdownAsterisks: the patch's, else the stored flag (NULL → `true`).
        let markdown_asterisks = patch
            .markdown_asterisks
            .unwrap_or_else(|| existing_markdown_asterisks.is_none_or(|v| v == 1));
        assignments.push(format!("markdownAsterisks = ?{}", values.len() + 1));
        values.push(Box::new(i64::from(markdown_asterisks)));
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE roleplay_templates SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Delete the template `id`. Returns `false` when no row matched (v4's
    /// `_delete` "deletedCount === 0 -> false").
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM roleplay_templates WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// v4 seeder `collection.findOne({name, isBuiltIn: true})` → the matching
    /// built-in template's id, for the seed insert-or-update decision. `None` when
    /// no built-in template by that name exists (so the seeder inserts a fresh
    /// one). Keys on `isBuiltIn = 1`, so a same-name USER template is invisible —
    /// the built-in is minted alongside it (the `isBuiltIn`-keying invariant).
    pub fn find_built_in_id_by_name(&self, name: &str) -> Result<Option<String>, DbError> {
        self.conn
            .query_row(
                "SELECT id FROM roleplay_templates WHERE name = ?1 AND isBuiltIn = 1 LIMIT 1",
                params![name],
                |row| row.get::<_, String>(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
            .map_err(DbError::from)
    }

    /// v4 seeder UPDATE path: `collection.updateOne({id}, {$set: this.encoded({
    /// systemPrompt, description, delimiters, renderingPatterns,
    /// dialogueDetection, narrationDelimiters, unmarkedStyle, markdownAsterisks,
    /// updatedAt})})` — run on EVERY boot, so it rewrites a legacy raw `*`
    /// narration cell to the encoded `"*"` and moves `updatedAt` each time
    /// (measured at v4 `236c04e35`). Unlike the public [`Self::update`], this
    /// is a RAW column write that does NOT re-validate through the schema — so
    /// `delimiters` (and the other JSON columns) are stored in the exact bytes the
    /// caller supplies (the seed-literal key order, NOT the Zod schema order the
    /// INSERT path produces). The caller pre-serializes every JSON column. Mirrors
    /// v4's collection-level seed, which bypasses the public op's `isBuiltIn`
    /// read-only guard.
    pub fn seed_update(&self, id: &str, patch: &SeedUpdate) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE roleplay_templates SET \
               systemPrompt = ?1, description = ?2, delimiters = ?3, renderingPatterns = ?4, \
               dialogueDetection = ?5, narrationDelimiters = ?6, unmarkedStyle = ?7, \
               markdownAsterisks = ?8, updatedAt = ?9 \
             WHERE id = ?10",
            params![
                patch.system_prompt,
                patch.description,
                patch.delimiters_json,
                patch.rendering_patterns_json,
                patch.dialogue_detection_json,
                patch.narration_delimiters,
                patch.unmarked_style_json,
                i64::from(patch.markdown_asterisks),
                patch.updated_at,
                id,
            ],
        )?;
        Ok(())
    }
}

/// The pre-serialized column set for [`RoleplayTemplatesRepository::seed_update`]
/// (v4's seeder-update `$set`). JSON columns arrive as their stored text so the
/// seeder can supply the SEED-LITERAL key order the UPDATE path stores (distinct
/// from the schema order the INSERT path produces).
pub struct SeedUpdate {
    pub system_prompt: String,
    pub description: String,
    /// `delimiters` stored text — the raw seed-literal-order JSON array.
    pub delimiters_json: String,
    /// `renderingPatterns` stored text.
    pub rendering_patterns_json: String,
    /// `dialogueDetection` stored text, or `None` → SQL NULL.
    pub dialogue_detection_json: Option<String>,
    /// `narrationDelimiters` stored text — ENCODED
    /// ([`encode_narration_delimiters`]).
    pub narration_delimiters: String,
    /// `unmarkedStyle` stored text (seed-literal order), or `None` → SQL NULL.
    pub unmarked_style_json: Option<String>,
    pub markdown_asterisks: bool,
    pub updated_at: String,
}

/// The shared column list every roleplay-template full read selects (schema order —
/// `unmarkedStyle` / `markdownAsterisks` before `createdAt`, as the re-dumped
/// `fresh_schema.json` renders them).
const RT_COLUMNS: &str = "id, userId, name, description, systemPrompt, isBuiltIn, tags, \
     delimiters, renderingPatterns, dialogueDetection, narrationDelimiters, unmarkedStyle, \
     markdownAsterisks, createdAt, updatedAt";

/// Marshal one `roleplay_templates` row into the route-read [`serde_json::Value`],
/// reproducing v4's SQLite-backend `hydrateRow` + `RoleplayTemplateSchema.parse`:
/// null nullable columns (`userId` / `description` / `dialogueDetection`) become
/// `undefined` → **omitted**; `isBuiltIn` → bool; the JSON columns
/// (`tags`/`delimiters`/`renderingPatterns`/`dialogueDetection`) are `JSON.parse`d;
/// **`narrationDelimiters` is DECODED** through v4's codec (the repository's
/// `decoded()` on every read): a string, a two-string array, or `null` (unmarked
/// prose); `unmarkedStyle` is OMITTED on a NULL / `null` cell (hydrate →
/// `undefined`) and otherwise the parsed object re-emitted through the schema;
/// `markdownAsterisks` is a bool (a NULL cell → the Zod default `true`).
fn marshal_rt_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<serde_json::Value> {
    use serde_json::{Map, Value};
    let mut obj = Map::new();
    obj.insert("id".into(), Value::String(r.get::<_, String>(0)?));
    if let Some(s) = r.get::<_, Option<String>>(1)? {
        obj.insert("userId".into(), Value::String(s));
    }
    obj.insert("name".into(), Value::String(r.get::<_, String>(2)?));
    if let Some(s) = r.get::<_, Option<String>>(3)? {
        obj.insert("description".into(), Value::String(s));
    }
    obj.insert("systemPrompt".into(), Value::String(r.get::<_, String>(4)?));
    obj.insert("isBuiltIn".into(), Value::Bool(r.get::<_, i64>(5)? == 1));
    obj.insert(
        "tags".into(),
        parse_json_array_col(r.get::<_, Option<String>>(6)?),
    );
    obj.insert(
        "delimiters".into(),
        parse_json_array_col(r.get::<_, Option<String>>(7)?),
    );
    obj.insert(
        "renderingPatterns".into(),
        parse_json_array_col(r.get::<_, Option<String>>(8)?),
    );
    // dialogueDetection: nullable JSON object → omit when SQL NULL.
    if let Some(raw) = r.get::<_, Option<String>>(9)? {
        if !raw.is_empty() {
            if let Ok(v) = serde_json::from_str::<Value>(&raw) {
                obj.insert("dialogueDetection".into(), v);
            }
        }
    }
    // narrationDelimiters: v4's codec, decoded.
    obj.insert(
        "narrationDelimiters".into(),
        narration_value(
            decode_narration_delimiters(r.get::<_, Option<String>>(10)?.as_deref()).as_ref(),
        ),
    );
    // unmarkedStyle: nullable JSON object → omitted when NULL / `null` / not JSON.
    if let Some(raw) = r.get::<_, Option<String>>(11)? {
        if let Ok(v) = serde_json::from_str::<Value>(&raw) {
            if !v.is_null() {
                let v = match serde_json::from_value::<UnmarkedStyle>(v.clone()) {
                    Ok(typed) => serde_json::to_value(typed).unwrap_or(v),
                    Err(_) => v,
                };
                obj.insert("unmarkedStyle".into(), v);
            }
        }
    }
    obj.insert(
        "markdownAsterisks".into(),
        Value::Bool(r.get::<_, Option<i64>>(12)?.is_none_or(|v| v == 1)),
    );
    obj.insert("createdAt".into(), Value::String(r.get::<_, String>(13)?));
    obj.insert("updatedAt".into(), Value::String(r.get::<_, String>(14)?));
    Ok(Value::Object(obj))
}

/// Parse a stored JSON-array column text into a `Value`, defaulting to `[]` on
/// NULL / empty / parse failure (v4 hydrate → `undefined` → Zod `.default([])`).
fn parse_json_array_col(raw: Option<String>) -> serde_json::Value {
    match raw {
        Some(s) if !s.is_empty() => {
            serde_json::from_str(&s).unwrap_or_else(|_| serde_json::Value::Array(Vec::new()))
        }
        _ => serde_json::Value::Array(Vec::new()),
    }
}

/// v4 `repos.roleplayTemplates.findById(id)` (route read) — the full-JSON DTO, or
/// `None`.
pub fn find_full_json_by_id(
    conn: &Connection,
    id: &str,
) -> Result<Option<serde_json::Value>, DbError> {
    conn.query_row(
        &format!("SELECT {RT_COLUMNS} FROM roleplay_templates WHERE id = ?1"),
        params![id],
        marshal_rt_row,
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other.into()),
    })
}

/// v4 `repos.roleplayTemplates.findAllForUser(userId)` — `$or: [{isBuiltIn:true},
/// {userId}]`, UNSORTED (the route sorts built-in-first then localeCompare).
pub fn find_all_for_user(
    conn: &Connection,
    user_id: &str,
) -> Result<Vec<serde_json::Value>, DbError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {RT_COLUMNS} FROM roleplay_templates WHERE isBuiltIn = 1 OR userId = ?1"
    ))?;
    let rows = stmt.query_map(params![user_id], marshal_rt_row)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// v4 `repos.roleplayTemplates.findByName(userId, name)` — the user-scoped
/// duplicate-name probe. Returns the matching template's id (the routes only need
/// existence / self-comparison), or `None`.
pub fn find_id_by_user_and_name(
    conn: &Connection,
    user_id: &str,
    name: &str,
) -> Result<Option<String>, DbError> {
    conn.query_row(
        "SELECT id FROM roleplay_templates WHERE userId = ?1 AND name = ?2 LIMIT 1",
        params![user_id, name],
        |row| row.get::<_, String>(0),
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other),
    })
    .map_err(DbError::from)
}

/// Parse a `delimiters` JSON [`serde_json::Value`] (from a marshaled DTO) into the
/// typed union, applying the read-side `kind:'wrap'` backfill — for the routes'
/// on-the-fly rendering-pattern regeneration. A non-array or unparseable entry
/// yields an empty list (v4's `template.delimiters || []`).
pub fn delimiters_from_value(v: &serde_json::Value) -> Vec<TemplateDelimiter> {
    let serde_json::Value::Array(items) = v else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let mut item = item.clone();
        if let serde_json::Value::Object(map) = &mut item {
            map.entry("kind")
                .or_insert_with(|| serde_json::Value::String("wrap".to_string()));
        }
        if let Ok(d) = serde_json::from_value::<TemplateDelimiter>(item) {
            out.push(d);
        }
    }
    out
}

/// **Scoped read** for the roleplay-template fallback chain
/// ([`crate::services::participant_resolver`]): the `systemPrompt` of a template,
/// v4 `roleplayTemplates.findById(id)?.systemPrompt`. Follows the scoped-read
/// precedent `chat_settings::find_auto_housekeeping_settings_by_user_id`, retired by P4.D288
/// — the resolver consumes ONLY `systemPrompt` (it returns `{ systemPrompt }`),
/// so the full `RoleplayTemplate` read marshaling is out of scope here. `None`
/// when no row exists (v4 `findById` → `null` → `getRoleplayTemplate` returns
/// `null`). `systemPrompt` is a required non-null TEXT column
/// (`z.string().min(1)`), so a present row always yields the string.
pub fn find_system_prompt_by_id(conn: &Connection, id: &str) -> Result<Option<String>, DbError> {
    conn.query_row(
        "SELECT systemPrompt FROM roleplay_templates WHERE id = ?1",
        params![id],
        |row| row.get::<_, String>(0),
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other),
    })
    .map_err(DbError::from)
}

#[cfg(test)]
mod narration_codec_tests {
    //! The codec's arms in isolation; the differential proof (v4's REAL
    //! `decodeNarrationDelimiters` over every planted cell) is
    //! `roleplay_templates_tier2_equivalence`'s `reads`.
    use super::*;

    fn single(s: &str) -> Option<StringOrPair> {
        Some(StringOrPair::Single(s.to_string()))
    }

    #[test]
    fn encode_is_json_stringify() {
        assert_eq!(encode_narration_delimiters(single("*").as_ref()), "\"*\"");
        let pair = StringOrPair::Pair(["[".to_string(), "]".to_string()]);
        assert_eq!(encode_narration_delimiters(Some(&pair)), "[\"[\",\"]\"]");
        assert_eq!(encode_narration_delimiters(None), "null");
    }

    #[test]
    fn decode_arms() {
        let enc =
            |v: Option<&str>| encode_narration_delimiters(decode_narration_delimiters(v).as_ref());
        // SQL NULL → the historical default '*'.
        assert_eq!(enc(None), "\"*\"");
        assert_eq!(enc(Some("")), "\"*\"");
        assert_eq!(enc(Some("null")), "null");
        assert_eq!(enc(Some("\"*\"")), "\"*\"");
        assert_eq!(enc(Some("[\"[\",\"]\"]")), "[\"[\",\"]\"]");
        // Legacy raw delimiters (not JSON) come back as-is.
        assert_eq!(enc(Some("*")), "\"*\"");
        assert_eq!(enc(Some("+")), "\"+\"");
        // JSON of another shape → the RAW stored string.
        assert_eq!(enc(Some("\"\"")), "\"\\\"\\\"\"");
        assert_eq!(enc(Some("[\"\",\"]\"]")), "\"[\\\"\\\",\\\"]\\\"]\"");
        assert_eq!(enc(Some("5")), "\"5\"");
    }
}
