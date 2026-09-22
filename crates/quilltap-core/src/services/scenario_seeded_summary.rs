//! The one predicate for "this chat's summary is really its own scenario"
//! (v4 `lib/chat/scenario-seeded-summary.ts`, bug 158, `da9c4f34f`).
//!
//! Until bug 158, creating a chat wrote the chosen scenario into `contextSummary`
//! as well as `scenarioText` — a leftover from before
//! `add-chat-scenario-text-field-v1` (4.1.0) gave the scenario a column of its
//! own. Every reader of `contextSummary` therefore believed a brand-new chat had
//! already been summarized, and the greeting's "Recent Conversations" block
//! handed the next character a stage direction to open from.
//!
//! Chat creation no longer does this ([`super::chat_create`]) and the boot heal
//! ([`crate::db::scenario_seeded_summary_heal`]) clears the rows on disk, but
//! neither reaches a chat that *arrives* — a `.qtap` import or a backup restore
//! carries whatever the source instance stored, and the heal has already run by
//! then. Every ingest path runs its rows through [`strip_scenario_seeded_summary`]
//! so a pre-fix export cannot reopen the bug in a fixed instance.
//!
//! Byte equality is the whole test, and it is safe for the same reason the
//! heal's is: a real summary is written only by the fold in
//! [`super::context_summary`], which replaces the column outright. Measured by
//! v4 against a live instance, **zero** of the 369 chats that had been folded at
//! least once matched this predicate, while 186 never-summarized chats did.
//!
//! The heal states the same rule in SQL. **The two must agree — change both or
//! neither.**
//!
//! ## The shape, and why it is a trait
//!
//! v4's `ScenarioSeededSummaryFields` is a structural interface — any row with
//! the two optional columns satisfies it, and `stripScenarioSeededSummary` is
//! generic over it (`<T extends ScenarioSeededSummaryFields>`). v5's two ingest
//! paths reach the predicate carrying two different things: the `.qtap` import
//! has a raw [`Value`] row and the restore has a deserialized
//! [`ChatCreate`]. [`ScenarioSeededSummaryFields`] is that structural interface,
//! so both go through ONE predicate rather than two copies that can drift.
//!
//! ## Identity
//!
//! v4 returns the row it was given, by reference, when it strips nothing, and a
//! shallow copy with `contextSummary: null` when it strips. v5 mutates in place
//! and reports whether it did, which preserves the same contract with no clone
//! on either arm — `strip_leaves_a_real_summary_by_identity` pins that the
//! untouched arm does not even reallocate the summary it was handed.

use serde_json::Value;

use crate::db::chats::ChatCreate;

/// v4's `ScenarioSeededSummaryFields`: a chat row as the ingest paths see it,
/// both columns optional. `None` covers v4's `undefined`, its `null`, and a
/// non-string cell alike — which is exactly what v4's `typeof scenario !==
/// 'string'` guard and its STRICT `===` do with all three.
pub trait ScenarioSeededSummaryFields {
    fn context_summary(&self) -> Option<&str>;
    fn scenario_text(&self) -> Option<&str>;
    /// Set `contextSummary` to SQL NULL / JSON `null`. Never touches
    /// `scenarioText`: the scenario is not the problem, its second home was.
    fn clear_context_summary(&mut self);
}

impl ScenarioSeededSummaryFields for Value {
    fn context_summary(&self) -> Option<&str> {
        self.get("contextSummary").and_then(Value::as_str)
    }
    fn scenario_text(&self) -> Option<&str> {
        self.get("scenarioText").and_then(Value::as_str)
    }
    fn clear_context_summary(&mut self) {
        if let Some(obj) = self.as_object_mut() {
            obj.insert("contextSummary".to_string(), Value::Null);
        }
    }
}

impl ScenarioSeededSummaryFields for ChatCreate {
    fn context_summary(&self) -> Option<&str> {
        self.context_summary.as_deref()
    }
    fn scenario_text(&self) -> Option<&str> {
        self.scenario_text.as_deref()
    }
    fn clear_context_summary(&mut self) {
        self.context_summary = None;
    }
}

