//! Lightweight markdown heading + frontmatter ops — v4
//! `lib/doc-edit/markdown-parser.ts` (the heading half + the serialize/update
//! half). The `parseFrontmatter` reader half of that same v4 file is already
//! ported as [`crate::markdown::parse_frontmatter`]; this module reuses it.
//!
//! Not a full AST parser — just heading trees + frontmatter. All character
//! offsets are **UTF-16 code units** (JS `content.substring` / `line.length`).
//!
//! ## `serialize_frontmatter` (v4 `YAML.stringify`)
//!
//! v4's `serializeFrontmatter` calls eemeli/yaml's `YAML.stringify`. The port
//! reuses the already-ported eemeli scalar emitter
//! ([`crate::vault_overlay::yaml_stringify_scalar`]) over the **frontmatter value
//! space** — string / bool / number / null scalars and flat sequences of those —
//! matching `YAML.stringify` byte-for-byte on that space. **Documented seam**
//! (kept out of the corpus): nested maps, arrays-of-objects, exotic numbers, and
//! non-identifier keys render best-effort and are not proven byte-exact.

use crate::jsstr::{is_js_ws, js_trim, js_trim_start, utf16_len};
use crate::markdown::parse_frontmatter;
use crate::vault_overlay::yaml_stringify_scalar;
use serde_json::{Map, Value};

// --- Frontmatter serialize / update ---

/// JS `String(number)` for the frontmatter value space (integers + simple
/// decimals; Rust's f64 `Display` omits a trailing `.0` exactly like JS).
fn js_number_string(n: &serde_json::Number) -> String {
    if let Some(i) = n.as_i64() {
        i.to_string()
    } else if let Some(u) = n.as_u64() {
        u.to_string()
    } else {
        format!("{}", n.as_f64().unwrap_or(0.0))
    }
}

/// Render one frontmatter value at `indent` / `indent_at_start` on the value side
/// of `key: ` (scalars) — v4's `YAML.stringify` scalar path.
fn render_scalar(value: &Value, indent: &str, indent_at_start: usize) -> String {
    match value {
        Value::String(s) => yaml_stringify_scalar(s, indent, indent_at_start),
        Value::Bool(b) => (if *b { "true" } else { "false" }).to_string(),
        Value::Number(n) => js_number_string(n),
        Value::Null => "null".to_string(),
        // Nested containers as a sequence item are the documented seam.
        other => other.to_string(),
    }
}

/// Serialize a data object to a YAML frontmatter string WITH `---` delimiters
/// (v4 `serializeFrontmatter` = `---\n${YAML.stringify(data)}---\n`).
pub fn serialize_frontmatter(data: &Map<String, Value>) -> String {
    format!("---\n{}---\n", yaml_stringify(data))
}

/// v4 `YAML.stringify(data)` for the frontmatter value space: a top-level block
/// map, insertion order, trailing newline.
fn yaml_stringify(data: &Map<String, Value>) -> String {
    if data.is_empty() {
        // YAML.stringify({}) === "{}\n".
        return "{}\n".to_string();
    }
    let mut parts: Vec<String> = Vec::new();
    for (key, value) in data {
        match value {
            Value::Array(items) if !items.is_empty() => {
                let mut entry = format!("{key}:");
                for item in items {
                    entry.push_str("\n  - ");
                    entry.push_str(&render_scalar(item, "    ", 4));
                }
                parts.push(entry);
            }
            Value::Array(_) => parts.push(format!("{key}: []")),
            Value::Object(m) if m.is_empty() => parts.push(format!("{key}: {{}}")),
            _ => {
                let ias = utf16_len(key) + 2; // "<key>: "
                parts.push(format!("{key}: {}", render_scalar(value, "  ", ias)));
            }
        }
    }
    let mut out = parts.join("\n");
    out.push('\n');
    out
}

/// Update frontmatter in file content (v4 `updateFrontmatterInContent`). Merges
/// `updates` into the existing frontmatter (a `Null` value deletes the key), or
/// replaces it entirely when `replace_all`. Reuses the ported reader for the
/// existing block + body split.
pub fn update_frontmatter_in_content(
    content: &str,
    updates: &Map<String, Value>,
    replace_all: bool,
) -> String {
    let parsed = parse_frontmatter(content);

    let new_data: Map<String, Value> = if replace_all {
        updates.clone()
    } else {
        let mut merged: Map<String, Value> = match &parsed.data {
            Some(Value::Object(m)) => m.clone(),
            _ => Map::new(),
        };
        for (key, value) in updates {
            if value.is_null() {
                merged.remove(key);
            } else {
                merged.insert(key.clone(), value.clone());
            }
        }
        merged
    };

    let new_frontmatter = serialize_frontmatter(&new_data);
    // Body = content from the reader's UTF-16 bodyStartOffset onward.
    let body = utf16_slice(content, parsed.body_start_offset, utf16_len(content));
    format!("{new_frontmatter}{body}")
}

// --- Headings ---

/// A parsed heading (v4 `HeadingInfo`). All offsets are UTF-16 code units.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadingInfo {
    pub text: String,
    pub level: usize,
    pub slug: String,
    pub line: usize,
    pub offset: usize,
    pub content_start: usize,
    pub content_end: usize,
}

