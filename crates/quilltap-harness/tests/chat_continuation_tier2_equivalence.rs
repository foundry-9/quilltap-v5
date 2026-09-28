//! Tier-2 differential for Continue Elsewhere — `services::chat_continuation::
//! apply_chat_continuation` vs v4's REAL `applyChatContinuation`
//! (`lib/chat/apply-chat-continuation.ts`).
//!
//! P4.D172's OPEN item 2. Until this family existed, everything
//! `replicate_turn_state` copies — the turn queue, the spoken set, the drawn
//! rotation, the impersonation overlay, the pause state, the two participant
//! anchors — was inspection-only: no differential drove the service at all, and
//! the only behavioural pin in the tree was the route trail's ABSENCE
//! (`route_trail_continuation_guard`, which stays: its source census is the
//! anti-rot half and this family cannot replace it).
//!
//! # What is compared
//!
//! Per case, over a FRESH copy of the same two-DB fixture on each side: the
//! returned `{replayedMessageCount, hadLibrarianSummary, postedSourceTailBubble}`
//! triple, the whole `chats` and `chat_messages` tables, and a `rowid`-ordered
//! message projection. The last is the point: continuation's contract is
//! POSITIONAL — the link bubble goes into the new chat FIRST, then the replayed
//! tail, and the source's tail bubble goes in LAST — and a key-sorted table dump
//! cannot see any of that.
//!
//! # Ids
//!
//! Every id the fixture bakes stays LITERAL; only the ids each side mints are
//! remapped to `<minted-N>` in first-appearance order. That is deliberate:
//! `turnQueue`, `spokenThisCycleParticipantIds`, `cycleOrderParticipantIds`,
//! `impersonatingParticipantIds`, `lastTurnParticipantId` and
//! `activeTypingParticipantId` all carry PINNED participant ids, and whether the
//! remap put the RIGHT one there is exactly the thing a first-appearance
//! relabelling can hide (the P4.D44 `chat_template_ids` trap). The keep-list is
//! read out of the corpus itself, so a new pinned id needs no code change.
//!
//! # P4.D233 — bugs 171 + 172 (v4 `acadcc7cd`)
//!
//! The result gains `leftBehindCharacterIds` on every case, and seven planted
//! arms mirror v4's (mocked) `apply-chat-continuation.test.ts` as real-DB rows:
//! one left behind; two named with a `removed` seat skipped, an `absent` seat
//! NAMED and a duplicate seat deduped; the persona seated in the source and
//! unseated in a Salon destination (not named) or an autonomous one (named —
//! the NEW chat's identity decides); an unreadable vault (v4's WARN, skipped);
//! a poisoned post (a planted trigger — the ids are NOT stamped); nobody left
//! behind in an autonomous room. Every `[ChatContinuation]`/`[HostNotification]`
//! line is capture-pinned per case with its silence legs (`expected_lines`),
//! and five v5-side SCAN PROBES run the per-turn off-scene scan on the case's
//! copy afterwards: a stamped absentee stays quiet, an un-stamped one (the
//! poisoned post) is introduced on the next turn, the unseated persona is
//! excluded by name in a Salon room and NOT in an autonomous one (bug 172), and
//! a seated persona is excluded by id in either.
//!
//! # What it does NOT cover
//!
//! The route that ships the feature (`POST /api/v1/chats` with
//! `continuationFromChatId`): its `notFound('Source chat')` pre-check and its
//! try/catch belong to `chat_create_capstone_equivalence`, which already carries
//! `cs_continuation_bubble_before_replay`.
//!
//! Build the fixture + oracle (Node 24, from the v4 checkout; jest ignores
//! `.claude/` paths, so the case is staged in a /tmp mirror). The fixture and
//! the oracle both need v4 at or after `acadcc7cd` (since the Concierge chain
//! unified with P4.D233, a target-built `chats` reads — the ONE-pin run was
//! proven at that unification); pinned worktrees per ledger §5.1:
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-continuation-main.db \
//!   QT_FIXTURE_MOUNT_OUT=/tmp/qt-continuation-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-chat-continuation-fixture.ts
//!   TMPO=/tmp/qt-continuation-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/chat-continuation-tier2.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/chat-continuation-tier2.json"  "$TMPO/fixtures/"
//!   TZ=UTC QT_FIXTURE_CONT_MAIN=/tmp/qt-continuation-main.db \
//!   QT_FIXTURE_CONT_MOUNT=/tmp/qt-continuation-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-chat-continuation.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- "chat-continuation-tier2\.test\.ts$"
//! Run:
//!   QT_ORACLE_CHAT_CONTINUATION=/tmp/oracle-chat-continuation.ndjson \
//!   QT_FIXTURE_CONT_MAIN=/tmp/qt-continuation-main.db \
//!   QT_FIXTURE_CONT_MOUNT=/tmp/qt-continuation-mount.db \
//!     cargo test -p quilltap-harness --test chat_continuation_tier2_equivalence

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::chat_continuation::apply_chat_continuation;
use quilltap_core::services::off_scene::{scan_off_scene_newcomers, OffSceneParticipant};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

