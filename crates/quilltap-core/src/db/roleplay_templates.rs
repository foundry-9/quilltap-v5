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
//!     `serde_json::Value` is deliberately NOT used (its `BTreeMap` would sort the
//!     keys and diverge from v4) — the same typed-struct rule `tags.visualStyle`
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
//!   - **`narrationDelimiters: z.union([string, tuple])`** → the [`StringOrPair`]
//!     union again. Per `schema-translator`'s `getBaseType`, a heterogeneous union
//!     is `'unknown'` — NOT registered as a JSON column — so a string is stored as
//!     plain TEXT (`*`) and a pair as JSON array text (`["[","]"]`, via v4's
//!     `documentToRow` `shouldStoreAsJson`-on-value branch). [`narration_storage`]
//!     reproduces both.
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

impl StringOrPair {
    /// The stored-column form of a `narrationDelimiters` value (see
    /// [`narration_storage`]): a bare string is plain TEXT, a pair is JSON array
    /// text. Exposed for the built-in seeder's raw update path.
    pub fn storage(&self) -> Result<String, DbError> {
        narration_storage(self)
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
        .map_err(|e| DbError::Key(format!("delimiters serialize: {e}")))
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
        .map_err(|e| DbError::Key(format!("delimiters parse: {e}")))?;
    let mut out = Vec::with_capacity(raw.len());
    for mut value in raw {
        if let serde_json::Value::Object(map) = &mut value {
            map.entry("kind")
                .or_insert_with(|| serde_json::Value::String("wrap".to_string()));
        }
        let delim: TemplateDelimiter = serde_json::from_value(value)
            .map_err(|e| DbError::Key(format!("delimiters element: {e}")))?;
        out.push(delim);
    }
    Ok(out)
}

/// The storage form of a `narrationDelimiters` value: a bare string stays plain
/// TEXT (`*`); a pair is stored as JSON array text (`["[","]"]`) — mirroring v4's
/// `documentToRow`, which JSON-encodes any array-valued cell even on a column the
/// schema classifies as `'unknown'` (not a registered JSON column).
fn narration_storage(nd: &StringOrPair) -> Result<String, DbError> {
    match nd {
        StringOrPair::Single(s) => Ok(s.clone()),
        StringOrPair::Pair(pair) => serde_json::to_string(pair)
            .map_err(|e| DbError::Key(format!("narration serialize: {e}"))),
    }
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
    /// The narration delimiter — a bare string (plain TEXT) or an `[open, close]`
    /// pair (JSON array text).
    pub narration_delimiters: StringOrPair,
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
    pub narration_delimiters: Option<StringOrPair>,
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

    /// Insert a roleplay template with the given pinned id + timestamps. All 13
    /// columns are written; JSON columns via `serde_json::to_string`, the bool via
    /// `i64::from`, nullable columns via `Option`. `delimiters` is the JSON text of
    /// an empty array (held empty across the corpus).
    pub fn create(&self, data: &RtCreate, opts: &CreateOptions) -> Result<(), DbError> {
        let tags_json = serde_json::to_string(&data.tags)
            .map_err(|e| DbError::Key(format!("tags serialize: {e}")))?;
        let rendering_patterns_json = serde_json::to_string(&data.rendering_patterns)
            .map_err(|e| DbError::Key(format!("renderingPatterns serialize: {e}")))?;
        let delimiters_json = serialize_delimiters(&data.delimiters)?;
        let narration_delimiters = narration_storage(&data.narration_delimiters)?;
        let dialogue_detection_json: Option<String> = match &data.dialogue_detection {
            Some(dd) => Some(
                serde_json::to_string(dd)
                    .map_err(|e| DbError::Key(format!("dialogueDetection serialize: {e}")))?,
            ),
            None => None,
        };

        self.conn.execute(
            "INSERT INTO roleplay_templates \
               (id, userId, name, description, systemPrompt, isBuiltIn, tags, delimiters, \
                renderingPatterns, dialogueDetection, narrationDelimiters, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
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
    /// every column EXCEPT `delimiters`, whose read-side `kind:'wrap'` backfill
    /// (`parse_delimiters`) upgrades a legacy kind-less entry. So this port always
    /// rewrites `delimiters` (from the patch, or from the re-parsed existing value)
    /// and leaves the other columns partial — byte-identical to v4.
    pub fn update(&self, id: &str, patch: &RtUpdate) -> Result<bool, DbError> {
        // v4 `_update` first `findById`s — the row must exist or it's a no-op.
        // Fetch the current `delimiters` in the same read for the backfill rewrite.
        let existing_delimiters: Option<String> = self
            .conn
            .query_row(
                "SELECT delimiters FROM roleplay_templates WHERE id = ?1",
                params![id],
                |row| row.get::<_, String>(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        let Some(existing_delimiters) = existing_delimiters else {
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
                .map_err(|e| DbError::Key(format!("tags serialize: {e}")))?;
            assignments.push(format!("tags = ?{}", values.len() + 1));
            values.push(Box::new(tags_json));
        }
        if let Some(rendering_patterns) = &patch.rendering_patterns {
            let json = serde_json::to_string(rendering_patterns)
                .map_err(|e| DbError::Key(format!("renderingPatterns serialize: {e}")))?;
            assignments.push(format!("renderingPatterns = ?{}", values.len() + 1));
            values.push(Box::new(json));
        }
        if let Some(dialogue_detection) = &patch.dialogue_detection {
            let json = serde_json::to_string(dialogue_detection)
                .map_err(|e| DbError::Key(format!("dialogueDetection serialize: {e}")))?;
            assignments.push(format!("dialogueDetection = ?{}", values.len() + 1));
            values.push(Box::new(json));
        }
        if let Some(narration_delimiters) = &patch.narration_delimiters {
            let storage = narration_storage(narration_delimiters)?;
            assignments.push(format!("narrationDelimiters = ?{}", values.len() + 1));
            values.push(Box::new(storage));
        }
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

    /// v4 seeder UPDATE path: `collection.updateOne({id}, {$set: {systemPrompt,
    /// description, delimiters, renderingPatterns, dialogueDetection,
    /// narrationDelimiters, updatedAt}})`. Unlike the public [`Self::update`], this
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
               dialogueDetection = ?5, narrationDelimiters = ?6, updatedAt = ?7 \
             WHERE id = ?8",
            params![
                patch.system_prompt,
                patch.description,
                patch.delimiters_json,
                patch.rendering_patterns_json,
                patch.dialogue_detection_json,
                patch.narration_delimiters,
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
    /// `narrationDelimiters` storage form (plain string or JSON pair text).
    pub narration_delimiters: String,
    pub updated_at: String,
}

/// **Scoped read** for the roleplay-template fallback chain
/// ([`crate::services::participant_resolver`]): the `systemPrompt` of a template,
/// v4 `roleplayTemplates.findById(id)?.systemPrompt`. Follows the scoped-read
/// precedent [`super::chat_settings::find_auto_housekeeping_settings_by_user_id`]
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
