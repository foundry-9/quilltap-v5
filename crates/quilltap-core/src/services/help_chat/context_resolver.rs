//! v4 `lib/help-chat/context-resolver.ts` — resolve the current page URL to
//! the best matching help documentation with a tiered strategy:
//! exact → query params → pattern → prefix → wildcard → fallback.
//!
//! Pure over the document list the caller reads (v4 reads `listDocuments()`
//! from the in-process cache and `getDocument(id)` per match; v5 hands the
//! resolver the rows), so the tier-1 family `help_context_resolver_equivalence`
//! drives it from a corpus.
//!
//! ## JS fidelity notes (each pinned by a corpus row)
//!
//! - `url.split('?')` has no limit: `'/a?b?c'` splits into THREE parts and v4
//!   reads `[urlPath, urlQuery]` = `['/a', 'b']` — the third part is dropped.
//!   [`split_url`] reproduces that (NOT `split_once`, which would keep `b?c`).
//! - `if (urlQuery)` is JS truthiness: `'/settings?'` yields `urlQuery = ''`,
//!   so strategy 1 (exact incl. query) is skipped and the query-specificity
//!   branch of strategy 2 is skipped too.
//! - `URLSearchParams` parsing (`application/x-www-form-urlencoded`): `&`-split,
//!   empty segments dropped, `+` → space, percent-decoded; `.get(k)` is the
//!   FIRST value; `[...entries()].length` counts every pair incl. repeats.
//! - Every `sort` is JS `Array.prototype.sort` — STABLE — so ties keep the
//!   filtered (document) order; `sort_by` here is stable too.
//! - Pattern specificity counts `url.split('/').length` over the WHOLE url
//!   (query included), prefix specificity is `url.length` in UTF-16 units.
//! - `resolveAllHelpContentForUrl`'s dedup compares `doc.id === primary.url` —
//!   an id against a URL, which is NEVER true, so when the primary IS a wildcard
//!   document it is pushed twice. Reproduced faithfully (pinned by the
//!   `duplicate-wildcard` corpus row) and recorded as a candidate upstream
//!   filing; the port never fixes v4's bugs silently.

use super::HelpDocument;
use crate::jsstr::utf16_len;

/// v4 `HelpPageContext['matchType']`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchType {
    Exact,
    Query,
    Pattern,
    Prefix,
    Wildcard,
    Fallback,
}

impl MatchType {
    pub fn as_str(self) -> &'static str {
        match self {
            MatchType::Exact => "exact",
            MatchType::Query => "query",
            MatchType::Pattern => "pattern",
            MatchType::Prefix => "prefix",
            MatchType::Wildcard => "wildcard",
            MatchType::Fallback => "fallback",
        }
    }
}

/// v4 `HelpPageContext`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelpPageContext {
    pub title: String,
    pub content: String,
    pub url: String,
    pub match_type: MatchType,
    /// The matched document's id (not on v4's context — carried so the
    /// differential can name the match; never serialized into a prompt).
    pub doc_id: String,
}

/// JS `s.split('?')` read as `const [path, query] = …`: the FIRST segment and the
/// SECOND (`None` when there is no `?` — JS `undefined`), any further segments
/// dropped.
fn split_url(url: &str) -> (&str, Option<&str>) {
    let mut it = url.split('?');
    let path = it.next().unwrap_or("");
    (path, it.next())
}

/// WHATWG `URLSearchParams` construction over an `application/x-www-form-urlencoded`
/// string: `&`-separated, empty segments skipped, split on the FIRST `=` (a
/// segment without `=` is a key with the empty value), `+` → space, then
/// percent-decoded (invalid sequences left as-is, lossy UTF-8).
fn parse_search_params(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|seg| !seg.is_empty())
        .map(|seg| match seg.find('=') {
            Some(i) => (form_decode(&seg[..i]), form_decode(&seg[i + 1..])),
            None => (form_decode(seg), String::new()),
        })
        .collect()
}

