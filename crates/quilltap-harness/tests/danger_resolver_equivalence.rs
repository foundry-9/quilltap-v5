//! W4.2 differential #1: the dangerous-content **mode resolver + per-chat
//! override truth table** (tier-1, pure) and the **manual Concierge flip**
//! (tier-2, chat-row dump).
//!
//! Two independent sections, each gated on its own env var (skips if unset):
//!
//!   1. `QT_ORACLE_DANGER_RESOLVER` — the pure NDJSON from
//!      `harness/oracle/cases/danger-resolver.ts` (drives v4's REAL
//!      `resolveDangerousContentSettings` + the `chat-override` predicates).
//!      P4.D141 widened it to the four-state control (v4 `60e3c4a0a`): the
//!      override rows carry v4's own `chat-override.test.ts` truth table and ask
//!      all three purpose-named questions, and the resolve rows cover the new
//!      `chat-uncensored` arm (incl. AUTO_ROUTE forced under a global OFF and
//!      exempt-beats-uncensored).
//!   2. `QT_ORACLE_DANGER_MANUAL_FLIP` + `QT_FIXTURE_MANUAL_FLIP` — the tier-2
//!      NDJSON from `harness/oracle/cases/danger-manual-flip.test.ts` (drives
//!      v4's REAL `applyConciergeFlip`) + the baked seed fixture. The ported
//!      `RealConciergeAnnouncer` posts the manual Concierge bubble (v4
//!      `postConciergeManualAnnouncement`, un-mocked in the oracle) into
//!      `chat_messages` on every **changed** flip, so both the `chats` row (its
//!      `addMessage`-bumped `updatedAt`/`lastMessageAt`/`messageCount`) and the
//!      `chat_messages` bubble are diffed.
//!
//! Generate (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
//!   TMPO=/tmp/qt-danger-manual-flip-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/harness/oracle/cases" "$TMPO/harness/oracle/fixtures"
//!   cp "$V5W/harness/oracle/cases/danger-manual-flip.test.ts" "$TMPO/harness/oracle/cases/"
//!   cp "$V5W/harness/oracle/fixtures/danger-manual-flip.json" "$TMPO/harness/oracle/fixtures/"
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/danger-resolver.ts > /tmp/oracle-danger-resolver.ndjson
//!   QT_FIXTURE_OUT=/tmp/qt-danger-manual-flip.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-danger-manual-flip-fixture.ts
//!   QT_FIXTURE_MANUAL_FLIP=/tmp/qt-danger-manual-flip.db \
//!   QT_ORACLE_OUT=/tmp/oracle-danger-manual-flip.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$TMPO/harness/oracle/cases" \
//!       -- danger-manual-flip
//! Run:
//!   QT_ORACLE_DANGER_RESOLVER=/tmp/oracle-danger-resolver.ndjson \
//!   QT_ORACLE_DANGER_MANUAL_FLIP=/tmp/oracle-danger-manual-flip.ndjson \
//!   QT_FIXTURE_MANUAL_FLIP=/tmp/qt-danger-manual-flip.db \
//!     cargo test -p quilltap-harness --test danger_resolver_equivalence

use quilltap_core::db::chat_settings::DangerousContentSettings;
use quilltap_core::db::runtime::Db;
use quilltap_core::db::{chats_read, dump_table_json_conn};
use quilltap_core::services::dangerous_content::chat_override::{
    get_concierge_state, is_classifier_on_duty, should_show_danger_styling,
    should_use_uncensored_route, ConciergeState,
};
use quilltap_core::services::dangerous_content::manual_flip::{
    apply_concierge_flip, RealConciergeAnnouncer,
};
use quilltap_core::services::dangerous_content::resolver::resolve_dangerous_content_settings;
use serde::Deserialize;
use serde_json::Value;

