//! Text content / MIME sniffing — a byte-exact port of v4
//! `lib/files/text-detection.ts`.
//!
//! Used during file upload to (1) detect whether a file is readable as text,
//! (2) infer a better MIME type when the browser hands us a generic one, and
//! (3) support previewing and LLM access to code/text files. This is the
//! upload-half leaf; it ships with its own tier-1 differential
//! (`text_detection_equivalence`).
//!
//! Fidelity notes carried from v4:
//! * [`is_text_content`] samples at most [`SAMPLE_SIZE`] bytes; an empty file IS
//!   text; the 3rd null byte short-circuits to binary; the non-printable ratio
//!   is a STRICT `<` against [`NON_PRINTABLE_THRESHOLD`], denominator =
//!   `sample_length`. A null byte is itself non-printable, so it increments both
//!   counters.
//! * [`is_printable_or_whitespace`] treats every high byte (`>= 0x80`) as
//!   printable with NO UTF-8 validation (v4's "could be a valid continuation").
//! * [`get_mime_type_from_extension`]'s compound pass walks
//!   `EXTENSION_MIME_MAP` in INSERTION ORDER and returns the first `ends_with`
//!   hit — so `component.d.ts` resolves via `.ts` (the first entry). The slice
//!   ordering is load-bearing; do not turn it into a map.

use regex::Regex;
use std::sync::LazyLock;

/// Bytes to sample for text detection (v4 `SAMPLE_SIZE`).
pub const SAMPLE_SIZE: usize = 8192;

/// Maximum ratio of non-printable characters to still count as text
/// (v4 `NON_PRINTABLE_THRESHOLD`).
pub const NON_PRINTABLE_THRESHOLD: f64 = 0.10;

/// Result of text-detection analysis (v4 `TextDetectionResult`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextDetectionResult {
    /// Whether the file content appears to be plain text.
    pub is_plain_text: bool,
    /// Inferred MIME type based on extension/content (if better than provided).
    pub detected_mime_type: Option<String>,
    /// Detected or assumed encoding (always `"utf-8"`).
    pub encoding: String,
}

/// File extension → MIME type mapping (v4 `EXTENSION_MIME_MAP`), a slice so the
/// INSERTION ORDER of the compound `ends_with` pass is preserved verbatim.
///
/// (`.R` becomes unreachable after the caller lowercases, but it is kept for
/// fidelity with the v4 object.)
const EXTENSION_MIME_MAP: &[(&str, &str)] = &[
    // TypeScript/JavaScript
    (".ts", "text/typescript"),
    (".tsx", "text/typescript"),
    (".js", "text/javascript"),
    (".jsx", "text/javascript"),
    (".mjs", "text/javascript"),
    (".cjs", "text/javascript"),
    // Web
    (".html", "text/html"),
    (".htm", "text/html"),
    (".css", "text/css"),
    (".scss", "text/x-scss"),
    (".sass", "text/x-sass"),
    (".less", "text/x-less"),
    (".vue", "text/x-vue"),
    (".svelte", "text/x-svelte"),
    // Data formats
    (".json", "application/json"),
    (".xml", "application/xml"),
    (".yaml", "text/yaml"),
    (".yml", "text/yaml"),
    (".toml", "text/toml"),
    (".csv", "text/csv"),
    (".tsv", "text/tab-separated-values"),
    // Documentation
    (".md", "text/markdown"),
    (".markdown", "text/markdown"),
    (".txt", "text/plain"),
    (".text", "text/plain"),
    (".log", "text/plain"),
    (".rst", "text/x-rst"),
    // Programming languages
    (".py", "text/x-python"),
    (".pyw", "text/x-python"),
    (".rb", "text/x-ruby"),
    (".rs", "text/x-rust"),
    (".go", "text/x-go"),
    (".java", "text/x-java"),
    (".kt", "text/x-kotlin"),
    (".kts", "text/x-kotlin"),
    (".scala", "text/x-scala"),
    (".swift", "text/x-swift"),
    (".c", "text/x-c"),
    (".h", "text/x-c"),
    (".cpp", "text/x-c++"),
    (".hpp", "text/x-c++"),
    (".cc", "text/x-c++"),
    (".cxx", "text/x-c++"),
    (".cs", "text/x-csharp"),
    (".php", "text/x-php"),
    (".pl", "text/x-perl"),
    (".pm", "text/x-perl"),
    (".r", "text/x-r"),
    (".R", "text/x-r"),
    (".lua", "text/x-lua"),
    (".dart", "text/x-dart"),
    (".ex", "text/x-elixir"),
    (".exs", "text/x-elixir"),
    (".erl", "text/x-erlang"),
    (".hrl", "text/x-erlang"),
    (".clj", "text/x-clojure"),
    (".cljs", "text/x-clojure"),
    (".hs", "text/x-haskell"),
    (".lhs", "text/x-haskell"),
    (".ml", "text/x-ocaml"),
    (".mli", "text/x-ocaml"),
    (".fs", "text/x-fsharp"),
    (".fsx", "text/x-fsharp"),
    // Shell/Scripts
    (".sh", "text/x-shellscript"),
    (".bash", "text/x-shellscript"),
    (".zsh", "text/x-shellscript"),
    (".fish", "text/x-shellscript"),
    (".ps1", "text/x-powershell"),
    (".psm1", "text/x-powershell"),
    (".bat", "text/x-batch"),
    (".cmd", "text/x-batch"),
    // Database
    (".sql", "text/x-sql"),
    (".pgsql", "text/x-sql"),
    (".mysql", "text/x-sql"),
    // Config files
    (".ini", "text/plain"),
    (".cfg", "text/plain"),
    (".conf", "text/plain"),
    (".config", "text/plain"),
    (".env", "text/plain"),
    (".env.local", "text/plain"),
    (".env.development", "text/plain"),
    (".env.production", "text/plain"),
    (".properties", "text/plain"),
    (".editorconfig", "text/plain"),
    (".gitignore", "text/plain"),
    (".gitattributes", "text/plain"),
    (".dockerignore", "text/plain"),
    (".npmignore", "text/plain"),
    (".eslintrc", "application/json"),
    (".prettierrc", "application/json"),
    (".babelrc", "application/json"),
    // Misc
    (".graphql", "text/x-graphql"),
    (".gql", "text/x-graphql"),
    (".proto", "text/x-protobuf"),
    (".tf", "text/x-terraform"),
    (".tfvars", "text/x-terraform"),
    (".dockerfile", "text/x-dockerfile"),
    (".makefile", "text/x-makefile"),
];

