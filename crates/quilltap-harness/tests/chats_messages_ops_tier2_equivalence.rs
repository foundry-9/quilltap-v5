//! Tier-2 differential test: v4's `ChatMessagesOps` mutation path (Phase-2, the
//! conversation capstone, sub-unit 4b — `updateMessage` / `deleteMessagesByIds` /
//! `clearMessages`).
//!
//! Both sides run the SAME op sequence (`chats-messages-ops-tier2.json`) on a
//! fresh copy of the seed fixture (three chats pre-seeded with messages via v4's
//! real `addMessages`), then BOTH the `chat_messages` and `chats` tables are
//! dumped canonically and the post-op state is asserted identical.
//!
//! Exercises: `updateMessage` (a message — scalar + number + a freshly-added
//! `dangerFlags` JSON column whose defaults bake, while the untouched
//! `reasoningSegments` round-trips byte-for-byte; a context-summary's `context`
//! edit; and a not-found id that no-ops); `deleteMessagesByIds` (remove two of
//! three, recount messageCount; then a nonexistent id that removes nothing and
//! leaves metadata untouched); `clearMessages` (delete all, messageCount→0,
//! lastMessageAt→null, updatedAt preserved).
//!
//! NORMALIZATION: NONE. The seed's minted timestamps are baked once and read by
//! both sides, and no 4b op mints a new chat timestamp, so every cell is pinned.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-chatsmsgops-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-chats-messages-ops-fixture.ts
//!   QT_FIXTURE_CHATSMSGOPS=/tmp/qt-chatsmsgops-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chats-messages-ops-tier2.ts > /tmp/oracle-chatsmsgops.ndjson
//! Run:
//!   QT_ORACLE_CHATSMSGOPS=/tmp/oracle-chatsmsgops.ndjson \
//!   QT_FIXTURE_CHATSMSGOPS=/tmp/qt-chatsmsgops-fixture.db \
//!     cargo test -p quilltap-harness --test chats_messages_ops_tier2_equivalence

use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Op {
    #[serde(rename = "updateMessage")]
    UpdateMessage {
        #[serde(rename = "chatId")]
        chat_id: String,
        #[serde(rename = "messageId")]
        message_id: String,
        updates: Value,
    },
    #[serde(rename = "deleteMessagesByIds")]
    DeleteMessagesByIds {
        #[serde(rename = "chatId")]
        chat_id: String,
        #[serde(rename = "messageIds")]
        message_ids: Vec<String>,
    },
    #[serde(rename = "clearMessages")]
    ClearMessages {
        #[serde(rename = "chatId")]
        chat_id: String,
    },
    // ── P4.D183: the rest of the funnel, plus the two must-NOT-bump arms ──
    // ── P4.109: P4.105's finding 1, a planted NULL-content row + a read ──
    #[serde(rename = "plantNullContent")]
    PlantNullContent {
        #[serde(rename = "messageId")]
        message_id: String,
    },
    // ── P4.112: v4's Zod-only row failures, one planted cell each ──
    #[serde(rename = "plantCell")]
    PlantCell {
        #[serde(rename = "messageId")]
        message_id: String,
        column: String,
        value: String,
    },
    #[serde(rename = "getMessages")]
    GetMessages {
        #[serde(rename = "chatId")]
        chat_id: String,
    },
    #[serde(rename = "addMessage")]
    AddMessage {
        #[serde(rename = "chatId")]
        chat_id: String,
        message: Value,
    },
    #[serde(rename = "addMessages")]
    AddMessages {
        #[serde(rename = "chatId")]
        chat_id: String,
        messages: Vec<Value>,
    },
    #[serde(rename = "replaceInMessages")]
    ReplaceInMessages {
        #[serde(rename = "chatId")]
        chat_id: String,
        #[serde(rename = "searchText")]
        search_text: String,
        #[serde(rename = "replaceText")]
        replace_text: String,
    },
    #[serde(rename = "chatUpdate")]
    ChatUpdate {
        #[serde(rename = "chatId")]
        chat_id: String,
        data: Value,
    },
}

