//! Terminal WebSocket protocol types — v4 `lib/schemas/terminal.types.ts`
//! (`WsClientMessageSchema` / `WsServerMessageSchema` / `PtySessionMetaSchema`),
//! field names byte-verbatim.
//!
//! P4.2's WebSocket route (`/api/v1/terminals/<id>/stream`) marshals these; the
//! manager in this crate produces/consumes them. Serialization matches v4's
//! `JSON.stringify` of the object literals: the `type` tag first, then the
//! fields in v4's insertion order; nullable meta fields serialize as explicit
//! `null` (v4's meta literal carries `label: opts.label ?? null` etc.).
//!
//! Dispatch semantics live in the route (v4 `lib/terminal/ws.ts` does a bare
//! `JSON.parse` + a `type` check — it does NOT run the Zod schema — so a
//! malformed message is swallowed by the route's catch; a failed serde parse
//! plays the same role in P4.2). `cols`/`rows` are `u16` (the PTY winsize
//! domain); v4's Zod `int().positive()` never sees values past that in
//! practice, and an out-of-range JSON number fails the parse → the message is
//! dropped like any other malformed frame.

use serde::{Deserialize, Serialize};

/// Wire-level session metadata sent over the WebSocket and returned by REST
/// endpoints (v4 `PtySessionMetaSchema` — the `terminal_sessions` row minus the
/// repository-managed `createdAt`/`updatedAt`; the session lifecycle is captured
/// by `startedAt`/`exitedAt` instead).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PtySessionMeta {
    pub id: String,
    #[serde(rename = "chatId")]
    pub chat_id: String,
    #[serde(default)]
    pub label: Option<String>,
    pub shell: String,
    pub cwd: String,
    #[serde(rename = "startedAt")]
    pub started_at: String,
    #[serde(rename = "exitedAt", default)]
    pub exited_at: Option<String>,
    #[serde(rename = "exitCode", default)]
    pub exit_code: Option<i64>,
    #[serde(rename = "transcriptPath", default)]
    pub transcript_path: Option<String>,
}

/// Client-to-server WebSocket message (v4 `WsClientMessageSchema`,
/// discriminated on `type`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsClientMessage {
    #[serde(rename = "input")]
    Input { data: String },
    #[serde(rename = "resize")]
    Resize { cols: u16, rows: u16 },
    #[serde(rename = "ping")]
    Ping,
}

/// The `chat-update` reason enum (v4 `z.enum(['ariel-terminal-output'])`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatUpdateReason {
    #[serde(rename = "ariel-terminal-output")]
    ArielTerminalOutput,
}

