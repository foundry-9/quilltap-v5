//! `quilltap anchor-probe <characterId>` — measure whether the episodic anchor
//! line embedded with each memory suppresses Memory Gate reinforcement (P4.D269;
//! v4 `packages/quilltap/lib/anchor-probe-command.js`, `3f7320138`, ported
//! line-for-line: flag parsing, the help text, the stderr progress line, the
//! band table, the row table, `--json`).
//!
//! Like v4's, a thin HTTP wrapper over the running server. v4 POSTs
//! `/api/v1/memories?action=anchor-gate-probe`; v5 has no memories REST family,
//! so it POSTs the same verb through its own dispatch endpoint
//! (`/api/dispatch`, `memoryAnchorGateProbe` — P4.D269 R-A, the
//! `recall_replay_cmd.rs` precedent). The posted body is v4's `{characterId,
//! limit?}` with `"type":"memoryAnchorGateProbe"` in front. `payload.data ??
//! payload` is v4's own line and handles both envelopes; the failure sentence
//! adds the dispatch envelope's `data.message` arm. Unlike `recall-replay`,
//! v4's probe prints NO `details` lines on a 400, and its table pads by the
//! ANSI-stripped width (recall-replay pads by the raw `padEnd`).
//!
//! Read-only, but it costs embedding calls (roughly up to 7 per sampled row).

use serde_json::Value;

use crate::http;
use crate::nodefmt;
use crate::out;

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";
const CYAN: &str = "\x1b[36m";

/// v4 `printAnchorProbeHelp` — a BYTE CAPTURE of `node
/// packages/quilltap/bin/quilltap.js anchor-probe --help` stdout at v4
/// `01a83539d` (P4.D269 R-B).
const HELP: &str = include_str!("help/anchor_probe_help.txt");

struct Flags {
    limit: Option<i64>,
    port: i64,
    json: bool,
    help: bool,
}

/// v4 `parseFlags` — position-independent; an unknown `-…` refuses at PARSE
/// time, so it beats `--help`.
fn parse_flags(args: &[String]) -> (Flags, Vec<String>) {
    let mut flags = Flags {
        limit: None,
        port: 3000,
        json: false,
        help: false,
    };
    let mut positional: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--limit" => {
                // `parseInt(args[++i], 10)` — `1e2` parses to 1 and is accepted.
                i += 1;
                match nodefmt::js_parse_int(args.get(i).map(String::as_str)) {
                    Some(v) if (1..=200).contains(&v) => flags.limit = Some(v),
                    _ => {
                        out::elog("Error: --limit must be between 1 and 200");
                        out::exit(1);
                    }
                }
            }
            "--port" => {
                i += 1;
                match nodefmt::js_parse_int(args.get(i).map(String::as_str)) {
                    Some(v) if (1..=65535).contains(&v) => flags.port = v,
                    _ => {
                        out::elog("Error: --port must be between 1 and 65535");
                        out::exit(1);
                    }
                }
            }
            "--json" => flags.json = true,
            "-h" | "--help" => flags.help = true,
            other => {
                if other.starts_with('-') {
                    out::elog(&format!("Unknown option: {other}"));
                    out::exit(1);
                }
                positional.push(other.to_string());
            }
        }
        i += 1;
    }
    (flags, positional)
}

/// JS `String.prototype.padEnd` over UTF-16 units, escapes counted (the band
/// lines' raw `padEnd`).
fn pad_end(s: &str, width: usize) -> String {
    let len = nodefmt::utf16_len(s);
    let mut out = s.to_string();
    if len < width {
        out.extend(std::iter::repeat_n(' ', width - len));
    }
    out
}

/// v4 `pad(s, width)` — pads a possibly colour-wrapped string to a VISIBLE
/// width: the UTF-16 length after stripping `/\x1b\[[0-9;]*m/g`.
fn pad(s: &str, width: usize) -> String {
    let visible = nodefmt::utf16_len(&strip_ansi(s));
    let mut out = s.to_string();
    if visible < width {
        out.extend(std::iter::repeat_n(' ', width - visible));
    }
    out
}

/// `s.replace(/\x1b\[[0-9;]*m/g, '')`.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(pos) = rest.find("\x1b[") {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + 2..];
        let run = after
            .find(|c: char| !(c.is_ascii_digit() || c == ';'))
            .unwrap_or(after.len());
        if after[run..].starts_with('m') {
            rest = &after[run + 1..];
        } else {
            out.push_str("\x1b[");
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// v4 `fmt(n, digits)`: `—` (dimmed) for null/undefined, `n.toFixed(digits)`
/// otherwise. A non-number throws into the launcher's generic catch.
fn fmt(v: Option<&Value>, digits: u32) -> String {
    match v {
        None | Some(Value::Null) => format!("{DIM}—{RESET}"),
        Some(Value::Number(n)) => {
            quilltap_core::jsnum::to_fixed(n.as_f64().unwrap_or(f64::NAN), digits)
        }
        Some(_) => {
            out::elog("Error: n.toFixed is not a function");
            out::exit(1);
        }
    }
}

/// JS relational coercion of a JSON value to a number (`undefined`/`null` →
/// `NaN`/`0` never matter here: every caller compares with `>`/`>=`, where
/// both fail; a string goes through `Number(s)`).
fn as_js_number(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
        Some(Value::String(s)) => quilltap_core::jsnum::number_from_str(s),
        Some(Value::Bool(b)) => f64::from(u8::from(*b)),
        Some(Value::Null) => 0.0,
        _ => f64::NAN,
    }
}

