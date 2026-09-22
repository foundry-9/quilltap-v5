//! Route-surface differential: the three Inform verbs (P4.D205, v4 `e7d77bb60`).
//!
//! The oracle (`harness/oracle/cases/chat-informs-routes.test.ts`) drives v4's
//! REAL `handleInform` / `handleGetInforms` / `handleCancelInform`, mocking
//! exactly what v4's own test mocks — the repos, the announcer writer, the
//! audience resolver, the realtime bus. v5's handlers take a real `Db`, so this
//! side runs them against a fixture seeded to the equivalent state (the same
//! room: two LLM seats, one of them SILENT, a user seat, a departed seat).
//!
//! **Two comparands per case**, because the body alone cannot see the rule that
//! matters:
//!
//!   - **status + body** — v4's bytes, verbatim, including the two 400 sentences
//!     with the offending ids joined.
//!   - **effects** — v4's mocked-collaborator calls, mapped onto the state v5
//!     actually leaves behind. `recordTargets` is the heart of it: the COVERAGE
//!     rule decides public-vs-whisper, and nothing in the response body shows
//!     which was chosen for a full explicit list.
//!
//! The mapping is exact rather than approximate:
//!
//! | v4 mock call | v5 observed effect |
//! |---|---|
//! | `postInformRecord({targetParticipantIds})` | the record message row's `targetParticipantIds` |
//! | `postInformRecord({contentMarkdown})` | the record message row's `content` |
//! | `createBatch({participantIds})` | the created rows' `participantId`, in order |
//! | `createBatch({contentMarkdown})` | the created rows' `contentMarkdown` |
//! | `createBatch({recordMessageId})` | the created rows' `recordMessageId` (tokenized) |
//! | `chats.deleteMessagesByIds` called | the record message row is GONE |
//!
//! `batchId` and the record message id are minted on both sides, so both are
//! tokenized before the diff; every other byte is compared exactly.
//!
//! Generate the oracle (Node 24, from a TARGET-pinned v4 worktree; jest ignores
//! `.claude/` paths, so the case is copied to a /tmp mirror first):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
//!   TMPO=/tmp/qt-informs-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
//!   cp "$V5W/harness/oracle/cases/chat-informs-routes.test.ts" "$TMPO/cases/"
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-chat-informs-routes.ndjson TZ=UTC \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- "chat-informs-routes\.test\.ts$"
//! And the fixture:
//!   QT_FIXTURE_OUT=/tmp/qt-chat-informs-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-chat-informs-fixture.ts
//! Run:
//!   QT_ORACLE_CHAT_INFORMS_ROUTES=/tmp/oracle-chat-informs-routes.ndjson \
//!   QT_FIXTURE_CHAT_INFORMS=/tmp/qt-chat-informs-fixture.db \
//!     cargo test -p quilltap-harness --test chat_informs_routes_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::chat_informs;
use quilltap_core::api::types::Response;
use quilltap_core::db::runtime::{Db, DbPaths};
use serde_json::{json, Value};

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/chat-informs-tier2.json")
}