/// The corpus, embedded so the keep-list of pinned ids is derived from the same
/// bytes both sides seed from.
const SPEC: &str = include_str!("../../../harness/oracle/fixtures/chat-continuation-tier2.json");

fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("SKIP: set {key} (see test header).");
            None
        }
    }
}

fn is_uuid_at(s: &[u8], i: usize) -> bool {
    if i + 36 > s.len() {
        return false;
    }
    for (off, ch) in s[i..i + 36].iter().enumerate() {
        match off {
            8 | 13 | 18 | 23 => {
                if *ch != b'-' {
                    return false;
                }
            }
            _ => {
                if !ch.is_ascii_hexdigit() {
                    return false;
                }
            }
        }
    }
    true
}

/// Every UUID the corpus pins — kept LITERAL through normalization.
fn pinned_ids() -> HashSet<String> {
    let b = SPEC.as_bytes();
    let mut out = HashSet::new();
    let mut i = 0usize;
    while i < b.len() {
        if is_uuid_at(b, i) {
            out.insert(SPEC[i..i + 36].to_lowercase());
            i += 36;
        } else {
            i += 1;
        }
    }
    out
}

/// Canonicalize a value for comparison: JS `1.0` → `1`, then serialize with keys
/// sorted, ISO stamps sentineled, and every NON-pinned UUID relabelled
/// `<minted-N>` in first-appearance order over the serialized form (so ids
/// nested inside JSON-TEXT columns are reached too).
fn normalize(v: &Value, pinned: &HashSet<String>) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    let serialized = serde_json::to_string(&sorted(&v)).unwrap();
    let ts = regex::Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z").unwrap();
    let serialized = ts.replace_all(&serialized, "<ts>").into_owned();

    let b = serialized.as_bytes();
    let mut out = String::with_capacity(serialized.len());
    let mut map: HashMap<String, String> = HashMap::new();
    let mut i = 0usize;
    while i < b.len() {
        if is_uuid_at(b, i) {
            let candidate = &serialized[i..i + 36];
            if pinned.contains(&candidate.to_lowercase()) {
                out.push_str(candidate);
            } else {
                let n = map.len() + 1;
                let label = map
                    .entry(candidate.to_lowercase())
                    .or_insert_with(|| format!("<minted-{n}>"));
                out.push_str(label);
            }
            i += 36;
        } else {
            out.push(b[i] as char);
            i += 1;
        }
    }
    out
}

fn canon_numbers(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.fract() == 0.0 && f.abs() < 9e15 {
                    *v = json!(f as i64);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(canon_numbers),
        Value::Object(o) => o.iter_mut().for_each(|(_, x)| canon_numbers(x)),
        _ => {}
    }
}

