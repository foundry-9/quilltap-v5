//! Tier-1 differential (P4.D172): the room's character map — one batched read,
//! every present seat (v4 `d14da3a56`, bug 131).
//!
//! Drives v4's REAL `lib/chat/turn-manager/room-characters.ts` at the
//! `78b381a96` pin over a counting stand-in for `repos.characters.findByIds`,
//! and this side replays the same stub through the same seam. The CALL COUNT and
//! the ids each call asked for are comparands — "reads the whole room in ONE
//! call, not one per seat" is the point of the commit, and only the count can say
//! it. Every case name in v4's own `room-characters.test.ts` is a row.
//!
//! Generate the oracle output (tsx imports the WORKTREE case file — point it at
//! this lane's copy, not main):
//!   cd ~/source/quilltap-server
//!   npx tsx <worktree>/harness/oracle/cases/room-characters.ts \
//!     > /tmp/oracle-room-characters.ndjson
//! Run:
//!   QT_ORACLE_ROOM_CHARACTERS=/tmp/oracle-room-characters.ndjson \
//!     cargo test -p quilltap-harness --test room_characters_equivalence

use std::collections::HashMap;

use quilltap_core::chat_predicates::participant_status_from_str;
use quilltap_core::cycle_order::{cycle_candidates, draw_cycle_order};
use quilltap_core::room_characters::{load_room_characters, to_speaker_characters};
use quilltap_core::select_speaker::SpeakerParticipant;
use quilltap_core::weighted_random::DrawSource;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize, Clone)]
struct WirePart {
    id: String,
    #[serde(rename = "type")]
    participant_type: String,
    status: String,
    #[serde(rename = "characterId")]
    character_id: Option<String>,
    #[serde(rename = "controlledBy")]
    controlled_by: String,
    talkativeness: Option<f64>,
}

fn speakers_of(row: &Value) -> Vec<SpeakerParticipant> {
    let parts: Vec<WirePart> =
        serde_json::from_value(row["participants"].clone()).expect("participants");
    parts
        .iter()
        .map(|p| SpeakerParticipant {
            id: p.id.clone(),
            participant_type: p.participant_type.clone(),
            // §C: participant-status parsing has ONE home.
            status: participant_status_from_str(Some(p.status.as_str())),
            character_id: p.character_id.clone(),
            controlled_by: p.controlled_by.clone(),
            talkativeness: p.talkativeness,
        })
        .collect()
}

/// The character rows the stub can resolve, as the overlaid `Value`s v5's
/// `characters_read::find_by_ids` hands back.
fn rows_of(row: &Value) -> Vec<Value> {
    row["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|c| {
            let id = c["id"].as_str().unwrap();
            let mut o = serde_json::Map::new();
            o.insert("id".into(), json!(id));
            o.insert(
                "name".into(),
                json!(c["name"]
                    .as_str()
                    .map(String::from)
                    .unwrap_or(format!("Character {id}"))),
            );
            if let Some(t) = c.get("talkativeness").and_then(Value::as_f64) {
                o.insert("talkativeness".into(), json!(t));
            }
            if let Some(a) = c.get("archivedAt").and_then(Value::as_str) {
                o.insert("archivedAt".into(), json!(a));
            }
            Value::Object(o)
        })
        .collect()
}

/// The counting stand-in — v4's `makeRepos`. Returns the closure plus the log of
/// what each call asked for, so the COUNT and the ids can be compared.
struct CountingReader {
    rows: Vec<Value>,
    calls: std::rc::Rc<std::cell::RefCell<Vec<Vec<String>>>>,
}

impl CountingReader {
    fn new(rows: Vec<Value>) -> Self {
        CountingReader {
            rows,
            calls: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
        }
    }
}

fn preloaded_of(row: &Value) -> Vec<Value> {
    match &row["preloaded"] {
        Value::Array(items) => items
            .iter()
            .filter(|v| !v.is_null())
            .map(|c| {
                let id = c["id"].as_str().unwrap();
                json!({
                    "id": id,
                    "name": c["name"].as_str().map(String::from)
                        .unwrap_or(format!("Character {id}")),
                })
            })
            .collect(),
        // v4 passes `[null, undefined]` in one case; both are dropped by the
        // `preloaded?.id` gate, so the Rust side simply never sees them. The
        // filter above is what reproduces that, and the `ignores-empty-preloaded`
        // row is what proves the read's own value survives.
        _ => Vec::new(),
    }
}

