//! Port of v4 `lib/mount-index/path-utils.ts` — shared path + file-type helpers
//! for mount-point file operations.
//!
//! `normalise_relative_path` is the single source of truth for the "strip
//! slashes, reject traversal" rule applied before any mount write. It reproduces
//! Node's `path.posix.normalize` exactly (the `.`/`..` collapsing that partially
//! resolves traversal) before stripping slashes and rejecting any surviving
//! `..`. `mime_for_extension` / `detect_native_text` reproduce Node
//! `path.posix.extname` semantics (a leading-dot "dotfile" has NO extension).

use super::file_op_error::{FileOpError, FileOpErrorCode};

/// Native-text extensions that belong in `doc_mount_documents` (chunkable text)
/// rather than the binary blob mirror (v4 `NativeTextType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeTextType {
    Markdown,
    Txt,
    Json,
    Jsonl,
}

impl NativeTextType {
    pub fn as_str(self) -> &'static str {
        match self {
            NativeTextType::Markdown => "markdown",
            NativeTextType::Txt => "txt",
            NativeTextType::Json => "json",
            NativeTextType::Jsonl => "jsonl",
        }
    }
}

// ---------------------------------------------------------------------------
// Node path.posix internals (faithful ports)
// ---------------------------------------------------------------------------

const SLASH: char = '/';
const DOT: char = '.';

/// Node `normalizeString(path, allowAboveRoot, '/', isPosixPathSeparator)`.
fn normalize_string(path: &[char], allow_above_root: bool) -> String {
    let mut res: Vec<char> = Vec::new();
    let mut last_segment_length: isize = 0;
    let mut last_slash: isize = -1;
    let mut dots: isize = 0;
    let n = path.len() as isize;
    let mut i: isize = 0;
    while i <= n {
        let code: Option<char> = if i < n {
            Some(path[i as usize])
        } else if i > 0 && path[(i - 1) as usize] == SLASH {
            // Node breaks out of the loop when the final char was a separator.
            break;
        } else {
            Some(SLASH)
        };
        let code = match code {
            Some(c) => c,
            None => break,
        };

        if code == SLASH {
            if last_slash == i - 1 || dots == 1 {
                // NOOP
            } else if dots == 2 {
                let rlen = res.len() as isize;
                if rlen < 2
                    || last_segment_length != 2
                    || res[res.len() - 1] != DOT
                    || res[res.len() - 2] != DOT
                {
                    if rlen > 2 {
                        if let Some(last_slash_index) = res.iter().rposition(|&c| c == SLASH) {
                            res.truncate(last_slash_index);
                            last_segment_length = res.len() as isize
                                - 1
                                - res
                                    .iter()
                                    .rposition(|&c| c == SLASH)
                                    .map_or(-1, |p| p as isize);
                            last_slash = i;
                            dots = 0;
                            i += 1;
                            continue;
                        } else {
                            res.clear();
                            last_segment_length = 0;
                            last_slash = i;
                            dots = 0;
                            i += 1;
                            continue;
                        }
                    } else if rlen != 0 {
                        res.clear();
                        last_segment_length = 0;
                        last_slash = i;
                        dots = 0;
                        i += 1;
                        continue;
                    }
                }
                if allow_above_root {
                    if !res.is_empty() {
                        res.push(SLASH);
                    }
                    res.push(DOT);
                    res.push(DOT);
                    last_segment_length = 2;
                }
            } else {
                if !res.is_empty() {
                    res.push(SLASH);
                }
                res.extend_from_slice(&path[(last_slash + 1) as usize..i as usize]);
                last_segment_length = i - last_slash - 1;
            }
            last_slash = i;
            dots = 0;
        } else if code == DOT && dots != -1 {
            dots += 1;
        } else {
            dots = -1;
        }
        i += 1;
    }
    res.into_iter().collect()
}

