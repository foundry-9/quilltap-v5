//! Port of v4 `lib/mount-index/converters/` — file → plaintext conversion for
//! the scanner / reindex / store-file pipelines.
//!
//! - `markdown` (fs path): strip YAML frontmatter + markdown syntax
//!   (`markdown-converter.ts` — regexes reproduced JS-faithfully, incl. the two
//!   backreference patterns hand-rolled below).
//! - `txt` / `json` / `jsonl` (fs path): read-and-return, `''` on any error.
//! - **Buffer path asymmetry (deliberate, v4 `convertBufferToPlainText`):**
//!   text-native buffers return the RAW utf-8 text (NO frontmatter/syntax
//!   strip) — reindex chunks raw text while the fs scan chunks stripped text.
//! - `pdf` / `docx`: the [`DocumentTextExtractor`] seam. v4's converters NEVER
//!   throw — any failure returns `''` — so the refusing default routes through
//!   exactly v4's empty-text bookkeeping arms ("extractor returned empty text" /
//!   "Converter produced no text" / scan `empty`), keeping the observable DB
//!   state v4-shaped while the refusal itself is loud (stderr, once per file).

use std::sync::Arc;

use crate::jsstr::{is_js_ws, js_trim, JS_WS_CLASS};

/// The pdf/docx → plaintext seam (tier-3 deferral of P4.6v/P4.6y): the
/// production extractor (pdf/docx crates or a host-side converter) is a named
/// future order. Implementations mirror v4's converter contract: return the
/// extracted text, or `''` when nothing could be extracted (v4's converters
/// swallow their own errors into `''`).
pub trait DocumentTextExtractor: Send + Sync {
    /// Extract plaintext from raw `pdf`/`docx` bytes. `''` = nothing extracted.
    fn extract(&self, bytes: &[u8], file_type: &str) -> String;
}

/// The default extractor: refuses loudly (stderr names the deferring order) and
/// returns `''`, which the call sites bookkeep exactly like v4's
/// converter-produced-no-text arms — never a silent skip.
pub struct RefusingTextExtractor;

impl DocumentTextExtractor for RefusingTextExtractor {
    fn extract(&self, _bytes: &[u8], file_type: &str) -> String {
        eprintln!(
            "DocumentTextExtractor unavailable — refusing {file_type} text extraction \
             (the production pdf/docx extractor is deferred by work order P4.6y); \
             the file will be bookkept as extraction-failed, not silently skipped"
        );
        String::new()
    }
}

/// A shared handle to the extractor seam (the default refuses loudly).
pub type SharedTextExtractor = Arc<dyn DocumentTextExtractor>;

/// The default seam instance.
pub fn default_text_extractor() -> SharedTextExtractor {
    Arc::new(RefusingTextExtractor)
}

// ---------------------------------------------------------------------------
// stripFrontmatter (markdown-converter.ts)
// ---------------------------------------------------------------------------

/// v4 `stripFrontmatter`: YAML frontmatter delimited by `---` at the very start.
fn strip_frontmatter(text: &str) -> &str {
    if !text.starts_with("---") {
        return text;
    }
    // indexOf('\n---', 3)
    let hay = &text.as_bytes()[3..];
    let needle = b"\n---";
    let end = hay
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + 3);
    let end = match end {
        Some(e) => e,
        None => return text,
    };
    // slice(end + 4) then strip ONE leading '\n' (v4 `.replace(/^\n/, '')`).
    let rest = &text[end + 4..];
    rest.strip_prefix('\n').unwrap_or(rest)
}

// ---------------------------------------------------------------------------
// stripMarkdownSyntax (markdown-converter.ts) — pass-by-pass, in v4's order
// ---------------------------------------------------------------------------

/// Is `i` a JS-multiline line start (start of string or right after `\n`)?
/// (v4's corpora are `\n`-terminated; JS also treats `\r`/U+2028/U+2029 as
/// line terminators — out of scope for mount corpora, noted here.)
fn at_line_start(chars: &[char], i: usize) -> bool {
    i == 0 || chars[i - 1] == '\n'
}

