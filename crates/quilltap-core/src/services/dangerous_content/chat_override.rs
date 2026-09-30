//! Per-chat Concierge helpers (v4
//! `lib/services/dangerous-content/chat-override.ts`, at `4d370a90f` — the
//! phase-3 rewrite, #75) — the single source of truth for a chat's Concierge
//! posture.
//!
//! A chat is in one of three states, stored in `chats.conciergeMode`:
//!
//! ```text
//!   | State         | Text / cheap LLM / images   | Failover on refusal | Concierge may move it |
//!   | 'moderated'   | ordinary providers first    | yes                 | yes (to Unmoderated)  |
//!   | 'unmoderated' | the uncensored desk only    | n/a (already there) | n/a                   |
//!   | 'locked'      | ordinary providers only     | never               | never                 |
//! ```
//!
//! Who put the chat in its state — the operator, or the Concierge after
//! refusals or on the classifier's reading — is *provenance*
//! (`conciergeModeSetBy` / `conciergeModeReason`). It is a note on the badge
//! and in the helper text, never a separate state and never a colour.
//!
//! The legacy pair (`conciergeOverride`, `isDangerousChat`) is no longer read
//! by any routing or display decision. `isDangerousChat` and its siblings are
//! the classifier's telemetry; `conciergeOverride` is not written at all.
//! [`derive_concierge_mode_from_legacy`] maps an old row or an old bundle onto
//! the three states, and is used only where such data enters (the importer,
//! the restore — v4's migration applies the same table in SQL).
//!
//! NOTHING outside this module (and the sanctioned writer,
//! `apply_concierge_flip`) should read the stored columns. Derive everything
//! from [`get_concierge_state`], or ask one of the purpose-named questions:
//!
//!   - "Take the uncensored routes right now?" → [`should_use_uncensored_route`]
//!     (or [`concierge_state_uses_uncensored_route`], given a derived state)
//!   - "Paint danger styling in the UI?"        → [`should_show_danger_styling`]
//!   - "May the Concierge move this chat?"      → [`is_classifier_on_duty`]
//!   - "May a refusal be rerouted?"             → [`may_fail_over`]
//!     (or [`concierge_state_may_fail_over`], given a derived state)
//!
//! The port operates on a `serde_json::Value` chat row (the shape every ported
//! read yields) or a server-derived payload: v4's `ChatLike` reads the columns
//! FIRST, then the payload keys (`conciergeState` / `conciergeSetBy` /
//! `conciergeReason`), so client and server ask the same functions.

use serde_json::{json, Map, Value};

/// The canonical Concierge state of a chat (v4 `ConciergeState =
/// ConciergeMode`). The string values are also the wire contract of
/// `conciergeState` on the chat PUT and the create.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConciergeState {
    Moderated,
    Unmoderated,
    Locked,
}

/// Every state, in the order the controls list them (v4 `CONCIERGE_STATES`).
pub const CONCIERGE_STATES: [ConciergeState; 3] = [
    ConciergeState::Moderated,
    ConciergeState::Unmoderated,
    ConciergeState::Locked,
];

impl ConciergeState {
    /// The v4 wire string (`'moderated' | 'unmoderated' | 'locked'`).
    pub fn as_str(self) -> &'static str {
        match self {
            ConciergeState::Moderated => "moderated",
            ConciergeState::Unmoderated => "unmoderated",
            ConciergeState::Locked => "locked",
        }
    }

    /// The inverse of [`ConciergeState::as_str`] — v4 `ConciergeModeSchema`'s
    /// three values. Anything else is `None`, which the PUT and the create
    /// refuse with 400 — the retired four-state values (`monitored`, `flagged`,
    /// `vouched`, `uncensored`) included (v4 `4d370a90f`).
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "moderated" => Some(ConciergeState::Moderated),
            "unmoderated" => Some(ConciergeState::Unmoderated),
            "locked" => Some(ConciergeState::Locked),
            _ => None,
        }
    }
}

/// Who put the chat in its state (v4 `ConciergeModeSetBy`); the provenance
/// ([`get_concierge_provenance`]) is `None` when the chat is Moderated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConciergeSetBy {
    Operator,
    Concierge,
}

impl ConciergeSetBy {
    pub fn as_str(self) -> &'static str {
        match self {
            ConciergeSetBy::Operator => "operator",
            ConciergeSetBy::Concierge => "concierge",
        }
    }

    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "operator" => Some(ConciergeSetBy::Operator),
            "concierge" => Some(ConciergeSetBy::Concierge),
            _ => None,
        }
    }
}