/// Open a FRESH copy of the fixture per case — every case starts from the same
/// room, exactly as the oracle's `beforeEach` rebuilds its mocks.
fn fresh_db(fixture: &str, pepper: &str, tag: &str) -> (Db, PathBuf) {
    let scratch =
        std::env::temp_dir().join(format!("qt-informs-routes-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    std::fs::copy(fixture, &main).expect("copy fixture");
    let db = Db::open(
        DbPaths {
            main,
            mount_index: None,
            llm_logs: None,
        },
        pepper,
    )
    .expect("open fixture copy");
    (db, scratch)
}

fn body_of(resp: &Response) -> (u16, Value) {
    let v = serde_json::to_value(resp).expect("serialize response");
    // The dispatch envelope is adjacently tagged; the oracle compares the HTTP
    // status + the JSON body v4's route answered, so map the core response onto
    // that pair the way the transports do.
    match resp {
        Response::Error(e) => {
            let status = match e.kind {
                quilltap_core::api::types::ErrorKind::BadRequest => 400,
                quilltap_core::api::types::ErrorKind::NotFound => 404,
                _ => 500,
            };
            (status, json!({ "error": e.message }))
        }
        Response::ChatInform(b) => (201, b.clone()),
        Response::ChatInforms(b) => (200, b.clone()),
        Response::ChatInformCancelled(b) => (200, b.clone()),
        _ => panic!("unexpected response variant: {v}"),
    }
}

/// Replace every minted id with a first-seen token, in traversal order.
struct Tokens {
    known: std::collections::HashSet<String>,
    map: HashMap<String, String>,
}

impl Tokens {
    fn walk(&mut self, v: &mut Value) {
        match v {
            Value::String(s) => {
                let is_uuid = s.len() == 36
                    && s.as_bytes().iter().enumerate().all(|(i, c)| match i {
                        8 | 13 | 18 | 23 => *c == b'-',
                        _ => c.is_ascii_hexdigit(),
                    });
                // v4's mocked ids are NOT uuid-shaped (`row-1`, `batch-gone`),
                // so they are tokenized by an explicit list instead.
                if (is_uuid && !self.known.contains(s.as_str()))
                    || s.starts_with("row-")
                    || s == "batch-gone"
                {
                    let next = format!("ID_{}", self.map.len());
                    let t = self.map.entry(s.clone()).or_insert(next).clone();
                    *v = Value::String(t);
                }
            }
            Value::Array(a) => a.iter_mut().for_each(|x| self.walk(x)),
            Value::Object(o) => o.values_mut().for_each(|x| self.walk(x)),
            _ => {}
        }
    }
}

#[test]
fn chat_informs_routes_match_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CHAT_INFORMS_ROUTES") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CHAT_INFORMS_ROUTES to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_CHAT_INFORMS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CHAT_INFORMS to the seed fixture .db (see header).");
            return;
        }
    };

    let spec: Value =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).expect("read spec"))
            .expect("parse spec");
    let r = &spec["routes"];
    let pepper = spec["testPepperBase64"].as_str().unwrap().to_string();
    let g = |k: &str| r[k].as_str().unwrap().to_string();
    let (chat, alice, bob, operator, departed, stranger, body) = (
        g("chatId"),
        g("alice"),
        g("bob"),
        g("operator"),
        g("departed"),
        g("stranger"),
        g("body"),
    );
    let missing_chat = "3f1c9f4a-1111-4a2b-9c3d-0000000000ff".to_string();

    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .expect("read oracle")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse oracle row"))
        .collect();
    assert!(!oracle.is_empty(), "oracle is EMPTY — the regen failed");
    let by_label: HashMap<String, &Value> = oracle
        .iter()
        .map(|o| (o["label"].as_str().unwrap_or_default().to_string(), o))
        .collect();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    // Every deterministic id the corpus names is compared EXACTLY; anything else
    // is minted and gets tokenized.
    let known: std::collections::HashSet<String> = [
        &chat,
        &alice,
        &bob,
        &operator,
        &departed,
        &stranger,
        &missing_chat,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    let mut checked = 0usize;
    let mut compare = |label: &str, status: u16, body: Value, effects: Value| {
        let want = by_label
            .get(label)
            .unwrap_or_else(|| panic!("oracle has no case labelled `{label}`"));

        let mut got =
            json!({ "status": status, "body": collapse_message(body), "effects": effects });
        let mut exp = json!({
            "status": want["status"],
            "body": collapse_message(want["body"].clone()),
            "effects": oracle_effects(&want["calls"]),
        });
        let mut t1 = Tokens {
            known: known.clone(),
            map: HashMap::new(),
        };
        let mut t2 = Tokens {
            known: known.clone(),
            map: HashMap::new(),
        };
        t1.walk(&mut got);
        t2.walk(&mut exp);
        assert_eq!(
            got, exp,
            "[{label}] diverged from v4\n  rust:   {got}\n  oracle: {exp}"
        );
        checked += 1;
    };

    // ---- POST ?action=inform ------------------------------------------------

    {
        let (db, _s) = fresh_db(&fixture, &pepper, "inform404");
        let resp = rt.block_on(chat_informs::chat_inform(
            &db,
            &missing_chat,
            &Some(Some(json!(body))),
            &Some(None),
        ));
        let (st, b) = body_of(&resp);
        compare("inform: 404s when the chat is gone", st, b, effects_none());
    }

    for (label, targets) in [
        (
            "inform: null targets reach every eligible seat, public record",
            Value::Null,
        ),
        (
            "inform: a FULL explicit list is still PUBLIC",
            json!([alice, bob]),
        ),
        ("inform: a subset whispers", json!([alice])),
    ] {
        let (db, _s) = fresh_db(&fixture, &pepper, "inform_ok");
        let t = if targets.is_null() {
            Some(None)
        } else {
            Some(Some(targets.clone()))
        };
        let resp = rt.block_on(chat_informs::chat_inform(
            &db,
            &chat,
            &Some(Some(json!(body))),
            &t,
        ));
        let (st, b) = body_of(&resp);
        let eff = observed_effects(&db, &chat, b["batchId"].as_str());
        compare(label, st, b, eff);
    }

    for (label, targets) in [
        ("inform: 400 on an unknown target", json!([stranger])),
        ("inform: 400 on a user-controlled seat", json!([operator])),
        // A REMOVED seat is deliberately NOT a case here — see the oracle's own
        // note: v4 mocks the audience resolver, so through that seam a departed
        // seat reaches the eligibility gate, while v4's REAL resolver reports it
        // as an UNKNOWN target. Measuring it would pin the mock.
    ] {
        let (db, _s) = fresh_db(&fixture, &pepper, "inform_400");
        let resp = rt.block_on(chat_informs::chat_inform(
            &db,
            &chat,
            &Some(Some(json!(body))),
            &Some(Some(targets)),
        ));
        let (st, b) = body_of(&resp);
        // A refusal creates nothing, so there is no batch to scope to.
        compare(label, st, b, effects_none());
    }

    {
        // A room with nobody an LLM speaks for: the departed + user seats only.
        let (db, _s) = fresh_db(&fixture, &pepper, "inform_noseat");
        strip_llm_seats(&db, &chat, &[alice.clone(), bob.clone()]);
        let resp = rt.block_on(chat_informs::chat_inform(
            &db,
            &chat,
            &Some(Some(json!(body))),
            &Some(None),
        ));
        let (st, b) = body_of(&resp);
        compare("inform: 400 when no LLM seat exists", st, b, effects_none());
    }

    {
        let (db, _s) = fresh_db(&fixture, &pepper, "inform_trim");
        let resp = rt.block_on(chat_informs::chat_inform(
            &db,
            &chat,
            &Some(Some(json!(format!("  {body}  \n")))),
            &Some(None),
        ));
        let (st, b) = body_of(&resp);
        let eff = observed_effects(&db, &chat, b["batchId"].as_str());
        compare("inform: the body is trimmed for the rows", st, b, eff);
    }

    // ---- GET ?action=informs ------------------------------------------------

    {
        let (db, _s) = fresh_db(&fixture, &pepper, "informs404");
        let resp = rt.block_on(chat_informs::chat_informs_list(&db, &missing_chat));
        let (st, b) = body_of(&resp);
        compare("informs: 404s when the chat is gone", st, b, effects_none());
    }

    // ---- POST ?action=cancel-inform -----------------------------------------

    {
        let (db, _s) = fresh_db(&fixture, &pepper, "cancel404");
        let resp = rt.block_on(chat_informs::chat_inform_cancel(
            &db,
            &chat,
            &Some(Some(json!("3f1c9f4a-1111-4a2b-9c3d-0000000000aa"))),
        ));
        let (st, b) = body_of(&resp);
        compare("cancel: 404 on an unknown batch", st, b, effects_none());
    }

    // ---- GET ?action=informs, over the room's OWN pending rows ---------------
    //
    // v4's GET cases mock `findPendingBatches` per case; v5 reads the whole chat,
    // so the fixture seeds the RICHER of v4's two shapes — one batch owed to a
    // live seat and a departed one, plus a batch owed only to the departed seat.
    // v4's simpler "returns the pending batches" case is a strict subset of it
    // (same fold, no filtering), so it is not seeded twice.
    {
        let (db, _s) = fresh_db(&fixture, &pepper, "informs_list");
        let resp = rt.block_on(chat_informs::chat_informs_list(&db, &chat));
        let (st, b) = body_of(&resp);
        compare(
            "informs: departed seats filtered, empty batch dropped",
            st,
            b,
            effects_none(),
        );
    }

    // ---- cancel: a batch belonging to another conversation -------------------
    {
        let (db, _s) = fresh_db(&fixture, &pepper, "cancel_foreign");
        let foreign = r["foreignBatchId"].as_str().unwrap().to_string();
        let resp = rt.block_on(chat_informs::chat_inform_cancel(
            &db,
            &chat,
            &Some(Some(json!(foreign))),
        ));
        let (st, b) = body_of(&resp);
        compare(
            "cancel: 400 on another conversation\u{2019}s batch",
            st,
            b,
            effects_none(),
        );
    }

    assert!(
        checked >= 12,
        "expected at least twelve route cases to run; ran {checked}"
    );
    eprintln!("OK: chat_informs routes matched oracle ({checked} cases).");
}

