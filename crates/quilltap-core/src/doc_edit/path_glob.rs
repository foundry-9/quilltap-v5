//! Glob matching for `doc_list_files`' `pattern` (bug 261) — v4
//! `lib/doc-edit/path-glob.ts` (`40fab1c54`).
//!
//! The tool's schema promises patterns like `*.md`, `*.{ts,tsx}` and
//! `src/**/*.js`; the old matcher understood only `*.ext` and an exact file
//! name, so `**/CHANGELOG.md` matched nothing anywhere. This compiles a glob
//! to a RegExp once per call.
//!
//! Rules (v4's, verbatim):
//!  - A pattern with no `/` matches a file's name in any folder (`*.md`,
//!    `CHANGELOG.md`), the way the tool always behaved for `*.ext`.
//!  - A pattern with a `/` matches the whole store-relative path.
//!  - `**` spans folders, and `**/` may match no folder at all, so
//!    `**/CHANGELOG.md` finds a file at the root too. `*` and `?` stay within
//!    one path segment. `{a,b}` is alternation; `[abc]` / `[!abc]` are classes.
//!  - Matching is case-insensitive: store names are case-insensitive, and a
//!    model's guess at a file's casing should not hide it.
//!
//! v4 emits a JS RegExp SOURCE and compiles it with `new RegExp(src, 'i')`;
//! this port generates the same source byte-for-byte and compiles it through
//! [`crate::db::regexp_function::compile_sql_regexp`] — the workspace's
//! V8-faithful `new RegExp` twin over `regress` (UTF-16 semantics without
//! `u`, V8's own refusals), so the engine is v4's dialect and nothing is
//! re-derived (P4.D354 R-3). Pure.

use crate::db::regexp_function::{compile_sql_regexp, CompiledRegexp};
use crate::jsstr::js_trim;

/// v4 `globToRegExpSource`: translate a glob to the body of an anchored
/// RegExp.
///
/// v4 indexes `glob[i]` by UTF-16 code unit, but every branch tests an ASCII
/// character and passes anything else through the metacharacter escape — the
/// identity on non-ASCII — so walking Rust `char`s yields the same source (an
/// astral character's two units are re-joined either way; pinned by a row).
pub fn glob_to_regexp_source(glob: &str) -> String {
    let chars: Vec<char> = glob.chars().collect();
    let mut out = String::new();
    let mut brace_depth = 0usize;
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c == '*' {
            if chars.get(i + 1) == Some(&'*') {
                let at_segment_start = i == 0 || chars[i - 1] == '/';
                i += 1; // consume the second '*'
                if chars.get(i + 1) == Some(&'/') && at_segment_start {
                    i += 1; // consume the '/': `**/` is zero or more whole folders
                    out.push_str("(?:[^/]*/)*");
                } else {
                    out.push_str(".*");
                }
            } else {
                out.push_str("[^/]*");
            }
        } else if c == '?' {
            out.push_str("[^/]");
        } else if c == '[' {
            match chars[i + 1..].iter().position(|&x| x == ']') {
                None => out.push_str("\\["),
                Some(off) => {
                    let close = i + 1 + off;
                    let mut body: String = chars[i + 1..close].iter().collect();
                    if let Some(rest) = body.strip_prefix('!') {
                        body = format!("^{rest}");
                    }
                    out.push('[');
                    out.push_str(&body.replace('\\', "\\\\"));
                    out.push(']');
                    i = close;
                }
            }
        } else if c == '{' {
            brace_depth += 1;
            out.push_str("(?:");
        } else if c == '}' && brace_depth > 0 {
            brace_depth -= 1;
            out.push(')');
        } else if c == ',' && brace_depth > 0 {
            out.push('|');
        } else {
            // `c.replace(/[.+^${}()|[\]\\]/g, '\\$&')`
            if ".+^${}()|[]\\".contains(c) {
                out.push('\\');
            }
            out.push(c);
        }
        i += 1;
    }
    // An unclosed brace: close it so the RegExp still compiles.
    for _ in 0..brace_depth {
        out.push(')');
    }
    out
}

