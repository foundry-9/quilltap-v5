//! P4.D126 unit 3 — the legacy connection-profile column seeding (v4
//! `e000d6bfc`, bug 103, `lib/llm/connection-profile-legacy-fields.ts`).
//!
//! Tier-1 exact against v4's REAL `seedLegacyConnectionProfileFields`, over
//! providers × `supportsImageUpload` {absent, true, false} ×
//! `multiCharacterPrefill` {absent, true, false, null, 1, "true"}.
//!
//! **Why this family is the load-bearing one for unit 3.** Both v5 call sites —
//! backup restore and `.qtap` import — answer through the same helper, which is
//! what makes v4's claim ("the two paths cannot drift") true of the port as
//! well. The restore half additionally gets a DB-state diff
//! (`system_restore_state`'s `restore_legacy_profiles_replace`) and a
//! migration-vintage pin (`restore_vintage_state`); the import half's wiring is
//! pinned v5-side in `services::quilltap_import::profiles`. This is the
//! DECISION itself.
//!
//! What the corpus pins beyond the obvious:
//!
//!   - the condition is `=== undefined`, so a stored `false` and a stored `null`
//!     are BOTH left alone. A truthiness test would flip a GOOGLE profile whose
//!     owner deliberately turned vision off — and it would pass every arm that
//!     only ever tests absent-vs-true.
//!   - the lookup is `(profile.provider ?? '').toUpperCase()`: casing folds, a
//!     record with no provider at all resolves to `''` rather than throwing, and
//!     whitespace does NOT fold (`' GOOGLE'` misses the map). v5's import site
//!     used to test membership on the stored casing, which is the bug the
//!     `openai-lower` rows catch.
//!   - `multiCharacterPrefill` is seeded as an explicit `null` — the "never
//!     chosen" tri-state — never as a boolean.
//!   - the helper returns a COPY: `inputUntouched` is asserted on v4's side, so
//!     a helper that started mutating its argument reddens here.
//!
//! ## ⚠ ONE RECORDED DIVERGENCE — a non-boolean stored `multiCharacterPrefill`
//!
//! v4's helper only ever FILLS: a stored `1` or `"true"` is not `undefined`, so
//! it survives into the returned record verbatim. v5 models the column as
//! `Option<bool>`, so it reads a non-boolean as "no stored boolean" and writes
//! SQL NULL.
//!
//! Nothing observable follows from it on the import path — v4's
//! `ConnectionProfileSchema` types the column `z.boolean().nullable()
//! .optional()`, so `repos.connections.create` refuses the whole profile, and
//! v5's `parse_connection_profile` refuses it at exactly the same point with a
//! warning. On the RESTORE path v4 refuses the profile the same way while v5,
//! which has no Zod layer there, writes the row with a NULL — a **pre-existing**
//! consequence of that standing shape, not of this port, and out of this lane's
//! scope. It is asserted in BOTH directions below so neither side can move
//! without saying so.
//!
//! Generate the oracle output (from the v4 checkout; pin a detached worktree
//! via `recipe_sweep.py --v4` when v4 HEAD has moved past the baseline):
//!   cd ~/source/quilltap-server
//!   ~/.nvm/versions/node/v24.13.1/bin/npx tsx \
//!     ~/source/quilltap-v5/harness/oracle/cases/connection-profile-legacy-fields.ts \
//!     > /tmp/oracle-cp-legacy-fields.ndjson
//! Run:
//!   QT_ORACLE_CP_LEGACY_FIELDS=/tmp/oracle-cp-legacy-fields.ndjson \
//!     cargo test -p quilltap-harness --test connection_profile_legacy_fields_equivalence

use quilltap_core::services::connection_profile_legacy_fields::seed_legacy_connection_profile_fields;
use serde_json::{json, Map, Value};

/// The oracle's stand-in for a JS `undefined` provider and for a key the profile
/// object never carried (NDJSON has neither value).
const UNDEFINED: &str = "<undefined>";
const ABSENT: &str = "<absent>";