/// Node `path.posix.resolve(...segments)` — right-to-left assembly, stopping at
/// the first absolute segment, then `normalizeString`. All our call sites pass
/// an absolute base first, so `posixCwd()` is never consulted (we omit it).
pub(crate) fn path_posix_resolve(segments: &[&str]) -> String {
    let mut resolved_path = String::new();
    let mut resolved_absolute = false;
    for seg in segments.iter().rev() {
        if resolved_absolute {
            break;
        }
        if seg.is_empty() {
            continue;
        }
        resolved_path = format!("{seg}/{resolved_path}");
        resolved_absolute = seg.starts_with('/');
    }
    let chars: Vec<char> = resolved_path.chars().collect();
    let normalized = normalize_string(&chars, !resolved_absolute);
    if resolved_absolute {
        format!("/{normalized}")
    } else if normalized.is_empty() {
        ".".to_string()
    } else {
        normalized
    }
}

/// Node `path.posix.normalize`.
fn path_posix_normalize(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    if chars.is_empty() {
        return ".".to_string();
    }
    let is_absolute = chars[0] == SLASH;
    let trailing_separator = *chars.last().unwrap() == SLASH;
    let mut normalized = normalize_string(&chars, !is_absolute);
    if normalized.is_empty() {
        if is_absolute {
            return "/".to_string();
        }
        return if trailing_separator { "./" } else { "." }.to_string();
    }
    if trailing_separator {
        normalized.push('/');
    }
    if is_absolute {
        format!("/{normalized}")
    } else {
        normalized
    }
}

/// Node `path.posix.extname` — returns the extension including the leading dot,
/// or `""`. A dotfile like `.md` has no extension.
pub(crate) fn path_extname(path: &str) -> String {
    let chars: Vec<char> = path.chars().collect();
    let mut start_dot: isize = -1;
    let mut start_part: isize = 0;
    let mut end: isize = -1;
    let mut matched_slash = true;
    // 0 = no dot yet; 1 = one or more dots; -1 = a non-dot char after a dot.
    let mut pre_dot_state: i32 = 0;
    let mut i = chars.len() as isize - 1;
    while i >= 0 {
        let code = chars[i as usize];
        if code == SLASH {
            if !matched_slash {
                start_part = i + 1;
                break;
            }
            i -= 1;
            continue;
        }
        if end == -1 {
            matched_slash = false;
            end = i + 1;
        }
        if code == DOT {
            if start_dot == -1 {
                start_dot = i;
            } else if pre_dot_state != 1 {
                pre_dot_state = 1;
            }
        } else if start_dot != -1 {
            pre_dot_state = -1;
        }
        i -= 1;
    }
    if start_dot == -1
        || end == -1
        || pre_dot_state == 0
        || (pre_dot_state == 1 && start_dot == end - 1 && start_dot == start_part + 1)
    {
        return String::new();
    }
    chars[start_dot as usize..end as usize].iter().collect()
}

// ---------------------------------------------------------------------------
// Public helpers
// ---------------------------------------------------------------------------

// === P4.D377 ===
/// v4 `normaliseRelativePath`'s options bag (`{ allowRoot?: boolean }`,
/// `path-utils.ts:30`, v4 `600c2906f`).
///
/// `allow_root` is the database store's form: there the store ROOT is a
/// legitimate target (listing it, creating a folder "at" it), so an empty /
/// `.` path answers `''` instead of throwing (v4 `database-store.ts:55-58` —
/// "The store root is `''` here; the shared rule otherwise").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NormaliseOpts {
    pub allow_root: bool,
}

/// v4 `normaliseRelativePath`: collapse `./` and redundant separators, strip
/// leading/trailing slashes, and reject `..` traversal. Errors with
/// `FileOpError('INVALID_PATH')` on an empty or traversing path.
///
/// The strict (`allowRoot: false`) form — kept as the one-arg fn so its
/// callers are untouched; see [`normalise_relative_path_opts`].
pub fn normalise_relative_path(input: &str) -> Result<String, FileOpError> {
    normalise_relative_path_opts(input, NormaliseOpts { allow_root: false })
}

