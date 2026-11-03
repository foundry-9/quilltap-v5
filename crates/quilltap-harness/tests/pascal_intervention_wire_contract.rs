//! P4.D345 **contract `InterventionWire`** guard — the four operator-
//! intervention dispatch verbs, their four `Response` variants, the realtime
//! topic and the three error sentences, pinned name-for-name so P4.D346's
//! hold dialog can author against them blind (the `p4_6ay_workbench_wire_
//! contract` precedent).
//!
//! Needs no oracle or fixture: this is a serialization contract, not a
//! behavior diff (the behavior lives in `pascal_intervention_route_
//! equivalence` and `pascal_intervention_pure_equivalence`).

use quilltap_core::api::custom_tool_interventions::{
    ALREADY_SETTLED, CONSULTS_NO_ORACLE, HELD_RUN_NOT_FOUND,
};
use quilltap_core::api::types::{Request, Response};
use quilltap_core::realtime::types::RealtimeTopic;
use serde_json::json;

#[test]
fn intervention_wire_verbs_round_trip() {
    assert_eq!(
        serde_json::from_str::<Request>(
            r#"{"type":"chatCustomToolInterventionsList","chatId":"c1"}"#
        )
        .expect("chatCustomToolInterventionsList"),
        Request::ChatCustomToolInterventionsList {
            chat_id: "c1".into()
        }
    );
    assert_eq!(
        serde_json::from_str::<Request>(
            r#"{"type":"chatCustomToolInterventionPreview","chatId":"c1","id":"h1","overrides":{"roll":2}}"#
        )
        .expect("chatCustomToolInterventionPreview"),
        Request::ChatCustomToolInterventionPreview {
            chat_id: "c1".into(),
            id: Some(json!("h1")),
            overrides: Some(json!({ "roll": 2 })),
        }
    );
    assert_eq!(
        serde_json::from_str::<Request>(
            r#"{"type":"chatCustomToolInterventionReask","chatId":"c1","id":"h1","overrides":{}}"#
        )
        .expect("chatCustomToolInterventionReask"),
        Request::ChatCustomToolInterventionReask {
            chat_id: "c1".into(),
            id: Some(json!("h1")),
            overrides: Some(json!({})),
        }
    );
    assert_eq!(
        serde_json::from_str::<Request>(
            r#"{"type":"chatCustomToolInterventionResolve","chatId":"c1","id":"h1","decision":{"kind":"veto"}}"#
        )
        .expect("chatCustomToolInterventionResolve"),
        Request::ChatCustomToolInterventionResolve {
            chat_id: "c1".into(),
            id: Some(json!("h1")),
            decision: Some(json!({ "kind": "veto" })),
        }
    );
}

/// A missing `overrides` / `decision` is NOT a serde refusal — it reaches the
/// core's Zod twin, which answers v4's `received undefined` sentence; a
/// present `null` stays distinguishable (`received null`).
#[test]
fn absent_and_null_bodies_reach_the_zod_twin() {
    assert_eq!(
        serde_json::from_str::<Request>(
            r#"{"type":"chatCustomToolInterventionPreview","chatId":"c1"}"#
        )
        .expect("absent keys deserialize"),
        Request::ChatCustomToolInterventionPreview {
            chat_id: "c1".into(),
            id: None,
            overrides: None,
        }
    );
    assert_eq!(
        serde_json::from_str::<Request>(
            r#"{"type":"chatCustomToolInterventionResolve","chatId":"c1","id":null,"decision":null}"#
        )
        .expect("null keys deserialize"),
        Request::ChatCustomToolInterventionResolve {
            chat_id: "c1".into(),
            id: Some(serde_json::Value::Null),
            decision: Some(serde_json::Value::Null),
        }
    );
}

#[test]
fn intervention_wire_responses_topic_and_sentences() {
    // The four Response variants exist under these names (a rename fails to
    // compile here, before the SPA ever sees it).
    for r in [
        Response::CustomToolInterventions(json!({ "holds": [] })),
        Response::CustomToolInterventionPreview(json!({ "preview": {} })),
        Response::CustomToolInterventionReask(json!({ "preview": {}, "consults": [] })),
        Response::CustomToolInterventionResolve(json!({ "resolved": "veto" })),
    ] {
        assert!(!matches!(r, Response::Error(_)));
    }
    assert_eq!(
        RealtimeTopic::CustomToolInterventions.as_str(),
        "customToolInterventions"
    );
    assert_eq!(HELD_RUN_NOT_FOUND, "Held run not found");
    assert_eq!(ALREADY_SETTLED, "That run has already been settled");
    assert_eq!(CONSULTS_NO_ORACLE, "This tool consults no oracle");
}
