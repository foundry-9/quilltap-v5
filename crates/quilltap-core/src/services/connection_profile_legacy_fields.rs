//! v4 `lib/llm/connection-profile-legacy-fields.ts` (`e000d6bfc`, bug 103) —
//! seeding the connection-profile columns an *older* archive cannot carry.
//!
//! Backup/restore and `.qtap` import are both schema-driven: an entity is
//! re-inserted from whatever the archive held, so a column added to the schema
//! rides along for free. That is only true for a column the archive actually
//! *has*. A key absent from the archive is absent from the INSERT, and SQLite
//! then applies the table DEFAULT — the right answer for a brand-new row and the
//! wrong one for a profile whose owner made a choice before the column existed.
//!
//! Two columns on `connection_profiles` are in that position, both decided by a
//! migration on the upgrade path and by nothing at all on the restore/import
//! path:
//!
//! - **`supportsImageUpload`** (4.3+) — `DEFAULT 0`. Restoring a pre-4.3 archive
//!   stripped image upload from every profile that had it. Seeded here from the
//!   historic per-provider capability map, which is what
//!   `add-profile-supports-image-upload-field-v1` did to the same rows in place.
//! - **`multiCharacterPrefill`** (4.9+) — `DEFAULT 1` in the MIGRATED shape
//!   (`add-profile-multi-character-prefill-field.ts:64`). Restoring a pre-4.9
//!   archive turned the `[Name]` assistant prefill ON, including for Anthropic
//!   profiles, where 4.6+ rejects an assistant tail outright and every
//!   multi-character turn then fails. Seeded here as an explicit `null` — the
//!   documented "never chosen" state — so
//!   [`crate::services::multi_character_prefill::profile_uses_name_prefill`]
//!   resolves the provider default instead of a table default nobody picked.
//!
//! ⚠ **Where the two DDLs disagree, and why the differential needs the vintage
//! fixture.** v4's `generateDDL` declares `multiCharacterPrefill INTEGER` with
//! NO default, so on a freshly-provisioned instance omitting the column and
//! writing an explicit NULL land the *same cell* — bug 103's headline half is
//! invisible to every fresh-target restore differential. It is visible only on
//! an instance whose column came from the migration, which is why the pin lives
//! in `restore_vintage_state` (measured 2026-08-26: every one of the six shapes
//! below landed `multiCharacterPrefill = 1` there before this module existed).
//!
//! Both restore and import call this, so the two paths cannot drift: a `.qtap`
//! bundle and a backup ZIP carrying the same profile land the same row.
//!
//! ## The v5 shape
//!
//! v4's helper takes the record and returns a COPY with the two keys filled in;
//! v5's two call sites build their `CpCreate` explicitly (no spread), so the
//! helper returns the two DECISIONS plus the two "did it fire" flags the debug
//! lines carry. The condition is v4's `=== undefined` exactly — **key presence
//! in the raw record**, not the parsed value — so a stored `false` and a stored
//! `null` are never touched.

use serde_json::Value;

/// The providers whose models could accept an image before the flag became
/// per-profile. Frozen historic data, not a live capability map — a provider
/// that gains vision today gets it from the profile editor, never from here.
///
/// Matched case-INSENSITIVELY. `ProviderEnum` is `z.string().min(1)` — a
/// plugin-supplied id, not a closed enum — so nothing guarantees the stored
/// casing, least of all in an archive old enough to be missing the column.
pub const LEGACY_IMAGE_CAPABLE_PROVIDERS: &[&str] = &["OPENAI", "ANTHROPIC", "GOOGLE", "GROK"];

/// v4's `LEGACY_IMAGE_CAPABLE_PROVIDERS.has((provider ?? '').toUpperCase())`.
pub fn legacy_provider_is_image_capable(provider: &str) -> bool {
    let upper = provider.to_uppercase();
    LEGACY_IMAGE_CAPABLE_PROVIDERS.contains(&upper.as_str())
}

/// What the two columns must carry, and whether each was seeded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeededLegacyFields {
    /// The value `supportsImageUpload` takes.
    pub supports_image_upload: bool,
    /// The value `multiCharacterPrefill` takes: `Some(b)` a stored choice,
    /// `None` the explicit "never chosen" SQL NULL. **Always written** — the
    /// seeded record always carries the key, so the column is never omitted.
    pub multi_character_prefill: Option<bool>,
    /// v4's `rawProfileData.supportsImageUpload === undefined`.
    pub seeded_supports_image_upload: bool,
    /// v4's `rawProfileData.multiCharacterPrefill === undefined`.
    pub seeded_multi_character_prefill: bool,
}

impl SeededLegacyFields {
    /// Whether either column was seeded — v4's condition for the debug line.
    pub fn seeded_anything(&self) -> bool {
        self.seeded_supports_image_upload || self.seeded_multi_character_prefill
    }
}

