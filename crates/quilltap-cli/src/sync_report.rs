//! Rendering for `quilltap sync` — port of v4
//! `packages/quilltap/lib/sync-report.js` (`23da0b322`).
//!
//! Deliberately pure — actions and a colour switch in, lines out — so the
//! report's shape can be tested without a server, a store, or a directory, the
//! way `docker-mounts` is.
//!
//! ## The shape of a line
//!
//! ```text
//!     modify   store  chapters/03.md                (disk newer by 2h 14m)
//!     ^        ^      ^                             ^
//!     action   side   path                          why
//! ```
//!
//! The first two columns are fixed width so the paths line up under each other,
//! because the thing an operator actually reads down is the path column. The
//! side is the side that CHANGES: `modify store` means the store is rewritten
//! from disk, which is the opposite of the intuition some people bring and so is
//! worth being unambiguous about.
//!
//! Advisory text — warnings, the summary — goes to stderr in the caller, so
//! stdout stays greppable.
//!
//! **The shapes here are LOOSE on purpose.** v4's renderer is plain JS reading
//! whatever JSON the server sent: an action kind it does not recognize simply
//! takes no colour, and a missing key reads as `undefined`. A typed enum would
//! refuse that payload instead of rendering it, so `kind` and `side` stay
//! strings and every optional field is an `Option`. They are built out of
//! `serde_json::Value` by hand for the same reason the rest of this crate does
//! (`db_characters.rs`): the crate links `serde_json` and not `serde`, and a
//! JSON-shaped reader is what v4 is.

use serde_json::Value;

use crate::nodefmt::format_bytes as node_format_bytes;
use quilltap_core::jsstr::utf16_len;

const RESET: &str = "\x1b[0m";
const DIM: &str = "\x1b[2m";
const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";

/// Column widths. `conflict` is the longest action word at 8.
const ACTION_WIDTH: usize = 8;
const SIDE_WIDTH: usize = 6;

/// Longest path that carries a detail, capped so one outlier cannot ruin the
/// column.
const MAX_PATH_COLUMN: usize = 44;

/// One action as the server sent it.
#[derive(Debug, Clone, Default)]
pub struct ReportAction {
    pub kind: String,
    pub side: Option<String>,
    pub relative_path: String,
    pub entry_kind: Option<String>,
    pub reason: Option<String>,
    pub sha256: Option<String>,
    pub size_bytes: Option<f64>,
    pub outcome: Option<String>,
    pub error: Option<String>,
}

