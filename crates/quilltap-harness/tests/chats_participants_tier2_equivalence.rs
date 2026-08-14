//! Tier-2 differential test: v4's `ChatParticipantsOps` (Phase-2, the
//! conversation capstone, sub-unit 5 — `addParticipant` / `updateParticipant` /
//! `removeParticipant` / `setParticipantStatus`).
//!
//! Both sides run the SAME op sequence (`chats-participants-tier2.json`) on a
//! fresh copy of the seed fixture (four chats with pinned participants, seeded
//! via v4's real `repos.chats.create`), then the `chats` table is dumped
//! canonically and the post-op state asserted identical.
//!
//! Each op is a read-modify-write of the `participants` JSON column; the CHAT's
//! own `updatedAt` is NEVER bumped (v4 `_update` preserves it), so chat-level
//! timestamps stay at the seed sentinel and are diffed EXACTLY. Exercises:
//! `addParticipant` (an llm participant; then a user-controlled one — banking the
//! `impersonatingParticipantIds` append + `activeTypingParticipantId` side-effect),
//! `updateParticipant` (a field patch + minted participant `updatedAt`, createdAt
//! preserved), `removeParticipant` (soft-delete with a present survivor; then the
//! LAST-participant guard that throws and leaves the chat unmutated),
//! `setParticipantStatus` (to `silent` — `removedAt` cleared to explicit `null`;
//! then to `removed` — `removedAt` minted), and two not-found no-ops.
//!
//! P4.D65 (the banked P4.D63 unit-5 arm): `setParticipantStatus` is driven
//! through the NAMED repository wrapper v4 added in `d553f72a`, and its
//! `{chat, oldStatus}` return — the half no table dump can show — is compared
//! per op as `returns`, alongside the status that participant carries INSIDE
//! the returned chat (which is what tells a post-write reload apart from an
//! echo of the pre-write row). The corpus gained the archive lifecycle's two
//! directions (`active` → `absent` → `active`) and both absent legs (a missing
//! chat and a missing participant, which answer `{chat: null, oldStatus:
//! 'active'}`); coverage of both is asserted, not assumed.
//!
//! NORMALIZATION (applied identically to both dumps): participant `id`s (pinned
//! seed AND minted) are remapped to first-appearance tokens `p0`, `p1`, … in
//! dump order, with the same map rewriting `impersonatingParticipantIds` +
//! `activeTypingParticipantId`; participant `createdAt`/`updatedAt`/`removedAt`
//! values EQUAL to the seed sentinel stay pinned (diffed exactly — proves
//! createdAt preservation and no stray mint), any other value (a genuine mint) →
//! `<ts>`, and an explicit `null` (a cleared `removedAt`) stays `null`. The
//! `participants` cell is re-serialized through `serde_json::Value` (sorted
//! keys) on BOTH sides, so participant key-order is normalized away here — it is
//! banked by sub-units 1 (write) and 2 (read).
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-chatsparts-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-chats-participants-fixture.ts
//!   QT_FIXTURE_CHATSPARTS=/tmp/qt-chatsparts-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chats-participants-tier2.ts > /tmp/oracle-chatsparts.ndjson
//! Run:
//!   QT_ORACLE_CHATSPARTS=/tmp/oracle-chatsparts.ndjson \
//!   QT_FIXTURE_CHATSPARTS=/tmp/qt-chatsparts-fixture.db \
//!     cargo test -p quilltap-harness --test chats_participants_tier2_equivalence

use std::collections::BTreeMap;

use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "seedTimestamp")]
    seed_timestamp: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Op {
    #[serde(rename = "addParticipant")]
    AddParticipant {
        #[serde(rename = "chatId")]
        chat_id: String,
        participant: Value,
    },
    #[serde(rename = "updateParticipant")]
    UpdateParticipant {
        #[serde(rename = "chatId")]
        chat_id: String,
        #[serde(rename = "participantId")]
        participant_id: String,
        data: Value,
    },
    #[serde(rename = "removeParticipant")]
    RemoveParticipant {
        #[serde(rename = "chatId")]
        chat_id: String,
        #[serde(rename = "participantId")]
        participant_id: String,
        #[serde(default, rename = "expectThrow")]
        expect_throw: bool,
    },
    #[serde(rename = "setParticipantStatus")]
    SetParticipantStatus {
        #[serde(rename = "chatId")]
        chat_id: String,
        #[serde(rename = "participantId")]
        participant_id: String,
        status: String,
    },
}

