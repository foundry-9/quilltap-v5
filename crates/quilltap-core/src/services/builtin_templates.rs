//! The built-in roleplay templates seeder (P4.4u3, family 1).
//!
//! Ports v4's `RoleplayTemplatesRepository.seedBuiltInTemplates()`
//! (`roleplay-templates.repository.ts:127`), which runs on EVERY startup
//! (`lib/startup/seed-initial-data.ts:57`) — not first-boot-gated — so code
//! changes to the presets propagate on upgrade. Per built-in template:
//!
//!   - **absent** (no `isBuiltIn` row by that name) → INSERT a fresh row with a
//!     minted UUID + `now` timestamps, through the repo's validating [`create`]
//!     (v4 `this.validate(...) → collection.insertOne`), so `delimiters` land in
//!     Zod SCHEMA key order;
//!   - **present** → UPDATE the six drift-tracked fields + `updatedAt` in place,
//!     through the RAW [`seed_update`] (v4 `collection.updateOne({id}, {$set})`),
//!     so `delimiters` land in the SEED-LITERAL key order — the two paths store
//!     DIFFERENT bytes for the same data, a v4 quirk this port reproduces.
//!
//! The seed data ([`BUILTIN_TEMPLATES_JSON`]) is generated verbatim from v4's real
//! seeder by `harness/oracle/provision/dump-builtin-templates.ts` (the
//! `[[byte-exact-static-data-transcription]]` pattern). `delimiters` there is in
//! SEED-LITERAL order (dumped after a second, update-path seed); the INSERT path
//! re-parses it into the typed union, whose serialization re-emits SCHEMA order —
//! byte-identical to v4's Zod parse. Both paths are proven by
//! `builtin_templates_equivalence`.
//!
//! [`create`]: crate::db::roleplay_templates::RoleplayTemplatesRepository::create
//! [`seed_update`]: crate::db::roleplay_templates::RoleplayTemplatesRepository::seed_update

use rusqlite::{Connection, OptionalExtension};
use serde::Deserialize;

use crate::clock;
use crate::db::roleplay_templates::{
    CreateOptions, DialogueDetection, RenderingPattern, RoleplayTemplatesRepository, RtCreate,
    SeedUpdate, StringOrPair, TemplateDelimiter,
};
use crate::db::DbError;

/// The verbatim built-in template seed data (Standard / Quilltap RP). Regenerate
/// with `harness/oracle/provision/dump-builtin-templates.ts` (recipe in its
/// header). `delimiters` is SEED-LITERAL key order.
static BUILTIN_TEMPLATES_JSON: &str = include_str!("builtin_templates.json");

/// One built-in template as authored in the data module. The JSON columns are held
/// as raw [`serde_json::Value`] so the UPDATE path can re-emit their exact
/// seed-literal bytes; the INSERT path converts them to the typed marshaling.
#[derive(Deserialize)]
struct BuiltInTemplate {
    name: String,
    description: String,
    #[serde(rename = "systemPrompt")]
    system_prompt: String,
    /// Raw delimiter objects in SEED-LITERAL key order.
    delimiters: Vec<serde_json::Value>,
    /// Raw rendering-pattern objects.
    #[serde(rename = "renderingPatterns")]
    rendering_patterns: Vec<serde_json::Value>,
    /// Raw dialogue-detection object, or `null`.
    #[serde(rename = "dialogueDetection")]
    dialogue_detection: Option<serde_json::Value>,
    /// The narration delimiter — bare string or `[open, close]` pair.
    #[serde(rename = "narrationDelimiters")]
    narration_delimiters: StringOrPair,
}

