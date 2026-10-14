//! Port of v4 `lib/memory/reinforcement-footnotes.ts` (`e0ac7493a`, Phase A of
//! the memory system refactor; `8af764514` added the `cap` argument) — the
//! `[+] detail` lines a retelling appends to the memory it reinforces.
//!
//! v4's why, carried: pure and dependency-free, so the memories repository can
//! apply them inside its write transaction against the row as it stands at
//! commit time, and the job child's overlay can apply the same rule to its
//! local view. Two jobs that each reinforce one row therefore both land their
//! footnotes, instead of the later job's whole-content write replacing the
//! earlier one's (bug 231).
//!
//! What moved against the pre shape (`memory_gate.rs`'s 2-argument
//! `append_capped_footnotes`, which took a PREFIX of `details`): a detail whose
//! `[+] detail` line the content already carries is SKIPPED, and details
//! repeated within one call are appended once (bugs 231 / 234), so applying
//! the same details twice — a replay, or two jobs that found the same detail —
//! appends them once.
//!
//! v4's doc comment on `MAX_REINFORCEMENT_FOOTNOTES` (`:15-19`) still says the
//! cap "skips the re-embed"; that is stale against `fold.ts` (bug 204 — an
//! anchor change re-embeds AT the cap). The re-embed rule lives in
//! [`crate::admission::fold_into`]; this module only counts and appends.

use std::collections::HashSet;

/// Most `[+]` footnote lines a memory may carry (v4
/// `MAX_REINFORCEMENT_FOOTNOTES`). Past this, reinforcement still counts the
/// observation and unions entities but appends no more prose — an unbounded
/// tail drifts the row's vector toward a bag of proper nouns.
pub const MAX_REINFORCEMENT_FOOTNOTES: usize = 8;

/// v4 `FOOTNOTE_PREFIX`.
const FOOTNOTE_PREFIX: &str = "[+] ";

/// Number of `[+]` footnote lines already appended to a memory's content (v4
/// `countReinforcementFootnotes`): the lines of `content.split('\n')` that
/// start with `[+] ` (a `[+]x` line with no space is prose; a `\r\n` content's
/// lines keep their `\r` at the END, so they still count).
pub fn count_reinforcement_footnotes(content: &str) -> usize {
    content
        .split('\n')
        .filter(|line| line.starts_with(FOOTNOTE_PREFIX))
        .count()
}

/// Append novel details to `content` as `[+]` footnotes, stopping at `cap`
/// footnote lines in all (v4 `appendCappedFootnotes(content, details, cap)`;
/// v4's default `cap` is [`MAX_REINFORCEMENT_FOOTNOTES`] — pass it).
///
/// A detail whose footnote line the content already carries is skipped, and a
/// detail repeated within `details` is appended once. Returns the new content
/// and the details actually appended (in order, possibly empty — then the
/// content comes back unchanged).
///
/// `present` is the set of existing footnote LINES, whole: a `\r\n` content's
/// lines end in `\r`, so `[+] X\r` never matches the detail `X` and `X` is
/// appended again (v4's behaviour, ported as is). `room` is
/// `max(0, cap − count)`, so a cap below the present count appends nothing.
pub fn append_capped_footnotes(
    content: &str,
    details: &[String],
    cap: usize,
) -> (String, Vec<String>) {
    let mut present: HashSet<String> = content
        .split('\n')
        .filter(|line| line.starts_with(FOOTNOTE_PREFIX))
        .map(str::to_string)
        .collect();
    let room = cap.saturating_sub(count_reinforcement_footnotes(content));
    let mut appended: Vec<String> = Vec::new();
    for detail in details {
        if appended.len() >= room {
            break;
        }
        let line = format!("{FOOTNOTE_PREFIX}{detail}");
        if !present.insert(line) {
            continue;
        }
        appended.push(detail.clone());
    }
    if appended.is_empty() {
        return (content.to_string(), appended);
    }
    let footnotes = appended
        .iter()
        .map(|d| format!("{FOOTNOTE_PREFIX}{d}"))
        .collect::<Vec<_>>()
        .join("\n");
    (format!("{content}\n{footnotes}"), appended)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn a_detail_already_present_is_skipped() {
        let (content, appended) =
            append_capped_footnotes("Body\n[+] Lisbon", &s(&["Lisbon", "Porto"]), 8);
        assert_eq!(appended, s(&["Porto"]));
        assert_eq!(content, "Body\n[+] Lisbon\n[+] Porto");
    }

    #[test]
    fn a_duplicate_within_the_call_is_appended_once() {
        let (content, appended) = append_capped_footnotes("Body", &s(&["A1", "A1"]), 8);
        assert_eq!(appended, s(&["A1"]));
        assert_eq!(content, "Body\n[+] A1");
    }

    #[test]
    fn a_cap_below_the_present_count_appends_nothing() {
        let (content, appended) =
            append_capped_footnotes("Body\n[+] a\n[+] b\n[+] c", &s(&["d"]), 2);
        assert!(appended.is_empty());
        assert_eq!(content, "Body\n[+] a\n[+] b\n[+] c");
    }

    #[test]
    fn counts_only_spaced_prefixes() {
        assert_eq!(count_reinforcement_footnotes("x\n[+]y\n[+] z\r\n[+] w"), 2);
    }
}
