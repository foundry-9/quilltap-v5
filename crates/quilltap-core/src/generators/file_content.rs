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
//! v4 tries `require('pdf-parse')` first and only runs its own regex fallback
//! (`extractPdfTextFallback`) when the module is absent. `pdf-parse` IS
//! installed in v4's checkout at the pin, so a real v4 extracts PDF text
//! through it; v5 carries no PDF parser and always runs v4's fallback. For a
//! `.pdf` source the two sides can therefore differ in the extracted text —
//! never in the shape (`success` + `contentType: 'text'` + the truncation). No
//! committed fixture carries a PDF; recorded, not pinned.

use std::sync::OnceLock;

use regex::Regex;
use serde::Serialize;

use crate::db::files::{FileEntry, FileFull};
use crate::db::runtime::Db;
use crate::format_bytes::format_bytes;
use crate::services::file_storage::{download_file, StorageBackend};

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

/// v4 `extractPdfContent` — the fallback arm only (module header).
fn extract_pdf_content(buffer: &[u8]) -> ExtractedContent {
    tracing::warn!(
        target: "quilltap::file_content_extractor",
        "pdf-parse not available, using native fallback extraction"
    );
    let fallback = extract_pdf_text_fallback(buffer);
    if fallback.is_empty() {
        return ExtractedContent::failure(
            "Failed to extract PDF content (pdf-parse unavailable and fallback extractor found no text)",
        );
    }
    let mut content = fallback;
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
