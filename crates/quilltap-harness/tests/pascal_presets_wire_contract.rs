//! P4.D379 **contract C11 (PRESETS)** guard — the `chatCustomToolPresets`
//! dispatch verb, its `customToolPresets` response, the three 400 sentences and
//! the 404, and the pool body's key order, pinned name-for-name so P4.D388's run
//! dialog can author against them blind (the `pascal_intervention_wire_contract`
//! precedent).
//!
//! Needs no oracle or fixture: this is a serialization contract, not a behavior
//! diff (the behavior lives in `pascal_custom_tools_route_equivalence` and
//! `pascal_tool_preset_pool_equivalence`).

use quilltap_core::api::custom_tools::{
    presets_unknown_character, PRESETS_CHAT_NOT_FOUND, PRESETS_INVALID_TOOL, PRESETS_NO_CHARACTER,
    PRESETS_UNKNOWN_CHARACTER_PREFIX, PRESETS_UNKNOWN_CHARACTER_SUFFIX,
};
use quilltap_core::api::types::{Request, Response};
use quilltap_core::db::tiered_mount_pool::MountTier;
use quilltap_core::pascal::tool_preset_pool::{
    ToolPreset, ToolPresetPool, ToolPresetStore, UnreadPresetStore,
};
use serde_json::json;

#[test]
fn presets_verb_round_trips_with_and_without_its_optional_keys() {
    assert_eq!(
        serde_json::from_str::<Request>(
            r#"{"type":"chatCustomToolPresets","chatId":"c1","tool":"coin","asCharacterId":"ch1"}"#
        )
        .expect("chatCustomToolPresets (whole)"),
        Request::ChatCustomToolPresets {
            chat_id: "c1".into(),
            tool: Some("coin".into()),
            as_character_id: Some("ch1".into()),
        }
    );
    assert_eq!(
        serde_json::from_str::<Request>(r#"{"type":"chatCustomToolPresets","chatId":"c1"}"#)
            .expect("chatCustomToolPresets (bare)"),
        Request::ChatCustomToolPresets {
            chat_id: "c1".into(),
            tool: None,
            as_character_id: None,
        }
    );
    // An empty `asCharacterId` survives the wire as `Some("")`; the core treats
    // it exactly like an absent one (v4's truthiness test).
    assert_eq!(
        serde_json::from_str::<Request>(
            r#"{"type":"chatCustomToolPresets","chatId":"c1","asCharacterId":""}"#
        )
        .expect("chatCustomToolPresets (empty as)"),
        Request::ChatCustomToolPresets {
            chat_id: "c1".into(),
            tool: None,
            as_character_id: Some(String::new()),
        }
    );
}

#[test]
fn presets_response_tags_as_custom_tool_presets() {
    let r = Response::CustomToolPresets(json!({ "presets": [], "stores": [], "unread": [] }));
    let v = serde_json::to_value(&r).unwrap();
    assert_eq!(v["type"], "customToolPresets");
    assert_eq!(
        v["data"],
        json!({ "presets": [], "stores": [], "unread": [] })
    );
}

#[test]
fn presets_sentences_are_v4s() {
    assert_eq!(PRESETS_INVALID_TOOL, "A valid custom tool name is required");
    assert_eq!(
        PRESETS_NO_CHARACTER,
        "This chat has no character whose presets could be read"
    );
    assert_eq!(
        PRESETS_UNKNOWN_CHARACTER_PREFIX,
        "No character participant with id "
    );
    assert_eq!(PRESETS_UNKNOWN_CHARACTER_SUFFIX, " is in this chat");
    assert_eq!(
        presets_unknown_character("ch9"),
        "No character participant with id ch9 is in this chat"
    );
    assert_eq!(PRESETS_CHAT_NOT_FOUND, "Chat not found");
}

/// C11's body, key order included (compared as BYTES — `Value ==` is
/// order-blind under `preserve_order`).
#[test]
fn a_pool_renders_c11s_key_order() {
    let near = ToolPresetStore {
        tier: MountTier::Character,
        mount_point_id: "vault".into(),
        mount_name: "Vault".into(),
    };
    let far = ToolPresetStore {
        tier: MountTier::Global,
        mount_point_id: "general".into(),
        mount_name: "Quilltap General".into(),
    };
    let pool = ToolPresetPool {
        presets: vec![ToolPreset {
            store: near.clone(),
            name: "house".into(),
            path: "Tools/coin.house.settings.json".into(),
            shadows: vec![far.clone()],
        }],
        stores: vec![near, far.clone()],
        unread: vec![UnreadPresetStore {
            store: far,
            reason: "EACCES".into(),
        }],
    };
    assert_eq!(
        serde_json::to_string(&pool.to_value()).unwrap(),
        concat!(
            r#"{"presets":[{"tier":"character","mountPointId":"vault","mountName":"Vault","#,
            r#""name":"house","path":"Tools/coin.house.settings.json","#,
            r#""shadows":[{"tier":"global","mountPointId":"general","mountName":"Quilltap General"}]}],"#,
            r#""stores":[{"tier":"character","mountPointId":"vault","mountName":"Vault"},"#,
            r#"{"tier":"global","mountPointId":"general","mountName":"Quilltap General"}],"#,
            r#""unread":[{"tier":"global","mountPointId":"general","mountName":"Quilltap General","reason":"EACCES"}]}"#,
        )
    );
}
