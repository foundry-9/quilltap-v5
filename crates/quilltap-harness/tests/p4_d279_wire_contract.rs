//! P4.D279 **contract C2** guard — the `4a38222e6` round's six new dispatch
//! verbs (R-D: ONE scoped family of FIVE folder actions + `link-image`),
//! pinned name-for-name (the `p4_d265_wire_contract` precedent), and the agreed
//! refusal body every arm answers at `KEYSTONE` until P4.D281 fills it (§R.14).
//!
//! The SPA lanes (P4.D285 / P4.D286) develop against these tags before they
//! meet them live, so a rename, a serde-attribute slip or a flattened body
//! that stops riding fails HERE:
//!
//! - every C2 `type` decodes to its variant with a representative body;
//! - the folder verbs' bodies ride RAW and flattened (`path`, `name`,
//!   `itemIds`, `folder` land in `body`, never typed), so the handler can
//!   answer v4's flat `Validation error` for a wrong type;
//! - an absent `containerId` is `None` (the General tier);
//! - (the refusal half — each arm answering `The '<action>' action is
//!   recognized but not yet available (P4.D281).` at `KEYSTONE` — was RETIRED
//!   at the union: P4.D281 serves all six arms, pinned by
//!   `wardrobe_folders_routes_equivalence` and the link-image routes rows.)
//!
//! Needs no oracle or fixture: a serialization contract, not a behaviour diff
//! (P4.D281's family carries the behaviour).
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test p4_d279_wire_contract

use quilltap_core::api::types::{Request, WardrobeContainerScope};
use serde_json::{json, Value};

fn decode(body: Value) -> Request {
    serde_json::from_value(body.clone()).unwrap_or_else(|e| panic!("decode {body}: {e}"))
}

#[test]
fn p4_d279_contract_wire_shapes() {
    assert_eq!(
        decode(json!({"type":"wardrobeFoldersList","scope":"character","containerId":"ch1"})),
        Request::WardrobeFoldersList {
            scope: WardrobeContainerScope::Character,
            container_id: Some("ch1".into()),
        }
    );
    assert_eq!(
        decode(json!({"type":"wardrobeFoldersList","scope":"general"})),
        Request::WardrobeFoldersList {
            scope: WardrobeContainerScope::General,
            container_id: None,
        }
    );
    match decode(json!({
        "type":"wardrobeFolderCreate","scope":"project","containerId":"p1","path":"Formal/Evening"
    })) {
        Request::WardrobeFolderCreate {
            scope,
            container_id,
            body,
        } => {
            assert_eq!(scope, WardrobeContainerScope::Project);
            assert_eq!(container_id.as_deref(), Some("p1"));
            assert_eq!(
                body,
                json!({"path": "Formal/Evening"}),
                "the body rides RAW"
            );
        }
        other => panic!("wardrobeFolderCreate decoded to {other:?}"),
    }
    match decode(json!({
        "type":"wardrobeFolderRename","scope":"group","containerId":"g1","path":"Formal","name":5
    })) {
        Request::WardrobeFolderRename { body, .. } => {
            assert_eq!(
                body,
                json!({"path": "Formal", "name": 5}),
                "a wrong-typed `name` DECODES — the handler refuses it as v4's Zod does"
            );
        }
        other => panic!("wardrobeFolderRename decoded to {other:?}"),
    }
    match decode(
        json!({"type":"wardrobeFolderDelete","scope":"character","containerId":"ch1","path":"Hats"}),
    ) {
        Request::WardrobeFolderDelete { body, .. } => assert_eq!(body, json!({"path": "Hats"})),
        other => panic!("wardrobeFolderDelete decoded to {other:?}"),
    }
    match decode(json!({
        "type":"wardrobeItemsMove","scope":"character","containerId":"ch1",
        "itemIds":["a","b"],"folder":""
    })) {
        Request::WardrobeItemsMove { body, .. } => {
            assert_eq!(body, json!({"itemIds": ["a", "b"], "folder": ""}));
        }
        other => panic!("wardrobeItemsMove decoded to {other:?}"),
    }
    assert_eq!(
        decode(json!({
            "type":"wardrobeItemImageLink","scope":"character","containerId":"ch1",
            "itemId":"i1","fileId":"f1"
        })),
        Request::WardrobeItemImageLink {
            scope: WardrobeContainerScope::Character,
            container_id: Some("ch1".into()),
            item_id: "i1".into(),
            file_id: "f1".into(),
        }
    );
    // A missing typed field is a decode refusal (serde), as for every typed verb.
    assert!(
        serde_json::from_value::<Request>(
            json!({"type":"wardrobeItemImageLink","scope":"character","itemId":"i1"})
        )
        .is_err(),
        "fileId is required"
    );
}
