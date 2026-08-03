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