/// v4 pass 1: `/^(`{3,}|~{3,})[^\n]*\n([\s\S]*?)^\1\s*$/gm` → `'$2'`.
/// Hand-rolled (the regex crate has no backreferences), reproducing the JS
/// engine's lazy-content + greedy-`\s*$`-backtracking behavior.
fn strip_code_fences(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut out = String::new();
    let mut i = 0; // scan position (chars)
    while i < n {
        // Find the next line start at-or-after i that opens a fence.
        let mut m: Option<(usize, usize, usize, usize)> = None; // (start, content_start, content_end, match_end)
        let mut j = i;
        while j < n {
            if at_line_start(&chars, j) && (chars[j] == '`' || chars[j] == '~') {
                let fence_char = chars[j];
                let mut run = 0;
                while j + run < n && chars[j + run] == fence_char {
                    run += 1;
                }
                if run >= 3 {
                    // `{3,}` is greedy: the capture is the WHOLE run.
                    let fence_len = run;
                    // [^\n]* then a literal '\n'.
                    let mut k = j + run;
                    while k < n && chars[k] != '\n' {
                        k += 1;
                    }
                    if k < n {
                        let content_start = k + 1;
                        // Lazy content: the EARLIEST line start where ^\1\s*$ matches.
                        let mut p = content_start;
                        'close: loop {
                            if p > n {
                                break;
                            }
                            if at_line_start(&chars, p)
                                && p + fence_len <= n
                                && chars[p..p + fence_len].iter().all(|&c| c == fence_char)
                            {
                                // Greedy \s* then $ (before '\n' or at EOS),
                                // backtracking to the longest position that
                                // satisfies $.
                                let ws_start = p + fence_len;
                                let mut ws_end = ws_start;
                                while ws_end < n && is_js_ws(chars[ws_end]) {
                                    ws_end += 1;
                                }
                                let mut q = ws_end;
                                loop {
                                    if q == n || chars[q] == '\n' {
                                        m = Some((j, content_start, p, q));
                                        break 'close;
                                    }
                                    if q == ws_start {
                                        break;
                                    }
                                    q -= 1;
                                }
                            }
                            // Advance to the next line start.
                            match chars[p..].iter().position(|&c| c == '\n') {
                                Some(off) => p = p + off + 1,
                                None => break,
                            }
                        }
                        if m.is_some() {
                            break;
                        }
                    }
                }
            }
            j += 1;
        }
        match m {
            Some((start, cs, ce, end)) => {
                out.extend(&chars[i..start]);
                out.extend(&chars[cs..ce]);
                i = end.max(start + 1);
            }
            None => {
                out.extend(&chars[i..]);
                break;
            }
        }
    }
    out
}

