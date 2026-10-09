//! Dogfood #153 — a stopped server releases the instance lock.
//!
//! v4 registers `handleShutdown` on SIGTERM / SIGINT (`lib/database/backends/
//! sqlite/client.ts`): close the three database clients, then
//! `releaseActiveInstanceLock()` (`Instance lock released`, the file
//! unlinked); `server.ts` logs `Shutting down`, closes every WebSocket client
//! and then the HTTP server. v5's `quilltap-web` installed no handler at all,
//! so a stopped server left `quilltap.lock` behind with a fresh heartbeat —
//! v4 on another machine sharing the instance refused to open it for the
//! five-minute stale window. This drives the composition `main` uses
//! (`begin_shutdown` → the graceful close) with a live `/api/events` stream
//! attached, the connection that would otherwise hold the close open forever.
//!
//! Run: `cargo test -p quilltap-web --test graceful_shutdown`

mod common;

use std::sync::Arc;
use std::time::Duration;

use quilltap_host::HostConfig;
use quilltap_web::{begin_shutdown, boot_startup_status, build_router, serve_listener, web_state};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_ends_the_event_stream_closes_http_and_releases_the_lock() {
    let base = common::materialize_bare_instance();
    let lock_path = base.path().join("data/quilltap.lock");

    let mut config = HostConfig::new(base.path());
    config.env_pepper = Some(common::TEST_PEPPER.to_string());
    config.startup_grace_ms = 3_600_000;
    config.danger_scan_interval_ms = 3_600_000;
    config.autonomous_tick_ms = 3_600_000;
    let version = config.version.clone();
    let state = web_state(
        boot_startup_status(config),
        version,
        base.path().to_path_buf(),
        None,
    );
    assert!(state.host().is_some(), "the bare instance boots");
    assert!(
        lock_path.exists(),
        "a running engine holds the instance lock"
    );

    let router = build_router(Arc::clone(&state));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (fire, fired) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(serve_listener(router, listener, async move {
        let _ = fired.await;
    }));

    // A live event stream: headers in, body still open.
    let stream = reqwest::get(format!("http://{addr}/api/events"))
        .await
        .unwrap();
    assert_eq!(stream.status().as_u16(), 200);

    begin_shutdown(&state, "SIGTERM").await;
    assert!(
        !lock_path.exists(),
        "begin_shutdown released the instance lock (v4 `releaseActiveInstanceLock`)"
    );
    let _ = fire.send(());

    // The stream ends instead of holding the graceful close open.
    let body = tokio::time::timeout(Duration::from_secs(3), stream.bytes())
        .await
        .expect("the event stream ends on shutdown");
    assert!(body.is_ok(), "the stream closed cleanly: {body:?}");
    let served = tokio::time::timeout(Duration::from_secs(3), server)
        .await
        .expect("the HTTP server closes well inside v4's 5 s timeout")
        .unwrap();
    assert!(served.is_ok(), "serve returned cleanly: {served:?}");
}
