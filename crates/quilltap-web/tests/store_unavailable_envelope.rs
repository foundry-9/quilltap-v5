//! P4.23 — the store-unavailable 503 envelope at the LIVE web edge, plus the
//! tier-2 wire pin on the three envelope bodies.
//!
//! `vault_unavailable_envelope_matches_oracle` boots the real server over a
//! doctored copy of the committed characters fixture (the FIRST vaulted
//! character's `properties.json` keystone DELETED; the SECOND's PLANTED with
//! the dogfood-#47 malformed bytes `{`) and drives `POST /api/dispatch`:
//!
//!   - `characterGet` on the absent keystone → **503** with v4's contextful
//!     body keys merged alongside the typed envelope (`error` +
//!     `characterId`), byte-compared against v4's REAL
//!     `GET /api/v1/characters/[id]` answer (the middleware envelope,
//!     `context.ts:176-205`) recorded by
//!     `harness/oracle/cases/store-unavailable-routes.test.ts`. An EQUALITY.
//!   - `characterUpdate` on the corrupt vault → **503** with the same vault
//!     envelope. **A PLAIN EQUALITY** since the 4.9.0-push round: v4 adopted
//!     the finding-#47 fix (`13ddc5ee`) and answers the identical envelope;
//!     the old both-directions tripwire here fired at the P4.D129 neutrality
//!     sweep (this twin was missed at the corpus retirement because the
//!     family SKIPs without its env var). The write-nothing post-state pins
//!     live in the characters-update tier-2 corpus.
//!
//! Generate the oracle (Node 24 — see the .ts header for the full recipe):
//!   QT_FIXTURE_CHARACTERS_MAIN=<repo>/crates/quilltap-web/tests/fixtures/characters-main.db \
//!   QT_FIXTURE_CHARACTERS_MOUNT=<...>/characters-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-store-unavailable.ndjson \
//!     npx jest … -- store-unavailable-routes
//! Run:
//!   QT_ORACLE_STORE_UNAVAILABLE=/tmp/oracle-store-unavailable.ndjson \
//!     cargo test -p quilltap-web --test store_unavailable_envelope
//!
//! `store_unavailable_wire_pin` runs unconditionally: the exact serialized
//! bytes (key order included — `preserve_order` makes insertion order the wire
//! order) of all three envelope bodies, per `context.ts:176-205`.

mod common;

use quilltap_core::api::types::Response;
use quilltap_core::db::doc_mount_file_links::DocMountFileLinksRepository;
use quilltap_core::db::Writer;
use serde_json::{json, Value};

/// The exact wire bodies of v4's three store-unavailable 503 envelopes
/// (`context.ts:176-205`), key order included.
#[test]
fn store_unavailable_wire_pin() {
    let cases = [
        (
            "project",
            "p-1",
            r#"{"error":"Project document store unavailable","projectId":"p-1"}"#,
        ),
        (
            "group",
            "g-1",
            r#"{"error":"Group document store unavailable","groupId":"g-1"}"#,
        ),
        (
            "character",
            "c-1",
            r#"{"error":"Character vault unavailable","characterId":"c-1"}"#,
        ),
    ];
    for (label, id, want) in cases {
        let Response::Error(e) = Response::store_unavailable(label, id) else {
            panic!("store_unavailable did not build an error");
        };
        let body = e.unavailable_wire_body().expect("entity carry");
        assert_eq!(
            serde_json::to_string(&body).unwrap(),
            want,
            "{label}: wire body bytes (incl. key order) diverged"
        );
        // The envelope message mirrors the wire `error` string.
        assert_eq!(
            e.message,
            body["error"].as_str().unwrap(),
            "{label}: message"
        );
    }
}

/// The two vaulted characters the arms use, in the oracle's deterministic
/// order (`ORDER BY name`): (id, mount_point_id) pairs.
fn pick_vaulted(main_db: &std::path::Path) -> Vec<(String, String)> {
    let w = Writer::open_writable(main_db, common::TEST_PEPPER).expect("open main");
    let mut stmt = w
        .connection()
        .prepare(
            "SELECT id, characterDocumentMountPointId FROM characters \
             WHERE characterDocumentMountPointId IS NOT NULL ORDER BY name LIMIT 2",
        )
        .unwrap();
    let rows: Vec<(String, String)> = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    assert!(
        rows.len() >= 2,
        "fixture has fewer than two vaulted characters"
    );
    rows
}

