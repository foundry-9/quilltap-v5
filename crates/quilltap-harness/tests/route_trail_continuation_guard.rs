//! P4.D173 — **Continue Elsewhere drops the route trail.**
//!
//! v4's `apply-chat-continuation.ts` does NOT copy `routeTrail` onto the new
//! chat's replayed messages: `projectMessageForNewChat` builds a *fresh* object
//! from a named list of keys, and the trail is not on it. v4's `5841a8c62` hunk
//! there is a COMMENT ONLY — the omission is by design (the new chat's row was
//! never routed anywhere; the profiles it would name belong to a different
//! turn), and the commit adds a line saying so rather than any code.
//!
//! `services/chat_continuation.rs` belongs to P4.D172 this round, so this pin is
//! deliberately READ-ONLY: no edit to that file, only a proof of what it does.
//! Two legs, because either alone can rot:
//!
//!   1. **Behavioural** — drive the REAL `apply_chat_continuation` over a
//!      provisioned instance whose source chat carries an assistant message WITH
//!      a trail, and read the replayed row back. A copy would show up here as a
//!      non-NULL column.
//!   2. **A source census** — the projection is an allow-list, so a future
//!      `routeTrail` in that file could only be a copy. Zero occurrences is the
//!      assertion (the `db_error_key_guard` idiom), and it catches the case
//!      where leg 1's seed silently stops carrying a trail.

use quilltap_core::db::runtime::{Db, DbPaths};
use serde_json::{json, Value};

/// One replayed assistant row: `(content, participantId, provider, routeTrail)`.
type ReplayedRow = (String, Option<String>, Option<String>, Option<String>);

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

/// A trail with two entries — a refusal and the seat that answered.
fn seeded_trail() -> Value {
    json!([
        {
            "profileId": "aa000001-0000-4000-8000-000000000001",
            "profileName": "House Anthropic",
            "provider": "ANTHROPIC",
            "modelName": "claude-sonnet-5",
            "via": "primary",
            "outcome": "refused",
            "trigger": "moderation-refusal",
            "evidence": "finish-reason",
            "detail": "finish_reason: content_filter"
        },
        {
            "profileId": "aa000002-0000-4000-8000-000000000002",
            "profileName": "The Back Room",
            "provider": "OPENROUTER",
            "modelName": "dolphin-mixtral",
            "via": "concierge",
            "outcome": "answered"
        }
    ])
}