/// Why the chat is in its state (v4 `ConciergeModeReason`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConciergeReason {
    Manual,
    Refusals,
    Classifier,
    Migration,
}

impl ConciergeReason {
    pub fn as_str(self) -> &'static str {
        match self {
            ConciergeReason::Manual => "manual",
            ConciergeReason::Refusals => "refusals",
            ConciergeReason::Classifier => "classifier",
            ConciergeReason::Migration => "migration",
        }
    }

    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "manual" => Some(ConciergeReason::Manual),
            "refusals" => Some(ConciergeReason::Refusals),
            "classifier" => Some(ConciergeReason::Classifier),
            "migration" => Some(ConciergeReason::Migration),
            _ => None,
        }
    }
}

/// v4's `chat?.column ?? chat?.payloadKey` — a string read, the column first.
/// JS `??` falls through on `null`/`undefined` only, so a present non-null
/// value of the wrong type stops the chain (and then fails the caller's own
/// literal test, as v4's `===` does).
fn column_or_payload<'a>(
    chat: Option<&'a Value>,
    column: &str,
    payload: &str,
) -> Option<&'a Value> {
    let chat = chat?;
    match chat.get(column) {
        Some(v) if !v.is_null() => Some(v),
        _ => chat.get(payload).filter(|v| !v.is_null()),
    }
}

/// THE canonical derivation of a chat's Concierge state (v4
/// `getConciergeState`): `mode = chat?.conciergeMode ?? chat?.conciergeState;
/// mode === 'unmoderated' || mode === 'locked' ? mode : 'moderated'`. A
/// missing, NULL or unknown value reads as Moderated; **the legacy pair is
/// never read**.
pub fn get_concierge_state(chat: Option<&Value>) -> ConciergeState {
    match column_or_payload(chat, "conciergeMode", "conciergeState").and_then(Value::as_str) {
        Some("unmoderated") => ConciergeState::Unmoderated,
        Some("locked") => ConciergeState::Locked,
        _ => ConciergeState::Moderated,
    }
}

/// Who put the chat in its current state (v4 `getConciergeProvenance`).
/// Always `None` for a Moderated chat; otherwise the stored `setBy` when it is
/// one of the two, else **`operator`** — an unknown or NULL `setBy` on a
/// non-Moderated row is never `None`.
pub fn get_concierge_provenance(chat: Option<&Value>) -> Option<ConciergeSetBy> {
    if get_concierge_state(chat) == ConciergeState::Moderated {
        return None;
    }
    match column_or_payload(chat, "conciergeModeSetBy", "conciergeSetBy").and_then(Value::as_str) {
        Some("concierge") => Some(ConciergeSetBy::Concierge),
        _ => Some(ConciergeSetBy::Operator),
    }
}

/// Why the chat is in its current state (v4 `getConciergeReason`): `None` for
/// Moderated; else `chat?.conciergeModeReason ?? chat?.conciergeReason ??
/// null`. v4 returns the stored value UNCHECKED (its type says the enum; the
/// code does not narrow), so the wire string is returned as stored.
pub fn get_concierge_reason(chat: Option<&Value>) -> Option<Value> {
    if get_concierge_state(chat) == ConciergeState::Moderated {
        return None;
    }
    column_or_payload(chat, "conciergeModeReason", "conciergeReason").cloned()
}

/// Does this state take the uncensored route? (v4
/// `conciergeStateUsesUncensoredRoute`.) THE one place that says which state
/// takes it — Unmoderated only.
pub fn concierge_state_uses_uncensored_route(state: ConciergeState) -> bool {
    state == ConciergeState::Unmoderated
}

/// Should this chat take the Concierge's uncensored routes right now (v4
/// `shouldUseUncensoredRoute`)? True only for Unmoderated, whoever set it.
pub fn should_use_uncensored_route(chat: Option<&Value>) -> bool {
    concierge_state_uses_uncensored_route(get_concierge_state(chat))
}

/// Should the UI paint this chat with danger styling (v4
/// `shouldShowDangerStyling`)? True for Unmoderated regardless of provenance:
/// the provenance goes in the tooltip and helper text, never in colour.
pub fn should_show_danger_styling(chat: Option<&Value>) -> bool {
    get_concierge_state(chat) == ConciergeState::Unmoderated
}

