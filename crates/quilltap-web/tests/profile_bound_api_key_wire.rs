//! P4.133 (dogfood #133) — the connection profile's OWN key reaches the wire
//! through the host's LIVE assembly.
//!
//! The finding: a NANOGPT profile bound to a junk key answered a real turn with
//! the instance's OTHER NANOGPT key. v5 resolved the right key in most places
//! and then dropped it — nothing crossed the model boundary but `(provider,
//! base_url, params)`, and the host's `DbProviderKeys` scanned for the FIRST
//! active key for the provider. v4 never scans on a chat or completion call: it
//! follows the effective profile's `apiKeyId`.
//!
//! The differential families prove which key each leg HANDS the provider
//! (`orchestrator_tier3`'s per-call keys, `primary_stream_tier3`'s ordered
//! `stream_calls`, `title_update_tier3`'s lifted key mock), and the core / host
//! unit pins prove the two `Wire*Provider` overrides put that key on the
//! request. What only THIS file can see is the composition between them: the
//! production factory (`ProductionSpineFactory`) builds the providers, the
//! logging cheap executor, the participant resolver and the requires-gate over
//! a real instance, and any layer that dropped the key would put the provider
//! scan's key (the `k-first-*` below) on the wire.
//!
//! ⚠ MEASURED (P4.133's M1): removing an `Arc` impl's explicit keyed forward
//! does NOT red this file. The spine hands the Salon turn `&*self.streaming`
//! and the cheap executor its completion provider by value, so neither leg
//! here goes through `Arc<T>`'s impl. The forward is proven by the two unit
//! pins that hold an `Arc` (`model::streaming_provider` and the host's
//! `spine::profile_timeout_tests`) and by `orchestrator_tier3`, whose spine
//! holds the streaming provider as an `Arc` exactly as the production Carina /
//! Brahma seams do — each reds on the mutation.
//!
//! So every profile in the fixture points at a header-capturing listener, each
//! provider holds a `k-first-*` key inserted BEFORE every bound key (the old
//! scan's pick, which no profile names), and each profile is bound to its own
//! `k-bound-*`. Two verbs over `/api/dispatch` reach the wire, each on the
//! committed pair whose cast can run it: the in-scene voice rehearsal (a
//! cheap-LLM COMPLETION through the logging executor and the host's
//! `WireCompletionProvider`; its pair's chat cannot run a Salon turn — one
//! character's vault is deliberately absent) and a Salon send on the
//! `chat-send` pair (the STREAMING provider behind the factory's `Arc`). Every
//! request the listener sees must carry a bound key, never a `k-first-*`; the
//! rehearsal's must be the seat profile's own, the send's the responder's. The
//! listener answers 500, so nothing is spent and both fail at the provider.
//!
//! Run:
//!   cargo test -p quilltap-web --test profile_bound_api_key_wire

mod common;

use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const CHAT_STACK: &str = "c1000000-0000-4000-8000-000000000001";
/// The `chat-send` pair's 1:1 chat (Friday on an ANTHROPIC profile — the
/// `x-api-key` header), sent in continue mode as the M2 smoke sends it.
const SMOKE_CHAT_ID: &str = "9fe3f87b-3833-46a9-bc1e-a88fc43dee6b";
/// Friday's profile in that pair (`Primary`, ANTHROPIC).
const RESPONDER_PROFILE: &str = "a1cb6066-2fcc-4006-a7db-ddb2fc6e2221";
const P_VESPER: &str = "e1000000-0000-4000-8000-000000000001";
/// The seat's profile — the rehearsal's cheap selection (OPENAI_COMPATIBLE).
const CONN_SEAT: &str = "c0000000-0000-4000-8000-000000000001";

/// Every profile on the listener, a `k-first-<provider>` per provider inserted
/// first, then each profile bound to its own `k-bound-<n>`. Returns the bound
/// key per profile id.
fn plant_two_keys(base: &std::path::Path, base_url: &str) -> Vec<(String, String)> {
    let w = quilltap_core::db::Writer::open_writable(
        &base.join("data").join("quilltap.db"),
        common::TEST_PEPPER,
    )
    .unwrap();
    let c = w.connection();
    c.execute("DELETE FROM api_keys", []).unwrap();
    let profiles: Vec<(String, String)> = {
        let mut stmt = c
            .prepare("SELECT id, provider FROM connection_profiles ORDER BY rowid")
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    let insert = |id: &str, provider: &str, value: &str| {
        c.execute(
            "INSERT INTO api_keys (id, userId, label, provider, key_value, isActive, \
             createdAt, updatedAt) VALUES (?1, ?2, ?3, ?4, ?3, 1, \
             '2026-02-01T00:00:00.000Z', '2026-02-01T00:00:00.000Z')",
            rusqlite::params![id, quilltap_core::api::SINGLE_USER_ID, value, provider],
        )
        .unwrap();
    };
    let mut providers: Vec<&str> = profiles.iter().map(|(_, p)| p.as_str()).collect();
    providers.sort_unstable();
    providers.dedup();
    for (i, provider) in providers.iter().enumerate() {
        insert(
            &format!("f1500000-0000-4000-8000-{:012x}", i + 1),
            provider,
            &format!("k-first-{provider}"),
        );
    }
    let mut bound = Vec::new();
    for (i, (id, provider)) in profiles.iter().enumerate() {
        let key_id = format!("b0500000-0000-4000-8000-{:012x}", i + 1);
        let value = format!("k-bound-{}", i + 1);
        insert(&key_id, provider, &value);
        c.execute(
            "UPDATE connection_profiles SET apiKeyId = ?1, baseUrl = ?2 WHERE id = ?3",
            rusqlite::params![key_id, base_url, id],
        )
        .unwrap();
        bound.push((id.clone(), value));
    }
    bound
}

/// Accepts every connection, records the request head, answers 500.
async fn capture_listener() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            let sink = Arc::clone(&sink);
            tokio::spawn(async move {
                let mut buf = vec![0u8; 1 << 20];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                if n > 0 {
                    sink.lock()
                        .unwrap()
                        .push(String::from_utf8_lossy(&buf[..n]).into_owned());
                }
                let _ = sock
                    .write_all(
                        b"HTTP/1.1 500 Internal Server Error\r\ncontent-type: application/json\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}",
                    )
                    .await;
            });
        }
    });
    (base_url, seen)
}

