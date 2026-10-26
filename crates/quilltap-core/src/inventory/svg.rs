//! Inventory SVG glyphs — the sanitiser every write passes an object's inline
//! `svg` through (v4 `lib/inventory/svg.ts`, `4fbdad186`; P4.D324).
//!
//! A glyph is an icon, not a document: it is rendered through an `<img>` data
//! URI (where no script can run anyway), but it is also stored, exported and
//! shown in the Scriptorium, so it is cleaned on write. The sanitiser is an
//! allowlist over a small tokenizer — no DOM on the server:
//!
//!   - the root must be `<svg>`; comments, CDATA, processing instructions and
//!     `<!DOCTYPE>` (and with it any entity declaration) are dropped;
//!   - only drawing elements survive; `script`, `style`, `foreignObject` and
//!     every element not on the list are dropped with their contents;
//!   - `on*` attributes are dropped, as is `href` / `xlink:href` unless it
//!     names a fragment (`#id`), any value carrying `javascript:` or a `url(`
//!     that is not `url(#…)`, and `style` carrying `url(` at all;
//!   - the result may be at most [`INVENTORY_SVG_MAX_BYTES`].
//!
//! Pure. Every delimiter the tokenizer scans for is ASCII, so it walks UTF-8
//! BYTE offsets where v4 walks UTF-16 units — the same positions, since an
//! ASCII byte never occurs inside a multi-byte sequence. JS `\s` is spelled
//! out ([`JS_WS`]) — it is NOT the regex crate's Unicode `\s` (JS adds
//! U+FEFF and drops U+0085). `/&(?![a-z0-9#]+;)/gi` has no lookahead in the
//! regex crate and is hand-matched ([`escape_amps`]).

use std::sync::LazyLock;

use regex::Regex;

use super::errors::InventorySvgError;
use super::types::INVENTORY_SVG_MAX_BYTES;
use crate::content_disposition::encode_uri_component;
use crate::jsstr::js_trim;

/// JS `\s` as a character-class body.
const JS_WS: &str = r"\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}";

const ALLOWED_ELEMENTS: &[&str] = &[
    "svg",
    "g",
    "path",
    "circle",
    "ellipse",
    "line",
    "polyline",
    "polygon",
    "rect",
    "title",
    "desc",
    "defs",
    "symbol",
    "use",
    "lineargradient",
    "radialgradient",
    "stop",
    "clippath",
    "mask",
    "pattern",
    "text",
    "tspan",
    "marker",
];

const HREF_ATTRIBUTES: &[&str] = &["href", "xlink:href"];

fn is_allowed(lower: &str) -> bool {
    ALLOWED_ELEMENTS.contains(&lower)
}

/// v4 `/[\s\u0000-\u001f]+/g`.
static COMPACT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"[{JS_WS}\x00-\x1F]+")).expect("compact regex"));
/// v4 `/url\(([^)]*)\)/g`.
static URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"url\(([^)]*)\)").expect("url regex"));
/// v4 `/^url\(['"]?#/`.
static URL_FRAGMENT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^url\(['"]?#"#).expect("fragment regex"));
/// v4 `ATTRIBUTE_RE`: `/([^\s=/>]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'>]+)))?/g`.
static ATTRIBUTE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r#"([^{JS_WS}=/>]+)(?:[{JS_WS}]*=[{JS_WS}]*(?:"([^"]*)"|'([^']*)'|([^{JS_WS}"'>]+)))?"#
    ))
    .expect("attribute regex")
});
/// v4 `/^[a-zA-Z_:][-a-zA-Z0-9_:.]*$/`.
static ATTRIBUTE_NAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z_:][-a-zA-Z0-9_:.]*$").expect("name regex"));
/// v4 `/^([a-zA-Z][-a-zA-Z0-9_:.]*)/`.
static ELEMENT_NAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([a-zA-Z][-a-zA-Z0-9_:.]*)").expect("element regex"));

/// True when an attribute and its value are safe to keep (v4 `keepAttribute`).
fn keep_attribute(name: &str, value: &str) -> bool {
    let lower = name.to_lowercase();
    if lower.starts_with("on") {
        return false;
    }
    let compact = COMPACT_RE.replace_all(value, "").to_lowercase();
    if compact.contains("javascript:") || compact.contains("data:") || compact.contains("vbscript:")
    {
        return false;
    }
    if HREF_ATTRIBUTES.contains(&lower.as_str()) {
        return js_trim(value).starts_with('#');
    }
    if lower == "style" {
        return !compact.contains("url(")
            && !compact.contains("expression(")
            && !compact.contains("@import");
    }
    // Paint and clip references may only point inside the glyph.
    URL_RE
        .find_iter(&compact)
        .all(|m| URL_FRAGMENT_RE.is_match(m.as_str()))
}

