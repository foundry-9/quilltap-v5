//! Extension → MIME detection — v4 `lib/file-storage/scanner.ts`
//! (`detectMimeType` + `EXTENSION_MIME_MAP`).
//!
//! The `blob` branch of the project Files card (`mimeForMountFile`) and the legacy
//! file listing both resolve a concrete MIME from a filename's extension. This is
//! the byte-faithful port of that lookup: the extension is the Node `path.extname`
//! of the basename, lowercased (a leading-dot dotfile has NO extension); an unknown
//! extension resolves to `application/octet-stream`.

/// v4 `EXTENSION_MIME_MAP` — verbatim (lowercase, leading-dot keys).
const EXTENSION_MIME_MAP: &[(&str, &str)] = &[
    (".jpg", "image/jpeg"),
    (".jpeg", "image/jpeg"),
    (".png", "image/png"),
    (".gif", "image/gif"),
    (".webp", "image/webp"),
    (".svg", "image/svg+xml"),
    (".bmp", "image/bmp"),
    (".ico", "image/x-icon"),
    (".tiff", "image/tiff"),
    (".tif", "image/tiff"),
    (".avif", "image/avif"),
    (".txt", "text/plain"),
    (".md", "text/markdown"),
    (".markdown", "text/markdown"),
    (".html", "text/html"),
    (".htm", "text/html"),
    (".css", "text/css"),
    (".js", "text/javascript"),
    (".ts", "text/typescript"),
    (".json", "application/json"),
    (".xml", "application/xml"),
    (".csv", "text/csv"),
    (".pdf", "application/pdf"),
    (".doc", "application/msword"),
    (
        ".docx",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    ),
    (".xls", "application/vnd.ms-excel"),
    (
        ".xlsx",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    ),
    (".ppt", "application/vnd.ms-powerpoint"),
    (
        ".pptx",
        "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    ),
    (".zip", "application/zip"),
    (".gz", "application/gzip"),
    (".tar", "application/x-tar"),
    (".mp3", "audio/mpeg"),
    (".wav", "audio/wav"),
    (".ogg", "audio/ogg"),
    (".mp4", "video/mp4"),
    (".webm", "video/webm"),
    (".avi", "video/x-msvideo"),
];

/// Node `path.extname(filename).toLowerCase()` — the trailing `.ext` (WITH the
/// dot), lowercased; empty when the basename has no dot or is a leading-dot
/// dotfile. Mirrors `database_store::path_extname`.
fn extname_lower(filename: &str) -> String {
    let base = match filename.rsplit_once('/') {
        Some((_, b)) => b,
        None => filename,
    };
    match base.rfind('.') {
        Some(idx) if idx > 0 => base[idx..].to_ascii_lowercase(),
        _ => String::new(),
    }
}

/// v4 `detectMimeType(filename)` — the extension MIME, or `application/octet-stream`.
pub fn detect_mime_type(filename: &str) -> String {
    let ext = extname_lower(filename);
    EXTENSION_MIME_MAP
        .iter()
        .find(|(k, _)| *k == ext)
        .map(|(_, v)| (*v).to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_extensions() {
        assert_eq!(detect_mime_type("logo.png"), "image/png");
        assert_eq!(detect_mime_type("notes.txt"), "text/plain");
        assert_eq!(detect_mime_type("a.b.jpg"), "image/jpeg"); // last dot wins
        assert_eq!(detect_mime_type("REPORT.PDF"), "application/pdf"); // lowercased
        assert_eq!(detect_mime_type("data.jsonl"), "application/octet-stream"); // not in map
    }

    #[test]
    fn no_or_dotfile_extension() {
        assert_eq!(detect_mime_type("README"), "application/octet-stream");
        assert_eq!(detect_mime_type(".gitignore"), "application/octet-stream");
        assert_eq!(detect_mime_type("weird."), "application/octet-stream");
    }
}