/// Collapse the response body's `message` to its IDENTITY.
///
/// v4's own test MOCKS `postInformRecord`, and the mock answers a stub
/// (`{id, type: 'message'}`) rather than the MessageEvent v4's real writer
/// returns — so the full record shape is not something this oracle can speak to.
/// It is pinned where it is actually observable: the `effects` comparand
/// compares the PERSISTED record row's `content` and `targetParticipantIds`
/// against v4's `postInformRecord` arguments, and `post_inform_record` builds
/// the row from v4's field list directly.
///
/// What this comparand DOES keep is the part the handler decides: whether a
/// record exists at all (the "still creates the batch when the record cannot be
/// written" case answers `null` here) and that its id is the one the rows carry.
fn collapse_message(mut body: Value) -> Value {
    if let Some(obj) = body.as_object_mut() {
        if let Some(m) = obj.get("message") {
            let collapsed = match m {
                Value::Null => Value::Null,
                other => other.get("id").cloned().unwrap_or(Value::Null),
            };
            obj.insert("message".into(), collapsed);
        }
    }
    body
}

/// v4's mocked-call record, projected onto the fields v5 can observe.
fn oracle_effects(calls: &Value) -> Value {
    // `recordBody` is the ARGUMENT v4's handler passed, and v4 passes
    // `validated.contentMarkdown` UNTRIMMED — its real `postInformRecord` trims
    // as its very first statement (`params.contentMarkdown?.trim() ?? ''`), so
    // the persisted record carries the trimmed text. v5's observed effect reads
    // that persisted row, so the argument is trimmed here to compare like with
    // like. (v4's test mocks the writer, so the persisted value is not something
    // the oracle can show directly.) `js_trim` is the JS-faithful trim, not
    // Rust's — the two disagree on some Unicode whitespace.
    let record_body = match calls["recordBody"].as_str() {
        Some(s) => Value::String(quilltap_core::jsstr::js_trim(s).to_string()),
        None => Value::Null,
    };
    json!({
        "recordTargets": calls["recordTargets"].clone(),
        "recordBody": record_body,
        "createBatchTargets": calls["createBatchTargets"].clone(),
        "createBatchBody": calls["createBatchBody"].clone(),
    })
}

