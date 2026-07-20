//! P4.d10 **Shared contract §A** guard — the nine chat / group / general state
//! dispatch verbs, pinned name-for-name (the `p4_6ar_wire_contract` precedent).
//!
//! Lane BE's SPA (`state.api.ts` over `core-contract.ts`) was developed against
//! these tags before ever meeting them live; the unification wire diffs the two
//! files name-for-name. These asserts pin the agreement in the meantime, so a
//! rename or serde-attribute slip fails here rather than in the SPA.
//!
//! Needs no oracle or fixture: this is a serialization contract, not a behavior
//! diff (the behavior lives in `state_routes_equivalence`).

use quilltap_core::api::types::{Request, Response};
use serde_json::json;

#[test]
fn p4_d10_state_contract_wire_shapes() {
    // §A — the chat tier.
    assert_eq!(
        serde_json::from_str::<Request>(r#"{"type":"chatStateGet","chatId":"c1"}"#)
            .expect("chatStateGet"),
        Request::ChatStateGet {
            chat_id: "c1".into()
        }
    );
    assert_eq!(
        serde_json::from_str::<Request>(
            r#"{"type":"chatStateSet","chatId":"c1","state":{"hp":10}}"#
        )
        .expect("chatStateSet"),
        Request::ChatStateSet {
            chat_id: "c1".into(),
            state: json!({ "hp": 10 }),
        }
    );
    assert_eq!(
        serde_json::from_str::<Request>(r#"{"type":"chatStateReset","chatId":"c1"}"#)
            .expect("chatStateReset"),
        Request::ChatStateReset {
            chat_id: "c1".into()
        }
    );

    // §A — the group tier (own state, no cascade).
    assert_eq!(
        serde_json::from_str::<Request>(r#"{"type":"groupStateGet","groupId":"g1"}"#)
            .expect("groupStateGet"),
        Request::GroupStateGet {
            group_id: "g1".into()
        }
    );
    assert_eq!(
        serde_json::from_str::<Request>(r#"{"type":"groupStateSet","groupId":"g1","state":{}}"#)
            .expect("groupStateSet"),
        Request::GroupStateSet {
            group_id: "g1".into(),
            state: json!({}),
        }
    );
    assert_eq!(
        serde_json::from_str::<Request>(r#"{"type":"groupStateReset","groupId":"g1"}"#)
            .expect("groupStateReset"),
        Request::GroupStateReset {
            group_id: "g1".into()
        }
    );

    // §A — the general tier (instance-wide; no id fields at all).
    assert_eq!(
        serde_json::from_str::<Request>(r#"{"type":"generalStateGet"}"#).expect("generalStateGet"),
        Request::GeneralStateGet {}
    );
    assert_eq!(
        serde_json::from_str::<Request>(r#"{"type":"generalStateSet","state":{"k":1}}"#)
            .expect("generalStateSet"),
        Request::GeneralStateSet {
            state: json!({ "k": 1 }),
        }
    );
    assert_eq!(
        serde_json::from_str::<Request>(r#"{"type":"generalStateReset"}"#)
            .expect("generalStateReset"),
        Request::GeneralStateReset {}
    );

    // The state-family response rides ONE `state` tag whose `data` is the v4
    // route body verbatim (lane BE's `dispatchData` reads only `data`).
    let resp = Response::State(json!({ "success": true, "state": {} }));
    let wire = serde_json::to_value(&resp).unwrap();
    assert_eq!(wire["type"], "state", "response type string");
    assert_eq!(wire["data"], json!({ "success": true, "state": {} }));
}