fn sorted(v: &Value) -> Value {
    match v {
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for k in keys {
                out.insert(k.clone(), sorted(&o[k]));
            }
            Value::Object(out)
        }
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        _ => v.clone(),
    }
}

/// Sort a table's rows by an id-free key.
///
/// ⚠ Neither key may be a minted id: the replayed rows and the two Host bubbles
/// are minted afresh on each side, so an id sort orders the two dumps
/// differently the moment a table holds more than one. `chatId` IS safe — it is
/// a pinned fixture id — and it is also what breaks the tie between a replayed
/// message and the original it was copied from, which are otherwise identical in
/// role and content.
fn sort_rows(table: &str, rows: &mut [Value]) {
    let text = |r: &Value, k: &str| -> String {
        r.get(k).and_then(Value::as_str).unwrap_or("").to_string()
    };
    match table {
        "chat_messages" => rows.sort_by_key(|r| {
            (
                text(r, "chatId"),
                text(r, "systemSender"),
                text(r, "role"),
                text(r, "content"),
                text(r, "createdAt"),
            )
        }),
        "chats" => rows.sort_by_key(|r| text(r, "id")),
        _ => {}
    }
}

fn table_section(v: &Value, table: &str) -> Value {
    let mut v = v.clone();
    if let Some(rows) = v.get_mut("rows").and_then(Value::as_array_mut) {
        sort_rows(table, rows);
    }
    v
}

#[derive(serde::Deserialize)]
struct CaseW {
    name: String,
    source: String,
    destination: String,
}

#[derive(serde::Deserialize)]
struct SpecW {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "characterA")]
    character_a: String,
    cases: Vec<CaseW>,
    #[serde(rename = "scanProbeUserCharacterName")]
    scan_probe_user_character_name: String,
    #[serde(rename = "scanProbes")]
    scan_probes: Vec<ScanProbeW>,
}

/// P4.D233: a per-turn off-scene scan run on the case's DB copy AFTER the
/// continuation — v5-side (v4's scan is inline in `buildContext` and has no
/// callable seam), the expectation written from v4's `acadcc7cd` semantics and
/// held by the two mutation proofs M1/M2 (see the lane record).
#[derive(serde::Deserialize)]
struct ScanProbeW {
    #[serde(rename = "afterCase")]
    after_case: String,
    #[serde(rename = "chatId")]
    chat_id: String,
    note: String,
    expect: Option<Vec<String>>,
}

