//! Port of v4's Prospero terminal tools — `terminal_read` and `terminal_list`
//! (one v4 file, `lib/tools/handlers/terminal-handler.ts`, + the tool schemas
//! `lib/tools/terminal-read-tool.ts` / `lib/tools/terminal-list-tool.ts` and the
//! dispatch rows in `lib/chat/tool-executor.ts`).
//!
//! `terminal_read` returns cleaned scrollback (a line window) from a terminal
//! session in the chat; `terminal_list` summarizes every session in the chat.
//! The terminal is operator-controlled — these tools only READ.
//!
//! ## The scrollback-source seam (host-side I/O, injected)
//!
//! v4's `executeTerminalReadTool` resolves the raw scrollback bytes from one of
//! two host runtime sources:
//!   - the live PTY ring buffer (`ptyManager.getRingBuffer(sessionId)`), used
//!     only when a live session exists AND `session.exitedAt == null`; else
//!   - the transcript file on disk (`tailFile(session.transcriptPath)`, tailing
//!     the last ≤1 MB), used when `transcriptPath` is set; else `''`.
//!
//! Both are host runtime I/O (a PTY manager and the filesystem) with no ported
//! equivalent in `quilltap-core`. So the resolved raw scrollback is modeled as an
//! **injected input**: [`execute_terminal_read`] takes `full_content: &str` (the
//! bytes v4 would have read) and the differential feeds identical bytes on both
//! sides (the oracle seeds the session with a `transcriptPath` pointing at a
//! fixture text file, so v4's `tailFile` reads exactly those bytes; the Rust side
//! is handed the same bytes). **Everything else** — the session lookup
//! (`terminal_sessions` repo), the `splitLines` / `sliceByRange` / `tailByCount` /
//! `resolveIndex` line math, `cleanTerminalOutput`, the status / exitCode / range
//! fields, the `raw` → `rawScrollback` branch, and `MAX_RETURN_LINES = 2000` — is
//! ported here and differential-verified.
//!
//! The `status` field is `"live"` when `session.exitedAt == null`, else
//! `"exited"` (v4 uses this same rule; whether a live PTY actually exists is a
//! host concern the seam abstracts away — it only affects which SOURCE the bytes
//! came from, not the emitted fields).
//!
//! ## Error handling
//!
//! v4 THROWS a `TerminalToolError` on no-chat-context / session-not-found /
//! wrong-chat, and the dispatcher (`tool-executor.ts`) catches it into a failure
//! `ToolResult`. The validators run BEFORE `execute`; an invalid-input dispatch
//! returns a fixed error string without calling `execute`. This port mirrors that
//! split: [`validate_terminal_read_input`] / [`validate_terminal_list_input`] gate
//! the call, and [`execute_terminal_read`] / [`execute_terminal_list`] return
//! `Result<_, TerminalToolError>` (the caller maps the error into the failure
//! result the same way the dispatcher does).

use serde::Serialize;
use serde_json::Value;

use crate::db::runtime::Db;
use crate::db::{terminal_sessions, DbError};
use crate::terminal_clean::clean_terminal_output;
use crate::tools::llm_number::llm_number;

/// v4 `MAX_RETURN_LINES` — the per-read line cap for both slice and tail modes.
const MAX_RETURN_LINES: usize = 2000;

/// v4's `TerminalToolError` (a thrown `Error` the dispatcher catches). The
/// `Db` variant carries an unexpected database failure (v4 has no analogue — a DB
/// error there surfaces through `safeQuery`; here it is surfaced explicitly).
///
/// The `Display` strings for the first three variants are v4's exact thrown
/// messages (the dispatcher surfaces `err.message` as the failure result's
/// `error`).
#[derive(Debug)]
pub enum TerminalToolError {
    /// v4: `'No chat context available'`.
    NoChatContext,
    /// v4: `` `Terminal session not found: ${sessionId}` ``.
    SessionNotFound(String),
    /// v4: `'Session does not belong to this chat'`.
    WrongChat,
    /// An unexpected database error (no v4 analogue at this layer).
    Db(DbError),
}

