//! P4.D166 (bug 126) — the foreign-hostname refusal reaches the served boot
//! status.
//!
//! v4 `25f534c0b` made acquisition refuse ANY foreign-hostname lock whose
//! heartbeat is still fresh, whatever its environment; before it, a `local`
//! lock from a different recorded name was claimed outright (the fail-open
//! arm, and the only reason two processes on one renamed machine could both
//! open the database). This test plants exactly that lock and asserts the new
//! refusal sentence reaches `GET /health` — the surface the SPA renders.
//!
//! **A measured asymmetry, recorded not "fixed".** v5 answers this on the
//! **503** `unhealthy` surface, not the **409** `lock-conflict` one, because
//! `boot_startup_status` classifies the file with `classify_lock_status` —
//! v5's mirror of v4's launcher `getLockStatus`, which `25f534c0b` did NOT
//! touch and which still gates its freshness window on `docker`. v4 fixed the
//! server module and left the launcher module alone, so the two disagree in
//! v4 as well; matching each of them where it stands is the faithful outcome,
//! and re-deciding which HTTP code this case deserves is a v5-composition
//! question for its own order. Before the fix there was no refusal here at
//! all — the boot succeeded and clobbered the other process's lock.
//!
//! Run: `cargo test -p quilltap-web --test lock_conflict_boot_status`

mod common;

use serde_json::Value;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn foreign_hostname_fresh_heartbeat_refuses_the_boot_with_v4s_sentence() {
    let base = common::materialize_bare_instance();
    let lock_path = base.path().join("data/quilltap.lock");
    let now = quilltap_core::clock::now_iso();
    std::fs::write(
        &lock_path,
        format!(
            r#"{{"pid": 4242, "hostname": "elsewhere-host", "startedAt": "{now}",
"lastHeartbeat": "{now}", "environment": "local", "processTitle": "node",
"processArgv0": "/usr/bin/node", "history": []}}"#
        ),
    )
    .unwrap();

    let (addr, _state) = common::serve_instance(base.path(), |c| c).await;
    let res = reqwest::get(format!("http://{addr}/health")).await.unwrap();
    let status = res.status().as_u16();
    let body: Value = res.json().await.unwrap();

    let error = body
        .get("error")
        .and_then(Value::as_str)
        .or_else(|| body.get("lockConflict")?.get("reason")?.as_str())
        .unwrap_or_else(|| panic!("no message in {body}"));

    // v5 wraps the typed conflict in its own `BootError::Assemble` prefix; the
    // sentence after it is v4's, byte for byte.
    assert!(
        error.starts_with(
            "engine assembly failed: Another Quilltap instance (local server, PID 4242 on \
             elsewhere-host) is already using this database (last heartbeat "
        ),
        "status {status}, body {body}"
    );
    assert!(
        error.contains(
            "If no other instance is running, this machine's hostname may have changed since \
             the lock was taken (now: "
        ),
        "{error}"
    );
    assert!(
        error.ends_with("for the lock to go stale, or use the lock override to force access."),
        "{error}"
    );
    // The measured surface (see the module doc): 503 `unhealthy`, because the
    // launcher-mirroring classifier still calls this lock stale.
    assert_eq!(status, 503, "body {body}");

    // The holder's record is untouched — the whole point of the refusal.
    let after: Value = serde_json::from_str(&std::fs::read_to_string(&lock_path).unwrap()).unwrap();
    assert_eq!(after["pid"], 4242);
    assert_eq!(after["hostname"], "elsewhere-host");
}
