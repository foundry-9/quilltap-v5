//! P4.6ar/as/at **Shared contract** guard (the round's binding §1 / §2 wire
//! shapes), added at unification — the P4.6ao precedent.
//!
//! Both SPA lanes developed against route mocks and only met these verbs live
//! at unification; the folded `core-contract.ts` members were diffed
//! name-for-name against `types.rs` by hand there, and these asserts keep that
//! agreement pinned against a later rename or serde-attribute slip.
//!
//! Needs no oracle or fixture: this is a serialization contract, not a behavior
//! diff (the behavior lives in `llm_logs_routes_equivalence` and
//! `image_aesthetics_routes_equivalence`).

use quilltap_core::api::types::Request;

#[test]
fn p4_6ar_shared_contract_wire_shapes() {
    // §1 — the Inspector's exact send: `{type, chatId, includeMessages}` and
    // nothing else (v4 `useLLMLogs.ts:28`'s query string).
    assert_eq!(
        serde_json::from_str::<Request>(
            r#"{"type":"llmLogsList","chatId":"c1","includeMessages":true}"#
        )
        .expect("llmLogsList as the Inspector sends it"),
        Request::LlmLogsList {
            message_id: None,
            chat_id: Some("c1".into()),
            character_id: None,
            log_type: None,
            standalone: false,
            include_messages: true,
            limit: None,
            offset: None,
        }
    );
    // §1 — `logType` is the wire name for v4's `?type=` param (a literal `type`
    // field would collide with the envelope key), and `limit`/`offset` cross as
    // RAW STRINGS so the handler owns v4's parseInt quirks.
    assert_eq!(
        serde_json::from_str::<Request>(
            r#"{"type":"llmLogsList","logType":"CHAT_MESSAGE","limit":"garbage","offset":"2"}"#
        )
        .expect("llmLogsList with logType + raw strings"),
        Request::LlmLogsList {
            message_id: None,
            chat_id: None,
            character_id: None,
            log_type: Some("CHAT_MESSAGE".into()),
            standalone: false,
            include_messages: false,
            limit: Some("garbage".into()),
            offset: Some("2".into()),
        }
    );

    // §2 — the aesthetics pair; absent `content` decodes to None (the handler
    // defaults it to `''`, which deletes).
    assert_eq!(
        serde_json::from_str::<Request>(r#"{"type":"systemImageAestheticsGet","kind":"lantern"}"#)
            .expect("systemImageAestheticsGet"),
        Request::SystemImageAestheticsGet {
            kind: "lantern".into()
        }
    );
    assert_eq!(
        serde_json::from_str::<Request>(
            r#"{"type":"systemImageAestheticsSet","kind":"aurora","content":""}"#
        )
        .expect("systemImageAestheticsSet with empty content"),
        Request::SystemImageAestheticsSet {
            kind: "aurora".into(),
            content: Some(String::new())
        }
    );
    assert_eq!(
        serde_json::from_str::<Request>(r#"{"type":"systemImageAestheticsSet","kind":"aurora"}"#)
            .expect("systemImageAestheticsSet without content"),
        Request::SystemImageAestheticsSet {
            kind: "aurora".into(),
            content: None
        }
    );

    // The round-trip, so the SPA's own encoder meets the same names.
    let encoded = serde_json::to_value(Request::LlmLogsList {
        message_id: None,
        chat_id: Some("c1".into()),
        character_id: None,
        log_type: None,
        standalone: false,
        include_messages: true,
        limit: None,
        offset: None,
    })
    .unwrap();
    assert_eq!(encoded["type"], "llmLogsList");
    assert_eq!(encoded["chatId"], "c1");
    assert_eq!(encoded["includeMessages"], true);
}