/// May the Concierge act on this chat of his own accord — the classifier job,
/// the scheduled scan, the per-turn trigger and the refusal ledger's
/// auto-switch (v4 `isClassifierOnDuty`)? True only for Moderated. A `None`
/// chat reads Moderated, so it is on duty.
pub fn is_classifier_on_duty(chat: Option<&Value>) -> bool {
    get_concierge_state(chat) == ConciergeState::Moderated
}

/// May a refusal in this state be rerouted to an uncensored understudy (v4
/// `conciergeStateMayFailOver`)? False only for Locked.
pub fn concierge_state_may_fail_over(state: ConciergeState) -> bool {
    state != ConciergeState::Locked
}

/// May a refusal on this chat be rerouted (v4 `mayFailOver`)? A chatless call
/// reads as Moderated, so it may.
pub fn may_fail_over(chat: Option<&Value>) -> bool {
    concierge_state_may_fail_over(get_concierge_state(chat))
}

/// The three stored columns that make up a chat's Concierge posture (v4
/// `ConciergeModeColumns`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConciergeModeColumns {
    pub concierge_mode: ConciergeState,
    pub concierge_mode_set_by: Option<ConciergeSetBy>,
    pub concierge_mode_reason: Option<ConciergeReason>,
}

impl ConciergeModeColumns {
    /// The three keys as v4 spreads them (`conciergeMode`,
    /// `conciergeModeSetBy`, `conciergeModeReason`, NULLs kept).
    pub fn to_json(self) -> Value {
        json!({
            "conciergeMode": self.concierge_mode.as_str(),
            "conciergeModeSetBy": self.concierge_mode_set_by.map(ConciergeSetBy::as_str),
            "conciergeModeReason": self.concierge_mode_reason.map(ConciergeReason::as_str),
        })
    }
}

/// Map the legacy pair onto the three states (v4
/// `deriveConciergeModeFromLegacy`), first match wins:
///
/// ```text
///   | conciergeOverride | isDangerousChat | → mode        | setBy     | reason     |
///   | 'UNCENSORED'      | any             | 'unmoderated' | operator  | migration  |
///   | 'OFF'             | any             | 'locked'      | operator  | migration  |
///   | NULL              | true            | 'unmoderated' | concierge | classifier |
///   | NULL              | else            | 'moderated'   | NULL      | NULL       |
/// ```
///
/// `isDangerousChat` is v4's `=== true` over the HYDRATED value: v5's read
/// OMITS a NULL nullable-optional, so absent == not `true`.
pub fn derive_concierge_mode_from_legacy(legacy: &Value) -> ConciergeModeColumns {
    match legacy.get("conciergeOverride").and_then(Value::as_str) {
        Some("UNCENSORED") => ConciergeModeColumns {
            concierge_mode: ConciergeState::Unmoderated,
            concierge_mode_set_by: Some(ConciergeSetBy::Operator),
            concierge_mode_reason: Some(ConciergeReason::Migration),
        },
        Some("OFF") => ConciergeModeColumns {
            concierge_mode: ConciergeState::Locked,
            concierge_mode_set_by: Some(ConciergeSetBy::Operator),
            concierge_mode_reason: Some(ConciergeReason::Migration),
        },
        _ if legacy.get("isDangerousChat") == Some(&Value::Bool(true)) => ConciergeModeColumns {
            concierge_mode: ConciergeState::Unmoderated,
            concierge_mode_set_by: Some(ConciergeSetBy::Concierge),
            concierge_mode_reason: Some(ConciergeReason::Classifier),
        },
        _ => ConciergeModeColumns {
            concierge_mode: ConciergeState::Moderated,
            concierge_mode_set_by: None,
            concierge_mode_reason: None,
        },
    }
}

/// For data entering from outside (an import bundle, a backup) — v4
/// `withConciergeModeFromLegacy`: a chat whose `conciergeMode` is `!= null`
/// is returned unchanged; otherwise (absent, or a present `null`) the three
/// columns are derived from the legacy pair and spread over it — v4's `{
/// ...chat, ...derived }`, so an existing key keeps its place and a new one
/// appends.
pub fn with_concierge_mode_from_legacy(chat: Value) -> Value {
    let Value::Object(mut obj) = chat else {
        return chat;
    };
    if obj.get("conciergeMode").is_some_and(|v| !v.is_null()) {
        return Value::Object(obj);
    }
    let derived = derive_concierge_mode_from_legacy(&Value::Object(obj.clone()));
    if let Value::Object(cols) = derived.to_json() {
        spread_into(&mut obj, cols);
    }
    Value::Object(obj)
}

