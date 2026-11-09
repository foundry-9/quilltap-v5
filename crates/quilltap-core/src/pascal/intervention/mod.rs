//! Operator intervention — Pascal the Croupier holds a run for the operator
//! (v4 `839575d89` / `8ae8a127c` / `b02d902f3` at the `236c04e35` pin; P4.D345).
//!
//! An intervenable tool's run, reached by a model's `run_custom` or a trigger
//! phrase, is DEALT and then HELD between the deal and its effects: nothing is
//! applied, nothing posted, no bytes drawn again, no model called — until the
//! operator previews, re-asks, deals, lets it stand or sweeps it away. The
//! module split mirrors v4's `lib/pascal/intervention/` (ruling R-E): each file
//! is a unit the pure family names.
//!
//! Design of record (v4): `docs/developer/features/complete/
//! pascal-operator-intervention.md`.

pub mod eligibility;
pub mod overrides;
pub mod recompute;
pub mod registry;
pub mod spans;
pub mod veto_result;
pub mod view;

/// v4 `intervention-leaks.test.ts:110-129` over THIS lane's files: an
/// intervenable definition's model-facing projections carry no word of the
/// hold — the key, the flag or the word, in any case (spec I13). The
/// storyteller roster, the visible-conversation readers and the Courier delta
/// were other lanes' files (H9): the union rows are [`union_leak_tests`] below
/// and `courier_transport`'s `a_held_and_adjusted_pascal_row_reaches_the_delta_final_only`.
#[cfg(test)]
mod leak_tests {
    use crate::db::tiered_mount_pool::MountTier;
    use crate::pascal::custom_tool_types::safe_parse;
    use crate::pascal::roster::DiscoveredCustomTool;
    use crate::pascal::tool_vocabulary::collect_tool_vocabulary;
    use crate::tools::run_custom::build_run_custom_description;
    use serde_json::json;

    fn intervenable(reveal_odds: Option<bool>) -> DiscoveredCustomTool {
        let mut doc = json!({
            "name": "wager",
            "description": "A modest wager.",
            "roll": {"min": 0, "max": 100},
            "parameters": {"bonus": {"type": "number", "default": 0, "intervene": false}},
            "effects": [{"target": "state.wagers", "value": 1, "intervene": false}],
            "outcomes": [
                {"when": {"gte": 50}, "message": "Paid at {{value}}.", "state": "success"},
                {"when": true, "message": "Kept at {{value}}.", "state": "failure"}
            ],
            "intervention": {"enabled": true, "fields": {"roll": false}},
        });
        if let Some(r) = reveal_odds {
            doc["revealOdds"] = json!(r);
        }
        DiscoveredCustomTool {
            definition: safe_parse(&doc).expect("loads"),
            tier: MountTier::Global,
            mount_point_id: "m".into(),
            mount_name: "Store".into(),
            definition_path: "Tools/wager.tool.json".into(),
        }
    }

    fn no_intervention_talk(text: &str) {
        let lower = text.to_lowercase();
        assert!(!lower.contains("intervention"), "leak: {text}");
        assert!(!lower.contains("intervene"), "leak: {text}");
    }

    #[test]
    fn the_fixture_really_is_intervenable() {
        let tool = intervenable(None);
        assert!(super::eligibility::is_intervenable(&tool.definition));
        assert!(serde_json::to_string(&tool.definition)
            .unwrap()
            .contains("intervene"));
    }

    #[test]
    fn run_custom_says_nothing_of_the_hold_odds_revealed_or_hidden() {
        for odds in [None, Some(false)] {
            let text = build_run_custom_description(&[intervenable(odds)]);
            assert!(text.contains("wager"));
            no_intervention_talk(&text);
        }
    }

    #[test]
    fn the_tool_vocabulary_carries_no_intervention_keys() {
        no_intervention_talk(&format!(
            "{:?}",
            collect_tool_vocabulary(&intervenable(None).definition)
        ));
    }
}

