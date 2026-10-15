//! Suparṇā mail READ feeder (v4 `lib/services/suparna-notifications/writer.ts`
//! `buildSuparnaMailLLMContext` + the `collectUnalertedMail` / `markAlerted`
//! mailbox check that `buildContext` runs before a turn).
//!
//! When a character is about to take a turn, the Post Office checks that
//! character's mailbox; any letters not yet announced feed a plain second-person
//! mail summary into the new user message so the model reliably ACTS on its mail.
//!
//! Closes the READ half of the `BuildContextSeams::resolve_suparna_mail_context`
//! seam (W4.6a). The whisper POST (`postSuparnaMailWhisper`) is the caller's.
//! Since P4.D291 (bug 227, v4 `69d38dd60`) this module flips NOTHING: the
//! letters a turn announces come back as a
//! [`MailAlertsOwed`](crate::post_office::mailbox::MailAlertsOwed) and are
//! marked `alerted` only once the turn has delivered them
//! ([`crate::post_office::mailbox::mark_mail_alerts_delivered`]), so a provider
//! failure or an abort offers them again on the next attempt.

use crate::db::runtime::Db;
use crate::db::DbError;
use crate::post_office::instructions::format_letter_date;
use crate::post_office::mailbox::{
    collect_unalerted_mail, letter_file_name, DeliveredLetterSummary, MailAlertsOwed,
};
use jiff::tz::TimeZone;

/// v4 `buildSuparnaMailLLMContext`: plain second-person framing for the LLM
/// context, so the model reliably acts on its mail. `''` for an empty list.
pub fn build_suparna_mail_llm_context(
    letters: &[DeliveredLetterSummary],
    zone: &TimeZone,
) -> String {
    if letters.is_empty() {
        return String::new();
    }
    let count = letters.len();
    let intro = format!(
        "Suparṇā of the Post Office has delivered new mail to you{}. Each letter is below.",
        if count == 1 {
            String::new()
        } else {
            format!(" ({count} letters)")
        }
    );
    let parts = letters
        .iter()
        .map(|letter| {
            let head = format!(
                "Letter from {}, delivered {} (letter: {}):",
                letter.from,
                format_letter_date(&letter.sent_at, zone),
                letter_file_name(&letter.path)
            );
            let body = {
                let trimmed = crate::jsstr::js_trim(&letter.body);
                if trimmed.is_empty() {
                    "(the letter is blank)".to_string()
                } else {
                    trimmed.to_string()
                }
            };
            format!("{head}\n{body}")
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    // v4 `39bc98ffc`/`12c336fad`: letters are named by FILE NAME, handed to the
    // Post Office's own tools (before them: the `qtap://self/…` URI and `doc_*`).
    let howto = "You can read any letter again with read_mail({ letter: \"<its file name>\" }), answer it with send_mail (set in_reply_to to its file name), or discard it with discard_mail({ letter: \"<its file name>\" }).";
    format!("{intro}\n\n{parts}\n\n{howto}")
}

/// The READ half of v4's Suparṇā mail check (build_context's suparna block minus
/// the whisper POST). Reads the character vault's unalerted mail; if any, builds
/// the LLM-context contribution and the owed-alerts record. It flips NOTHING
/// (bug 227): `build_context` collects and announces, the turn's delivery site
/// marks. The whisper POST is the caller's (it posts before handing the context
/// on, exactly as v4 orders the two).
///
/// Letter dates render in `zone` (v4's zone-less `formatDateTime` — the host's
/// zone, threaded from the composition root; P4.119/P4.127).
///
/// Returns `(llm_context, unalerted, owed)` — the unalerted list for the
/// whisper post without a re-read, and `Some(owed)` exactly when there is a
/// letter. A failed read is the caller's to WARN (v4's `[Suparna] Mail check
/// failed; turn continues` — a mail failure never breaks the turn).
pub async fn resolve_suparna_mail_context_in_zone(
    db: &Db,
    mail_vault_id: &str,
    zone: &TimeZone,
) -> Result<(String, Vec<DeliveredLetterSummary>, Option<MailAlertsOwed>), DbError> {
    let vault = mail_vault_id.to_string();
    let unalerted = db.read_mount_index(move |conn| collect_unalerted_mail(conn, &vault))?;
    if unalerted.is_empty() {
        return Ok((String::new(), Vec::new(), None));
    }
    let llm_context = build_suparna_mail_llm_context(&unalerted, zone);
    let owed = MailAlertsOwed {
        vault_id: mail_vault_id.to_string(),
        paths: unalerted.iter().map(|letter| letter.path.clone()).collect(),
    };
    Ok((llm_context, unalerted, Some(owed)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn letter(from: &str, sent_at: &str, body: &str, path: &str) -> DeliveredLetterSummary {
        DeliveredLetterSummary {
            path: path.to_string(),
            from: from.to_string(),
            sent_at: sent_at.to_string(),
            body: body.to_string(),
            alerted: false,
            in_reply_to: None,
        }
    }

    #[test]
    fn empty_letters_yield_empty_context() {
        assert_eq!(build_suparna_mail_llm_context(&[], &TimeZone::UTC), "");
    }

    #[test]
    fn single_letter_context() {
        let l = letter(
            "Friday",
            "2026-02-01T09:05:00.000Z",
            "  Hello there.  ",
            "Mail/friday-2026-02-01.md",
        );
        let out = build_suparna_mail_llm_context(std::slice::from_ref(&l), &TimeZone::UTC);
        assert!(out.starts_with(
            "Suparṇā of the Post Office has delivered new mail to you. Each letter is below."
        ));
        assert!(out.contains("Letter from Friday, delivered "));
        assert!(out.contains("(letter: friday-2026-02-01.md):"));
        assert!(out.contains("\nHello there.\n"));
        assert!(out.contains("You can read any letter again with read_mail"));
    }

    #[test]
    fn plural_letters_count_and_blank_body() {
        let l1 = letter("Ada", "2026-02-02T09:00:00.000Z", "First.", "Mail/a.md");
        let l2 = letter("Bob", "2026-02-01T09:00:00.000Z", "   ", "Mail/b.md");
        let out = build_suparna_mail_llm_context(&[l1, l2], &TimeZone::UTC);
        assert!(out.contains("delivered new mail to you (2 letters)."));
        assert!(out.contains("(the letter is blank)"));
    }
}