/// Recursively collapse integer-valued JSON floats to integers, matching JS
/// `JSON.stringify` (which renders `1.0` as `1`). serde renders an f64 `1.0` as
/// `1.0`, so both sides are canonicalized before comparison.
fn canon_numbers(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if n.is_f64() && f.fract() == 0.0 && f.abs() < 9.007e15 {
                    *v = Value::Number(serde_json::Number::from(f as i64));
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(canon_numbers),
        Value::Object(o) => o.values_mut().for_each(canon_numbers),
        _ => {}
    }
}

fn canon(mut v: Value) -> Value {
    canon_numbers(&mut v);
    v
}

// --- section 1: pure resolver + override matrix ---

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum PureRow {
    #[serde(rename = "resolve")]
    Resolve {
        id: String,
        global: Option<Value>,
        chat: Option<Value>,
        settings: Value,
        source: String,
    },
    #[serde(rename = "override")]
    Override {
        id: String,
        chat: Option<Value>,
        state: String,
        #[serde(rename = "uncensoredRoute")]
        uncensored_route: bool,
        #[serde(rename = "dangerStyling")]
        danger_styling: bool,
        #[serde(rename = "classifierOnDuty")]
        classifier_on_duty: bool,
    },
}

#[test]
fn danger_resolver_pure_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_DANGER_RESOLVER") else {
        eprintln!("SKIP: set QT_ORACLE_DANGER_RESOLVER to the pure NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: PureRow = serde_json::from_str(line).expect("parse pure row");
        match row {
            PureRow::Resolve {
                id,
                global,
                chat,
                settings,
                source,
            } => {
                let global_settings: Option<DangerousContentSettings> =
                    global.map(|v| serde_json::from_value(v).expect("deserialize global settings"));
                let resolved = resolve_dangerous_content_settings(global_settings, chat.as_ref());
                let got = serde_json::to_value(&resolved.settings).expect("serialize settings");
                assert_eq!(canon(got), canon(settings), "resolve[{id}] settings");
                assert_eq!(resolved.source.as_str(), source, "resolve[{id}] source");
            }
            PureRow::Override {
                id,
                chat,
                state,
                uncensored_route,
                danger_styling,
                classifier_on_duty,
            } => {
                assert_eq!(
                    get_concierge_state(chat.as_ref()).as_str(),
                    state,
                    "override[{id}] state"
                );
                assert_eq!(
                    should_use_uncensored_route(chat.as_ref()),
                    uncensored_route,
                    "override[{id}] uncensoredRoute"
                );
                assert_eq!(
                    should_show_danger_styling(chat.as_ref()),
                    danger_styling,
                    "override[{id}] dangerStyling"
                );
                assert_eq!(
                    is_classifier_on_duty(chat.as_ref()),
                    classifier_on_duty,
                    "override[{id}] classifierOnDuty"
                );
                // The 2x2's diagonal: uncensored is the one state that routes
                // uncensored yet is never painted as a hazard.
                if state == "uncensored" {
                    assert!(uncensored_route && !danger_styling, "override[{id}] corner");
                }
            }
        }
        count += 1;
    }
    assert!(count > 0, "resolver oracle looks empty");
    eprintln!("OK: danger resolver/override matched oracle ({count} rows).");
}

// --- section 2: manual flip (tier-2 chat-row dump) ---

#[derive(Deserialize)]
struct FlipSpec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    chats: Vec<FlipSeedChat>,
    ops: Vec<FlipOp>,
}

/// Only the two fields the preservation assert needs; the builder owns the rest.
#[derive(Deserialize)]
struct FlipSeedChat {
    id: String,
    #[serde(rename = "isDangerousChat")]
    is_dangerous_chat: Option<bool>,
}

