//! The Post Office — agent-facing instruction snippets (v4
//! `lib/post-office/instructions.ts`).
//!
//! The literal tool calls handed to a character so it can read, answer, or discard
//! a letter. Mail always lives in the recipient's own vault, so it is addressed
//! with the canonical `qtap://self/…` URI form.

use super::mailbox::DeliveredLetterSummary;
use crate::doc_edit::qtap_uri::format_self_uri;
use crate::format_time::{format_date_time, MonthStyle};

/// v4 `formatLetterActions`: the indented block of the three actions available on
/// a letter. Read/discard lead with the `qtap://self/…` URI; `in_reply_to` keeps
/// the raw letter id.
pub fn format_letter_actions(path: &str, from: &str) -> String {
    let uri = format_self_uri(path, None, None);
    [
        format!("   • Read it again: doc_read_file({{ uri: \"{uri}\" }})"),
        format!(
            "   • Answer it: send_mail({{ character: \"{from}\", message: \"…your reply…\", in_reply_to: \"{path}\" }})"
        ),
        format!("   • Discard it: doc_delete_file({{ uri: \"{uri}\" }})"),
    ]
    .join("\n")
}

/// v4 `formatLetterDate`: a one-line human date, falling back gracefully.
pub fn format_letter_date(sent_at: &str) -> String {
    let formatted = format_date_time(Some(sent_at), MonthStyle::Long);
    if formatted.is_empty() {
        "an unrecorded hour".to_string()
    } else {
        formatted
    }
}

/// v4 `formatLetterHeading`: heading line(s) for a letter in a numbered listing.
pub fn format_letter_heading(letter: &DeliveredLetterSummary, index: usize) -> String {
    let announced = if letter.alerted {
        " (already announced)"
    } else {
        " (newly arrived)"
    };
    format!(
        "{index}. From {} — {}{announced}\n   {}",
        letter.from,
        format_letter_date(&letter.sent_at),
        format_self_uri(&letter.path, None, None)
    )
}
