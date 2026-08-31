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
//! The 4.10 fallback-chain columns — **`fallbackProfileId`** and
//! **`allowTierFallback`** (v4 `65f5021c8`) — are named here too, but for a
//! different reason. Their table DEFAULTs (NULL and 0) *are* the neutral
//! answer: a profile from an archive that predates them simply has no
//! understudy, which is exactly how it behaved before the columns existed. What
//! they need instead is a sanity check, because `fallbackProfileId` is the
//! module's first column holding a *reference*: a hand-edited bundle can name
//! the profile itself, and a self-referential chain is the one shape config
//! validation forbids.
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
    /// The value `fallbackProfileId` takes: the carried understudy id, or
    /// `None` for "no understudy named" (both an absent key and a
    /// self-reference land here). **Always written.**
    pub fallback_profile_id: Option<String>,
    /// The value `allowTierFallback` takes. **Always written.**
    pub allow_tier_fallback: bool,
    /// v4's `rawProfileData.fallbackProfileId === undefined`.
    pub seeded_fallback_profile_id: bool,
    /// v4's `rawProfileData.allowTierFallback === undefined`.
    pub seeded_allow_tier_fallback: bool,
    /// Whether a carried `fallbackProfileId` named the profile itself and was
    /// dropped. v4 does this silently; v5 surfaces it on the debug line.
    pub dropped_self_reference: bool,
}

impl SeededLegacyFields {
    /// Whether either column was seeded — v4's condition for the debug line.
    pub fn seeded_anything(&self) -> bool {
        self.seeded_supports_image_upload
            || self.seeded_multi_character_prefill
            || self.seeded_fallback_profile_id
            || self.seeded_allow_tier_fallback
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

    // Fallback chain (4.10). Absent means "no understudy named", which is both
    // the table DEFAULT and the pre-column behaviour — stated explicitly so a
    // later change to either DEFAULT can't quietly rewrite a restored profile.
    let stored_fallback = raw.get("fallbackProfileId");
    let seeded_fallback_profile_id = stored_fallback.is_none();
    let carried_fallback = stored_fallback
        .and_then(Value::as_str)
        .map(str::to_string)
        // v4's gate is JS-truthy (`seeded.fallbackProfileId &&`), so an empty
        // string is neither checked nor cleared — it rides through as the empty
        // string it was. Reproduced by treating only a non-empty value as
        // carried; an empty one falls to the same `None` v5's `os()` reader
        // would produce for it anyway.
        .filter(|id| !id.is_empty());
    let stored_allow_tier = raw.get("allowTierFallback");
    let seeded_allow_tier_fallback = stored_allow_tier.is_none();
    let allow_tier_fallback = stored_allow_tier.and_then(Value::as_bool).unwrap_or(false);

    // A profile can't understudy itself: the chain would be one attempt wearing
    // two names. Config validation refuses it on the way in; an archive is
    // data, not a contract, so it gets refused on the way back too.
    let own_id = raw.get("id").and_then(Value::as_str);
    let dropped_self_reference =
        carried_fallback.is_some() && carried_fallback.as_deref() == own_id;
    let fallback_profile_id = if dropped_self_reference {
        None
    } else {
        carried_fallback
    };

    SeededLegacyFields {
        supports_image_upload,
        multi_character_prefill,
        seeded_supports_image_upload,
        seeded_multi_character_prefill,
        fallback_profile_id,
        allow_tier_fallback,
        seeded_fallback_profile_id,
        seeded_allow_tier_fallback,
        dropped_self_reference,
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
                fallback_profile_id: None,
                allow_tier_fallback: false,
                seeded_fallback_profile_id: true,
                seeded_allow_tier_fallback: true,
                dropped_self_reference: false,
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
                fallback_profile_id: None,
                allow_tier_fallback: false,
                seeded_fallback_profile_id: true,
                seeded_allow_tier_fallback: true,
                dropped_self_reference: false,
            }
        );
    }

    #[test]
    fn a_carried_understudy_rides_through() {
        let out = seed(json!({
            "id": "p-primary",
            "provider": "OPENAI",
            "fallbackProfileId": "p-understudy",
            "allowTierFallback": true,
        }));
        assert_eq!(out.fallback_profile_id.as_deref(), Some("p-understudy"));
        assert!(out.allow_tier_fallback);
        assert!(!out.seeded_fallback_profile_id);
        assert!(!out.seeded_allow_tier_fallback);
        assert!(!out.dropped_self_reference);
    }

    #[test]
    fn an_absent_pair_seeds_the_neutral_answer() {
        let out = seed(json!({ "id": "p-primary", "provider": "OPENAI" }));
        assert_eq!(out.fallback_profile_id, None);
        assert!(!out.allow_tier_fallback);
        assert!(out.seeded_fallback_profile_id);
        assert!(out.seeded_allow_tier_fallback);
        assert!(out.seeded_anything());
    }

    /// The one shape config validation forbids and an archive can still carry.
    #[test]
    fn a_self_referential_understudy_is_dropped() {
        let out = seed(json!({
            "id": "p-primary",
            "provider": "OPENAI",
            "fallbackProfileId": "p-primary",
        }));
        assert_eq!(out.fallback_profile_id, None);
        assert!(out.dropped_self_reference);
        assert!(!out.seeded_fallback_profile_id, "the key WAS carried");
    }

    /// v4's self-reference gate is JS-truthy, so an empty string never reaches
    /// the comparison — and a stored `false` never becomes `true`.
    #[test]
    fn an_empty_understudy_is_neither_checked_nor_promoted() {
        let out = seed(json!({
            "id": "",
            "provider": "OPENAI",
            "fallbackProfileId": "",
            "allowTierFallback": false,
        }));
        assert_eq!(out.fallback_profile_id, None);
        assert!(!out.dropped_self_reference, "an empty id is falsy in v4");
        assert!(!out.allow_tier_fallback);
        assert!(!out.seeded_allow_tier_fallback, "a stored false is carried");
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
            "fallbackProfileId": null,
            "allowTierFallback": false,
        }));
        assert!(!out.seeded_anything());
    }
}
