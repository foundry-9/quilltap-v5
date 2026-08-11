//! The CLI differential (tier-4 #4): every shipped verb's stdout + stderr +
//! exit code, byte-diffed against v4's launcher on the same fixture.
//!
//! Env-gated: set `QT_V4_CHECKOUT=/path/to/quilltap-server` (and have Node 24
//! on PATH, or set `QT_NODE`) to run; otherwise the test skips with a message.
//!
//! Mechanics: a master fixture (two instances — an internal-sentinel `.dbkey`
//! and a passphrase-protected one — plus a controlled HOME with the instance
//! registry) is built once; before EACH side of EACH case it is copied to one
//! shared `live/` path (so path-bearing output matches), an optional pre-hook
//! runs (lock files, schema surgery), and the binary runs non-TTY with
//! `TZ=UTC`. Documented normalizations only: the elapsed-seconds heartbeat
//! display (`heartbeat <N>s ago`) — environment truth that legitimately
//! differs between the two runs.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use quilltap_core::db::Writer;
use quilltap_core::dbkey;

/// Any valid base64 keys a fresh encrypted DB; the same value opens it back.
const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const PASSPHRASE_B: &str = "swordfish";
const TS: &str = "2026-01-02T03:04:05.000Z";
const TS2: &str = "2026-02-03T04:05:06.000Z";

const N1: &str = "11111111-1111-4111-8111-111111111111"; // notes
const N2: &str = "22222222-2222-4222-8222-222222222222"; // archive
const N3: &str = "33333333-3333-4333-8333-333333333333"; // attic (filesystem)
const N4: &str = "44444444-4444-4444-8444-444444444444"; // twin (a)
const N5: &str = "55555555-5555-4555-8555-555555555555"; // twin (b)

struct Ctx {
    /// Keep the tempdir alive for the whole run.
    _root: tempfile::TempDir,
    master: PathBuf,
    live: PathBuf,
    node: String,
    v4_bin: PathBuf,
    failures: Vec<String>,
    cases_run: usize,
}

type PreHook<'a> = Box<dyn Fn(&Path) + 'a>;

#[derive(Default)]
struct CaseOpts<'a> {
    envs: Vec<(&'a str, String)>,
    stdin: Option<Vec<u8>>,
    pre: Option<PreHook<'a>>,
    normalize_heartbeat: bool,
    /// P4.d13 recall-replay: normalize the transport-truth spans — the target
    /// URL in the stderr progress line (v4 posts `/api/v1/chats/...`, v5 posts
    /// `/api/dispatch`) and the connect-error reason (Node's `fetch failed` vs
    /// the Rust io error text).
    normalize_recall: bool,
    /// P4.D66 `db characters`: the ONE transport truth in the
    /// could-not-reach-the-server sentence — Node's `err.message` is
    /// `fetch failed`, Rust's is the OS connect error. Same URL, same
    /// sentence, same following line; only the reason tail differs.
    normalize_reach: bool,
    /// Working directory for the child. `characters export` without `--out`
    /// resolves its output path against the cwd, so those cases run in a
    /// scratch dir instead of the crate root.
    cwd: Option<PathBuf>,
}

struct RunOut {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    code: i32,
}

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let ty = entry.file_type().unwrap();
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), &to).unwrap();
        }
    }
}

/// Replace every digit-run immediately before `s ago` with `N` (the one
/// documented normalization — elapsed seconds between the two runs).
fn normalize_heartbeat(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if text[i..].starts_with("s ago") {
                out.push('N');
            } else {
                out.push_str(&text[start..i]);
            }
        } else {
            out.push(text[i..].chars().next().unwrap());
            i += text[i..].chars().next().unwrap().len_utf8();
        }
    }
    out
}

/// Replace the reason tail of the `Could not reach the Quilltap server at
/// http://localhost:<port>: <reason>` line. Everything else on the line — and
/// the whole sentence that follows it — is compared byte-for-byte.
fn normalize_reach(text: &str) -> String {
    const HEAD: &str = "Could not reach the Quilltap server at http://localhost:";
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        match line.find(HEAD) {
            Some(pos) => {
                let after = &line[pos + HEAD.len()..];
                // The port runs to the `: ` that introduces the reason.
                match after.find(": ") {
                    Some(colon) => {
                        out.push_str(&line[..pos + HEAD.len() + colon]);
                        out.push_str(": <REASON>");
                        if line.ends_with('\n') {
                            out.push('\n');
                        }
                    }
                    None => out.push_str(line),
                }
            }
            None => out.push_str(line),
        }
    }
    out
}

const RECALL_CANNED_RESULT: &str = r#"{"chatId":"cd000000-0000-4000-8000-000000000001","characterId":"c1000000-0000-4000-8000-0000000000e1","characterName":"Elowen","turnIndex":5,"totalTurns":5,"signals":{"keywords":["Gullwing Quay","dawn","visit"],"temporal":"past","context":"history","paraphrase":"They remember visiting Gullwing Quay at dawn last week.","retrospective":true,"timeRange":{"from":"2026-06-01T00:00:00.000Z","to":"2026-06-07T23:59:59.999Z"},"entities":["Gullwing Quay"]},"query":"They remember visiting Gullwing Quay at dawn last week.","clockIso":"2026-06-11T09:00:00.000Z","oldPath":[{"memoryId":"e2000000-0000-4000-8000-0000000000a1","summary":"Visited Gullwing Quay at dawn.","kind":"episodic","occurredAt":"2026-06-03T06:00:00.000Z","narrativeTime":"the second dawn","createdAt":"2026-05-01T00:00:00.000Z","keywords":["quay","past","scope: wide","history"],"cosine":0.8999999761581421,"rawWeight":0.2795939868358039,"blendedBefore":0.679268857920964,"multiplier":0.935,"fired":["past↓","ctx✓"],"blendedAfter":0.6351163821560014,"selected":true},{"memoryId":"e2000000-0000-4000-8000-0000000000d1","summary":"The tide ledger and mooring fees.","kind":"semantic","occurredAt":null,"narrativeTime":null,"createdAt":"2026-05-01T00:00:00.000Z","keywords":["ledger"],"cosine":0.30000001192092896,"rawWeight":null,"blendedBefore":null,"multiplier":null,"fired":[],"blendedAfter":null,"selected":false}],"newPath":[{"memoryId":"e2000000-0000-4000-8000-0000000000a1","summary":"Visited Gullwing Quay at dawn.","kind":"episodic","occurredAt":"2026-06-03T06:00:00.000Z","narrativeTime":"the second dawn","createdAt":"2026-05-01T00:00:00.000Z","keywords":["quay","past","scope: wide","history"],"cosine":0.8999999761581421,"rawWeight":0.2795939868358039,"blendedBefore":0.679268857920964,"multiplier":1.6444999999999999,"fired":["past↑retro","ctx✓","window↑"],"blendedAfter":1.1170576418410254,"selected":true}]}"#;

/// Replace `http://localhost:...` spans (through the next ESC byte or
/// whitespace) with a placeholder, and — on connect-error lines — the reason
/// tail after `at <URL>: ` too. Documented transport truths that legitimately
/// differ between the two CLIs (URL shape; io-error wording).
fn normalize_recall(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        if let Some(pos) = line.find("http://localhost:") {
            let head = &line[..pos];
            let rest = &line[pos..];
            // The span ends at the first ESC (ANSI reset) or whitespace.
            let end = rest
                .char_indices()
                .find(|(_, c)| *c == '\u{1b}' || c.is_whitespace())
                .map(|(i, _)| i)
                .unwrap_or(rest.len());
            let tail = &rest[end..];
            if line.contains("Could not reach Quilltap server at ") {
                // Swallow the reason too — everything to the ESC/EOL.
                let reason_end = tail
                    .char_indices()
                    .find(|(_, c)| *c == '\u{1b}')
                    .map(|(i, _)| i)
                    .unwrap_or(tail.len());
                out.push_str(head);
                out.push_str("<TARGET>");
                out.push_str(&tail[reason_end..]);
            } else {
                out.push_str(head);
                out.push_str("<URL>");
                out.push_str(tail);
            }
        } else {
            out.push_str(line);
        }
    }
    out
}