#[tokio::test]
async fn continue_elsewhere_does_not_copy_the_route_trail() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    quilltap_core::services::provisioning::provision_fresh_instance(&data, PEPPER)
        .expect("provision");
    let db = Db::open(
        DbPaths {
            main: data.join("quilltap.db"),
            mount_index: None,
            llm_logs: None,
        },
        PEPPER,
    )
    .expect("open");

    // Two chats sharing one character, so the participant map is total and the
    // replay keeps the message rather than dropping it.
    const CHAR_ID: &str = "cc000001-0000-4000-8000-000000000001";
    const SRC: &str = "dd000001-0000-4000-8000-000000000001";
    const DST: &str = "dd000002-0000-4000-8000-000000000002";
    const SRC_PART: &str = "ee000001-0000-4000-8000-000000000001";
    const DST_PART: &str = "ee000002-0000-4000-8000-000000000002";

    let participants = |pid: &str| {
        json!([{
            "id": pid,
            "type": "character",
            "characterId": CHAR_ID,
            "controlledBy": "llm",
            "status": "active",
            // `ChatParticipant` mirrors v4's Zod: `createdAt`/`updatedAt` are
            // REQUIRED, and a participant that fails the parse is silently
            // DROPPED — which would leave the id map empty and make the replay
            // (and so this whole test) vacuous. The `replayed_message_count`
            // assertion below is what catches that.
            "createdAt": "2026-09-01T00:00:00.000Z",
            "updatedAt": "2026-09-01T00:00:00.000Z"
        }])
        .to_string()
    };
    let src_parts = participants(SRC_PART);
    let dst_parts = participants(DST_PART);
    let trail = seeded_trail().to_string();
    db.write(move |writers| {
        let c = writers.main().connection();
        for (id, title, parts) in [(SRC, "Source", &src_parts), (DST, "Continued", &dst_parts)] {
            c.execute(
                "INSERT INTO chats (id, userId, title, participants, createdAt, updatedAt) \
                 VALUES (?1, 'u', ?2, ?3, '2026-09-01T00:00:00.000Z', '2026-09-01T00:00:00.000Z')",
                rusqlite::params![id, title, parts],
            )
            .unwrap();
        }
        c.execute(
            "INSERT INTO chat_messages (id, chatId, type, role, content, participantId, \
             provider, modelName, routeTrail, createdAt) \
             VALUES ('ff000001-0000-4000-8000-000000000001', ?1, 'message', 'ASSISTANT', \
             'The reply that took three tries.', ?2, 'OPENROUTER', 'dolphin-mixtral', ?3, \
             '2026-09-01T00:01:00.000Z')",
            rusqlite::params![SRC, SRC_PART, trail],
        )
        .unwrap();
        Ok(())
    })
    .await
    .expect("seed");

    // The seed is only a proof if it actually landed.
    let seeded: Option<String> = db
        .read_main(|c| {
            Ok(c.query_row(
                "SELECT routeTrail FROM chat_messages WHERE chatId = ?1",
                [SRC],
                |r| r.get(0),
            )
            .ok())
        })
        .unwrap();
    assert!(
        seeded
            .as_deref()
            .is_some_and(|s| s.contains("dolphin-mixtral")),
        "the source message must carry a trail for this test to mean anything: {seeded:?}"
    );

    let result = quilltap_core::services::chat_continuation::apply_chat_continuation(&db, DST, SRC)
        .await
        .expect("continuation");
    assert_eq!(
        result.replayed_message_count, 1,
        "the assistant message must be replayed, else the trail's absence is vacuous"
    );

    let replayed: Vec<ReplayedRow> = db
        .read_main(|c| {
            let mut st = c
                .prepare(
                    "SELECT content, participantId, provider, routeTrail FROM chat_messages \
                     WHERE chatId = ?1 AND role = 'ASSISTANT' AND systemSender IS NULL",
                )
                .unwrap();
            let rows = st
                .query_map([DST], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            Ok(rows)
        })
        .unwrap();

    assert_eq!(
        replayed.len(),
        1,
        "one replayed assistant row: {replayed:?}"
    );
    let (content, participant_id, provider, route_trail) = &replayed[0];
    // The content IS copied and the author IS remapped — that contrast is what
    // makes the trail's absence a measurement rather than an empty projection.
    assert_eq!(content, "The reply that took three tries.");
    assert_eq!(participant_id.as_deref(), Some(DST_PART));
    assert_eq!(
        route_trail.as_deref(),
        None,
        "Continue Elsewhere must not copy routeTrail (v4 `applyChatContinuation`)"
    );
    // MEASURED, worth writing down: v4's allow-list drops `provider`/`modelName`
    // too, so a continued message shows no provider badge at all — the route
    // trail's absence is consistent with that, not an exception to it.
    assert_eq!(
        provider.as_deref(),
        None,
        "v4's projection carries neither provider nor the trail"
    );
}

/// Leg 2 — the source census.
///
/// `project_message_for_new_chat` builds a FRESH object from a named list of
/// keys, so a `routeTrail` anywhere in that file could only be a copy. Zero
/// occurrences is the assertion (the `db_error_key_guard` idiom). It exists
/// because leg 1 can rot silently: if its seed ever stops carrying a trail, the
/// absence it measures becomes vacuous, and this leg would still fail loudly on
/// a real copy.
#[test]
fn chat_continuation_never_mentions_the_route_trail() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../quilltap-core/src/services/chat_continuation.rs"
    ))
    .expect("read chat_continuation.rs");
    let hits: Vec<&str> = src
        .lines()
        .filter(|l| l.contains("routeTrail") || l.contains("route_trail"))
        .collect();
    assert!(
        hits.is_empty(),
        "`projectMessageForNewChat` is an ALLOW-LIST: a `routeTrail` in this file \
         could only be a copy, and v4 deliberately does not copy it. Found: {hits:?}"
    );
}
