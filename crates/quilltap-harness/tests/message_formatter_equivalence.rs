//! Tier-1 differential test (chat-orchestration wave 1.6): the message-formatter
//! anti-hijack cleanups + provider name/prefix formatting. Pure functions —
//! exact equality on every field.
//!
//! P4.D157: v4 `d4138b96b` deleted `buildMultiCharacterContextSection`; the v5
//! twin's only callers were the Host roster builders v4 deleted in the same
//! commit and nothing in v5 called, so the twin went too and the `context` rows
//! left the case (118 -> 109 rows; every surviving row byte-identical).
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/message-formatter.ts \
//!     > /tmp/oracle-message-formatter.ndjson
//! Run:
//!   QT_ORACLE_MESSAGE_FORMATTER=/tmp/oracle-message-formatter.ndjson \
//!     cargo test -p quilltap-harness --test message_formatter_equivalence

use quilltap_core::message_formatter::{
    format_display_name, format_messages_for_provider, format_participant_name,
    get_provider_name_support, normalize_content_block_format, strip_character_name_prefix,
    supports_name_field, truncate_at_foreign_speaker, MessageRole, MultiCharacterMessage, WireRole,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct WireMsg {
    role: String,
    content: String,
    name: Option<String>,
    #[serde(rename = "thoughtSignature", default)]
    thought_signature: Option<String>,
}

#[derive(Deserialize)]
struct WireFormatted {
    role: String,
    content: String,
    name: Option<String>,
    #[serde(rename = "thoughtSignature")]
    thought_signature: Option<String>,
}

#[derive(Deserialize)]
struct WireTruncation {
    text: String,
    #[serde(rename = "truncatedAt")]
    truncated_at: Option<usize>,
}

#[derive(Deserialize)]
struct WireNameSupport {
    #[serde(rename = "supportsNameField")]
    supports_name_field: bool,
    #[serde(rename = "supportedRoles")]
    supported_roles: Vec<String>,
    #[serde(rename = "maxNameLength")]
    max_name_length: Option<usize>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "strip")]
    Strip {
        id: String,
        content: String,
        #[serde(rename = "characterName", default)]
        character_name: Option<String>,
        #[serde(default)]
        aliases: Option<Vec<String>>,
        out: String,
    },
    #[serde(rename = "truncate")]
    Truncate {
        id: String,
        content: String,
        #[serde(rename = "foreignNames")]
        foreign_names: Vec<String>,
        out: WireTruncation,
    },
    #[serde(rename = "normalize")]
    Normalize {
        id: String,
        content: String,
        out: String,
    },
    #[serde(rename = "participant")]
    Participant {
        id: String,
        name: String,
        #[serde(rename = "maxLength", default)]
        max_length: Option<usize>,
        out: String,
    },
    #[serde(rename = "display")]
    Display {
        id: String,
        name: String,
        out: String,
    },
    #[serde(rename = "nameSupport")]
    NameSupport {
        id: String,
        provider: String,
        out: WireNameSupport,
    },
    #[serde(rename = "supportsRole")]
    SupportsRole {
        id: String,
        provider: String,
        role: String,
        out: bool,
    },
    #[serde(rename = "format")]
    Format {
        id: String,
        provider: String,
        #[serde(rename = "respondingCharacterName")]
        responding_character_name: String,
        messages: Vec<WireMsg>,
        out: Vec<WireFormatted>,
    },
}

fn wire_role(s: &str) -> WireRole {
    match s {
        "system" => WireRole::System,
        "user" => WireRole::User,
        "assistant" => WireRole::Assistant,
        other => panic!("unexpected role {other}"),
    }
}

fn message_role(s: &str) -> MessageRole {
    match s {
        "user" => MessageRole::User,
        "assistant" => MessageRole::Assistant,
        other => panic!("unexpected message role {other}"),
    }
}

fn role_str(r: WireRole) -> &'static str {
    match r {
        WireRole::System => "system",
        WireRole::User => "user",
        WireRole::Assistant => "assistant",
    }
}

fn message_role_str(r: MessageRole) -> &'static str {
    match r {
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
    }
}

#[test]
fn message_formatter_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_MESSAGE_FORMATTER") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_MESSAGE_FORMATTER to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Row>(line).unwrap() {
            Row::Strip {
                id,
                content,
                character_name,
                aliases,
                out,
            } => {
                let got = strip_character_name_prefix(
                    &content,
                    character_name.as_deref(),
                    aliases.as_deref(),
                );
                assert_eq!(got, out, "strip '{id}'");
            }
            Row::Truncate {
                id,
                content,
                foreign_names,
                out,
            } => {
                let got = truncate_at_foreign_speaker(&content, &foreign_names);
                assert_eq!(got.text, out.text, "truncate text '{id}'");
                assert_eq!(got.truncated_at, out.truncated_at, "truncate offset '{id}'");
            }
            Row::Normalize { id, content, out } => {
                assert_eq!(
                    normalize_content_block_format(&content),
                    out,
                    "normalize '{id}'"
                );
            }
            Row::Participant {
                id,
                name,
                max_length,
                out,
            } => {
                let got = format_participant_name(&name, max_length.unwrap_or(64));
                assert_eq!(got, out, "participant '{id}'");
            }
            Row::Display { id, name, out } => {
                assert_eq!(format_display_name(&name), out, "display '{id}'");
            }
            Row::NameSupport { id, provider, out } => {
                let got = get_provider_name_support(&provider);
                assert_eq!(
                    got.supports_name_field, out.supports_name_field,
                    "nameSupport field '{id}'"
                );
                let got_roles: Vec<String> = got
                    .supported_roles
                    .iter()
                    .map(|r| message_role_str(*r).to_string())
                    .collect();
                assert_eq!(got_roles, out.supported_roles, "nameSupport roles '{id}'");
                assert_eq!(
                    got.max_name_length, out.max_name_length,
                    "nameSupport max '{id}'"
                );
            }
            Row::SupportsRole {
                id,
                provider,
                role,
                out,
            } => {
                let got = supports_name_field(&provider, message_role(&role));
                assert_eq!(got, out, "supportsRole '{id}'");
            }
            Row::Format {
                id,
                provider,
                responding_character_name,
                messages,
                out,
            } => {
                let msgs: Vec<MultiCharacterMessage> = messages
                    .into_iter()
                    .map(|m| MultiCharacterMessage {
                        role: wire_role(&m.role),
                        content: m.content,
                        id: None,
                        name: m.name,
                        participant_id: None,
                        thought_signature: m.thought_signature,
                    })
                    .collect();
                let got =
                    format_messages_for_provider(&msgs, &provider, &responding_character_name);
                assert_eq!(got.len(), out.len(), "format len '{id}'");
                for (i, (g, w)) in got.iter().zip(out.iter()).enumerate() {
                    assert_eq!(role_str(g.role), w.role, "format[{i}] role '{id}'");
                    assert_eq!(g.content, w.content, "format[{i}] content '{id}'");
                    assert_eq!(g.name, w.name, "format[{i}] name '{id}'");
                    assert_eq!(
                        g.thought_signature, w.thought_signature,
                        "format[{i}] thoughtSig '{id}'"
                    );
                }
            }
        }
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    eprintln!("OK: message-formatter matched oracle ({count} rows).");
}