enum Matcher {
    /// An empty pattern matches everything.
    All,
    Regex(CompiledRegexp),
    /// A pattern that cannot compile matches only its literal text
    /// (lower-cased; compared case-insensitively).
    Literal(String),
}

/// v4 `compilePathGlob(pattern)` — a compiled predicate over store-relative
/// paths (`/` separators, no leading slash).
pub struct PathGlob {
    matcher: Matcher,
    matches_basename: bool,
}

impl PathGlob {
    /// Compile `pattern`. An empty pattern matches everything; a pattern that
    /// cannot compile matches only its literal text.
    pub fn compile(pattern: &str) -> PathGlob {
        let normalised = normalise_pattern(pattern);
        if normalised.is_empty() {
            return PathGlob {
                matcher: Matcher::All,
                matches_basename: false,
            };
        }
        let matches_basename = !normalised.contains('/');
        let source = format!("^{}$", glob_to_regexp_source(&normalised));
        let matcher = match compile_sql_regexp(&source, "i") {
            Ok(re) => Matcher::Regex(re),
            Err(_) => Matcher::Literal(normalised.to_lowercase()),
        };
        PathGlob {
            matcher,
            matches_basename,
        }
    }

    /// Whether `relative_path` matches. A slash-less pattern tests the LAST
    /// segment; otherwise the whole path (`\` → `/`, leading `/`s stripped).
    pub fn matches(&self, relative_path: &str) -> bool {
        match &self.matcher {
            Matcher::All => true,
            Matcher::Literal(literal) => {
                // v4's literal arm picks its target WITHOUT the normalisation.
                let target = if self.matches_basename {
                    last_segment(relative_path)
                } else {
                    relative_path
                };
                target.to_lowercase() == *literal
            }
            Matcher::Regex(re) => {
                let p = strip_leading_slashes(&relative_path.replace('\\', "/")).to_string();
                let target = if self.matches_basename {
                    last_segment(&p)
                } else {
                    &p
                };
                re.test(target)
            }
        }
    }

    /// Whether the pattern fell to the literal arm (a source that would not
    /// compile) — the grid's count.
    pub fn is_literal(&self) -> bool {
        matches!(self.matcher, Matcher::Literal(_))
    }
}

/// `pattern.trim().replace(/\\/g, '/').replace(/^(?:\.\/)+/, '').replace(/^\/+/, '')`.
fn normalise_pattern(pattern: &str) -> String {
    let mut s = js_trim(pattern).replace('\\', "/");
    while let Some(rest) = s.strip_prefix("./") {
        s = rest.to_string();
    }
    strip_leading_slashes(&s).to_string()
}

fn strip_leading_slashes(s: &str) -> &str {
    s.trim_start_matches('/')
}