/// v4's `.replace(/&(?![a-z0-9#]+;)/gi, '&amp;')`: an `&` NOT followed by one
/// or more `[a-z0-9#]` (case-insensitive) and then `;` becomes `&amp;`.
fn escape_amps(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut last = 0usize;
    for (i, b) in bytes.iter().enumerate() {
        if *b != b'&' {
            continue;
        }
        let mut j = i + 1;
        while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'#') {
            j += 1;
        }
        let entity = j > i + 1 && j < bytes.len() && bytes[j] == b';';
        if !entity {
            out.push_str(&s[last..i]);
            out.push_str("&amp;");
            last = i + 1;
        }
    }
    out.push_str(&s[last..]);
    out
}

fn escape_attribute(value: &str) -> String {
    escape_amps(value)
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}

fn escape_text(text: &str) -> String {
    escape_amps(text).replace('<', "&lt;").replace('>', "&gt;")
}

/// Rebuild a start tag from its allowed attributes (v4 `cleanStartTag`).
fn clean_start_tag(name: &str, raw_attributes: &str, self_closing: bool) -> String {
    let mut kept: Vec<String> = Vec::new();
    for caps in ATTRIBUTE_RE.captures_iter(raw_attributes) {
        let attr = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let value = caps
            .get(2)
            .or_else(|| caps.get(3))
            .or_else(|| caps.get(4))
            .map(|m| m.as_str())
            .unwrap_or("");
        if !ATTRIBUTE_NAME_RE.is_match(attr) {
            continue;
        }
        if !keep_attribute(attr, value) {
            continue;
        }
        kept.push(format!("{attr}=\"{}\"", escape_attribute(value)));
    }
    let attrs = if kept.is_empty() {
        String::new()
    } else {
        format!(" {}", kept.join(" "))
    };
    format!("<{name}{attrs}{}", if self_closing { "/>" } else { ">" })
}

/// The index of the `>` closing a tag, skipping quoted attribute values.
fn find_tag_end(source: &[u8], from: usize) -> Option<usize> {
    let mut quote: Option<u8> = None;
    for (j, ch) in source.iter().enumerate().skip(from) {
        match quote {
            Some(q) => {
                if *ch == q {
                    quote = None;
                }
            }
            None => {
                if *ch == b'"' || *ch == b'\'' {
                    quote = Some(*ch);
                } else if *ch == b'>' {
                    return Some(j);
                }
            }
        }
    }
    None
}

fn last_index_of_name(stack: &[Option<String>], name: &str) -> Option<usize> {
    for k in (0..stack.len()).rev() {
        match &stack[k] {
            Some(entry) if entry.to_lowercase() == name => return Some(k),
            // A dropped element closes by its own name too; it was pushed as
            // None, so a close tag for a dropped element matches the nearest
            // None.
            None if !is_allowed(name) => return Some(k),
            _ => {}
        }
    }
    None
}

fn find_from(source: &str, needle: &str, from: usize) -> Option<usize> {
    source
        .get(from..)
        .and_then(|rest| rest.find(needle))
        .map(|p| p + from)
}

