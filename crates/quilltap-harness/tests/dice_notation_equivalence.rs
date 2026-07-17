//! Tier-1 differential (P4.d5 unit 1): the shared dice module
//! (`quilltap_core::pascal::dice` — v4 `lib/pascal/dice.ts`, new at `a33ac8b8`).
//!
//! Exact field-by-field equivalence against v4's REAL exports over a committed
//! corpus: `parseDiceNotation` (strict), `scanDiceNotation` (prose),
//! `formatDiceNotation`, `rollNotation` + `formatDiceBreakdown` (over a
//! committed byte stream), `flipCoin`, and the five bounds constants.
//!
//! What this proves beyond the TS side's own tests — the JS→Rust fidelity traps:
//!   * the two regexes' DELIBERATE whitespace disagreement (the scan form
//!     forbids space around the sign so "2d6 - 1 apple" stays a plain 2d6; the
//!     strict form allows it);
//!   * JS `\s` vs Rust `\s` in BOTH directions (U+FEFF is JS whitespace and Rust
//!     `\s` excludes it; U+0085 NEL is not and Rust `\s` includes it) — hence
//!     `jsstr::JS_WS_CLASS`;
//!   * ASCII `\b`/`\d` (`(?-u:\b)` / `[0-9]`) and non-ASCII adjacency;
//!   * the trailing-`\b` leftmost-first fallback ("3d6+2x" → a bare 3d6);
//!   * skip-never-clamp bounds, including digit strings past 2^53;
//!   * byte-stream parity — each roll case must draw EXACTLY the committed bytes,
//!     proving the rejection-sampling loop survived v4's move of the roller into
//!     `dice.ts` unchanged.
//!
//! Generate the oracle (Node 24, from the v4 checkout). Jest ignores `.claude/`
//! paths, so a worktree lane MUST mirror the case file to /tmp
//! ([[w4-4b-file-attachment-gotchas]]) and point `--roots` at the mirror
//! ([[oracle-regen-from-worktree-path]] — mirror from the WORKTREE, not main,
//! and verify by grepping the NDJSON for a row only this corpus emits):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; W=<this worktree>
//!   mkdir -p /tmp/qt-oracle-dice
//!   cp $W/harness/oracle/cases/dice-notation.test.ts /tmp/qt-oracle-dice/
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-dice-notation.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" \
//!       --roots /tmp/qt-oracle-dice -- dice-notation
//! Run:
//!   QT_ORACLE_DICE_NOTATION=/tmp/oracle-dice-notation.ndjson \
//!     cargo test -p quilltap-harness --test dice_notation_equivalence

use quilltap_core::pascal::dice::{
    flip_coin, format_dice_breakdown, format_dice_notation, parse_dice_notation, roll_notation,
    scan_dice_notation, DiceNotation, FixedBytes, MAX_DICE_COUNT, MAX_DICE_MODIFIER, MAX_DIE_SIDES,
    MIN_DICE_COUNT, MIN_DIE_SIDES,
};
use serde_json::{json, Value};

/// v4 serializes a `DiceNotation` as a flat `{count, sides, modifier}`.
fn notation_json(n: &DiceNotation) -> Value {
    json!({ "count": n.count, "sides": n.sides, "modifier": n.modifier })
}

/// Read a `{count, sides, modifier}` row back into a `DiceNotation`.
fn notation_from_json(v: &Value) -> DiceNotation {
    DiceNotation {
        count: v["count"].as_u64().expect("count") as u32,
        sides: v["sides"].as_u64().expect("sides") as u32,
        modifier: v["modifier"].as_i64().expect("modifier"),
    }
}

