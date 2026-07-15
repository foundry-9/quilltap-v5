//! P4.6ao **Shared contract** guard (the round's binding §1 / §2 wire shapes).
//!
//! Lane B (the Salon/settings SPA) develops against a route mock and only meets
//! these verbs live at unification, so a silent rename or a serde-attribute slip
//! on this side would surface there rather than here. These asserts pin the exact
//! JSON the order specifies — cheap, and they fail in the lane that owns the
//! contract.
//!
//! Needs no oracle or fixture: this is a serialization contract, not a behavior
//! diff (the behavior lives in `cost_background_routes_equivalence`).

use quilltap_core::api::types::Request;

#[test]
fn p4_6ao_shared_contract_wire_shapes() {
    // §1 — `{"type": "chatGetCost", "chatId": "<uuid>", "detailed": <bool, optional>}`.
    assert_eq!(
        serde_json::from_str::<Request>(r#"{"type":"chatGetCost","chatId":"c1","detailed":true}"#)
            .expect("chatGetCost with detailed"),
        Request::ChatGetCost {
            chat_id: "c1".into(),
            detailed: Some(true)
        }
    );
    // "absent means false" — decodes to None; the dispatch arm unwraps it to false.
    assert_eq!(
        serde_json::from_str::<Request>(r#"{"type":"chatGetCost","chatId":"c1"}"#)
            .expect("chatGetCost without detailed"),
        Request::ChatGetCost {
            chat_id: "c1".into(),
            detailed: None
        }
    );

    // §2 — the variant and wire name DO NOT change (only the dispatch target did).
    assert_eq!(
        serde_json::from_str::<Request>(r#"{"type":"chatRegenerateBackground","chatId":"c1"}"#)
            .expect("chatRegenerateBackground"),
        Request::ChatRegenerateBackground {
            chat_id: "c1".into()
        }
    );

    // The round-trip too, so the SPA's own encoder meets the same names.
    let encoded = serde_json::to_value(Request::ChatGetCost {
        chat_id: "c1".into(),
        detailed: Some(false),
    })
    .unwrap();
    assert_eq!(encoded["type"], "chatGetCost");
    assert_eq!(encoded["chatId"], "c1");
    assert_eq!(encoded["detailed"], false);
}