fn form_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 3 <= bytes.len()
                && bytes[i + 1].is_ascii_hexdigit()
                && bytes[i + 2].is_ascii_hexdigit() =>
            {
                let hi = (bytes[i + 1] as char).to_digit(16).unwrap() as u8;
                let lo = (bytes[i + 2] as char).to_digit(16).unwrap() as u8;
                out.push(hi * 16 + lo);
                i += 3;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `URLSearchParams.get(key)` — the FIRST value, `None` for absent (JS `null`).
fn params_get<'a>(params: &'a [(String, String)], key: &str) -> Option<&'a str> {
    params
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

/// v4 `matchUrlPattern(pattern, actualPath)` — `:param` segments match anything;
/// the segment counts must agree (`split('/')` on both sides, so leading slashes
/// produce an empty first segment on each).
pub fn match_url_pattern(pattern: &str, actual_path: &str) -> bool {
    let pattern_parts: Vec<&str> = pattern.split('/').collect();
    let actual_parts: Vec<&str> = actual_path.split('/').collect();
    if pattern_parts.len() != actual_parts.len() {
        return false;
    }
    pattern_parts
        .iter()
        .zip(actual_parts.iter())
        .all(|(p, a)| p.starts_with(':') || p == a)
}

fn build_context(doc: &HelpDocument, match_type: MatchType) -> HelpPageContext {
    HelpPageContext {
        title: doc.title.clone(),
        content: doc.content.clone(),
        url: doc.url.clone(),
        match_type,
        doc_id: doc.id.clone(),
    }
}

/// v4 `resolveHelpContentForUrl(url)` over the document list. `None` when the
/// list is empty or nothing matches (v4 logs a warning on both).
pub fn resolve_help_content_for_url(
    url: &str,
    documents: &[HelpDocument],
) -> Option<HelpPageContext> {
    if documents.is_empty() {
        return None;
    }

    let (url_path, url_query) = split_url(url);
    // `new URLSearchParams(urlQuery || '')`.
    let url_params = parse_search_params(url_query.unwrap_or(""));
    // `if (urlQuery)` — JS truthiness: `undefined` and `''` are both falsy.
    let query_truthy = url_query.is_some_and(|q| !q.is_empty());

    // Strategy 1: exact match (path + query params).
    if query_truthy {
        if let Some(doc) = documents.iter().find(|d| d.url == url) {
            return Some(build_context(doc, MatchType::Exact));
        }
    }

    // Strategy 2: exact path match (ignoring query).
    let path_match = documents.iter().find(|d| {
        let (doc_path, _) = split_url(&d.url);
        doc_path == url_path && !d.url.contains(':')
    });
    if let Some(path_match) = path_match {
        // If there are query param matches too, prefer the most specific.
        if query_truthy {
            let mut query_matches: Vec<&HelpDocument> = documents
                .iter()
                .filter(|d| {
                    let (doc_path, doc_query) = split_url(&d.url);
                    // `if (docPath !== urlPath || !docQuery) return false`.
                    if doc_path != url_path || doc_query.is_none_or(|q| q.is_empty()) {
                        return false;
                    }
                    let doc_params = parse_search_params(doc_query.unwrap_or(""));
                    // Every doc param must match the URL's (`.get` → FIRST value;
                    // an absent key is `null !== value`).
                    doc_params
                        .iter()
                        .all(|(k, v)| params_get(&url_params, k) == Some(v.as_str()))
                })
                .collect();
            if !query_matches.is_empty() {
                // Most query params wins; a stable sort keeps document order on ties.
                let param_count =
                    |d: &HelpDocument| parse_search_params(split_url(&d.url).1.unwrap_or("")).len();
                query_matches.sort_by_key(|d| std::cmp::Reverse(param_count(d)));
                return Some(build_context(query_matches[0], MatchType::Query));
            }
        }
        return Some(build_context(path_match, MatchType::Exact));
    }

    // Strategy 3: pattern match (`/aurora/:id/edit` matches `/aurora/abc-123/edit`).
    let mut pattern_matches: Vec<&HelpDocument> = documents
        .iter()
        .filter(|d| {
            if !d.url.contains(':') {
                return false;
            }
            let (doc_path, _) = split_url(&d.url);
            match_url_pattern(doc_path, url_path)
        })
        .collect();
    if !pattern_matches.is_empty() {
        // Most segments wins — counted over the WHOLE url (v4 `b.url.split('/')`).
        let segments = |d: &HelpDocument| d.url.split('/').count();
        pattern_matches.sort_by_key(|d| std::cmp::Reverse(segments(d)));
        return Some(build_context(pattern_matches[0], MatchType::Pattern));
    }

    // Strategy 4: prefix match (`/settings` matches `/settings/something`).
    let mut prefix_matches: Vec<&HelpDocument> = documents
        .iter()
        .filter(|d| {
            let (doc_path, _) = split_url(&d.url);
            doc_path != "*" && url_path.starts_with(&format!("{doc_path}/"))
        })
        .collect();
    if !prefix_matches.is_empty() {
        // Longest url wins (`b.url.length` — UTF-16 units).
        prefix_matches.sort_by_key(|d| std::cmp::Reverse(utf16_len(&d.url)));
        return Some(build_context(prefix_matches[0], MatchType::Prefix));
    }

    // Strategy 5: wildcard (`url: '*'`) — the FIRST.
    if let Some(doc) = documents.iter().find(|d| d.url == "*") {
        return Some(build_context(doc, MatchType::Wildcard));
    }

    // Strategy 6: fallback to the homepage.
    if let Some(doc) = documents.iter().find(|d| d.url == "/") {
        return Some(build_context(doc, MatchType::Fallback));
    }

    None
}