/// v4 `isScenarioSeededSummary`: true when `contextSummary` is byte-identical to
/// the row's own non-empty `scenarioText` — the shape chat creation used to
/// produce.
///
/// The emptiness test is on the SCENARIO only, and it is v4's
/// `scenario.length === 0`, not a trim: a whitespace-only scenario seeded into
/// a whitespace-only summary IS the seed and is cleared. Two empty strings are
/// not, because an empty scenario carries no information either way.
pub fn is_scenario_seeded_summary<T: ScenarioSeededSummaryFields + ?Sized>(chat: &T) -> bool {
    match chat.scenario_text() {
        Some(scenario) if !scenario.is_empty() => chat.context_summary() == Some(scenario),
        _ => false,
    }
}

/// v4 `stripScenarioSeededSummary`: null out a scenario-seeded `contextSummary`,
/// or leave the row exactly as it was. Returns whether it stripped.
pub fn strip_scenario_seeded_summary<T: ScenarioSeededSummaryFields + ?Sized>(
    chat: &mut T,
) -> bool {
    if !is_scenario_seeded_summary(chat) {
        return false;
    }
    chat.clear_context_summary();
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SCENARIO: &str = "# Scenario: Amy's Pool\n\nAmy is in her pool.";

    #[test]
    fn strip_leaves_a_real_summary_by_identity() {
        // v4's contract is `stripScenarioSeededSummary(row) === row` when it
        // strips nothing. v5's analogue is stronger and is pinned here by
        // pointer: the untouched arm does not clone, copy, or reallocate the
        // summary string it was handed.
        let mut create = ChatCreate {
            context_summary: Some("They argued about a wall.".to_string()),
            scenario_text: Some(SCENARIO.to_string()),
            ..bare_create()
        };
        let before = create.context_summary.as_ref().unwrap().as_ptr();
        assert!(!strip_scenario_seeded_summary(&mut create));
        let after = create.context_summary.as_ref().unwrap().as_ptr();
        assert_eq!(before, after, "the untouched arm must not reallocate");
        assert_eq!(
            create.context_summary.as_deref(),
            Some("They argued about a wall.")
        );
        assert_eq!(create.scenario_text.as_deref(), Some(SCENARIO));
    }

    #[test]
    fn strip_nulls_the_summary_and_keeps_the_scenario_on_a_value_row() {
        let mut row = json!({
            "title": "Damp Curtains and Cold Water",
            "contextSummary": SCENARIO,
            "scenarioText": SCENARIO,
        });
        assert!(strip_scenario_seeded_summary(&mut row));
        assert_eq!(row["contextSummary"], Value::Null);
        assert_eq!(row["scenarioText"], json!(SCENARIO));
        assert_eq!(row["title"], json!("Damp Curtains and Cold Water"));
    }

    #[test]
    fn a_stripped_value_row_carries_an_explicit_null_not_an_absent_key() {
        // The `.qtap` import deserializes the stripped row into `ChatCreate`,
        // where `contextSummary` carries `#[serde(default)]` — so an absent key
        // and an explicit null both land as `None`. But the restore diffs whole
        // rows, and v4's spread writes the key. Insert it.
        let mut row = json!({ "contextSummary": SCENARIO, "scenarioText": SCENARIO });
        strip_scenario_seeded_summary(&mut row);
        assert!(
            row.as_object().unwrap().contains_key("contextSummary"),
            "v4's `{{...chat, contextSummary: null}}` writes the key"
        );
    }

    #[test]
    fn a_non_object_value_is_never_seeded_and_is_never_touched() {
        let mut row = json!("not a row");
        assert!(!is_scenario_seeded_summary(&row));
        assert!(!strip_scenario_seeded_summary(&mut row));
        assert_eq!(row, json!("not a row"));
    }

    fn bare_create() -> ChatCreate {
        serde_json::from_value(json!({
            "userId": "u1",
            "title": "t",
            "participants": [],
        }))
        .expect("the minimal create shape deserializes")
    }
}