/// A canned recall-replay stub server: answers every POST with the payload for
/// the caller's dialect — v4's raw route body on `/api/v1/...`, the dispatch
/// envelope on `/api/dispatch`. Runs on a detached thread for the test's life.
fn spawn_recall_stub(result_json: &'static str, error_arm: bool) -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            use std::io::{Read, Write};
            // Read the head + enough of the body (Connection: close both sides).
            let mut buf = Vec::new();
            let mut tmp = [0u8; 4096];
            loop {
                match stream.read(&mut tmp) {
                    Ok(0) => break,
                    Ok(n) => {
                        buf.extend_from_slice(&tmp[..n]);
                        let text = String::from_utf8_lossy(&buf);
                        if let Some(hdr_end) = text.find("\r\n\r\n") {
                            let cl = text
                                .lines()
                                .find(|l| l.to_ascii_lowercase().starts_with("content-length:"))
                                .and_then(|l| l.split(':').nth(1))
                                .and_then(|v| v.trim().parse::<usize>().ok())
                                .unwrap_or(0);
                            if buf.len() >= hdr_end + 4 + cl {
                                break;
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
            let text = String::from_utf8_lossy(&buf);
            let dispatch = text.starts_with("POST /api/dispatch");
            let (status_line, body) = if error_arm {
                if dispatch {
                    (
                        "HTTP/1.1 400 Bad Request",
                        "{\"type\":\"error\",\"data\":{\"kind\":\"badRequest\",\"message\":\"Chat settings not found.\"}}"
                            .to_string(),
                    )
                } else {
                    (
                        "HTTP/1.1 400 Bad Request",
                        "{\"error\":\"Chat settings not found.\"}".to_string(),
                    )
                }
            } else if dispatch {
                (
                    "HTTP/1.1 200 OK",
                    format!("{{\"type\":\"recallReplay\",\"data\":{result_json}}}"),
                )
            } else {
                ("HTTP/1.1 200 OK", result_json.to_string())
            };
            let resp = format!(
                "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
        }
    });
    port
}

/// A one-answer HTTP stub: every request gets the same status and body.
///
/// `db characters archive|rehydrate|export` post the SAME v4 URLs from both
/// CLIs, so one stub answers both sides and the request/print path is a real
/// Tier R arm rather than a unit test — the recall-replay precedent, minus
/// the dialect split.
/// Every request the canned stubs receive, in arrival order: the request line
/// and the body bytes. Each case runs v4 then v5 against the same stub, so the
/// captures arrive in (v4, v5) pairs — `assert_canned_wire_parity` checks each
/// pair agrees on URL + body, which turns the "both CLIs POST the same v4
/// URLs" claim from an inspection into an assertion (§3 review, finding 6).
static CANNED_WIRE: std::sync::Mutex<Vec<(String, Vec<u8>)>> = std::sync::Mutex::new(Vec::new());

fn assert_canned_wire_parity() {
    let captures = CANNED_WIRE.lock().unwrap();
    assert!(
        !captures.is_empty() && captures.len().is_multiple_of(2),
        "expected an even, non-empty number of canned-stub requests, got {}",
        captures.len()
    );
    for pair in captures.chunks(2) {
        assert_eq!(
            pair[0].0, pair[1].0,
            "v4 and v5 sent different request lines"
        );
        assert_eq!(
            pair[0].1, pair[1].1,
            "v4 and v5 sent different bodies for {}",
            pair[0].0
        );
    }
}

fn spawn_canned_stub(status: u16, body: &str) -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let body = body.to_string();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            use std::io::{Read, Write};
            let mut buf = Vec::new();
            let mut tmp = [0u8; 4096];
            loop {
                match stream.read(&mut tmp) {
                    Ok(0) => break,
                    Ok(n) => {
                        buf.extend_from_slice(&tmp[..n]);
                        let text = String::from_utf8_lossy(&buf);
                        if let Some(hdr_end) = text.find("\r\n\r\n") {
                            let cl = text
                                .lines()
                                .find(|l| l.to_ascii_lowercase().starts_with("content-length:"))
                                .and_then(|l| l.split(':').nth(1))
                                .and_then(|v| v.trim().parse::<usize>().ok())
                                .unwrap_or(0);
                            if buf.len() >= hdr_end + 4 + cl {
                                break;
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
            // Record what was actually sent (request line + body) for the
            // wire-parity assertion.
            {
                let text = String::from_utf8_lossy(&buf);
                let line = text.lines().next().unwrap_or("").to_string();
                let body_bytes = text
                    .find("\r\n\r\n")
                    .map(|i| buf[i + 4..].to_vec())
                    .unwrap_or_default();
                CANNED_WIRE.lock().unwrap().push((line, body_bytes));
            }
            let reason = match status {
                200 => "OK",
                400 => "Bad Request",
                500 => "Internal Server Error",
                _ => "Error",
            };
            let resp = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
        }
    });
    port
}

impl Ctx {
    fn reset_live(&self, opts: &CaseOpts) {
        if self.live.exists() {
            std::fs::remove_dir_all(&self.live).unwrap();
        }
        copy_dir(&self.master, &self.live);
        if let Some(pre) = &opts.pre {
            pre(&self.live);
        }
    }

    fn run_bin(
        &self,
        program: &str,
        base_args: &[String],
        args: &[String],
        opts: &CaseOpts,
    ) -> RunOut {
        let mut cmd = Command::new(program);
        cmd.args(base_args)
            .args(args)
            .env_remove("QUILLTAP_DATA_DIR")
            .env_remove("QUILLTAP_DB_PASSPHRASE")
            .env_remove("QUILLTAP_QUIET_HINTS")
            .env("HOME", self.live.join("home"))
            .env("TZ", "UTC")
            .stdin(if opts.stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(cwd) = &opts.cwd {
            std::fs::create_dir_all(cwd).unwrap();
            cmd.current_dir(cwd);
        }
        for (k, v) in &opts.envs {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn().expect("spawn CLI");
        if let Some(stdin) = &opts.stdin {
            use std::io::Write;
            child
                .stdin
                .as_mut()
                .unwrap()
                .write_all(stdin)
                .expect("write stdin");
        }
        let out = child.wait_with_output().expect("wait CLI");
        RunOut {
            stdout: out.stdout,
            stderr: out.stderr,
            code: out.status.code().unwrap_or(-1),
        }
    }

    fn run_v4(&self, args: &[String], opts: &CaseOpts) -> RunOut {
        self.run_bin(
            &self.node.clone(),
            &[self.v4_bin.to_string_lossy().into_owned()],
            args,
            opts,
        )
    }

    fn run_v5(&self, args: &[String], opts: &CaseOpts) -> RunOut {
        self.run_bin(env!("CARGO_BIN_EXE_quilltap"), &[], args, opts)
    }

    fn case_with(&mut self, name: &str, args: &[String], opts: CaseOpts) {
        self.cases_run += 1;
        self.reset_live(&opts);
        let a = self.run_v4(args, &opts);
        self.reset_live(&opts);
        let b = self.run_v5(args, &opts);
        self.compare(name, &a, &b, &opts);
    }

    fn case(&mut self, name: &str, args: &[&str]) {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        self.case_with(name, &args, CaseOpts::default());
    }

    fn compare(&mut self, name: &str, a: &RunOut, b: &RunOut, opts: &CaseOpts) {
        let norm = |bytes: &[u8]| -> Vec<u8> {
            let mut text = String::from_utf8_lossy(bytes).into_owned();
            if opts.normalize_heartbeat {
                text = normalize_heartbeat(&text);
            }
            if opts.normalize_recall {
                text = normalize_recall(&text);
            }
            if opts.normalize_reach {
                text = normalize_reach(&text);
            }
            text.into_bytes()
        };
        let (a_out, b_out) = (norm(&a.stdout), norm(&b.stdout));
        let (a_err, b_err) = (norm(&a.stderr), norm(&b.stderr));
        if a_out != b_out {
            self.failures.push(format!(
                "[{name}] stdout differs\n--- v4 ---\n{}\n--- v5 ---\n{}",
                String::from_utf8_lossy(&a_out),
                String::from_utf8_lossy(&b_out)
            ));
        }
        if a_err != b_err {
            self.failures.push(format!(
                "[{name}] stderr differs\n--- v4 ---\n{}\n--- v5 ---\n{}",
                String::from_utf8_lossy(&a_err),
                String::from_utf8_lossy(&b_err)
            ));
        }
        if a.code != b.code {
            self.failures.push(format!(
                "[{name}] exit code differs: v4={} v5={}",
                a.code, b.code
            ));
        }
    }
}

// ============================================================================
// Fixture
// ============================================================================

fn build_master(master: &Path, live: &Path) {
    let live_s = live.to_string_lossy().into_owned();

    // ---- HOME with the instance registry (mode 0600).
    #[cfg(target_os = "macos")]
    let app_dir = master.join("home/Library/Application Support/Quilltap");
    #[cfg(not(target_os = "macos"))]
    let app_dir = master.join("home/.quilltap");
    std::fs::create_dir_all(&app_dir).unwrap();
    let registry = format!(
        "{{\n  \"version\": 1,\n  \"instances\": {{\n    \"instA\": {{\n      \"path\": \"{live_s}/instA\"\n    }},\n    \"instB\": {{\n      \"path\": \"{live_s}/instB\",\n      \"passphrase\": \"{PASSPHRASE_B}\"\n    }}\n  }},\n  \"defaultInstance\": null\n}}\n"
    );
    let reg_path = app_dir.join("instances.json");
    std::fs::write(&reg_path, registry).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&reg_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }

    // ---- Instance A (internal-sentinel dbkey).
    let data_a = master.join("instA/data");
    std::fs::create_dir_all(&data_a).unwrap();
    dbkey::save_dbkey(&data_a, PEPPER, "").unwrap();

    // Main DB.
    {
        let w = Writer::open_writable(&data_a.join("quilltap.db"), PEPPER).unwrap();
        let c = w.connection();
        c.execute_batch(
            "CREATE TABLE widgets (id TEXT PRIMARY KEY, label TEXT, n REAL, flag INTEGER, note TEXT);
             CREATE TABLE gears (id TEXT PRIMARY KEY, teeth REAL);",
        )
        .unwrap();
        let mut ins = c
            .prepare("INSERT INTO widgets (id, label, n, flag, note) VALUES (?, ?, ?, ?, ?)")
            .unwrap();
        ins.execute(rusqlite::params![
            "w1",
            "plain",
            1.0f64,
            1i64,
            rusqlite::types::Null
        ])
        .unwrap();
        ins.execute(rusqlite::params!["w2", "it's", 2.5f64, 0i64, "say \"hi\""])
            .unwrap();
        ins.execute(rusqlite::params![
            "w3",
            "café",
            42.0f64,
            1i64,
            "line1\nline2"
        ])
        .unwrap();
        c.execute("INSERT INTO gears (id, teeth) VALUES ('g1', 12.0)", [])
            .unwrap();
    }

    // LLM-logs DB.
    {
        let w = Writer::open_writable(&data_a.join("quilltap-llm-logs.db"), PEPPER).unwrap();
        w.connection()
            .execute_batch(
                "CREATE TABLE llm_logs (id TEXT PRIMARY KEY, type TEXT);
                 INSERT INTO llm_logs VALUES ('l1', 'CHAT_MESSAGE');
                 INSERT INTO llm_logs VALUES ('l2', 'SUMMARIZATION');",
            )
            .unwrap();
    }

    // Mount-index DB (the real post-link-table schema subset the verbs read).
    {
        let w = Writer::open_writable(&data_a.join("quilltap-mount-index.db"), PEPPER).unwrap();
        let c = w.connection();
        c.execute_batch(&mount_index_ddl()).unwrap();

        let mut mp = c
            .prepare(
                "INSERT INTO doc_mount_points (id, name, basePath, mountType, storeType, includePatterns, excludePatterns, enabled, lastScannedAt, scanStatus, lastScanError, conversionStatus, conversionError, fileCount, chunkCount, totalSizeBytes, createdAt, updatedAt)
                 VALUES (?, ?, ?, ?, ?, '[\"*.md\"]', '[\".git\"]', ?, ?, 'idle', NULL, 'idle', NULL, ?, ?, ?, ?, ?)",
            )
            .unwrap();
        // notes: live counts match cached (4 links, 3 chunks).
        mp.execute(rusqlite::params![
            N1,
            "notes",
            "",
            "database",
            "documents",
            1i64,
            TS,
            4i64,
            3i64,
            1234i64,
            TS,
            TS2
        ])
        .unwrap();
        // archive: cached counts deliberately stale (the 'consider docs scan' note).
        mp.execute(rusqlite::params![
            N2,
            "archive",
            "",
            "database",
            "documents",
            1i64,
            rusqlite::types::Null,
            9i64,
            9i64,
            9999i64,
            TS,
            TS
        ])
        .unwrap();
        // attic: filesystem mount whose bytes live under <live>/fsdocs.
        mp.execute(rusqlite::params![
            N3,
            "attic",
            format!("{live_s}/fsdocs"),
            "filesystem",
            "documents",
            1i64,
            TS,
            1i64,
            0i64,
            22i64,
            TS,
            TS
        ])
        .unwrap();
        // Two mounts sharing the name 'twin' (the ambiguity paths).
        mp.execute(rusqlite::params![
            N4,
            "twin",
            "",
            "database",
            "documents",
            1i64,
            TS,
            0i64,
            0i64,
            0i64,
            TS,
            TS
        ])
        .unwrap();
        mp.execute(rusqlite::params![
            N5,
            "twin",
            "",
            "database",
            "documents",
            1i64,
            TS,
            0i64,
            0i64,
            0i64,
            TS,
            TS
        ])
        .unwrap();

        // Content rows.
        let mut f = c
            .prepare(
                "INSERT INTO doc_mount_files (id, sha256, fileSizeBytes, fileType, source, createdAt, updatedAt) VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .unwrap();
        f.execute(rusqlite::params![
            "F1",
            "a".repeat(64),
            24i64,
            "markdown",
            "database",
            TS,
            TS
        ])
        .unwrap();
        f.execute(rusqlite::params![
            "F2",
            "b".repeat(64),
            64i64,
            "markdown",
            "database",
            TS,
            TS
        ])
        .unwrap();
        f.execute(rusqlite::params![
            "F3",
            "c".repeat(64),
            8i64,
            "blob",
            "database",
            TS,
            TS
        ])
        .unwrap();
        f.execute(rusqlite::params![
            "F4",
            "d".repeat(64),
            30i64,
            "markdown",
            "database",
            TS,
            TS
        ])
        .unwrap();
        f.execute(rusqlite::params![
            "F5",
            "e".repeat(64),
            22i64,
            "markdown",
            "filesystem",
            TS,
            TS
        ])
        .unwrap();

        c.execute(
            "INSERT INTO doc_mount_folders (id, mountPointId, parentId, name, path, createdAt, updatedAt) VALUES ('FO1', ?, NULL, 'Knowledge', 'Knowledge', ?, ?)",
            rusqlite::params![N1, TS, TS2],
        )
        .unwrap();

        let mut l = c
            .prepare(
                "INSERT INTO doc_mount_file_links (id, fileId, mountPointId, relativePath, fileName, folderId, description, conversionStatus, extractedText, extractedTextSha256, extractionStatus, chunkCount, lastModified, createdAt, updatedAt, linkGroupId)
                 VALUES (?, ?, ?, ?, ?, ?, '', ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .unwrap();
        // notes/today.md — no chunks.
        l.execute(rusqlite::params![
            "L1",
            "F1",
            N1,
            "today.md",
            "today.md",
            rusqlite::types::Null,
            "converted",
            rusqlite::types::Null,
            rusqlite::types::Null,
            "none",
            0i64,
            TS,
            TS,
            TS,
            rusqlite::types::Null
        ])
        .unwrap();
        // notes/Knowledge/facts.md — 2 chunks, 1 embedded ('~').
        l.execute(rusqlite::params![
            "L2",
            "F2",
            N1,
            "Knowledge/facts.md",
            "facts.md",
            "FO1",
            "converted",
            rusqlite::types::Null,
            rusqlite::types::Null,
            "none",
            2i64,
            TS2,
            TS,
            TS,
            rusqlite::types::Null
        ])
        .unwrap();
        // notes/pic.png — binary with extracted text ('T').
        l.execute(rusqlite::params![
            "L3",
            "F3",
            N1,
            "pic.png",
            "pic.png",
            rusqlite::types::Null,
            "converted",
            "A small test picture.",
            "f".repeat(64),
            "converted",
            0i64,
            TS,
            TS,
            TS,
            rusqlite::types::Null
        ])
        .unwrap();
        // notes/shared.md + archive/shared-too.md — a DELIBERATE hard link
        // (`linkGroupId` LG1). Both report links=2 and expand under `--links`.
        l.execute(rusqlite::params![
            "L4",
            "F4",
            N1,
            "shared.md",
            "shared.md",
            rusqlite::types::Null,
            "converted",
            rusqlite::types::Null,
            rusqlite::types::Null,
            "none",
            1i64,
            TS,
            TS,
            TS,
            "LG1"
        ])
        .unwrap();
        l.execute(rusqlite::params![
            "L5",
            "F4",
            N2,
            "shared-too.md",
            "shared-too.md",
            rusqlite::types::Null,
            "converted",
            rusqlite::types::Null,
            rusqlite::types::Null,
            "none",
            0i64,
            TS,
            TS,
            TS,
            "LG1"
        ])
        .unwrap();
        // [40319484] notes/coincidence.md shares F4's content row by sha dedup
        // and NOTHING else — the case that used to report "2 links". It must now
        // report 1 and expand to nothing, while L4/L5 still report 2.
        l.execute(rusqlite::params![
            "L7",
            "F4",
            N1,
            "coincidence.md",
            "coincidence.md",
            rusqlite::types::Null,
            "converted",
            rusqlite::types::Null,
            rusqlite::types::Null,
            "none",
            0i64,
            TS,
            TS,
            TS,
            rusqlite::types::Null
        ])
        .unwrap();
        // attic/attic-note.md — filesystem-backed.
        l.execute(rusqlite::params![
            "L6",
            "F5",
            N3,
            "attic-note.md",
            "attic-note.md",
            rusqlite::types::Null,
            "converted",
            rusqlite::types::Null,
            rusqlite::types::Null,
            "none",
            0i64,
            TS,
            TS,
            TS,
            rusqlite::types::Null
        ])
        .unwrap();

        let mut d = c
            .prepare(
                "INSERT INTO doc_mount_documents (id, fileId, content, contentSha256, plainTextLength, createdAt, updatedAt) VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .unwrap();
        d.execute(rusqlite::params![
            "D1",
            "F1",
            "# Today\n\nHello, Salon.\n",
            "a".repeat(64),
            24i64,
            TS,
            TS
        ])
        .unwrap();
        d.execute(rusqlite::params![
            "D2",
            "F2",
            "# Facts\n\n- The cipher is ChaCha20.\n- Café rules apply.\n",
            "b".repeat(64),
            64i64,
            TS,
            TS
        ])
        .unwrap();
        d.execute(rusqlite::params![
            "D4",
            "F4",
            "Shared bytes, two homes.\n",
            "d".repeat(64),
            30i64,
            TS,
            TS
        ])
        .unwrap();

        c.execute(
            "INSERT INTO doc_mount_blobs (id, fileId, sha256, sizeBytes, storedMimeType, data, createdAt, updatedAt) VALUES ('B1', 'F3', ?, 8, 'image/png', ?, ?, ?)",
            rusqlite::params![
                "c".repeat(64),
                vec![0x89u8, 0x50, 0x4e, 0x47, 0x00, 0x01, 0xfe, 0xff],
                TS,
                TS
            ],
        )
        .unwrap();

        let mut ch = c
            .prepare(
                "INSERT INTO doc_mount_chunks (id, linkId, mountPointId, chunkIndex, content, tokenCount, headingContext, embedding, createdAt, updatedAt) VALUES (?, ?, ?, ?, ?, ?, NULL, ?, ?, ?)",
            )
            .unwrap();
        ch.execute(rusqlite::params![
            "C1",
            "L2",
            N1,
            0i64,
            "The cipher is ChaCha20.",
            6i64,
            vec![0u8, 0, 128, 63],
            TS,
            TS
        ])
        .unwrap();
        ch.execute(rusqlite::params![
            "C2",
            "L2",
            N1,
            1i64,
            "Café rules apply.",
            5i64,
            rusqlite::types::Null,
            TS,
            TS
        ])
        .unwrap();
        ch.execute(rusqlite::params![
            "C3",
            "L4",
            N1,
            0i64,
            "Shared bytes, two homes.",
            5i64,
            vec![0u8, 0, 128, 63],
            TS,
            TS
        ])
        .unwrap();
    }

    // ---- The `db characters` family's slice of instance A (P4.D66).
    build_characters_fixture(master, &data_a);

    // The attic's on-disk file.
    let fsdocs = master.join("fsdocs");
    std::fs::create_dir_all(&fsdocs).unwrap();
    std::fs::write(fsdocs.join("attic-note.md"), "Up in the attic.\n").unwrap();

    // ---- Instance B (passphrase-protected dbkey).
    let data_b = master.join("instB/data");
    std::fs::create_dir_all(&data_b).unwrap();
    dbkey::save_dbkey(&data_b, PEPPER, PASSPHRASE_B).unwrap();
    {
        let w = Writer::open_writable(&data_b.join("quilltap.db"), PEPPER).unwrap();
        w.connection()
            .execute_batch(
                "CREATE TABLE secrets (id TEXT PRIMARY KEY);
                 INSERT INTO secrets VALUES ('s1');",
            )
            .unwrap();
    }
}

// ============================================================================
// The `db characters` fixture (P4.D66)
// ============================================================================

/// Character ids (name-sorted where the table order matters).
const CH_BRAM: &str = "b0000000-0000-4000-8000-00000000000b";
const CH_ELOWEN: &str = "e0000000-0000-4000-8000-00000000000e";
const CH_NELL: &str = "0e000000-0000-4000-8000-0000000000ee";
const CH_ORRIN: &str = "07000000-0000-4000-8000-0000000000aa";
const CH_PIPPA: &str = "b1000000-0000-4000-8000-0000000000bb";
const CH_ROWAN: &str = "40000000-0000-4000-8000-0000000000cc";
const CH_TWIN_A: &str = "d1000000-0000-4000-8000-0000000000d1";
const CH_TWIN_B: &str = "d2000000-0000-4000-8000-0000000000d2";
const CH_SABLE: &str = "5a000000-0000-4000-8000-0000000000f1";
const CH_TOBIAS: &str = "70000000-0000-4000-8000-0000000000f2";
const CH_UMBER: &str = "11000000-0000-4000-8000-0000000000f3";
const CH_VESPER: &str = "0e100000-0000-4000-8000-0000000000f4";
const CH_WREN: &str = "e1000000-0000-4000-8000-0000000000f5";
const CH_YARROW: &str = "1a000000-0000-4000-8000-0000000000f6";
const CH_ZEPHYR: &str = "2e000000-0000-4000-8000-0000000000f7";
const CH_THORN: &str = "70100000-0000-4000-8000-0000000000f8";
const CH_CORVID: &str = "c0000000-0000-4000-8000-0000000000f9";

/// The character vaults (mount points in the mount-index DB).
const VAULT_ELOWEN: &str = "0a000000-0000-4000-8000-0000000000a1";
const VAULT_BRAM: &str = "0a000000-0000-4000-8000-0000000000a2";
const VAULT_ORRIN: &str = "0a000000-0000-4000-8000-0000000000a3";
const VAULT_PIPPA: &str = "0a000000-0000-4000-8000-0000000000a4";
const VAULT_ROWAN: &str = "0a000000-0000-4000-8000-0000000000a5";

/// The passphrase the Vesper bundle predates.
const OLD_PASSPHRASE: &str = "the-old-passphrase";

/// v4's `characters` DDL at `ed8934f1` (from `generateDDL`, via
/// `fresh_schema.json`) — verbatim, because the verb's `PRAGMA table_info`
/// tolerance keys off exactly which columns exist. Note that the 4.6 vault
/// cutover ABANDONED the content columns without dropping them, so a modern
/// instance still reports `preCutover` — the divergence report is the live
/// path, not the dead one.
const CHARACTERS_DDL: &str = r#"
CREATE TABLE "characters" (
  "id" TEXT PRIMARY KEY NOT NULL,
  "userId" TEXT NOT NULL,
  "name" TEXT NOT NULL,
  "title" TEXT,
  "identity" TEXT,
  "description" TEXT,
  "manifesto" TEXT,
  "personality" TEXT,
  "scenarios" TEXT DEFAULT '[]',
  "firstMessage" TEXT,
  "exampleDialogues" TEXT,
  "systemPrompts" TEXT DEFAULT '[]',
  "sillyTavernData" TEXT,
  "metadata" TEXT,
  "isFavorite" INTEGER DEFAULT 0,
  "npc" INTEGER DEFAULT 0,
  "talkativeness" REAL DEFAULT 0.5,
  "controlledBy" TEXT DEFAULT 'llm',
  "characterDocumentMountPointId" TEXT,
  "archivedAt" TEXT,
  "archiveFileId" TEXT,
  "archivedAvatarFileId" TEXT,
  "systemTransparency" INTEGER,
  "aliases" TEXT DEFAULT '[]',
  "pronouns" TEXT,
  "createdAt" TEXT NOT NULL,
  "updatedAt" TEXT NOT NULL
);
CREATE TABLE "files" (
  "id" TEXT PRIMARY KEY NOT NULL,
  "userId" TEXT NOT NULL,
  "sha256" TEXT NOT NULL,
  "originalFilename" TEXT NOT NULL,
  "mimeType" TEXT NOT NULL,
  "size" REAL NOT NULL,
  "linkedTo" TEXT DEFAULT '[]',
  "source" TEXT NOT NULL,
  "category" TEXT NOT NULL,
  "description" TEXT,
  "tags" TEXT DEFAULT '[]',
  "storageKey" TEXT,
  "fileStatus" TEXT DEFAULT 'ok',
  "createdAt" TEXT NOT NULL,
  "updatedAt" TEXT NOT NULL
);
"#;

/// Build the characters + files rows in the main DB, their vaults in the
/// mount index, and the ARCHIVE bundle bytes under `<instance>/files/`.
///
/// The bundles are planted with **core's own** archive crypto
/// (`encrypt_archive`), which is the format v4 reads — the same
/// `QTAPARC1` layout round 1 proved byte-exact against v4's real
/// `archive-crypto.ts`. That keeps this lane independent of P4.D65's
/// committed fixture family, whose DBs carry a different pepper and no
/// on-disk bundle bytes at all.
fn build_characters_fixture(master: &Path, data_a: &Path) {
    let w = Writer::open_writable(&data_a.join("quilltap.db"), PEPPER).unwrap();
    let c = w.connection();
    c.execute_batch(CHARACTERS_DDL).unwrap();

    // ---- The bundle bytes (planted before the rows so `size` is the truth).
    let files_dir = master.join("instA/files/archives");
    std::fs::create_dir_all(&files_dir).unwrap();
    let plant = |name: &str, bytes: &[u8]| -> f64 {
        std::fs::write(files_dir.join(name), bytes).unwrap();
        bytes.len() as f64
    };
    let sable_plain = b"{\"kind\":\"character\",\"name\":\"Sable\"}\n{\"kind\":\"memory\"}\n";
    let sable_bytes = quilltap_core::services::character_archive::crypto::encrypt_archive(
        sable_plain,
        quilltap_core::dbkey::INTERNAL_PASSPHRASE,
        None,
    )
    .unwrap();
    let sable_size = plant("sable.qtaparc", &sable_bytes);
    // A pre-encryption bundle: no magic, passed through untouched.
    let umber_size = plant(
        "umber-plain.qtap",
        b"{\"kind\":\"character\",\"name\":\"Umber\"}\n",
    );
    // Encrypted under a passphrase this instance no longer has.
    let vesper_bytes = quilltap_core::services::character_archive::crypto::encrypt_archive(
        b"{\"kind\":\"character\",\"name\":\"Vesper\"}\n",
        OLD_PASSPHRASE,
        None,
    )
    .unwrap();
    let vesper_size = plant("vesper.qtaparc", &vesper_bytes);
    // Cut short of its auth tag: the launcher's OWN length check fires
    // before any crypto runs.
    let thorn_full = quilltap_core::services::character_archive::crypto::encrypt_archive(
        b"{\"kind\":\"character\",\"name\":\"Thorn\"}\n",
        quilltap_core::dbkey::INTERNAL_PASSPHRASE,
        None,
    )
    .unwrap();
    let thorn_size = plant("truncated.qtaparc", &thorn_full[..thorn_full.len() - 50]);
    // Full length, right key, flipped tag: the length check passes, the key
    // hash matches, and GCM refuses — the corruption arm, distinct from a
    // wrong passphrase.
    let mut corvid_bytes = thorn_full.clone();
    let last = corvid_bytes.len() - 1;
    corvid_bytes[last] ^= 0xff;
    let corvid_size = plant("corrupt.qtaparc", &corvid_bytes);
    let orphan_size = plant("orphan.qtaparc", &sable_bytes);

    // ---- characters.
    let mut ins = c
        .prepare(
            "INSERT INTO characters (id, userId, name, title, identity, description, manifesto, personality,
                                     scenarios, firstMessage, exampleDialogues, systemPrompts, isFavorite, npc,
                                     talkativeness, controlledBy, characterDocumentMountPointId, archivedAt,
                                     archiveFileId, archivedAvatarFileId, systemTransparency, aliases, pronouns,
                                     createdAt, updatedAt)
             VALUES (?, 'u1', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, 0, ?, 'llm', ?, ?, ?, NULL, ?, ?, ?, ?, ?)",
        )
        .unwrap();
    // A live, healthy character whose DB columns still agree with the vault.
    ins.execute(rusqlite::params![
        CH_ELOWEN,
        "Elowen",
        "Cartographer",
        "I map the coastline.",
        "A cartographer of tidal charts.",
        "Chart the unknown.",
        "Patient, exacting.",
        r#"[{"id":"s1","name":"The Quay"}]"#,
        "Hello.",
        "You: Where to?\nElowen: East.",
        r#"[{"id":"p1"},{"id":"p2"}]"#,
        0.5f64,
        VAULT_ELOWEN,
        rusqlite::types::Null,
        rusqlite::types::Null,
        1i64,
        r#"["Ellie","The Cartographer"]"#,
        "she/her",
        TS,
        TS2
    ])
    .unwrap();
    // Three of the five required single files never written.
    ins.execute(rusqlite::params![
        CH_BRAM,
        "Bram",
        rusqlite::types::Null,
        "A ledger-keeper.",
        rusqlite::types::Null,
        rusqlite::types::Null,
        rusqlite::types::Null,
        "[]",
        rusqlite::types::Null,
        rusqlite::types::Null,
        "[]",
        0.5f64,
        VAULT_BRAM,
        rusqlite::types::Null,
        rusqlite::types::Null,
        rusqlite::types::Null,
        "[]",
        rusqlite::types::Null,
        TS,
        TS
    ])
    .unwrap();
    // No vault at all.
    ins.execute(rusqlite::params![
        CH_NELL,
        "Nell",
        rusqlite::types::Null,
        rusqlite::types::Null,
        rusqlite::types::Null,
        rusqlite::types::Null,
        rusqlite::types::Null,
        "[]",
        rusqlite::types::Null,
        rusqlite::types::Null,
        "[]",
        0.5f64,
        rusqlite::types::Null,
        rusqlite::types::Null,
        rusqlite::types::Null,
        rusqlite::types::Null,
        "[]",
        rusqlite::types::Null,
        TS,
        TS
    ])
    .unwrap();
    // A vault with no links at all.
    ins.execute(rusqlite::params![
        CH_ORRIN,
        "Orrin",
        rusqlite::types::Null,
        rusqlite::types::Null,
        rusqlite::types::Null,
        rusqlite::types::Null,
        rusqlite::types::Null,
        "[]",
        rusqlite::types::Null,
        rusqlite::types::Null,
        "[]",
        0.5f64,
        VAULT_ORRIN,
        rusqlite::types::Null,
        rusqlite::types::Null,
        rusqlite::types::Null,
        "[]",
        rusqlite::types::Null,
        TS,
        TS
    ])
    .unwrap();
    // Exactly one of the physical-* pair.
    ins.execute(rusqlite::params![
        CH_PIPPA,
        "Pippa",
        rusqlite::types::Null,
        "A signal-keeper.",
        "Keeps the lamps.",
        rusqlite::types::Null,
        "Bright.",
        "[]",
        rusqlite::types::Null,
        "You: Evening.\nPippa: Evening.",
        "[]",
        0.5f64,
        VAULT_PIPPA,
        rusqlite::types::Null,
        rusqlite::types::Null,
        rusqlite::types::Null,
        "[]",
        rusqlite::types::Null,
        TS,
        TS
    ])
    .unwrap();
    // Every file present and every comparable field disagreeing.
    ins.execute(rusqlite::params![
        CH_ROWAN,
        "Rowan",
        "Ferryman",
        "DB identity, stale.",
        "DB description, stale.",
        "DB manifesto.",
        "DB personality.",
        r#"[{"id":"sx"}]"#,
        "DB first message.",
        "DB dialogues.",
        r#"[{"id":"px"}]"#,
        0.5f64,
        VAULT_ROWAN,
        rusqlite::types::Null,
        rusqlite::types::Null,
        rusqlite::types::Null,
        r#"["Ro"]"#,
        "he/him",
        TS,
        TS
    ])
    .unwrap();
    // Two rows sharing a name — the ambiguous-resolution arm (exit code 2).
    for id in [CH_TWIN_A, CH_TWIN_B] {
        ins.execute(rusqlite::params![
            id,
            "Twin",
            rusqlite::types::Null,
            rusqlite::types::Null,
            rusqlite::types::Null,
            rusqlite::types::Null,
            rusqlite::types::Null,
            "[]",
            rusqlite::types::Null,
            rusqlite::types::Null,
            "[]",
            0.5f64,
            rusqlite::types::Null,
            rusqlite::types::Null,
            rusqlite::types::Null,
            rusqlite::types::Null,
            "[]",
            rusqlite::types::Null,
            TS,
            TS
        ])
        .unwrap();
    }
    // The archived shelf. Each one is a distinct export arm; `archivedAt`
    // values are distinct so `ORDER BY archivedAt DESC` is deterministic.
    let archived: &[(&str, &str, &str, rusqlite::types::Value)] = &[
        (
            CH_SABLE,
            "Sable",
            "2026-03-01T00:00:08.000Z",
            rusqlite::types::Value::Text("fa000000-0000-4000-8000-000000000001".into()),
        ),
        (
            CH_TOBIAS,
            "Tobias",
            "2026-03-01T00:00:07.000Z",
            rusqlite::types::Value::Null,
        ),
        (
            CH_UMBER,
            "Umber",
            "2026-03-01T00:00:06.000Z",
            rusqlite::types::Value::Text("fa000000-0000-4000-8000-000000000003".into()),
        ),
        (
            CH_VESPER,
            "Vesper",
            "2026-03-01T00:00:05.000Z",
            rusqlite::types::Value::Text("fa000000-0000-4000-8000-000000000004".into()),
        ),
        (
            CH_WREN,
            "Wren",
            "2026-03-01T00:00:04.000Z",
            rusqlite::types::Value::Text("fa000000-0000-4000-8000-0000000000ff".into()),
        ),
        (
            CH_YARROW,
            "Yarrow",
            "2026-03-01T00:00:03.000Z",
            rusqlite::types::Value::Text("fa000000-0000-4000-8000-000000000005".into()),
        ),
        (
            CH_ZEPHYR,
            "Zephyr",
            "2026-03-01T00:00:02.000Z",
            rusqlite::types::Value::Text("fa000000-0000-4000-8000-000000000006".into()),
        ),
        (
            CH_THORN,
            "Thorn",
            "2026-03-01T00:00:01.000Z",
            rusqlite::types::Value::Text("fa000000-0000-4000-8000-000000000007".into()),
        ),
        (
            CH_CORVID,
            "Corvid",
            "2026-03-01T00:00:00.500Z",
            rusqlite::types::Value::Text("fa000000-0000-4000-8000-000000000008".into()),
        ),
    ];
    for (id, name, archived_at, file_id) in archived {
        ins.execute(rusqlite::params![
            id,
            name,
            rusqlite::types::Null,
            rusqlite::types::Null,
            rusqlite::types::Null,
            rusqlite::types::Null,
            rusqlite::types::Null,
            "[]",
            rusqlite::types::Null,
            rusqlite::types::Null,
            "[]",
            0.5f64,
            rusqlite::types::Null,
            archived_at,
            file_id,
            rusqlite::types::Null,
            "[]",
            rusqlite::types::Null,
            TS,
            TS
        ])
        .unwrap();
    }
    drop(ins);

    // ---- files: the ARCHIVE shelf (+ one non-ARCHIVE row the filter drops).
    let mut f = c
        .prepare(
            "INSERT INTO files (id, userId, sha256, originalFilename, mimeType, size, source, category, storageKey, createdAt, updatedAt)
             VALUES (?, 'u1', ?, ?, 'application/octet-stream', ?, 'archive', ?, ?, ?, ?)",
        )
        .unwrap();
    let mut file_row =
        |id: &str, filename: &str, size: f64, key: rusqlite::types::Value, created: &str| {
            f.execute(rusqlite::params![
                id,
                "0".repeat(64),
                filename,
                size,
                "ARCHIVE",
                key,
                created,
                created
            ])
            .unwrap();
        };
    file_row(
        "fa000000-0000-4000-8000-000000000001",
        "Sable-a-name-long-enough-to-need-the-forty-four-char-truncation.qtap",
        sable_size,
        rusqlite::types::Value::Text("archives/sable.qtaparc".into()),
        "2026-03-01T00:01:08.000Z",
    );
    file_row(
        "fa000000-0000-4000-8000-000000000003",
        "Umber-archive.qtap",
        umber_size,
        rusqlite::types::Value::Text("archives/umber-plain.qtap".into()),
        "2026-03-01T00:01:06.000Z",
    );
    file_row(
        "fa000000-0000-4000-8000-000000000004",
        "Vesper-archive.qtap",
        vesper_size,
        rusqlite::types::Value::Text("archives/vesper.qtaparc".into()),
        "2026-03-01T00:01:05.000Z",
    );
    // No storage key: the row exists, the bytes were never written.
    file_row(
        "fa000000-0000-4000-8000-000000000005",
        "Yarrow-archive.qtap",
        0.0,
        rusqlite::types::Value::Null,
        "2026-03-01T00:01:03.000Z",
    );
    // A storage key whose file is not on disk.
    file_row(
        "fa000000-0000-4000-8000-000000000006",
        "Zephyr-archive.qtap",
        321.0,
        rusqlite::types::Value::Text("archives/absent.qtaparc".into()),
        "2026-03-01T00:01:02.000Z",
    );
    file_row(
        "fa000000-0000-4000-8000-000000000007",
        "Thorn-archive.qtap",
        thorn_size,
        rusqlite::types::Value::Text("archives/truncated.qtaparc".into()),
        "2026-03-01T00:01:01.000Z",
    );
    file_row(
        "fa000000-0000-4000-8000-000000000008",
        "Corvid-archive.qtap",
        corvid_size,
        rusqlite::types::Value::Text("archives/corrupt.qtaparc".into()),
        "2026-03-01T00:01:00.500Z",
    );
    // A bundle no character points at — the survivor of a "keep archived
    // bundles" wipe, importable only.
    file_row(
        "fa000000-0000-4000-8000-00000000000a",
        "Orphan-archive.qtap",
        orphan_size,
        rusqlite::types::Value::Text("archives/orphan.qtaparc".into()),
        "2026-03-01T00:01:00.000Z",
    );
    drop(f);
    c.execute(
        "INSERT INTO files (id, userId, sha256, originalFilename, mimeType, size, source, category, storageKey, createdAt, updatedAt)
         VALUES ('fb000000-0000-4000-8000-000000000001', 'u1', ?, 'portrait.png', 'image/png', 12, 'upload', 'AVATAR', 'images/portrait.png', ?, ?)",
        rusqlite::params!["1".repeat(64), TS, TS],
    )
    .unwrap();
    drop(w);

    // ---- The vaults themselves, in the mount index.
    let w = Writer::open_writable(&data_a.join("quilltap-mount-index.db"), PEPPER).unwrap();
    let c = w.connection();
    let mut mp = c
        .prepare(
            "INSERT INTO doc_mount_points (id, name, basePath, mountType, storeType, includePatterns, excludePatterns, enabled, lastScannedAt, scanStatus, lastScanError, conversionStatus, conversionError, fileCount, chunkCount, totalSizeBytes, createdAt, updatedAt)
             VALUES (?, ?, '', 'database', 'character', '[\"*.md\"]', '[]', 1, ?, 'idle', NULL, 'idle', NULL, 0, 0, 0, ?, ?)",
        )
        .unwrap();
    for (id, name) in [
        (VAULT_ELOWEN, "vault-elowen"),
        (VAULT_BRAM, "vault-bram"),
        (VAULT_ORRIN, "vault-orrin"),
        (VAULT_PIPPA, "vault-pippa"),
        (VAULT_ROWAN, "vault-rowan"),
    ] {
        mp.execute(rusqlite::params![id, name, TS, TS, TS]).unwrap();
    }
    drop(mp);

    let mut nth = 0usize;
    let mut file_stmt = c
        .prepare("INSERT INTO doc_mount_files (id, sha256, fileSizeBytes, fileType, source, createdAt, updatedAt) VALUES (?, ?, ?, 'markdown', 'database', ?, ?)")
        .unwrap();
    let mut link_stmt = c
        .prepare(
            "INSERT INTO doc_mount_file_links (id, fileId, mountPointId, relativePath, fileName, folderId, description, conversionStatus, extractionStatus, chunkCount, lastModified, createdAt, updatedAt)
             VALUES (?, ?, ?, ?, ?, NULL, '', 'converted', 'none', 0, ?, ?, ?)",
        )
        .unwrap();
    let mut doc_stmt = c
        .prepare("INSERT INTO doc_mount_documents (id, fileId, content, contentSha256, plainTextLength, createdAt, updatedAt) VALUES (?, ?, ?, ?, ?, ?, ?)")
        .unwrap();
    let mut vault_doc = |mount: &str, rel: &str, content: &str| {
        nth += 1;
        let fid = format!("VF{nth}");
        file_stmt
            .execute(rusqlite::params![
                fid,
                format!("{:0>64}", nth),
                content.len() as i64,
                TS,
                TS
            ])
            .unwrap();
        link_stmt
            .execute(rusqlite::params![
                format!("VL{nth}"),
                fid,
                mount,
                rel,
                rel.rsplit('/').next().unwrap(),
                TS,
                TS,
                TS
            ])
            .unwrap();
        doc_stmt
            .execute(rusqlite::params![
                format!("VD{nth}"),
                fid,
                content,
                format!("{:0>64}", nth),
                content.len() as i64,
                TS,
                TS
            ])
            .unwrap();
    };

    // Elowen: complete, and every comparable field agreeing with the DB.
    vault_doc(
        VAULT_ELOWEN,
        "properties.json",
        r#"{"pronouns":"she/her","title":"Cartographer","firstMessage":"Hello.","talkativeness":0.5,"aliases":["Ellie","The Cartographer"],"systemTransparency":1}"#,
    );
    vault_doc(VAULT_ELOWEN, "identity.md", "I map the coastline.");
    vault_doc(
        VAULT_ELOWEN,
        "description.md",
        "A cartographer of tidal charts.",
    );
    vault_doc(VAULT_ELOWEN, "personality.md", "Patient, exacting.");
    vault_doc(
        VAULT_ELOWEN,
        "example-dialogues.md",
        "You: Where to?\nElowen: East.",
    );
    vault_doc(VAULT_ELOWEN, "manifesto.md", "Chart the unknown.");
    vault_doc(VAULT_ELOWEN, "prompts/harbour.md", "# Harbour");
    vault_doc(VAULT_ELOWEN, "prompts/inland.md", "# Inland");
    vault_doc(VAULT_ELOWEN, "scenarios/the-quay.md", "# The Quay");
    vault_doc(VAULT_ELOWEN, "wardrobe/oilskin.md", "# Oilskin");
    vault_doc(VAULT_ELOWEN, "wardrobe/gloves.md", "# Gloves");
    vault_doc(VAULT_ELOWEN, "wardrobe/boots.md", "# Boots");

    // Bram: two of the five required files.
    vault_doc(VAULT_BRAM, "properties.json", r#"{"pronouns":null}"#);
    vault_doc(VAULT_BRAM, "identity.md", "A ledger-keeper.");

    // Pippa: all five, plus exactly one of the physical pair.
    vault_doc(
        VAULT_PIPPA,
        "properties.json",
        r#"{"pronouns":null,"title":null,"firstMessage":null,"talkativeness":0.5,"aliases":[]}"#,
    );
    vault_doc(VAULT_PIPPA, "identity.md", "A signal-keeper.");
    vault_doc(VAULT_PIPPA, "description.md", "Keeps the lamps.");
    vault_doc(VAULT_PIPPA, "personality.md", "Bright.");
    vault_doc(
        VAULT_PIPPA,
        "example-dialogues.md",
        "You: Evening.\nPippa: Evening.",
    );
    vault_doc(VAULT_PIPPA, "physical-description.md", "Weathered, tall.");

    // Rowan: complete and thoroughly diverged, including the physical pair
    // (whose DB side no longer exists at all — `physicalDescriptions` was
    // dropped from the schema, so v4 compares against the empty string).
    vault_doc(
        VAULT_ROWAN,
        "properties.json",
        r#"{"pronouns":"they/them","title":"Ferryman","firstMessage":"Vault first message.","talkativeness":0.5,"aliases":["Ro","Rowan of the Ford"],"systemTransparency":0}"#,
    );
    vault_doc(VAULT_ROWAN, "identity.md", "Vault identity, current.");
    vault_doc(VAULT_ROWAN, "description.md", "Vault description, current.");
    vault_doc(VAULT_ROWAN, "personality.md", "DB personality.");
    vault_doc(VAULT_ROWAN, "example-dialogues.md", "DB dialogues.");
    vault_doc(VAULT_ROWAN, "manifesto.md", "DB manifesto.");
    vault_doc(VAULT_ROWAN, "physical-description.md", "Broad-shouldered.");
    vault_doc(
        VAULT_ROWAN,
        "physical-prompts.json",
        r#"{"short":"a ferryman","medium":"a broad ferryman","long":"a broad ferryman at the ford","complete":"a broad ferryman at the ford, dusk"}"#,
    );
    drop(file_stmt);
    drop(link_stmt);
    drop(doc_stmt);
    drop(w);
}

fn mount_index_ddl() -> String {
    // The real v4 schema (docs/v4/developer/DDL.md) for the tables the docs
    // verbs read — column set + affinities faithful.
    r#"
CREATE TABLE IF NOT EXISTS "doc_mount_points" (
  "id" TEXT PRIMARY KEY,
  "name" TEXT NOT NULL,
  "basePath" TEXT NOT NULL DEFAULT '',
  "mountType" TEXT NOT NULL DEFAULT 'filesystem',
  "storeType" TEXT NOT NULL DEFAULT 'documents',
  "includePatterns" TEXT NOT NULL DEFAULT '["*.md","*.txt","*.pdf","*.docx"]',
  "excludePatterns" TEXT NOT NULL DEFAULT '[".git","node_modules",".obsidian",".trash"]',
  "enabled" INTEGER NOT NULL DEFAULT 1,
  "lastScannedAt" TEXT,
  "scanStatus" TEXT NOT NULL DEFAULT 'idle',
  "lastScanError" TEXT,
  "conversionStatus" TEXT NOT NULL DEFAULT 'idle',
  "conversionError" TEXT,
  "fileCount" INTEGER NOT NULL DEFAULT 0,
  "chunkCount" INTEGER NOT NULL DEFAULT 0,
  "totalSizeBytes" INTEGER NOT NULL DEFAULT 0,
  "createdAt" TEXT NOT NULL,
  "updatedAt" TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS "doc_mount_folders" (
  "id" TEXT PRIMARY KEY,
  "mountPointId" TEXT NOT NULL REFERENCES "doc_mount_points"("id"),
  "parentId" TEXT,
  "name" TEXT NOT NULL,
  "path" TEXT NOT NULL,
  "createdAt" TEXT NOT NULL,
  "updatedAt" TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS "doc_mount_files" (
  "id" TEXT PRIMARY KEY,
  "sha256" TEXT NOT NULL,
  "fileSizeBytes" INTEGER NOT NULL,
  "fileType" TEXT NOT NULL,
  "source" TEXT NOT NULL DEFAULT 'filesystem',
  "createdAt" TEXT NOT NULL,
  "updatedAt" TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS "doc_mount_file_links" (
  "id" TEXT PRIMARY KEY,
  "fileId" TEXT NOT NULL REFERENCES "doc_mount_files"("id") ON DELETE CASCADE,
  "mountPointId" TEXT NOT NULL REFERENCES "doc_mount_points"("id") ON DELETE CASCADE,
  "relativePath" TEXT NOT NULL,
  "fileName" TEXT NOT NULL,
  "folderId" TEXT,
  "originalFileName" TEXT,
  "originalMimeType" TEXT,
  "description" TEXT NOT NULL DEFAULT '',
  "descriptionUpdatedAt" TEXT,
  "conversionStatus" TEXT NOT NULL DEFAULT 'pending',
  "conversionError" TEXT,
  "plainTextLength" INTEGER,
  "extractedText" TEXT,
  "extractedTextSha256" TEXT,
  "extractionStatus" TEXT NOT NULL DEFAULT 'none',
  "extractionError" TEXT,
  "chunkCount" INTEGER NOT NULL DEFAULT 0,
  "allowEmbed" INTEGER NOT NULL DEFAULT 1,
  "allowCharacterRead" INTEGER NOT NULL DEFAULT 1,
  "allowCharacterWrite" INTEGER NOT NULL DEFAULT 1,
  "lastModified" TEXT NOT NULL,
  "createdAt" TEXT NOT NULL,
  "updatedAt" TEXT NOT NULL,
  -- v4 `40319484`: deliberate hard-link groups. APPENDED, which is the shape a
  -- real instance gets it in (from the migration / ensureLinkGroupColumn); the
  -- generateDDL position is separately pinned by fresh_schema.json.
  "linkGroupId" TEXT DEFAULT NULL
);
CREATE INDEX IF NOT EXISTS "idx_doc_mount_file_links_linkGroupId"
  ON "doc_mount_file_links" ("linkGroupId") WHERE "linkGroupId" IS NOT NULL;
CREATE TABLE IF NOT EXISTS "doc_mount_documents" (
  "id" TEXT PRIMARY KEY,
  "fileId" TEXT NOT NULL REFERENCES "doc_mount_files"("id") ON DELETE CASCADE,
  "content" TEXT NOT NULL,
  "contentSha256" TEXT NOT NULL,
  "plainTextLength" INTEGER NOT NULL,
  "createdAt" TEXT NOT NULL,
  "updatedAt" TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS "doc_mount_blobs" (
  "id" TEXT PRIMARY KEY,
  "fileId" TEXT NOT NULL REFERENCES "doc_mount_files"("id") ON DELETE CASCADE,
  "sha256" TEXT NOT NULL,
  "sizeBytes" INTEGER NOT NULL,
  "storedMimeType" TEXT NOT NULL,
  "data" BLOB NOT NULL,
  "createdAt" TEXT NOT NULL,
  "updatedAt" TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS "doc_mount_chunks" (
  "id" TEXT PRIMARY KEY,
  "linkId" TEXT NOT NULL REFERENCES "doc_mount_file_links"("id") ON DELETE CASCADE,
  "mountPointId" TEXT NOT NULL REFERENCES "doc_mount_points"("id"),
  "chunkIndex" INTEGER NOT NULL,
  "content" TEXT NOT NULL,
  "tokenCount" INTEGER NOT NULL,
  "headingContext" TEXT,
  "embedding" BLOB,
  "createdAt" TEXT NOT NULL,
  "updatedAt" TEXT NOT NULL
);
"#
    .to_string()
}

/// A v4-shaped lock file value.
fn lock_json(
    pid: u32,
    hostname: &str,
    environment: &str,
    last_heartbeat: &str,
    history: Vec<serde_json::Value>,
) -> String {
    let v = serde_json::json!({
        "pid": pid,
        "hostname": hostname,
        "startedAt": TS,
        "lastHeartbeat": last_heartbeat,
        "environment": environment,
        "processTitle": "node",
        "processArgv0": "/usr/bin/node",
        "history": history,
    });
    format!("{}\n", serde_json::to_string_pretty(&v).unwrap())
}

fn iso_minus_secs(secs: i64) -> String {
    quilltap_core::clock::iso_from_unix_ms(quilltap_core::clock::now_unix_ms() - secs * 1000)
}

fn dead_pid() -> u32 {
    let child = std::process::Command::new("true").spawn().expect("spawn");
    let pid = child.id();
    let _ = child.wait_with_output();
    std::thread::sleep(std::time::Duration::from_millis(20));
    pid
}

// ============================================================================
// The differential
// ============================================================================

#[test]
fn cli_differential() {
    let Ok(v4_checkout) = std::env::var("QT_V4_CHECKOUT") else {
        eprintln!("skipping CLI differential: set QT_V4_CHECKOUT=/path/to/quilltap-server (and have Node 24 / QT_NODE)");
        return;
    };
    let node = std::env::var("QT_NODE").unwrap_or_else(|_| "node".to_string());
    let v4_bin = PathBuf::from(&v4_checkout).join("packages/quilltap/bin/quilltap.js");
    assert!(
        v4_bin.exists(),
        "v4 launcher not found at {}",
        v4_bin.display()
    );

    let root = tempfile::tempdir().expect("tempdir");
    let master = root.path().join("master");
    let live = root.path().join("live");
    std::fs::create_dir_all(&master).unwrap();
    build_master(&master, &live);

    let mut ctx = Ctx {
        master,
        live,
        node,
        v4_bin,
        failures: Vec::new(),
        cases_run: 0,
        _root: root,
    };
    let live = ctx.live.clone();
    let live_s = live.to_string_lossy().into_owned();
    let inst_a = format!("{live_s}/instA");
    let inst_b = format!("{live_s}/instB");

    // ---------------- help texts ----------------
    ctx.case("main help", &["--help"]);
    ctx.case("db help", &["db", "--help"]);
    ctx.case("docs help", &["docs", "--help"]);
    ctx.case("instances help", &["instances", "--help"]);

    // ---------------- launcher arg validation ----------------
    ctx.case("unknown argument", &["frobnicate"]);
    ctx.case("bad port", &["-p", "0"]);
    ctx.case("bad port nan", &["--port", "xyz"]);

    // ---------------- db: legacy flags ----------------
    let d = |rest: &[&str]| -> Vec<String> {
        let mut v = vec!["db".to_string(), "--data-dir".to_string(), inst_a.clone()];
        v.extend(rest.iter().map(|s| s.to_string()));
        v
    };
    ctx.case_with("db tables", &d(&["--tables"]), CaseOpts::default());
    ctx.case_with(
        "db tables json",
        &d(&["--tables", "--json"]),
        CaseOpts::default(),
    );
    ctx.case_with("db count", &d(&["--count", "widgets"]), CaseOpts::default());
    ctx.case_with(
        "db count json",
        &d(&["--count", "widgets", "--json"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "db count missing table",
        &d(&["--count", "nope"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "db select table",
        &d(&["SELECT id, label, n, flag, note FROM widgets ORDER BY id"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "db select json",
        &d(&[
            "--json",
            "SELECT id, label, n, flag, note FROM widgets ORDER BY id",
        ]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "db select empty",
        &d(&["SELECT * FROM widgets WHERE id = 'none'"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "db select empty json",
        &d(&["--json", "SELECT * FROM widgets WHERE id = 'none'"]),
        CaseOpts::default(),
    );
    ctx.case_with("db bad sql", &d(&["SELEC oops"]), CaseOpts::default());
    ctx.case_with(
        "db write without --write",
        &d(&["UPDATE widgets SET flag = 0 WHERE id = 'w1'"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "db write update",
        &d(&["--write", "UPDATE widgets SET flag = 0 WHERE id = 'w1'"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "db write insert json",
        &d(&[
            "--write",
            "--json",
            "INSERT INTO gears (id, teeth) VALUES ('g2', 9)",
        ]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "db llm-logs count",
        &d(&["--llm-logs", "--count", "llm_logs"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "db mount-points tables",
        &d(&["--mount-points", "--tables"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "db llm-logs mount-points conflict",
        &d(&["--llm-logs", "--mount-points", "--tables"]),
        CaseOpts::default(),
    );
    ctx.case_with("db unknown option", &d(&["--frob"]), CaseOpts::default());
    ctx.case(
        "db missing database",
        &["db", "--data-dir", "/nonexistent-qt", "--tables"],
    );
    ctx.case(
        "db data-dir and instance conflict",
        &["db", "--data-dir", "/x", "--instance", "instA", "--tables"],
    );

    // Global flags before the verb + the registry path.
    ctx.case(
        "db via instance flag",
        &["--instance", "instA", "db", "--tables"],
    );
    ctx.case("db unknown instance", &["db", "-i", "Ghost", "--tables"]);

    // Passphrase handling.
    ctx.case_with(
        "db passphrase flag",
        &[
            "db".to_string(),
            "--data-dir".to_string(),
            inst_b.clone(),
            "--passphrase".to_string(),
            PASSPHRASE_B.to_string(),
            "--tables".to_string(),
        ],
        CaseOpts::default(),
    );
    ctx.case_with(
        "db passphrase env",
        &[
            "db".to_string(),
            "--data-dir".to_string(),
            inst_b.clone(),
            "--tables".to_string(),
        ],
        CaseOpts {
            envs: vec![("QUILLTAP_DB_PASSPHRASE", PASSPHRASE_B.to_string())],
            ..Default::default()
        },
    );
    ctx.case_with(
        "db wrong passphrase",
        &[
            "db".to_string(),
            "--data-dir".to_string(),
            inst_b.clone(),
            "--passphrase".to_string(),
            "nope".to_string(),
            "--tables".to_string(),
        ],
        CaseOpts::default(),
    );
    ctx.case_with(
        "db passphrase needed no tty",
        &[
            "db".to_string(),
            "--data-dir".to_string(),
            inst_b.clone(),
            "--tables".to_string(),
        ],
        CaseOpts::default(),
    );
    // instB via the registry (stored passphrase).
    ctx.case("db instB via registry", &["db", "-i", "instB", "--tables"]);

    // The platform-default hint (fires; the DB then isn't found there).
    ctx.case("db default hint", &["db", "--tables"]);
    ctx.case_with(
        "db default hint silenced",
        &["db".to_string(), "--tables".to_string()],
        CaseOpts {
            envs: vec![("QUILLTAP_QUIET_HINTS", "1".to_string())],
            ..Default::default()
        },
    );

    // Recognized-but-unshipped verbs exit loud on the v5 side only — assert
    // v5 directly (not diffed; v4 ships them).
    {
        let opts = CaseOpts::default();
        ctx.reset_live(&opts);
        let r = ctx.run_v5(&d(&["schema"]), &opts);
        assert_eq!(r.code, 1, "db schema should exit loud");
        assert!(
            String::from_utf8_lossy(&r.stderr).contains("recognized but not yet available"),
            "loud message names the verb"
        );
    }

    // ---------------- db: lock commands ----------------
    let host = quilltap_host::lock::hostname();

    ctx.case_with(
        "lock status absent",
        &d(&["--lock-status"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "lock clean absent",
        &d(&["--lock-clean"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "lock override absent",
        &d(&["--lock-override"]),
        CaseOpts::default(),
    );

    // Corrupt lock.
    let corrupt_pre = |live: &Path| {
        std::fs::write(live.join("instA/data/quilltap.lock"), "{ not json").unwrap();
    };
    ctx.case_with(
        "lock status corrupt",
        &d(&["--lock-status"]),
        CaseOpts {
            pre: Some(Box::new(corrupt_pre)),
            ..Default::default()
        },
    );
    ctx.case_with(
        "lock clean corrupt is silent",
        &d(&["--lock-clean"]),
        CaseOpts {
            pre: Some(Box::new(corrupt_pre)),
            ..Default::default()
        },
    );

    // Stale (dead PID) with a >10-entry history.
    let dead = dead_pid();
    let stale_pre = {
        let host = host.clone();
        move |live: &Path| {
            let history: Vec<serde_json::Value> = (0..12)
                .map(|i| {
                    serde_json::json!({
                        "event": "acquired",
                        "pid": 1000 + i,
                        "hostname": host,
                        "timestamp": format!("2026-01-02T03:04:{:02}.123Z", i),
                        "detail": format!("entry {i}"),
                    })
                })
                .collect();
            std::fs::write(
                live.join("instA/data/quilltap.lock"),
                lock_json(dead, &host, "local", "2026-01-02T03:04:05.000Z", history),
            )
            .unwrap();
        }
    };
    ctx.case_with(
        "lock status stale dead pid",
        &d(&["--lock-status"]),
        CaseOpts {
            pre: Some(Box::new(stale_pre.clone())),
            normalize_heartbeat: false,
            ..Default::default()
        },
    );
    ctx.case_with(
        "lock clean stale dead pid",
        &d(&["--lock-clean"]),
        CaseOpts {
            pre: Some(Box::new(stale_pre.clone())),
            ..Default::default()
        },
    );
    ctx.case_with(
        "lock override stale dead pid",
        &d(&["--lock-override"]),
        CaseOpts {
            pre: Some(Box::new(stale_pre.clone())),
            ..Default::default()
        },
    );

    // Live Quilltap-shaped process (node sleeper) → ACTIVE.
    let mut sleeper_node = Command::new(&ctx.node)
        .args(["-e", "setTimeout(() => {}, 60000)"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn node sleeper");
    let node_pid = sleeper_node.id();
    let active_pre = {
        let host = host.clone();
        move |live: &Path| {
            // Heartbeat ~10 minutes old → the stable "10m ago" display plus
            // the may-be-hung amber suffix.
            std::fs::write(
                live.join("instA/data/quilltap.lock"),
                lock_json(node_pid, &host, "local", &iso_minus_secs(600), vec![]),
            )
            .unwrap();
        }
    };
    ctx.case_with(
        "lock status active",
        &d(&["--lock-status"]),
        CaseOpts {
            pre: Some(Box::new(active_pre.clone())),
            ..Default::default()
        },
    );
    ctx.case_with(
        "lock clean refuses live",
        &d(&["--lock-clean"]),
        CaseOpts {
            pre: Some(Box::new(active_pre.clone())),
            ..Default::default()
        },
    );
    ctx.case_with(
        "lock override live warns",
        &d(&["--lock-override"]),
        CaseOpts {
            pre: Some(Box::new(active_pre.clone())),
            ..Default::default()
        },
    );
    ctx.case_with(
        "db write refused on live lock",
        &d(&["--write", "UPDATE widgets SET flag = 1 WHERE id = 'w1'"]),
        CaseOpts {
            pre: Some(Box::new(active_pre.clone())),
            ..Default::default()
        },
    );

    // Suspect (live non-Quilltap process).
    let mut sleeper_plain = Command::new("sleep")
        .arg("600")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn sleep");
    let sleep_pid = sleeper_plain.id();
    let suspect_pre = {
        let host = host.clone();
        move |live: &Path| {
            std::fs::write(
                live.join("instA/data/quilltap.lock"),
                lock_json(sleep_pid, &host, "local", &iso_minus_secs(60), vec![]),
            )
            .unwrap();
        }
    };
    ctx.case_with(
        "lock status suspect",
        &d(&["--lock-status"]),
        CaseOpts {
            pre: Some(Box::new(suspect_pre.clone())),
            ..Default::default()
        },
    );
    ctx.case_with(
        "lock clean suspect removes",
        &d(&["--lock-clean"]),
        CaseOpts {
            pre: Some(Box::new(suspect_pre.clone())),
            ..Default::default()
        },
    );
    ctx.case_with(
        "lock override suspect rejects",
        &d(&["--lock-override"]),
        CaseOpts {
            pre: Some(Box::new(suspect_pre.clone())),
            ..Default::default()
        },
    );

    // Different-host docker lock — fresh heartbeat (seconds display is the one
    // documented normalization) and stale.
    let docker_fresh_pre = |live: &Path| {
        std::fs::write(
            live.join("instA/data/quilltap.lock"),
            lock_json(
                4242,
                "elsewhere-host",
                "docker",
                &iso_minus_secs(60),
                vec![],
            ),
        )
        .unwrap();
    };
    ctx.case_with(
        "lock status docker fresh",
        &d(&["--lock-status"]),
        CaseOpts {
            pre: Some(Box::new(docker_fresh_pre)),
            normalize_heartbeat: true,
            ..Default::default()
        },
    );
    ctx.case_with(
        "lock clean docker fresh refuses",
        &d(&["--lock-clean"]),
        CaseOpts {
            pre: Some(Box::new(docker_fresh_pre)),
            normalize_heartbeat: true,
            ..Default::default()
        },
    );
    let docker_stale_pre = |live: &Path| {
        std::fs::write(
            live.join("instA/data/quilltap.lock"),
            lock_json(
                4242,
                "elsewhere-host",
                "docker",
                "2020-01-01T00:00:00.000Z",
                vec![],
            ),
        )
        .unwrap();
    };
    ctx.case_with(
        "lock status docker stale",
        &d(&["--lock-status"]),
        CaseOpts {
            pre: Some(Box::new(docker_stale_pre)),
            ..Default::default()
        },
    );
    let foreign_local_pre = |live: &Path| {
        std::fs::write(
            live.join("instA/data/quilltap.lock"),
            lock_json(4242, "elsewhere-host", "local", &iso_minus_secs(60), vec![]),
        )
        .unwrap();
    };
    ctx.case_with(
        "lock status foreign local",
        &d(&["--lock-status"]),
        CaseOpts {
            pre: Some(Box::new(foreign_local_pre)),
            ..Default::default()
        },
    );
    // --write claims a stale (dead-PID) lock, runs, and releases it.
    {
        let opts = CaseOpts {
            pre: Some(Box::new(stale_pre.clone())),
            ..Default::default()
        };
        let args = d(&["--write", "UPDATE widgets SET flag = 1 WHERE id = 'w1'"]);
        ctx.case_with("db write claims stale lock", &args, opts);
        // After the v5 run (live holds v5's post-state) the lock is released.
        assert!(
            !ctx.live.join("instA/data/quilltap.lock").exists(),
            "write lock released after one-shot --write"
        );
    }

    // ---------------- db characters (P4.D66) ----------------
    {
        let scratch = ctx.live.parent().unwrap().join("cwd");
        let out_path = format!("{live_s}/exported.qtap");
        let base = |rest: &[&str]| -> Vec<String> {
            let mut v = vec!["db".to_string(), "--data-dir".to_string(), inst_a.clone()];
            v.extend(rest.iter().map(|s| s.to_string()));
            v
        };

        // -- status
        ctx.case_with(
            "characters default sub",
            &base(&["characters"]),
            CaseOpts::default(),
        );
        ctx.case_with(
            "characters status",
            &base(&["characters", "status"]),
            CaseOpts::default(),
        );
        ctx.case_with(
            "characters status json",
            &base(&["characters", "status", "--json"]),
            CaseOpts::default(),
        );
        ctx.case_with(
            "characters status limit",
            &base(&["characters", "status", "--limit", "3"]),
            CaseOpts::default(),
        );
        ctx.case_with(
            "characters status diverged",
            &base(&["characters", "status", "--diverged"]),
            CaseOpts::default(),
        );
        ctx.case_with(
            "characters status diverged json",
            &base(&["characters", "status", "--diverged", "--json"]),
            CaseOpts::default(),
        );
        ctx.case_with(
            "characters status blocked",
            &base(&["characters", "status", "--blocked"]),
            CaseOpts::default(),
        );
        ctx.case_with(
            "characters status by name",
            &base(&["characters", "status", "--id", "Elowen"]),
            CaseOpts::default(),
        );
        // The vault-alias fallback: 'Ellie' lives only in properties.json.
        ctx.case_with(
            "characters status by alias",
            &base(&["characters", "status", "--id", "Ellie"]),
            CaseOpts::default(),
        );
        ctx.case_with(
            "characters status by uuid",
            &base(&["characters", "status", "--id", CH_ROWAN]),
            CaseOpts::default(),
        );
        ctx.case_with(
            "characters status by unknown uuid",
            &base(&[
                "characters",
                "status",
                "--id",
                "99999999-9999-4999-8999-999999999999",
            ]),
            CaseOpts::default(),
        );
        ctx.case_with(
            "characters status no match",
            &base(&["characters", "status", "--id", "Nobody"]),
            CaseOpts::default(),
        );
        // Two rows share the name — v4's `ambiguous` error exits 2.
        ctx.case_with(
            "characters status ambiguous",
            &base(&["characters", "status", "--id", "Twin"]),
            CaseOpts::default(),
        );
        // v4's parseSubArgs swallows the subcommand as `--json`'s VALUE, so
        // this runs the DEFAULT sub with JSON off.
        ctx.case_with(
            "characters flag before sub",
            &base(&["characters", "--json", "status"]),
            CaseOpts::default(),
        );
        // A bare `--id` becomes the string 'true'.
        ctx.case_with(
            "characters status bare id flag",
            &base(&["characters", "status", "--id"]),
            CaseOpts::default(),
        );

        // The 4.6-cutover shape: the content columns actually dropped. v4
        // keeps working — no `flag` column, vault-only counts, and no
        // divergence report at all.
        let post_cutover_pre = |live: &Path| {
            let w = Writer::open_writable(&live.join("instA/data/quilltap.db"), PEPPER).unwrap();
            w.connection()
                .execute_batch(
                    "ALTER TABLE characters DROP COLUMN identity;
                     ALTER TABLE characters DROP COLUMN description;
                     ALTER TABLE characters DROP COLUMN systemPrompts;",
                )
                .unwrap();
        };
        ctx.case_with(
            "characters status post-cutover",
            &base(&["characters", "status"]),
            CaseOpts {
                pre: Some(Box::new(post_cutover_pre)),
                ..Default::default()
            },
        );
        ctx.case_with(
            "characters status post-cutover json",
            &base(&["characters", "status", "--json"]),
            CaseOpts {
                pre: Some(Box::new(post_cutover_pre)),
                ..Default::default()
            },
        );

        // -- archives
        ctx.case_with(
            "characters archives",
            &base(&["characters", "archives"]),
            CaseOpts::default(),
        );
        ctx.case_with(
            "characters archives json",
            &base(&["characters", "archives", "--json"]),
            CaseOpts::default(),
        );
        // A database that predates archiving entirely (the `PRAGMA
        // table_info` tolerance the order pins).
        let pre_archive_pre = |live: &Path| {
            let w = Writer::open_writable(&live.join("instA/data/quilltap.db"), PEPPER).unwrap();
            w.connection()
                .execute_batch(
                    "ALTER TABLE characters DROP COLUMN archivedAt;
                     ALTER TABLE characters DROP COLUMN archiveFileId;
                     ALTER TABLE characters DROP COLUMN archivedAvatarFileId;",
                )
                .unwrap();
        };
        ctx.case_with(
            "characters archives pre-archive schema",
            &base(&["characters", "archives"]),
            CaseOpts {
                pre: Some(Box::new(pre_archive_pre)),
                ..Default::default()
            },
        );
        let empty_shelf_pre = |live: &Path| {
            let w = Writer::open_writable(&live.join("instA/data/quilltap.db"), PEPPER).unwrap();
            w.connection()
                .execute_batch(
                    "UPDATE characters SET archivedAt = NULL, archiveFileId = NULL;
                     DELETE FROM files WHERE category = 'ARCHIVE';",
                )
                .unwrap();
        };
        ctx.case_with(
            "characters archives empty shelf",
            &base(&["characters", "archives"]),
            CaseOpts {
                pre: Some(Box::new(empty_shelf_pre)),
                ..Default::default()
            },
        );

        // -- dispatch
        ctx.case_with(
            "characters unknown sub",
            &base(&["characters", "frobnicate"]),
            CaseOpts::default(),
        );
        // The bare token qualifies the verb path, but `runVerb` reads args[0].
        ctx.case_with(
            "db flag before verb",
            &base(&["--json", "characters"]),
            CaseOpts::default(),
        );

        // -- archive / rehydrate (usage + guard, no server needed)
        ctx.case_with(
            "characters archive usage",
            &base(&["characters", "archive"]),
            CaseOpts::default(),
        );
        ctx.case_with(
            "characters archive needs write",
            &base(&["characters", "archive", "Elowen"]),
            CaseOpts::default(),
        );
        ctx.case_with(
            "characters rehydrate usage",
            &base(&["characters", "rehydrate"]),
            CaseOpts::default(),
        );
        ctx.case_with(
            "characters rehydrate needs write",
            &base(&["characters", "rehydrate", "Sable"]),
            CaseOpts::default(),
        );
        // Resolution runs before the request: ambiguity still exits 2.
        ctx.case_with(
            "characters archive ambiguous",
            &base(&["characters", "archive", "Twin", "--write"]),
            CaseOpts::default(),
        );

        // -- archive / rehydrate / export against a canned server. Both CLIs
        //    POST the SAME v4 URLs, so one stub answers both and the whole
        //    request+print path is diffed without a live Quilltap.
        let dead_port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let p = l.local_addr().unwrap().port();
            drop(l);
            p
        };
        let reach = || CaseOpts {
            normalize_reach: true,
            ..Default::default()
        };
        ctx.case_with(
            "characters archive unreachable",
            &base(&[
                "characters",
                "archive",
                "Elowen",
                "--write",
                "--port",
                &dead_port.to_string(),
            ]),
            reach(),
        );

        let ok_port = spawn_canned_stub(
            200,
            r#"{"archived":true,"archiveFileId":"fa000000-0000-4000-8000-000000000001","pruneComplete":true}"#,
        );
        ctx.case_with(
            "characters archive ok",
            &base(&[
                "characters",
                "archive",
                "Elowen",
                "--write",
                "--port",
                &ok_port.to_string(),
            ]),
            CaseOpts::default(),
        );
        ctx.case_with(
            "characters archive ok json",
            &base(&[
                "characters",
                "archive",
                "Elowen",
                "--write",
                "--json",
                "--port",
                &ok_port.to_string(),
            ]),
            CaseOpts::default(),
        );
        let partial_port = spawn_canned_stub(
            200,
            r#"{"archived":true,"archiveFileId":null,"pruneComplete":false}"#,
        );
        ctx.case_with(
            "characters archive prune incomplete",
            &base(&[
                "characters",
                "archive",
                "Elowen",
                "--write",
                "--port",
                &partial_port.to_string(),
            ]),
            CaseOpts::default(),
        );
        let rehydrate_port = spawn_canned_stub(
            200,
            r#"{"rehydrated":true,"archived":false,"archiveBundleFileId":"fa000000-0000-4000-8000-000000000001","restored":{"memories":42,"documents":7,"blobs":3},"warnings":["One photograph could not be re-linked.","The mail is short two letters."]}"#,
        );
        ctx.case_with(
            "characters rehydrate ok",
            &base(&[
                "characters",
                "rehydrate",
                "Sable",
                "--write",
                "--port",
                &rehydrate_port.to_string(),
            ]),
            CaseOpts::default(),
        );
        let bare_rehydrate_port = spawn_canned_stub(
            200,
            r#"{"rehydrated":true,"archived":false,"archiveBundleFileId":null,"warnings":[]}"#,
        );
        ctx.case_with(
            "characters rehydrate without restored counts",
            &base(&[
                "characters",
                "rehydrate",
                "Sable",
                "--write",
                "--port",
                &bare_rehydrate_port.to_string(),
            ]),
            CaseOpts::default(),
        );
        let err_port = spawn_canned_stub(
            400,
            r#"{"error":"The archive cannot be sealed: your passphrase has not been entered since Quilltap started. Unlock once (or restart and unlock), then archive again."}"#,
        );
        ctx.case_with(
            "characters archive server error",
            &base(&[
                "characters",
                "archive",
                "Elowen",
                "--write",
                "--port",
                &err_port.to_string(),
            ]),
            CaseOpts::default(),
        );
        let bare_err_port = spawn_canned_stub(503, "not json at all");
        ctx.case_with(
            "characters archive non-json error",
            &base(&[
                "characters",
                "archive",
                "Elowen",
                "--write",
                "--port",
                &bare_err_port.to_string(),
            ]),
            CaseOpts::default(),
        );

        // -- export
        ctx.case_with(
            "characters export usage",
            &base(&["characters", "export"]),
            CaseOpts::default(),
        );
        let export_case = |name: &str, who: &str| -> Vec<String> {
            let _ = name;
            base(&["characters", "export", who, "--out", &out_path])
        };
        ctx.case_with(
            "characters export archived",
            &export_case("archived", "Sable"),
            CaseOpts::default(),
        );
        ctx.case_with(
            "characters export plaintext bundle",
            &export_case("plaintext", "Umber"),
            CaseOpts::default(),
        );
        // Encrypted under a passphrase this process has not seen: the
        // internal sentinel fails, the env var is unset, and the prompt has
        // no TTY.
        ctx.case_with(
            "characters export wrong passphrase",
            &export_case("wrong", "Vesper"),
            CaseOpts::default(),
        );
        ctx.case_with(
            "characters export old passphrase from env",
            &export_case("env", "Vesper"),
            CaseOpts {
                envs: vec![("QUILLTAP_DB_PASSPHRASE", OLD_PASSPHRASE.to_string())],
                ..Default::default()
            },
        );
        ctx.case_with(
            "characters export tombstone",
            &export_case("tombstone", "Tobias"),
            CaseOpts::default(),
        );
        ctx.case_with(
            "characters export missing file row",
            &export_case("missing row", "Wren"),
            CaseOpts::default(),
        );
        ctx.case_with(
            "characters export no storage key",
            &export_case("no key", "Yarrow"),
            CaseOpts::default(),
        );
        ctx.case_with(
            "characters export bytes absent",
            &export_case("absent", "Zephyr"),
            CaseOpts::default(),
        );
        ctx.case_with(
            "characters export truncated bundle",
            &export_case("truncated", "Thorn"),
            CaseOpts::default(),
        );
        ctx.case_with(
            "characters export corrupt bundle",
            &export_case("corrupt", "Corvid"),
            CaseOpts::default(),
        );
        ctx.case_with(
            "characters export live unreachable",
            &base(&[
                "characters",
                "export",
                "Elowen",
                "--out",
                &out_path,
                "--port",
                &dead_port.to_string(),
            ]),
            reach(),
        );
        // A live character on a pre-archive database takes the same server
        // leg (the `archivedAt` probe simply never runs).
        ctx.case_with(
            "characters export pre-archive schema",
            &base(&[
                "characters",
                "export",
                "Sable",
                "--out",
                &out_path,
                "--port",
                &dead_port.to_string(),
            ]),
            CaseOpts {
                pre: Some(Box::new(pre_archive_pre)),
                normalize_reach: true,
                ..Default::default()
            },
        );
        let export_port = spawn_canned_stub(
            200,
            "{\"type\":\"character\",\"name\":\"Elowen\"}\n{\"type\":\"memory\"}\n",
        );
        // No `--out`: the path is resolved against the cwd from the
        // character's name, so this one runs in a scratch directory.
        ctx.case_with(
            "characters export live default out",
            &base(&[
                "characters",
                "export",
                "Elowen",
                "--port",
                &export_port.to_string(),
            ]),
            CaseOpts {
                cwd: Some(scratch.clone()),
                ..Default::default()
            },
        );
        // -- Coverage guard. A green byte-diff proves the two CLIs agree; it
        //    does NOT prove the fixture reaches the branches. Assert the
        //    SHAPE of what the fixture produces (never hand-counted totals),
        //    so a future fixture edit that quietly collapses the report to one
        //    issue class fails here instead of passing silently.
        {
            let opts = CaseOpts::default();
            ctx.reset_live(&opts);
            let r = ctx.run_v5(&base(&["characters", "status", "--json"]), &opts);
            let v: serde_json::Value =
                serde_json::from_slice(&r.stdout).expect("characters status --json parses");
            let rows = v["characters"].as_array().expect("characters array");
            let issues: Vec<&str> = rows
                .iter()
                .map(|c| c["issue"].as_str().unwrap_or(""))
                .collect();
            for want in [
                "ok (db matches vault)",
                "no vault",
                "vault empty",
                "physical files incomplete (1 of 2)",
            ] {
                assert!(
                    issues.contains(&want),
                    "fixture must exercise the '{want}' arm: {issues:?}"
                );
            }
            assert!(
                issues.iter().any(|i| i.ends_with(" files missing")),
                "fixture must exercise the missing-files arm: {issues:?}"
            );
            assert!(
                issues.iter().any(|i| i.starts_with("diverged (")),
                "fixture must exercise the divergence arm: {issues:?}"
            );
            // The 4.6 cutover abandoned the content columns without dropping
            // them, so the divergence report is the LIVE path on a modern
            // instance. If this ever flips, the post-cutover pre-hook cases
            // are testing the only path there is.
            assert!(
                rows.iter().all(|c| c["preCutover"] == true),
                "the stock fixture is the pre-cutover (columns present) shape"
            );
            let counts = &v["counts"];
            for key in [
                "ok",
                "diverged",
                "missingFiles",
                "noVault",
                "empty",
                "physIncomplete",
            ] {
                assert!(
                    counts[key].as_u64().unwrap_or(0) > 0,
                    "summary counter '{key}' must be exercised: {counts}"
                );
            }

            let r = ctx.run_v5(&base(&["characters", "archives", "--json"]), &opts);
            let v: serde_json::Value =
                serde_json::from_slice(&r.stdout).expect("characters archives --json parses");
            assert!(
                v["looseBundles"].as_array().is_some_and(|b| !b.is_empty()),
                "fixture must carry a loose bundle"
            );
            assert!(
                v["archivedCharacters"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|c| c["archiveFileId"].is_null())),
                "fixture must carry a pre-bundle tombstone"
            );
            assert!(
                v["bundles"]
                    .as_array()
                    .is_some_and(|b| b.len() > v["looseBundles"].as_array().unwrap().len()),
                "fixture must carry held bundles as well as loose ones"
            );
            ctx.cases_run += 1;
        }

        let export_err_port =
            spawn_canned_stub(500, r#"{"error":"Export failed inside the pipeline."}"#);
        ctx.case_with(
            "characters export live server error",
            &base(&[
                "characters",
                "export",
                "Elowen",
                "--out",
                &out_path,
                "--port",
                &export_err_port.to_string(),
            ]),
            CaseOpts::default(),
        );
    }

    // Every stub-backed case above ran v4 then v5 against the same port —
    // prove the two sides put the same URL + body on the wire.
    assert_canned_wire_parity();

    // ---------------- docs ----------------
    let dd = |rest: &[&str]| -> Vec<String> {
        let mut v = vec!["docs".to_string(), "--data-dir".to_string(), inst_a.clone()];
        v.extend(rest.iter().map(|s| s.to_string()));
        v
    };
    ctx.case_with("docs list", &dd(&["list"]), CaseOpts::default());
    ctx.case_with(
        "docs list json",
        &dd(&["list", "--json"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "docs list names-only",
        &dd(&["list", "--names-only"]),
        CaseOpts::default(),
    );
    ctx.case_with("docs show", &dd(&["show", "notes"]), CaseOpts::default());
    ctx.case_with(
        "docs show json",
        &dd(&["show", "notes", "--json"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "docs show stale counts",
        &dd(&["show", "archive"]),
        CaseOpts::default(),
    );
    ctx.case_with("docs show by uuid", &dd(&["show", N1]), CaseOpts::default());
    ctx.case_with(
        "docs show missing",
        &dd(&["show", "nope"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "docs show ambiguous",
        &dd(&["show", "twin"]),
        CaseOpts::default(),
    );
    ctx.case_with("docs ls root", &dd(&["ls", "notes"]), CaseOpts::default());
    ctx.case_with(
        "docs ls root json",
        &dd(&["ls", "notes", "--json"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "docs ls folder",
        &dd(&["ls", "notes", "Knowledge"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "docs ls single file",
        &dd(&["ls", "notes", "Knowledge/facts.md"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "docs ls links",
        &dd(&["ls", "notes", "--links"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "docs ls recursive",
        &dd(&["ls", "notes", "-R"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "docs ls sort size",
        &dd(&["ls", "notes", "--sort", "size"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "docs ls sort time reverse",
        &dd(&["ls", "notes", "--sort", "time", "-r"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "docs ls missing path",
        &dd(&["ls", "notes", "nosuch"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "docs dir alias",
        &dd(&["dir", "notes"]),
        CaseOpts::default(),
    );
    ctx.case_with("docs tree", &dd(&["tree", "notes"]), CaseOpts::default());
    ctx.case_with(
        "docs tree json",
        &dd(&["tree", "notes", "--json"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "docs tree subfolder",
        &dd(&["tree", "notes", "Knowledge"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "docs tree of file errors",
        &dd(&["tree", "notes", "today.md"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "docs tree max nodes",
        &dd(&["tree", "notes", "--max-nodes", "2"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "docs read text",
        &dd(&["read", "notes", "today.md"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "docs read blob",
        &dd(&["read", "notes", "pic.png"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "docs read rendered blob",
        &dd(&["read", "--rendered", "notes", "pic.png"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "docs read rendered chunks",
        &dd(&["read", "--rendered", "notes", "Knowledge/facts.md"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "docs read filesystem",
        &dd(&["read", "attic", "attic-note.md"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "docs read missing",
        &dd(&["read", "notes", "missing.md"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "docs read qtap uri",
        &dd(&["read", "qtap://notes/today.md"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "docs read qtap project rejected",
        &dd(&["read", "qtap://project/x.md"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "docs read qtap self rejected",
        &dd(&["read", "qtap://self/x.md"]),
        CaseOpts::default(),
    );
    ctx.case_with(
        "docs read qtap bad encoding",
        &dd(&["read", "qtap://notes/bad%zz.md"]),
        CaseOpts::default(),
    );
    ctx.case_with("docs unknown verb", &dd(&["frob"]), CaseOpts::default());
    ctx.case_with("docs ls usage", &dd(&["ls"]), CaseOpts::default());
    ctx.case_with("docs no args", &["docs".to_string()], CaseOpts::default());

    // [40319484] The un-migrated instance: `linkGroupId` absent. Both launchers
    // must PROBE for it and degrade the links column to 1 rather than failing
    // the whole listing with `no such column`.
    let no_link_group_column_pre = |live: &Path| {
        let path = live.join("instA/data/quilltap-mount-index.db");
        let w = Writer::open_writable(&path, PEPPER).unwrap();
        w.connection()
            .execute_batch(
                "DROP INDEX IF EXISTS \"idx_doc_mount_file_links_linkGroupId\";\
                 ALTER TABLE \"doc_mount_file_links\" DROP COLUMN \"linkGroupId\";",
            )
            .unwrap();
    };
    ctx.case_with(
        "docs ls links un-migrated",
        &dd(&["ls", "notes", "--links"]),
        CaseOpts {
            pre: Some(Box::new(no_link_group_column_pre)),
            ..Default::default()
        },
    );

    // The pre-link-table schema refusal (drop the table via surgery).
    let old_schema_pre = |live: &Path| {
        let path = live.join("instA/data/quilltap-mount-index.db");
        std::fs::remove_file(&path).unwrap();
        let w = Writer::open_writable(&path, PEPPER).unwrap();
        w.connection()
            .execute_batch("CREATE TABLE doc_mount_points (id TEXT PRIMARY KEY);")
            .unwrap();
    };
    ctx.case_with(
        "docs schema guard",
        &dd(&["list"]),
        CaseOpts {
            pre: Some(Box::new(old_schema_pre)),
            ..Default::default()
        },
    );

    // ---------------- instances (each case on a fresh registry state) ----------------
    ctx.case("instances list", &["instances"]);
    ctx.case("instances list verb", &["instances", "list"]);
    ctx.case("instances list json", &["instances", "list", "--json"]);
    ctx.case(
        "instances list names-only",
        &["instances", "list", "--names-only"],
    );
    ctx.case("instances path", &["instances", "path"]);
    ctx.case("instances show", &["instances", "show", "instA"]);
    ctx.case(
        "instances show case-insensitive",
        &["instances", "show", "INSTB"],
    );
    ctx.case("instances show unknown", &["instances", "show", "Ghost"]);
    ctx.case("instances show usage", &["instances", "show"]);
    ctx.case("instances unknown verb", &["instances", "frob"]);
    ctx.case_with(
        "instances add existing path",
        &[
            "instances".to_string(),
            "add".to_string(),
            "Fresh".to_string(),
            inst_a.clone(),
        ],
        CaseOpts {
            stdin: Some(b"n\n".to_vec()),
            ..Default::default()
        },
    );
    ctx.case_with(
        "instances add missing path declined",
        &[
            "instances".to_string(),
            "add".to_string(),
            "Ghostly".to_string(),
            "/no/such/path".to_string(),
        ],
        CaseOpts {
            stdin: Some(b"n\n".to_vec()),
            ..Default::default()
        },
    );
    // DOCUMENTED DIVERGENCE (not diffed): two successive prompts fed from one
    // pipe. v4's first `readline` interface slurps the whole piped buffer and
    // discards the unread remainder on close, so the second prompt never gets
    // its answer — v4 prints the prompt and exits 0 WITHOUT saving. The v5
    // port reads stdin line-by-line (each prompt consumes exactly one line),
    // so the same script completes the add. Assert the v5 behavior directly.
    {
        let opts = CaseOpts {
            stdin: Some(b"y\nn\n".to_vec()),
            ..Default::default()
        };
        ctx.reset_live(&opts);
        let r = ctx.run_v5(
            &[
                "instances".to_string(),
                "add".to_string(),
                "Ghostly".to_string(),
                "/no/such/path".to_string(),
            ],
            &opts,
        );
        assert_eq!(r.code, 0, "v5 two-prompt add completes from a pipe");
        assert!(
            String::from_utf8_lossy(&r.stdout).contains("Saved instance \"Ghostly\""),
            "v5 saves the instance when both piped answers are supplied"
        );
    }
    ctx.case_with(
        "instances add passphrase needs tty",
        &[
            "instances".to_string(),
            "add".to_string(),
            "Fresh".to_string(),
            inst_a.clone(),
        ],
        CaseOpts {
            stdin: Some(b"y\n".to_vec()),
            ..Default::default()
        },
    );
    ctx.case("instances remove", &["instances", "remove", "insta"]);
    ctx.case(
        "instances remove unknown",
        &["instances", "remove", "Ghost"],
    );
    ctx.case("instances default show none", &["instances", "default"]);
    ctx.case("instances default set", &["instances", "default", "instA"]);
    ctx.case(
        "instances default set unknown",
        &["instances", "default", "Ghost"],
    );
    ctx.case(
        "instances default clear",
        &["instances", "default", "--clear"],
    );
    ctx.case(
        "instances rename",
        &["instances", "rename", "instA", "Friday"],
    );
    ctx.case(
        "instances rename collision",
        &["instances", "rename", "instA", "instb"],
    );
    ctx.case("instances rename usage", &["instances", "rename", "instA"]);

    // ---------------- completion ----------------
    ctx.case("completion no args", &["completion"]);
    ctx.case("completion help", &["completion", "--help"]);
    ctx.case("completion bash", &["completion", "bash"]);
    ctx.case("completion zsh", &["completion", "zsh"]);
    ctx.case("completion fish", &["completion", "fish"]);
    ctx.case("completion unknown shell", &["completion", "tcsh"]);

    // ---------------- recall-replay (P4.d13) ----------------
    ctx.case("recall-replay help", &["recall-replay", "--help"]);
    ctx.case("recall-replay no chat", &["recall-replay"]);
    ctx.case(
        "recall-replay two ids",
        &["recall-replay", "cid-a", "cid-b"],
    );
    ctx.case(
        "recall-replay bad turn",
        &["recall-replay", "cid", "--turn", "0"],
    );
    ctx.case(
        "recall-replay bad turn nan",
        &["recall-replay", "cid", "--turn", "xyz"],
    );
    ctx.case(
        "recall-replay bad limit",
        &["recall-replay", "cid", "--limit", "101"],
    );
    ctx.case(
        "recall-replay bad port",
        &["recall-replay", "cid", "--port", "0"],
    );
    ctx.case(
        "recall-replay unknown option",
        &["recall-replay", "cid", "--frob"],
    );
    {
        // The canned render arms: both CLIs hit the stub (each on its own
        // dialect) and render the SAME payload — the table geometry, ANSI
        // codes, JS number formatting, and --json pretty print byte-diff.
        let port = spawn_recall_stub(RECALL_CANNED_RESULT, false);
        let recall_opts = || CaseOpts {
            normalize_recall: true,
            ..Default::default()
        };
        ctx.case_with(
            "recall-replay table",
            &[
                "recall-replay".to_string(),
                "cd000000-0000-4000-8000-000000000001".to_string(),
                "--port".to_string(),
                port.to_string(),
            ],
            recall_opts(),
        );
        ctx.case_with(
            "recall-replay json",
            &[
                "recall-replay".to_string(),
                "cd000000-0000-4000-8000-000000000001".to_string(),
                "--port".to_string(),
                port.to_string(),
                "--json".to_string(),
            ],
            recall_opts(),
        );
        let err_port = spawn_recall_stub(RECALL_CANNED_RESULT, true);
        ctx.case_with(
            "recall-replay server error",
            &[
                "recall-replay".to_string(),
                "cd000000-0000-4000-8000-000000000001".to_string(),
                "--port".to_string(),
                err_port.to_string(),
            ],
            recall_opts(),
        );
        // A closed port: the connect-error arm (reason text normalized).
        let dead = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let dead_port = dead.local_addr().unwrap().port();
        drop(dead);
        ctx.case_with(
            "recall-replay unreachable",
            &[
                "recall-replay".to_string(),
                "cid".to_string(),
                "--port".to_string(),
                dead_port.to_string(),
            ],
            recall_opts(),
        );
    }

    // Registry round-trip: the same mutation sequence on each side must leave
    // byte-identical registry files.
    {
        let seq: Vec<(Vec<String>, Option<Vec<u8>>)> = vec![
            (
                vec![
                    "instances".to_string(),
                    "add".to_string(),
                    "Fresh".to_string(),
                    inst_a.clone(),
                ],
                Some(b"n\n".to_vec()),
            ),
            (
                vec![
                    "instances".to_string(),
                    "default".to_string(),
                    "fresh".to_string(),
                ],
                None,
            ),
            (
                vec![
                    "instances".to_string(),
                    "rename".to_string(),
                    "Fresh".to_string(),
                    "Brisk".to_string(),
                ],
                None,
            ),
            (
                vec![
                    "instances".to_string(),
                    "remove".to_string(),
                    "instb".to_string(),
                ],
                None,
            ),
            (vec!["instances".to_string(), "list".to_string()], None),
        ];
        #[cfg(target_os = "macos")]
        let reg_rel = "home/Library/Application Support/Quilltap/instances.json";
        #[cfg(not(target_os = "macos"))]
        let reg_rel = "home/.quilltap/instances.json";

        let run_seq = |ctx: &Ctx, v5: bool| -> (Vec<RunOut>, String) {
            ctx.reset_live(&CaseOpts::default());
            let mut outs = Vec::new();
            for (args, stdin) in &seq {
                let opts = CaseOpts {
                    stdin: stdin.clone(),
                    ..Default::default()
                };
                outs.push(if v5 {
                    ctx.run_v5(args, &opts)
                } else {
                    ctx.run_v4(args, &opts)
                });
            }
            let registry = std::fs::read_to_string(ctx.live.join(reg_rel)).unwrap();
            (outs, registry)
        };
        let (v4_outs, v4_reg) = run_seq(&ctx, false);
        let (v5_outs, v5_reg) = run_seq(&ctx, true);
        ctx.cases_run += 1;
        for (i, (a, b)) in v4_outs.iter().zip(&v5_outs).enumerate() {
            ctx.compare(
                &format!("instances sequence step {i}"),
                a,
                b,
                &CaseOpts::default(),
            );
        }
        if v4_reg != v5_reg {
            ctx.failures.push(format!(
                "[instances sequence] final registry differs\n--- v4 ---\n{v4_reg}\n--- v5 ---\n{v5_reg}"
            ));
        }
    }

    // verifyPassphrase's four outcomes — v4's real lib driven via `node -e`
    // vs the ported host function.
    {
        ctx.cases_run += 1;
        ctx.reset_live(&CaseOpts::default());
        let lib = PathBuf::from(std::env::var("QT_V4_CHECKOUT").unwrap())
            .join("packages/quilltap/lib/instances.js");
        let check = |root: &str, pass: &str| -> String {
            let out = Command::new(&ctx.node)
                .arg("-e")
                .arg("require(process.argv[1]).verifyPassphrase(process.argv[2], process.argv[3]).then(r => console.log(r))")
                .arg(&lib)
                .arg(root)
                .arg(pass)
                .env("HOME", ctx.live.join("home"))
                .output()
                .expect("node -e verifyPassphrase");
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        let cases = [
            (inst_b.as_str(), PASSPHRASE_B, "valid"),
            (inst_b.as_str(), "nope", "wrong"),
            (inst_a.as_str(), "anything", "no-encryption"),
            ("/no/such/root", "x", "no-dbkey"),
        ];
        for (root, pass, expected) in cases {
            let v4 = check(root, pass);
            let v5 = quilltap_host::instances::verify_passphrase(root, pass)
                .map(|r| r.as_str().to_string())
                .unwrap_or_else(|e| format!("ERR {e}"));
            if v4 != v5 || v4 != expected {
                ctx.failures.push(format!(
                    "[verifyPassphrase {root} {pass}] v4={v4} v5={v5} expected={expected}"
                ));
            }
        }
    }

    // ---------------- wrap up ----------------
    let _ = sleeper_node.kill();
    let _ = sleeper_node.wait();
    let _ = sleeper_plain.kill();
    let _ = sleeper_plain.wait();

    eprintln!(
        "CLI differential: {} cases, {} failures",
        ctx.cases_run,
        ctx.failures.len()
    );
    if !ctx.failures.is_empty() {
        panic!(
            "CLI differential failures ({}):\n\n{}",
            ctx.failures.len(),
            ctx.failures.join("\n\n============================\n\n")
        );
    }
}
