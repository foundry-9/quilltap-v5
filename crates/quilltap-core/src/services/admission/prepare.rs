//! The safety nets a candidate row passes before it is embedded and decided —
//! v4 `lib/memory/admission/prepare.ts` (`8af764514`).
//!
//! v4's why, carried:
//!
//!  - **Name presence.** An extracted (AUTO) row that claims to be about
//!    someone other than its holder, when that someone is never named in the
//!    text, was almost certainly mis-attributed by the model: the claim
//!    collapses to the holder. Hand-written rows keep their stated subject.
//!  - **Episodic fallback anchors.** When the extractor left out `entities` or
//!    `occurredAt`, the deterministic date and proper-noun regexes recover
//!    them from the text (a date phrase resolves against the source message's
//!    time on the instance calendar). Fills gaps only; hand-written rows pass
//!    untouched.
//!
//! Reads only (characters). The pre-shapes were the old gate's
//! `apply_name_presence_check` / `apply_episodic_fallback_anchors`; these are
//! the pin's.

use serde_json::Value;

use crate::about_character::resolve_about_character_id;
use crate::admission::{extract_novel_details, AdmissionRow};
use crate::db::characters_read;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::memory_time::clock::resolve_when_phrase;

/// Cap on regex-derived fallback entities so noise can't flood the column
/// (v4 `FALLBACK_ANCHOR_MAX_ENTITIES`).
const FALLBACK_ANCHOR_MAX_ENTITIES: usize = 6;

/// v4's `row.source && row.source !== 'AUTO'` — a truthy source other than
/// AUTO (an empty string reads as no source).
fn is_non_auto(source: Option<&str>) -> bool {
    matches!(source, Some(s) if !s.is_empty() && s != "AUTO")
}

/// A character's `{ name, aliases }` (and `controlledBy`) as the pure
/// resolver reads them.
fn name_and_aliases(character: &Value) -> (String, Vec<String>) {
    let name = character
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let aliases = character
        .get("aliases")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    (name, aliases)
}

/// Both characters, read through the vault-overlaid `characters_read::
/// find_by_id` (v4 `repos.characters.findById`).
fn read_pair(
    db: &Db,
    about_id: &str,
    holder_id: &str,
) -> Result<(Option<Value>, Option<Value>), DbError> {
    db.read_main(|main| {
        db.read_mount_index(|mount| {
            let about = characters_read::find_by_id(main, mount, about_id)?;
            let holder = characters_read::find_by_id(main, mount, holder_id)?;
            Ok((about, holder))
        })
    })
}

/// Collapse a mis-attributed AUTO subject to the holder (v4
/// `applyNamePresenceCheck(row, notes)`). Never fails.
///
/// No proposal, a self proposal, or a truthy non-AUTO source → the row as it
/// is. Otherwise both characters are read and the pure
/// [`resolve_about_character_id`] decides; a flip pushes the note `subject
/// <id> not named in the text; recorded as a self memory`, logs DEBUG
/// `[Admission] Name-presence check collapsed the subject to the holder
/// {holderCharacterId, proposedAboutCharacterId}` and sets the subject to the
/// holder (LR-8: the stored `aboutCharacterId` then IS the holder's id, as
/// v4's `|| null` keeps it). A failed read logs WARN `[Admission]
/// Name-presence check failed; using proposed aboutCharacterId unchanged
/// {holderCharacterId, proposedAboutCharacterId, error}` and passes the row
/// through — the safety net never blocks a write.
pub fn apply_name_presence_check(
    db: &Db,
    row: AdmissionRow,
    notes: &mut Vec<String>,
) -> AdmissionRow {
    let proposed = match row.about_character_id.as_deref() {
        None | Some("") => return row,
        Some(p) if p == row.character_id => return row,
        Some(p) => p.to_string(),
    };
    // Only second-guess AUTO-extracted attributions; MANUAL memories carry the
    // user's deliberate choice of about-target.
    if is_non_auto(row.source.as_deref()) {
        return row;
    }
    let (about, holder) = match read_pair(db, &proposed, &row.character_id) {
        Ok(pair) => pair,
        Err(e) => {
            tracing::warn!(
                holderCharacterId = %row.character_id,
                proposedAboutCharacterId = %proposed,
                error = %e,
                "[Admission] Name-presence check failed; using proposed aboutCharacterId unchanged"
            );
            return row;
        }
    };
    let holder_parts = holder.as_ref().map(name_and_aliases);
    let about_parts = about.as_ref().map(|c| {
        let (name, aliases) = name_and_aliases(c);
        let controlled_by = c
            .get("controlledBy")
            .and_then(Value::as_str)
            .unwrap_or("llm")
            .to_string();
        (name, aliases, controlled_by)
    });
    let text = format!("{}\n{}", row.summary, row.content);
    let resolution = resolve_about_character_id(
        &row.character_id,
        holder_parts
            .as_ref()
            .map(|(name, aliases)| (name.as_str(), aliases.as_slice())),
        Some(&proposed),
        about_parts.as_ref().map(|(name, aliases, controlled_by)| {
            (name.as_str(), aliases.as_slice(), controlled_by.as_str())
        }),
        &text,
    );
    if !resolution.flipped {
        return row;
    }
    notes.push(format!(
        "subject {proposed} not named in the text; recorded as a self memory"
    ));
    tracing::debug!(
        holderCharacterId = %row.character_id,
        proposedAboutCharacterId = %proposed,
        "[Admission] Name-presence check collapsed the subject to the holder"
    );
    let holder_id = row.character_id.clone();
    AdmissionRow {
        about_character_id: Some(holder_id),
        ..row
    }
}

