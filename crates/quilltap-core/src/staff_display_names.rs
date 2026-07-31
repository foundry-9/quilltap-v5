//! How a Staff member is named in prose — v4 `lib/chat/staff-display-names.ts`
//! (`0246c6c8`).
//!
//! The single source of truth for the display name behind a `systemSender`.
//! Every surface that spells one out reads it here: the Markdown transcript
//! export, and anything that comes after. Two copies of this table drift the
//! moment a member is added — and adding one already means touching the
//! `systemSender` enum, the `chat_messages` column, `getMessageAvatar`, and the
//! export schema, so it has no business also being a hunt for scattered name
//! maps.
//!
//! A Carina answer is the exception the table cannot express: it renders under
//! the ANSWERER character's own name (and avatar), and falls back to `Carina`
//! only when that character cannot be resolved. Callers handle that before
//! reaching for this map — see
//! [`crate::services::markdown_transcript`].
//!
//! The nearest OTHER v5 table (`courier_transport::courier_system_sender_label`)
//! is a different surface: it lacks three entries and wraps its output in
//! `[Staff: …]`. It is deliberately not folded in here, exactly as v4 left
//! `getMessageAvatar`'s eleven-branch chain alone.

/// v4 `STAFF_DISPLAY_NAMES`, byte-transcribed and in v4's key order (`Suparṇā`
/// carries U+1E47 LATIN SMALL LETTER N WITH DOT BELOW and U+0101 LATIN SMALL
/// LETTER A WITH MACRON).
///
/// v4's is a `Record<SystemSender, string>` — a total map over the sender enum,
/// so the eleven entries here are the whole enum. Order is presentational only;
/// [`staff_display_name`] is a lookup.
pub const STAFF_DISPLAY_NAMES: [(&str, &str); 11] = [
    ("lantern", "The Lantern"),
    ("aurora", "Aurora"),
    ("librarian", "The Librarian"),
    ("concierge", "The Concierge"),
    ("prospero", "Prospero"),
    ("host", "The Host"),
    ("commonplaceBook", "The Commonplace Book"),
    ("ariel", "Ariel"),
    ("carina", "Carina"),
    ("suparna", "Suparṇā"),
    ("pascal", "Pascal"),
];

/// The display name for a `systemSender`, or `""` when there is none (an
/// ordinary participant message) — v4 `staffDisplayName`.
///
/// An unrecognised sender — a row written by a newer build — falls back to the
/// RAW TAG rather than vanishing (v4's `?? sender`). The transcript exporter
/// relied on that same fallback inline before the extraction, so the byte
/// output is unmoved.
pub fn staff_display_name(sender: Option<&str>) -> String {
    let Some(sender) = sender.filter(|s| !s.is_empty()) else {
        // v4 `if (!sender) return ''` — JS falsiness, so '' lands here too.
        return String::new();
    };
    STAFF_DISPLAY_NAMES
        .iter()
        .find(|(key, _)| *key == sender)
        .map(|(_, label)| (*label).to_string())
        .unwrap_or_else(|| sender.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_senders_resolve_and_unknown_falls_back_to_the_raw_tag() {
        assert_eq!(staff_display_name(Some("lantern")), "The Lantern");
        assert_eq!(staff_display_name(Some("suparna")), "Suparṇā");
        assert_eq!(staff_display_name(Some("host")), "The Host");
        // v4 `?? sender`: a row written by a newer build keeps its tag.
        assert_eq!(staff_display_name(Some("newcomer")), "newcomer");
    }

    #[test]
    fn absent_or_empty_sender_is_the_empty_string() {
        assert_eq!(staff_display_name(None), "");
        assert_eq!(staff_display_name(Some("")), "");
    }

    #[test]
    fn the_table_covers_v4s_whole_sender_enum() {
        assert_eq!(STAFF_DISPLAY_NAMES.len(), 11);
    }
}