// **A v4 BUG this port found — and v4 has now CONVERGED onto v5** (P4.D183 →
// P4.D191). v4's `deleteMessagesByIds` used to count a requested id as removed
// whether or not it existed:
//
// ```js
// const result = await messagesCollection.deleteOne({ id: messageId, chatId });
// if (typeof result === 'number') { removed += result; }
// else if (result) { removed += 1; }          // ← an OBJECT is always truthy
// ```
//
// The real SQLite backend answers a miss with `{ deletedCount: 0, acknowledged:
// true }` (`backends/sqlite/backend.ts:289`) — never a number — so the second
// branch fired and `removed` ended up as `messageIds.length`. v4 then took its
// `removed > 0` path: it rewrote `messageCount`/`lastMessageAt` and, since
// `5029075bb`, BUMPED the transcript counter and published a hint for a delete
// that deleted nothing. v4's own unit suite could not see it —
// `chats-messages-transcript-version.test.ts:57` mocked `deleteOne` to return a
// NUMBER, taking the one branch where the arithmetic is right — so it took this
// real-DB oracle to expose it, and it only became VISIBLE once the counter
// arrived (`messageCount`/`lastMessageAt` are recomputed from what survives and
// land on the same values either way).
//
// v5 was left correct, and the divergence was pinned in BOTH directions as
// `DELETE_MISS_DIVERGENCE` (chat `c0000020-…`: v5 wrote `transcriptVersion` 2,
// v4 wrote 3) so it could not drift. **Filed upstream as v4 bug 142
// (`364b04ac4`) and FIXED at v4 `ffb6b3119`** — `removed += result.deletedCount`,
// both stale branches deleted. The tripwire fired by name on the first regen at
// that pin, which is the tripwire working (drift-ledger §5.4).
//
// **The pin is RETIRED to a plain equality** (P4.D191), on a measured TOTAL
// convergence rather than an assumed one: the same seed fixture run through v4
// at `31436bae4` and at `ffb6b3119` differs in exactly ONE of 1,513 cells
// across both dumped tables — this chat's `transcriptVersion`, 3 → 2 — plus the
// two wall-clock cells `ADD_MINTS_TIMESTAMPS_ON` already carves out. So the
// cell now COMPARES: a v5 regression that counts a miss again reds this test.
// The mint carve-out below stays (v4 `5029075bb` is still this family's one
// minting op).

/// The `addMessage` / `addMessages` ops MINT `updatedAt` / `lastMessageAt`
/// from the wall clock, so those two cells cannot be compared on the chat they
/// touch — the rest of the corpus keeps this family's zero-normalization
/// property (v4 `5029075bb` is the first change to put a minting op in it).
/// Both sides are asserted to be a plausible ISO timestamp rather than simply
/// dropped, so a NULL or an empty string still reds.
const ADD_MINTS_TIMESTAMPS_ON: &str = "c0000060-0000-4000-8000-000000000001";

/// The chat whose delete-miss op used to be `DELETE_MISS_DIVERGENCE` (retired
/// at P4.D191 when v4 converged at `ffb6b3119`). Its `transcriptVersion` is
/// the cell that now DISCRIMINATES a v5 regression that counts a miss again —
/// but only while the chat and its miss op stay in the corpus, so its presence
/// is pinned exactly as the mint carve-out's is: an equality whose row has left
/// the corpus is measuring nothing (the `ffb6b3119` round's §3 review).
const CONVERGED_DELETE_MISS_CHAT: &str = "c0000020-0000-4000-8000-000000000001";

/// Apply the ONE remaining carve-out to a `chats` dump, asserting it is REAL
/// (present and of the expected shape) before neutralizing it — a carve-out
/// nothing exercises is a hole, not an exemption. (Its sibling,
/// `DELETE_MISS_DIVERGENCE`, retired above when v4 converged at `ffb6b3119`;
/// that chat's `transcriptVersion` is now compared like any other cell.)
fn apply_chats_carve_outs(got: &mut Value, oracle: &mut Value) {
    let mut saw_mint = false;
    let mut saw_converged = false;
    for (side, dump) in [("rust", &mut *got), ("oracle", &mut *oracle)] {
        let Some(rows) = dump.get_mut("rows").and_then(Value::as_array_mut) else {
            continue;
        };
        for row in rows {
            let Some(o) = row.as_object_mut() else {
                continue;
            };
            let id = o
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if id == CONVERGED_DELETE_MISS_CHAT {
                assert!(
                    o.get("transcriptVersion").and_then(Value::as_i64).is_some(),
                    "[{side}] the converged delete-miss chat carries no integer \
                     transcriptVersion — the cell the retirement made comparable"
                );
                saw_converged = true;
            }
            if id == ADD_MINTS_TIMESTAMPS_ON {
                for key in ["updatedAt", "lastMessageAt"] {
                    let v = o.get(key).and_then(Value::as_str).unwrap_or("");
                    assert!(
                        v.len() == 24 && v.ends_with('Z'),
                        "[{side}] {key} on the add chat should be a minted ISO                          timestamp, got {v:?} — the carve-out must not hide a NULL"
                    );
                    o.insert((*key).into(), Value::String("<minted>".into()));
                }
                saw_mint = true;
            }
        }
    }
    assert!(
        saw_mint && saw_converged,
        "the mint carve-out row and the converged delete-miss chat must both be \
         PRESENT in the dump (mint: {saw_mint}, converged: {saw_converged}) — a \
         carve-out or an equality whose row has left the corpus is measuring \
         nothing"
    );
}