/// Server-to-client WebSocket message (v4 `WsServerMessageSchema`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsServerMessage {
    #[serde(rename = "output")]
    Output { data: String },
    #[serde(rename = "exit")]
    Exit {
        code: Option<i64>,
        signal: Option<String>,
    },
    #[serde(rename = "meta")]
    Meta { meta: PtySessionMeta },
    #[serde(rename = "pong")]
    Pong,
    #[serde(rename = "chat-update")]
    ChatUpdate {
        #[serde(rename = "chatId")]
        chat_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<ChatUpdateReason>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // Round-trips against literal v4 JSON (the exact strings v4's
    // JSON.stringify / JSON.parse produce and consume).
    // ------------------------------------------------------------------

    #[test]
    fn client_input_round_trip() {
        let v4 = r#"{"type":"input","data":"ls -la\r"}"#;
        let msg: WsClientMessage = serde_json::from_str(v4).unwrap();
        assert_eq!(
            msg,
            WsClientMessage::Input {
                data: "ls -la\r".into()
            }
        );
        assert_eq!(serde_json::to_string(&msg).unwrap(), v4);
    }

    #[test]
    fn client_resize_round_trip() {
        let v4 = r#"{"type":"resize","cols":120,"rows":40}"#;
        let msg: WsClientMessage = serde_json::from_str(v4).unwrap();
        assert_eq!(
            msg,
            WsClientMessage::Resize {
                cols: 120,
                rows: 40
            }
        );
        assert_eq!(serde_json::to_string(&msg).unwrap(), v4);
    }

    #[test]
    fn client_ping_round_trip() {
        let v4 = r#"{"type":"ping"}"#;
        let msg: WsClientMessage = serde_json::from_str(v4).unwrap();
        assert_eq!(msg, WsClientMessage::Ping);
        assert_eq!(serde_json::to_string(&msg).unwrap(), v4);
    }

    #[test]
    fn server_output_round_trip() {
        // v4's JSON.stringify escapes the ESC control char as \u001b — serde
        // emits the same escape, so the wire bytes match byte-for-byte.
        let v4 = "{\"type\":\"output\",\"data\":\"hello\\u001b[0m\"}";
        let msg: WsServerMessage = serde_json::from_str(v4).unwrap();
        assert_eq!(
            msg,
            WsServerMessage::Output {
                data: "hello\u{1b}[0m".into()
            }
        );
        assert_eq!(serde_json::to_string(&msg).unwrap(), v4);
    }

    #[test]
    fn server_exit_round_trip() {
        // v4: `{ type: 'exit', code: exitCode, signal: signal ? String(signal) : null }`.
        let v4 = r#"{"type":"exit","code":0,"signal":null}"#;
        let msg: WsServerMessage = serde_json::from_str(v4).unwrap();
        assert_eq!(
            msg,
            WsServerMessage::Exit {
                code: Some(0),
                signal: None
            }
        );
        assert_eq!(serde_json::to_string(&msg).unwrap(), v4);

        // The ws.ts session_not_found frame.
        let v4_nf = r#"{"type":"exit","code":-1,"signal":"session_not_found"}"#;
        let msg: WsServerMessage = serde_json::from_str(v4_nf).unwrap();
        assert_eq!(
            msg,
            WsServerMessage::Exit {
                code: Some(-1),
                signal: Some("session_not_found".into())
            }
        );
        assert_eq!(serde_json::to_string(&msg).unwrap(), v4_nf);
    }

    #[test]
    fn server_meta_round_trip() {
        // v4's meta literal, keys in construction order, nullables explicit.
        let v4 = r#"{"type":"meta","meta":{"id":"6a1e8e0c-6a5f-4d2a-9d5b-2f6f1a3b4c5d","chatId":"c0000001-0000-4000-8000-0000000000c1","label":null,"shell":"/bin/bash","cwd":"/tmp/files","startedAt":"2026-07-08T12:00:00.000Z","exitedAt":null,"exitCode":null,"transcriptPath":"/tmp/logs/terminals/6a1e8e0c.log"}}"#;
        let msg: WsServerMessage = serde_json::from_str(v4).unwrap();
        match &msg {
            WsServerMessage::Meta { meta } => {
                assert_eq!(meta.label, None);
                assert_eq!(meta.shell, "/bin/bash");
                assert_eq!(meta.exited_at, None);
            }
            other => panic!("wrong variant: {other:?}"),
        }
        assert_eq!(serde_json::to_string(&msg).unwrap(), v4);
    }

    #[test]
    fn server_pong_round_trip() {
        let v4 = r#"{"type":"pong"}"#;
        let msg: WsServerMessage = serde_json::from_str(v4).unwrap();
        assert_eq!(msg, WsServerMessage::Pong);
        assert_eq!(serde_json::to_string(&msg).unwrap(), v4);
    }

    #[test]
    fn server_chat_update_round_trip() {
        let v4 = r#"{"type":"chat-update","chatId":"c0000001-0000-4000-8000-0000000000c1","reason":"ariel-terminal-output"}"#;
        let msg: WsServerMessage = serde_json::from_str(v4).unwrap();
        assert_eq!(
            msg,
            WsServerMessage::ChatUpdate {
                chat_id: "c0000001-0000-4000-8000-0000000000c1".into(),
                reason: Some(ChatUpdateReason::ArielTerminalOutput),
            }
        );
        assert_eq!(serde_json::to_string(&msg).unwrap(), v4);

        // The Zod schema marks `reason` optional; an absent key parses and the
        // serialization omits it (skip_serializing_if).
        let no_reason = r#"{"type":"chat-update","chatId":"c1"}"#;
        let msg: WsServerMessage = serde_json::from_str(no_reason).unwrap();
        assert_eq!(serde_json::to_string(&msg).unwrap(), no_reason);
    }

    #[test]
    fn malformed_client_messages_fail_parse() {
        // Unknown type / wrong field types fail the parse (the route drops them,
        // v4's catch-around-dispatch shape).
        assert!(serde_json::from_str::<WsClientMessage>(r#"{"type":"nope"}"#).is_err());
        assert!(serde_json::from_str::<WsClientMessage>(r#"{"type":"input"}"#).is_err());
        assert!(serde_json::from_str::<WsClientMessage>(
            r#"{"type":"resize","cols":1.5,"rows":2}"#
        )
        .is_err());
    }
}