/// Slugify heading text for stable IDs (v4 `slugifyHeading`). JS `\w` is ASCII
/// `[A-Za-z0-9_]`; `\s` is the JS whitespace set (`is_js_ws`).
pub fn slugify_heading(text: &str) -> String {
    // toLowerCase → trim → remove [^\w\s-] → \s+ to '-' → collapse '-' → trim '-'.
    let lowered = text.to_lowercase();
    let trimmed = js_trim(&lowered);

    // Step 1: remove any char not word / whitespace / hyphen.
    let kept: String = trimmed
        .chars()
        .filter(|&c| c.is_ascii_alphanumeric() || c == '_' || is_js_ws(c) || c == '-')
        .collect();

    // Steps 2+3: collapse each run of (whitespace | hyphen) into a single '-'.
    let mut out = String::with_capacity(kept.len());
    let mut in_sep = false;
    for c in kept.chars() {
        if is_js_ws(c) || c == '-' {
            if !in_sep {
                out.push('-');
                in_sep = true;
            }
        } else {
            out.push(c);
            in_sep = false;
        }
    }
    // Step 4: trim leading/trailing '-'.
    out.trim_matches('-').to_string()
}

/// Detect an ATX heading line `^(#{1,6})\s+(.+)$` — returns `(level, trimmed
/// text)` or `None`. `\s` is the JS whitespace set; the text is `js_trim`med.
fn match_heading(line: &str) -> Option<(usize, String)> {
    let chars: Vec<char> = line.chars().collect();
    let mut hashes = 0usize;
    while hashes < chars.len() && chars[hashes] == '#' {
        hashes += 1;
    }
    if !(1..=6).contains(&hashes) {
        return None;
    }
    // `\s+`: at least one whitespace char immediately after the hashes.
    if hashes >= chars.len() || !is_js_ws(chars[hashes]) {
        return None;
    }
    let mut idx = hashes;
    while idx < chars.len() && is_js_ws(chars[idx]) {
        idx += 1;
    }
    // `(.+)`: at least one remaining char.
    if idx >= chars.len() {
        return None;
    }
    let rest: String = chars[idx..].iter().collect();
    Some((hashes, js_trim(&rest).to_string()))
}

/// Parse all headings into a tree with section ranges (v4 `parseHeadingTree`).
/// Headings inside fenced code blocks (``` or ~~~) are ignored; duplicate slugs
/// get counter suffixes.
pub fn parse_heading_tree(content: &str) -> Vec<HeadingInfo> {
    let lines: Vec<&str> = content.split('\n').collect();

    struct Raw {
        text: String,
        level: usize,
        raw_slug: String,
        slug: String,
        line: usize,
        offset: usize,
        content_start: usize,
        content_end: usize,
    }
    let mut headings: Vec<Raw> = Vec::new();

    let mut in_code_block = false;
    let mut code_block_delimiter = "";
    let mut current_offset = 0usize;

    for (i, line) in lines.iter().enumerate() {
        let trimmed_start = js_trim_start(line);
        let triple_backtick = trimmed_start.starts_with("```");
        let tilde_fence = trimmed_start.starts_with("~~~");

        if triple_backtick && (!in_code_block || code_block_delimiter == "```") {
            in_code_block = !in_code_block;
            code_block_delimiter = if in_code_block { "```" } else { "" };
        } else if tilde_fence && (!in_code_block || code_block_delimiter == "~~~") {
            in_code_block = !in_code_block;
            code_block_delimiter = if in_code_block { "~~~" } else { "" };
        }

        if !in_code_block {
            if let Some((level, text)) = match_heading(line) {
                let raw_slug = slugify_heading(&text);
                let line_len16 = utf16_len(line);
                headings.push(Raw {
                    text,
                    level,
                    slug: raw_slug.clone(),
                    raw_slug,
                    line: i,
                    offset: current_offset,
                    content_start: current_offset + line_len16 + 1,
                    content_end: 0, // filled below
                });
            }
        }

        current_offset += utf16_len(line) + 1;
    }

    // contentEnd: next heading with level <= this level, else EOF (current_offset).
    let count = headings.len();
    for i in 0..count {
        let mut next_offset = current_offset;
        for j in (i + 1)..count {
            if headings[j].level <= headings[i].level {
                next_offset = headings[j].offset;
                break;
            }
        }
        headings[i].content_end = next_offset;
    }

    // Duplicate slugs → counter suffixes (v4's two-pass scheme).
    let mut first_occurrence: Vec<String> = Vec::new();
    let mut slug_suffixes: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for h in headings.iter_mut() {
        if !first_occurrence.contains(&h.raw_slug) {
            first_occurrence.push(h.raw_slug.clone());
            h.slug = h.raw_slug.clone();
        } else {
            let suffix = *slug_suffixes.get(&h.raw_slug).unwrap_or(&2);
            h.slug = format!("{}-{}", h.raw_slug, suffix);
            slug_suffixes.insert(h.raw_slug.clone(), suffix + 1);
        }
    }

    headings
        .into_iter()
        .map(|h| HeadingInfo {
            text: h.text,
            level: h.level,
            slug: h.slug,
            line: h.line,
            offset: h.offset,
            content_start: h.content_start,
            content_end: h.content_end,
        })
        .collect()
}

