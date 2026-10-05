//! v4 `lib/services/file-content-extractor.ts` — "Extracts text content from
//! various file types for use in project context and LLM tool calls" (`p4.9k`,
//! P4.9K2): the AI Wizard's `document` source and Summon From Lore's
//! `sourceFileIds` both read a file through it.
//!
//! Ported whole: the image arm (the stored description, else the
//! `[Image: name (WxH)]` placeholder), the 10 MB ceiling, the storage
//! download, the PDF arm, the text/code arms with the 50,000-unit truncation,
//! and the `[Binary file: …]` placeholder.
//!
//! ## Recorded divergence — PDFs
//!
//! v4 reads a PDF through `convertPdfBufferToText` (`lib/mount-index/
//! converters/pdf-converter.ts`, pdf-parse 2.x) FIRST and runs its own regex
//! fallback (`extractPdfTextFallback`) only when that answers no text — empty,
//! whitespace, a throw, or an empty buffer, all of which the converter turns
//! into `''`. v5 reads through the same seam ([`DocumentTextExtractor`],
//! resolved in place by [`default_text_extractor`]), whose production default
//! REFUSES (the pdf/docx extractor is deferred by P4.6y — the refusal prints
//! one stderr line per PDF naming that order). So the two sides take the SAME
//! fallback arm, with the same bytes, the same lines and the same result,
//! whenever pdf-parse finds nothing — and differ ONLY where pdf-parse finds
//! text: there v4 answers the parsed text and v5 answers the fallback's scrape
//! of the same bytes (often nothing, so `(no text found)`).
//!
//! Pinned both ways by `file_content_extractor_equivalence` (the first family
//! over this module): `scripted_seam_matches_v4_on_every_row` scripts the seam
//! identically to v4's mocked `pdf-parse` and compares every row exactly;
//! `production_seam_diverges_only_where_pdf_parse_finds_text` runs the
//! production seam and asserts the divergence on exactly the four
//! parsed-text rows.
//!
//! **The premise this header used to rest on was false** (`a434c715b`, v4 bug
//! 177): before that commit v4 called pdf-parse 2.x's `PDFParse` class as the
//! 1.x function, threw `pdfParse is not a function` into its catch, and FAILED
//! EVERY PDF (`Failed to extract PDF content`) — while v5 always ran the
//! fallback. The family's oracle at the old baseline `52d6e7ecd` records
//! exactly that on all eleven PDF rows.
//!
//! [`DocumentTextExtractor`]: crate::services::mount_index::converters::DocumentTextExtractor

use std::sync::OnceLock;

use regex::Regex;
use serde::Serialize;

use crate::db::files::{FileEntry, FileFull};
use crate::db::runtime::Db;
use crate::format_bytes::format_bytes;
use crate::services::file_storage::{download_file, StorageBackend};
use crate::services::mount_index::converters::{default_text_extractor, SharedTextExtractor};

/// v4 `ExtractedContent`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedContent {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    pub content_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
}

impl ExtractedContent {
    fn failure(error: impl Into<String>) -> Self {
        ExtractedContent {
            success: false,
            content: None,
            content_type: "error",
            language: None,
            error: Some(error.into()),
            truncated: None,
        }
    }
}

/// v4 `MAX_CONTENT_LENGTH` — "Maximum content length to return (chars)"
/// (UTF-16 units).
pub const MAX_CONTENT_LENGTH: usize = 50000;

const TEXT_MIME_TYPES: &[&str] = &[
    "text/plain",
    "text/markdown",
    "text/csv",
    "text/html",
    "text/xml",
    "application/json",
    "application/xml",
];

/// v4 `CODE_EXTENSIONS` — extension → language hint (v4's order).
const CODE_EXTENSIONS: &[(&str, &str)] = &[
    (".ts", "typescript"),
    (".tsx", "typescript"),
    (".js", "javascript"),
    (".jsx", "javascript"),
    (".py", "python"),
    (".rb", "ruby"),
    (".rs", "rust"),
    (".go", "go"),
    (".java", "java"),
    (".c", "c"),
    (".cpp", "cpp"),
    (".h", "c"),
    (".hpp", "cpp"),
    (".cs", "csharp"),
    (".php", "php"),
    (".swift", "swift"),
    (".kt", "kotlin"),
    (".scala", "scala"),
    (".r", "r"),
    (".sql", "sql"),
    (".sh", "shell"),
    (".bash", "shell"),
    (".zsh", "shell"),
    (".yaml", "yaml"),
    (".yml", "yaml"),
    (".toml", "toml"),
    (".ini", "ini"),
    (".css", "css"),
    (".scss", "scss"),
    (".less", "less"),
    (".vue", "vue"),
    (".svelte", "svelte"),
];

