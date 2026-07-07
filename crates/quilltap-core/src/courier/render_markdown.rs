//! The Courier — render an assembled LLM request as a single Markdown blob (port
//! of v4 `lib/llm/courier/render-markdown.ts`).
//!
//! Used by the manual / clipboard transport. The orchestrator builds the same
//! provider-shaped messages it would send to an API provider, then hands them
//! here. The entire request is rendered as Markdown the human user can copy out,
//! carry to an external LLM, and paste a reply back in. No tool descriptions are
//! included — the remote LLM operates purely on text context.
//!
//! Two renderers: [`render_courier_request_as_markdown`] (the full bundle — an
//! `# Instructions` header, a `# System` section, a `# Conversation`, and a
//! `# Your turn` footer) and [`render_courier_delta_as_markdown`] (the delta
//! bundle for a character that already has a resolved checkpoint — no system
//! section). Both collapse `\n{3,}` → `\n\n`, then `trimEnd() + '\n'`, byte-exact
//! against v4.

use std::sync::LazyLock;

use regex::Regex;

use crate::format_bytes::format_bytes;
use crate::jsstr::js_trim_end;

/// The attachment descriptor surfaced to the UI and referenced inside the
/// Markdown blob (v4 `CourierAttachmentDescriptor`).
#[derive(Clone, Debug, PartialEq)]
pub struct CourierAttachmentDescriptor {
    pub file_id: String,
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: f64,
    pub download_url: String,
}

/// One provider-shaped message the full-bundle renderer consumes (v4
/// `LLMMessage` subset — `role` / `content` / optional `name` / `attachments`).
#[derive(Clone, Debug)]
pub struct CourierMessage {
    pub role: String,
    pub content: String,
    pub name: Option<String>,
    pub attachments: Vec<CourierMessageAttachment>,
}

/// One attachment on a provider-shaped message (v4 `LLMMessage['attachments']`
/// element — `{ id, filename?, mimeType?, size? }`).
#[derive(Clone, Debug)]
pub struct CourierMessageAttachment {
    pub id: String,
    pub filename: Option<String>,
    pub mime_type: Option<String>,
    pub size: Option<f64>,
}

/// Per-character delta-mode checkpoint stored on `chats.courierCheckpoints[cid]`
/// (v4 `CourierCheckpoint`).
#[derive(Clone, Debug)]
pub struct CourierCheckpoint {
    pub last_resolved_message_id: String,
    /// ISO timestamp of when the operator submitted the paste. The delta includes
    /// every chat event with `createdAt > resolvedAt`.
    pub resolved_at: String,
}

/// One delta event — a `chat_messages` row that came AFTER the checkpoint (v4
/// `CourierDeltaEvent`).
#[derive(Clone, Debug)]
pub struct CourierDeltaEvent {
    /// Display speaker label, already resolved (e.g. "Ariadne", "User", or
    /// "[Staff: Aurora]").
    pub speaker: String,
    pub created_at: String,
    pub content: String,
    /// Attachments referenced by this event (v4: `undefined` when empty).
    pub attachments: Option<Vec<CourierAttachmentDescriptor>>,
}

/// v4 `HUMAN_READABLE_ROLES`.
fn human_readable_role(role: &str) -> &str {
    match role {
        "system" => "System",
        "user" => "User",
        "assistant" => "Assistant",
        "tool" => "Tool result",
        other => other,
    }
}

/// v4 `escapeFilename` — escape `]`, `[`, `(`, `)` with a leading backslash.
fn escape_filename(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if matches!(c, ']' | '[' | '(' | ')') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn attachment_download_url(file_id: &str) -> String {
    format!("/api/v1/files/{file_id}")
}

/// v4 `descriptorFromAttachment` — filename falls back to the id, mime defaults
/// to `application/octet-stream`, size to `0`.
fn descriptor_from_attachment(att: &CourierMessageAttachment) -> CourierAttachmentDescriptor {
    let filename = att
        .filename
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| att.id.clone());
    let mime_type = att
        .mime_type
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "application/octet-stream".to_string());
    CourierAttachmentDescriptor {
        file_id: att.id.clone(),
        filename,
        mime_type,
        size_bytes: att.size.unwrap_or(0.0),
        download_url: attachment_download_url(&att.id),
    }
}

/// v4's final normalize: `parts.join('\n').replace(/\n{3,}/g,'\n\n').trimEnd() + '\n'`.
fn finalize(parts: &[String]) -> String {
    static COLLAPSE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\n{3,}").expect("courier newline collapse regex"));
    let joined = parts.join("\n");
    let collapsed = COLLAPSE.replace_all(&joined, "\n\n");
    let trimmed = js_trim_end(&collapsed);
    format!("{trimmed}\n")
}

