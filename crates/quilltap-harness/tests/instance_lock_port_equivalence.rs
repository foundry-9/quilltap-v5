//! P4.D299 tier-2 differential — v4 bug 239 (`93c5af6ad`): the shared
//! instance lock's optional `port`. `quilltap_host::lock::acquire_instance_lock`
//! / `heartbeat_tick` against v4's REAL `acquireInstanceLock` and the
//! heartbeat's own interval callback
//! (`harness/oracle/cases/instance-lock-port.ts`).
//!
//! Each row rebuilds its plant from the recorded template — `"__PID__"` →
//! a PID this test just watched exit (`dead`) or this process's own (`self`),
//! `__HOST__` → `quilltap_host::lock::hostname()` — into a fresh lock path,
//! then runs the row's actions with v4's own `resolveListeningPort()` under the
//! row's `PORT` (`listeningPort`, `null` → `None`) as the listening port:
//!   - `acquire`   → `acquire_instance_lock(path, listening_port)`;
//!   - `heartbeat` → `heartbeat_tick(path)` (must answer `true` — a lock this
//!     process still owns is never "lost", whatever its `port` holds).
//!
//! Compared per action, as BYTES (memory note `serde-json-map-eq-ignores-order`
//! — `Value ==` is blind to key order, which is this family's whole contract):
//!   - the lock file masked exactly as the oracle masks it (top-level identity
//!     values and each history entry's `pid` / `hostname` / `timestamp` →
//!     `"<m>"`, digit runs in `detail` → `<n>`; `port` and every unknown key
//!     raw, in place) and re-serialized compactly — key ORDER included;
//!   - the file's own shape: `JSON.stringify(content, null, 2) + '\n'`;
//!   - the two lines this lane ports (`Building instance lock content`,
//!     `Recorded listening port on instance lock`): count, order, level,
//!     message, and the fields from the child's `module` binding onward with
//!     their keys IN ORDER (`pid` masked; `port` absent when v4's is
//!     `undefined`). v4's other `database:instance-lock` lines are recorded in
//!     the oracle but are not this lane's (the pre-existing `lock.rs` lines
//!     carry no `module` binding — P4.D299 R-L5, recorded).
//!
//! Red-first (the lane record): on unported `main` (one-arg
//! `acquire_instance_lock`, adapted below to ignore the port) the fresh rows
//! with a port, every claim row, and every row planting a malformed `port` or
//! unknown keys were RED.
//!
//! P4.171 (C-6) widens the compared lines to EVERY one v5 ports (the
//! `OWNED` list: the two read lines, the three acquire lines, the re-entrant
//! DEBUG, the heartbeat / lost / released / not-owned lines carrying the
//! `module` binding), masks `lockPath`, `stalePid` and `lastHeartbeat` on both
//! sides, reads v5's `errorJson` back as v4's `error`, and adds the lenient-read
//! rows: an action v4 THROWS on must fail on v5 too (`threw`). v4's
//! `Lock heartbeat started` / `stopped` DEBUGs are not compared — v5's host
//! drives the heartbeat, outside `acquire_instance_lock`.
//!
//! The C-6 follow-up (the §3 review) adds four foreign-host rows over a
//! NON-STRING `lastHeartbeat` (`"__NOW_MS__"` → this side's `Date.now()`): a
//! fresh epoch-ms number v4 REFUSES — RED before the fix, when v5's lenient
//! view read it as "never" and CLAIMED the lock — then `0`, `{}` and `true`.
//!
//! Generate the oracle (Node 24, from the v4 checkout — or the pinned
//! worktree; a `tsx` script):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-instance-lock-port.ndjson \
//!     $N/npx tsx $V5W/harness/oracle/cases/instance-lock-port.ts
//! Run:
//!   QT_ORACLE_INSTANCE_LOCK_PORT=/tmp/oracle-instance-lock-port.ndjson \
//!     cargo test -p quilltap-harness --test instance_lock_port_equivalence -- --nocapture
//!
//! Skips (does not fail) when the env var is unset — the standing gated
//! discipline.

