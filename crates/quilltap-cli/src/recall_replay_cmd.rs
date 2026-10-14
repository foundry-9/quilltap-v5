//! `quilltap recall-replay <chatId>` — the §3 replay CLI (P4.d13; v4
//! `packages/quilltap/lib/recall-replay-command.js`, ported line-for-line:
//! flag parsing, the help text, the stderr progress line, the ANSI old/new
//! candidate tables, `--json`). P4.D269 grew it to v4 `01a83539d`'s shape
//! (`3f7320138` + `f7d8064be`): `--memory-budget`, `--tuning` /
//! `--tuning-file`, `--signals-from`, `--as-of`, the `Harness` and `Tuning`
//! header lines, `, head N` in each path title, and the 400 `details` lines.
//!
// === P4.D291: recall-replay rendering ===
//! P4.D291 grew it to v4 `83334b9a5`'s Phase E shape: the `Ring` header line
//! (the anti-repetition ring in force at the turn, and where it was read
//! from), the `TEXT FALLBACK` table (what the new path returns with no usable
//! vector) and the `STAGE TRACE` (query → gather → hydrate → score → select →
//! assemble: what the archive removed, what the budget cut, what the
//! inter-character section took).
// === end P4.D291 ===
//!
//! Like v4's, this is a thin HTTP wrapper over the running server — v4 POSTs
//! `/api/v1/chats/<id>?action=recall-replay`; v5 POSTs the same verb through
//! its own dispatch endpoint (`/api/dispatch`, `chatRecallReplay`). The
//! `payload.data ?? payload` unwrap is v4's own line and handles both
//! envelopes; the error-message extraction adds the dispatch envelope's
//! `data.message` arm (v5's error shape — a documented adaptation, the
//! rendering around it is byte-identical). The dispatch transport merges a
//! validation refusal's `details` to the envelope's top level, so the
//! per-issue lines read `payload.details` exactly as v4's do. No direct-core
//! mode: v4's command has none (the replay needs the live host's cheap-LLM
//! providers).
//!
//! The forwarded `tuning` / `signals` values are re-serialized JS-faithfully
//! ([`nodefmt::js_faithful_json`]), so the posted body is v4's
//! `JSON.stringify(body)` with v5's `type` + `chatId` in front (P4.D269 R-I —
//! Tier R byte-checks it).

use crate::http;
use crate::nodefmt;
use crate::nodefmt::slice_utf16;
use crate::out;
use crate::resolve;
use quilltap_host::instances::InstanceRegistry;
use serde_json::Value;

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";
const CYAN: &str = "\x1b[36m";

/// v4 `printRecallReplayHelp` — a BYTE CAPTURE of `node
/// packages/quilltap/bin/quilltap.js recall-replay --help` stdout at v4
/// `83334b9a5` (P4.D299 — bug 239's three flag lines and two-line `--port`,
/// `584d28164`'s "After them: what the text fallback…" paragraph; first
/// captured at `01a83539d`, P4.D269 R-B). The template literal starts and ends with a
/// newline and `console.log` appends one more; its `\`-continued example line
/// is a JS line continuation, which the capture keeps exactly as Node prints it
/// (an inline Rust literal would have to re-derive it).
const HELP: &str = include_str!("help/recall_replay_help.txt");

