//! Tier-2 differential: the speaker-name resolver (P4.D212; v4
//! `lib/chat/speaker-names.ts`, `e7821606f`, bug 161), ported as
//! `quilltap_core::services::speaker_names`.
//!
//! Both sides READ a COPY of one baked fixture (characters + chats written by
//! v4's REAL repositories; the seat shapes v4's schemas refuse PLANTED by SQL —
//! see the builder's header for which were measured necessary). Per spec chat,
//! the oracle reads the chat through v4's real `repos.chats.findById` (or, for
//! the planted `read: raw` chat that `findById` answers NULL for, from the raw
//! `participants` column), runs v4's REAL `resolveSpeakerNames`, and labels a
//! fixed label list plus every seat under USER and ASSISTANT through v4's REAL
//! `speakerLabel`. This side does the same through `chats_read::find_by_id` /
//! the same raw column, `resolve_speaker_names`, and `speaker_label`, and
//! compares, per case:
//!
//!   * the seats fed to the resolver (`(id, characterId)` — a precondition);
//!   * the resolved map as an ORDERED `(participantId, name)` pair list (v4's
//!     `Map` insertion order);
//!   * the COUNT of character reads — v4's is every `findByIdRaw` call (the
//!     cached repo container's method is wrapped); v5's is every statement
//!     SQLite actually starts that reads `FROM characters`, counted by a
//!     `sqlite3_trace_v2` statement trace on the connection. The counter is
//!     self-checked (a hit counts 1, a MISS counts 1, a chat read counts 0)
//!     before any case runs. (A first attempt — a TEMP VIEW shadowing
//!     `characters` with a ticking scalar subquery — failed that self-check:
//!     SQLite never evaluated the subquery on a miss.);
//!   * every label.
//!
//! Plus `probes`: what each side's raw character read answers for every spec
//! character and the missing id — above all the empty-name row, which v4's
//! `characters.create` ACCEPTS (measured at `a2db63da7`), so the resolver's
//! `if (character?.name)` gate — not the read — is what keeps it unnamed.
//!
//! The corpus: two named seats incl. the persona (a `CHARACTER` seat,
//! `controlledBy: user`), a `removed` seat, a `silent` seat, an `absent` seat,
//! a seat with NO `characterId` and one with `""`, a seat naming NO character
//! row, the empty-name character, two seats on ONE character (two reads), a
//! duplicate-participant-id chat (a named first seat stops the second's read;
//! an unresolved first seat does NOT), and an empty chat.
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   rm -f /tmp/oracle-speaker-names.ndjson
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_SPEAKER_NAMES_MAIN=/tmp/qt-speaker-names-main.db \
//!   QT_FIXTURE_SPEAKER_NAMES_MOUNT=/tmp/qt-speaker-names-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-speaker-names-fixture.ts
//!   QT_FIXTURE_SPEAKER_NAMES_MAIN=/tmp/qt-speaker-names-main.db \
//!   QT_FIXTURE_SPEAKER_NAMES_MOUNT=/tmp/qt-speaker-names-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/cases/speaker-names.ts \
//!     > /tmp/oracle-speaker-names.ndjson
//! Run:
//!   cd $V5W
//!   QT_ORACLE_SPEAKER_NAMES=/tmp/oracle-speaker-names.ndjson \
//!   QT_FIXTURE_SPEAKER_NAMES_MAIN=/tmp/qt-speaker-names-main.db \
//!     cargo test -p quilltap-harness --test speaker_names_equivalence -- --nocapture

use std::ffi::{c_char, c_int, c_uint, c_void, CStr};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use quilltap_core::db::{characters_read, chats_read, Writer};
use quilltap_core::services::speaker_names::{resolve_speaker_names, speaker_label};
use rusqlite::{ffi, Connection};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct SpecChar {
    id: String,
}
#[derive(Deserialize)]
struct SpecChat {
    name: String,
    id: String,
    read: String,
}
#[derive(Deserialize)]
struct SpecLabel {
    #[serde(rename = "participantId")]
    participant_id: Option<String>,
    role: String,
}
#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    characters: Vec<SpecChar>,
    #[serde(rename = "missingCharacterId")]
    missing_character_id: String,
    chats: Vec<SpecChat>,
    labels: Vec<SpecLabel>,
}