#[test]
fn room_characters_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_ROOM_CHARACTERS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_ROOM_CHARACTERS to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut draw_heads: Vec<(String, Option<String>)> = Vec::new();

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("oracle row");
        let kind = row["kind"].as_str().unwrap().to_string();
        let id = row["id"].as_str().unwrap().to_string();
        *counts.entry(kind.clone()).or_default() += 1;

        let parts = speakers_of(&row);
        let reader = CountingReader::new(rows_of(&row));
        let calls = reader.calls.clone();
        let rows_owned = reader.rows.clone();
        let mut find_by_ids = |ids: &[String]| -> Result<Vec<Value>, quilltap_core::db::DbError> {
            calls.borrow_mut().push(ids.to_vec());
            Ok(rows_owned
                .iter()
                .filter(|c| ids.iter().any(|i| i == c["id"].as_str().unwrap()))
                .cloned()
                .collect())
        };
        let map = load_room_characters(&parts, &preloaded_of(&row), &mut find_by_ids)
            .unwrap_or_else(|e| panic!("{kind} '{id}': load failed: {e:?}"));

        match kind.as_str() {
            "load" => {
                // The map, projected to what v4's row carries.
                let want = row["out"].as_object().expect("out");
                let got_keys: std::collections::BTreeSet<&str> =
                    map.keys().map(String::as_str).collect();
                let want_keys: std::collections::BTreeSet<&str> =
                    want.keys().map(String::as_str).collect();
                assert_eq!(got_keys, want_keys, "load '{id}': map key set");
                for (cid, w) in want {
                    let g = map.get(cid).unwrap();
                    assert_eq!(
                        g.get("name").and_then(Value::as_str),
                        w["name"].as_str(),
                        "load '{id}': {cid} name"
                    );
                    assert_eq!(
                        g.get("talkativeness").and_then(Value::as_f64),
                        w["talkativeness"].as_f64(),
                        "load '{id}': {cid} talkativeness"
                    );
                    assert_eq!(
                        g.get("archivedAt").and_then(Value::as_str),
                        w["archivedAt"].as_str(),
                        "load '{id}': {cid} archivedAt"
                    );
                }
                // The batched read: how many calls, and which ids each asked for.
                let want_calls: Vec<Vec<String>> =
                    serde_json::from_value(row["calls"].clone()).expect("calls");
                let got_calls = calls.borrow().clone();
                assert_eq!(
                    got_calls.len(),
                    want_calls.len(),
                    "load '{id}': find_by_ids CALL COUNT rust={} oracle={}",
                    got_calls.len(),
                    want_calls.len()
                );
                for (i, (g, w)) in got_calls.iter().zip(want_calls.iter()).enumerate() {
                    assert_eq!(g, w, "load '{id}': call[{i}] ids");
                }
            }
            "candidates" => {
                let chars = to_speaker_characters(&map);
                let got: Vec<String> = cycle_candidates(&parts, &chars)
                    .iter()
                    .map(|p| p.id.clone())
                    .collect();
                let want: Vec<String> = serde_json::from_value(row["out"].clone()).unwrap();
                assert_eq!(got, want, "candidates '{id}'");
            }
            "draw" => {
                let chars = to_speaker_characters(&map);
                let pinned: Vec<f64> = serde_json::from_value(row["draws"].clone()).unwrap();
                let got =
                    draw_cycle_order(&parts, &chars, None, &DrawSource::sequence(pinned.clone()));
                let want: Vec<String> = serde_json::from_value(row["out"].clone()).unwrap();
                assert_eq!(got, want, "draw '{id}'");
                draw_heads.push((id.clone(), got.first().cloned()));
            }
            other => panic!("unknown oracle row kind '{other}'"),
        }
    }

    // Bug 131, made non-vacuous: the two `bug131-*` rows are the SAME room drawn
    // with the SAME pinned value, differing only in the USER seat's
    // talkativeness. A map built from LLM seats alone weights the human at the
    // 0.5 default either way, so those rows would come out IDENTICAL — the
    // assertion below is what would fail if the room map ever narrowed back to
    // `get_active_character_participants`.
    let loud = draw_heads
        .iter()
        .find(|(i, _)| i == "bug131-loud-user-seat")
        .expect("the bug-131 loud row");
    let quiet = draw_heads
        .iter()
        .find(|(i, _)| i == "bug131-quiet-user-seat")
        .expect("the bug-131 quiet row");
    assert_ne!(
        loud.1, quiet.1,
        "bug 131: a loud and a quiet user seat drew the SAME head — the room map \
         cannot see user-driven seats"
    );

    for (kind, floor) in [("load", 10usize), ("candidates", 2), ("draw", 2)] {
        let seen = counts.get(kind).copied().unwrap_or(0);
        assert!(
            seen >= floor,
            "oracle carries {seen} '{kind}' rows, expected >= {floor}"
        );
    }
    let total: usize = counts.values().sum();
    eprintln!("OK: room-characters matched oracle ({total} rows: {counts:?}).");
}