/// v4 pass 9 (order preserved below): the bold/italic backreference pattern
/// `/(\*{1,3}|_{1,3})([^\s*_](?:.*?[^\s*_])?)(\1)/g` → `'$2'`, hand-rolled.
fn strip_emphasis(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let is_g2_edge = |c: char| !is_js_ws(c) && c != '*' && c != '_';
    let mut out = String::new();
    let mut i = 0;
    while i < n {
        let c = chars[i];
        if c != '*' && c != '_' {
            out.push(c);
            i += 1;
            continue;
        }
        // Opener: `\*{1,3}` (or `_{1,3}`) tries the longest first, backtracking
        // down to 1.
        let mut run = 0;
        while i + run < n && chars[i + run] == c && run < 3 {
            run += 1;
        }
        let mut matched: Option<(usize, usize, usize)> = None; // (g2_start, g2_end, match_end)
        'opener: for len in (1..=run).rev() {
            let g2_start = i + len;
            if g2_start >= n || !is_g2_edge(chars[g2_start]) {
                continue;
            }
            // (?:.*?[^\s*_])? — lazy: try the empty middle first (group2 = one
            // char), then grow. `.` excludes '\n'.
            // Empty option: closer immediately after the single char.
            let single_end = g2_start + 1;
            if single_end + len <= n && chars[single_end..single_end + len].iter().all(|&x| x == c)
            {
                matched = Some((g2_start, single_end, single_end + len));
                break 'opener;
            }
            // Growing middle: group2 = chars[g2_start..=e] where the middle
            // chars are `.` (non-newline) and chars[e] passes the edge class.
            let mut e = g2_start + 1;
            while e < n && chars[e] != '\n' {
                if is_g2_edge(chars[e]) {
                    let close_start = e + 1;
                    if close_start + len <= n
                        && chars[close_start..close_start + len]
                            .iter()
                            .all(|&x| x == c)
                    {
                        matched = Some((g2_start, close_start, close_start + len));
                        break 'opener;
                    }
                }
                e += 1;
            }
        }
        match matched {
            Some((g2s, g2e, end)) => {
                out.extend(&chars[g2s..g2e]);
                i = end;
            }
            None => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// The remaining v4 passes, in order, via the regex crate (patterns adjusted
/// for JS fidelity: explicit JS-`\s` class, `[0-9]` for JS `\d`).
fn strip_markdown_syntax(text: &str) -> String {
    use regex::Regex;
    use std::sync::OnceLock;

    // JS \s as an explicit class (regex-crate \s lacks U+FEFF) — the shared
    // `jsstr` class string (already bracketed).
    let ws = JS_WS_CLASS;

    struct Res {
        image: Regex,
        link: Regex,
        ref_link: Regex,
        ref_def: Regex,
        html: Regex,
        heading: Regex,
        inline_code: Regex,
        hrule: Regex,
        blockquote: Regex,
        ulist: Regex,
        olist: Regex,
        blanks: Regex,
    }
    static RES: OnceLock<Res> = OnceLock::new();
    let r = RES.get_or_init(|| Res {
        image: Regex::new(r"!\[([^\]]*)\]\([^)]*\)").unwrap(),
        link: Regex::new(r"\[([^\]]*)\]\([^)]*\)").unwrap(),
        ref_link: Regex::new(r"\[([^\]]*)\]\[[^\]]*\]").unwrap(),
        ref_def: Regex::new(&format!(r"(?m)^\[[^\]]*\]:{ws}+.*$")).unwrap(),
        html: Regex::new(r"<[^>]+>").unwrap(),
        heading: Regex::new(&format!(r"(?m)^#{{1,6}}{ws}+")).unwrap(),
        inline_code: Regex::new(r"`([^`]+)`").unwrap(),
        hrule: Regex::new(&format!(r"(?m)^[-*_]{{3,}}{ws}*$")).unwrap(),
        blockquote: Regex::new(&format!(r"(?m)^>{ws}?")).unwrap(),
        ulist: Regex::new(&format!(r"(?m)^[ \t]*[-*+]{ws}+")).unwrap(),
        olist: Regex::new(&format!(r"(?m)^[ \t]*[0-9]+\.{ws}+")).unwrap(),
        blanks: Regex::new(r"\n{3,}").unwrap(),
    });

    // v4's pass order (strip_code_fences ran before this fn is called).
    let s = r.image.replace_all(text, "$1");
    let s = r.link.replace_all(&s, "$1");
    let s = r.ref_link.replace_all(&s, "$1");
    let s = r.ref_def.replace_all(&s, "");
    let s = r.html.replace_all(&s, "");
    let s = r.heading.replace_all(&s, "");
    let s = strip_emphasis(&s);
    let s = r.inline_code.replace_all(&s, "$1");
    let s = r.hrule.replace_all(&s, "");
    let s = r.blockquote.replace_all(&s, "");
    let s = r.ulist.replace_all(&s, "");
    let s = r.olist.replace_all(&s, "");
    let s = r.blanks.replace_all(&s, "\n\n");
    js_trim(&s).to_string()
}

// ---------------------------------------------------------------------------
// Public converters
// ---------------------------------------------------------------------------

/// v4 `convertMarkdownToText` applied to already-read text (the fs converter
/// reads the file itself; the Rust call sites read via `std::fs` and pass the
/// text in so error handling stays at the caller's `''`-on-read-error arm).
pub fn markdown_to_text(raw: &str) -> String {
    if js_trim(raw).is_empty() {
        return String::new();
    }
    let without_frontmatter = strip_frontmatter(raw);
    let defenced = strip_code_fences(without_frontmatter);
    strip_markdown_syntax(&defenced)
}

/// v4 `convertToPlainText(absolutePath, fileType)` — the filesystem-path
/// converter dispatch. Any read failure returns `''` (each v4 converter
/// swallows its own errors into `''`).
pub fn convert_to_plain_text(
    absolute_path: &str,
    file_type: &str,
    extractor: &dyn DocumentTextExtractor,
) -> String {
    match file_type {
        "markdown" => match std::fs::read_to_string(absolute_path) {
            Ok(raw) => markdown_to_text(&raw),
            Err(_) => String::new(),
        },
        "txt" | "json" | "jsonl" => {
            // v4 convertTxtToText: read utf-8, '' on failure. Node's
            // readFile('utf-8') replaces invalid sequences rather than failing;
            // mirror with lossy decoding.
            match std::fs::read(absolute_path) {
                Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                Err(_) => String::new(),
            }
        }
        "pdf" | "docx" => match std::fs::read(absolute_path) {
            Ok(bytes) => extractor.extract(&bytes, file_type),
            Err(_) => String::new(),
        },
        _ => String::new(),
    }
}

/// v4 `convertBufferToPlainText(buffer, fileType)` — the in-memory dispatch.
/// Text-native types return the RAW utf-8 text (deliberately NO
/// frontmatter/markdown strip — reindex chunks raw text; the fs scan chunks
/// stripped text). pdf/docx go through the extractor seam.
pub fn convert_buffer_to_plain_text(
    bytes: &[u8],
    file_type: &str,
    extractor: &dyn DocumentTextExtractor,
) -> String {
    match file_type {
        "markdown" | "txt" | "json" | "jsonl" => String::from_utf8_lossy(bytes).into_owned(),
        "pdf" | "docx" => extractor.extract(bytes, file_type),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_strips_only_leading_block() {
        assert_eq!(
            strip_frontmatter("---\ntitle: x\n---\nBody"),
            "Body",
            "closing --- plus one newline consumed"
        );
        assert_eq!(strip_frontmatter("No frontmatter"), "No frontmatter");
        assert_eq!(strip_frontmatter("---\nunclosed"), "---\nunclosed");
    }

    #[test]
    fn code_fences_keep_content() {
        let s = "before\n```rust\nlet x = 1;\n```\nafter";
        assert_eq!(strip_code_fences(s), "before\nlet x = 1;\n\nafter");
    }

    #[test]
    fn emphasis_unwraps() {
        assert_eq!(strip_emphasis("**bold** and *it*"), "bold and it");
        assert_eq!(strip_emphasis("__deep__"), "deep");
        assert_eq!(strip_emphasis("a * b"), "a * b");
    }
}