/// Render an attachment bullet list into `lines` (shared by both renderers, with
/// the caller supplying the exact intro string — the two renderers differ by one
/// word in the parenthetical).
fn push_attachment_lines(lines: &mut Vec<String>, intro: &str, atts: &[CourierAttachmentDescriptor]) {
    lines.push(intro.to_string());
    lines.push(String::new());
    for a in atts {
        lines.push(format!(
            "- [{}]({}) — {}, {}",
            escape_filename(&a.filename),
            a.download_url,
            a.mime_type,
            format_bytes(a.size_bytes),
        ));
    }
    lines.push(String::new());
}

/// v4 `renderMessageBlock` — one full-bundle message as a Markdown section.
fn render_message_block(m: &CourierMessage, atts: &[CourierAttachmentDescriptor]) -> String {
    let role_label = human_readable_role(&m.role);
    let speaker = match &m.name {
        Some(name) => format!("{role_label} — {name}"),
        None => role_label.to_string(),
    };
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("### {speaker}"));
    lines.push(String::new());
    if !m.content.is_empty() {
        lines.push(m.content.clone());
        lines.push(String::new());
    }
    if !atts.is_empty() {
        push_attachment_lines(
            &mut lines,
            "**Attachments** _(download below; re-upload in your destination client if it supports the type):_",
            atts,
        );
    }
    lines.join("\n")
}

/// The full-bundle result (v4 `RenderCourierRequestOutput`).
pub struct RenderCourierRequestOutput {
    pub markdown: String,
    pub attachments: Vec<CourierAttachmentDescriptor>,
}

/// v4 `renderCourierRequestAsMarkdown`.
pub fn render_courier_request_as_markdown(
    messages: &[CourierMessage],
    character_name: &str,
    model_label: Option<&str>,
) -> RenderCourierRequestOutput {
    let mut aggregated: Vec<CourierAttachmentDescriptor> = Vec::new();

    let system_messages: Vec<&CourierMessage> =
        messages.iter().filter(|m| m.role == "system").collect();
    let non_system_messages: Vec<&CourierMessage> =
        messages.iter().filter(|m| m.role != "system").collect();

    let mut parts: Vec<String> = Vec::new();
    parts.push("# Instructions".to_string());
    parts.push(String::new());
    parts.push(format!(
        "You are roleplaying as **{character_name}**. Below is the full context Quilltap would otherwise send to an LLM API. Read the **System** section for character identity, scene state, memories, and any roleplay-template instructions; then read the **Conversation** for prior messages; then write **{character_name}**'s next turn."
    ));
    parts.push(String::new());
    parts.push("- Respond in Markdown as plain prose — no JSON wrapper.".to_string());
    parts.push("- Do not break character.".to_string());
    parts.push("- You have no Quilltap tools available; respond using only text. Any tools offered by your own host (web search, code interpreter, etc.) are your own discretion.".to_string());
    if let Some(label) = model_label {
        parts.push(format!(
            "- Suggested model: `{label}` (informational; use whichever LLM you prefer)."
        ));
    }
    parts.push(String::new());

    if !system_messages.is_empty() {
        parts.push("# System".to_string());
        parts.push(String::new());
        for m in &system_messages {
            if !m.content.is_empty() {
                parts.push(m.content.clone());
                parts.push(String::new());
            }
        }
    }

    parts.push("# Conversation".to_string());
    parts.push(String::new());
    if non_system_messages.is_empty() {
        parts.push("_(no prior conversation)_".to_string());
        parts.push(String::new());
    } else {
        for m in &non_system_messages {
            let message_atts: Vec<CourierAttachmentDescriptor> =
                m.attachments.iter().map(descriptor_from_attachment).collect();
            for a in &message_atts {
                if !aggregated.iter().any(|x| x.file_id == a.file_id) {
                    aggregated.push(a.clone());
                }
            }
            parts.push(render_message_block(m, &message_atts));
        }
    }

    parts.push("# Your turn".to_string());
    parts.push(String::new());
    parts.push(format!(
        "Write **{character_name}**'s next message. Return only the message body in Markdown."
    ));
    parts.push(String::new());

    RenderCourierRequestOutput {
        markdown: finalize(&parts),
        attachments: aggregated,
    }
}

/// v4 `renderDeltaEventBlock`.
fn render_delta_event_block(event: &CourierDeltaEvent) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("### {} _({})_", event.speaker, event.created_at));
    lines.push(String::new());
    if !event.content.is_empty() {
        lines.push(event.content.clone());
        lines.push(String::new());
    }
    let empty: Vec<CourierAttachmentDescriptor> = Vec::new();
    let atts = event.attachments.as_ref().unwrap_or(&empty);
    if !atts.is_empty() {
        push_attachment_lines(
            &mut lines,
            "**Attachments** _(download below; re-upload if your destination client supports the type):_",
            atts,
        );
    }
    lines.join("\n")
}

/// The delta-bundle result (v4 `RenderCourierDeltaOutput`).
pub struct RenderCourierDeltaOutput {
    pub markdown: String,
    pub attachments: Vec<CourierAttachmentDescriptor>,
}