/// P4.D345 H9 — the union rows of v4 `intervention-leaks.test.ts` (`:135-146`,
/// `:225-244`): the storyteller bid roster names an intervenable tool without a
/// word of the operator's controls, and the two transcript readers a model sees
/// (`extract_visible_conversation` — every cheap-LLM pass — and
/// `read_conversation`'s renderer) carry a held-and-adjusted Pascal message's
/// FINAL content only. v5's input rows for both readers carry no
/// `pascalMeta` at all (the type is the guard); these pin that the final
/// content is what passes and no dealt word does.
#[cfg(test)]
mod union_leak_tests {
    use crate::storyteller::bid_call::build_bid_offer;
    use crate::storyteller::prompts::build_bid_section;
    use crate::tools::effects::ToolEffect;
    use serde_json::json;
    use std::collections::BTreeMap;

    const FINAL_CONTENT: &str = "The house pays out at seventy.";

    fn no_record(text: &str) {
        let lower = text.to_lowercase();
        for word in ["intervention", "intervene", "adjusted", "dealt-secret"] {
            assert!(!lower.contains(word), "leak of {word}: {text}");
        }
    }

    #[test]
    fn the_storyteller_bid_roster_names_the_tool_without_the_operators_controls() {
        let custom = BTreeMap::from([("wager".to_string(), ToolEffect::Write)]);
        let offer = build_bid_offer(
            &[json!({"function": {"name": "run_custom"}})],
            &custom,
            &BTreeMap::new(),
            false,
        );
        assert_eq!(
            offer.custom_tools,
            vec![("wager".to_string(), ToolEffect::Write)]
        );
        no_record(&format!("{offer:?}"));
        let section = build_bid_section("Ada", 0.5, &offer);
        assert!(section.contains("run_custom:<name>"), "{section}");
        assert!(section.contains("wager"), "{section}");
        no_record(&section);
    }

    #[test]
    fn extract_visible_conversation_hands_on_the_final_content_only() {
        use crate::chat_tasks::{extract_visible_conversation, RawMessage};
        let rows = [
            RawMessage {
                type_: Some("message".into()),
                role: Some("USER".into()),
                content: Some("Roll for it.".into()),
                ..Default::default()
            },
            RawMessage {
                type_: Some("message".into()),
                role: Some("ASSISTANT".into()),
                content: Some(FINAL_CONTENT.into()),
                system_kind: Some("custom-tool-result".into()),
                system_sender: Some("pascal".into()),
                ..Default::default()
            },
        ];
        let text = format!("{:?}", extract_visible_conversation(&rows));
        assert!(text.contains(FINAL_CONTENT), "{text}");
        no_record(&text);
    }

    #[test]
    fn the_read_conversation_renderer_shows_the_final_content_only() {
        use crate::services::conversation_markdown::{
            render_conversation_markdown, ConversationMetadata, RenderEvent,
        };
        let events = [
            RenderEvent {
                id: "u1".into(),
                type_: Some("message".into()),
                role: Some("USER".into()),
                content: Some("Roll for it.".into()),
                participant_id: None,
                created_at: "2026-05-01T00:00:00.000Z".into(),
            },
            RenderEvent {
                id: "msg-pascal".into(),
                type_: Some("message".into()),
                role: Some("ASSISTANT".into()),
                content: Some(FINAL_CONTENT.into()),
                participant_id: None,
                created_at: "2026-05-02T00:00:00.000Z".into(),
            },
        ];
        let rendered = render_conversation_markdown(
            &events,
            &[],
            Some(&ConversationMetadata {
                conversation_id: "chat-1".into(),
                title: "T".into(),
                created_at: "2026-05-01T00:00:00.000Z".into(),
                last_updated_at: "2026-05-02T00:00:00.000Z".into(),
            }),
            "2026-05-03T00:00:00.000Z",
            &jiff::tz::TimeZone::UTC,
        );
        assert!(
            rendered.markdown.contains(FINAL_CONTENT),
            "{}",
            rendered.markdown
        );
        no_record(&format!("{rendered:?}"));
    }
}
