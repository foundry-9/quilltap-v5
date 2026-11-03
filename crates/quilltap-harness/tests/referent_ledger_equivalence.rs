//! Tier-1 differential (P4.D347, v4 `f7a1789df` / `9a88f862f`): the referent
//! ledger and the closing echo — `buildReferentLedger` / `buildClosingEcho`
//! (`lib/chat/context/referent-ledger.ts`) against
//! `quilltap_core::services::referent_ledger`. The private `sentenceAround`
//! (the quote) and `whereNamed` (`by …, N lines back`) are pinned through the
//! ledger entry bytes; `rendered` pins v4's laziness (a card is rendered only
//! for a LISTED person not in the room).
//!
//! ## The named divergences (both ways)
//!
//! A JS `slice` can cut a surrogate pair and yield a LONE surrogate (written by
//! `JSON.stringify` as a `\udXXX` escape); a Rust `String` cannot hold one, so
//! v5 rounds the cut to the code-point boundary — one unit shorter, never
//! longer. [`SURROGATE_DIVERGENCES`] names the rows: each MUST still carry a
//! lone surrogate in v4's output (so a v4 convergence trips the table), and the
//! comparand is v4's output with that lone unit removed, which v5 must equal
//! exactly. A lone surrogate on any OTHER row is a failure.
//!
//! Generate the oracle output (Node 24, from the pinned v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/referent-ledger.ts \
//!     > /tmp/oracle-referent-ledger.ndjson
//! Run:
//!   QT_ORACLE_REFERENT_LEDGER=/tmp/oracle-referent-ledger.ndjson \
//!     cargo test -p quilltap-harness --test referent_ledger_equivalence -- --nocapture

use std::cell::RefCell;

use quilltap_core::services::referent_ledger::{
    build_closing_echo, build_referent_ledger, ReferentLedgerLine, ReferentLedgerPerson,
};
use serde::Deserialize;

/// `(row id, divergence name)` — v4's output carries a lone surrogate here.
const SURROGATE_DIVERGENCES: &[(&str, &str)] = &[
    ("sentence-slice-surrogate-from", "SENTENCE_SLICE_SURROGATE"),
    ("sentence-slice-surrogate-to", "SENTENCE_SLICE_SURROGATE"),
    ("echo-slice-surrogate", "ECHO_SLICE_SURROGATE"),
];

#[derive(Deserialize)]
struct Pronouns {
    subject: String,
    object: String,
    possessive: String,
}

#[derive(Deserialize)]
struct PersonW {
    id: String,
    name: String,
    #[serde(default)]
    aliases: Option<Vec<String>>,
    #[serde(default)]
    pronouns: Option<Pronouns>,
    #[serde(rename = "inRoom")]
    in_room: bool,
    #[serde(default)]
    card: Option<String>,
}

#[derive(Deserialize)]
struct LineW {
    speaker: String,
    #[serde(rename = "isResponder")]
    is_responder: bool,
    content: String,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "ledger")]
    Ledger {
        id: String,
        lines: Vec<LineW>,
        people: Vec<PersonW>,
        #[serde(rename = "responderId")]
        responder_id: String,
        out: String,
        rendered: Vec<String>,
    },
    #[serde(rename = "echo")]
    Echo {
        id: String,
        #[serde(rename = "newUserMessage")]
        new_user_message: Option<String>,
        #[serde(rename = "speakerName")]
        speaker_name: String,
        out: String,
    },
}

/// Remove every `\udXXX` escape from a JSON text line. `JSON.stringify` writes
/// a well-formed astral character raw, so an escape there is a LONE surrogate.
fn strip_lone_surrogates(line: &str) -> (String, usize) {
    let mut out = String::with_capacity(line.len());
    let mut removed = 0usize;
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\'
            && i + 5 < bytes.len()
            && bytes[i + 1] == b'u'
            && bytes[i + 2].eq_ignore_ascii_case(&b'd')
            && matches!(bytes[i + 3].to_ascii_lowercase(), b'8'..=b'9' | b'a'..=b'f')
        {
            removed += 1;
            i += 6;
            continue;
        }
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            // Keep any other escape (incl. `\\`) whole.
            out.push_str(&line[i..i + 2]);
            i += 2;
            continue;
        }
        let ch = line[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    (out, removed)
}