impl std::fmt::Display for TerminalToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TerminalToolError::NoChatContext => write!(f, "No chat context available"),
            TerminalToolError::SessionNotFound(id) => {
                write!(f, "Terminal session not found: {id}")
            }
            TerminalToolError::WrongChat => write!(f, "Session does not belong to this chat"),
            TerminalToolError::Db(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for TerminalToolError {}

impl From<DbError> for TerminalToolError {
    fn from(e: DbError) -> Self {
        TerminalToolError::Db(e)
    }
}

/// Parsed `terminal_read` input (v4 `TerminalReadInput`). The Zod schema:
/// `sessionId` required string; `lines` int 1..=2000 default 200 optional; `start`
/// / `end` int optional; `raw` bool default false optional.
///
/// The Zod `.default()` on `lines` / `raw` only materializes when the KEY IS
/// PRESENT (the field is `.default(...).optional()` — absent → `undefined`, not
/// the default). The handler reads `input.lines ?? 200` (so an absent `lines`
/// behaves as 200 anyway) and `input.raw === true` (absent → false). This port
/// keeps `lines` / `raw` as `Option` and applies the same `?? 200` / `=== true`
/// downstream, matching v4 whether the key is absent or defaulted.
#[derive(Debug, Clone, PartialEq)]
pub struct TerminalReadInput {
    pub session_id: String,
    pub lines: Option<i64>,
    pub start: Option<i64>,
    pub end: Option<i64>,
    pub raw: Option<bool>,
}

/// v4 `TerminalReadOutput`. Field order matches v4's object-literal construction
/// (the order `JSON.stringify` emits). `rawScrollback` is omitted unless
/// `raw === true` (v4 conditionally assigns the key).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TerminalReadOutput {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub shell: String,
    pub cwd: String,
    pub status: TerminalStatus,
    #[serde(rename = "exitCode")]
    pub exit_code: Option<i64>,
    pub lines: i64,
    #[serde(rename = "totalLines")]
    pub total_lines: i64,
    #[serde(rename = "startLine")]
    pub start_line: i64,
    #[serde(rename = "endLine")]
    pub end_line: i64,
    pub truncated: bool,
    pub scrollback: String,
    #[serde(rename = "rawScrollback", skip_serializing_if = "Option::is_none")]
    pub raw_scrollback: Option<String>,
}

/// v4 `TerminalListOutput`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TerminalListOutput {
    pub sessions: Vec<TerminalListEntry>,
}

/// One entry of the `terminal_list` output. Field order matches v4's
/// object-literal construction.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TerminalListEntry {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub label: Option<String>,
    pub shell: String,
    pub cwd: String,
    #[serde(rename = "startedAt")]
    pub started_at: String,
    pub status: TerminalStatus,
    #[serde(rename = "exitCode")]
    pub exit_code: Option<i64>,
}

/// The session status — v4's `'live' | 'exited'` string literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TerminalStatus {
    Live,
    Exited,
}

impl TerminalStatus {
    /// v4: `session.exitedAt == null ? 'live' : 'exited'`.
    fn from_exited_at(exited_at: &Option<String>) -> Self {
        if exited_at.is_none() {
            TerminalStatus::Live
        } else {
            TerminalStatus::Exited
        }
    }
}

// ============================================================================
// Line math (v4's private helpers)
// ============================================================================

/// v4 `splitLines`: `text.split(/\r?\n/)`. An empty string splits to `[""]`
/// (one element), matching JS `''.split(/\r?\n/) === ['']`.
fn split_lines(text: &str) -> Vec<&str> {
    // JS `.split(/\r?\n/)` splits on each `\n` optionally preceded by `\r`.
    // Reproduce by splitting on '\n' after stripping a trailing '\r' from each
    // piece — but the regex splits at the delimiter, so a "\r\n" is one split
    // point. Implement by scanning.
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            // delimiter is "\r?\n": if the char before is '\r', the segment ends
            // before that '\r'.
            let seg_end = if i > start && bytes[i - 1] == b'\r' {
                i - 1
            } else {
                i
            };
            out.push(&text[start..seg_end]);
            start = i + 1;
            i += 1;
        } else {
            i += 1;
        }
    }
    out.push(&text[start..]);
    out
}