/// v4 `ChatMetadataBaseSchema`'s three Concierge columns as a chat entering from
/// outside meets them (P4.124, P4.D226): `conciergeMode` /
/// `conciergeModeSetBy` / `conciergeModeReason` are `z.enum([...])
/// .nullable().optional()`, so after [`with_concierge_mode_from_legacy`] a value
/// outside its enum fails v4's `repos.chats.create` validation — the restore
/// and the import both catch that and skip the chat with a warning carrying the
/// `ZodError` message. v5's `ChatCreate` stores any string, so both sites ask
/// this first. `None` when all three pass; otherwise v4's message
/// (`JSON.stringify(issues, null, 2)`, one `invalid_value` per bad column in
/// schema order — measured with v4's real schema at `97b25fc53`, and proven
/// row by row by `repository_zod_messages_equivalence` since P4.130). The
/// schema is `ChatMetadataBaseSchema` — the one `chats.repository.ts`
/// constructs its repository with, not `ChatMetadataSchema` (P4.124's text
/// named the wrong one; the extension columns read `undefined` on a restored
/// or imported chat, so outcome and issue order are the same).
///
/// ⚠ Scope: only these three columns are checked here; any OTHER field v4's
/// schema would reject still reaches v5's typed decode, whose own error text
/// is v5's (pre-existing).
pub fn concierge_columns_zod_error(chat: &Value) -> Option<String> {
    use crate::api::zod_issues::{key, zod_error_message, ZodIssue};
    const COLUMNS: [(&str, &[&str]); 3] = [
        ("conciergeMode", &["moderated", "unmoderated", "locked"]),
        ("conciergeModeSetBy", &["operator", "concierge"]),
        (
            "conciergeModeReason",
            &["manual", "refusals", "classifier", "migration"],
        ),
    ];
    let issues: Vec<ZodIssue> = COLUMNS
        .iter()
        .filter(|(column, values)| match chat.get(*column) {
            None | Some(Value::Null) => false,
            Some(Value::String(s)) => !values.contains(&s.as_str()),
            Some(_) => true,
        })
        .map(|(column, values)| ZodIssue::invalid_value(values, vec![key(column)]))
        .collect();
    (!issues.is_empty()).then(|| zod_error_message(&issues))
}

/// The two repository ERRORs v4 logs when `repos.chats.create` refuses a chat
/// (`base.repository.ts:130-141` `validate` → `Data validation failed
/// {collection, error}`, then the rethrowing `safeQuery` around `_create` →
/// `Error creating entity {collection, error}`, `:350-378`), in that order,
/// BEFORE the caller's per-chat catch. Both restore and import log them beside
/// the refusal [`concierge_columns_zod_error`] returns (unified at the
/// `97b25fc53` follow-ups round — P4.124 item 14 ported the refusal without
/// them). `error` is the ZodError's message on both lines (`extractErrorMessage`).
pub fn log_chat_create_validation_failure(zod_message: &str) {
    tracing::error!(
        target: "quilltap::db",
        collection = "chats",
        error = %zod_message,
        "Data validation failed"
    );
    tracing::error!(
        target: "quilltap::db",
        collection = "chats",
        error = %zod_message,
        "Error creating entity"
    );
}

/// JS object spread of `src` over `dst`: an existing key is overwritten in
/// place, a new one is appended.
fn spread_into(dst: &mut Map<String, Value>, src: Map<String, Value>) {
    for (k, v) in src {
        dst.insert(k, v);
    }
}

#[cfg(test)]
mod tests {

    //! v4 `chat-override.test.ts` at `4d370a90f`, mirrored by name. The
    //! `danger_resolver_equivalence` family runs the same questions against v4's
    //! real module over a wider corpus.
    use super::*;
    use serde_json::json;

