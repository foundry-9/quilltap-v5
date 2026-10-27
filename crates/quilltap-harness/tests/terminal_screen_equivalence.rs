//! P4.D337 tier-1 differential: the server-side terminal screen model (v4
//! `lib/terminal/screen.ts` over `@xterm/headless` 6.0.0, bug 255, v4
//! `a6292f169`) vs `quilltap_core::terminal_screen` (over `vt100` 0.18.1).
//!
//! The oracle drives v4's REAL `TerminalScreen` / `renderTranscript` through
//! the corpus `harness/oracle/fixtures/terminal-screen.json` and echoes every
//! case's EXPANDED step list as `input` — the synthetic Claude-Code-shaped
//! alternate-screen stream is generated there from a fixed seed, so this side
//! replays the oracle's own bytes and never regenerates them. Every `render`,
//! `takeSummary` and `transcript` answer is compared EXACTLY, as
//! `serde_json::to_string` bytes (never `Value ==`, which ignores key order).
//!
//! Three divergence classes are named both-ways tables ([`DIVERGENCES`]): for
//! each listed output v4's answer and v5's answer are each asserted against a
//! literal, so a convergence on either side trips it. `SCREEN_RESIZE_NO_REFLOW`
//! is ruled (R-D); the other two were MEASURED by this family's corpus and are
//! recorded, not planned:
//!
//! - `SCREEN_ERASE_UNLINKS_CONTINUATION` — xterm keeps its wrap flag on the
//!   line that CONTINUES the one above (`isWrapped`) and clears it when an
//!   erase starts at column 0 (`EL 0` at the left margin, `EL 2`); vt100 keeps
//!   it on the row that wraps INTO the next and clears it only when that row's
//!   last cell changes, with no public way to clear it from outside. A line
//!   rewritten from its start after soft-wrapping (`\r ESC[K new text`) stays
//!   joined to the line above here. (Editing inside a wrapped line — `ICH` /
//!   `DCH` — and a readline-style long command both MATCH; corpus rows.)
//! - `SCREEN_MARKER_LINE_SPLICE` — xterm moves (or disposes) the summary marker
//!   when lines are inserted or deleted inside the screen (`IL` / `DL`, region
//!   scrolls splice its circular buffer); here the marker is a line index that
//!   only the scrollback's drops and the `ED` resets move, so a plain-shell
//!   summary after an `IL` above the marker starts one line early. (The `DL`
//!   row happens to match.)
//!
//! Generate the oracle output (Node 24, from the v4 checkout — `screen.ts`
//! arrived at `a6292f169`, past the `e8e17b2d9` baseline, so regenerate from a
//! pin at or after it through the sweep driver until the baseline moves):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/terminal-screen.ts > /tmp/oracle-terminal-screen.ndjson
//! Run:
//!   cd $V5W
//!   QT_ORACLE_TERMINAL_SCREEN=/tmp/oracle-terminal-screen.ndjson cargo test -p quilltap-harness --test terminal_screen_equivalence

use quilltap_core::terminal_screen::{render_transcript, TerminalScreen, SCREEN_SCROLLBACK_LINES};
use serde_json::Value;

/// `(table, case, output index, v4's output bytes, v5's output bytes)`.
///
/// `SCREEN_RESIZE_NO_REFLOW` (R-D, ruled): xterm re-wraps the normal buffer on
/// a column change, `vt100` truncates / pads each row and clears its wrap flag.
const DIVERGENCES: &[(&str, &str, usize, &str, &str)] = &[
    (
        "SCREEN_RESIZE_NO_REFLOW",
        "resize-reflow-narrow",
        0,
        r#"{"buffer":"normal","lines":["012345678901234567890123456789012345678901234567890123456789","$"],"cols":40,"rows":4}"#,
        r#"{"buffer":"normal","lines":["0123456789012345678901234567890123456789","$"],"cols":40,"rows":4}"#,
    ),
    (
        "SCREEN_RESIZE_NO_REFLOW",
        "resize-reflow-wide",
        0,
        r#"{"buffer":"normal","lines":["0123456789012345678901234","$"],"cols":40,"rows":4}"#,
        r#"{"buffer":"normal","lines":["01234567890123456789","01234","$"],"cols":40,"rows":4}"#,
    ),
    (
        "SCREEN_ERASE_UNLINKS_CONTINUATION",
        "edge-el-clears-continuation",
        0,
        r#"{"buffer":"normal","lines":["0123456789","xyz","$"],"cols":10,"rows":4}"#,
        r#"{"buffer":"normal","lines":["0123456789xyz","$"],"cols":10,"rows":4}"#,
    ),
    (
        "SCREEN_ERASE_UNLINKS_CONTINUATION",
        "edge-el2-clears-continuation",
        0,
        r#"{"buffer":"normal","lines":["0123456789","xyz","$"],"cols":10,"rows":4}"#,
        r#"{"buffer":"normal","lines":["0123456789xyz","$"],"cols":10,"rows":4}"#,
    ),
    (
        "SCREEN_MARKER_LINE_SPLICE",
        "edge-marker-insert-line",
        1,
        r#"{"kind":"output","text":"more"}"#,
        r#"{"kind":"output","text":"b\nmore"}"#,
    ),
];