use std::path::Path;
use std::sync::{Arc, Mutex};

use quilltap_host::lock::{acquire_instance_lock, heartbeat_tick, hostname};
use serde_json::{Map, Value};

/// The `database:instance-lock` lines v5 ports (P4.D299's two, P4.171's rest).
const OWNED: [&str; 12] = [
    "Building instance lock content",
    "Recorded listening port on instance lock",
    "Lock file has invalid structure, treating as corrupt",
    "Could not read lock file, treating as absent",
    "Instance lock acquired",
    "Stale instance lock detected, claiming",
    "Instance lock acquired after stale claim",
    "Instance lock re-acquired (same PID)",
    "Lock heartbeat updated",
    "Instance lock file disappeared — another process may claim the database. Shutting down.",
    "Instance lock lost — another process has taken over the database. Shutting down.",
    "Lock file not owned by this process, skipping release",
];

/// Fields whose values differ by construction on the two sides (each side's
/// own scratch path, its own dead child's PID, its own clock).
const MASKED_FIELDS: [&str; 4] = ["pid", "lockPath", "stalePid", "lastHeartbeat"];

/// One line field as compared: the per-side values masked, and the claim's
/// `reason` (`PID <dead pid> is no longer running`) with its digit runs
/// masked as the file's history `detail` is.
fn mask_field(k: &str, v: Value) -> Value {
    match (k, v) {
        (k, _) if MASKED_FIELDS.contains(&k) => Value::from("<m>"),
        ("reason", Value::String(s)) => Value::from(mask_digits(&s)),
        (_, v) => v,
    }
}

/// One captured line: `(level, message, fields in order)`.
type Line = (String, String, Vec<(String, Value)>);

struct Fields {
    message: String,
    fields: Vec<(String, Value)>,
}

impl tracing::field::Visit for Fields {
    fn record_str(&mut self, f: &tracing::field::Field, v: &str) {
        self.fields.push((f.name().to_string(), Value::from(v)));
    }
    fn record_u64(&mut self, f: &tracing::field::Field, v: u64) {
        self.fields.push((f.name().to_string(), Value::from(v)));
    }
    fn record_i64(&mut self, f: &tracing::field::Field, v: i64) {
        self.fields.push((f.name().to_string(), Value::from(v)));
    }
    fn record_bool(&mut self, f: &tracing::field::Field, v: bool) {
        self.fields.push((f.name().to_string(), Value::from(v)));
    }
    fn record_debug(&mut self, f: &tracing::field::Field, v: &dyn std::fmt::Debug) {
        if f.name() == "message" {
            self.message = format!("{v:?}");
        } else {
            self.fields
                .push((f.name().to_string(), Value::from(format!("{v:?}"))));
        }
    }
}

struct Capture(Arc<Mutex<Vec<Line>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Capture {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut f = Fields {
            message: String::new(),
            fields: Vec::new(),
        };
        event.record(&mut f);
        let level = event.metadata().level().as_str().to_ascii_lowercase();
        self.0.lock().unwrap().push((level, f.message, f.fields));
    }
}

fn captured<T>(f: impl FnOnce() -> T) -> (T, Vec<Line>) {
    use tracing_subscriber::layer::SubscriberExt;
    let sink = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(Capture(sink.clone()));
    let out = tracing::subscriber::with_default(subscriber, f);
    let lines = std::mem::take(&mut *sink.lock().unwrap());
    (out, lines)
}

