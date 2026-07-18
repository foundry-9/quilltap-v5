//! P4.6ay unit 12 ∥ P4.6bb **Shared contract §W1** guard — the four Workbench
//! dispatch verbs, pinned name-for-name (the `p4_6ar_wire_contract` precedent).
//!
//! Lane BB's Workbench SPA develops against these verbs before ever meeting
//! them live; the §W wire diff runs at unification against its
//! `core-contract.ts`. These asserts pin the agreement in the meantime, so a
//! later rename or serde-attribute slip fails here rather than in the SPA.
//!
//! Needs no oracle or fixture: this is a serialization contract, not a behavior
//! diff (the behavior lives in `pascal_workbench_route_equivalence`).

use quilltap_core::api::types::{Request, Response};
use serde_json::json;

#[test]
fn p4_6ay_workbench_contract_wire_shapes() {
    // §W1 — the two GET verbs carry no fields at all.
    assert_eq!(
        serde_json::from_str::<Request>(r#"{"type":"customToolsLibrary"}"#)
            .expect("customToolsLibrary"),
        Request::CustomToolsLibrary
    );
    assert_eq!(
        serde_json::from_str::<Request>(r#"{"type":"customToolsDestinations"}"#)
            .expect("customToolsDestinations"),
        Request::CustomToolsDestinations
    );

    // §W1 — preview: `definition` required, everything else optional. This is
    // the minimal send the bench makes on a first keystroke.
    assert_eq!(
        serde_json::from_str::<Request>(
            r#"{"type":"customToolPreview","definition":{"name":"x"}}"#
        )
        .expect("customToolPreview, bare"),
        Request::CustomToolPreview {
            definition: json!({ "name": "x" }),
            params: None,
            private: None,
            metadata: None,
            llm: None,
        }
    );

    // §W1 — preview, fully dressed. `metadata` crosses as an opaque Value
    // because it is v4's UNION: a plain object passes through verbatim, a
    // `{characterId}` object hydrates server-side. The SPA sends either form
    // and never resolves the sheet itself.
    assert_eq!(
        serde_json::from_str::<Request>(
            r#"{"type":"customToolPreview","definition":{"name":"x"},"params":{"bonus":2},"private":true,"metadata":{"characterId":"c1"}}"#
        )
        .expect("customToolPreview, dressed"),
        Request::CustomToolPreview {
            definition: json!({ "name": "x" }),
            params: Some(json!({ "bonus": 2 })),
            private: Some(true),
            metadata: Some(json!({ "characterId": "c1" })),
            llm: None,
        }
    );

    // §W1 — audit takes no `private` and no `runs`: the draw count is
    // server-fixed at `AUDIT_RUNS` and never crosses the wire.
    assert_eq!(
        serde_json::from_str::<Request>(
            r#"{"type":"customToolAudit","definition":{"name":"x"},"metadata":{"str":3}}"#
        )
        .expect("customToolAudit"),
        Request::CustomToolAudit {
            definition: json!({ "name": "x" }),
            params: None,
            metadata: Some(json!({ "str": 3 })),
            llm: None,
        }
    );

    // §B (P4.d8) — the bench oracle rides both bodies as an opaque Value; the
    // core owns the union check, so the wire simply carries the object through.
    // The preview union admits `{live:true}`; the audit union deliberately does
    // NOT, and that is enforced in the core rather than here (a `live` audit
    // body deserializes fine and is then refused with a 400 — the shape rule is
    // v4's, and v4 enforces it at its own schema).
    for (wire, want) in [
        (r#"{"live":true}"#, json!({ "live": true })),
        (r#"{"output":"YES"}"#, json!({ "output": "YES" })),
        (r#"{"fail":true}"#, json!({ "fail": true })),
    ] {
        assert_eq!(
            serde_json::from_str::<Request>(&format!(
                r#"{{"type":"customToolPreview","definition":{{"name":"x"}},"llm":{wire}}}"#
            ))
            .expect("customToolPreview with an llm oracle"),
            Request::CustomToolPreview {
                definition: json!({ "name": "x" }),
                params: None,
                private: None,
                metadata: None,
                llm: Some(want.clone()),
            }
        );
    }
    assert_eq!(
        serde_json::from_str::<Request>(
            r#"{"type":"customToolAudit","definition":{"name":"x"},"llm":{"output":"YES"}}"#
        )
        .expect("customToolAudit with a scripted oracle"),
        Request::CustomToolAudit {
            definition: json!({ "name": "x" }),
            params: None,
            metadata: None,
            llm: Some(json!({ "output": "YES" })),
        }
    );

    // §W1 — the four response `type` strings, which lane BB switches on.
    for (resp, want) in [
        (
            Response::CustomToolsLibrary(json!({ "tools": [], "errors": [] })),
            "customToolsLibrary",
        ),
        (
            Response::CustomToolsDestinations(json!({ "general": null })),
            "customToolsDestinations",
        ),
        (Response::CustomToolPreview(json!({})), "customToolPreview"),
        (Response::CustomToolAudit(json!({})), "customToolAudit"),
    ] {
        let wire = serde_json::to_value(&resp).unwrap();
        assert_eq!(wire["type"], want, "response type string");
        assert!(wire.get("data").is_some(), "{want} carries a data payload");
    }
}
