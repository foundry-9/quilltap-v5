//! P4.71 — the host-gateway differential (tier 1, EXACT).
//!
//! Compares `quilltap_host::host_gateway` against v4's REAL
//! `lib/host-rewrite.ts` at `6d2a50382`: the environment predicate
//! (`isVMEnvironment`), the two-strategy gateway ladder (`resolveHostGateway`)
//! and the whole public rewrite (`rewriteLocalhostUrl`), over the full matrix
//! of {`QUILLTAP_HOST_IP` unset / empty / an IP / a hostname} x {Docker,
//! not} x six URL shapes.
//!
//! Three things are compared per row, not one:
//!   - `isVMEnvironment()` — the JS `!!` on the env var (an EMPTY var is unset),
//!   - the returned URL — which carries the resolved gateway inside it,
//!   - **every log line the call emitted, in order**, with its level, its
//!     message bytes and v4's context bag. That makes the four sentences and
//!     their fields contractual rather than transcribed, and it is what pins
//!     v4's ORDER: `rewriteLocalhostUrl` returns before resolving for a
//!     non-localhost or unparseable URL, so those rows are SILENT even in an
//!     environment that would have resolved happily.
//!
//! The `cache` rows pin v4's module-level `cachedGatewayHost`: two rewrites on
//! a freshly-reset module resolve (and log the resolution) exactly once, so the
//! second call carries only the debug line. v5's process resolver caches in a
//! `OnceLock`; the pure function the differential drives is cache-free, so the
//! rows are compared as "the first call's lines, then the debug line alone".
//!
//! Regenerate the oracle (Node 24; jest ignores `.claude/` venues, so the case
//! is copied to a /tmp mirror. v4 HEAD is past the baseline, so this runs from
//! a pinned worktree — drift-ledger 5.1):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   PIN=/tmp/qt-v4-pin-p471-6d2a50382
//!   git -C ~/source/quilltap-server worktree add --detach "$PIN" 6d2a50382 || true
//!   ln -sfn ~/source/quilltap-server/node_modules "$PIN/node_modules"
//!   TMPO=/tmp/qt-host-gateway-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
//!   cp "$V5W/harness/oracle/cases/host-gateway.test.ts" "$TMPO/cases/"
//!   cd "$PIN"
//!   QT_ORACLE_OUT=/tmp/oracle-host-gateway.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$TMPO/cases" -- host-gateway
//!
//! Run:
//!   QT_ORACLE_HOST_GATEWAY=/tmp/oracle-host-gateway.ndjson \
//!     cargo test -p quilltap-harness --test host_gateway_equivalence -- --nocapture
//!
//! Skips (does not fail) when the env var is unset — the standing gated-
//! differential discipline.

use quilltap_host::host_gateway::{
    is_vm_environment_pure, resolve_host_gateway_pure, rewrite_localhost_url_pure, GatewayLog,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct WireLog {
    level: String,
    message: String,
    context: Option<Value>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "rewrite")]
    Rewrite {
        name: String,
        /// `null` = the var was DELETED; `""` = present but empty.
        #[serde(rename = "hostIp")]
        host_ip: Option<String>,
        #[serde(rename = "isDocker")]
        is_docker: bool,
        url: String,
        #[serde(rename = "isVM")]
        is_vm: bool,
        rewritten: String,
        logs: Vec<WireLog>,
    },
    #[serde(rename = "cache")]
    Cache {
        name: String,
        #[serde(rename = "hostIp")]
        host_ip: Option<String>,
        #[serde(rename = "isDocker")]
        is_docker: bool,
        url: String,
        first: String,
        #[serde(rename = "firstLogs")]
        first_logs: Vec<WireLog>,
        second: String,
        #[serde(rename = "secondLogs")]
        second_logs: Vec<WireLog>,
    },
    #[serde(rename = "child_context")]
    ChildContext { name: String, context: Value },
}

/// Render one of v5's log values into v4's wire shape: level word, message
/// bytes, and v4's context bag (`null` where v4 passes none).
fn render(line: &GatewayLog) -> Value {
    let context = match line {
        GatewayLog::EnvOverride { host } => json!({ "host": host }),
        GatewayLog::Docker | GatewayLog::Unresolved => Value::Null,
        GatewayLog::Rewrote {
            original,
            rewritten,
            gateway_host,
        } => json!({
            "original": original,
            "rewritten": rewritten,
            "gatewayHost": gateway_host,
        }),
    };
    json!({ "level": line.level(), "message": line.message(), "context": context })
}

fn render_wire(line: &WireLog) -> Value {
    json!({
        "level": line.level,
        "message": line.message,
        "context": line.context.clone().unwrap_or(Value::Null),
    })
}

fn render_all(lines: &[GatewayLog]) -> Vec<Value> {
    lines.iter().map(render).collect()
}

fn render_all_wire(lines: &[WireLog]) -> Vec<Value> {
    lines.iter().map(render_wire).collect()
}

