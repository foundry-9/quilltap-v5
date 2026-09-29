//! The Post Office — agent-facing instruction snippets (v4
//! `lib/post-office/instructions.ts`).
//!
//! The literal tool calls handed to a character so it can read, answer, or discard
//! a letter. Reused by `list_mail`, `read_mail` and Suparṇā's mail whisper so they
//! never drift. A letter is named to the Post Office tools by its bare file name —
//! `read_mail` and `send_mail` put the `Mail/` folder on it themselves, and reach
//! the caller's own mailbox whether or not the character may otherwise see its own
//! vault (v4 `39bc98ffc`/`12c336fad`; before them the actions named the
//! `qtap://self/…` URI and the `doc_*` tools).

use super::mailbox::{letter_file_name, DeliveredLetterSummary};
use crate::format_time::{format_date_time, MonthStyle};

/// v4 `formatLetterActions`: the indented block of the actions available on a
/// letter, each naming it by its file name. `include_read` is v4's
/// `options.includeRead !== false` — `read_mail` omits the "Read it again" line
/// for the letter it has just read.
pub fn format_letter_actions(path: &str, from: &str, include_read: bool) -> String {
    let name = letter_file_name(path);
    let mut lines: Vec<String> = Vec::with_capacity(3);
    if include_read {
        lines.push(format!(
            "   • Read it again: read_mail({{ letter: \"{name}\" }})"
        ));
    }
    lines.push(format!(
        "   • Answer it: send_mail({{ character: \"{from}\", message: \"…your reply…\", in_reply_to: \"{name}\" }})"
    ));
    lines.push(format!(
        "   • Discard it: discard_mail({{ letter: \"{name}\" }})"
    ));
    lines.join("\n")
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

/// v4 `formatLetterHeading`: heading line(s) for a letter in a numbered listing,
/// naming the letter by its file name — the handle `read_mail` and `in_reply_to`
/// take.
pub fn format_letter_heading(letter: &DeliveredLetterSummary, index: usize) -> String {
    let announced = if letter.alerted {
        " (already announced)"
    } else {
        " (newly arrived)"
    };
    format!(
        "{index}. From {} — {}{announced}\n   Letter: {}",
        letter.from,
        format_letter_date(&letter.sent_at),
        letter_file_name(&letter.path)
    )
}