/// Seed (insert-or-update) the built-in roleplay templates into `main` — v4's
/// `seedBuiltInTemplates()`. Idempotent and safe to run on every startup:
/// existing built-ins are drift-updated in place, absent ones are inserted with a
/// minted id. Keyed by `(name, isBuiltIn=true)`, so a same-name user template is
/// never disturbed.
pub fn seed_built_in_templates(main: &Connection) -> Result<(), DbError> {
    // v4's `seedBuiltInTemplates` runs inside a `safeQuery` that swallows errors
    // (a bare / not-yet-provisioned db has no `roleplay_templates` table). Mirror
    // that graceful skip without swallowing genuine write failures.
    let has_table: bool = main
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'roleplay_templates'",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !has_table {
        return Ok(());
    }

    let templates: Vec<BuiltInTemplate> = serde_json::from_str(BUILTIN_TEMPLATES_JSON)
        .map_err(|e| DbError::Internal(format!("builtin_templates.json: {e}")))?;
    let repo = RoleplayTemplatesRepository::new(main);

    for template in &templates {
        match repo.find_built_in_id_by_name(&template.name)? {
            None => {
                let now = clock::now_iso();
                repo.create(
                    &template.to_create()?,
                    &CreateOptions {
                        id: uuid::Uuid::new_v4().to_string(),
                        created_at: now.clone(),
                        updated_at: now,
                    },
                )?;
            }
            Some(id) => {
                repo.seed_update(&id, &template.to_seed_update(&clock::now_iso())?)?;
            }
        }
    }
    Ok(())
}

impl BuiltInTemplate {
    /// The INSERT-path payload (v4 `this.validate(...)` → schema key order). The
    /// raw JSON columns are re-parsed into the typed marshaling, which serializes
    /// them in Zod schema order.
    fn to_create(&self) -> Result<RtCreate, DbError> {
        let delimiters: Vec<TemplateDelimiter> =
            serde_json::from_value(serde_json::Value::Array(self.delimiters.clone()))
                .map_err(|e| DbError::Internal(format!("builtin delimiters: {e}")))?;
        let rendering_patterns: Vec<RenderingPattern> =
            serde_json::from_value(serde_json::Value::Array(self.rendering_patterns.clone()))
                .map_err(|e| DbError::Internal(format!("builtin renderingPatterns: {e}")))?;
        let dialogue_detection: Option<DialogueDetection> = match &self.dialogue_detection {
            Some(v) => Some(
                serde_json::from_value(v.clone())
                    .map_err(|e| DbError::Internal(format!("builtin dialogueDetection: {e}")))?,
            ),
            None => None,
        };
        Ok(RtCreate {
            user_id: None,
            name: self.name.clone(),
            description: Some(self.description.clone()),
            system_prompt: self.system_prompt.clone(),
            is_built_in: true,
            tags: Vec::new(),
            delimiters,
            rendering_patterns,
            dialogue_detection,
            narration_delimiters: self.narration_delimiters.clone(),
        })
    }

    /// The UPDATE-path payload (v4 `collection.updateOne` raw `$set` → seed-literal
    /// key order). Every JSON column is serialized straight from its raw value, so
    /// the stored bytes equal v4's `JSON.stringify(template.<col>)`.
    fn to_seed_update(&self, now: &str) -> Result<SeedUpdate, DbError> {
        let delimiters_json = serde_json::to_string(&self.delimiters)
            .map_err(|e| DbError::Internal(format!("builtin delimiters serialize: {e}")))?;
        let rendering_patterns_json = serde_json::to_string(&self.rendering_patterns)
            .map_err(|e| DbError::Internal(format!("builtin renderingPatterns serialize: {e}")))?;
        let dialogue_detection_json = match &self.dialogue_detection {
            Some(v) => Some(serde_json::to_string(v).map_err(|e| {
                DbError::Internal(format!("builtin dialogueDetection serialize: {e}"))
            })?),
            None => None,
        };
        Ok(SeedUpdate {
            system_prompt: self.system_prompt.clone(),
            description: self.description.clone(),
            delimiters_json,
            rendering_patterns_json,
            dialogue_detection_json,
            narration_delimiters: self.narration_delimiters.storage()?,
            updated_at: now.to_string(),
        })
    }
}