/// Sentinel-aware placeholder for a participant timestamp field: keep a value
/// that equals the seed sentinel (pinned, diffed exactly); collapse any other
/// non-null string (a mint) to `<ts>`; leave `null`/absent untouched.
fn placeholder_ts(obj: &mut serde_json::Map<String, Value>, key: &str, sentinel: &str) {
    if let Some(Value::String(s)) = obj.get(key) {
        if s != sentinel {
            obj.insert(key.to_string(), Value::String("<ts>".to_string()));
        }
    }
}

/// Map a participant id to its first-appearance token, minting `p{N}` on first
/// sight.
fn token_for(id: &str, map: &mut BTreeMap<String, String>, counter: &mut usize) -> String {
    if let Some(t) = map.get(id) {
        return t.clone();
    }
    let t = format!("p{counter}");
    *counter += 1;
    map.insert(id.to_string(), t.clone());
    t
}

/// Normalize a `chats` dump in place: remap participant ids across the three
/// referencing cells and sentinel-placeholder the nested participant timestamps.
fn normalize(dump: &mut Value, sentinel: &str) {
    let mut id_map: BTreeMap<String, String> = BTreeMap::new();
    let mut counter = 0usize;
    let Some(rows) = dump.get_mut("rows").and_then(Value::as_array_mut) else {
        return;
    };
    for row in rows.iter_mut() {
        let Some(rowobj) = row.as_object_mut() else {
            continue;
        };

        // participants: remap ids + placeholder timestamps, then re-serialize.
        if let Some(Value::String(cell)) = rowobj.get("participants") {
            if let Ok(mut parts) = serde_json::from_str::<Value>(cell) {
                if let Some(arr) = parts.as_array_mut() {
                    for p in arr.iter_mut() {
                        if let Some(obj) = p.as_object_mut() {
                            if let Some(id) =
                                obj.get("id").and_then(Value::as_str).map(str::to_owned)
                            {
                                let tok = token_for(&id, &mut id_map, &mut counter);
                                obj.insert("id".to_string(), Value::String(tok));
                            }
                            placeholder_ts(obj, "createdAt", sentinel);
                            placeholder_ts(obj, "updatedAt", sentinel);
                            placeholder_ts(obj, "removedAt", sentinel);
                        }
                    }
                }
                rowobj.insert(
                    "participants".to_string(),
                    Value::String(serde_json::to_string(&parts).unwrap()),
                );
            }
        }

        // impersonatingParticipantIds: rewrite each id through the map.
        if let Some(Value::String(cell)) = rowobj.get("impersonatingParticipantIds") {
            if let Ok(Value::Array(ids)) = serde_json::from_str::<Value>(cell) {
                let mapped: Vec<Value> = ids
                    .iter()
                    .map(|v| match v.as_str() {
                        Some(s) => {
                            Value::String(id_map.get(s).cloned().unwrap_or_else(|| s.to_string()))
                        }
                        None => v.clone(),
                    })
                    .collect();
                rowobj.insert(
                    "impersonatingParticipantIds".to_string(),
                    Value::String(serde_json::to_string(&Value::Array(mapped)).unwrap()),
                );
            }
        }

        // activeTypingParticipantId: a single nullable id reference.
        if let Some(Value::String(s)) = rowobj.get("activeTypingParticipantId") {
            if let Some(tok) = id_map.get(s) {
                rowobj.insert(
                    "activeTypingParticipantId".to_string(),
                    Value::String(tok.clone()),
                );
            }
        }
    }
}