/// Replay one case's expanded steps through v5, answering one JSON value per
/// output step.
fn replay(case: &Value) -> Vec<String> {
    let cols = case["cols"].as_u64().expect("cols") as u16;
    let rows = case["rows"].as_u64().expect("rows") as u16;
    let mut screen = match case["scrollback"].as_u64() {
        Some(n) => TerminalScreen::with_scrollback(cols, rows, n as usize),
        None => TerminalScreen::new(cols, rows),
    };
    let mut outputs = Vec::new();
    for step in case["input"].as_array().expect("input") {
        if let Some(data) = step.get("write") {
            screen.write(data.as_str().expect("write string"));
        } else if let Some(size) = step.get("resize") {
            let c = size[0].as_u64().expect("resize cols") as u16;
            let r = size[1].as_u64().expect("resize rows") as u16;
            screen.resize(c, r);
        } else if step.get("render").is_some() {
            outputs.push(serde_json::to_string(&screen.render()).unwrap());
        } else if step.get("takeSummary").is_some() {
            outputs.push(serde_json::to_string(&screen.take_summary()).unwrap());
        } else if let Some(t) = step.get("transcript") {
            let r = render_transcript(
                t["raw"].as_str().expect("raw"),
                t["cols"].as_u64().expect("cols") as u16,
                t["rows"].as_u64().expect("rows") as u16,
            );
            outputs.push(serde_json::to_string(&r).unwrap());
        } else {
            panic!("unknown step in {}: {step}", case["name"]);
        }
    }
    outputs
}

#[test]
fn terminal_screen_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_TERMINAL_SCREEN") else {
        eprintln!("SKIP: set QT_ORACLE_TERMINAL_SCREEN to the oracle NDJSON (see header).");
        return;
    };
    let body = std::fs::read_to_string(&path).expect("read oracle");
    let mut cases = 0usize;
    let mut outputs_compared = 0usize;
    let mut saw_scrollback_line = false;
    let mut table_hits = 0usize;
    let mut failures: Vec<String> = Vec::new();
    for line in body.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("ndjson row");
        if let Some(n) = row.get("scrollbackLines") {
            assert_eq!(n.as_u64(), Some(SCREEN_SCROLLBACK_LINES as u64));
            saw_scrollback_line = true;
            continue;
        }
        cases += 1;
        let name = row["name"].as_str().expect("name");
        let expected: Vec<String> = row["outputs"]
            .as_array()
            .expect("outputs")
            .iter()
            .map(|v| serde_json::to_string(v).unwrap())
            .collect();
        let got = replay(&row);
        assert_eq!(expected.len(), got.len(), "{name}: output count");
        for (i, (e, g)) in expected.iter().zip(&got).enumerate() {
            if let Some((table, _, _, v4, v5)) = DIVERGENCES
                .iter()
                .find(|(_, n, idx, _, _)| *n == name && *idx == i)
            {
                table_hits += 1;
                assert_eq!(e, v4, "{table} {name}[{i}]: v4 converged off the table");
                assert_eq!(g, v5, "{table} {name}[{i}]: v5 converged off the table");
                continue;
            }
            outputs_compared += 1;
            if e != g {
                failures.push(format!("{name}[{i}]\n  v4: {e}\n  v5: {g}"));
            }
        }
    }
    assert!(
        saw_scrollback_line,
        "oracle lacks the trailing scrollbackLines row"
    );
    assert_eq!(
        table_hits,
        DIVERGENCES.len(),
        "a divergence-table row is missing"
    );
    assert!(cases >= 60, "only {cases} cases — a stale oracle?");
    eprintln!("terminal_screen: {cases} cases, {outputs_compared} outputs compared");
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The bug-255 shapes — v4's own cases and the synthetic Claude-Code stream —
/// must render differently from v5's old strip over the same bytes on every
/// alternate-screen output: a row the strip already answered would prove
/// nothing about the emulator. (The `edge-` rows are xterm-parity rows, where a
/// strip-equal answer is fine — `?2026` around one word, say.)
#[test]
fn alternate_screen_rows_differ_from_the_strip() {
    let Ok(path) = std::env::var("QT_ORACLE_TERMINAL_SCREEN") else {
        eprintln!("SKIP: set QT_ORACLE_TERMINAL_SCREEN to the oracle NDJSON (see header).");
        return;
    };
    let body = std::fs::read_to_string(&path).expect("read oracle");
    let mut checked = 0;
    for line in body.lines() {
        let row: Value = serde_json::from_str(line).expect("ndjson row");
        let Some(name) = row["name"].as_str() else {
            continue;
        };
        if !(name.starts_with("v4-") || name.starts_with("synth-")) {
            continue;
        }
        let writes: String = row["input"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|s| s.get("write").and_then(Value::as_str))
            .collect();
        for out in row["outputs"].as_array().unwrap() {
            if out["buffer"] == "alternate" {
                let rendered = out["lines"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|l| l.as_str().unwrap())
                    .collect::<Vec<_>>()
                    .join("\n");
                let stripped = quilltap_core::terminal_clean::clean_terminal_output(&writes);
                assert_ne!(
                    rendered, stripped,
                    "{name}: the strip already answers this row"
                );
                checked += 1;
            }
        }
    }
    assert!(checked >= 3, "only {checked} alternate-screen outputs");
}