/// v4 `normaliseRelativePath(input, { allowRoot })` (`path-utils.ts:30-44`):
/// collapse `./` and redundant separators, strip leading/trailing slashes, and
/// reject `..` traversal. Throws `FileOpError('INVALID_PATH')` on a traversing
/// path, and on an empty one unless `allowRoot` is set — then the store root
/// comes back as `''`. Trailing slashes are stripped so folder paths match the
/// form that `folder-paths.ts` writes into `doc_mount_folders.path`.
pub fn normalise_relative_path_opts(
    input: &str,
    opts: NormaliseOpts,
) -> Result<String, FileOpError> {
    // `path.normalize(input || '.')` FIRST (on the raw input — backslashes are
    // ordinary chars on posix), THEN backslash→slash, THEN strip
    // leading/trailing slashes. (`path_posix_normalize('')` is already `.`,
    // so the `|| '.'` is carried by the normaliser itself.)
    let source = if input.is_empty() { "." } else { input };
    let normalized = path_posix_normalize(source)
        .replace('\\', "/")
        .trim_start_matches('/')
        .trim_end_matches('/')
        .to_string();
    if normalized.is_empty() || normalized == "." {
        if opts.allow_root {
            return Ok(String::new());
        }
        return Err(FileOpError::new(
            format!("Invalid relative path: {input}"),
            FileOpErrorCode::InvalidPath,
        ));
    }
    if normalized.split('/').any(|s| s == "..") {
        return Err(FileOpError::new(
            format!("Path traversal not allowed: {input}"),
            FileOpErrorCode::InvalidPath,
        ));
    }
    Ok(normalized)
}