/// `(s || '').replace(/\s+/g, ' ')` — every run of JS whitespace becomes one
/// space.
fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    for c in s.chars() {
        if quilltap_core::jsstr::is_js_ws(c) {
            if !in_ws {
                out.push(' ');
            }
            in_ws = true;
        } else {
            out.push(c);
            in_ws = false;
        }
    }
    out
}

/// v4 `printRows(rows, thresholds)`.
fn print_rows(rows: &[Value], thresholds: &Value) {
    out::log(&format!("\n{BOLD}Rows{RESET} ({})", rows.len()));
    if rows.is_empty() {
        out::log(&format!("  {DIM}(none){RESET}"));
        return;
    }
    out::log(&format!(
        "  {DIM}{}{}{}{}summary{RESET}",
        pad_end("anchored", 10),
        pad_end("free", 10),
        pad_end("delta", 9),
        pad_end("anchor", 8)
    ));
    // `thresholds?.reinforce ?? 0.85`.
    let reinforce_value = match thresholds.get("reinforce") {
        None | Some(Value::Null) => Value::from(0.85),
        Some(v) => v.clone(),
    };
    let reinforce = as_js_number(Some(&reinforce_value));
    for row in rows {
        let a = row.get("anchoredBest");
        let f = row.get("anchorFreeBest");
        let a_num = a.and_then(|v| v.is_number().then(|| as_js_number(Some(v))));
        let f_num = f.and_then(|v| v.is_number().then(|| as_js_number(Some(v))));
        let delta = match (a_num, f_num) {
            (Some(a), Some(f)) => Some(f - a),
            _ => None,
        };
        let only_free =
            f_num.is_some_and(|f| f >= reinforce) && !a_num.is_some_and(|a| a >= reinforce);
        let delta_str = match delta {
            None => format!("{DIM}—{RESET}"),
            Some(d) => format!(
                "{}{}",
                if d >= 0.0 { "+" } else { "" },
                quilltap_core::jsnum::to_fixed(d, 3)
            ),
        };
        let anchor_str = if nodefmt::js_truthy(row.get("hasAnchorLine")) {
            "yes".to_string()
        } else {
            format!("{DIM}no{RESET}")
        };
        let summary = nodefmt::slice_utf16(
            &collapse_ws(row.get("summary").and_then(Value::as_str).unwrap_or("")),
            60,
        );
        let line = format!(
            "{}{}{}{}{summary}",
            pad(&fmt(a, 3), 10),
            pad(&fmt(f, 3), 10),
            pad(&delta_str, 9),
            pad(&anchor_str, 8)
        );
        if only_free {
            out::log(&format!("{YELLOW}▸ {RESET}{line}"));
        } else {
            out::log(&format!("  {line}"));
        }
    }
    out::log(&format!(
        "\n  {YELLOW}▸{RESET} {DIM}= crosses {} only without the anchor line{RESET}",
        nodefmt::js_string(Some(&reinforce_value))
    ));
}

/// `x || {}` over a JSON field.
fn or_empty(v: Option<&Value>) -> Value {
    match v {
        Some(v) if nodefmt::js_truthy(Some(v)) => v.clone(),
        _ => Value::Object(serde_json::Map::new()),
    }
}