fn row_id(line: &str) -> String {
    let start = line.find("\"id\":\"").expect("row id") + 6;
    let end = start + line[start..].find('"').unwrap();
    line[start..end].to_string()
}

#[test]
fn referent_ledger_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_REFERENT_LEDGER") else {
        eprintln!("SKIP: set QT_ORACLE_REFERENT_LEDGER to the oracle NDJSON (see test header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut failures = Vec::new();
    let (mut ledgers, mut echoes, mut divergences) = (0usize, 0usize, 0usize);
    for raw in text.lines().filter(|l| !l.trim().is_empty()) {
        let id = row_id(raw);
        let (line, lone) = strip_lone_surrogates(raw);
        let named = SURROGATE_DIVERGENCES.iter().find(|(r, _)| *r == id);
        match (named, lone) {
            (Some((_, name)), 0) => failures.push(format!(
                "{id}: {name} — v4 no longer emits a lone surrogate (converged?); retire the row"
            )),
            (Some(_), _) => divergences += 1,
            (None, 0) => {}
            (None, n) => failures.push(format!(
                "{id}: {n} unexpected lone surrogate(s) in v4's output"
            )),
        }
        match serde_json::from_str::<Row>(&line).unwrap_or_else(|e| panic!("{id}: {e}")) {
            Row::Ledger {
                id,
                lines,
                people,
                responder_id,
                out,
                rendered,
            } => {
                let lines: Vec<ReferentLedgerLine> = lines
                    .into_iter()
                    .map(|l| ReferentLedgerLine {
                        speaker: l.speaker,
                        is_responder: l.is_responder,
                        content: l.content,
                    })
                    .collect();
                let calls: RefCell<Vec<String>> = RefCell::new(Vec::new());
                let people: Vec<ReferentLedgerPerson<'_>> = people
                    .into_iter()
                    .map(|p| {
                        let pid = p.id.clone();
                        let calls = &calls;
                        ReferentLedgerPerson {
                            id: p.id,
                            name: p.name,
                            aliases: p.aliases,
                            pronouns: p.pronouns.map(|x| (x.subject, x.object, x.possessive)),
                            in_room: p.in_room,
                            render_card: p.card.map(|card| {
                                Box::new(move || {
                                    calls.borrow_mut().push(pid.clone());
                                    card.clone()
                                }) as Box<dyn Fn() -> String + '_>
                            }),
                        }
                    })
                    .collect();
                let got = build_referent_ledger(&lines, &people, &responder_id);
                if got != out {
                    failures.push(format!("{id}: ledger\n   v4 {out:?}\n   v5 {got:?}"));
                }
                if *calls.borrow() != rendered {
                    failures.push(format!(
                        "{id}: rendered v4 {rendered:?} v5 {:?}",
                        calls.borrow()
                    ));
                }
                ledgers += 1;
            }
            Row::Echo {
                id,
                new_user_message,
                speaker_name,
                out,
            } => {
                let got = build_closing_echo(new_user_message.as_deref(), &speaker_name);
                if got != out {
                    failures.push(format!("{id}: echo\n   v4 {out:?}\n   v5 {got:?}"));
                }
                echoes += 1;
            }
        }
    }
    assert_eq!(
        divergences,
        SURROGATE_DIVERGENCES.len(),
        "a named divergence row went missing"
    );
    assert!(
        ledgers >= 25 && echoes >= 12,
        "oracle looks partial: {ledgers} ledger, {echoes} echo rows"
    );
    assert!(
        failures.is_empty(),
        "{} rows differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
    eprintln!(
        "OK: referent-ledger matched oracle ({ledgers} ledger, {echoes} echo rows; {divergences} named surrogate divergences)."
    );
}