/// P4.D233: the log lines each case must (and must not) emit — v4's
/// `[ChatContinuation]` / `[HostNotification]` / resolver lines, capture-pinned
/// on the real service. A `must` entry is one or more ` && `-joined substrings
/// that must all sit on ONE captured line (the rig renders `LEVEL target
/// message field=value …`, string fields unquoted).
fn expected_lines(case: &str) -> (Vec<&'static str>, Vec<&'static str>) {
    const CHECK: &str = "[ChatContinuation] Left-behind check complete";
    const PERSONA: &str = "[ChatContinuation] Persona stays in the room unseated";
    const UNREADABLE: &str = "[ChatContinuation] Could not load a left-behind character";
    const POSTED: &str = "[HostNotification] Off-scene introduction posted";
    const SKIPPED: &str = "[HostNotification] Off-scene introduction skipped";
    const RESOLVED: &str = "Unseated persona presence resolved";
    const FAILED: &str = "[ChatContinuation] Failed to name left-behind characters";
    const COMPLETE: &str = "[ChatContinuation] Continuation complete";
    match case {
        // Early returns: no step 2b, no completion line.
        "missing_source" | "missing_destination" => {
            (vec![], vec![CHECK, COMPLETE, POSTED, RESOLVED, FAILED])
        }
        // Nobody left behind (or a row-less absentee): the check line fires
        // with an EMPTY list (trap (d)); nothing posts.
        "basic_carryover" | "librarian_anchor" | "malformed_turn_state"
        | "nobody_left_behind_autonomous" => (
            vec!["Left-behind check complete && leftBehindCharacterIdsJson=[]", COMPLETE],
            vec![POSTED, SKIPPED, PERSONA, UNREADABLE, RESOLVED, FAILED],
        ),
        // Seat B's character has no row: skipped SILENTLY; identity resolved
        // (the one user-controlled character, in a Salon room → in the room).
        "half_cast_drops" => (
            vec!["leftBehindCharacterIdsJson=[]", RESOLVED, COMPLETE],
            vec![POSTED, SKIPPED, PERSONA, UNREADABLE, FAILED],
        ),
        "left_behind_one" => (
            vec![
                "Off-scene introduction posted && reason=left-behind",
                "leftBehindCharacterIdsJson=[\"c1000000-0000-4000-8000-0000000000c1\"]",
                COMPLETE,
            ],
            vec![SKIPPED, PERSONA, UNREADABLE, FAILED],
        ),
        "left_behind_two_removed_skipped" => (
            vec![
                "characterCount=2 reason=left-behind",
                "leftBehindCharacterIdsJson=[\"c1000000-0000-4000-8000-0000000000c1\",\"c2000000-0000-4000-8000-0000000000c2\"]",
            ],
            vec![SKIPPED, PERSONA, UNREADABLE, FAILED],
        ),
        "persona_stays_unseated" => (
            vec![
                "inRoom=true",
                "Persona stays in the room unseated; not named as left behind && characterId=9e000000-0000-4000-8000-00000000009e",
                "leftBehindCharacterIdsJson=[]",
            ],
            vec![POSTED, SKIPPED, UNREADABLE, FAILED],
        ),
        "persona_named_in_autonomous_room" => (
            vec![
                "chatType=autonomous inRoom=false",
                "reason=left-behind",
                "leftBehindCharacterIdsJson=[\"9e000000-0000-4000-8000-00000000009e\"]",
            ],
            vec![PERSONA, SKIPPED, UNREADABLE, FAILED],
        ),
        "left_behind_vault_unreadable" => (
            vec![
                                "WARN && Could not load a left-behind character; not naming them && characterId=5e000000-0000-4000-8000-00000000005e",
                UNREADABLE,
                "leftBehindCharacterIdsJson=[]",
            ],
            vec![POSTED, SKIPPED, FAILED],
        ),
        // Trap (a): the post fails, the ids are NOT stamped.
        "left_behind_post_poisoned" => (
            vec![SKIPPED, "p4d233 poisoned off-scene post", "leftBehindCharacterIdsJson=[]"],
            vec![POSTED, PERSONA, UNREADABLE, FAILED],
        ),
        other => panic!("no expected log lines for case {other}"),
    }
}