struct Flags {
    turn: Option<i64>,
    char_id: Option<String>,
    limit: Option<i64>,
    memory_budget: Option<f64>,
    tuning: Option<String>,
    tuning_file: Option<String>,
    signals_from: Option<String>,
    as_of: bool,
    /// `None` unless `--port` is given — the lock then supplies it
    /// ([`resolve::resolve_server_port`]).
    port: Option<i64>,
    /// `-d` / `--data-dir`, `-i` / `--instance` (v4 bug 239): which
    /// instance's server lock names the port. Empty when absent.
    data_dir: String,
    instance: String,
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
        memory_budget: None,
        tuning: None,
        tuning_file: None,
        signals_from: None,
        as_of: false,
        port: None,
        data_dir: String::new(),
        instance: String::new(),
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
            "--memory-budget" => {
                // v4 `Number(args[++i])` + `Number.isInteger(n) && n >= 1`
                // (`Number(undefined)` is NaN; `' 12'`, `1e3`, `0x10`, `5.`
                // are integers; `''` is 0).
                i += 1;
                let n = args
                    .get(i)
                    .map(|a| quilltap_core::jsnum::number_from_str(a))
                    .unwrap_or(f64::NAN);
                if !(n.is_finite() && n.trunc() == n && n >= 1.0) {
                    out::elog("Error: --memory-budget must be a positive integer");
                    out::exit(1);
                }
                flags.memory_budget = Some(n);
            }
            // The three value flags take `args[++i]` VERBATIM — a missing value
            // leaves the flag undefined, i.e. not passed (no error).
            "--tuning" => {
                i += 1;
                flags.tuning = args.get(i).cloned();
            }
            "--tuning-file" => {
                i += 1;
                flags.tuning_file = args.get(i).cloned();
            }
            "--signals-from" => {
                i += 1;
                flags.signals_from = args.get(i).cloned();
            }
            "--as-of" => flags.as_of = true,
            // `args[++i]` — a missing value reads as empty (v4's `undefined`).
            "-d" | "--data-dir" => {
                i += 1;
                flags.data_dir = args.get(i).cloned().unwrap_or_default();
            }
            "-i" | "--instance" => {
                i += 1;
                flags.instance = args.get(i).cloned().unwrap_or_default();
            }
            // Accepted so universal flags never error; this tool talks to the
            // running server and never opens the database itself.
            "--passphrase" => i += 1,
            "--port" => {
                i += 1;
                let n = nodefmt::js_parse_int(args.get(i).map(String::as_str));
                match n {
                    Some(v) if (1..=65535).contains(&v) => flags.port = Some(v),
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

/// v4 `printPath(label, rows, headSize)` — `, head N` only when the server
/// sent a number (`typeof headSize === 'number'`; an old server sends none).
fn print_path(label: &str, rows: &[serde_json::Value], head_size: Option<&Value>) {
    let head = match head_size {
        Some(Value::Number(_)) => format!(", head {}", nodefmt::js_string(head_size)),
        _ => String::new(),
    };
    out::log(&format!(
        "\n{BOLD}{label}{RESET} ({} candidates{head})",
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

// === P4.D291: recall-replay rendering ===
/// One `detail` value as v4's template literal renders it: an object or array
/// through `JSON.stringify`, anything else through `${v}` (`String(v)`).
fn trace_value(v: &Value) -> String {
    match v {
        Value::Object(_) | Value::Array(_) => {
            serde_json::to_string(&nodefmt::js_faithful_json(v)).unwrap_or_default()
        }
        other => nodefmt::js_string(Some(other)),
    }
}

/// v4 `printTrace(trace)` — one line per stage entry: the stage padded to 9,
/// the dimmed `(label) ` when the entry carries one, then `k=v` pairs.
fn print_trace(trace: Option<&Value>) {
    let Some(entries) = trace.and_then(Value::as_array).filter(|a| !a.is_empty()) else {
        return;
    };
    out::log(&format!("\n{BOLD}STAGE TRACE{RESET}"));
    for entry in entries {
        let label = match entry.get("label") {
            Some(l) if nodefmt::js_truthy(Some(l)) => {
                format!("{DIM}({}){RESET} ", nodefmt::js_string(Some(l)))
            }
            _ => String::new(),
        };
        let detail = entry
            .get("detail")
            .and_then(Value::as_object)
            .map(|d| {
                d.iter()
                    .map(|(k, v)| format!("{k}={}", trace_value(v)))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
        out::log(&format!(
            "  {CYAN}{}{RESET}{label}{detail}",
            pad_end(&nodefmt::js_string(entry.get("stage")), 9)
        ));
    }
}
// === end P4.D291 ===

/// v4 `readJson(text, what)`: `JSON.parse` or the RED refusal carrying V8's
/// message (`jsstr::v8_json_parse_message`; where V8 would ACCEPT what serde
/// refuses — `1e400`, a lone surrogate — serde's message stands in, the
/// recorded class).
fn read_json(text: &str, what: &str) -> Value {
    match serde_json::from_str::<Value>(text) {
        Ok(v) => nodefmt::js_faithful_json(&v),
        Err(e) => {
            let message =
                quilltap_core::jsstr::v8_json_parse_message(text).unwrap_or_else(|| e.to_string());
            out::elog(&format!(
                "{RED}Could not parse {what} as JSON: {message}{RESET}"
            ));
            out::exit(1);
        }
    }
}

/// v4 `readJsonFile(path, what)` — Node's `readFileSync(path, 'utf8')`, then
/// [`read_json`] naming `<what> <path>`.
fn read_json_file(path: &str, what: &str) -> Value {
    match nodefmt::node_read_file_utf8(path) {
        Ok(text) => read_json(&text, &format!("{what} {path}")),
        Err(message) => {
            out::elog(&format!(
                "{RED}Could not read {what} {path}: {message}{RESET}"
            ));
            out::exit(1);
        }
    }
}

/// v4 `loadTuning(flags)` — the tuning object from `--tuning` or
/// `--tuning-file` (never both; that refusal is PLAIN, no colour).
fn load_tuning(flags: &Flags) -> Option<Value> {
    if flags.tuning.is_some() && flags.tuning_file.is_some() {
        out::elog("Error: pass --tuning or --tuning-file, not both");
        out::exit(1);
    }
    if let Some(text) = &flags.tuning {
        return Some(read_json(text, "--tuning"));
    }
    flags
        .tuning_file
        .as_deref()
        .map(|path| read_json_file(path, "--tuning-file"))
}

/// v4 `loadSignals(path, chatId, turn)` — the signals saved in a previous
/// `--json` replay. Refuses a file from another chat or turn (reusing its
/// signals would replay a different question).
fn load_signals(path: &str, chat_id: &str, turn: Option<i64>) -> Value {
    let saved = read_json_file(path, "--signals-from");
    // `saved.data ?? saved` — a file holding `null` throws a TypeError that
    // the launcher's generic catch prints as `Error: <message>`.
    if saved.is_null() {
        out::elog("Error: Cannot read properties of null (reading 'data')");
        out::exit(1);
    }
    let result = match saved.get("data") {
        Some(d) if !d.is_null() => d.clone(),
        _ => saved,
    };
    if !nodefmt::js_truthy(result.get("signals")) {
        out::elog(&format!(
            "{RED}{path} has no saved signals (its distillation failed); replay without --signals-from{RESET}"
        ));
        out::exit(1);
    }
    let saved_chat = result.get("chatId");
    if nodefmt::js_truthy(saved_chat) && saved_chat != Some(&Value::String(chat_id.to_string())) {
        out::elog(&format!(
            "{RED}{path} is a replay of chat {}, not {chat_id}{RESET}",
            nodefmt::js_string(saved_chat)
        ));
        out::exit(1);
    }
    if let Some(turn) = turn {
        // `result.turnIndex !== undefined && result.turnIndex !== turn` — a
        // strict compare: `null` and `"5"` both mismatch.
        let saved_turn = result.get("turnIndex");
        let same = saved_turn
            .and_then(Value::as_f64)
            .is_some_and(|t| t == turn as f64);
        if saved_turn.is_some() && !same {
            out::elog(&format!(
                "{RED}{path} is a replay of turn {}, not {turn}{RESET}",
                nodefmt::js_string(saved_turn)
            ));
            out::exit(1);
        }
    }
    result.get("signals").cloned().unwrap_or(Value::Null)
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

    // === P4.D299: server port resolution ===
    // v4 resolves here, BEFORE the body and its `--tuning-file` /
    // `--signals-from` refusals, so a both-flags error beats a bad tuning file.
    let target = match resolve::resolve_server_port(
        flags.port,
        &flags.instance,
        &flags.data_dir,
        &InstanceRegistry::at_default_location(),
    ) {
        Ok(target) => target,
        Err(message) => {
            out::elog(&format!("Error: {message}"));
            out::exit(1);
        }
    };
    let port = target.port;
    if let Some(note) = &target.note {
        out::write_stderr(&format!("{DIM}{note}{RESET}\n"));
    }
    // v5 posts the same verb through its own dispatch endpoint.
    let url = format!("http://localhost:{port}/api/dispatch");
    // === end P4.D299 ===
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
    if let Some(budget) = flags.memory_budget {
        body.insert(
            "memoryBudget".to_string(),
            nodefmt::js_number_to_json(budget),
        );
    }
    // v4's order: the tuning's refusals precede the signals'; both precede the
    // stderr line.
    if let Some(tuning) = load_tuning(&flags) {
        body.insert("tuning".to_string(), tuning);
    }
    if let Some(path) = &flags.signals_from {
        body.insert(
            "signals".to_string(),
            load_signals(path, chat_id, flags.turn),
        );
    }
    if flags.as_of {
        body.insert("asOf".to_string(), serde_json::Value::Bool(true));
    }
    let body_text = serde_json::Value::Object(body).to_string();

    out::write_stderr(&format!(
        "{BOLD}Replaying recall{RESET} for chat {DIM}{chat_id}{RESET} via {DIM}{url}{RESET}\n"
    ));

    // === P4.D299: server port resolution ===
    let (status, payload_text) = match http::post(port, "/api/dispatch", Some(&body_text)) {
        Ok(r) => r,
        Err(message) => {
            out::elog(&format!(
                "{RED}Could not reach Quilltap server at http://localhost:{port}: {message}{RESET}"
            ));
            out::elog(&target.hint);
            out::exit(1);
        }
    };
    // === end P4.D299 ===

    let payload: serde_json::Value = match serde_json::from_str(&payload_text) {
        // `res.json()` is `JSON.parse`: re-shape the numbers the JS way.
        Ok(v) => nodefmt::js_faithful_json(&v),
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
        // v4: `Array.isArray(payload?.details)` → one uncoloured line per
        // issue, `  ${(issue.path || []).join('.') || '(body)'}: ${issue.message}`.
        if let Some(details) = payload.get("details").and_then(Value::as_array) {
            for issue in details {
                if issue.is_null() {
                    // `null.path` throws into the launcher's generic catch.
                    out::elog("Error: Cannot read properties of null (reading 'path')");
                    out::exit(1);
                }
                // `Array#join('.')`: each element through `String`, a `null`
                // element empty. (A truthy NON-array `path` would throw in v4;
                // v5's server never sends one — out of corpus.)
                let path = match issue.get("path") {
                    Some(Value::Array(items)) => items
                        .iter()
                        .map(|p| match p {
                            Value::Null => String::new(),
                            other => nodefmt::js_string(Some(other)),
                        })
                        .collect::<Vec<_>>()
                        .join("."),
                    _ => String::new(),
                };
                let path = if path.is_empty() {
                    "(body)".to_string()
                } else {
                    path
                };
                out::elog(&format!(
                    "  {path}: {}",
                    nodefmt::js_string(issue.get("message"))
                ));
            }
        }
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

    // v4 `f7d8064be`: the harness + tuning lines.
    let pinned = if nodefmt::js_truthy(result.get("signalsPinned")) {
        format!("{CYAN}pinned{RESET}")
    } else {
        format!("{DIM}fresh distillation{RESET}")
    };
    let as_of = if nodefmt::js_truthy(result.get("asOf")) {
        format!(
            "{CYAN}as of {}{RESET} {DIM}({} later memories left out){RESET}",
            nodefmt::js_string(result.get("asOf")),
            nodefmt::js_string(result.get("excludedAfterAsOf"))
        )
    } else {
        format!("{DIM}whole corpus{RESET}")
    };
    out::log(&format!(
        "{BOLD}Harness{RESET}   signals {pinned} · {as_of}"
    ));
    let tuning = result.get("tuning");
    let tuning =
        if tuning == Some(&Value::String("defaults".to_string())) || !nodefmt::js_truthy(tuning) {
            format!("{DIM}defaults{RESET}")
        } else {
            format!("{YELLOW}{}{RESET}", nodefmt::js_string(tuning))
        };
    out::log(&format!(
        "{BOLD}Tuning{RESET}    {tuning} {DIM}(new path only){RESET}"
    ));
    // === P4.D291: recall-replay rendering ===
    if nodefmt::js_truthy(result.get("ringSource")) {
        let source = if result.get("ringSource") == Some(&Value::String("debug-log".to_string())) {
            format!("{DIM}as of the turn, from its recall records{RESET}")
        } else {
            format!("{YELLOW}the chat's current ring (no recall records that early){RESET}")
        };
        // `(result.ring || []).length` — a non-array ring has no `length` in
        // corpus; the server always sends an array.
        let ring_len = result
            .get("ring")
            .and_then(serde_json::Value::as_array)
            .map_or(0, Vec::len);
        out::log(&format!(
            "{BOLD}Ring{RESET}      {ring_len} recently whispered · {source}"
        ));
    }
    // === end P4.D291 ===

    let rows = |k: &str| {
        result
            .get(k)
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    print_path(
        "OLD PATH (episodic signals inert)",
        &rows("oldPath"),
        result.get("oldHeadSize"),
    );
    print_path(
        "NEW PATH (retrospective/window/entities live)",
        &rows("newPath"),
        result.get("newHeadSize"),
    );
    // === P4.D291: recall-replay rendering ===
    if let Some(fallback) = result
        .get("fallbackPath")
        .and_then(serde_json::Value::as_array)
    {
        print_path(
            "TEXT FALLBACK (what the new path returns with no usable vector)",
            fallback,
            result.get("newHeadSize"),
        );
    }
    print_trace(result.get("trace"));
    // === end P4.D291 ===
    out::log("");
    out::exit(0);
}

#[cfg(test)]
mod tests {
    // === P4.D299: the universal flags (v4 `server-port-resolution.test.js`) ===
    use super::*;

    fn parse(args: &[&str]) -> (Flags, Vec<String>) {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        parse_flags(&args)
    }

    #[test]
    fn accepts_the_instance_flag_around_the_positional_id() {
        for args in [
            &["--instance", "V4test", "ID"][..],
            &["ID", "--instance", "V4test"],
            &["-i", "V4test", "ID"],
            &["ID", "-i", "V4test"],
        ] {
            let (flags, positional) = parse(args);
            assert_eq!(flags.instance, "V4test", "{args:?}");
            assert_eq!(positional, ["ID"], "{args:?}");
        }
    }

    #[test]
    fn accepts_the_data_dir_flag() {
        for args in [&["--data-dir", "/x/y", "ID"][..], &["ID", "-d", "/x/y"]] {
            let (flags, positional) = parse(args);
            assert_eq!(flags.data_dir, "/x/y", "{args:?}");
            assert_eq!(positional, ["ID"], "{args:?}");
        }
    }

    #[test]
    fn accepts_passphrase_without_treating_its_value_as_the_id() {
        assert_eq!(parse(&["--passphrase", "hunter2", "ID"]).1, ["ID"]);
        assert_eq!(parse(&["ID", "--passphrase", "hunter2"]).1, ["ID"]);
        assert_eq!(parse(&["ID", "--passphrase"]).1, ["ID"]);
    }

    #[test]
    fn leaves_the_port_unset_unless_given() {
        assert_eq!(parse(&["ID"]).0.port, None);
        assert_eq!(parse(&["ID", "--port", "3005"]).0.port, Some(3005));
    }

    #[test]
    fn a_trailing_value_flag_reads_as_empty() {
        assert_eq!(parse(&["ID", "-i"]).0.instance, "");
        assert_eq!(parse(&["ID", "--data-dir"]).0.data_dir, "");
    }
    // === end P4.D299 ===
}