    /// P4.124 (P4.D226): the three Concierge enums against v4's REAL
    /// `ChatMetadataBaseSchema` — the messages below are v4's `error.message`
    /// recorded at the `97b25fc53` pin (a throwaway probe `safeParse`d a chat
    /// carrying each patch; run from the pin with `npx tsx`). v4's base chat
    /// fails no other issue for these patches, so the message is the three
    /// columns' alone; a valid or absent/null value passes.
    #[test]
    fn concierge_columns_fail_with_v4s_zod_message() {
        let recorded: &[(&str, &str)] = &[
            (
                r#"{"conciergeMode": "bogus"}"#,
                r#""[\n  {\n    \"code\": \"invalid_value\",\n    \"values\": [\n      \"moderated\",\n      \"unmoderated\",\n      \"locked\"\n    ],\n    \"path\": [\n      \"conciergeMode\"\n    ],\n    \"message\": \"Invalid option: expected one of \\\"moderated\\\"|\\\"unmoderated\\\"|\\\"locked\\\"\"\n  }\n]""#,
            ),
            (
                r#"{"conciergeMode": 5}"#,
                r#""[\n  {\n    \"code\": \"invalid_value\",\n    \"values\": [\n      \"moderated\",\n      \"unmoderated\",\n      \"locked\"\n    ],\n    \"path\": [\n      \"conciergeMode\"\n    ],\n    \"message\": \"Invalid option: expected one of \\\"moderated\\\"|\\\"unmoderated\\\"|\\\"locked\\\"\"\n  }\n]""#,
            ),
            (
                r#"{"conciergeModeSetBy": "nobody"}"#,
                r#""[\n  {\n    \"code\": \"invalid_value\",\n    \"values\": [\n      \"operator\",\n      \"concierge\"\n    ],\n    \"path\": [\n      \"conciergeModeSetBy\"\n    ],\n    \"message\": \"Invalid option: expected one of \\\"operator\\\"|\\\"concierge\\\"\"\n  }\n]""#,
            ),
            (
                r#"{"conciergeModeReason": "whim"}"#,
                r#""[\n  {\n    \"code\": \"invalid_value\",\n    \"values\": [\n      \"manual\",\n      \"refusals\",\n      \"classifier\",\n      \"migration\"\n    ],\n    \"path\": [\n      \"conciergeModeReason\"\n    ],\n    \"message\": \"Invalid option: expected one of \\\"manual\\\"|\\\"refusals\\\"|\\\"classifier\\\"|\\\"migration\\\"\"\n  }\n]""#,
            ),
            (
                r#"{"conciergeMode": "bogus", "conciergeModeSetBy": "nobody", "conciergeModeReason": "whim"}"#,
                r#""[\n  {\n    \"code\": \"invalid_value\",\n    \"values\": [\n      \"moderated\",\n      \"unmoderated\",\n      \"locked\"\n    ],\n    \"path\": [\n      \"conciergeMode\"\n    ],\n    \"message\": \"Invalid option: expected one of \\\"moderated\\\"|\\\"unmoderated\\\"|\\\"locked\\\"\"\n  },\n  {\n    \"code\": \"invalid_value\",\n    \"values\": [\n      \"operator\",\n      \"concierge\"\n    ],\n    \"path\": [\n      \"conciergeModeSetBy\"\n    ],\n    \"message\": \"Invalid option: expected one of \\\"operator\\\"|\\\"concierge\\\"\"\n  },\n  {\n    \"code\": \"invalid_value\",\n    \"values\": [\n      \"manual\",\n      \"refusals\",\n      \"classifier\",\n      \"migration\"\n    ],\n    \"path\": [\n      \"conciergeModeReason\"\n    ],\n    \"message\": \"Invalid option: expected one of \\\"manual\\\"|\\\"refusals\\\"|\\\"classifier\\\"|\\\"migration\\\"\"\n  }\n]""#,
            ),
        ];
        for (patch, v4_message) in recorded {
            let chat: Value = serde_json::from_str(patch).unwrap();
            let want: String = serde_json::from_str(v4_message).unwrap();
            assert_eq!(
                concierge_columns_zod_error(&chat).as_deref(),
                Some(want.as_str()),
                "{patch}"
            );
        }
        for ok in [
            serde_json::json!({}),
            serde_json::json!({"conciergeMode": null, "conciergeModeSetBy": null}),
            serde_json::json!({"conciergeMode": "locked", "conciergeModeSetBy": "operator", "conciergeModeReason": "migration"}),
        ] {
            assert_eq!(concierge_columns_zod_error(&ok), None, "{ok}");
        }
    }