/// v4 `resolveIndex`: a negative value counts back from `lastIndex`
/// (`lastIndex - abs(value)`); a non-negative value is returned as-is.
fn resolve_index(value: i64, last_index: i64) -> i64 {
    if value < 0 {
        last_index - value.abs()
    } else {
        value
    }
}

/// The result of a slice/tail (v4's `SliceResult`).
struct SliceResult {
    text: String,
    count: i64,
    start_line: i64,
    end_line: i64,
    truncated: bool,
}

/// v4 `sliceByRange`.
fn slice_by_range(all: &[&str], start: Option<i64>, end: Option<i64>) -> SliceResult {
    let total = all.len() as i64;
    if total == 0 {
        return SliceResult {
            text: String::new(),
            count: 0,
            start_line: 0,
            end_line: 0,
            truncated: false,
        };
    }
    let last_index = total - 1;
    let raw_start = match start {
        None => 0,
        Some(s) => resolve_index(s, last_index),
    };
    let raw_end = match end {
        None => last_index,
        Some(e) => resolve_index(e, last_index),
    };
    let lo = 0.max(raw_start.min(raw_end));
    let hi = last_index.min(raw_start.max(raw_end));
    let mut truncated = false;
    let mut effective_hi = hi;
    if hi - lo + 1 > MAX_RETURN_LINES as i64 {
        effective_hi = lo + MAX_RETURN_LINES as i64 - 1;
        truncated = true;
    }
    let slice = &all[lo as usize..=(effective_hi as usize)];
    SliceResult {
        text: slice.join("\n"),
        count: slice.len() as i64,
        start_line: lo,
        end_line: effective_hi,
        truncated,
    }
}

/// v4 `tailByCount`.
fn tail_by_count(all: &[&str], n: i64) -> SliceResult {
    let total = all.len() as i64;
    if total == 0 {
        return SliceResult {
            text: String::new(),
            count: 0,
            start_line: 0,
            end_line: 0,
            truncated: false,
        };
    }
    let cap = n.min(MAX_RETURN_LINES as i64);
    let lo = 0.max(total - cap);
    let hi = total - 1;
    let slice = &all[lo as usize..=(hi as usize)];
    SliceResult {
        text: slice.join("\n"),
        count: slice.len() as i64,
        start_line: lo,
        end_line: hi,
        truncated: lo > 0,
    }
}

// ============================================================================
// Validators (v4's Zod `validate*` — the dispatcher gates on these)
// ============================================================================

/// v4 `validateTerminalReadInput` (`terminalReadToolInputSchema.safeParse(...)
/// .success`). Returns the parsed input on success, `None` on failure. On failure
/// the dispatcher returns `'Invalid arguments for terminal_read: sessionId is
/// required'` WITHOUT calling execute.
///
/// The Zod schema: `sessionId` required string (no min-length — an empty string
/// passes); `lines` int 1..=2000 optional; `start`/`end` int optional; `raw` bool
/// optional. Zod strips unknown keys by default. This validator reproduces the
/// success/failure decision and the coercion (int-ness, ranges) the schema
/// enforces.
pub fn validate_terminal_read_input(args: &Value) -> Option<TerminalReadInput> {
    let obj = args.as_object()?;

    // sessionId: required, must be a string.
    let session_id = obj.get("sessionId")?.as_str()?.to_string();

    // lines: optional; when present must be an integer in [1, 2000].
    let lines = match obj.get("lines") {
        None | Some(Value::Null) => None,
        Some(v) => {
            let n = json_int(v)?;
            if !(1..=2000).contains(&n) {
                return None;
            }
            Some(n)
        }
    };

    // start / end: optional; when present must be an integer (any range).
    let start = match obj.get("start") {
        None | Some(Value::Null) => None,
        Some(v) => Some(json_int(v)?),
    };
    let end = match obj.get("end") {
        None | Some(Value::Null) => None,
        Some(v) => Some(json_int(v)?),
    };

    // raw: optional; when present must be a boolean.
    let raw = match obj.get("raw") {
        None | Some(Value::Null) => None,
        Some(Value::Bool(b)) => Some(*b),
        Some(_) => return None,
    };

    Some(TerminalReadInput {
        session_id,
        lines,
        start,
        end,
        raw,
    })
}

