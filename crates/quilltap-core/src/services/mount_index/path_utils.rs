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
fn path_extname(path: &str) -> String {
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

/// v4 `normaliseRelativePath`: collapse `./` and redundant separators, strip
/// leading/trailing slashes, and reject `..` traversal. Errors with
/// `FileOpError('INVALID_PATH')` on an empty or traversing path.
pub fn normalise_relative_path(input: &str) -> Result<String, FileOpError> {
    // path.normalize FIRST (on the raw input — backslashes are ordinary chars on
    // posix), THEN backslash→slash, THEN strip leading/trailing slashes.
    let normalized = path_posix_normalize(input)
        .replace('\\', "/")
        .trim_start_matches('/')
        .trim_end_matches('/')
        .to_string();
    if normalized.is_empty() || normalized == "." {
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