/// `p.split('/').pop() ?? ''`.
fn last_segment(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(pattern: &str, path: &str) -> bool {
        PathGlob::compile(pattern).matches(path)
    }

    // The eight `it`s of v4 `lib/doc-edit/__tests__/path-glob.test.ts`, every
    // `expect` a row.
    #[test]
    fn an_empty_pattern_matches_everything() {
        assert!(m("", "a/b.md"));
    }

    #[test]
    fn doublestar_slash_may_match_no_folder_at_all() {
        assert!(m("**/CHANGELOG.md", "CHANGELOG.md"));
        assert!(m("**/CHANGELOG.md", "docs/CHANGELOG.md"));
        assert!(m("**/CHANGELOG.md", "a/b/c/CHANGELOG.md"));
        assert!(!m("**/CHANGELOG.md", "docs/OLD-CHANGELOG.md"));
    }

    #[test]
    fn a_pattern_without_a_slash_matches_the_file_name_in_any_folder() {
        assert!(m("*.md", "notes.md"));
        assert!(m("*.md", "deep/folder/notes.md"));
        assert!(m("CHANGELOG.md", "docs/CHANGELOG.md"));
        assert!(!m("*.md", "notes.mdx"));
    }

    #[test]
    fn a_pattern_with_a_slash_matches_the_whole_path_star_within_one_segment() {
        assert!(m("docs/*.md", "docs/a.md"));
        assert!(!m("docs/*.md", "docs/sub/a.md"));
        assert!(m("src/**/*.js", "src/a.js"));
        assert!(m("src/**/*.js", "src/x/y/a.js"));
        assert!(!m("src/**/*.js", "lib/a.js"));
        assert!(m("docs/**", "docs/a/b.md"));
    }

    #[test]
    fn supports_braces_question_and_character_classes() {
        assert!(m("*.{ts,tsx}", "a.tsx"));
        assert!(!m("*.{ts,tsx}", "a.js"));
        assert!(m("v?.md", "v2.md"));
        assert!(!m("v?.md", "v10.md"));
        assert!(m("v[0-9].md", "v3.md"));
        assert!(!m("v[!0-9].md", "v3.md"));
    }

    #[test]
    fn is_case_insensitive_and_ignores_a_leading_dot_slash_or_slash() {
        assert!(m("changelog.MD", "CHANGELOG.md"));
        assert!(m("./docs/*.md", "docs/a.md"));
        assert!(m("/docs/*.md", "docs/a.md"));
    }

    #[test]
    fn treats_regex_metacharacters_literally() {
        assert!(m("notes (2).md", "notes (2).md"));
        assert!(!m("a+b.md", "aab.md"));
        assert!(!m("a.md", "aXmd"));
    }

    #[test]
    fn never_throws_on_a_malformed_pattern() {
        let _ = PathGlob::compile("[unclosed");
        assert!(m("{a,b", "a"));
        assert!(m("[unclosed", "x/[unclosed"));
    }

    #[test]
    fn an_astral_character_passes_through_unchanged() {
        assert_eq!(glob_to_regexp_source("📁/*.md"), "📁/[^/]*\\.md");
        assert!(m("📁/*.md", "📁/notes.md"));
        assert!(!m("📁/*.md", "📂/notes.md"));
    }

    /// Contract C4 (P4.D354 item 8): the `doc_list_files` `pattern` description
    /// gains v4's sentence through P4.D350's `CATALOG` regen — this lane never
    /// edits `data.rs`. Without `P4D350_CATALOG_LANDED` the test records the
    /// OLD bytes and passes; the unifier sets it at §S.9 and the NEW sentence
    /// must then be there.
    #[test]
    fn the_catalog_describes_the_glob_rules_once_p4d350_lands() {
        const OLD: &str = r#""pattern":{"type":"string","description":"Optional: glob pattern to filter results (e.g., \"*.md\", \"*.{ts,tsx}\", \"src/**/*.js\")."}"#;
        const NEW: &str = r#"A pattern with no \"/\" matches file names in any folder; \"**/\" spans any number of folders, including none. Case-insensitive."#;
        let def = crate::tools::definitions::TOOL_DEFINITIONS
            .iter()
            .find(|d| d.name == "doc_list_files")
            .expect("doc_list_files in the catalog");
        if std::env::var_os("P4D350_CATALOG_LANDED").is_some() {
            assert!(
                def.json.contains(NEW),
                "the catalog lacks v4's glob sentence"
            );
        } else {
            assert!(
                def.json.contains(OLD),
                "the pre-P4.D350 catalog bytes moved"
            );
        }
    }

    #[test]
    fn the_source_generator_matches_v4s_bytes() {
        assert_eq!(
            glob_to_regexp_source("**/CHANGELOG.md"),
            "(?:[^/]*/)*CHANGELOG\\.md"
        );
        assert_eq!(glob_to_regexp_source("a**b"), "a.*b");
        assert_eq!(glob_to_regexp_source("[!a\\b]x"), "[^a\\\\b]x");
        assert_eq!(glob_to_regexp_source("{a,b"), "(?:a|b)");
        assert_eq!(glob_to_regexp_source("x}"), "x\\}");
    }
}
