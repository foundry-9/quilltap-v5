//! Scenario ordering — v4 `lib/scenarios/sort.ts` (`1d0a31132`, pinned at
//! `322a5488e`): the one comparator every list of scenario objects sorts by.
//!
//! Order is the display title (frontmatter `name`, or whatever the scope's
//! parser falls back to), then the filename, each compared case-insensitively
//! with a leading definite article ignored — "The Gilded Cage" files under G.
//! The untrimmed title and then the path break any remaining tie, so the order
//! is total and stable.
//!
//! Pure. Default-conflict resolution ("alphabetically-first filename wins") is
//! a separate rule and stays on path order.
//!
//! The three first legs are v4's `localeCompare(…, undefined, { sensitivity:
//! 'base' })` ([`locale_compare_base`]); the last — the `path` — is the bare
//! `localeCompare` at DEFAULT options ([`locale_compare`], tertiary: `a` <
//! `A`). The comparator is total and never NaN, so Rust's stable `sort_by` is
//! the V8-stable twin (no `js_array_sort_by`).

use std::cmp::Ordering;

use crate::collation::{locale_compare, locale_compare_base};
use crate::jsstr::{is_js_ws, js_trim};

/// What the comparator needs from a scenario. Owned strings: v4 computes each
/// item's key ONCE (`toSortable`) and sorts the `{item, key}` pairs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortableScenario {
    /// Display title.
    pub title: String,
    /// Filename without `.md`, or the relative path when that is all there is.
    pub filename: String,
    /// Final tiebreaker; usually the relative path.
    pub path: Option<String>,
}

/// v4 `LEADING_ARTICLE = /^the[\s_-]+/i` — "whitespace, dash or underscore
/// after it: filenames are dashed (`the-attic`)". Hand-rolled: the first three
/// chars ASCII-case-insensitively `the`, then a GREEDY run of at least one JS
/// `\s`, `_` or `-`. Returns the remainder after the match, or `None`.
fn strip_leading_article(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    if bytes.len() < 4 || !bytes[..3].eq_ignore_ascii_case(b"the") {
        return None;
    }
    let rest = &s[3..];
    let after = rest.trim_start_matches(|c: char| is_js_ws(c) || c == '_' || c == '-');
    if after.len() == rest.len() {
        None
    } else {
        Some(after)
    }
}

/// The string a title or filename sorts under: trimmed, leading "The " dropped
/// (v4 `scenarioSortKey`). Both results are substrings of the input.
pub fn scenario_sort_key(value: &str) -> &str {
    let trimmed = js_trim(value);
    let stripped = strip_leading_article(trimmed).unwrap_or(trimmed);
    // A title that is only "The" keeps itself rather than sorting as empty.
    if stripped.is_empty() {
        trimmed
    } else {
        stripped
    }
}

/// Compare two scenarios for display order (v4 `compareScenarios`).
pub fn compare_scenarios(a: &SortableScenario, b: &SortableScenario) -> Ordering {
    locale_compare_base(scenario_sort_key(&a.title), scenario_sort_key(&b.title))
        .then_with(|| {
            locale_compare_base(
                scenario_sort_key(&a.filename),
                scenario_sort_key(&b.filename),
            )
        })
        .then_with(|| locale_compare_base(&a.title, &b.title))
        .then_with(|| {
            locale_compare(
                a.path.as_deref().unwrap_or(""),
                b.path.as_deref().unwrap_or(""),
            )
        })
}

/// Return a sorted copy of `items`, reading each one's sort fields through
/// `to_sortable` (v4 `sortScenarios`).
pub fn sort_scenarios<T>(items: Vec<T>, to_sortable: impl Fn(&T) -> SortableScenario) -> Vec<T> {
    let mut keyed: Vec<(T, SortableScenario)> = items
        .into_iter()
        .map(|item| {
            let key = to_sortable(&item);
            (item, key)
        })
        .collect();
    keyed.sort_by(|x, y| compare_scenarios(&x.1, &y.1));
    keyed.into_iter().map(|(item, _)| item).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sc(title: &str, filename: &str, path: Option<&str>) -> SortableScenario {
        SortableScenario {
            title: title.into(),
            filename: filename.into(),
            path: path.map(str::to_string),
        }
    }

    #[test]
    fn drops_a_leading_the_whatever_its_case() {
        assert_eq!(scenario_sort_key("The Gilded Cage"), "Gilded Cage");
        assert_eq!(scenario_sort_key("  THE Gilded Cage "), "Gilded Cage");
        assert_eq!(scenario_sort_key("the-attic"), "attic");
        assert_eq!(scenario_sort_key("the_attic"), "attic");
    }

    #[test]
    fn leaves_the_inside_a_word_or_mid_title_alone() {
        assert_eq!(scenario_sort_key("Theatre Night"), "Theatre Night");
        assert_eq!(scenario_sort_key("Into the Woods"), "Into the Woods");
        assert_eq!(scenario_sort_key("Thé X"), "Thé X");
    }

    #[test]
    fn keeps_a_title_that_is_only_the() {
        assert_eq!(scenario_sort_key("The"), "The");
        assert_eq!(scenario_sort_key("The-"), "The-");
        assert_eq!(scenario_sort_key(""), "");
    }

    #[test]
    fn the_strip_follows_js_whitespace_not_rusts() {
        assert_eq!(scenario_sort_key("The\u{A0}X"), "X");
        assert_eq!(scenario_sort_key("The\u{FEFF}X"), "X");
        // U+0085 is NOT JS whitespace: no strip.
        assert_eq!(scenario_sort_key("The\u{85}X"), "The\u{85}X");
        // The trim comes first: a BOM-led title still strips.
        assert_eq!(scenario_sort_key("\u{FEFF}The Ballroom"), "Ballroom");
    }

    #[test]
    fn sorts_by_title_with_the_leading_article_ignored() {
        let titles = ["The Ballroom", "Casino", "untitled-dock", "The Zeppelin"];
        let sorted = sort_scenarios(titles.to_vec(), |t| sc(t, t, None));
        assert_eq!(
            sorted,
            vec!["The Ballroom", "Casino", "untitled-dock", "The Zeppelin"]
        );
    }

    #[test]
    fn breaks_a_title_tie_on_the_filename_article_ignored_there_too() {
        let items = vec![("b", "the-zed"), ("a", "attic")];
        let sorted = sort_scenarios(items, |(t, f)| sc(t, f, None));
        assert_eq!(sorted, vec![("a", "attic"), ("b", "the-zed")]);
        let items = vec![("same", "the-zed"), ("same", "attic")];
        let sorted = sort_scenarios(items, |(t, f)| sc(t, f, None));
        assert_eq!(sorted, vec![("same", "attic"), ("same", "the-zed")]);
    }

    #[test]
    fn is_a_total_order_falling_back_to_the_raw_title_then_the_path() {
        // Primary-equal title and filename: the raw title (also primary-equal
        // here) ties, so the DEFAULT-option path decides — `a` before `A`.
        let a = sc("gala", "gala", Some("Scenarios/a.md"));
        let b = sc("Gala", "Gala", Some("Scenarios/A.md"));
        assert_eq!(compare_scenarios(&a, &b), Ordering::Less);
        assert_eq!(compare_scenarios(&b, &a), Ordering::Greater);
        assert_eq!(compare_scenarios(&a, &a), Ordering::Equal);
        // A path-less pair: `''` before `'a'`.
        assert_eq!(
            compare_scenarios(&sc("x", "x", None), &sc("x", "x", Some("a"))),
            Ordering::Less
        );
    }
}