fn effects_none() -> Value {
    json!({
        "recordTargets": Value::Null,
        "recordBody": Value::Null,
        "createBatchTargets": Value::Null,
        "createBatchBody": Value::Null,
    })
}

/// The same four facts, read off the state v5 actually left behind.
/// `batch` scopes the read to the rows THIS call created — the room is seeded
/// with pending rows of its own for the GET case, and `find_by_chat_id` would
/// otherwise fold those into the comparand.
fn observed_effects(db: &Db, chat_id: &str, batch: Option<&str>) -> Value {
    let cid = chat_id.to_string();
    let b = batch.map(str::to_string);
    let rows = db
        .read_main(move |c| {
            let repo = quilltap_core::db::chat_informs::ChatInformsRepository::new(c);
            match &b {
                Some(batch) => repo.find_by_batch_id(batch),
                None => repo.find_by_chat_id(&cid),
            }
        })
        .unwrap_or_default();
    if rows.is_empty() {
        return effects_none();
    }
    let targets: Vec<String> = rows.iter().map(|r| r.participant_id.clone()).collect();
    let body = rows[0].content_markdown.clone();
    let record_id = rows[0].record_message_id.clone();

    // The record message row, if the handler wrote one.
    let record_targets = match &record_id {
        Some(id) => {
            let mid = id.clone();
            db.read_main(move |c| {
                let mut stmt =
                    c.prepare("SELECT targetParticipantIds FROM chat_messages WHERE id = ?1")?;
                let v: Option<String> = stmt
                    .query_row([&mid], |r| r.get::<_, Option<String>>(0))
                    .ok()
                    .flatten();
                Ok::<_, quilltap_core::db::DbError>(v)
            })
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .unwrap_or(Value::Null)
        }
        None => Value::Null,
    };
    let record_body = match &record_id {
        Some(id) => {
            let mid = id.clone();
            db.read_main(move |c| {
                let mut stmt = c.prepare("SELECT content FROM chat_messages WHERE id = ?1")?;
                let v: Option<String> = stmt
                    .query_row([&mid], |r| r.get::<_, Option<String>>(0))
                    .ok()
                    .flatten();
                Ok::<_, quilltap_core::db::DbError>(v)
            })
            .ok()
            .flatten()
            .map(Value::String)
            .unwrap_or(Value::Null)
        }
        None => Value::Null,
    };

    json!({
        "recordTargets": record_targets,
        "recordBody": record_body,
        "createBatchTargets": targets,
        "createBatchBody": body,
    })
}

/// Turn the named LLM seats into user-controlled ones, so the room has nobody an
/// LLM speaks for (v4's own case swaps the whole participants array).
fn strip_llm_seats(db: &Db, chat_id: &str, seats: &[String]) {
    let cid = chat_id.to_string();
    let drop: std::collections::HashSet<String> = seats.iter().cloned().collect();
    let chat = db
        .read_main(move |c| quilltap_core::db::chats_read::find_by_id(c, &cid))
        .expect("read chat")
        .expect("chat present");
    let mut participants = chat["participants"].as_array().cloned().unwrap_or_default();
    for p in participants.iter_mut() {
        if let Some(id) = p.get("id").and_then(Value::as_str) {
            if drop.contains(id) {
                p["controlledBy"] = Value::String("user".into());
            }
        }
    }
    let cid2 = chat_id.to_string();
    let json_text = serde_json::to_string(&participants).unwrap();
    db.write_blocking(move |ws| {
        ws.main().connection().execute(
            "UPDATE chats SET participants = ?1 WHERE id = ?2",
            rusqlite::params![json_text, cid2],
        )?;
        Ok::<_, quilltap_core::db::DbError>(())
    })
    .expect("strip llm seats");
}
