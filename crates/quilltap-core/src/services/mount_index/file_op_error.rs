//! Port of v4 `lib/mount-index/file-op-error.ts` — the shared error type for
//! mount-point file operations.
//!
//! v4 keeps this in its own leaf module (no other imports) so the write pipeline
//! (`store-file.ts`), the path utilities (`path-utils.ts`), the cross-mount
//! operations (`file-ops.ts`), and the HTTP status mapper (`file-op-status.ts`)
//! can share one error class without an import cycle. Keep it a leaf in Rust too.

/// v4 `FileOpErrorCode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileOpErrorCode {
    SourceNotFound,
    DestExists,
    MountNotFound,
    InvalidPath,
    Unsupported,
    VerifyFailed,
    Conflict,
}

impl FileOpErrorCode {
    /// The wire string v4 puts on `FileOpError.code` (the SPA error translation
    /// keys on this exact token).
    pub fn as_str(self) -> &'static str {
        match self {
            FileOpErrorCode::SourceNotFound => "SOURCE_NOT_FOUND",
            FileOpErrorCode::DestExists => "DEST_EXISTS",
            FileOpErrorCode::MountNotFound => "MOUNT_NOT_FOUND",
            FileOpErrorCode::InvalidPath => "INVALID_PATH",
            FileOpErrorCode::Unsupported => "UNSUPPORTED",
            FileOpErrorCode::VerifyFailed => "VERIFY_FAILED",
            FileOpErrorCode::Conflict => "CONFLICT",
        }
    }
}

/// v4 `FileOpError` — a message + a typed code. `Display` is the message
/// verbatim (user-facing steampunk copy the next round's SPA surfaces).
#[derive(Clone, Debug)]
pub struct FileOpError {
    pub message: String,
    pub code: FileOpErrorCode,
}

impl FileOpError {
    pub fn new(message: impl Into<String>, code: FileOpErrorCode) -> Self {
        Self {
            message: message.into(),
            code,
        }
    }
}

impl std::fmt::Display for FileOpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for FileOpError {}