pub fn run(args: &[String]) {
    let (flags, positional) = parse_flags(args);

    if flags.help || positional.is_empty() {
        out::write_stdout(HELP.as_bytes());
        out::exit(if flags.help { 0 } else { 1 });
    }
    if positional.len() > 1 {
        out::elog("Error: only one characterId may be specified");
        out::exit(1);
    }
    let character_id = &positional[0];

    // v5 posts the same verb through its own dispatch endpoint (R-A).
    let url = format!("http://localhost:{}/api/dispatch", flags.port);
    let mut body = serde_json::Map::new();
    body.insert(
        "type".to_string(),
        Value::String("memoryAnchorGateProbe".to_string()),
    );
    body.insert(
        "characterId".to_string(),
        Value::String(character_id.clone()),
    );
    if let Some(limit) = flags.limit {
        body.insert("limit".to_string(), Value::from(limit));
    }
    let body_text = Value::Object(body).to_string();

    out::write_stderr(&format!(
        "{BOLD}Probing anchor gate{RESET} for character {DIM}{character_id}{RESET} via {DIM}{url}{RESET}\n"
    ));

    let (status, payload_text) = match http::post(flags.port, "/api/dispatch", Some(&body_text)) {
        Ok(r) => r,
        Err(message) => {
            out::elog(&format!(
                "{RED}Could not reach Quilltap server at http://localhost:{}: {message}{RESET}",
                flags.port
            ));
            out::elog("Start the server (npm run dev) or pass --port to match a non-default port.");
            out::exit(1);
        }
    };

    let payload: Value = match serde_json::from_str(&payload_text) {
        Ok(v) => nodefmt::js_faithful_json(&v),
        Err(_) => {
            out::elog(&format!(
                "{RED}Server returned a non-JSON response (status {status}){RESET}"
            ));
            out::exit(1);
        }
    };
    let failed = status >= 300
        || payload.get("success").and_then(Value::as_bool) == Some(false)
        || payload.get("type").and_then(Value::as_str) == Some("error");
    if failed {
        // v4 `payload?.error || payload?.message || 'unknown error'`, plus the
        // v5 dispatch envelope's `data.message` arm. No `details` lines (v4's
        // probe prints none).
        let message = payload
            .get("error")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .or_else(|| {
                payload
                    .get("message")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
            })
            .or_else(|| {
                payload
                    .get("data")
                    .and_then(|d| d.get("message"))
                    .and_then(Value::as_str)
            })
            .unwrap_or("unknown error");
        out::elog(&format!(
            "{RED}Probe failed (status {status}): {message}{RESET}"
        ));
        out::exit(1);
    }

    // v4 `payload.data ?? payload`.
    let result = match payload.get("data") {
        Some(d) if !d.is_null() => d.clone(),
        _ => payload,
    };
    if flags.json {
        out::log(&nodefmt::json_stringify_pretty(&result));
        out::exit(0);
    }

    let t = or_empty(result.get("thresholds"));
    let anchored = or_empty(result.get("anchored"));
    let free = or_empty(result.get("anchorFree"));
    let s = |v: Option<&Value>| nodefmt::js_string(v);
    out::log(&format!(
        "\n{BOLD}Character{RESET}  {}",
        s(result.get("characterId"))
    ));
    out::log(&format!(
        "{BOLD}Sampled{RESET}    {} rows, {} with an anchor line",
        s(result.get("sampled")),
        s(result.get("anchoredRows"))
    ));
    out::log(&format!(
        "{BOLD}Embeddings{RESET} {} generated",
        s(result.get("embeddingsGenerated"))
    ));
    out::log(&format!(
        "\n{DIM}{}{}anchor-free{RESET}",
        pad_end("", 22),
        pad_end("anchored", 11)
    ));
    out::log(&format!(
        "{BOLD}{}{RESET}{}{CYAN}{}{RESET}",
        pad_end(&format!("reinforce ≥ {}", fmt(t.get("reinforce"), 2)), 22),
        pad_end(&s(anchored.get("reinforce")), 11),
        s(free.get("reinforce"))
    ));
    out::log(&format!(
        "{BOLD}{}{RESET}{}{CYAN}{}{RESET}",
        pad_end(
            &format!("near-dup ≥ {}", fmt(t.get("nearDuplicate"), 2)),
            22
        ),
        pad_end(&s(anchored.get("nearDuplicate")), 11),
        s(free.get("nearDuplicate"))
    ));
    let only = result.get("crossedOnlyWithoutAnchors");
    let colour = if as_js_number(only) > 0.0 {
        YELLOW
    } else {
        GREEN
    };
    out::log(&format!(
        "{BOLD}Crossed only without anchors{RESET}  {colour}{}{RESET}",
        s(only)
    ));

    let rows = match result.get("rows") {
        Some(Value::Array(items)) => items.clone(),
        _ => Vec::new(),
    };
    print_rows(&rows, &t);
    out::log("");
    out::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_ansi_matches_the_v4_regex() {
        assert_eq!(strip_ansi("\x1b[2m—\x1b[0m"), "—");
        assert_eq!(strip_ansi("plain"), "plain");
        assert_eq!(strip_ansi("\x1b[1;33mx\x1b[0m y"), "x y");
        // `\x1b[` not closed by `m` stays (the regex needs the `m`).
        assert_eq!(strip_ansi("\x1b[12x"), "\x1b[12x");
    }

    #[test]
    fn pad_counts_the_visible_width_only() {
        assert_eq!(pad("\x1b[2mno\x1b[0m", 8), "\x1b[2mno\x1b[0m      ");
        assert_eq!(pad("0.912", 10), "0.912     ");
        assert_eq!(pad("too-long-for-it", 4), "too-long-for-it");
    }

    #[test]
    fn whitespace_runs_collapse_the_js_way() {
        assert_eq!(collapse_ws("a\t\n b\u{a0}\u{2028}c"), "a b c");
        assert_eq!(collapse_ws("  lead"), " lead");
        // U+0085 is not JS whitespace.
        assert_eq!(collapse_ws("a\u{85}b"), "a\u{85}b");
    }
}