/// Clean an operator's SVG (v4 `sanitizeInventorySvg`). `Ok(None)` for an
/// absent or blank input, the cleaned markup otherwise; errs when the input
/// is not an `<svg>` or the result is over the cap.
pub fn sanitize_inventory_svg(input: Option<&str>) -> Result<Option<String>, InventorySvgError> {
    let Some(input) = input else {
        return Ok(None);
    };
    let source = js_trim(input);
    if source.is_empty() {
        return Ok(None);
    }
    let bytes = source.as_bytes();

    let mut out: Vec<String> = Vec::new();
    // Names of open elements; a None entry marks an element being dropped.
    let mut stack: Vec<Option<String>> = Vec::new();
    let mut dropping = 0usize;
    let mut saw_root = false;
    let mut i = 0usize;

    let not_svg = || InventorySvgError("An SVG glyph must start with an <svg> element".to_string());

    while i < source.len() {
        let Some(lt) = find_from(source, "<", i) else {
            if dropping == 0 {
                out.push(escape_text(&source[i..]));
            }
            break;
        };
        if lt > i && dropping == 0 {
            out.push(escape_text(&source[i..lt]));
        }

        let at = &source[lt..];
        if at.starts_with("<!--") {
            i = find_from(source, "-->", lt + 4).map_or(source.len(), |end| end + 3);
            continue;
        }
        if at.starts_with("<![CDATA[") {
            i = find_from(source, "]]>", lt + 9).map_or(source.len(), |end| end + 3);
            continue;
        }
        if at.starts_with("<?") {
            i = find_from(source, "?>", lt + 2).map_or(source.len(), |end| end + 2);
            continue;
        }
        if at.starts_with("<!") {
            // A DOCTYPE, with any internal subset in brackets.
            let mut depth: i64 = 0;
            let mut j = lt + 2;
            while j < bytes.len() {
                match bytes[j] {
                    b'[' => depth += 1,
                    b']' => depth -= 1,
                    b'>' if depth <= 0 => break,
                    _ => {}
                }
                j += 1;
            }
            i = j + 1;
            continue;
        }

        let Some(gt) = find_tag_end(bytes, lt + 1) else {
            break;
        };
        let raw_tag = &source[lt + 1..gt];
        i = gt + 1;

        if let Some(rest) = raw_tag.strip_prefix('/') {
            let name = js_trim(rest).to_lowercase();
            // Close back to the matching open element, ignoring a stray close.
            let Some(at) = last_index_of_name(&stack, &name) else {
                continue;
            };
            while stack.len() > at {
                match stack.pop().flatten() {
                    None => dropping -= 1,
                    Some(top) => {
                        if dropping == 0 {
                            out.push(format!("</{top}>"));
                        }
                    }
                }
            }
            continue;
        }

        let Some(name_match) = ELEMENT_NAME_RE.captures(raw_tag) else {
            continue;
        };
        let name = name_match.get(1).map(|m| m.as_str()).unwrap_or("");
        let lower = name.to_lowercase();
        let self_closing = raw_tag.ends_with('/');
        // v4 `rawTag.slice(name.length, selfClosing ? -1 : undefined)` — an
        // end before the start is `''`.
        let end = if self_closing {
            raw_tag.len() - 1
        } else {
            raw_tag.len()
        };
        let raw_attributes = if end > name.len() {
            &raw_tag[name.len()..end]
        } else {
            ""
        };

        if !saw_root {
            if lower != "svg" {
                return Err(not_svg());
            }
            saw_root = true;
        }

        if !is_allowed(&lower) {
            if !self_closing {
                stack.push(None);
                dropping += 1;
            }
            continue;
        }
        if dropping == 0 {
            out.push(clean_start_tag(name, raw_attributes, self_closing));
        }
        if !self_closing {
            stack.push(Some(name.to_string()));
        }
    }

    // Close anything left open.
    while let Some(top) = stack.pop() {
        match top {
            None => dropping = dropping.saturating_sub(1),
            Some(top) => out.push(format!("</{top}>")),
        }
    }

    if !saw_root {
        return Err(not_svg());
    }
    let cleaned = js_trim(&out.concat()).to_string();
    if cleaned.len() > INVENTORY_SVG_MAX_BYTES {
        return Err(InventorySvgError(format!(
            "An SVG glyph may be at most {} KB",
            INVENTORY_SVG_MAX_BYTES / 1024
        )));
    }
    Ok(Some(cleaned))
}

/// The `<img src>` a glyph renders through (v4 `inventorySvgDataUri`).
pub fn inventory_svg_data_uri(svg: &str) -> String {
    format!("data:image/svg+xml;utf8,{}", encode_uri_component(svg))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean(s: &str) -> String {
        sanitize_inventory_svg(Some(s)).unwrap().unwrap()
    }

    #[test]
    fn drops_scripts_handlers_and_foreign_links() {
        assert_eq!(
            clean(
                r#"<svg viewBox="0 0 1 1" onload="x()"><script>alert(1)</script><path d="M0 0"/></svg>"#
            ),
            r#"<svg viewBox="0 0 1 1"><path d="M0 0"/></svg>"#
        );
        assert_eq!(
            clean(r##"<svg><use xlink:href="#a"/><use href="http://x"/></svg>"##),
            r##"<svg><use xlink:href="#a"/><use/></svg>"##
        );
        assert_eq!(
            clean(
                r##"<svg><rect style="fill:url(#g)" fill="url(#g)"/><rect fill="url(x)"/></svg>"##
            ),
            r##"<svg><rect fill="url(#g)"/><rect/></svg>"##
        );
        assert_eq!(
            clean("<svg><!-- c --><g>a &amp; b & c</g></svg>"),
            "<svg><g>a &amp; b &amp; c</g></svg>"
        );
        assert_eq!(clean("<svg>"), "<svg></svg>");
        assert_eq!(
            sanitize_inventory_svg(Some("<div/>")).unwrap_err().0,
            "An SVG glyph must start with an <svg> element"
        );
        assert_eq!(sanitize_inventory_svg(Some("  ")).unwrap(), None);
        assert_eq!(sanitize_inventory_svg(None).unwrap(), None);
        let big = format!("<svg><desc>{}</desc></svg>", "x".repeat(16 * 1024));
        assert_eq!(
            sanitize_inventory_svg(Some(&big)).unwrap_err().0,
            "An SVG glyph may be at most 16 KB"
        );
        assert_eq!(
            inventory_svg_data_uri("<svg/>"),
            "data:image/svg+xml;utf8,%3Csvg%2F%3E"
        );
    }
}