const IMAGE_MIME_TYPES: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/gif",
    "image/webp",
    "image/svg+xml",
];

/// v4 `isTextMimeType`.
fn is_text_mime_type(mime_type: &str) -> bool {
    TEXT_MIME_TYPES.contains(&mime_type)
        || mime_type.starts_with("text/")
        || mime_type.contains("json")
        || mime_type.contains("xml")
}

/// v4 `isImageMimeType`.
fn is_image_mime_type(mime_type: &str) -> bool {
    IMAGE_MIME_TYPES.contains(&mime_type) || mime_type.starts_with("image/")
}

/// v4 `getLanguageFromFilename` — `filename.toLowerCase().match(/\.[^.]+$/)`
/// then the table.
fn language_from_filename(filename: &str) -> Option<&'static str> {
    let lower = filename.to_lowercase();
    let dot = lower.rfind('.')?;
    let ext = &lower[dot..];
    if ext.len() < 2 {
        return None; // `\.[^.]+$` needs at least one char after the dot
    }
    CODE_EXTENSIONS
        .iter()
        .find(|(e, _)| *e == ext)
        .map(|(_, lang)| *lang)
}

/// JS `s.length` / `s.slice(0, n)` in UTF-16 units.
fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}
fn utf16_prefix(s: &str, n: usize) -> String {
    String::from_utf16_lossy(&s.encode_utf16().take(n).collect::<Vec<u16>>())
}

/// v4 `extractTextContent` — `buffer.toString('utf-8')` (lossy, as Node's
/// decoder is), the 50,000-unit truncation, the language / markdown hints.
fn extract_text_content(buffer: &[u8], filename: &str) -> ExtractedContent {
    let mut content = String::from_utf8_lossy(buffer).into_owned();
    let mut truncated = false;
    if utf16_len(&content) > MAX_CONTENT_LENGTH {
        content = utf16_prefix(&content, MAX_CONTENT_LENGTH);
        truncated = true;
    }
    let language = language_from_filename(filename);
    let lower = filename.to_lowercase();
    let is_markdown = lower.ends_with(".md") || lower.ends_with(".markdown");
    ExtractedContent {
        success: true,
        content: Some(content),
        content_type: if language.is_some() {
            "code"
        } else if is_markdown {
            "markdown"
        } else {
            "text"
        },
        language,
        error: None,
        truncated: Some(truncated),
    }
}

/// v4 `decodePdfEscapes`.
fn decode_pdf_escapes(input: &str) -> String {
    input
        .replace("\\(", "(")
        .replace("\\)", ")")
        .replace("\\n", "\n")
        .replace("\\r", "\r")
        .replace("\\t", "\t")
        .replace("\\\\", "\\")
}

/// JS `\s` over the Latin-1 range (`\t \n \v \f \r`, space, U+00A0) — Rust's
/// Unicode `\s` would also take U+0085, which JS's does not.
const JS_WS: &str = r"[\t\n\x0B\x0C\r \x{a0}]";

fn re(pattern: &str) -> Regex {
    Regex::new(&pattern.replace("{WS}", JS_WS)).expect("static pdf regex")
}

