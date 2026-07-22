//! `quilltap recall-replay <chatId>` — the §3 replay CLI (P4.d13; v4
//! `packages/quilltap/lib/recall-replay-command.js`, ported line-for-line:
//! flag parsing, the help text, the stderr progress line, the ANSI old/new
//! candidate tables, `--json`).
//!
//! Like v4's, this is a thin HTTP wrapper over the running server — v4 POSTs
//! `/api/v1/chats/<id>?action=recall-replay`; v5 POSTs the same verb through
//! its own dispatch endpoint (`/api/dispatch`, `chatRecallReplay`). The
//! `payload.data ?? payload` unwrap is v4's own line and handles both
//! envelopes; the error-message extraction adds the dispatch envelope's
//! `data.message` arm (v5's error shape — a documented adaptation, the
//! rendering around it is byte-identical). No direct-core mode: v4's command
//! has none (the replay needs the live host's cheap-LLM providers).

use crate::nodefmt;
use crate::out;

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";
const CYAN: &str = "\x1b[36m";

/// v4 `printRecallReplayHelp` — the template literal starts and ends with a
/// newline and `console.log` appends one more (hence the double trailing).
const HELP: &str = "
Quilltap recall-replay Tool

Usage: quilltap recall-replay <chatId> [options]

Replays the per-turn memory recall for a chat turn against the running
Quilltap server and prints the full candidate table twice — the pre-overhaul
ranking and the episodic (retrospective/time-window/entity) ranking — so the
recall constants can be tuned against real \"the character forgot\" turns.

Options:
      --turn <number>        1-based interchange to replay at (default: last)
      --char <characterId>   Character whose memories are searched
                             (default: first LLM-controlled participant)
      --limit <number>       Candidate rows per path (default: 25, max: 100)
      --port <number>        Server port for API calls (default: 3000)
      --json                 Print the raw JSON result instead of tables
  -h, --help                 Show this help

Examples:
  quilltap recall-replay <chatId>
  quilltap recall-replay <chatId> --turn 42
  quilltap recall-replay <chatId> --turn 42 --json > replay.json

";

struct Flags {
    turn: Option<i64>,
    char_id: Option<String>,
    limit: Option<i64>,
    port: i64,
    json: bool,
    help: bool,
}