fn assert_dump_eq(got: &Value, oracle: &Value, label: &str) {
    assert_eq!(got["table"], oracle["table"], "{label}: table name");
    assert_eq!(
        got["columns"], oracle["columns"],
        "{label}: column set / order"
    );
    assert_eq!(
        got["rows"], oracle["rows"],
        "{label}: row state diverged\n  rust:   {}\n  oracle: {}",
        got["rows"], oracle["rows"]
    );
}

#[test]
fn chats_messages_ops_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CHATSMSGOPS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CHATSMSGOPS to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_CHATSMSGOPS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CHATSMSGOPS to the seed fixture .db (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/chats-messages-ops-tier2.json"),
        )
        .unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle: Value = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle dump");

    let work = std::env::temp_dir().join(format!("qt-chatsmsgops-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    // P4.109: each `updateMessage` op's return (`null` ↔ `Ok(false)`, the
    // event's id ↔ `Ok(true)`, an `Err` recorded as a string) and its
    // ERROR/WARN lines, in op order — compared against `updateReturns`.
    let mut update_returns: Vec<(Value, Vec<String>)> = Vec::new();
    // P4.109: each `getMessages` op's ids and ERROR/WARN lines.
    let mut reads: Vec<(String, Value, Vec<String>)> = Vec::new();
    {
        let repo = writer.chat_messages();
        for op in &spec.ops {
            match op {
                Op::UpdateMessage {
                    chat_id,
                    message_id,
                    updates,
                } => {
                    let (got, lines) = quilltap_core::test_support::captured_with(|| {
                        repo.update_message(chat_id, message_id, updates)
                    });
                    // v4 returns the VALIDATED event and the oracle records
                    // its id — which an update carrying `id` (P4.113's
                    // non-uuid repair) moves; v5 answers `bool`, so the id
                    // the write left is the update's, else the op's.
                    let written_id = updates
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or(message_id);
                    let returned = match got {
                        Ok(true) => Value::String(written_id.to_string()),
                        Ok(false) => Value::Null,
                        Err(e) => Value::String(format!("Err: {e}")),
                    };
                    let lines = lines
                        .into_iter()
                        .filter(|l| l.starts_with("ERROR ") || l.starts_with("WARN "))
                        .collect();
                    update_returns.push((returned, lines));
                }
                Op::DeleteMessagesByIds {
                    chat_id,
                    message_ids,
                } => {
                    repo.delete_messages_by_ids(chat_id, message_ids)
                        .expect("delete_messages_by_ids");
                }
                Op::PlantNullContent { message_id } => {
                    writer
                        .connection()
                        .execute(
                            "UPDATE chat_messages SET content = NULL WHERE id = ?1",
                            [message_id],
                        )
                        .expect("plant the NULL content");
                }
                Op::PlantCell {
                    message_id,
                    column,
                    value,
                } => {
                    // The oracle's closed set — the name is spliced into SQL.
                    assert!(
                        ["id", "role", "hostEvent", "createdAt", "participantId"]
                            .contains(&column.as_str()),
                        "plantCell: unplantable column {column}"
                    );
                    writer
                        .connection()
                        .execute(
                            &format!("UPDATE chat_messages SET \"{column}\" = ?1 WHERE id = ?2"),
                            [value, message_id],
                        )
                        .expect("plant the cell");
                }
                Op::GetMessages { chat_id } => {
                    let (got, lines) = quilltap_core::test_support::captured_with(|| {
                        quilltap_core::db::chats_messages_read::get_messages(
                            writer.connection(),
                            chat_id,
                        )
                    });
                    let ids: Vec<Value> = got
                        .expect("get_messages never answers Err")
                        .iter()
                        .map(|e| e["id"].clone())
                        .collect();
                    let lines = lines
                        .into_iter()
                        .filter(|l| l.starts_with("ERROR ") || l.starts_with("WARN "))
                        .collect();
                    reads.push((chat_id.clone(), Value::Array(ids), lines));
                }
                Op::ClearMessages { chat_id } => {
                    repo.clear_messages(chat_id).expect("clear_messages");
                }
                Op::AddMessage { chat_id, message } => {
                    let e = serde_json::from_value(message.clone()).expect("parse add message");
                    repo.add_message(chat_id, &e).expect("add_message");
                }
                Op::AddMessages { chat_id, messages } => {
                    let es: Vec<_> = messages
                        .iter()
                        .map(|m| serde_json::from_value(m.clone()).expect("parse add messages"))
                        .collect();
                    repo.add_messages(chat_id, &es).expect("add_messages");
                }
                Op::ReplaceInMessages {
                    chat_id,
                    search_text,
                    replace_text,
                } => {
                    quilltap_core::db::chats_search::ChatSearchRepository::new(writer.connection())
                        .replace_in_messages(chat_id, search_text, replace_text)
                        .expect("replace_in_messages");
                }
                Op::ChatUpdate { chat_id, data } => {
                    // `ChatUpdate` is a hand-built setter struct, not a
                    // Deserialize target (the `double_option` tri-states make
                    // a derive the wrong shape), so the spec's two keys are
                    // read explicitly. Keep this in step with the spec's
                    // `chatUpdate` op if it grows a field.
                    let patch = quilltap_core::db::chats::ChatUpdate {
                        title: data
                            .get("title")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        message_count: data.get("messageCount").and_then(Value::as_f64),
                        ..Default::default()
                    };
                    assert!(
                        patch.title.is_some() && patch.message_count.is_some(),
                        "the chatUpdate op must actually carry both keys, or the \
                         must-NOT-bump arm proves nothing: {data}"
                    );
                    quilltap_core::db::chats::ChatsRepository::new(writer.connection())
                        .update(chat_id, &patch)
                        .expect("chats update");
                }
            }
        }
    }

    let got_messages = writer
        .dump_table_json("chat_messages", "id")
        .expect("dump chat_messages");
    let got_chats = writer.dump_table_json("chats", "id").expect("dump chats");
    let _ = std::fs::remove_file(&work);

    assert_update_returns(&update_returns, &oracle["updateReturns"]);
    assert_reads(&reads, &oracle["reads"]);
    assert_dump_eq(&got_messages, &oracle["messages"], "chat_messages");
    let mut got_chats = got_chats;
    let mut want_chats = oracle["chats"].clone();
    apply_chats_carve_outs(&mut got_chats, &mut want_chats);
    assert_dump_eq(&got_chats, &want_chats, "chats");

    eprintln!("OK: chats messages ops tier-2 matched oracle.");
}

/// P4.109: v4's `updateMessage` is a FALLBACK `safeQuery` answering `null` —
/// the `content: 42` op fails `ChatEventSchema.parse` and logs ONE `Failed to
/// update message in chat {chatId, messageId}`; every other op is a silence
/// leg.
///
/// P4.113: v4's find is ONE raw row (`findOne`, no Zod), and the parse runs
/// on the MERGED event. So after the plants: a healthy sibling's update in
/// the corrupted chat is SILENT (the whole-chat read v5 had emitted every
/// corrupted sibling's WARN); for each planted shape (`role`, the non-uuid
/// `id`, `hostEvent: 42`, a no-seconds `createdAt`, a `yesterday`
/// context-summary `createdAt`, a non-uuid `participantId`) an update that
/// leaves the field alone ERRORs and answers `null`, and one that REPAIRS it
/// writes (the dumps and the final read show it) — where v5 had answered
/// both with the skip WARN and `Ok(false)`. Per `updateMessage` op, in order: the return, then the lines by level,
/// message and the two context fields.
fn assert_update_returns(got: &[(Value, Vec<String>)], want: &Value) {
    let want = want
        .as_array()
        .expect("the oracle carries updateReturns — regenerate it (P4.109)");
    assert_eq!(
        got.len(),
        want.len(),
        "updateMessage op count: rust vs oracle"
    );
    // P4.113: every op is checked and EVERY mismatching op is reported (not
    // just the first), so a red-first run counts its reds.
    let mut reds: Vec<String> = Vec::new();
    for (i, ((returned, lines), w)) in got.iter().zip(want).enumerate() {
        if let Err(e) = check_update_return(i, returned, lines, w) {
            reds.push(e);
        }
    }
    assert!(
        reds.is_empty(),
        "{} updateMessage op(s) differ from v4:\n{}",
        reds.len(),
        reds.join("\n")
    );
}

/// One `updateMessage` op against v4: the return, then the lines by level,
/// message and the two context fields.
fn check_update_return(
    i: usize,
    returned: &Value,
    lines: &[String],
    w: &Value,
) -> Result<(), String> {
    if returned != &w["returned"] {
        return Err(format!(
            "op {i} ({}): return — v4 {} rust {returned} (v4 `null` is v5 `Ok(false)`)",
            w["messageId"], w["returned"]
        ));
    }
    let w_logs = w["logs"].as_array().expect("logs");
    if lines.len() != w_logs.len() {
        return Err(format!(
            "op {i} ({}): ERROR/WARN lines — v4 {w_logs:?} rust {lines:?}",
            w["messageId"]
        ));
    }
    for (g, wl) in lines.iter().zip(w_logs) {
        let level = wl["level"].as_str().unwrap().to_uppercase();
        if !g.starts_with(&format!("{level} quilltap::db")) {
            return Err(format!("op {i}: level/target: {g}"));
        }
        if !g.contains(wl["message"].as_str().unwrap()) {
            return Err(format!("op {i}: {wl} vs {g}"));
        }
        for key in ["chatId", "messageId"] {
            if let Some(v) = wl[key].as_str() {
                if !g.contains(&format!("{key}={v}")) {
                    return Err(format!("op {i}: {key}: {g}"));
                }
            }
        }
    }
    Ok(())
}

/// P4.109 / P4.105's finding 1: a NULL-content row is SKIPPED by v4's per-row
/// `ChatEventSchema.safeParse` with WARN `Skipping corrupted chat message
/// {chatId, messageId, messageType}` — the rest of the chat still reads (and
/// the replace after it still lands, which the `chat_messages` dump holds).
///
/// P4.112: the second read covers the Zod-only failures a cell that still
/// marshals can carry — a `role` outside `RoleEnum`, a non-uuid `id` (whose
/// WARN names that id), a `hostEvent` of `42`, and a `hostEvent` object with a
/// `toStatus` outside its enum — each skipped with the same WARN, in row
/// order; a well-formed `hostEvent` and an untouched row are kept.
///
/// P4.113: the same read also skips a `createdAt` that fails zod 4.6.5's
/// `z.iso.datetime()` — no seconds, an offset, `yesterday` — on a message, a
/// context-summary AND a system row, and a non-uuid message `participantId`;
/// a well-formed `participantId` is kept.
fn assert_reads(got: &[(String, Value, Vec<String>)], want: &Value) {
    let want = want
        .as_array()
        .expect("the oracle carries reads — regenerate it (P4.109)");
    assert_eq!(
        got.len(),
        want.len(),
        "getMessages op count: rust vs oracle"
    );
    for (i, ((chat_id, ids, lines), w)) in got.iter().zip(want).enumerate() {
        assert_eq!(
            chat_id.as_str(),
            w["chatId"].as_str().unwrap(),
            "read {i}: chat"
        );
        assert_eq!(ids, &w["ids"], "read {i}: the ids v4's getMessages keeps");
        let w_logs = w["logs"].as_array().expect("logs");
        assert_eq!(
            lines.len(),
            w_logs.len(),
            "read {i}: ERROR/WARN lines — v4 {w_logs:?} rust {lines:?}"
        );
        for (g, wl) in lines.iter().zip(w_logs) {
            let level = wl["level"].as_str().unwrap().to_uppercase();
            assert!(
                g.starts_with(&format!("{level} quilltap::db")),
                "read {i}: {g}"
            );
            assert!(
                g.contains(wl["message"].as_str().unwrap()),
                "read {i}: {wl} vs {g}"
            );
            for key in ["chatId", "messageId", "messageType"] {
                if let Some(v) = wl[key].as_str() {
                    assert!(g.contains(&format!("{key}={v}")), "read {i}: {key}: {g}");
                }
            }
        }
    }
}