fn is_absent(v: &Value) -> bool {
    v.as_str() == Some(ABSENT)
}

#[test]
fn connection_profile_legacy_fields_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_CP_LEGACY_FIELDS") else {
        eprintln!("SKIP: set QT_ORACLE_CP_LEGACY_FIELDS to the oracle NDJSON (see the header).");
        return;
    };
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let cases: Vec<Value> = raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle line is JSON"))
        .collect();
    assert!(!cases.is_empty(), "oracle produced no cases");

    let mut failed: Vec<String> = Vec::new();
    let mut ran = 0usize;
    let mut recorded_non_boolean = 0usize;
    let mut fallback_cases = 0usize;
    let mut recorded_fallback_non_scalar = 0usize;
    let mut self_reference_rows = 0usize;

    for case in &cases {
        let id = case["id"].as_str().expect("case id");

        // ── P4.D135: the 4.10 fallback-chain block ──────────────────────────
        if case["kind"].as_str() == Some("fallback") {
            fallback_cases += 1;
            let record_id = &case["recordId"];
            let stored_fallback = &case["storedFallback"];
            let stored_tier = &case["storedTier"];

            let mut profile = Map::new();
            profile.insert("name".into(), json!("A Profile"));
            profile.insert("provider".into(), json!("OPENAI"));
            if !is_absent(record_id) {
                profile.insert("id".into(), record_id.clone());
            }
            if !is_absent(stored_fallback) {
                profile.insert("fallbackProfileId".into(), stored_fallback.clone());
            }
            if !is_absent(stored_tier) {
                profile.insert("allowTierFallback".into(), stored_tier.clone());
            }
            let got = seed_legacy_connection_profile_fields(&Value::Object(profile));

            let mut diffs: Vec<String> = Vec::new();

            // v4's helper returns a copy: BOTH keys must end up present.
            for key in ["fallbackProfileId", "allowTierFallback"] {
                if is_absent(&case[key]) {
                    diffs.push(format!("    oracle dropped `{key}` from its copy"));
                }
            }
            for (key, got_flag) in [
                ("seededFallbackProfileId", got.seeded_fallback_profile_id),
                ("seededAllowTierFallback", got.seeded_allow_tier_fallback),
            ] {
                if case[key] != json!(got_flag) {
                    diffs.push(format!(
                        "    {key}: rust {got_flag} != oracle {}",
                        case[key]
                    ));
                }
            }

            // `fallbackProfileId`: exact where v4's value is a string or null.
            // A carried NON-STRING (the corpus's stored `7`) is passed through
            // untouched by v4 and read as absent by v5 — the same RECORDED
            // divergence class as `multiCharacterPrefill`'s non-boolean, and
            // guarded in BOTH directions the same way.
            let oracle_fallback = &case["fallbackProfileId"];
            if oracle_fallback.is_string() || oracle_fallback.is_null() {
                let rust = got
                    .fallback_profile_id
                    .clone()
                    .map(Value::String)
                    .unwrap_or(Value::Null);
                // v5 folds the empty string to None; v4 keeps it — and then
                // its repo's `UUIDSchema` parse REFUSES the whole profile, so
                // v4 never writes the cell at all where v5 lands NULL. That
                // divergence is a member of the recorded `.qtap` Zod-format
                // gap (`quilltap_import/profiles.rs`), reachable only from a
                // hand-edited bundle; this carve-out deliberately blinds the
                // comparison to it so the rest of the row stays
                // discriminating. (Justification corrected at the round-1
                // unification review, 2026-09-01.)
                let oracle_cell = if oracle_fallback.as_str() == Some("") {
                    Value::Null
                } else {
                    oracle_fallback.clone()
                };
                if rust != oracle_cell {
                    diffs.push(format!(
                        "    fallbackProfileId: rust {rust} != oracle {oracle_fallback}"
                    ));
                }
            } else {
                recorded_fallback_non_scalar += 1;
                if got.fallback_profile_id.is_some() {
                    diffs.push(format!(
                        "    fallbackProfileId: the RECORDED divergence has CLOSED — v5 now \
                         carries the non-string {oracle_fallback} as {:?}. Retire the carve-out.",
                        got.fallback_profile_id
                    ));
                }
                if *oracle_fallback != *stored_fallback {
                    diffs.push(format!(
                        "    fallbackProfileId: v4 no longer passes a non-string through \
                         ({stored_fallback} became {oracle_fallback}) — the recorded \
                         divergence needs re-measuring, not deleting."
                    ));
                }
            }

            // `allowTierFallback`: same shape, one type over.
            let oracle_tier = &case["allowTierFallback"];
            if oracle_tier.is_boolean() {
                if *oracle_tier != json!(got.allow_tier_fallback) {
                    diffs.push(format!(
                        "    allowTierFallback: rust {} != oracle {oracle_tier}",
                        got.allow_tier_fallback
                    ));
                }
            } else {
                recorded_fallback_non_scalar += 1;
                if got.allow_tier_fallback {
                    diffs.push(format!(
                        "    allowTierFallback: the RECORDED divergence has CLOSED — v5 now \
                         carries the non-boolean {oracle_tier} as true. Retire the carve-out."
                    ));
                }
                if *oracle_tier != *stored_tier {
                    diffs.push(format!(
                        "    allowTierFallback: v4 no longer passes a non-boolean through \
                         ({stored_tier} became {oracle_tier}) — re-measure."
                    ));
                }
            }

            // The self-reference strip, and its JS-TRUTHY gate: the empty
            // string is NOT compared, so an empty id + an empty fallback is a
            // NON-strip on both sides.
            let is_self_reference = stored_fallback.is_string()
                && !is_absent(stored_fallback)
                && !is_absent(record_id)
                && stored_fallback.as_str() != Some("")
                && stored_fallback == record_id;
            if is_self_reference {
                self_reference_rows += 1;
                if !oracle_fallback.is_null() {
                    diffs.push(format!(
                        "    v4 no longer strips a self-referential understudy \
                         ({oracle_fallback}) — re-measure the rule."
                    ));
                }
                if !got.dropped_self_reference {
                    diffs.push("    rust did not record the self-reference drop".into());
                }
            } else if got.dropped_self_reference {
                diffs.push(format!(
                    "    rust dropped a self-reference that is not one \
                     (stored {stored_fallback}, id {record_id})"
                ));
            }

            if diffs.is_empty() {
                ran += 1;
            } else {
                failed.push(format!("{id}:\n{}", diffs.join("\n")));
            }
            continue;
        }

        let provider = &case["provider"];
        let stored_image = &case["storedImage"];
        let stored_prefill = &case["storedPrefill"];

        // Rebuild the EXACT record v4 was handed.
        let mut profile = Map::new();
        profile.insert("id".into(), json!("p1"));
        profile.insert("name".into(), json!("A Profile"));
        if provider.as_str() != Some(UNDEFINED) {
            profile.insert("provider".into(), provider.clone());
        }
        if !is_absent(stored_image) {
            profile.insert("supportsImageUpload".into(), stored_image.clone());
        }
        if !is_absent(stored_prefill) {
            profile.insert("multiCharacterPrefill".into(), stored_prefill.clone());
        }
        let got = seed_legacy_connection_profile_fields(&Value::Object(profile));

        let mut diffs: Vec<String> = Vec::new();

        // v4's helper returns a copy, so BOTH keys must end up present.
        for key in ["supportsImageUpload", "multiCharacterPrefill"] {
            if is_absent(&case[key]) {
                diffs.push(format!("    oracle dropped `{key}` from its copy"));
            }
        }
        if case["inputUntouched"] != json!(true) {
            diffs.push("    v4's helper MUTATED its argument (it must return a copy)".into());
        }

        // The two `=== undefined` flags both call sites read for the debug line.
        for (key, got_flag) in [
            (
                "seededSupportsImageUpload",
                got.seeded_supports_image_upload,
            ),
            (
                "seededMultiCharacterPrefill",
                got.seeded_multi_character_prefill,
            ),
        ] {
            if case[key] != json!(got_flag) {
                diffs.push(format!(
                    "    {key}: rust {got_flag} != oracle {}",
                    case[key]
                ));
            }
        }

        // `supportsImageUpload` is a boolean on both sides in every reachable
        // shape (the corpus stores only absent/true/false there).
        if case["supportsImageUpload"] != json!(got.supports_image_upload) {
            diffs.push(format!(
                "    supportsImageUpload: rust {} != oracle {}",
                got.supports_image_upload, case["supportsImageUpload"]
            ));
        }

        // `multiCharacterPrefill`: exact where v4's value is a boolean or null;
        // the recorded divergence where it is neither (see the header).
        let oracle_prefill = &case["multiCharacterPrefill"];
        if oracle_prefill.is_boolean() || oracle_prefill.is_null() {
            if *oracle_prefill != json!(got.multi_character_prefill) {
                diffs.push(format!(
                    "    multiCharacterPrefill: rust {:?} != oracle {oracle_prefill}",
                    got.multi_character_prefill
                ));
            }
        } else {
            recorded_non_boolean += 1;
            if got.multi_character_prefill.is_some() {
                diffs.push(format!(
                    "    multiCharacterPrefill: the RECORDED divergence has CLOSED — v5 now \
                     carries the non-boolean {oracle_prefill} as {:?}. Retire the carve-out.",
                    got.multi_character_prefill
                ));
            }
            if *oracle_prefill != *stored_prefill {
                diffs.push(format!(
                    "    multiCharacterPrefill: v4 no longer passes a non-boolean through \
                     ({stored_prefill} became {oracle_prefill}) — the recorded divergence \
                     needs re-measuring, not deleting."
                ));
            }
        }

        if diffs.is_empty() {
            ran += 1;
        } else {
            failed.push(format!("{id}:\n{}", diffs.join("\n")));
        }
    }

    assert!(
        failed.is_empty(),
        "{} of {} case(s) failed:\n{}",
        failed.len(),
        cases.len(),
        failed.join("\n")
    );
    assert_eq!(ran, cases.len(), "every case must be compared");
    // Shape assertion, not a hand count: the corpus must still carry the
    // non-boolean rows the recorded divergence is about, and the boolean ones
    // that make up the bulk of the claim.
    assert!(
        recorded_non_boolean > 0,
        "the corpus lost its non-boolean stored-prefill rows — the recorded \
         divergence is no longer measured"
    );
    assert!(
        cases.len() - recorded_non_boolean - fallback_cases > 100,
        "the corpus lost its reachable-domain rows ({} of {})",
        cases.len() - recorded_non_boolean - fallback_cases,
        cases.len()
    );
    // P4.D135 shape assertions: the fallback block must still be there, must
    // still carry the non-scalar rows its recorded divergence is about, and
    // must still exercise the self-reference strip. A corpus that lost any of
    // the three would go green having measured nothing.
    assert!(
        fallback_cases > 50,
        "the corpus lost its fallback-pair block ({fallback_cases} rows)"
    );
    assert!(
        recorded_fallback_non_scalar > 0,
        "the corpus lost its non-scalar fallback rows — the recorded divergence \
         is no longer measured"
    );
    assert!(
        self_reference_rows > 0,
        "the corpus lost its self-referential-understudy rows"
    );
    eprintln!(
        "OK {} cases ({} of them the recorded non-boolean divergence; {} fallback \
         rows, {} of them the recorded non-scalar divergence, {} self-references)",
        ran,
        recorded_non_boolean,
        fallback_cases,
        recorded_fallback_non_scalar,
        self_reference_rows
    );
}