#[derive(Deserialize)]
struct FlipOp {
    id: String,
    #[serde(rename = "chatId")]
    chat_id: String,
    requested: String,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum FlipOracleRow {
    #[serde(rename = "op")]
    Op {
        id: String,
        #[serde(rename = "newState")]
        new_state: String,
        changed: bool,
    },
    #[serde(rename = "table")]
    Table { table: String, rows: Vec<Value> },
}

fn state_from_str(s: &str) -> ConciergeState {
    ConciergeState::from_wire(s).unwrap_or_else(|| panic!("unknown ConciergeState {s}"))
}

const FLIP_SEED_TS: &str = "2020-01-01T00:00:00.000Z";

/// ⚠ CROSS-LANE DIVERGENCE — retire this when P4.D140 lands.
///
/// This lane regenerates from a worktree pinned at v4 `60e3c4a0a`, and bug 112
/// (`735d9408c`, "date a chat by when a character last spoke") is an ANCESTOR of
/// that commit. At the pin, v4's `addMessage` moves `lastMessageAt` only for a
/// *character-authored* row (`isCharacterAuthoredMessage`), and the Concierge
/// manual bubble carries `systemSender: 'concierge'` — so v4 leaves the column
/// NULL where v5 still mints a timestamp.
///
/// The fix lives in `db/chats_messages.rs`, which **P4.D140 owns** (§Ownership),
/// so this lane may not make it. The column is therefore masked on both sides —
/// but only after [`assert_last_message_at_cross_lane_divergence`] MEASURES the
/// divergence in the exact shape bug 112 predicts, so the mask can never hide a
/// different failure. When P4.D140 lands, this constant flips to `false`, the
/// mask disappears, and the plain equality returns; if v5 converges early the
/// measurement itself reddens.
const LAST_MESSAGE_AT_PENDING_P4D140: bool = true;

/// Placeholder the minted `dangerClassifiedAt` (present-non-null → `<ts>`) so the
/// flagged-path mint compares. `updatedAt` / `lastMessageAt` are bumped by the
/// Concierge manual-announcement's `addMessage` (v4 `repos.chats.addMessage`) on a
/// **changed** flip — sentinel-aware, so a value equal to the `2020` seed stays
/// (proving the noop path posted nothing) and a mint collapses to `<ts>`.
fn normalize_chat_rows(rows: &mut [Value]) {
    for row in rows.iter_mut() {
        if let Some(obj) = row.as_object_mut() {
            if obj.get("dangerClassifiedAt").map(|v| !v.is_null()) == Some(true) {
                obj.insert("dangerClassifiedAt".into(), Value::String("<ts>".into()));
            }
            for col in ["updatedAt", "lastMessageAt"] {
                let minted = obj
                    .get(col)
                    .and_then(Value::as_str)
                    .map(|s| s != FLIP_SEED_TS)
                    .unwrap_or(false);
                if minted {
                    obj.insert(col.into(), Value::String("<ts>".into()));
                }
            }
            if LAST_MESSAGE_AT_PENDING_P4D140 {
                obj.insert("lastMessageAt".into(), Value::String("<p4d140>".into()));
            }
        }
    }
}

/// The Concierge manual bubble's minted `id` + `createdAt` are placeholdered; every
/// other column (content / opaqueContent / systemSender / systemKind / role /
/// type / participantId / attachments …) is diffed exactly against v4's REAL
/// writer.
fn normalize_message_rows(rows: &mut [Value]) {
    for row in rows.iter_mut() {
        if let Some(obj) = row.as_object_mut() {
            if obj.contains_key("id") {
                obj.insert("id".into(), Value::String("<id>".into()));
            }
            if obj.get("createdAt").map(|v| !v.is_null()) == Some(true) {
                obj.insert("createdAt".into(), Value::String("<ts>".into()));
            }
        }
    }
}

#[tokio::test]
async fn danger_manual_flip_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_DANGER_MANUAL_FLIP") else {
        eprintln!("SKIP: set QT_ORACLE_DANGER_MANUAL_FLIP to the tier-2 NDJSON (see header).");
        return;
    };
    let Ok(fixture) = std::env::var("QT_FIXTURE_MANUAL_FLIP") else {
        eprintln!("SKIP: set QT_FIXTURE_MANUAL_FLIP to the seed fixture .db (see header).");
        return;
    };

    let spec_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/danger-manual-flip.json");
    let spec: FlipSpec = serde_json::from_str(
        &std::fs::read_to_string(&spec_path).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse flip spec");
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));

    // Parse oracle: op results (by id) + the chats + chat_messages table dumps.
    let mut want_ops: std::collections::HashMap<String, (String, bool)> = Default::default();
    let mut want_rows: Vec<Value> = Vec::new();
    let mut want_message_rows: Vec<Value> = Vec::new();
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<FlipOracleRow>(line).expect("parse flip oracle row") {
            FlipOracleRow::Op {
                id,
                new_state,
                changed,
            } => {
                want_ops.insert(id, (new_state, changed));
            }
            FlipOracleRow::Table { table, rows } => match table.as_str() {
                "chats" => want_rows = rows,
                "chat_messages" => want_message_rows = rows,
                other => panic!("unexpected oracle table {other}"),
            },
        }
    }

    let work = std::env::temp_dir().join(format!(
        "qt-danger-manual-flip-rust-{}.db",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let db = Db::open_main(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));

    for op in &spec.ops {
        let chat = db
            .read_main(|c| chats_read::find_by_id(c, &op.chat_id))
            .unwrap_or_else(|e| panic!("read chat {}: {e:?}", op.chat_id))
            .unwrap_or_else(|| panic!("op {}: chat {} missing", op.id, op.chat_id));
        let result = apply_concierge_flip(
            &db,
            &RealConciergeAnnouncer { db: &db },
            &op.chat_id,
            state_from_str(&op.requested),
            &chat,
        )
        .await
        .unwrap_or_else(|e| panic!("flip {}: {e:?}", op.id));

        let (want_state, want_changed) = want_ops
            .get(&op.id)
            .unwrap_or_else(|| panic!("oracle missing op {}", op.id));
        assert_eq!(
            result.new_state.as_str(),
            want_state,
            "op {} newState",
            op.id
        );
        assert_eq!(result.changed, *want_changed, "op {} changed", op.id);
    }

    let mut got_dump = db
        .read_main(|c| dump_table_json_conn(c, "chats", "id"))
        .expect("dump chats");
    let mut got_message_dump = db
        .read_main(|c| dump_table_json_conn(c, "chat_messages", "chatId"))
        .expect("dump chat_messages");
    drop(db);
    let _ = std::fs::remove_file(&work);

    let mut got_rows: Vec<Value> = got_dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .expect("dump rows")
        .clone();

    // MEASURE the cross-lane divergence before the normalizer masks it.
    assert_last_message_at_cross_lane_divergence(&spec, &got_rows, &want_rows);

    normalize_chat_rows(&mut got_rows);
    normalize_chat_rows(&mut want_rows);
    // Numeric canonicalization so INTEGER/REAL rendering agrees across sides.
    let got_rows = got_rows.into_iter().map(canon).collect::<Vec<_>>();
    let want_rows = want_rows.into_iter().map(canon).collect::<Vec<_>>();

    assert_eq!(got_rows, want_rows, "chats rows diverge after manual flips");

    // The Concierge manual bubble (v4 `postConciergeManualAnnouncement`, now real
    // on both sides) lands in `chat_messages` — one per changed flip, none for the
    // no-op. Diff those rows byte-for-byte (id/createdAt placeholdered).
    let mut got_msgs: Vec<Value> = got_message_dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .expect("dump message rows")
        .clone();
    normalize_message_rows(&mut got_msgs);
    normalize_message_rows(&mut want_message_rows);
    let got_msgs = got_msgs.into_iter().map(canon).collect::<Vec<_>>();
    let want_msgs = want_message_rows.into_iter().map(canon).collect::<Vec<_>>();
    assert_eq!(
        got_msgs, want_msgs,
        "chat_messages rows diverge after manual flips"
    );

    // v4 pins "operator states preserve isDangerousChat (the update never
    // touches the label)" by asserting the update object lacks the key. v5's
    // standalone write has no update object to inspect, so the equivalent claim
    // is made at COLUMN level over the dumped rows — and it is checked against
    // BOTH sides, so it can never pass vacuously on a corpus that stopped
    // carrying an operator target.
    assert_operator_states_preserve_the_label(&spec, &got_rows);
    assert_operator_states_preserve_the_label(&spec, &want_rows);

    eprintln!(
        "OK: danger manual-flip chats + chat_messages dumps matched oracle ({} ops).",
        spec.ops.len()
    );
}