/// The owned lines in v4's shape: fields from `module` onward, `pid` masked.
fn owned_v5(lines: &[Line]) -> Vec<Value> {
    lines
        .iter()
        .filter(|(_, m, _)| OWNED.contains(&m.as_str()))
        .map(|(level, message, fields)| {
            let mut obj = Map::new();
            for (k, v) in fields {
                // The `…Json` convention (§R.5): v5's `<k>Json` is v4's `<k>`
                // JSON-encoded (`errorJson` a string, `staleEnvironmentJson` a
                // non-string value).
                let (k, v) = match (k.strip_suffix("Json"), v) {
                    (Some(stem), Value::String(raw)) if !stem.is_empty() => (
                        stem.to_string(),
                        serde_json::from_str(raw).unwrap_or_else(|_| v.clone()),
                    ),
                    _ => (k.clone(), v.clone()),
                };
                let v = mask_field(&k, v);
                obj.insert(k, v);
            }
            serde_json::json!({ "level": level, "message": message, "fields": obj })
        })
        .collect()
}

fn owned_v4(lines: &[Value]) -> Vec<Value> {
    lines
        .iter()
        .filter(|l| OWNED.contains(&l["message"].as_str().unwrap_or("")))
        .map(|l| {
            let mut obj = Map::new();
            let mut on = false;
            for (k, v) in l["fields"].as_object().unwrap() {
                on |= k == "module";
                if on {
                    obj.insert(k.clone(), mask_field(k, v.clone()));
                }
            }
            // v4's `Instance lock acquired` passes `environment` at the call
            // site (`{lockPath, pid, environment}`), but the merged object keeps
            // the logger's BASE `environment` key's slot (before `module`) —
            // so the call's value is read there and put back in call order.
            if l["message"] == "Instance lock acquired" {
                obj.insert("environment".into(), l["fields"]["environment"].clone());
            }
            serde_json::json!({ "level": l["level"], "message": l["message"], "fields": obj })
        })
        .collect()
}

const MASKED_TOP: [&str; 7] = [
    "pid",
    "hostname",
    "startedAt",
    "lastHeartbeat",
    "environment",
    "processTitle",
    "processArgv0",
];

/// The digit runs of a history `detail` → `<n>` (the oracle's
/// `/\d+/g` → `'<n>'`).
fn mask_digits(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_run = false;
    for c in s.chars() {
        if c.is_ascii_digit() {
            if !in_run {
                out.push_str("<n>");
                in_run = true;
            }
        } else {
            in_run = false;
            out.push(c);
        }
    }
    out
}

/// The oracle's `maskLock`, over the file's own key order.
fn mask_lock(text: &str) -> String {
    // P4.171: a refused plant stays as written (the oracle's `RAW:` / compact
    // non-object arms).
    let parsed: Map<String, Value> = match serde_json::from_str::<Value>(text) {
        Err(_) => return format!("RAW:{text}"),
        Ok(Value::Object(m)) => m,
        Ok(other) => return serde_json::to_string(&other).unwrap(),
    };
    let mut masked = Map::new();
    for (k, v) in parsed {
        if MASKED_TOP.contains(&k.as_str()) {
            masked.insert(k, Value::from("<m>"));
        } else if k == "history" {
            let entries = v
                .as_array()
                .expect("history array")
                .iter()
                .map(|entry| {
                    let mut e = Map::new();
                    for (ek, ev) in entry.as_object().expect("history entry") {
                        let ev = match (ek.as_str(), ev) {
                            ("pid" | "hostname" | "timestamp", _) => Value::from("<m>"),
                            ("detail", Value::String(s)) => Value::from(mask_digits(s)),
                            _ => ev.clone(),
                        };
                        e.insert(ek.clone(), ev);
                    }
                    Value::Object(e)
                })
                .collect();
            masked.insert(k, Value::Array(entries));
        } else {
            masked.insert(k, v);
        }
    }
    serde_json::to_string(&Value::Object(masked)).unwrap()
}

fn dead_pid() -> u32 {
    let child = std::process::Command::new("true")
        .spawn()
        .expect("spawn true");
    let pid = child.id();
    let _ = child.wait_with_output();
    std::thread::sleep(std::time::Duration::from_millis(20));
    pid
}

fn acquire(path: &Path, port: Option<u16>) -> Result<(), String> {
    acquire_instance_lock(path, port).map_err(|e| e.to_string())
}

