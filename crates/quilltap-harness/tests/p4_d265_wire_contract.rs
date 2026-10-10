//! P4.D265 **contract C2** guard — the `01a83539d` round's new and grown
//! dispatch verbs, pinned name-for-name (the `p4_d10_wire_contract`
//! precedent), plus the agreed refusal body every new arm answers until its
//! owning order fills it (R-G, §R.14).
//!
//! The SPA lanes (P4.D276 / P4.D277) develop against these tags before they
//! meet them live, so a rename, a serde-attribute slip or a tri-state that
//! collapses (`null` read as absent) fails HERE:
//!
//! - every C2 `type` decodes to its variant with a representative body;
//! - a JSON `null` for `tuning` / `signals` / `asOf` / `caption` / `tags`
//!   decodes to `Some(None)` (PRESENT — v4's Zod refuses it) while
//!   `memoryBudget: null` decodes to `None` (v4 coerces it to absent);
//! - each new arm, and each grown arm given a PRESENT new field, answers
//!   `The '<type>' action is recognized but not yet available (<order>).`
//!   (`ErrorKind::Internal`), dispatched through a booted engine.
//!
//! Needs no oracle or fixture: a serialization + refusal contract, not a
//! behaviour diff (each owner's family carries the behaviour).
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test p4_d265_wire_contract

use std::sync::Arc;

use quilltap_core::api::engine::{CoreConfig, CoreEngine, NoopAssembler};
use quilltap_core::api::types::{ErrorKind, Request, Response, WardrobeContainerScope};
use quilltap_core::api::{InstanceDirectory, QuilltapCore};
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

struct EmptyInstances;
impl InstanceDirectory for EmptyInstances {
    fn list(&self) -> Result<quilltap_core::api::types::InstancesDto, String> {
        Ok(quilltap_core::api::types::InstancesDto {
            instances: vec![],
            default_instance: None,
        })
    }
}

#[tokio::test]
async fn every_new_arm_answers_its_agreed_refusal() {
    let base = tempfile::tempdir().unwrap();
    let data = base.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    quilltap_core::services::provisioning::provision_fresh_instance(
        &data,
        "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=",
    )
    .expect("provision");
    let engine = CoreEngine::boot(
        CoreConfig {
            base_dir: base.path().to_path_buf(),
            version: "test".to_string(),
            env_pepper: Some("dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=".to_string()),
            display_zone: quilltap_core::host_zone::TimeZone::UTC,
        },
        Box::new(NoopAssembler),
        Arc::new(EmptyInstances),
    )
    .expect("boot");
    let cases: Vec<(Value, &str, &str)> = vec![
        (
            json!({"type":"memoryAnchorGateProbe","characterId":"c1"}),
            "memoryAnchorGateProbe",
            "P4.D269",
        ),
        // P4.D271 filled its five arms (`memoryConsolidationConfigGet/Set`,
        // `memoryExtractionModeConfigGet/Set`, `memoryConsolidate`); their
        // behaviour is pinned by `memory_consolidation_routes_equivalence`.
        (
            json!({"type":"wardrobeItemImageSaveTargets","scope":"general","itemId":"i1"}),
            "wardrobeItemImageSaveTargets",
            "P4.D275",
        ),
        (
            json!({"type":"wardrobeItemImageSaveToStore","scope":"general","itemId":"i1","fileId":"f","mountPointId":"m"}),
            "wardrobeItemImageSaveToStore",
            "P4.D275",
        ),
        // The two GROWN verbs refuse only when a new field is PRESENT.
        (
            json!({"type":"memoryList","characterId":"c1","tier":"hot"}),
            "memoryList",
            "P4.D269",
        ),
        (
            json!({"type":"chatRecallReplay","chatId":"chat1","tuning":null}),
            "chatRecallReplay",
            "P4.D267",
        ),
        (
            json!({"type":"chatRecallReplay","chatId":"chat1","signals":{}}),
            "chatRecallReplay",
            "P4.D267",
        ),
        (
            json!({"type":"chatRecallReplay","chatId":"chat1","asOf":false}),
            "chatRecallReplay",
            "P4.D267",
        ),
        (
            json!({"type":"chatRecallReplay","chatId":"chat1","memoryBudget":2000}),
            "chatRecallReplay",
            "P4.D267",
        ),
    ];
    for (body, action, owner) in cases {
        match engine.dispatch(decode(body.clone())).await {
            Response::Error(e) => {
                assert_eq!(e.kind, ErrorKind::Internal, "{body}");
                assert_eq!(
                    e.message,
                    format!("The '{action}' action is recognized but not yet available ({owner})."),
                    "{body}"
                );
            }
            other => panic!("{body}: expected the agreed refusal, got {other:?}"),
        }
    }
    // Absent new fields keep the EXISTING behaviour (here: the not-ready
    // refusal of a locked/unprovisioned engine — anything but the P4.D265
    // sentence).
    for body in [
        json!({"type":"memoryList","characterId":"c1"}),
        json!({"type":"chatRecallReplay","chatId":"chat1","memoryBudget":null}),
    ] {
        if let Response::Error(e) = engine.dispatch(decode(body.clone())).await {
            assert!(
                !e.message.contains("recognized but not yet available"),
                "{body}: {}",
                e.message
            );
        }
    }
}