/// Special extension-less filenames → MIME type (v4 `specialFiles`).
const SPECIAL_FILES: &[(&str, &str)] = &[
    ("dockerfile", "text/x-dockerfile"),
    ("makefile", "text/x-makefile"),
    ("gemfile", "text/x-ruby"),
    ("rakefile", "text/x-ruby"),
    ("cmakelists.txt", "text/x-cmake"),
    ("license", "text/plain"),
    ("readme", "text/plain"),
    ("changelog", "text/plain"),
    ("authors", "text/plain"),
    ("contributors", "text/plain"),
];

/// Common text-based `application/*` MIME types (v4 `textApplicationTypes`).
const TEXT_APPLICATION_TYPES: &[&str] = &[
    "application/json",
    "application/xml",
    "application/javascript",
    "application/typescript",
    "application/x-javascript",
    "application/x-typescript",
    "application/x-sh",
    "application/x-shellscript",
    "application/graphql",
];

/// The standard-extension matcher `\.([^.]+)$` (v4's `.match(/\.([^.]+)$/)`).
static EXT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\.([^.]+)$").unwrap());

/// Whether a byte is printable ASCII or common whitespace (v4
/// `isPrintableOrWhitespace`).
///
/// Tab/newline/CR, printable ASCII (space..tilde), and every high byte
/// (`>= 0x80`) count as printable; nothing else does.
fn is_printable_or_whitespace(byte: u8) -> bool {
    if byte == 0x09 || byte == 0x0a || byte == 0x0d {
        return true;
    }
    if (0x20..=0x7e).contains(&byte) {
        return true;
    }
    if byte >= 0x80 {
        return true;
    }
    false
}

/// Detect if file content is likely plain text (v4 `isTextContent`).
///
/// Samples the first [`SAMPLE_SIZE`] bytes: an empty buffer is text; a 3rd null
/// byte short-circuits to binary; otherwise text iff the non-printable ratio is
/// strictly below [`NON_PRINTABLE_THRESHOLD`].
pub fn is_text_content(buffer: &[u8]) -> bool {
    let sample_length = buffer.len().min(SAMPLE_SIZE);

    if sample_length == 0 {
        // Empty files are considered text.
        return true;
    }

    let mut null_bytes: u32 = 0;
    let mut non_printable: u64 = 0;

    for &byte in &buffer[..sample_length] {
        // Null bytes are a strong indicator of binary content.
        if byte == 0x00 {
            null_bytes += 1;
            // More than a couple null bytes → likely binary.
            if null_bytes > 2 {
                return false;
            }
        }

        if !is_printable_or_whitespace(byte) {
            non_printable += 1;
        }
    }

    let non_printable_ratio = non_printable as f64 / sample_length as f64;
    non_printable_ratio < NON_PRINTABLE_THRESHOLD
}

