//! P4.D265 **contract C2** guard — the `01a83539d` round's new and grown
//! dispatch verbs, pinned name-for-name (the `p4_d10_wire_contract`
//! precedent). Until the union it also pinned the agreed refusal body every
//! new arm answered before its owning order filled it (R-G, §R.14); on the
//! union every arm is filled (P4.D267 / P4.D269 / P4.D271 / P4.D275), the
//! refusal fn is gone, and each arm's behaviour is pinned by its owner's
//! family, so that half retired with the unification (2026-10-12).
//!
//! The SPA lanes (P4.D276 / P4.D277) develop against these tags before they
//! meet them live, so a rename, a serde-attribute slip or a tri-state that
//! collapses (`null` read as absent) fails HERE:
//!
//! - every C2 `type` decodes to its variant with a representative body;
//! - a JSON `null` for `tuning` / `signals` / `asOf` / `caption` / `tags`
//!   decodes to `Some(None)` (PRESENT — v4's Zod refuses it) while
//!   `memoryBudget: null` decodes to `None` (v4 coerces it to absent);
//!
//! Needs no oracle or fixture: a serialization contract, not a behaviour diff
//! (each owner's family carries the behaviour).
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test p4_d265_wire_contract

use quilltap_core::api::types::{Request, WardrobeContainerScope};
use serde_json::{json, Value};

fn decode(body: Value) -> Request {
    serde_json::from_value(body.clone()).unwrap_or_else(|e| panic!("decode {body}: {e}"))
}

#[test]
fn p4_d265_contract_wire_shapes() {
    // [P4.D269] grown: memoryList gains `tier` (query-sourced, typed).
    match decode(
        json!({"type":"memoryList","characterId":"c1","tier":"cold","source":"CONSOLIDATED"}),
    ) {
        Request::MemoryList {
            character_id,
            tier,
            source,
            ..
        } => {
            assert_eq!(character_id, "c1");
            assert_eq!(tier.as_deref(), Some("cold"));
            assert_eq!(source.as_deref(), Some("CONSOLIDATED"));
        }
        other => panic!("memoryList decoded to {other:?}"),
    }
    match decode(json!({"type":"memoryList","characterId":"c1"})) {
        Request::MemoryList { tier: None, .. } => {}
        other => panic!("an absent tier must stay None: {other:?}"),
    }
    // [P4.D269] NEW anchor-gate probe.
    assert_eq!(
        decode(json!({"type":"memoryAnchorGateProbe","characterId":"c1","limit":25})),
        Request::MemoryAnchorGateProbe {
            character_id: "c1".into(),
            limit: Some(25.0)
        }
    );
    assert_eq!(
        decode(json!({"type":"memoryAnchorGateProbe","characterId":"c1"})),
        Request::MemoryAnchorGateProbe {
            character_id: "c1".into(),
            limit: None
        }
    );
    // [P4.D271] the five consolidation / extraction-mode verbs.
    assert_eq!(
        decode(json!({"type":"memoryConsolidationConfigGet"})),
        Request::MemoryConsolidationConfigGet
    );
    assert_eq!(
        decode(json!({"type":"memoryConsolidationConfigSet","config":{"enabled":true}})),
        Request::MemoryConsolidationConfigSet {
            config: json!({"enabled": true})
        }
    );
    assert_eq!(
        decode(json!({"type":"memoryExtractionModeConfigGet"})),
        Request::MemoryExtractionModeConfigGet
    );
    assert_eq!(
        decode(json!({"type":"memoryExtractionModeConfigSet","config":{"otherPass":"fold"}})),
        Request::MemoryExtractionModeConfigSet {
            config: json!({"otherPass": "fold"})
        }
    );
    assert_eq!(
        decode(json!({"type":"memoryConsolidate","options":{"characterId":"c1","dryRun":true}})),
        Request::MemoryConsolidate {
            options: json!({"characterId": "c1", "dryRun": true})
        }
    );
    // [P4.D267] grown chatRecallReplay: the tri-states.
    match decode(json!({
        "type":"chatRecallReplay","chatId":"chat1",
        "memoryBudget": null, "tuning": null, "signals": null, "asOf": null
    })) {
        Request::ChatRecallReplay {
            memory_budget,
            tuning,
            signals,
            as_of,
            ..
        } => {
            assert_eq!(
                memory_budget, None,
                "memoryBudget: null is ABSENT (v4 coerces it)"
            );
            assert_eq!(tuning, Some(None), "tuning: null is PRESENT");
            assert_eq!(signals, Some(None), "signals: null is PRESENT");
            assert_eq!(as_of, Some(None), "asOf: null is PRESENT");
        }
        other => panic!("chatRecallReplay decoded to {other:?}"),
    }
    match decode(
        json!({"type":"chatRecallReplay","chatId":"chat1","memoryBudget":3000,"asOf":true}),
    ) {
        Request::ChatRecallReplay {
            memory_budget,
            as_of,
            tuning,
            ..
        } => {
            assert_eq!(memory_budget, Some(json!(3000)));
            assert_eq!(as_of, Some(Some(json!(true))));
            assert_eq!(tuning, None);
        }
        other => panic!("chatRecallReplay decoded to {other:?}"),
    }
    // [P4.D275] the two wardrobe item-image save verbs.
    assert_eq!(
        decode(json!({
            "type":"wardrobeItemImageSaveTargets","scope":"character",
            "containerId":"ch1","itemId":"i1"
        })),
        Request::WardrobeItemImageSaveTargets {
            scope: WardrobeContainerScope::Character,
            container_id: Some("ch1".into()),
            item_id: "i1".into()
        }
    );
    match decode(json!({
        "type":"wardrobeItemImageSaveToStore","scope":"general","itemId":"i1",
        "fileId":"f1","mountPointId":"mp1","caption":null,"tags":null
    })) {
        Request::WardrobeItemImageSaveToStore {
            scope,
            container_id,
            item_id,
            file_id,
            mount_point_id,
            caption,
            tags,
        } => {
            assert_eq!(scope, WardrobeContainerScope::General);
            assert_eq!(container_id, None);
            assert_eq!(
                (item_id.as_str(), file_id.as_str(), mount_point_id.as_str()),
                ("i1", "f1", "mp1")
            );
            assert_eq!(
                caption,
                Some(None),
                "caption: null is PRESENT (v4's Zod refuses it)"
            );
            assert_eq!(tags, Some(None), "tags: null is PRESENT");
        }
        other => panic!("wardrobeItemImageSaveToStore decoded to {other:?}"),
    }
    match decode(json!({
        "type":"wardrobeItemImageSaveToStore","scope":"general","itemId":"i1",
        "fileId":"f1","mountPointId":"mp1"
    })) {
        Request::WardrobeItemImageSaveToStore {
            caption: None,
            tags: None,
            ..
        } => {}
        other => panic!("absent caption / tags must stay None: {other:?}"),
    }
}