#[test]
fn dice_notation_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_DICE_NOTATION") else {
        eprintln!("SKIP: set QT_ORACLE_DICE_NOTATION to the oracle NDJSON (see test header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut counts = [0usize; 5]; // bounds, parse, scan, format, roll+coin
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("oracle line parses");
        let id = row["id"].as_str().unwrap_or("-");
        match row["kind"].as_str().expect("kind") {
            // The five bounds constants are part of the binding §1 surface.
            "bounds" => {
                assert_eq!(row["minDieSides"], json!(MIN_DIE_SIDES), "MIN_DIE_SIDES");
                assert_eq!(row["maxDieSides"], json!(MAX_DIE_SIDES), "MAX_DIE_SIDES");
                assert_eq!(row["minDiceCount"], json!(MIN_DICE_COUNT), "MIN_DICE_COUNT");
                assert_eq!(row["maxDiceCount"], json!(MAX_DICE_COUNT), "MAX_DICE_COUNT");
                assert_eq!(
                    row["maxDiceModifier"],
                    json!(MAX_DICE_MODIFIER),
                    "MAX_DICE_MODIFIER"
                );
                counts[0] += 1;
            }

            // parseDiceNotation — strict; null when unparseable OR out of bounds.
            "parse" => {
                let input = row["input"].as_str().expect("input");
                let got = match parse_dice_notation(input) {
                    Some(n) => notation_json(&n),
                    None => Value::Null,
                };
                assert_eq!(
                    got, row["result"],
                    "parseDiceNotation mismatch for '{id}' (input {input:?})"
                );
                counts[1] += 1;
            }

            // scanDiceNotation — prose; a flat {count, sides, modifier, matchText}.
            "scan" => {
                let input = row["input"].as_str().expect("input");
                let got: Vec<Value> = scan_dice_notation(input)
                    .iter()
                    .map(|hit| {
                        let mut v = notation_json(&hit.notation);
                        v["matchText"] = json!(hit.match_text);
                        v
                    })
                    .collect();
                assert_eq!(
                    json!(got),
                    row["results"],
                    "scanDiceNotation mismatch for '{id}' (input {input:?})"
                );
                counts[2] += 1;
            }

            // formatDiceNotation.
            "format" => {
                let n = notation_from_json(&row["notation"]);
                assert_eq!(
                    json!(format_dice_notation(&n)),
                    row["result"],
                    "formatDiceNotation mismatch for '{id}'"
                );
                counts[3] += 1;
            }

            // rollNotation + formatDiceBreakdown, over the committed byte stream.
            "roll" => {
                let n = notation_from_json(&row["notation"]);
                let bytes: Vec<u8> = serde_json::from_value(row["bytes"].clone()).expect("bytes");
                let mut rng = FixedBytes::new(bytes.clone());
                let got = roll_notation(&n, &mut rng);

                // Byte-stream parity: the rejection-sampling loop must draw exactly
                // the sequence v4's mock served — the property v4's move of the
                // roller into dice.ts had to preserve.
                assert_eq!(
                    rng.consumed(),
                    bytes.len(),
                    "roll case '{id}': consumed {} bytes, committed {}",
                    rng.consumed(),
                    bytes.len()
                );

                let mut got_json = notation_json(&DiceNotation {
                    count: got.count,
                    sides: got.sides,
                    modifier: got.modifier,
                });
                got_json["results"] = json!(got.results);
                got_json["subtotal"] = json!(got.subtotal);
                got_json["total"] = json!(got.total);
                assert_eq!(got_json, row["result"], "rollNotation mismatch for '{id}'");
                assert_eq!(
                    json!(format_dice_breakdown(&got)),
                    row["breakdown"],
                    "formatDiceBreakdown mismatch for '{id}'"
                );
                counts[4] += 1;
            }

            // flipCoin — bare 'heads'/'tails' strings (v5 lifts them into
            // RngResult at the rng call site, not here).
            "coin" => {
                let count = row["count"].as_u64().expect("count") as u32;
                let bytes: Vec<u8> = serde_json::from_value(row["bytes"].clone()).expect("bytes");
                let mut rng = FixedBytes::new(bytes.clone());
                let got = flip_coin(count, &mut rng);
                assert_eq!(
                    rng.consumed(),
                    bytes.len(),
                    "coin case '{id}': consumed {} bytes, committed {}",
                    rng.consumed(),
                    bytes.len()
                );
                assert_eq!(json!(got), row["results"], "flipCoin mismatch for '{id}'");
                counts[4] += 1;
            }

            other => panic!("unknown oracle row kind {other:?}"),
        }
    }

    assert_eq!(counts[0], 1, "the bounds row must appear exactly once");
    assert!(counts[1] >= 25, "parse corpus too small: {}", counts[1]);
    assert!(counts[2] >= 20, "scan corpus too small: {}", counts[2]);
    assert!(counts[3] >= 6, "format corpus too small: {}", counts[3]);
    assert!(counts[4] >= 10, "roll/coin corpus too small: {}", counts[4]);
    eprintln!(
        "OK: dice-notation matched oracle (bounds {}, parse {}, scan {}, format {}, roll+coin {}).",
        counts[0], counts[1], counts[2], counts[3], counts[4]
    );
}