/// Fill in the columns an archive older than them could not have carried.
///
/// `raw` is the archive / bundle record as it came off disk, NOT a parsed
/// struct: the decision is key PRESENCE, and every v5 parse of this record
/// folds an absent key and an explicit `null` into the same `None`.
///
/// A key the archive *did* carry is never touched, **a stored `false` and a
/// stored `null` included**.
///
/// One shape v4 refuses and v5 cannot: an explicit `supportsImageUpload: null`.
/// v4's schema is `z.boolean().default(false)`, so the default fires only on
/// `undefined` and a stored null fails the parse — the whole profile is
/// refused. v5 has no boolean to write there and lands `false`, which is what
/// its `b(record, key, false)` reader has always done. Not a shape any Quilltap
/// ever wrote; recorded rather than modeled.
pub fn seed_legacy_connection_profile_fields(raw: &Value) -> SeededLegacyFields {
    let provider = raw.get("provider").and_then(Value::as_str).unwrap_or("");

    let stored_image = raw.get("supportsImageUpload");
    let seeded_supports_image_upload = stored_image.is_none();
    let supports_image_upload = match stored_image {
        None => legacy_provider_is_image_capable(provider),
        Some(v) => v.as_bool().unwrap_or(false),
    };

    // Absent is NOT the same as unset here: the column is a tri-state, and only
    // an explicit null reads back as "never chosen".
    let stored_prefill = raw.get("multiCharacterPrefill");
    let seeded_multi_character_prefill = stored_prefill.is_none();
    let multi_character_prefill = stored_prefill.and_then(Value::as_bool);

    SeededLegacyFields {
        supports_image_upload,
        multi_character_prefill,
        seeded_supports_image_upload,
        seeded_multi_character_prefill,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn seed(v: Value) -> SeededLegacyFields {
        seed_legacy_connection_profile_fields(&v)
    }

    #[test]
    fn a_carried_pair_is_never_touched() {
        let out = seed(json!({
            "provider": "OPENAI",
            "supportsImageUpload": true,
            "multiCharacterPrefill": false,
        }));
        assert_eq!(
            out,
            SeededLegacyFields {
                supports_image_upload: true,
                multi_character_prefill: Some(false),
                seeded_supports_image_upload: false,
                seeded_multi_character_prefill: false,
            }
        );
    }

    /// The arm a truthiness test would get wrong: a GOOGLE profile whose owner
    /// deliberately turned vision OFF, and a prefill deliberately left unset.
    #[test]
    fn a_stored_false_and_a_stored_null_are_never_touched() {
        let out = seed(json!({
            "provider": "GOOGLE",
            "supportsImageUpload": false,
            "multiCharacterPrefill": null,
        }));
        assert_eq!(
            out,
            SeededLegacyFields {
                supports_image_upload: false,
                multi_character_prefill: None,
                seeded_supports_image_upload: false,
                seeded_multi_character_prefill: false,
            }
        );
    }

    #[test]
    fn an_absent_prefill_becomes_an_explicit_null() {
        let out = seed(json!({ "provider": "ANTHROPIC", "supportsImageUpload": true }));
        assert_eq!(out.multi_character_prefill, None);
        assert!(out.seeded_multi_character_prefill);
        assert!(!out.seeded_supports_image_upload);
    }

    #[test]
    fn an_absent_flag_is_seeded_from_the_historic_map() {
        for provider in LEGACY_IMAGE_CAPABLE_PROVIDERS {
            let out = seed(json!({ "provider": provider }));
            assert!(out.supports_image_upload, "{provider} was image-capable");
            assert!(out.seeded_supports_image_upload);
        }
    }

    #[test]
    fn a_provider_that_never_had_the_capability_seeds_false() {
        for provider in ["OLLAMA", "OPENROUTER", "DEEPSEEK", "Z_AI", "NANOGPT"] {
            let out = seed(json!({ "provider": provider }));
            assert!(!out.supports_image_upload, "{provider} was not");
        }
    }

    /// v4 upcases before the lookup; v5's import site used to test membership on
    /// the stored casing, so a lowercase legacy `provider` lost its vision flag.
    #[test]
    fn the_provider_match_is_case_insensitive() {
        for provider in ["openai", "Anthropic", "gOoGlE", "grok"] {
            let out = seed(json!({ "provider": provider }));
            assert!(out.supports_image_upload, "{provider} matches the map");
        }
    }

    /// v4's `(profile.provider ?? '').toUpperCase()` — a record with no provider
    /// at all does not throw, it just misses the map.
    #[test]
    fn a_missing_provider_is_the_empty_string() {
        let out = seed(json!({}));
        assert!(!out.supports_image_upload);
        assert!(out.seeded_supports_image_upload);
        assert!(out.seeded_multi_character_prefill);
        assert!(out.seeded_anything());
    }

    #[test]
    fn nothing_seeded_reports_nothing_seeded() {
        let out = seed(json!({
            "provider": "GROK",
            "supportsImageUpload": true,
            "multiCharacterPrefill": true,
        }));
        assert!(!out.seeded_anything());
    }
}