#[tokio::test]
async fn vault_unavailable_envelope_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_STORE_UNAVAILABLE") else {
        eprintln!("SKIP: set QT_ORACLE_STORE_UNAVAILABLE to the oracle NDJSON (see header).");
        return;
    };
    let mut oracle: std::collections::HashMap<String, Value> = std::collections::HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .expect("read oracle")
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }

    // Doctor a fresh instance BEFORE boot: absent keystone on the first
    // vaulted character, the #47 malformed plant on the second.
    let base = common::materialize_characters_instance();
    let data = base.path().join("data");
    let picked = pick_vaulted(&data.join("quilltap.db"));
    let (absent_id, absent_mp) = picked[0].clone();
    let (corrupt_id, corrupt_mp) = picked[1].clone();
    {
        let w = Writer::open_writable(&data.join("quilltap-mount-index.db"), common::TEST_PEPPER)
            .expect("open mount");
        let links = DocMountFileLinksRepository::new(w.connection());
        links
            .delete_database_document(&absent_mp, "properties.json")
            .expect("delete keystone");
        links
            .write_database_document(&corrupt_mp, "properties.json", "{")
            .expect("plant corrupt bytes");
    }

    let (addr, _state) = common::serve_instance(base.path(), |c| c).await;
    let client = reqwest::Client::new();
    let url = format!("http://{addr}/api/dispatch");

    // ── Arm 1: the absent-keystone READ → the 503 envelope (EQUALITY vs v4) ──
    let want = &oracle["character_get_vault_absent"];
    assert_eq!(
        want["characterId"].as_str(),
        Some(absent_id.as_str()),
        "oracle picked a different character — the ORDER BY name pick diverged"
    );
    let resp = client
        .post(&url)
        .json(&json!({ "type": "characterGet", "characterId": absent_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status().as_u16(),
        want["status"].as_u64().unwrap() as u16,
        "absent-keystone GET status (v4 answers the deliberate 503)"
    );
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["data"]["kind"],
        json!("unavailable"),
        "typed envelope kind"
    );
    // v4's exact body keys, merged alongside the typed envelope (the Locked
    // pattern): both must equal v4's recorded values...
    assert_eq!(body["error"], want["body"]["error"], "merged `error` key");
    assert_eq!(
        body["characterId"], want["body"]["characterId"],
        "merged `characterId` key"
    );
    // ...and v4's body must be EXACTLY those two keys, in that order (pins the
    // v4 shape so a drifted envelope cannot pass unnoticed).
    let want_obj = want["body"].as_object().expect("oracle body object");
    assert_eq!(
        want_obj.keys().collect::<Vec<_>>(),
        vec!["error", "characterId"],
        "v4's envelope body shape moved"
    );

    // ── Arm 2: the corrupt-vault WRITE refusal (a PLAIN EQUALITY since the
    // 4.9.0-push round) ── v4 adopted the finding-#47 fix (`13ddc5ee`; the
    // characters-update tier-2 corpus arm retired then) and its route now
    // answers the same vault 503 envelope v5 has refused with since P4.22.
    // The old both-directions tripwire here fired at the P4.D129 neutrality
    // sweep — this web-edge twin had been missed at the corpus retirement
    // because the family SKIPs without its env var. Both sides now compare
    // as equals against the recorded oracle answer.
    let v4 = &oracle["character_update_vault_corrupt"];
    assert_eq!(
        v4["body"]["error"].as_str(),
        Some("Character vault unavailable"),
        "v4's corrupt-vault WRITE answer moved off the vault envelope"
    );
    assert_eq!(
        v4["status"].as_u64(),
        Some(503),
        "v4's corrupt-vault WRITE status moved"
    );
    assert_eq!(
        v4["body"]
            .as_object()
            .map(|o| o.keys().map(|k| k.as_str()).collect::<Vec<_>>()),
        Some(vec!["error", "characterId"]),
        "v4's corrupt-vault envelope body shape moved"
    );
    let resp = client
        .post(&url)
        .json(&json!({
            "type": "characterUpdate",
            "characterId": corrupt_id,
            "character": { "title": "clobber probe" },
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status().as_u16(),
        503,
        "corrupt-vault WRITE must answer the v5 refusal 503 (the deliberate divergence)"
    );
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["data"]["kind"],
        json!("unavailable"),
        "typed envelope kind"
    );
    assert_eq!(
        body["error"],
        json!("Character vault unavailable"),
        "merged `error` key"
    );
    assert_eq!(
        body["characterId"],
        json!(corrupt_id),
        "merged `characterId` key"
    );

    eprintln!(
        "OK: the vault 503 envelope matched v4 on the absent-keystone read; \
         the corrupt-write divergence is pinned both ways."
    );
}