/// The key a request head carries, whichever header its provider's manifest
/// names (`Authorization: Bearer …`, `x-api-key`, `x-goog-api-key`).
fn carried_key(head: &str) -> Option<String> {
    head.lines().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
        match k.as_str() {
            "authorization" => Some(v.strip_prefix("Bearer ").unwrap_or(v).to_string()),
            "x-api-key" | "x-goog-api-key" => Some(v.to_string()),
            _ => None,
        }
    })
}

async fn dispatch(client: &reqwest::Client, addr: &std::net::SocketAddr, body: Value) -> u16 {
    client
        .post(format!("http://{addr}/api/dispatch"))
        .json(&body)
        .send()
        .await
        .unwrap()
        .status()
        .as_u16()
}

async fn serve_production(base: &std::path::Path) -> std::net::SocketAddr {
    let base_dir = base.to_path_buf();
    let (addr, _state) = common::serve_instance(base, move |mut c| {
        c.terminal = false;
        c.spine = Some(Arc::new(quilltap_host::ProductionSpineFactory::new(
            base_dir,
            c.version.clone(),
            c.tz.clone(),
            c.display_zone.clone(),
        )));
        c
    })
    .await;
    addr
}

/// The COMPLETION half: the in-scene voice rehearsal, a cheap-LLM task on the
/// seat's profile. The provider answers 500 → v4's preview failure (400).
#[tokio::test(flavor = "multi_thread")]
async fn the_rehearsal_carries_the_seat_profiles_own_key() {
    let (base_url, seen) = capture_listener().await;
    let base = common::materialize_in_scene_voiced_instance();
    let bound = plant_two_keys(base.path(), &base_url);
    let seat_key = bound
        .iter()
        .find(|(id, _)| id == CONN_SEAT)
        .map(|(_, k)| k.clone())
        .expect("the seat profile is in the fixture");
    let addr = serve_production(base.path()).await;
    let client = reqwest::Client::new();

    let status = dispatch(
        &client,
        &addr,
        json!({
            "type": "chatImpersonationVoicePreview",
            "chatId": CHAT_STACK,
            "participantId": P_VESPER,
            "seedMarkdown": "Tell them the lamps are out and I am seeing to it.",
        }),
    )
    .await;
    assert_eq!(
        status, 400,
        "the rehearsal must reach the provider and fail there"
    );
    let heads: Vec<String> = seen.lock().unwrap().clone();
    assert!(
        !heads.is_empty(),
        "the rehearsal never reached the listener"
    );
    for head in &heads {
        assert_eq!(
            carried_key(head).as_deref(),
            Some(seat_key.as_str()),
            "the rehearsal must carry the SEAT profile's own key, not the provider scan's \
             `k-first-*`:\n{head}"
        );
    }
}

/// The STREAMING half: a Salon send in continue mode. The responder's request
/// (and any understudy's the failover chain tries after the 500) must carry a
/// key some profile is BOUND to — the responder's first.
#[tokio::test(flavor = "multi_thread")]
async fn the_salon_send_carries_the_responders_own_key() {
    let (base_url, seen) = capture_listener().await;
    let base = common::materialize_fixture_instance();
    let bound = plant_two_keys(base.path(), &base_url);
    let addr = serve_production(base.path()).await;
    let client = reqwest::Client::new();

    let _ = dispatch(
        &client,
        &addr,
        json!({
            "type": "chatSend",
            "chatId": SMOKE_CHAT_ID,
            "content": "",
            "continueMode": true,
        }),
    )
    .await;
    let heads: Vec<String> = seen.lock().unwrap().clone();
    assert!(
        !heads.is_empty(),
        "the Salon send never reached the listener"
    );
    let responder_key = bound
        .iter()
        .find(|(id, _)| id == RESPONDER_PROFILE)
        .map(|(_, k)| k.as_str())
        .expect("the responder's profile is in the fixture");
    assert_eq!(
        carried_key(&heads[0]).as_deref(),
        Some(responder_key),
        "the primary stream must carry the RESPONDER profile's own key:\n{}",
        heads[0]
    );
    let bound_keys: Vec<&str> = bound.iter().map(|(_, k)| k.as_str()).collect();
    for head in &heads {
        let key = carried_key(head).unwrap_or_default();
        assert!(
            bound_keys.contains(&key.as_str()),
            "a streamed request carried `{key}`, which no profile is bound to (the scan's \
             `k-first-*` is the dogfood-#133 symptom):\n{head}"
        );
    }
}
