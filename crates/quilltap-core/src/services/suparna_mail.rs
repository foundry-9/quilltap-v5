//! Suparṇā mail READ feeder (v4 `lib/services/suparna-notifications/writer.ts`
//! `buildSuparnaMailLLMContext` + the `collectUnalertedMail` / `markAlerted`
//! mailbox check that `buildContext` runs before a turn).
//!
//! When a character is about to take a turn, the Post Office checks that
//! character's mailbox; any letters not yet announced feed a plain second-person
//! mail summary into the new user message so the model reliably ACTS on its mail.
//!
//! Closes the READ half of the `BuildContextSeams::resolve_suparna_mail_context`
//! seam (W4.6a). The whisper POST (`postSuparnaMailWhisper`) is W4.6b — it is NOT
//! ported here. The `alerted` flip IS part of this unit (the read is only correct
//! with the double-announce guard): `mark_alerted` runs on each surfaced letter.

use crate::db::runtime::Db;
use crate::post_office::instructions::format_letter_date;
use crate::post_office::mailbox::{collect_unalerted_mail, mark_alerted, DeliveredLetterSummary};

/// v4 `buildSuparnaMailLLMContext`: plain second-person framing for the LLM
/// context, so the model reliably acts on its mail. `''` for an empty list.
pub fn build_suparna_mail_llm_context(letters: &[DeliveredLetterSummary]) -> String {
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
                "Letter from {}, delivered {} ({}):",
                letter.from,
                format_letter_date(&letter.sent_at),
                crate::doc_edit::qtap_uri::format_self_uri(&letter.path, None, None)
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
    let howto = "You can read any letter again with doc_read_file({ uri: \"qtap://self/<its path>\" }), answer it with send_mail (set in_reply_to to its id — its path), or discard it with doc_delete_file({ uri: \"qtap://self/<its path>\" }).";
    format!("{intro}\n\n{parts}\n\n{howto}")
}

/// The READ half of v4's Suparṇā mail check (build_context's suparna block minus
/// the whisper POST). Reads the character vault's unalerted mail; if any, builds
/// the LLM-context contribution, flips each letter's `alerted` flag, and returns
/// the context string. The whisper POST (`postSuparnaMailWhisper`) is W4.6b (the
/// caller invokes the recorded seam separately). Any failure → `""` (v4 wraps the
/// whole check warn-only — a mail failure never breaks the turn).
///
/// Returns `(llm_context, unalerted)` so the caller can hand the unalerted list to
/// the W4.6b whisper-post seam without re-reading.
pub async fn resolve_suparna_mail_context(
    db: &Db,
    mail_vault_id: &str,
) -> (String, Vec<DeliveredLetterSummary>) {
    let vault = mail_vault_id.to_string();
    let unalerted = match db.read_mount_index(move |conn| collect_unalerted_mail(conn, &vault)) {
        Ok(u) => u,
        Err(_) => return (String::new(), Vec::new()),
    };
    if unalerted.is_empty() {
        return (String::new(), Vec::new());
    }

    let llm_context = build_suparna_mail_llm_context(&unalerted);

    // Flip `alerted` on each surfaced letter (the double-announce guard). Each is a
    // content-only write; a failure never breaks the turn (best-effort).
    for letter in &unalerted {
        let vault = mail_vault_id.to_string();
        let path = letter.path.clone();
        let _ = db
            .write(move |writers| match writers.mount_index() {
                Some(mount_w) => mark_alerted(mount_w.connection(), &vault, &path),
                None => Ok(()),
            })
            .await;
    }

    (llm_context, unalerted)
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
        assert_eq!(build_suparna_mail_llm_context(&[]), "");
    }

    #[test]
    fn single_letter_context() {
        let l = letter(
            "Friday",
            "2026-02-01T09:05:00.000Z",
            "  Hello there.  ",
            "Mail/friday-2026-02-01.md",
        );
        let out = build_suparna_mail_llm_context(std::slice::from_ref(&l));
        assert!(out.starts_with(
            "Suparṇā of the Post Office has delivered new mail to you. Each letter is below."
        ));
        assert!(out.contains("Letter from Friday, delivered "));
        assert!(out.contains("(qtap://self/Mail/friday-2026-02-01.md):"));
        assert!(out.contains("\nHello there.\n"));
        assert!(out.contains("You can read any letter again with doc_read_file"));
    }

    #[test]
    fn plural_letters_count_and_blank_body() {
        let l1 = letter("Ada", "2026-02-02T09:00:00.000Z", "First.", "Mail/a.md");
        let l2 = letter("Bob", "2026-02-01T09:00:00.000Z", "   ", "Mail/b.md");
        let out = build_suparna_mail_llm_context(&[l1, l2]);
        assert!(out.contains("delivered new mail to you (2 letters)."));
        assert!(out.contains("(the letter is blank)"));
    }
}