/// The [`LAST_MESSAGE_AT_PENDING_P4D140`] measurement. For every op that CHANGED
/// state (and therefore posted a Concierge bubble), v4 at the `60e3c4a0a` pin
/// must leave `lastMessageAt` untouched (NULL — the seed never set it) while v5
/// mints a timestamp. Anything else means the mask is covering a different bug.
fn assert_last_message_at_cross_lane_divergence(
    spec: &FlipSpec,
    got_rows: &[Value],
    want_rows: &[Value],
) {
    if !LAST_MESSAGE_AT_PENDING_P4D140 {
        return;
    }
    let find = |rows: &[Value], id: &str| -> Value {
        rows.iter()
            .find(|r| r.get("id").and_then(Value::as_str) == Some(id))
            .unwrap_or_else(|| panic!("chat {id} missing from the dump"))
            .get("lastMessageAt")
            .cloned()
            .unwrap_or(Value::Null)
    };
    let mut changed = 0usize;
    for op in &spec.ops {
        // The no-op ops post nothing, so neither side writes the column.
        if op.id.ends_with("-noop") {
            assert_eq!(find(got_rows, &op.chat_id), Value::Null, "op {} v5", op.id);
            assert_eq!(find(want_rows, &op.chat_id), Value::Null, "op {} v4", op.id);
            continue;
        }
        assert_eq!(
            find(want_rows, &op.chat_id),
            Value::Null,
            "op {}: v4 at 60e3c4a0a must NOT bump lastMessageAt for a Concierge \
             bubble (bug 112, 735d9408c). A non-null here means the mask is \
             hiding something else.",
            op.id
        );
        assert!(
            find(got_rows, &op.chat_id).is_string(),
            "op {}: v5 still bumps lastMessageAt (bug 112 unported — P4.D140). \
             If this is null, P4.D140 has landed: flip \
             LAST_MESSAGE_AT_PENDING_P4D140 to false and drop the mask.",
            op.id
        );
        changed += 1;
    }
    assert!(changed > 0, "no changed flip to measure the divergence on");
}