    /// The two repository ERRORs precede the caller's catch, in v4's order,
    /// with the ZodError message on both — and nothing else is logged.
    #[test]
    fn a_refused_chat_create_logs_v4s_two_repository_errors_in_order() {
        let (_, lines) = crate::test_support::captured_with(|| {
            log_chat_create_validation_failure("[\n  \"posed\"\n]")
        });
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(
            lines[0]
                .starts_with("ERROR quilltap::db Data validation failed collection=chats error="),
            "{}",
            lines[0]
        );
        assert!(
            lines[1]
                .starts_with("ERROR quilltap::db Error creating entity collection=chats error="),
            "{}",
            lines[1]
        );
        assert!(lines.iter().all(|l| l.contains("posed")), "{lines:?}");
    }

    type SetByRow = Option<ConciergeSetBy>;
    /// A TABLE row.
    type Row = (
        &'static str,
        Option<&'static str>,
        SetByRow,
        bool,
        bool,
        bool,
        bool,
    );
    /// `(mode, setBy, provenance, uncensoredRoute, dangerStyling,
    /// classifierOnDuty, failOver)` — v4's TABLE, row for row.
    const TABLE: &[Row] = &[
        ("moderated", None, None, false, false, true, true),
        // A stray provenance on a Moderated row is never reported.
        (
            "moderated",
            Some("operator"),
            None,
            false,
            false,
            true,
            true,
        ),
        (
            "unmoderated",
            Some("operator"),
            Some(ConciergeSetBy::Operator),
            true,
            true,
            false,
            true,
        ),
        (
            "unmoderated",
            Some("concierge"),
            Some(ConciergeSetBy::Concierge),
            true,
            true,
            false,
            true,
        ),
        (
            "locked",
            Some("operator"),
            Some(ConciergeSetBy::Operator),
            false,
            false,
            false,
            false,
        ),
        // A locked row with no provenance still reads as the operator's.
        (
            "locked",
            None,
            Some(ConciergeSetBy::Operator),
            false,
            false,
            false,
            false,
        ),
    ];

    #[test]
    fn lists_the_three_states_in_control_order() {
        let got: Vec<&str> = CONCIERGE_STATES.iter().map(|s| s.as_str()).collect();
        assert_eq!(got, ["moderated", "unmoderated", "locked"]);
    }

    #[test]
    fn returns_moderated_for_a_null_or_undefined_chat_or_a_null_column() {
        assert_eq!(get_concierge_state(None), ConciergeState::Moderated);
        assert_eq!(
            get_concierge_state(Some(&json!({}))),
            ConciergeState::Moderated
        );
        assert_eq!(
            get_concierge_state(Some(&json!({ "conciergeMode": null }))),
            ConciergeState::Moderated
        );
    }

    #[test]
    fn ignores_the_legacy_pair_entirely() {
        let legacy = json!({ "conciergeOverride": "UNCENSORED", "isDangerousChat": true });
        assert_eq!(
            get_concierge_state(Some(&legacy)),
            ConciergeState::Moderated
        );
        assert!(!should_use_uncensored_route(Some(&legacy)));
        assert!(!should_show_danger_styling(Some(&legacy)));
    }

    #[test]
    fn reads_a_server_derived_payload_when_the_column_is_absent() {
        assert_eq!(
            get_concierge_state(Some(&json!({ "conciergeState": "locked" }))),
            ConciergeState::Locked
        );
        assert_eq!(
            get_concierge_provenance(Some(
                &json!({ "conciergeState": "unmoderated", "conciergeSetBy": "concierge" })
            )),
            Some(ConciergeSetBy::Concierge)
        );
        assert_eq!(
            get_concierge_reason(Some(
                &json!({ "conciergeState": "unmoderated", "conciergeReason": "refusals" })
            )),
            Some(json!("refusals"))
        );
    }

    #[test]
    fn prefers_the_column_over_a_derived_payload_value() {
        assert_eq!(
            get_concierge_state(Some(
                &json!({ "conciergeMode": "moderated", "conciergeState": "locked" })
            )),
            ConciergeState::Moderated
        );
    }