/// v4 `resolveAllHelpContentForUrl(url)` — the primary match plus every wildcard
/// document. ⚠ v4's "don't add if it's already the primary" test is
/// `doc.id === primary.url` — an id compared against a URL, never true — so a
/// wildcard primary is pushed TWICE. Reproduced deliberately; see the module doc.
pub fn resolve_all_help_content_for_url(
    url: &str,
    documents: &[HelpDocument],
) -> Vec<HelpPageContext> {
    let mut results = Vec::new();
    let primary = resolve_help_content_for_url(url, documents);
    if let Some(p) = &primary {
        results.push(p.clone());
    }
    // v4 gates this on `helpSearch.isLoaded()`, which the load above guarantees.
    for doc in documents.iter().filter(|d| d.url == "*") {
        if let Some(p) = &primary {
            if doc.id == p.url {
                continue;
            }
        }
        results.push(build_context(doc, MatchType::Wildcard));
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(id: &str, url: &str) -> HelpDocument {
        HelpDocument {
            id: id.to_string(),
            slug: id.to_string(),
            title: format!("T {id}"),
            path: format!("help/{id}.md"),
            url: url.to_string(),
            content: format!("C {id}"),
        }
    }

    #[test]
    fn split_url_drops_the_third_part_like_js() {
        assert_eq!(split_url("/a?b?c"), ("/a", Some("b")));
        assert_eq!(split_url("/a"), ("/a", None));
        assert_eq!(split_url("/a?"), ("/a", Some("")));
    }

    #[test]
    fn search_params_are_whatwg_shaped() {
        assert_eq!(
            parse_search_params("tab=chat&section=a+b&x=%41&&y"),
            vec![
                ("tab".into(), "chat".into()),
                ("section".into(), "a b".into()),
                ("x".into(), "A".into()),
                ("y".into(), String::new()),
            ]
        );
        let p = parse_search_params("k=1&k=2");
        assert_eq!(params_get(&p, "k"), Some("1"));
        assert_eq!(p.len(), 2);
    }

    #[test]
    fn duplicate_wildcard_quirk_is_reproduced() {
        let docs = vec![doc("side", "*"), doc("home", "/")];
        let all = resolve_all_help_content_for_url("/nowhere/at/all", &docs);
        // The primary is the wildcard (strategy 5), and the id-vs-url compare
        // never dedups it: pushed twice.
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].doc_id, "side");
        assert_eq!(all[1].doc_id, "side");
    }
}