#[test]
fn chats_participants_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CHATSPARTS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CHATSPARTS to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_CHATSPARTS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CHATSPARTS to the seed fixture .db (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/chats-participants-tier2.json"),
        )
        .unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let mut oracle: Value = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle dump");

    let work = std::env::temp_dir().join(format!("qt-chatsparts-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    let mut returns: Vec<Value> = Vec::new();
    {
        let repo = writer.chat_participants();
        for (index, op) in spec.ops.iter().enumerate() {
            match op {
                Op::AddParticipant {
                    chat_id,
                    participant,
                } => {
                    repo.add_participant(chat_id, participant)
                        .expect("add_participant");
                }
                Op::UpdateParticipant {
                    chat_id,
                    participant_id,
                    data,
                } => {
                    repo.update_participant(chat_id, participant_id, data)
                        .expect("update_participant");
                }
                Op::RemoveParticipant {
                    chat_id,
                    participant_id,
                    expect_throw,
                } => {
                    let r = repo.remove_participant(chat_id, participant_id);
                    if *expect_throw {
                        assert!(r.is_err(), "expected last-participant guard to fire");
                    } else {
                        r.expect("remove_participant");
                    }
                }
                // Driven through the NAMED wrapper on the chats repository (v4
                // `d553f72a`), not the ops method underneath it: the wrapper's
                // `{chat, oldStatus}` is the half no dump can show, and the
                // archive service's participant flips are its production caller.
                Op::SetParticipantStatus {
                    chat_id,
                    participant_id,
                    status,
                } => {
                    let (chat, old_status) = writer
                        .chats()
                        .set_participant_status(chat_id, participant_id, status)
                        .expect("set_participant_status");
                    // The returned chat is RELOADED after the write, so the
                    // participant's status inside it is already the new one —
                    // recording it is what tells a reload apart from an echo of
                    // the pre-write row.
                    let status_in_returned = chat
                        .as_ref()
                        .and_then(|c| c.get("participants"))
                        .and_then(Value::as_array)
                        .and_then(|parts| {
                            parts
                                .iter()
                                .find(|p| {
                                    p.get("id").and_then(Value::as_str) == Some(participant_id)
                                })
                                .and_then(|p| p.get("status").cloned())
                        })
                        .unwrap_or(Value::Null);
                    returns.push(json!({
                        "op": index,
                        "oldStatus": old_status,
                        "chatId": chat
                            .as_ref()
                            .and_then(|c| c.get("id").cloned())
                            .unwrap_or(Value::Null),
                        "statusInReturnedChat": status_in_returned,
                    }));
                }
            }
        }
    }

    let mut got = writer.dump_table_json("chats", "id").expect("dump chats");
    let _ = std::fs::remove_file(&work);

    normalize(&mut got, &spec.seed_timestamp);
    normalize(&mut oracle["chats"], &spec.seed_timestamp);

    assert_eq!(got["table"], oracle["chats"]["table"], "chats: table name");
    assert_eq!(
        got["columns"], oracle["chats"]["columns"],
        "chats: column set / order"
    );
    assert_eq!(
        got["rows"], oracle["chats"]["rows"],
        "chats: row state diverged\n  rust:   {}\n  oracle: {}",
        got["rows"], oracle["chats"]["rows"]
    );

    // Guard against an empty-vs-empty false pass.
    assert_ne!(got["rows"], json!([]), "expected non-empty chats dump");

    // The `{chat, oldStatus}` wrapper shape, per op.
    assert_eq!(
        Value::Array(returns.clone()),
        oracle["returns"],
        "setParticipantStatus return shape diverged\n  rust:   {}\n  oracle: {}",
        Value::Array(returns.clone()),
        oracle["returns"],
    );
    // Coverage, asserted rather than assumed: the corpus must drive BOTH
    // absent legs (a missing chat and a missing participant, which answer
    // `{chat: null, oldStatus: 'active'}`) and at least one real flip in each
    // direction — the two the archive lifecycle performs. Without them the
    // comparand above would pass on a wrapper that never reloaded and never
    // reported an old status but `active`.
    let flipped_from: BTreeMap<String, usize> =
        returns.iter().fold(BTreeMap::new(), |mut acc, r| {
            let key = r["oldStatus"].as_str().unwrap_or_default().to_string();
            *acc.entry(key).or_insert(0) += 1;
            acc
        });
    assert!(
        returns.iter().filter(|r| r["chatId"].is_null()).count() >= 2,
        "the corpus must drive both absent legs of setParticipantStatus; got {returns:?}"
    );
    assert!(
        flipped_from.len() >= 2,
        "the corpus must observe more than one OLD status, or `oldStatus` is \
         indistinguishable from its default; got {flipped_from:?}"
    );

    eprintln!("OK: chats participants tier-2 matched oracle.");
}