/// `obj[key]` as a JS string, or `None` for absent / `null` / a non-string —
/// which is what every read below then tests for truthiness anyway.
fn str_at(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn num_at(value: &Value, key: &str) -> Option<f64> {
    value.get(key).and_then(Value::as_f64)
}

fn int_at(value: &Value, key: &str) -> i64 {
    value.get(key).and_then(Value::as_i64).unwrap_or(0)
}

impl ReportAction {
    pub fn from_value(value: &Value) -> Self {
        Self {
            kind: str_at(value, "kind").unwrap_or_default(),
            side: str_at(value, "side"),
            relative_path: str_at(value, "relativePath").unwrap_or_default(),
            entry_kind: str_at(value, "entryKind"),
            reason: str_at(value, "reason"),
            sha256: str_at(value, "sha256"),
            size_bytes: num_at(value, "sizeBytes"),
            outcome: str_at(value, "outcome"),
            error: str_at(value, "error"),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ReportSummary {
    pub created: i64,
    pub modified: i64,
    pub deleted: i64,
    pub touched: i64,
    pub described: i64,
    pub conflicts: i64,
    pub skipped: i64,
    pub failed: i64,
}

impl ReportSummary {
    pub fn from_value(value: &Value) -> Self {
        Self {
            created: int_at(value, "created"),
            modified: int_at(value, "modified"),
            deleted: int_at(value, "deleted"),
            touched: int_at(value, "touched"),
            described: int_at(value, "described"),
            conflicts: int_at(value, "conflicts"),
            skipped: int_at(value, "skipped"),
            failed: int_at(value, "failed"),
        }
    }
}

/// The whole report, as the CLI reads it back off the wire.
#[derive(Debug, Clone, Default)]
pub struct SyncReportView {
    pub actions: Vec<ReportAction>,
    pub summary: ReportSummary,
    pub warnings: Vec<String>,
    pub elapsed_ms: f64,
    pub dry_run: bool,
}

impl SyncReportView {
    pub fn from_value(value: &Value) -> Self {
        Self {
            actions: value
                .get("actions")
                .and_then(Value::as_array)
                .map(|a| a.iter().map(ReportAction::from_value).collect())
                .unwrap_or_default(),
            summary: ReportSummary::from_value(value.get("summary").unwrap_or(&Value::Null)),
            // v4 `report.warnings || []`.
            warnings: value
                .get("warnings")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .map(|w| w.as_str().unwrap_or_default().to_string())
                        .collect()
                })
                .unwrap_or_default(),
            elapsed_ms: num_at(value, "elapsedMs").unwrap_or(0.0),
            dry_run: value
                .get("dryRun")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }
}

/// v4's `ACTION_COLOURS` table. An unknown kind takes `''` — no colour, and no
/// reset either.
fn action_colour(kind: &str) -> &'static str {
    match kind {
        "create" | "mkdir" | "modify" | "describe" => GREEN,
        "delete" | "rmdir" => YELLOW,
        "touch" | "skip" => DIM,
        "conflict" => RED,
        _ => "",
    }
}

/// JS `text.length >= width ? text : text + ' '.repeat(width - text.length)` —
/// `String#length` is UTF-16 code units, and the ANSI escapes v4 pads INSIDE
/// count toward it.
fn pad(text: &str, width: usize) -> String {
    let len = utf16_len(text);
    if len >= width {
        text.to_string()
    } else {
        format!("{text}{}", " ".repeat(width - len))
    }
}

/// v4 `formatBytes`.
///
/// Measured at the pin: the body is byte-identical to `docs-commands.js`'s,
/// which [`crate::nodefmt::format_bytes`] already ports (the same four branches,
/// the same `toFixed` digits, the same division spellings). The ONLY difference
/// is the absent-value guard this one adds, so the port reuses the existing
/// kernel rather than minting a fourth copy.
pub fn format_bytes(n: Option<f64>) -> String {
    match n {
        None => String::new(),
        Some(n) => node_format_bytes(n),
    }
}

/// v4 `detailFor` — the parenthetical after a path: whatever the engine said,
/// plus the sha and size on a line that put bytes somewhere.
pub fn detail_for(action: &ReportAction) -> String {
    let mut parts: Vec<String> = Vec::new();
    // v4 `action.outcome === 'failed' && action.error` — JS truthiness, so an
    // empty error string falls through to the reason.
    let failed_with_error = action.outcome.as_deref() == Some("failed")
        && action.error.as_deref().is_some_and(|e| !e.is_empty());
    if failed_with_error {
        parts.push(format!("FAILED: {}", action.error.as_deref().unwrap_or("")));
    } else if let Some(reason) = action.reason.as_deref().filter(|r| !r.is_empty()) {
        parts.push(reason.to_string());
    }
    if (action.kind == "create" || action.kind == "modify")
        && action.sha256.as_deref().is_some_and(|s| !s.is_empty())
    {
        let sha = action.sha256.as_deref().unwrap_or("");
        let size = format_bytes(action.size_bytes);
        let tail = if size.is_empty() {
            String::new()
        } else {
            format!(", {size}")
        };
        // `sha256.slice(0, 4)` on a JS string — UTF-16 units; a sha is hex, so
        // this is four bytes in practice.
        let short: String = sha.chars().take(4).collect();
        parts.push(format!("sha {short}\u{2026}{tail}"));
    }
    parts.join("; ")
}

/// v4 `displayPath` — a trailing slash marks a folder, so `drafts/` and a file
/// called `drafts` are not the same line.
pub fn display_path(action: &ReportAction) -> String {
    if action.entry_kind.as_deref() == Some("folder") {
        format!("{}/", action.relative_path)
    } else {
        action.relative_path.clone()
    }
}

/// v4 `formatActionLine`.
pub fn format_action_line(action: &ReportAction, colour: bool, path_width: usize) -> String {
    let tint = if colour {
        if action.outcome.as_deref() == Some("failed") {
            RED
        } else {
            action_colour(&action.kind)
        }
    } else {
        ""
    };
    // v4 `const reset = tint ? RESET : ''` — an unknown kind under colour takes
    // neither.
    let reset = if tint.is_empty() { "" } else { RESET };
    // v4 `action.side || '—'` — JS truthiness, so `null` AND an empty string
    // both take the em dash.
    let side = match action.side.as_deref() {
        Some(s) if !s.is_empty() => s,
        _ => "\u{2014}",
    };
    let detail = detail_for(action);
    let shown_path = if detail.is_empty() {
        display_path(action)
    } else {
        pad(&display_path(action), path_width)
    };
    let head = format!(
        "{tint}{}{reset} {} {shown_path}",
        pad(&action.kind, ACTION_WIDTH),
        pad(side, SIDE_WIDTH),
    );
    if detail.is_empty() {
        return head;
    }
    if colour {
        format!("{head}  {DIM}({detail}){RESET}")
    } else {
        format!("{head}  ({detail})")
    }
}

/// v4 `formatActionLines` — one line per action, with the path column measured
/// once across the whole plan so a single very long path does not push every
/// other line's detail off the screen.
pub fn format_action_lines(actions: &[ReportAction], colour: bool) -> Vec<String> {
    let widest = actions.iter().fold(0usize, |widest, action| {
        if detail_for(action).is_empty() {
            widest
        } else {
            widest.max(utf16_len(&display_path(action)))
        }
    });
    let path_width = MAX_PATH_COLUMN.min(widest);
    actions
        .iter()
        .map(|action| format_action_line(action, colour, path_width))
        .collect()
}

/// v4 `formatSummary` — `3 created, 1 modified, … — 0.8 s`, or a plain "nothing
/// to do".
pub fn format_summary(summary: &ReportSummary, elapsed_ms: f64, dry_run: bool) -> String {
    let mut bits: Vec<String> = Vec::new();
    // v4 tests each count for JS truthiness, so a zero is simply absent. The
    // ORDER is v4's, and it is not the summary struct's: `skipped` comes before
    // `conflicts`, and `failed` is last.
    if summary.created != 0 {
        bits.push(format!("{} created", summary.created));
    }
    if summary.modified != 0 {
        bits.push(format!("{} modified", summary.modified));
    }
    if summary.deleted != 0 {
        bits.push(format!("{} deleted", summary.deleted));
    }
    if summary.touched != 0 {
        bits.push(format!("{} touched", summary.touched));
    }
    if summary.described != 0 {
        bits.push(format!("{} described", summary.described));
    }
    if summary.skipped != 0 {
        bits.push(format!("{} skipped", summary.skipped));
    }
    if summary.conflicts != 0 {
        bits.push(format!(
            "{} conflict{}",
            summary.conflicts,
            if summary.conflicts == 1 { "" } else { "s" }
        ));
    }
    if summary.failed != 0 {
        bits.push(format!("{} failed", summary.failed));
    }

    let seconds = format!(
        "{} s",
        quilltap_core::jsnum::to_fixed(elapsed_ms / 1000.0, 1)
    );
    if bits.is_empty() {
        return if dry_run {
            format!("Nothing to do \u{2014} {seconds}")
        } else {
            format!("Already in step \u{2014} {seconds}")
        };
    }
    let prefix = if dry_run { "Would do: " } else { "" };
    format!("{prefix}{} \u{2014} {seconds}", bits.join(", "))
}

/// v4 `exitCodeFor` — the process exit code a report earns.
///
///   0  clean
///   1  something failed outright
///   2  at least one conflict is still unresolved
///
/// `--dry-run` uses the same codes, so a script can gate on a clean plan before
/// it lets a real run proceed.
pub fn exit_code_for(summary: &ReportSummary) -> i32 {
    if summary.failed > 0 {
        return 1;
    }
    if summary.conflicts > 0 {
        return 2;
    }
    0
}