#[derive(Deserialize)]
struct OracleLabel {
    #[serde(rename = "participantId")]
    participant_id: Option<String>,
    role: String,
    label: String,
}
#[derive(Deserialize)]
struct OracleCase {
    name: String,
    #[serde(rename = "chatId")]
    chat_id: String,
    read: String,
    seats: Vec<Value>,
    names: Vec<(String, String)>,
    reads: Vec<String>,
    labels: Vec<OracleLabel>,
}
#[derive(Deserialize)]
struct Oracle {
    probes: Vec<Value>,
    cases: Vec<OracleCase>,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/speaker-names.json")
}

/// Counts every statement SQLite actually starts against `characters` on one
/// connection, via `sqlite3_trace_v2(SQLITE_TRACE_STMT)` over the unexpanded
/// SQL text (rusqlite's own `trace` feature is not enabled in this workspace,
/// so the raw FFI is used). A statement counts when its text reads
/// `FROM characters` — `characters_read`'s row query; the `PRAGMA
/// table_info(characters)` column probe does not match. Unregistered on drop,
/// before the counter it points at can go away.
struct ReadCounter<'c> {
    conn: &'c Connection,
    count: Box<AtomicUsize>,
}

unsafe extern "C" fn count_character_reads(
    _mask: c_uint,
    ctx: *mut c_void,
    _stmt: *mut c_void,
    sql: *mut c_void,
) -> c_int {
    if !ctx.is_null() && !sql.is_null() {
        // SAFETY: for SQLITE_TRACE_STMT, `sql` is the NUL-terminated
        // unexpanded statement text, and `ctx` is the boxed counter the
        // ReadCounter keeps alive until it unregisters the callback.
        let text = unsafe { CStr::from_ptr(sql as *const c_char) }.to_string_lossy();
        if text.contains("FROM characters") {
            unsafe { &*(ctx as *const AtomicUsize) }.fetch_add(1, Ordering::SeqCst);
        }
    }
    0
}

impl<'c> ReadCounter<'c> {
    fn install(conn: &'c Connection) -> Self {
        let count = Box::new(AtomicUsize::new(0));
        // SAFETY: the handle is live for 'c; the context pointer is the boxed
        // counter, which outlives the registration (Drop unregisters first).
        let rc = unsafe {
            ffi::sqlite3_trace_v2(
                conn.handle(),
                ffi::SQLITE_TRACE_STMT as c_uint,
                Some(count_character_reads),
                &*count as *const AtomicUsize as *mut c_void,
            )
        };
        assert_eq!(rc, ffi::SQLITE_OK, "sqlite3_trace_v2 registration");
        ReadCounter { conn, count }
    }

    /// Take the count so far and reset it.
    fn take(&self) -> usize {
        self.count.swap(0, Ordering::SeqCst)
    }
}

impl Drop for ReadCounter<'_> {
    fn drop(&mut self) {
        // SAFETY: unregister before the boxed counter is freed.
        unsafe {
            ffi::sqlite3_trace_v2(self.conn.handle(), 0, None, std::ptr::null_mut());
        }
    }
}

fn seats_projection(participants: &[Value]) -> Vec<Value> {
    participants
        .iter()
        .map(|p| {
            json!({
                "id": p.get("id").cloned().unwrap_or(Value::Null),
                "characterId": p.get("characterId").cloned().unwrap_or(Value::Null),
            })
        })
        .collect()
}