#[test]
fn chat_continuation_matches_oracle() {
    let (Some(oracle_path), Some(fixture_main), Some(fixture_mount)) = (
        env_or_skip("QT_ORACLE_CHAT_CONTINUATION"),
        env_or_skip("QT_FIXTURE_CONT_MAIN"),
        env_or_skip("QT_FIXTURE_CONT_MOUNT"),
    ) else {
        return;
    };

    let spec: SpecW = serde_json::from_str(SPEC).expect("corpus parses");
    let pinned = pinned_ids();

    let oracle_text = std::fs::read_to_string(&oracle_path).expect("read oracle");
    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("oracle line parses");
        oracle.insert(
            v["name"]
                .as_str()
                .expect("oracle row has a name")
                .to_string(),
            v,
        );
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    for c in &spec.cases {
        let want = oracle
            .get(&c.name)
            .unwrap_or_else(|| panic!("oracle has no row for {}", c.name));

        let scratch = std::env::temp_dir().join(format!(
            "qt-continuation-harness-{}-{}",
            std::process::id(),
            c.name
        ));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).expect("scratch");
        let main_work = scratch.join("main.db");
        let mount_work = scratch.join("mount.db");
        std::fs::copy(&fixture_main, &main_work).expect("copy main");
        std::fs::copy(&fixture_mount, &mount_work).expect("copy mount");

        let db = Db::open(
            DbPaths {
                main: main_work.clone(),
                mount_index: Some(mount_work.clone()),
                llm_logs: None,
            },
            &spec.test_pepper_base64,
        )
        .expect("open");

        let (result, lines) = quilltap_core::test_support::captured_with(|| {
            rt.block_on(apply_chat_continuation(&db, &c.destination, &c.source))
                .expect("apply_chat_continuation")
        });

        // P4.D233: every line the case owes, and the silence legs.
        let (must, must_not) = expected_lines(&c.name);
        // `A && B` = both substrings on ONE line.
        for needle in must {
            assert!(
                lines
                    .iter()
                    .any(|l| needle.split(" && ").all(|part| l.contains(part))),
                "{}: no captured line contains {needle:?}\n{lines:#?}",
                c.name
            );
        }
        for needle in must_not {
            assert!(
                !lines.iter().any(|l| l.contains(needle)),
                "{}: a captured line unexpectedly contains {needle:?}\n{lines:#?}",
                c.name
            );
        }

        // P4.D233: the per-turn off-scene scan on the NEXT turn (bug 172's gate,
        // and trap (a)'s other half) — run on this case's DB copy.
        for probe in spec.scan_probes.iter().filter(|p| p.after_case == c.name) {
            let chat = {
                let id = probe.chat_id.clone();
                db.read_main(move |conn| quilltap_core::db::chats_read::find_by_id(conn, &id))
            }
            .expect("read probe chat")
            .expect("probe chat exists");
            let participants: Vec<OffSceneParticipant> = chat["participants"]
                .as_array()
                .expect("participants")
                .iter()
                .map(|p| OffSceneParticipant {
                    participant_type: p["type"].as_str().unwrap_or_default().to_string(),
                    character_id: p["characterId"].as_str().map(str::to_string),
                    controlled_by: p["controlledBy"].as_str().unwrap_or("llm").to_string(),
                    status: p["status"].as_str().unwrap_or("active").to_string(),
                })
                .collect();
            let (newcomers, scan_lines) = quilltap_core::test_support::captured_with(|| {
                scan_off_scene_newcomers(
                    &db,
                    &probe.chat_id,
                    &spec.user_id,
                    &spec.character_a,
                    Some(spec.scan_probe_user_character_name.as_str()),
                    &participants,
                )
            });
            let got: Option<Vec<String>> =
                newcomers.map(|cards| cards.into_iter().map(|c| c.id).collect());
            assert_eq!(
                got, probe.expect,
                "scan probe after {}: {}",
                c.name, probe.note
            );
            // The `[ContextManager]` DEBUG: `excludesPersonaByName` follows the
            // room, not the chat type alone (trap (e)).
            let autonomous = chat["chatType"].as_str() == Some("autonomous");
            let want = format!(
                "[ContextManager] Off-scene persona exclusion chatId={} chatType={} excludesPersonaByName={}",
                probe.chat_id,
                chat["chatType"].as_str().unwrap_or_default(),
                !autonomous
            );
            assert!(
                scan_lines.iter().any(|l| l.contains(&want)),
                "scan probe after {}: want {want:?} in {scan_lines:#?}",
                c.name
            );
            // Trap (e)'s other half (added at the round's unification): v4 logs
            // `Boolean(userCharacterNameLower)`, so NO persona name and a name
            // that trims EMPTY are both `false` in EVERY room — the flag follows
            // the gated name, not the room alone.
            for name in [None, Some("  ")] {
                let (_, lines) = quilltap_core::test_support::captured_with(|| {
                    scan_off_scene_newcomers(
                        &db,
                        &probe.chat_id,
                        &spec.user_id,
                        &spec.character_a,
                        name,
                        &participants,
                    )
                });
                let want = format!(
                    "[ContextManager] Off-scene persona exclusion chatId={} chatType={} excludesPersonaByName=false",
                    probe.chat_id,
                    chat["chatType"].as_str().unwrap_or_default(),
                );
                assert!(
                    lines.iter().any(|l| l.contains(&want)),
                    "scan probe after {} (name {name:?}): want {want:?} in {lines:#?}",
                    c.name
                );
            }
        }

        let dump = |t: &'static str| -> Value {
            table_section(
                &db.read_main(move |conn| dump_table_json_conn(conn, t, "id"))
                    .unwrap_or_else(|e| panic!("dump {t}: {e}")),
                t,
            )
        };

        // The insertion-ordered trace. `rowid` IS insertion order on both sides
        // (neither ever deletes a message row), so this is the position proof:
        // the ids come back in `rowid` order and the canonical row bodies come
        // from the same dump helper the table sections use, which keeps the two
        // sides' cell rendering identical by construction.
        const ORDER_COLUMNS: &[&str] = &[
            "chatId",
            "type",
            "role",
            "systemSender",
            "systemKind",
            "content",
            "participantId",
            "targetParticipantIds",
            "opaqueContent",
            "hostEvent",
            "provider",
            "modelName",
            "tokenCount",
            "routeTrail",
        ];
        let all_messages = db
            .read_main(|conn| dump_table_json_conn(conn, "chat_messages", "id"))
            .expect("dump chat_messages for the order trace");
        let by_id: HashMap<String, &Value> = all_messages["rows"]
            .as_array()
            .expect("rows")
            .iter()
            .map(|r| (r["id"].as_str().unwrap_or_default().to_string(), r))
            .collect();
        let ids_in_rowid_order: Vec<String> = db
            .read_main(|conn| {
                let mut stmt = conn.prepare("SELECT id FROM chat_messages ORDER BY rowid")?;
                let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
                let mut out = Vec::new();
                for r in rows {
                    out.push(r?);
                }
                Ok(out)
            })
            .expect("rowid order");
        let message_order = Value::Array(
            ids_in_rowid_order
                .iter()
                .map(|id| {
                    let row = by_id.get(id).expect("every rowid id is in the dump");
                    let mut o = serde_json::Map::new();
                    for col in ORDER_COLUMNS {
                        o.insert(
                            (*col).to_string(),
                            row.get(*col).cloned().unwrap_or(Value::Null),
                        );
                    }
                    Value::Object(o)
                })
                .collect::<Vec<_>>(),
        );

        let got = json!({
            "result": {
                "replayedMessageCount": result.replayed_message_count,
                "hadLibrarianSummary": result.had_librarian_summary,
                "postedSourceTailBubble": result.posted_source_tail_bubble,
                "leftBehindCharacterIds": result.left_behind_character_ids,
            },
            "tables": {
                "chats": dump("chats"),
                "chatMessages": dump("chat_messages"),
            },
            "messageOrder": message_order,
        });

        let want_cmp = json!({
            "result": want["result"],
            "tables": {
                "chats": table_section(&want["tables"]["chats"], "chats"),
                "chatMessages": table_section(&want["tables"]["chatMessages"], "chat_messages"),
            },
            "messageOrder": want["messageOrder"],
        });

        let g = normalize(&got, &pinned);
        let w = normalize(&want_cmp, &pinned);
        assert_eq!(g, w, "chat_continuation mismatch for {}", c.name);

        drop(db);
        let _ = std::fs::remove_dir_all(&scratch);
    }

    // Shape guard: a corpus that silently loses an arm must not read as green.
    assert_eq!(spec.scan_probes.len(), 5, "the P4.D233 scan probes");
    assert!(
        spec.cases.len() >= 13,
        "corpus shrank: {} cases",
        spec.cases.len()
    );
    assert!(
        oracle.len() >= spec.cases.len(),
        "stale oracle: {} rows for {} cases",
        oracle.len(),
        spec.cases.len()
    );
}
