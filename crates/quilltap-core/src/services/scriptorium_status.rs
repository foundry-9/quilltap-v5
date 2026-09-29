//! The Scriptorium badge's status — v4 `lib/scriptorium/status.ts`
//! (`deriveScriptoriumStatus`, `f7f3d7bf0`).
//!
//! Derived from the chat's `conversation_chunks` counts ALONE, the one rule
//! both readers share (the chat list's enrichment and the character GET's
//! conversations page). Before `f7f3d7bf0` each reader inlined its own copy
//! gated on a non-empty `chats.renderedMarkdown`; that column is gone.
//!
//! ⚠ Two edges INVERT and the commit message never says so (§R.4(c)): a chat
//! with chunks but no stored Markdown was `none` and is now `rendered` /
//! `embedded` — every chat the old stale sweep cold-tiered; a chat with stored
//! Markdown but no chunks was `rendered` and is now `none`.

/// v4 `ScriptoriumStatus` — the DTO's `scriptoriumStatus` union, unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptoriumStatus {
    None,
    Rendered,
    Embedded,
}

impl ScriptoriumStatus {
    /// The wire string.
    pub fn as_str(self) -> &'static str {
        match self {
            ScriptoriumStatus::None => "none",
            ScriptoriumStatus::Rendered => "rendered",
            ScriptoriumStatus::Embedded => "embedded",
        }
    }
}

/// v4 `deriveScriptoriumStatus(counts)`: `counts` is `(total, embedded)` for
/// one chat, `None` when the chat has no row in the grouped count (v4's
/// `Map.get` → `undefined`).
pub fn derive_scriptorium_status(counts: Option<(i64, i64)>) -> ScriptoriumStatus {
    match counts {
        None | Some((0, _)) => ScriptoriumStatus::None,
        Some((total, embedded)) if embedded >= total => ScriptoriumStatus::Embedded,
        Some(_) => ScriptoriumStatus::Rendered,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v4 `__tests__/unit/lib/scriptorium/status.test.ts`, row for row.
    #[test]
    fn v4s_five_rows() {
        assert_eq!(derive_scriptorium_status(None).as_str(), "none");
        assert_eq!(derive_scriptorium_status(Some((0, 0))).as_str(), "none");
        assert_eq!(
            derive_scriptorium_status(Some((14, 12))).as_str(),
            "rendered"
        );
        assert_eq!(derive_scriptorium_status(Some((3, 0))).as_str(), "rendered");
        assert_eq!(
            derive_scriptorium_status(Some((24, 24))).as_str(),
            "embedded"
        );
    }
}
