//! W4.2 differential #1: the dangerous-content **mode resolver + per-chat
//! override truth table** (tier-1, pure) and the **manual Concierge flip**
//! (tier-2, chat-row dump).
//!
//! Two independent sections, each gated on its own env var (skips if unset):
//!
//!   1. `QT_ORACLE_DANGER_RESOLVER` — the pure NDJSON from
//!      `harness/oracle/cases/danger-resolver.ts` (drives v4's REAL
//!      `resolveDangerousContentSettings` + the `chat-override` predicates).
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
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/danger-resolver.ts > /tmp/oracle-danger-resolver.ndjson
//!   QT_FIXTURE_OUT=/tmp/qt-danger-manual-flip.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-danger-manual-flip-fixture.ts
//!   QT_FIXTURE_MANUAL_FLIP=/tmp/qt-danger-manual-flip.db \
//!   QT_ORACLE_OUT=/tmp/oracle-danger-manual-flip.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5W/harness/oracle/cases" \
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
    get_concierge_state, is_chat_active_dangerous, is_concierge_off_duty, ConciergeState,
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
        #[serde(rename = "offDuty")]
        off_duty: bool,
        state: String,
        active: bool,
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
                off_duty,
                state,
                active,
            } => {
                assert_eq!(
                    is_concierge_off_duty(chat.as_ref()),
                    off_duty,
                    "override[{id}] offDuty"
                );
                assert_eq!(
                    get_concierge_state(chat.as_ref()).as_str(),
                    state,
                    "override[{id}] state"
                );
                assert_eq!(
                    is_chat_active_dangerous(chat.as_ref()),
                    active,
                    "override[{id}] active"
                );
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
    ops: Vec<FlipOp>,
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
    match s {
        "safe" => ConciergeState::Safe,
        "flagged" => ConciergeState::Flagged,
        "off" => ConciergeState::Off,
        other => panic!("unknown ConciergeState {other}"),
    }
}

const FLIP_SEED_TS: &str = "2020-01-01T00:00:00.000Z";

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

    eprintln!("OK: danger manual-flip chats + chat_messages dumps matched oracle.");
}