/// Infer a MIME type from a filename (v4 `getMimeTypeFromExtension`).
///
/// Three passes over the lowercased name: (1) the first `ends_with` hit in
/// [`EXTENSION_MIME_MAP`] insertion order (handles `.d.ts`, `.env.local`);
/// (2) the `\.([^.]+)$` standard extension looked up exactly; (3) the special
/// extension-less basenames (`dockerfile`, `readme`, …). `None` if unknown.
pub fn get_mime_type_from_extension(filename: &str) -> Option<String> {
    let lower = filename.to_lowercase();

    // Check for compound extensions first (insertion order — first match wins).
    for &(ext, mime) in EXTENSION_MIME_MAP {
        if lower.ends_with(ext) {
            return Some(mime.to_string());
        }
    }

    // Check standard extension.
    if let Some(caps) = EXT_RE.captures(&lower) {
        let ext = format!(".{}", &caps[1]);
        return EXTENSION_MIME_MAP
            .iter()
            .find(|&&(k, _)| k == ext)
            .map(|&(_, v)| v.to_string());
    }

    // Handle special filenames without extensions.
    let base_name = lower.split('/').next_back().filter(|s| !s.is_empty());
    let base_name = base_name.unwrap_or(&lower);
    SPECIAL_FILES
        .iter()
        .find(|&&(k, _)| k == base_name)
        .map(|&(_, v)| v.to_string())
}

/// Whether a MIME type indicates text content (v4 `isTextMimeType`).
pub fn is_text_mime_type(mime_type: &str) -> bool {
    if mime_type.starts_with("text/") {
        return true;
    }
    TEXT_APPLICATION_TYPES.contains(&mime_type)
}

/// Detect if a file is text and infer its best MIME type (v4
/// `detectTextContent`).
pub fn detect_text_content(
    buffer: &[u8],
    filename: &str,
    provided_mime_type: &str,
) -> TextDetectionResult {
    let content_is_text = is_text_content(buffer);
    let extension_mime_type = get_mime_type_from_extension(filename);

    let is_generic = provided_mime_type == "application/octet-stream"
        || provided_mime_type.is_empty()
        || provided_mime_type == "application/x-unknown";

    let detected_mime_type: Option<String> = if extension_mime_type.is_some() && is_generic {
        extension_mime_type.clone()
    } else if content_is_text && is_generic {
        // Content is text but we don't recognize the extension.
        Some("text/plain".to_string())
    } else {
        None
    };

    let is_plain_text = content_is_text
        && (is_text_mime_type(provided_mime_type)
            || is_text_mime_type(detected_mime_type.as_deref().unwrap_or(""))
            || extension_mime_type.is_some());

    TextDetectionResult {
        is_plain_text,
        detected_mime_type,
        encoding: "utf-8".to_string(),
    }
}

/// Return the best MIME type for a file (v4 `getBestMimeType`).
///
/// The detected type when it is present AND non-empty (JS `||` truthiness),
/// otherwise the provided type.
pub fn get_best_mime_type(result: &TextDetectionResult, provided_mime_type: &str) -> String {
    match result.detected_mime_type.as_deref() {
        Some(m) if !m.is_empty() => m.to_string(),
        _ => provided_mime_type.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_buffer_is_text() {
        assert!(is_text_content(&[]));
    }

    #[test]
    fn two_nulls_ok_three_binary() {
        // Two nulls in a 30-byte buffer: 2/30 < 0.10, so still text (the 3rd-null
        // short-circuit is not hit).
        let mut two = vec![b'a'; 30];
        two[2] = 0;
        two[17] = 0;
        assert!(is_text_content(&two));
        // Three nulls short-circuits to binary regardless of ratio.
        let three = b"\0\0\0aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        assert!(!is_text_content(three));
    }

    #[test]
    fn exactly_ten_percent_nonprintable_is_not_text() {
        // 1 non-printable out of 10 = 0.10, which is NOT < 0.10 → not text.
        let mut buf = vec![b'a'; 9];
        buf.push(0x01);
        assert!(!is_text_content(&buf));
    }

    #[test]
    fn high_bytes_are_printable() {
        // High bytes are unconditionally printable (no UTF-8 validation).
        assert!(is_text_content(&[0xff, 0xfe, 0x80, b'a']));
    }

    #[test]
    fn ext_lookup_basic() {
        assert_eq!(
            get_mime_type_from_extension("foo.py").as_deref(),
            Some("text/x-python")
        );
        assert_eq!(get_mime_type_from_extension("foo.zzz"), None);
        assert_eq!(get_mime_type_from_extension("noext"), None);
    }

    #[test]
    fn ends_with_d_ts_resolves_via_ts() {
        // `.ts` is the first slice entry, so `ends_with` wins the compound pass.
        assert_eq!(
            get_mime_type_from_extension("component.d.ts").as_deref(),
            Some("text/typescript")
        );
    }

    #[test]
    fn special_file_and_path_basename() {
        assert_eq!(
            get_mime_type_from_extension("Dockerfile").as_deref(),
            Some("text/x-dockerfile")
        );
        assert_eq!(
            get_mime_type_from_extension("README").as_deref(),
            Some("text/plain")
        );
    }

    #[test]
    fn best_mime_type_falls_back_on_empty() {
        let r = TextDetectionResult {
            is_plain_text: false,
            detected_mime_type: Some(String::new()),
            encoding: "utf-8".to_string(),
        };
        assert_eq!(get_best_mime_type(&r, "provided/x"), "provided/x");
    }
}