/// v4 `renderCourierDeltaAsMarkdown`.
pub fn render_courier_delta_as_markdown(
    events: &[CourierDeltaEvent],
    character_name: &str,
    model_label: Option<&str>,
) -> RenderCourierDeltaOutput {
    let mut aggregated: Vec<CourierAttachmentDescriptor> = Vec::new();

    let mut parts: Vec<String> = Vec::new();
    parts.push("# Continuing the conversation".to_string());
    parts.push(String::new());
    parts.push(format!(
        "You are still **{character_name}**. The bundle below contains only what is new since your last reply — Quilltap has otherwise been carrying on the conversation without you. Read the new messages, then write **{character_name}**'s next turn."
    ));
    parts.push(String::new());
    parts.push("- Respond in Markdown as plain prose — no JSON wrapper.".to_string());
    parts.push("- Do not break character.".to_string());
    parts.push("- If your LLM client has lost the earlier conversation (new chat, app restart, switched models), ask the operator to switch this bubble to \"Use full context\" before you reply.".to_string());
    if let Some(label) = model_label {
        parts.push(format!(
            "- Suggested model: `{label}` (informational; use whichever LLM you prefer)."
        ));
    }
    parts.push(String::new());

    parts.push("# New since your last reply".to_string());
    parts.push(String::new());
    if events.is_empty() {
        parts.push("_(Nothing new — the operator nudged you to continue speaking.)_".to_string());
        parts.push(String::new());
    } else {
        for e in events {
            if let Some(atts) = &e.attachments {
                for a in atts {
                    if !aggregated.iter().any(|x| x.file_id == a.file_id) {
                        aggregated.push(a.clone());
                    }
                }
            }
            parts.push(render_delta_event_block(e));
        }
    }

    parts.push("# Your turn".to_string());
    parts.push(String::new());
    parts.push(format!(
        "Write **{character_name}**'s next message. Return only the message body in Markdown."
    ));
    parts.push(String::new());

    RenderCourierDeltaOutput {
        markdown: finalize(&parts),
        attachments: aggregated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: &str, content: &str) -> CourierMessage {
        CourierMessage {
            role: role.to_string(),
            content: content.to_string(),
            name: None,
            attachments: Vec::new(),
        }
    }

    #[test]
    fn escape_filename_escapes_brackets_and_parens() {
        assert_eq!(escape_filename("a[b](c)"), "a\\[b\\]\\(c\\)");
        assert_eq!(escape_filename("plain.png"), "plain.png");
    }

    #[test]
    fn full_bundle_structure_and_normalization() {
        let out = render_courier_request_as_markdown(
            &[msg("system", "SYS\n\n\n\nBODY"), msg("user", "hi")],
            "Aria",
            Some("gpt-4"),
        );
        // Leads with the header; ends with exactly one trailing newline.
        assert!(out.markdown.starts_with("# Instructions\n"));
        assert!(out.markdown.ends_with("\n"));
        assert!(!out.markdown.ends_with("\n\n"));
        // Sections present, model label rendered.
        assert!(out.markdown.contains("# System"));
        assert!(out.markdown.contains("# Conversation"));
        assert!(out.markdown.contains("### User"));
        assert!(out.markdown.contains("# Your turn"));
        assert!(out.markdown.contains("- Suggested model: `gpt-4`"));
        // Runs of 3+ newlines collapse to exactly two.
        assert!(!out.markdown.contains("\n\n\n"));
    }

    #[test]
    fn full_bundle_empty_conversation_marker() {
        let out = render_courier_request_as_markdown(&[msg("system", "SYS")], "Aria", None);
        assert!(out.markdown.contains("_(no prior conversation)_"));
        assert!(!out.markdown.contains("Suggested model"));
    }

    #[test]
    fn delta_bundle_nothing_new_marker() {
        let out = render_courier_delta_as_markdown(&[], "Aria", None);
        assert!(out.markdown.starts_with("# Continuing the conversation\n"));
        assert!(out
            .markdown
            .contains("_(Nothing new — the operator nudged you to continue speaking.)_"));
        assert!(out.markdown.ends_with("\n"));
        assert!(!out.markdown.contains("\n\n\n"));
    }

    #[test]
    fn full_bundle_attachment_descriptor_defaults() {
        let m = CourierMessage {
            role: "user".to_string(),
            content: "see file".to_string(),
            name: Some("Charlie".to_string()),
            attachments: vec![CourierMessageAttachment {
                id: "file-1".to_string(),
                filename: None,
                mime_type: None,
                size: None,
            }],
        };
        let out = render_courier_request_as_markdown(&[m], "Aria", None);
        // filename falls back to id; mime defaults; size 0 → "0 B".
        assert_eq!(out.attachments.len(), 1);
        assert_eq!(out.attachments[0].filename, "file-1");
        assert_eq!(out.attachments[0].mime_type, "application/octet-stream");
        assert_eq!(out.attachments[0].size_bytes, 0.0);
        assert!(out
            .markdown
            .contains("- [file-1](/api/v1/files/file-1) — application/octet-stream, 0 B"));
        // The speaker carries the name.
        assert!(out.markdown.contains("### User — Charlie"));
    }
}