/// Fill a missing `entities` / `occurredAt` on an AUTO row from its text (v4
/// `applyEpisodicFallbackAnchors(row, timeZone)`). Never fails.
///
/// The novel details of the content (against `''`) are the source: the first
/// that resolves as a `when` phrase — against `sourceMessageTimestamp ??
/// now`, on the memory calendar `time_zone` — becomes `occurredAt`; the
/// capitalised, digit-free ones (at most six) become `entities`. v4's `try`
/// returns the row unchanged on any throw; nothing here can panic.
pub fn apply_episodic_fallback_anchors(row: AdmissionRow, time_zone: &str) -> AdmissionRow {
    if is_non_auto(row.source.as_deref()) {
        return row;
    }
    let need_entities = row.entities.as_ref().is_none_or(|e| e.is_empty());
    let need_occurred_at = row.occurred_at.as_deref().is_none_or(str::is_empty);
    if !need_entities && !need_occurred_at {
        return row;
    }
    let details = extract_novel_details(&row.content, "");
    let mut next = row;
    if need_occurred_at {
        let anchor_iso = next
            .source_message_timestamp
            .clone()
            .unwrap_or_else(crate::clock::now_iso);
        for detail in &details {
            if let Some(resolved) = resolve_when_phrase(Some(detail), &anchor_iso, time_zone) {
                next.occurred_at = Some(resolved);
                break;
            }
        }
    }
    if need_entities {
        // Proper-noun-shaped details only (skip the date/number/measure
        // captures): v4's `/^[A-Z]/` and `!/\d/` — ASCII both (no `u` flag).
        let entities: Vec<String> = details
            .iter()
            .filter(|d| d.starts_with(|c: char| c.is_ascii_uppercase()))
            .filter(|d| !d.chars().any(|c| c.is_ascii_digit()))
            .take(FALLBACK_ANCHOR_MAX_ENTITIES)
            .cloned()
            .collect();
        if !entities.is_empty() {
            next.entities = Some(entities);
        }
    }
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auto_row(content: &str) -> AdmissionRow {
        AdmissionRow {
            character_id: "holder".into(),
            content: content.into(),
            summary: "s".into(),
            source: Some("AUTO".into()),
            ..Default::default()
        }
    }

    #[test]
    fn fallback_anchors_fill_entities_and_a_dated_occasion_from_the_text() {
        let mut row = auto_row("We met Mariana at the Lighthouse on 2026-07-14.");
        row.source_message_timestamp = Some("2026-07-20T12:00:00.000Z".into());
        let out = apply_episodic_fallback_anchors(row, "UTC");
        assert_eq!(
            out.entities.as_deref(),
            Some(&["Mariana".to_string(), "Lighthouse".to_string()][..])
        );
        assert!(out.occurred_at.is_some());
    }

    #[test]
    fn fallback_anchors_leave_a_manual_row_and_a_complete_row_alone() {
        let mut manual = auto_row("We met Mariana.");
        manual.source = Some("MANUAL".into());
        assert_eq!(
            apply_episodic_fallback_anchors(manual.clone(), "UTC"),
            manual
        );
        let mut full = auto_row("We met Mariana.");
        full.entities = Some(vec!["X".into()]);
        full.occurred_at = Some("2026-01-01T00:00:00.000Z".into());
        assert_eq!(apply_episodic_fallback_anchors(full.clone(), "UTC"), full);
    }

    #[test]
    fn an_empty_source_reads_as_auto_for_the_safety_nets() {
        assert!(!is_non_auto(Some("")));
        assert!(!is_non_auto(None));
        assert!(!is_non_auto(Some("AUTO")));
        assert!(is_non_auto(Some("MANUAL")));
    }
}