/// v4 `isSafeRelativePath` (`path-utils.ts:51-62`, moved out of the folders
/// route by `600c2906f`): true when a normalised relative path is safe to
/// create as a folder or file name on any backend — no absolute path, no
/// empty / `.` / `..` segment, no control characters, none of `<>:"|?*\`,
/// and no segment over 200 characters.
///
/// "Characters" is JS `.length` — UTF-16 code units, so an astral character
/// counts TWO ([`crate::jsstr::utf16_len`]); `path.isAbsolute` on posix is a
/// leading `/`.
pub fn is_safe_relative_path(rel: &str) -> bool {
    if rel.is_empty() {
        return false;
    }
    if rel.starts_with('/') {
        return false;
    }
    for seg in rel.split('/') {
        if seg.is_empty() || seg == "." || seg == ".." {
            return false;
        }
        if seg.chars().any(|c| ('\x00'..='\x1f').contains(&c)) {
            return false;
        }
        if seg
            .chars()
            .any(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*' | '\\'))
        {
            return false;
        }
        if crate::jsstr::utf16_len(seg) > 200 {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod p4d377_tests {
    use super::*;

    #[test]
    fn allow_root_answers_the_root_and_strict_refuses_it() {
        let root = NormaliseOpts { allow_root: true };
        assert_eq!(normalise_relative_path_opts("", root).unwrap(), "");
        assert_eq!(normalise_relative_path_opts("./", root).unwrap(), "");
        let e = normalise_relative_path("").unwrap_err();
        assert_eq!(e.message, "Invalid relative path: ");
        assert_eq!(
            normalise_relative_path_opts("../a", root)
                .unwrap_err()
                .message,
            "Path traversal not allowed: ../a"
        );
        assert_eq!(normalise_relative_path("a/./b").unwrap(), "a/b");
    }

    #[test]
    fn safe_relative_path_counts_utf16_units() {
        let astral = "\u{1F600}";
        assert!(is_safe_relative_path(&format!(
            "{}{astral}",
            "x".repeat(198)
        )));
        assert!(!is_safe_relative_path(&format!(
            "{}{astral}",
            "x".repeat(199)
        )));
        assert!(!is_safe_relative_path("a/./b"));
        assert!(is_safe_relative_path("a/b"));
    }
}
// === P4.D377 ===

/// v4 `detectNativeText`: map a path's extension to a native-text file type, or
/// `None` when binary / not a chunkable text format.
pub fn detect_native_text(relative_path: &str) -> Option<NativeTextType> {
    match path_extname(relative_path).to_lowercase().as_str() {
        ".md" | ".markdown" => Some(NativeTextType::Markdown),
        ".txt" => Some(NativeTextType::Txt),
        ".json" => Some(NativeTextType::Json),
        ".jsonl" | ".ndjson" => Some(NativeTextType::Jsonl),
        _ => None,
    }
}

/// v4 `detectBlobFileType` (`path-utils.ts:69-74`, moved out of
/// `store-file.ts` by `40fab1c54`): the converter type of a blob path —
/// `pdf`, `docx`, or the generic `blob`.
pub fn detect_blob_file_type(relative_path: &str) -> &'static str {
    match path_extname(relative_path).to_lowercase().as_str() {
        ".pdf" => "pdf",
        ".docx" => "docx",
        _ => "blob",
    }
}

/// v4 `EXTRACTABLE_BLOB_EXTENSIONS` (`path-utils.ts:89-91`, bug 260): the
/// blob extensions whose text the doc tools can read — every native-text type
/// [`detect_native_text`] knows plus the two converter types. v4's order.
pub const EXTRACTABLE_BLOB_EXTENSIONS: &[&str] = &[
    ".md",
    ".markdown",
    ".txt",
    ".json",
    ".jsonl",
    ".ndjson",
    ".pdf",
    ".docx",
];

/// v4 `isExtractableBlobPath` (`path-utils.ts:81-83`, bug 260): whether a
/// blob at this path yields text (case-insensitive on the extension).
pub fn is_extractable_blob_path(relative_path: &str) -> bool {
    EXTRACTABLE_BLOB_EXTENSIONS.contains(&path_extname(relative_path).to_lowercase().as_str())
}

/// v4 `nativeTextAttachmentMime` (bug 38): a clean, parameter-free MIME type for
/// a native-text document, suitable for a provider attachment. Unlike
/// [`mime_for_extension`] this drops the `; charset=utf-8` suffix, because the
/// provider text whitelist and the file-attachment fallback's `is_text_file`
/// compare EXACT strings. `None` for paths that aren't a native-text type.
pub fn native_text_attachment_mime(relative_path: &str) -> Option<&'static str> {
    match detect_native_text(relative_path)? {
        NativeTextType::Markdown => Some("text/markdown"),
        NativeTextType::Txt => Some("text/plain"),
        NativeTextType::Json | NativeTextType::Jsonl => Some("application/json"),
    }
}

/// v4 `mimeForExtension`: best-effort MIME type from a path extension. Used when
/// a write has no caller-supplied MIME (cross-storage byte copies and CLI writes).
pub fn mime_for_extension(relative_path: &str) -> &'static str {
    match path_extname(relative_path).to_lowercase().as_str() {
        ".md" | ".markdown" => "text/markdown; charset=utf-8",
        ".txt" => "text/plain; charset=utf-8",
        ".json" => "application/json; charset=utf-8",
        ".jsonl" | ".ndjson" => "application/jsonl; charset=utf-8",
        ".webp" => "image/webp",
        ".png" => "image/png",
        ".jpg" | ".jpeg" => "image/jpeg",
        ".gif" => "image/gif",
        ".svg" => "image/svg+xml",
        ".pdf" => "application/pdf",
        ".docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod extractable_tests {
    use super::*;

    // v4 `lib/mount-index/__tests__/extractable-blob-paths.test.ts:11-31`.
    #[test]
    fn extractable_blob_paths_accept_the_eight_case_insensitively() {
        for p in [
            "a.md",
            "a.MARKDOWN",
            "notes/a.Txt",
            "a.json",
            "a.JSONL",
            "a.ndjson",
            "Report.PDF",
            "x/y.Docx",
        ] {
            assert!(is_extractable_blob_path(p), "{p}");
        }
    }

    #[test]
    fn extractable_blob_paths_reject_images_audio_archives_and_no_extension() {
        for p in [
            "avatar.webp",
            "photo.png",
            "song.mp3",
            "archive.zip",
            "noext",
        ] {
            assert!(!is_extractable_blob_path(p), "{p}");
        }
    }

    #[test]
    fn the_extractable_set_is_native_text_plus_the_two_converter_types() {
        let native: Vec<&str> = EXTRACTABLE_BLOB_EXTENSIONS
            .iter()
            .copied()
            .filter(|e| detect_native_text(&format!("f{e}")).is_some())
            .collect();
        assert_eq!(
            native,
            vec![".md", ".markdown", ".txt", ".json", ".jsonl", ".ndjson"]
        );
        let converters: Vec<&str> = EXTRACTABLE_BLOB_EXTENSIONS
            .iter()
            .copied()
            .filter(|e| detect_native_text(&format!("f{e}")).is_none())
            .collect();
        assert_eq!(converters, vec![".pdf", ".docx"]);
        assert_eq!(detect_blob_file_type("f.PDF"), "pdf");
        assert_eq!(detect_blob_file_type("f.docx"), "docx");
        assert_eq!(detect_blob_file_type("f.md"), "blob");
    }
}