#[test]
fn instance_lock_port_equivalence() {
    let Ok(oracle) = std::env::var("QT_ORACLE_INSTANCE_LOCK_PORT") else {
        eprintln!("SKIP: instance lock port: set QT_ORACLE_INSTANCE_LOCK_PORT");
        return;
    };
    let text = std::fs::read_to_string(&oracle).expect("read oracle");
    let host = hostname();
    let mut failures: Vec<String> = Vec::new();
    let mut rows = 0usize;

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("oracle row");
        if row["kind"] != "row" {
            continue;
        }
        rows += 1;
        let name = row["name"].as_str().unwrap().to_string();
        let listening = row["listeningPort"].as_u64().map(|p| p as u16);
        let scratch = tempfile::Builder::new()
            .prefix("qt-instance-lock-port-")
            .tempdir()
            .unwrap();
        let path = scratch.path().join("quilltap.lock");

        if let Some(plant) = row["plant"].as_object() {
            let pid = match plant["pid"].as_str().unwrap() {
                "dead" => dead_pid(),
                "self" => std::process::id(),
                other => panic!("{name}: plant pid {other}"),
            };
            let template = plant["template"].as_str().unwrap();
            // `"__NOW_MS__"` (P4.171 C-6 follow-up): a numeric epoch-ms
            // heartbeat stamped at plant time on each side.
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis();
            let text = template
                .replacen("\"__PID__\"", &pid.to_string(), 1)
                .replacen("\"__NOW_MS__\"", &now_ms.to_string(), 1)
                .replace("__HOST__", &host);
            std::fs::write(&path, text).unwrap();
        }

        for action in row["actions"].as_array().unwrap() {
            let what = action["action"].as_str().unwrap();
            let label = format!("{name} :: {what}");
            let (outcome, lines) = captured(|| match what {
                "acquire" => acquire(&path, listening),
                "heartbeat" => {
                    if heartbeat_tick(&path) {
                        Ok(())
                    } else {
                        Err("heartbeat reported the lock LOST".to_string())
                    }
                }
                other => panic!("{label}: unknown action {other}"),
            });
            let v4_threw = action["threw"].as_bool().unwrap_or(false);
            match (&outcome, v4_threw) {
                (Err(e), false) => {
                    failures.push(format!("[{label}] v5 failed: {e}"));
                    continue;
                }
                (Ok(()), true) => {
                    failures.push(format!("[{label}] v4 threw, v5 succeeded"));
                    continue;
                }
                _ => {}
            }

            // The file: masked key order + values, and v4's pretty shape.
            let want = action["masked"].as_str().unwrap_or("<absent>");
            let raw = std::fs::read_to_string(&path).unwrap_or_default();
            let got = if raw.is_empty() {
                "<absent>".to_string()
            } else {
                mask_lock(&raw)
            };
            if got != want {
                failures.push(format!(
                    "[{label}] lock file differs\n  v4: {want}\n  v5: {got}"
                ));
            }
            if !raw.is_empty() && !v4_threw {
                let reparsed: Value = serde_json::from_str(&raw).unwrap();
                let pretty = format!("{}\n", serde_json::to_string_pretty(&reparsed).unwrap());
                if pretty != raw {
                    failures.push(format!("[{label}] file is not pretty-2 + '\\n':\n{raw}"));
                }
            }

            // The owned lines.
            let v4_lines = owned_v4(action["lines"].as_array().unwrap());
            let v5_lines = owned_v5(&lines);
            let (a, b) = (
                serde_json::to_string(&v4_lines).unwrap(),
                serde_json::to_string(&v5_lines).unwrap(),
            );
            if a != b {
                failures.push(format!("[{label}] lines differ\n  v4: {a}\n  v5: {b}"));
            }
        }
    }

    assert!(rows > 0, "oracle {oracle} has no rows");
    eprintln!(
        "instance lock port: {rows} rows, {} failures",
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