/// v4 `extractPdfTextFallback` — the regex scrape over the `latin1` decode:
/// literal `(…) Tj` strings, `[…] TJ` arrays, then ASCII runs inside
/// `stream…endstream` blocks; de-duplicated in first-seen order.
pub fn extract_pdf_text_fallback(buffer: &[u8]) -> String {
    static LITERAL: OnceLock<Regex> = OnceLock::new();
    static ARRAY: OnceLock<Regex> = OnceLock::new();
    static ITEM: OnceLock<Regex> = OnceLock::new();
    static STREAM: OnceLock<Regex> = OnceLock::new();
    static RUN: OnceLock<Regex> = OnceLock::new();
    static WS_RUN: OnceLock<Regex> = OnceLock::new();
    let literal = LITERAL.get_or_init(|| re(r"\(([^)]{1,4000})\){WS}*Tj"));
    let array = ARRAY.get_or_init(|| re(r"\[([\s\S]*?)\]{WS}*TJ"));
    let item = ITEM.get_or_init(|| re(r"\(([^)]{1,4000})\)"));
    let stream = STREAM.get_or_init(|| re(r"stream\r?\n([\s\S]*?)\r?\nendstream"));
    let run = RUN.get_or_init(|| re(r#"[A-Za-z0-9][A-Za-z0-9{WS},.;:!?()'"\-]{20,}"#));
    let ws_run = WS_RUN.get_or_init(|| re(r"{WS}+"));

    // `buffer.toString('latin1')` — every byte its own code point.
    let raw: String = buffer.iter().map(|b| *b as char).collect();
    let js_trim = crate::jsstr::js_trim;

    let mut chunks: Vec<String> = Vec::new();
    for m in literal.captures_iter(&raw) {
        let decoded = decode_pdf_escapes(&m[1]);
        let decoded = js_trim(&decoded);
        if utf16_len(decoded) >= 2 {
            chunks.push(decoded.to_string());
        }
    }
    for m in array.captures_iter(&raw) {
        let mut parts: Vec<String> = Vec::new();
        for it in item.captures_iter(&m[1]) {
            let decoded = decode_pdf_escapes(&it[1]);
            let decoded = js_trim(&decoded);
            if !decoded.is_empty() {
                parts.push(decoded.to_string());
            }
        }
        if !parts.is_empty() {
            chunks.push(parts.join(" "));
        }
    }
    for m in stream.captures_iter(&raw) {
        for r in run.find_iter(&m[1]) {
            let collapsed = ws_run.replace_all(r.as_str(), " ");
            let s = js_trim(&collapsed);
            if utf16_len(s) >= 20 {
                chunks.push(s.to_string());
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    let deduped: Vec<String> = chunks
        .into_iter()
        .filter(|c| seen.insert(c.clone()))
        .collect();
    js_trim(&deduped.join("\n")).to_string()
}

std::thread_local! {
    /// The differential's twin of v4's mocked `pdf-parse` (P4.D253): armed only
    /// through [`ScriptedTextExtractorGuard`]; `None` always in production, so
    /// [`pdf_text_extractor`] answers the seam's default.
    static SCRIPTED_TEXT_EXTRACTOR: std::cell::RefCell<Option<SharedTextExtractor>> =
        const { std::cell::RefCell::new(None) };
}

/// The extractor the PDF arm reads through: this thread's scripted one when a
/// test armed it, else [`default_text_extractor`] resolved in place (the
/// `api/mount_files.rs` idiom — nothing threads an extractor here).
fn pdf_text_extractor() -> SharedTextExtractor {
    SCRIPTED_TEXT_EXTRACTOR
        .with(|s| s.borrow().clone())
        .unwrap_or_else(default_text_extractor)
}

/// Arms a scripted [`crate::services::mount_index::converters::DocumentTextExtractor`]
/// for the PDF arm on THIS thread until dropped (the thread-scoped seam — a
/// process-global one would leak into every parallel test reaching a PDF).
#[cfg(any(test, feature = "test-support"))]
pub struct ScriptedTextExtractorGuard(Option<SharedTextExtractor>);

#[cfg(any(test, feature = "test-support"))]
impl ScriptedTextExtractorGuard {
    pub fn install(extractor: SharedTextExtractor) -> Self {
        ScriptedTextExtractorGuard(
            SCRIPTED_TEXT_EXTRACTOR.with(|s| s.borrow_mut().replace(extractor)),
        )
    }
}

#[cfg(any(test, feature = "test-support"))]
impl Drop for ScriptedTextExtractorGuard {
    fn drop(&mut self) {
        let previous = self.0.take();
        SCRIPTED_TEXT_EXTRACTOR.with(|s| *s.borrow_mut() = previous);
    }
}

/// v4 `extractPdfContent` (`a434c715b`): the converter seam first, trimmed
/// (v4 `(await convertPdfBufferToText(buffer)).trim()` — the converter answers
/// `''` on any failure, so a throw and an empty buffer arrive here as `''`);
/// the regex fallback only when that is empty, announced with the buffer's
/// size; failure only when BOTH find nothing; success announced with `size` +
/// `chars` (v4 `content.length`, UTF-16 units); then the truncation. v4 does
/// NOT re-trim the fallback's answer (it trims itself).
///
/// v4 wraps the arm in a `try/catch` (`Error extracting PDF content` →
/// `Failed to extract PDF content`); nothing here can fail — the seam answers
/// a `String` and the fallback is a pure scrape — so that arm is UNREACHABLE
/// in v5 (and, since `a434c715b`, practically in v4: the converter swallows
/// every throw). Recorded, not ported.
fn extract_pdf_content(buffer: &[u8]) -> ExtractedContent {
    let extractor = pdf_text_extractor();
    let mut content = crate::jsstr::js_trim(&extractor.extract(buffer, "pdf")).to_string();
    if content.is_empty() {
        tracing::warn!(
            target: "quilltap::file_content_extractor",
            size = buffer.len(),
            "pdf-parse found no text, using native fallback extraction"
        );
        content = extract_pdf_text_fallback(buffer);
    }
    if content.is_empty() {
        return ExtractedContent::failure("Failed to extract PDF content (no text found)");
    }
    tracing::debug!(
        target: "quilltap::file_content_extractor",
        size = buffer.len(),
        chars = utf16_len(&content),
        "Extracted PDF content"
    );
    let mut truncated = false;
    if utf16_len(&content) > MAX_CONTENT_LENGTH {
        content = utf16_prefix(&content, MAX_CONTENT_LENGTH);
        truncated = true;
    }
    ExtractedContent {
        success: true,
        content: Some(content),
        content_type: "text",
        language: None,
        error: None,
        truncated: Some(truncated),
    }
}

/// v4 `handleImageContent` — the stored description, else the placeholder
/// (`file.width && file.height` — JS truthy: a zero dimension omits the size).
fn handle_image_content(file: &FileFull) -> ExtractedContent {
    if let Some(desc) = file.description.as_deref().filter(|d| !d.is_empty()) {
        return ExtractedContent {
            success: true,
            content: Some(desc.to_string()),
            content_type: "image_description",
            language: None,
            error: None,
            truncated: None,
        };
    }
    let size = match (file.width, file.height) {
        (Some(w), Some(h)) if w != 0 && h != 0 => format!(" ({w}x{h})"),
        _ => String::new(),
    };
    ExtractedContent {
        success: true,
        content: Some(format!("[Image: {}{size}]", file.original_filename)),
        content_type: "image_description",
        language: None,
        error: None,
        truncated: None,
    }
}

/// The photo-tool [`FileEntry`] projection of a [`FileFull`] row — what
/// `download_file` keys its read on (the generation-prompt columns are not
/// carried; nothing here reads them).
pub fn file_entry_of(file: &FileFull) -> FileEntry {
    FileEntry {
        id: file.id.clone(),
        sha256: file.sha256.clone(),
        original_filename: file.original_filename.clone(),
        mime_type: file.mime_type.clone(),
        size: file.size,
        width: file.width,
        height: file.height,
        category: file.category.clone(),
        generation_prompt: None,
        generation_model: None,
        generation_revised_prompt: None,
        generation_key: None,
        description: file.description.clone(),
        storage_key: file.storage_key.clone(),
    }
}

/// v4 `extractFileContent(file)`: the image arm, the 10 MB ceiling, the
/// storage download (`download_file`'s error is v4's wrapper's), then PDF /
/// text / code / the binary placeholder.
pub fn extract_file_content(
    db: &Db,
    backend: &dyn StorageBackend,
    file: &FileFull,
) -> ExtractedContent {
    if is_image_mime_type(&file.mime_type) {
        return handle_image_content(file);
    }
    if file.size > 10 * 1024 * 1024 {
        tracing::warn!(
            target: "quilltap::file_content_extractor",
            file_id = %file.id,
            size = file.size,
            "File too large for extraction"
        );
        return ExtractedContent::failure("File too large for content extraction (max 10MB)");
    }
    if file.storage_key.as_deref().is_none_or(str::is_empty) {
        return ExtractedContent::failure("File has no storage key");
    }
    let buffer = match download_file(db, backend, &file_entry_of(file)) {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(
                target: "quilltap::file_content_extractor",
                file_id = %file.id,
                storage_key = %file.storage_key.as_deref().unwrap_or(""),
                error = %e,
                "Failed to download file from storage"
            );
            return ExtractedContent::failure("Failed to download file from storage");
        }
    };
    if file.mime_type == "application/pdf" {
        return extract_pdf_content(&buffer);
    }
    if is_text_mime_type(&file.mime_type) {
        return extract_text_content(&buffer, &file.original_filename);
    }
    if language_from_filename(&file.original_filename).is_some() {
        return extract_text_content(&buffer, &file.original_filename);
    }
    ExtractedContent {
        success: true,
        content: Some(format!(
            "[Binary file: {} ({}, {})]",
            file.original_filename,
            file.mime_type,
            format_bytes(file.size as f64)
        )),
        content_type: "binary",
        language: None,
        error: None,
        truncated: None,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::services::mount_index::converters::DocumentTextExtractor;
    use crate::test_support::global_capture;

    /// The scripted converter (v4's mocked `pdf-parse`): answers its text.
    struct Scripted(&'static str);
    impl DocumentTextExtractor for Scripted {
        fn extract(&self, _bytes: &[u8], _file_type: &str) -> String {
            self.0.to_string()
        }
    }

    fn run(converter: &'static str, bytes: &[u8]) -> (ExtractedContent, Vec<String>) {
        let _guard = ScriptedTextExtractorGuard::install(Arc::new(Scripted(converter)));
        global_capture::capture(|| extract_pdf_content(bytes))
    }

    const WARN: &str = "WARN quilltap::file_content_extractor pdf-parse found no text, using native fallback extraction";
    const DEBUG: &str = "DEBUG quilltap::file_content_extractor Extracted PDF content";

    #[test]
    fn converter_text_is_trimmed_and_announced_with_size_and_utf16_chars() {
        let bytes = b"%PDF-1.4 not really parsed here";
        let (out, lines) = run("\n  Caf\u{e9} \u{1f41d}  \n", bytes);
        assert_eq!(out.content.as_deref(), Some("Caf\u{e9} \u{1f41d}"));
        assert!(out.success);
        assert_eq!(out.truncated, Some(false));
        // `chars` is v4's `content.length` — UTF-16 units (the bee is two),
        // never bytes (8) or scalars (6).
        assert_eq!(lines, vec![format!("{DEBUG} size=31 chars=7")]);
    }

    #[test]
    fn empty_converter_answer_takes_the_announced_fallback() {
        let bytes = b"BT (Fallback lore line) Tj ET";
        let (out, lines) = run("   ", bytes);
        assert_eq!(out.content.as_deref(), Some("Fallback lore line"));
        assert_eq!(
            lines,
            vec![
                format!("{WARN} size=29"),
                format!("{DEBUG} size=29 chars=18")
            ]
        );
    }

    #[test]
    fn nothing_found_by_either_reader_fails_with_v4s_sentence() {
        let (out, lines) = run("", b"%PDF-1.4 junk");
        assert_eq!(
            out,
            ExtractedContent::failure("Failed to extract PDF content (no text found)")
        );
        // The WARN fires; the success DEBUG does not.
        assert_eq!(lines, vec![format!("{WARN} size=13")]);
    }

    #[test]
    fn chars_counts_before_the_truncation() {
        let long: &'static str = Box::leak("x".repeat(60_000).into_boxed_str());
        let (out, lines) = run(long, b"%PDF");
        assert_eq!(out.truncated, Some(true));
        assert_eq!(
            out.content.as_deref().map(str::len),
            Some(MAX_CONTENT_LENGTH)
        );
        assert_eq!(lines, vec![format!("{DEBUG} size=4 chars=60000")]);
    }

    #[test]
    fn the_production_seam_refuses_so_the_fallback_runs() {
        // No guard: `default_text_extractor()` (P4.6y's refusal) answers `''`.
        let (out, lines) =
            global_capture::capture(|| extract_pdf_content(b"BT (Recovered text) Tj ET"));
        assert_eq!(out.content.as_deref(), Some("Recovered text"));
        assert_eq!(
            lines,
            vec![
                format!("{WARN} size=25"),
                format!("{DEBUG} size=25 chars=14")
            ]
        );
    }
}
