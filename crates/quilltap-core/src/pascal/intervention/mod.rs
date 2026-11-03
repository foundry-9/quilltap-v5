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
/// are other lanes' files (H9).
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