/// v4 `parseFlags` — position-independent; value flags consume the next token
/// (`args[++i]`, possibly absent → NaN/undefined semantics).
fn parse_flags(args: &[String]) -> (Flags, Vec<String>) {
    let mut flags = Flags {
        turn: None,
        char_id: None,
        limit: None,
        port: 3000,
        json: false,
        help: false,
    };
    let mut positional: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--turn" => {
                i += 1;
                let n = nodefmt::js_parse_int(args.get(i).map(String::as_str));
                match n {
                    Some(v) if v >= 1 => flags.turn = Some(v),
                    _ => {
                        out::elog("Error: --turn must be a positive integer");
                        out::exit(1);
                    }
                }
            }
            "--char" => {
                i += 1;
                flags.char_id = args.get(i).cloned();
            }
            "--limit" => {
                i += 1;
                let n = nodefmt::js_parse_int(args.get(i).map(String::as_str));
                match n {
                    Some(v) if (1..=100).contains(&v) => flags.limit = Some(v),
                    _ => {
                        out::elog("Error: --limit must be between 1 and 100");
                        out::exit(1);
                    }
                }
            }
            "--port" => {
                i += 1;
                let n = nodefmt::js_parse_int(args.get(i).map(String::as_str));
                match n {
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

/// JS `String.prototype.padEnd` over UTF-16 units (ANSI escapes count — v4's
/// dash cell is `DIM + '—' + RESET`, whose escape bytes eat the padding).
fn pad_end(s: &str, width: usize) -> String {
    let len = s.encode_utf16().count();
    if len >= width {
        s.to_string()
    } else {
        let mut out = s.to_string();
        out.extend(std::iter::repeat_n(' ', width - len));
        out
    }
}

/// JS `.slice(0, n)` over UTF-16 units.
fn slice_utf16(s: &str, n: usize) -> String {
    let units: Vec<u16> = s.encode_utf16().take(n).collect();
    String::from_utf16_lossy(&units)
}

/// v4 `fmt(n, digits = 3)` — `—` (dimmed) for null/absent, `toFixed` otherwise.
fn fmt(v: Option<f64>, digits: u32) -> String {
    match v {
        None => format!("{DIM}—{RESET}"),
        Some(n) => quilltap_core::jsnum::to_fixed(n, digits),
    }
}

fn get_f64(row: &serde_json::Value, key: &str) -> Option<f64> {
    row.get(key).and_then(serde_json::Value::as_f64)
}

fn get_str<'a>(row: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    row.get(key).and_then(serde_json::Value::as_str)
}

/// v4 `printPath`.
fn print_path(label: &str, rows: &[serde_json::Value]) {
    out::log(&format!(
        "\n{BOLD}{label}{RESET} ({} candidates)",
        rows.len()
    ));
    if rows.is_empty() {
        out::log(&format!("  {DIM}(none){RESET}"));
        return;
    }
    out::log(&format!(
        "  {DIM}{}{}{}{}{}{}{}{}{}{RESET}",
        pad_end("sel", 4),
        pad_end("cosine", 8),
        pad_end("blend", 8),
        pad_end("×mult", 7),
        pad_end("after", 8),
        pad_end("kind", 9),
        pad_end("occurredAt", 12),
        pad_end("fired", 24),
        "summary"
    ));
    for row in rows {
        let selected = row
            .get("selected")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let sel = if selected {
            format!("{GREEN}✓{RESET}  ")
        } else {
            "   ".to_string()
        };
        let occurred = match get_str(row, "occurredAt").filter(|s| !s.is_empty()) {
            Some(s) => slice_utf16(s, 10),
            None => "—".to_string(),
        };
        let fired = row
            .get("fired")
            .and_then(serde_json::Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(serde_json::Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
        let fired = if fired.is_empty() {
            "—".to_string()
        } else {
            fired
        };
        let summary = slice_utf16(get_str(row, "summary").unwrap_or(""), 60);
        let kind = match get_str(row, "kind").filter(|s| !s.is_empty()) {
            Some(k) => k.to_string(),
            None => "semantic".to_string(),
        };
        out::log(&format!(
            "  {sel} {}{}{}{}{}{}{}{}",
            pad_end(&fmt(get_f64(row, "cosine"), 3), 8),
            pad_end(&fmt(get_f64(row, "blendedBefore"), 3), 8),
            pad_end(&fmt(get_f64(row, "multiplier"), 2), 7),
            pad_end(&fmt(get_f64(row, "blendedAfter"), 3), 8),
            pad_end(&kind, 9),
            pad_end(&occurred, 12),
            slice_utf16(&pad_end(&fired, 24), 24),
            summary
        ));
    }
}

/// Minimal HTTP/1.1 POST to localhost (JSON in/out, Connection: close). Kept
/// dependency-free — the CLI links no async runtime or HTTP client.
fn http_post_json(port: i64, path: &str, body: &str) -> Result<(u16, String), String> {
    use std::io::{Read, Write};
    let addr = format!("localhost:{port}");
    let mut stream = std::net::TcpStream::connect(&addr).map_err(|e| e.to_string())?;
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(req.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&raw);
    let mut parts = text.splitn(2, "\r\n\r\n");
    let head = parts.next().unwrap_or("");
    let mut payload = parts.next().unwrap_or("").to_string();
    let status: u16 = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| "malformed HTTP response".to_string())?;
    // Minimal chunked-transfer decode (axum may chunk when no length is set).
    if head
        .lines()
        .any(|l| l.to_ascii_lowercase().starts_with("transfer-encoding:") && l.contains("chunked"))
    {
        let mut decoded = String::new();
        let mut rest = payload.as_str();
        while let Some(nl) = rest.find("\r\n") {
            let size = usize::from_str_radix(rest[..nl].trim(), 16).unwrap_or(0);
            if size == 0 {
                break;
            }
            let start = nl + 2;
            decoded.push_str(rest.get(start..start + size).unwrap_or(""));
            rest = rest.get(start + size + 2..).unwrap_or("");
        }
        payload = decoded;
    }
    Ok((status, payload))
}

pub fn run(args: &[String]) {
    let (flags, positional) = parse_flags(args);

    if flags.help || positional.is_empty() {
        out::write_stdout(HELP.as_bytes());
        out::exit(if flags.help { 0 } else { 1 });
    }
    if positional.len() > 1 {
        out::elog("Error: only one chatId may be specified");
        out::exit(1);
    }
    let chat_id = &positional[0];

    // v5 posts the same verb through its own dispatch endpoint.
    let url = format!("http://localhost:{}/api/dispatch", flags.port);
    let mut body = serde_json::Map::new();
    body.insert(
        "type".to_string(),
        serde_json::Value::String("chatRecallReplay".to_string()),
    );
    body.insert(
        "chatId".to_string(),
        serde_json::Value::String(chat_id.clone()),
    );
    if let Some(turn) = flags.turn {
        body.insert("turnIndex".to_string(), serde_json::Value::from(turn));
    }
    if let Some(c) = flags.char_id.as_ref().filter(|c| !c.is_empty()) {
        body.insert(
            "characterId".to_string(),
            serde_json::Value::String(c.clone()),
        );
    }
    if let Some(limit) = flags.limit {
        body.insert("limit".to_string(), serde_json::Value::from(limit));
    }
    let body_text = serde_json::Value::Object(body).to_string();

    out::write_stderr(&format!(
        "{BOLD}Replaying recall{RESET} for chat {DIM}{chat_id}{RESET} via {DIM}{url}{RESET}\n"
    ));

    let (status, payload_text) = match http_post_json(flags.port, "/api/dispatch", &body_text) {
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

    let payload: serde_json::Value = match serde_json::from_str(&payload_text) {
        Ok(v) => v,
        Err(_) => {
            out::elog(&format!(
                "{RED}Server returned a non-JSON response (status {status}){RESET}"
            ));
            out::exit(1);
        }
    };
    let failed = status >= 300
        || payload.get("success").and_then(serde_json::Value::as_bool) == Some(false)
        || payload.get("type").and_then(serde_json::Value::as_str) == Some("error");
    if failed {
        // v4 `payload?.error || payload?.message || 'unknown error'`, plus the
        // v5 dispatch envelope's `data.message` arm.
        let message = payload
            .get("error")
            .and_then(serde_json::Value::as_str)
            .or_else(|| payload.get("message").and_then(serde_json::Value::as_str))
            .or_else(|| {
                payload
                    .get("data")
                    .and_then(|d| d.get("message"))
                    .and_then(serde_json::Value::as_str)
            })
            .unwrap_or("unknown error");
        out::elog(&format!(
            "{RED}Replay failed (status {status}): {message}{RESET}"
        ));
        out::exit(1);
    }

    // v4 `payload.data ?? payload` — handles both the raw v4 body and the v5
    // dispatch envelope.
    let result = payload.get("data").cloned().unwrap_or(payload);

    if flags.json {
        // v4 `JSON.stringify(result, null, 2)` — the CLI's node-faithful
        // pretty printer (JS number rendering + 2-space indent).
        out::log(&nodefmt::json_stringify_pretty(&result));
        out::exit(0);
    }

    let s = |k: &str| get_str(&result, k).unwrap_or("").to_string();
    out::log(&format!("\n{BOLD}Chat{RESET}      {}", s("chatId")));
    out::log(&format!(
        "{BOLD}Character{RESET} {} {DIM}({}){RESET}",
        s("characterName"),
        s("characterId")
    ));
    out::log(&format!(
        "{BOLD}Turn{RESET}      {} of {}  {DIM}(clock {}){RESET}",
        result
            .get("turnIndex")
            .and_then(serde_json::Value::as_f64)
            .map(nodefmt::js_num_string)
            .unwrap_or_default(),
        result
            .get("totalTurns")
            .and_then(serde_json::Value::as_f64)
            .map(nodefmt::js_num_string)
            .unwrap_or_default(),
        s("clockIso")
    ));
    out::log(&format!("{BOLD}Query{RESET}     {}", s("query")));
    match result.get("signals").filter(|v| !v.is_null()) {
        Some(sig) => {
            let retro = if sig
                .get("retrospective")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
            {
                format!("{GREEN}retrospective{RESET}")
            } else {
                format!("{DIM}not retrospective{RESET}")
            };
            let range = match sig.get("timeRange").filter(|v| !v.is_null()) {
                Some(tr) => format!(
                    "{} → {}",
                    slice_utf16(get_str(tr, "from").unwrap_or(""), 10),
                    slice_utf16(get_str(tr, "to").unwrap_or(""), 10)
                ),
                None => "—".to_string(),
            };
            let entities = sig
                .get("entities")
                .and_then(serde_json::Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(serde_json::Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .filter(|e| !e.is_empty())
                .unwrap_or_else(|| "—".to_string());
            out::log(&format!(
                "{BOLD}Signals{RESET}   {retro} · timeRange {CYAN}{range}{RESET} · entities {CYAN}{entities}{RESET}"
            ));
        }
        None => {
            out::log(&format!(
                "{BOLD}Signals{RESET}   {YELLOW}distillation failed — new path ran inert{RESET}"
            ));
        }
    }

    let rows = |k: &str| {
        result
            .get(k)
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    print_path("OLD PATH (episodic signals inert)", &rows("oldPath"));
    print_path(
        "NEW PATH (retrospective/window/entities live)",
        &rows("newPath"),
    );
    out::log("");
    out::exit(0);
}