/// v4 `validateTerminalListInput` (`z.object({}).safeParse(...).success`). An
/// empty object schema; Zod strips unknown keys, so any object (or, since JS
/// `safeParse` coerces, any value Zod accepts as an object) passes. v4's
/// dispatcher passes `toolCall.arguments` which is always an object, so this
/// returns `true` for any JSON object and `false` otherwise. On failure the
/// dispatcher returns `'Invalid arguments for terminal_list'`.
pub fn validate_terminal_list_input(args: &Value) -> bool {
    args.is_object()
}

/// Coerce a JSON number to an `i64` iff it is an exact integer (v4 Zod
/// `.number().int()` rejects non-integers). Rejects non-numbers.
fn json_int(v: &Value) -> Option<i64> {
    // `lines`/`start`/`end` are all llmNumber(...) fields, so a numeric-looking
    // string converts before the integer check below. Negatives stay legal for
    // `start`/`end` (v4 bounds only `lines`), which is why this keeps its own
    // integer test rather than borrowing a bounded one.
    let v = &*llm_number(v);
    match v {
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(i)
            } else if let Some(f) = n.as_f64() {
                if f.fract() == 0.0 && f >= i64::MIN as f64 && f <= i64::MAX as f64 {
                    Some(f as i64)
                } else {
                    None
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

// ============================================================================
// Handlers
// ============================================================================

/// v4 `executeTerminalReadTool`. `chat_id` is the tool context's chat (v4's
/// `ctx.chatId`) — an empty string models a missing chat context (v4 throws on
/// `!ctx.chatId`). `full_content` is the injected raw scrollback (see the module
/// header seam note).
pub async fn execute_terminal_read(
    db: &Db,
    chat_id: &str,
    args: &TerminalReadInput,
    full_content: &str,
) -> Result<TerminalReadOutput, TerminalToolError> {
    if chat_id.is_empty() {
        return Err(TerminalToolError::NoChatContext);
    }

    let session_id = args.session_id.clone();
    let session = db.read_main(|conn| terminal_sessions::find_by_id(conn, &session_id))?;

    let session = match session {
        Some(s) => s,
        None => return Err(TerminalToolError::SessionNotFound(args.session_id.clone())),
    };

    if session.chat_id != chat_id {
        return Err(TerminalToolError::WrongChat);
    }

    // The raw scrollback bytes are injected (host seam). v4 resolves them from the
    // live ring buffer or the transcript file; here `full_content` IS the resolved
    // bytes (identical on both sides of the differential).
    let all_lines = split_lines(full_content);
    let total_lines = all_lines.len() as i64;

    let use_slice = args.start.is_some() || args.end.is_some();
    let result = if use_slice {
        slice_by_range(&all_lines, args.start, args.end)
    } else {
        tail_by_count(&all_lines, args.lines.unwrap_or(200))
    };

    let cleaned = clean_terminal_output(&result.text);
    let status = TerminalStatus::from_exited_at(&session.exited_at);

    let mut output = TerminalReadOutput {
        session_id: args.session_id.clone(),
        shell: session.shell,
        cwd: session.cwd,
        status,
        exit_code: session.exit_code,
        lines: result.count,
        total_lines,
        start_line: result.start_line,
        end_line: result.end_line,
        truncated: result.truncated,
        scrollback: cleaned,
        raw_scrollback: None,
    };

    if args.raw == Some(true) {
        output.raw_scrollback = Some(result.text);
    }

    Ok(output)
}

/// v4 `executeTerminalListTool`.
pub async fn execute_terminal_list(
    db: &Db,
    chat_id: &str,
) -> Result<TerminalListOutput, TerminalToolError> {
    if chat_id.is_empty() {
        return Err(TerminalToolError::NoChatContext);
    }

    let cid = chat_id.to_string();
    let sessions = db.read_main(|conn| terminal_sessions::find_by_chat_id(conn, &cid))?;

    let entries = sessions
        .into_iter()
        .map(|s| TerminalListEntry {
            session_id: s.id,
            label: s.label,
            shell: s.shell,
            cwd: s.cwd,
            started_at: s.started_at,
            status: TerminalStatus::from_exited_at(&s.exited_at),
            exit_code: s.exit_code,
        })
        .collect();

    Ok(TerminalListOutput { sessions: entries })
}

// ============================================================================
// Formatters (the LLM-visible text; the emoji badges are load-bearing)
// ============================================================================

/// v4 `formatTerminalReadResults`. The `🔴 LIVE` / `⚫ EXITED` badge is
/// load-bearing.
pub fn format_terminal_read(out: &TerminalReadOutput) -> String {
    let status_badge = match out.status {
        TerminalStatus::Live => "🔴 LIVE",
        TerminalStatus::Exited => "⚫ EXITED",
    };
    let exit_code_str = match out.exit_code {
        Some(code) => format!(" (exit code: {code})"),
        None => String::new(),
    };
    let range_str = if out.total_lines > 0 {
        // v4 uses the EN DASH (U+2013) between start and end.
        format!(
            "Lines: {} (showing {}\u{2013}{} of {})",
            out.lines, out.start_line, out.end_line, out.total_lines
        )
    } else {
        "Lines: 0".to_string()
    };
    let truncation_str = if out.truncated {
        // v4 uses the EM DASH (U+2014).
        "\n\n[scrollback truncated \u{2014} output exceeded the per-read line cap]"
    } else {
        ""
    };
    let raw_block = match &out.raw_scrollback {
        Some(raw) => format!("\n\nRaw (ANSI preserved):\n```\n{raw}\n```"),
        None => String::new(),
    };

    format!(
        "{status_badge}{exit_code_str}\n\
         Shell: {}\n\
         Working Directory: {}\n\
         {range_str}\n\
         \n\
         ```\n\
         {}\n\
         ```{truncation_str}{raw_block}",
        out.shell, out.cwd, out.scrollback
    )
}

/// v4 `formatTerminalListResults`. The `🔴` / `⚫` badges are load-bearing.
pub fn format_terminal_list(out: &TerminalListOutput) -> String {
    if out.sessions.is_empty() {
        return "No terminal sessions in this chat.".to_string();
    }

    let lines: Vec<String> = out
        .sessions
        .iter()
        .map(|s| {
            let status_emoji = match s.status {
                TerminalStatus::Live => "🔴",
                TerminalStatus::Exited => "⚫",
            };
            let label = match &s.label {
                Some(l) => format!(" \"{l}\""),
                None => String::new(),
            };
            let exit_str = match s.exit_code {
                Some(code) => format!(" [exit {code}]"),
                None => String::new(),
            };
            // v4 uses the EM DASH (U+2014) between the id and the shell/cwd.
            format!(
                "{status_emoji} {}{label} \u{2014} {} in {}{exit_str}",
                s.session_id, s.shell, s.cwd
            )
        })
        .collect();

    format!("**Terminal Sessions**\n\n{}", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn split_lines_matches_js() {
        assert_eq!(split_lines(""), vec![""]);
        assert_eq!(split_lines("a"), vec!["a"]);
        assert_eq!(split_lines("a\nb"), vec!["a", "b"]);
        assert_eq!(split_lines("a\r\nb"), vec!["a", "b"]);
        assert_eq!(split_lines("a\nb\n"), vec!["a", "b", ""]);
        assert_eq!(split_lines("a\r\nb\r\n"), vec!["a", "b", ""]);
    }

    #[test]
    fn resolve_index_negative_counts_back() {
        assert_eq!(resolve_index(-1, 9), 8);
        assert_eq!(resolve_index(-50, 99), 49);
        assert_eq!(resolve_index(3, 9), 3);
    }

    #[test]
    fn slice_negative_window() {
        // 100 lines (indices 0..99, lastIndex=99). start=-50 => 99-50=49;
        // end=-1 => 99-1=98. So lo=49, hi=98, count=50.
        let lines: Vec<String> = (0..100).map(|i| format!("L{i}")).collect();
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let r = slice_by_range(&refs, Some(-50), Some(-1));
        assert_eq!(r.start_line, 49);
        assert_eq!(r.end_line, 98);
        assert_eq!(r.count, 50);
        assert!(!r.truncated);
    }

    #[test]
    fn tail_default() {
        let lines: Vec<String> = (0..500).map(|i| format!("L{i}")).collect();
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let r = tail_by_count(&refs, 200);
        assert_eq!(r.start_line, 300);
        assert_eq!(r.end_line, 499);
        assert_eq!(r.count, 200);
        assert!(r.truncated);
    }

    #[test]
    fn validate_read_ok_and_bad() {
        assert!(validate_terminal_read_input(&json!({ "sessionId": "s1" })).is_some());
        assert!(validate_terminal_read_input(
            &json!({ "sessionId": "s1", "lines": 50, "raw": true })
        )
        .is_some());
        // Missing sessionId -> None.
        assert!(validate_terminal_read_input(&json!({ "lines": 50 })).is_none());
        // Non-string sessionId -> None.
        assert!(validate_terminal_read_input(&json!({ "sessionId": 5 })).is_none());
        // lines out of range -> None.
        assert!(
            validate_terminal_read_input(&json!({ "sessionId": "s", "lines": 5000 })).is_none()
        );
        assert!(validate_terminal_read_input(&json!({ "sessionId": "s", "lines": 0 })).is_none());
        // Non-integer start -> None.
        assert!(validate_terminal_read_input(&json!({ "sessionId": "s", "start": 1.5 })).is_none());
    }

    #[test]
    fn validate_list_ok_and_bad() {
        assert!(validate_terminal_list_input(&json!({})));
        assert!(validate_terminal_list_input(&json!({ "extra": 1 })));
        assert!(!validate_terminal_list_input(&json!("nope")));
        assert!(!validate_terminal_list_input(&json!(5)));
    }

    #[test]
    fn read_output_serialization_omits_raw() {
        let out = TerminalReadOutput {
            session_id: "s1".into(),
            shell: "/bin/zsh".into(),
            cwd: "/tmp".into(),
            status: TerminalStatus::Live,
            exit_code: None,
            lines: 2,
            total_lines: 2,
            start_line: 0,
            end_line: 1,
            truncated: false,
            scrollback: "a\nb".into(),
            raw_scrollback: None,
        };
        let v = serde_json::to_value(&out).unwrap();
        assert!(v.get("rawScrollback").is_none());
        assert_eq!(v["status"], "live");
        assert_eq!(v["exitCode"], Value::Null);
    }

    #[test]
    fn format_read_badges() {
        let out = TerminalReadOutput {
            session_id: "s1".into(),
            shell: "/bin/zsh".into(),
            cwd: "/home".into(),
            status: TerminalStatus::Exited,
            exit_code: Some(137),
            lines: 1,
            total_lines: 1,
            start_line: 0,
            end_line: 0,
            truncated: false,
            scrollback: "boom".into(),
            raw_scrollback: None,
        };
        let s = format_terminal_read(&out);
        assert!(s.starts_with("\u{26AB} EXITED (exit code: 137)"));
        assert!(s.contains("Shell: /bin/zsh"));
        assert!(s.contains("Lines: 1 (showing 0\u{2013}0 of 1)"));
    }

    #[test]
    fn format_list_empty() {
        let out = TerminalListOutput { sessions: vec![] };
        assert_eq!(
            format_terminal_list(&out),
            "No terminal sessions in this chat."
        );
    }
}