#[test]
fn host_gateway_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_HOST_GATEWAY") else {
        eprintln!("SKIP: set QT_ORACLE_HOST_GATEWAY to the oracle NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read the host-gateway oracle");
    let rows: Vec<Row> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse an oracle row"))
        .collect();

    let mut failed: Vec<String> = Vec::new();
    let mut rewrite_cases = 0;
    let mut cache_cases = 0;
    let mut child_cases = 0;
    // Shape guards for the two arms the corpus exists to hold (a corpus that
    // quietly lost them would otherwise pass green — the standing
    // `harness-corpus-shape-constants-rot` rule).
    let mut silent_in_a_resolving_env = 0;
    let mut rewrote_rows = 0;

    for row in &rows {
        match row {
            Row::Rewrite {
                name,
                host_ip,
                is_docker,
                url,
                is_vm,
                rewritten,
                logs,
            } => {
                rewrite_cases += 1;
                let host_ip = host_ip.as_deref();

                let got_vm = is_vm_environment_pure(host_ip, *is_docker);
                if got_vm != *is_vm {
                    eprintln!("[{name}] isVMEnvironment MISMATCH: got {got_vm} want {is_vm}");
                    failed.push(name.clone());
                    continue;
                }

                let (got_url, got_logs) = rewrite_localhost_url_pure(host_ip, *is_docker, url);
                if &got_url != rewritten {
                    eprintln!("[{name}] URL MISMATCH:\n  got  {got_url}\n  want {rewritten}");
                    failed.push(name.clone());
                    continue;
                }

                let got_lines = render_all(&got_logs);
                let want_lines = render_all_wire(logs);
                if got_lines != want_lines {
                    eprintln!(
                        "[{name}] LOG MISMATCH:\n  got  {}\n  want {}",
                        serde_json::to_string(&got_lines).unwrap(),
                        serde_json::to_string(&want_lines).unwrap()
                    );
                    failed.push(name.clone());
                    continue;
                }

                // The two shapes worth counting by name.
                if *is_vm && logs.is_empty() {
                    silent_in_a_resolving_env += 1;
                }
                if logs.iter().any(|l| l.message == "Rewrote localhost URL") {
                    rewrote_rows += 1;
                }
                eprintln!("[{name}] OK ({} log line(s)).", logs.len());
            }

            Row::Cache {
                name,
                host_ip,
                is_docker,
                url,
                first,
                first_logs,
                second,
                second_logs,
            } => {
                cache_cases += 1;
                let host_ip = host_ip.as_deref();

                // Call 1 on a cold module: the whole ladder plus the rewrite.
                let (got_first, got_first_logs) =
                    rewrite_localhost_url_pure(host_ip, *is_docker, url);
                if &got_first != first || render_all(&got_first_logs) != render_all_wire(first_logs)
                {
                    eprintln!(
                        "[{name}] FIRST CALL MISMATCH: got {got_first} / {:?}",
                        got_first_logs
                    );
                    failed.push(name.clone());
                    continue;
                }

                // Call 2 with the gateway already cached: v5's `OnceLock`
                // suppresses the resolution lines, leaving whatever the rewrite
                // itself emits. Derive that from the same pure call rather than
                // hand-listing it, so the two stay in step.
                let (gateway, _) = resolve_host_gateway_pure(host_ip, *is_docker);
                let cached_logs: Vec<Value> = if gateway.is_some() {
                    render_all(&got_first_logs)
                        .into_iter()
                        .filter(|l| l["message"] == "Rewrote localhost URL")
                        .collect()
                } else {
                    Vec::new()
                };
                if &got_first != second {
                    eprintln!("[{name}] SECOND CALL URL MISMATCH: got {got_first} want {second}");
                    failed.push(name.clone());
                    continue;
                }
                let want_second = render_all_wire(second_logs);
                if cached_logs != want_second {
                    eprintln!(
                        "[{name}] CACHED LOG MISMATCH:\n  got  {}\n  want {}",
                        serde_json::to_string(&cached_logs).unwrap(),
                        serde_json::to_string(&want_second).unwrap()
                    );
                    failed.push(name.clone());
                    continue;
                }
                eprintln!(
                    "[{name}] OK (cache: {} then {} line(s)).",
                    first_logs.len(),
                    second_logs.len()
                );
            }

            Row::ChildContext { name, context } => {
                child_cases += 1;
                // v4 `logger.child({ module: 'host-rewrite' })` — v5 carries the
                // same key/value as a `tracing` field on every event (pinned on
                // the emitter by `host_gateway`'s own capture test).
                let want = json!({ "module": "host-rewrite" });
                if context != &want {
                    eprintln!("[{name}] CHILD CONTEXT MISMATCH: got {context} want {want}");
                    failed.push(name.clone());
                } else {
                    eprintln!("[{name}] OK ({context}).");
                }
            }
        }
    }

    assert!(
        rewrite_cases >= 48,
        "expected the full 4x2x6 rewrite matrix (got {rewrite_cases}) — regenerate the oracle"
    );
    assert!(
        cache_cases >= 8,
        "expected the cache arms (got {cache_cases}) — regenerate the oracle"
    );
    assert_eq!(child_cases, 1, "expected the child-context row");
    assert!(
        silent_in_a_resolving_env >= 6,
        "the corpus lost its SILENCE arms (got {silent_in_a_resolving_env}): the rows where the \
         environment resolves but the URL is remote/unparseable are what pin v4's order"
    );
    assert!(
        rewrote_rows >= 12,
        "the corpus lost its rewriting arms (got {rewrote_rows}) — regenerate the oracle"
    );
    assert!(failed.is_empty(), "host-gateway FAILED: {failed:?}");
}