/// The failure of [`find_heading_section`] (v4 throws an `Error` whose message is
/// user-facing). The message is byte-exact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadingNotFound(pub String);

/// Find a heading section by text and optional level (v4 `findHeadingSection`).
/// Case-insensitive, whitespace-insensitive text match; `level` disambiguates.
pub fn find_heading_section(
    content: &str,
    heading_text: &str,
    level: Option<usize>,
) -> Result<HeadingInfo, HeadingNotFound> {
    let headings = parse_heading_tree(content);
    let normalized_search = js_trim(&heading_text.to_lowercase()).to_string();

    let text_matches: Vec<&HeadingInfo> = headings
        .iter()
        .filter(|h| js_trim(&h.text.to_lowercase()) == normalized_search)
        .collect();

    if text_matches.is_empty() {
        let available: String = headings
            .iter()
            .map(|h| format!("- Level {}: \"{}\"", h.level, h.text))
            .collect::<Vec<_>>()
            .join("\n");
        return Err(HeadingNotFound(format!(
            "Heading \"{heading_text}\" not found. Available headings:\n{available}"
        )));
    }

    if let Some(level) = level {
        let level_matches: Vec<&&HeadingInfo> =
            text_matches.iter().filter(|h| h.level == level).collect();
        if level_matches.is_empty() {
            let available_for_text: String = text_matches
                .iter()
                .map(|h| format!("- Level {}", h.level))
                .collect::<Vec<_>>()
                .join("\n");
            return Err(HeadingNotFound(format!(
                "Heading \"{heading_text}\" found, but not at level {level}. Found at:\n{available_for_text}"
            )));
        }
        if level_matches.len() == 1 {
            return Ok((*level_matches[0]).clone());
        }
        return Err(HeadingNotFound(format!(
            "Multiple headings \"{heading_text}\" at level {level} found. Please specify level more precisely."
        )));
    }

    if text_matches.len() == 1 {
        return Ok(text_matches[0].clone());
    }
    let levels: String = text_matches
        .iter()
        .map(|h| h.level.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Err(HeadingNotFound(format!(
        "Multiple headings \"{heading_text}\" found at levels: {levels}. Please specify level to disambiguate."
    )))
}

/// Read all content under a heading (v4 `readHeadingContent`).
pub fn read_heading_content(content: &str, heading: &HeadingInfo) -> String {
    utf16_slice(content, heading.content_start, heading.content_end)
}

/// Replace all content under a heading (v4 `replaceHeadingContent`). When
/// `preserve_subheadings`, only the content before the first subheading is
/// replaced.
pub fn replace_heading_content(
    content: &str,
    heading: &HeadingInfo,
    new_content: &str,
    preserve_subheadings: bool,
) -> String {
    if !preserve_subheadings {
        return format!(
            "{}{}{}",
            utf16_slice(content, 0, heading.content_start),
            new_content,
            utf16_slice(content, heading.content_end, utf16_len(content))
        );
    }

    let headings = parse_heading_tree(content);
    let mut first_subheading_offset = heading.content_end;
    for other in &headings {
        if other.level > heading.level
            && other.offset >= heading.content_start
            && other.offset < heading.content_end
        {
            first_subheading_offset = other.offset;
            break;
        }
    }

    format!(
        "{}{}{}",
        utf16_slice(content, 0, heading.content_start),
        new_content,
        utf16_slice(content, first_subheading_offset, utf16_len(content))
    )
}

/// UTF-16 `String.prototype.substring(start, end)` (clamped; the callers never
/// pass start > end).
fn utf16_slice(s: &str, start: usize, end: usize) -> String {
    let units: Vec<u16> = s.encode_utf16().collect();
    let len = units.len();
    let start = start.min(len);
    let end = end.min(len);
    if start >= end {
        return String::new();
    }
    String::from_utf16_lossy(&units[start..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn slugify_basics() {
        assert_eq!(
            slugify_heading("Character Backstory"),
            "character-backstory"
        );
        assert_eq!(slugify_heading("  Hello, World!  "), "hello-world");
        assert_eq!(slugify_heading("A -- B"), "a-b");
    }

    #[test]
    fn heading_tree_and_read() {
        let content = "# A\nintro\n## B\nbody\n# C\ntail\n";
        let tree = parse_heading_tree(content);
        assert_eq!(tree.len(), 3);
        assert_eq!(tree[0].text, "A");
        let a = find_heading_section(content, "a", None).unwrap();
        assert_eq!(read_heading_content(content, &a), "intro\n## B\nbody\n");
    }

    #[test]
    fn serialize_and_update_frontmatter() {
        let mut m = Map::new();
        m.insert("title".into(), json!("My Doc"));
        m.insert("embed".into(), json!(false));
        assert_eq!(
            serialize_frontmatter(&m),
            "---\ntitle: My Doc\nembed: false\n---\n"
        );
    }
}