    #[test]
    fn the_truth_table_row_by_row() {
        for (mode, set_by, provenance, route, styling, on_duty, fail_over) in TABLE {
            let chat = json!({
                "conciergeMode": mode,
                "conciergeModeSetBy": set_by,
                "conciergeModeReason": set_by.map(|_| "manual"),
            });
            let c = Some(&chat);
            let state = ConciergeState::from_wire(mode).unwrap();
            // derives the state / the provenance
            assert_eq!(get_concierge_state(c), state, "{mode}/{set_by:?}");
            assert_eq!(
                get_concierge_provenance(c),
                *provenance,
                "{mode}/{set_by:?}"
            );
            // shouldUseUncensoredRoute and its state-only twin
            assert_eq!(should_use_uncensored_route(c), *route);
            assert_eq!(concierge_state_uses_uncensored_route(state), *route);
            // shouldShowDangerStyling (provenance never changes the colour)
            assert_eq!(should_show_danger_styling(c), *styling);
            assert_eq!(is_classifier_on_duty(c), *on_duty);
            // mayFailOver and its state-only twin
            assert_eq!(may_fail_over(c), *fail_over);
            assert_eq!(concierge_state_may_fail_over(state), *fail_over);
        }
    }

    #[test]
    fn get_concierge_reason_is_null_for_moderated_whatever_is_stored() {
        let chat = json!({ "conciergeMode": "moderated", "conciergeModeReason": "refusals" });
        assert_eq!(get_concierge_reason(Some(&chat)), None);
    }

    #[test]
    fn get_concierge_reason_returns_the_stored_reason_otherwise() {
        let chat = json!({ "conciergeMode": "unmoderated", "conciergeModeReason": "classifier" });
        assert_eq!(get_concierge_reason(Some(&chat)), Some(json!("classifier")));
    }

    #[test]
    fn may_fail_over_reads_a_chatless_call_as_moderated() {
        assert!(may_fail_over(None));
    }

    #[test]
    fn derive_concierge_mode_from_legacy_table() {
        let unmod_op = "unmoderated/operator/migration";
        let locked = "locked/operator/migration";
        let classifier = "unmoderated/concierge/classifier";
        let moderated = "moderated/null/null";
        let cases = [
            (
                json!({ "conciergeOverride": "UNCENSORED", "isDangerousChat": false }),
                unmod_op,
            ),
            (
                json!({ "conciergeOverride": "UNCENSORED", "isDangerousChat": true }),
                unmod_op,
            ),
            (
                json!({ "conciergeOverride": "OFF", "isDangerousChat": true }),
                locked,
            ),
            (
                json!({ "conciergeOverride": "OFF", "isDangerousChat": null }),
                locked,
            ),
            (
                json!({ "conciergeOverride": null, "isDangerousChat": true }),
                classifier,
            ),
            (
                json!({ "conciergeOverride": null, "isDangerousChat": false }),
                moderated,
            ),
            (
                json!({ "conciergeOverride": null, "isDangerousChat": null }),
                moderated,
            ),
            (json!({}), moderated),
        ];
        for (legacy, want) in cases {
            let c = derive_concierge_mode_from_legacy(&legacy);
            let got = format!(
                "{}/{}/{}",
                c.concierge_mode.as_str(),
                c.concierge_mode_set_by
                    .map_or("null", ConciergeSetBy::as_str),
                c.concierge_mode_reason
                    .map_or("null", ConciergeReason::as_str)
            );
            assert_eq!(got, want, "{legacy}");
        }
    }

    #[test]
    fn derives_the_state_for_a_chat_that_carries_none() {
        let out = with_concierge_mode_from_legacy(
            json!({ "id": "c", "conciergeOverride": "OFF", "isDangerousChat": false }),
        );
        assert_eq!(out["id"], "c");
        assert_eq!(out["conciergeMode"], "locked");
        assert_eq!(out["conciergeModeSetBy"], "operator");
    }

    #[test]
    fn leaves_a_chat_that_already_carries_a_state_untouched() {
        let chat =
            json!({ "id": "c", "conciergeMode": "moderated", "conciergeOverride": "UNCENSORED" });
        assert_eq!(with_concierge_mode_from_legacy(chat.clone()), chat);
    }

    #[test]
    fn wire_strings_round_trip_and_the_retired_four_do_not_decode() {
        for s in CONCIERGE_STATES {
            assert_eq!(ConciergeState::from_wire(s.as_str()), Some(s));
        }
        for s in [
            "monitored",
            "flagged",
            "vouched",
            "uncensored",
            "OFF",
            "UNCENSORED",
            "",
            "MODERATED",
        ] {
            assert_eq!(ConciergeState::from_wire(s), None, "{s} must not decode");
        }
    }
}