/// Every op that ASKED for an operator state (`vouched` / `uncensored`) must
/// leave `isDangerousChat` exactly as the seed left it — the label is preserved
/// underneath so returning to Monitored/Flagged picks up where the classifier
/// left off. Fails loudly if the corpus carries no such op at all.
fn assert_operator_states_preserve_the_label(spec: &FlipSpec, rows: &[Value]) {
    let seeded: std::collections::HashMap<&str, Option<bool>> = spec
        .chats
        .iter()
        .map(|c| (c.id.as_str(), c.is_dangerous_chat))
        .collect();
    let mut checked = 0usize;
    for op in &spec.ops {
        if op.requested != "vouched" && op.requested != "uncensored" {
            continue;
        }
        let seed = *seeded
            .get(op.chat_id.as_str())
            .unwrap_or_else(|| panic!("op {} names an unseeded chat", op.id));
        let row = rows
            .iter()
            .find(|r| r.get("id").and_then(Value::as_str) == Some(op.chat_id.as_str()))
            .unwrap_or_else(|| panic!("op {}: chat row missing from the dump", op.id));
        // The column is stored as 0/1; the seed spec speaks booleans.
        let stored = match row.get("isDangerousChat") {
            None | Some(Value::Null) => None,
            Some(v) => Some(v.as_i64().map(|n| n != 0).unwrap_or_else(|| {
                v.as_bool()
                    .unwrap_or_else(|| panic!("op {}: odd isDangerousChat {v}", op.id))
            })),
        };
        assert_eq!(
            stored, seed,
            "op {} requested {} and must not touch isDangerousChat",
            op.id, op.requested
        );
        checked += 1;
    }
    assert!(
        checked >= 2,
        "the corpus must exercise BOTH operator states as flip targets (found {checked})"
    );
}