#[test]
fn speaker_names_match_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_SPEAKER_NAMES") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_SPEAKER_NAMES to the oracle NDJSON (see header).");
            return;
        }
    };
    let main_fixture = match std::env::var("QT_FIXTURE_SPEAKER_NAMES_MAIN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_SPEAKER_NAMES_MAIN to the main fixture .db (header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let oracle: Oracle = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle");

    let work = std::env::temp_dir().join(format!(
        "qt-speaker-names-main-rust-{}.db",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&main_fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));
    let main = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture: {e}"));
    let conn = main.connection();

    // ---- probes: the raw character read, per spec character + the missing id.
    let mut probe_ids: Vec<&str> = spec.characters.iter().map(|c| c.id.as_str()).collect();
    probe_ids.push(&spec.missing_character_id);
    let got_probes: Vec<Value> = probe_ids
        .iter()
        .map(|id| match characters_read::find_by_id_raw(conn, id) {
            Ok(Some(row)) => json!({
                "id": id,
                "outcome": "found",
                "name": row.get("name").cloned().unwrap_or(Value::Null),
            }),
            Ok(None) => json!({ "id": id, "outcome": "none", "name": null }),
            Err(_) => json!({ "id": id, "outcome": "threw", "name": null }),
        })
        .collect();
    assert_eq!(
        got_probes,
        oracle.probes,
        "probes diverged\n  rust:   {}\n  oracle: {}",
        serde_json::to_string(&got_probes).unwrap(),
        serde_json::to_string(&oracle.probes).unwrap()
    );

    // ---- the read counter, self-checked on a hit and a miss.
    let reads = ReadCounter::install(conn);
    characters_read::find_by_id_raw(conn, &spec.characters[0].id).expect("counter hit");
    assert_eq!(reads.take(), 1, "read counter: a hit must count once");
    characters_read::find_by_id_raw(conn, &spec.missing_character_id).expect("counter miss");
    assert_eq!(reads.take(), 1, "read counter: a miss must count once");
    chats_read::find_by_id(conn, &spec.chats[0].id).expect("counter chat read");
    assert_eq!(reads.take(), 0, "read counter: a chat read must not count");

    assert_eq!(
        spec.chats.len(),
        oracle.cases.len(),
        "case count: spec vs oracle"
    );
    let mut label_rows = 0usize;
    let mut name_rows = 0usize;
    let mut read_total = 0usize;

    for (sc, oc) in spec.chats.iter().zip(&oracle.cases) {
        assert_eq!(
            (&oc.name, &oc.chat_id, &oc.read),
            (&sc.name, &sc.id, &sc.read)
        );
        let ctx = &oc.name;

        let participants: Vec<Value> = match sc.read.as_str() {
            "repo" => chats_read::find_by_id(conn, &sc.id)
                .expect("chat read")
                .unwrap_or_else(|| panic!("{ctx}: chat not found"))
                .get("participants")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            "raw" => {
                // Informational: v4's `findById` answers NULL here (Zod refuses
                // the planted seats); v5's read is not validated.
                let v5_repo = chats_read::find_by_id(conn, &sc.id);
                eprintln!(
                    "  note [{ctx}]: v5 chats_read::find_by_id on the planted chat → {}",
                    match &v5_repo {
                        Ok(Some(_)) => "Some(chat)",
                        Ok(None) => "None",
                        Err(_) => "Err",
                    }
                );
                let raw: String = conn
                    .query_row(
                        "SELECT participants FROM chats WHERE id = ?1",
                        [&sc.id],
                        |r| r.get(0),
                    )
                    .expect("raw participants");
                serde_json::from_str::<Vec<Value>>(&raw).expect("parse raw participants")
            }
            other => panic!("unknown read mode {other}"),
        };

        assert_eq!(
            seats_projection(&participants),
            oc.seats,
            "{ctx}: the seats fed to the resolver diverged"
        );

        reads.take();
        let names = resolve_speaker_names(conn, &participants);
        let v5_reads = reads.take();

        assert_eq!(
            names.entries(),
            oc.names.as_slice(),
            "{ctx}: resolved names (ordered pairs) diverged"
        );
        assert_eq!(
            v5_reads,
            oc.reads.len(),
            "{ctx}: character read count diverged (v4 read {:?})",
            oc.reads
        );

        // Label inputs: the spec list, then every seat under USER + ASSISTANT.
        let mut inputs: Vec<(Option<String>, String)> = spec
            .labels
            .iter()
            .map(|l| (l.participant_id.clone(), l.role.clone()))
            .collect();
        for p in &participants {
            let id = p.get("id").and_then(Value::as_str).map(str::to_string);
            inputs.push((id.clone(), "USER".to_string()));
            inputs.push((id, "ASSISTANT".to_string()));
        }
        assert_eq!(inputs.len(), oc.labels.len(), "{ctx}: label row count");
        for (i, ((pid, role), ol)) in inputs.iter().zip(&oc.labels).enumerate() {
            assert_eq!(
                (pid, role),
                (&ol.participant_id, &ol.role),
                "{ctx}: label {i} input"
            );
            let got = speaker_label(pid.as_deref(), role, &names);
            assert_eq!(
                got, ol.label,
                "{ctx}: label {i} (participantId {pid:?}, role {role}) diverged"
            );
        }

        label_rows += inputs.len();
        name_rows += oc.names.len();
        read_total += oc.reads.len();
    }

    drop(reads);
    drop(main);
    let _ = std::fs::remove_file(&work);
    eprintln!(
        "OK: speaker names matched oracle ({} cases, {} probes, {} name pairs, {} character reads, {} labels).",
        oracle.cases.len(),
        oracle.probes.len(),
        name_rows,
        read_total,
        label_rows
    );
}
