//! Tier-1 differential for the wardrobe item picture prompt (P4.D263 item 5):
//! `quilltap-core::services::wardrobe_item_image_prompt::{build_wardrobe_item_
//! image_prompt, build_wardrobe_item_cue}` and `services::avatar_prompt::
//! build_figure_identity_block` vs v4's REAL `buildWardrobeItemImagePrompt` /
//! `buildWardrobeItemCue` / `buildFigureIdentityBlock` at the pin, EXACT.
//!
//! The corpus (`harness/oracle/fixtures/wardrobe-item-image-prompt.json`)
//! covers garment / outfit / hair-only (garment and outfit) / archived owner /
//! no owner, the pronoun → noun mapping (she / he / they / none), BOTH figure
//! ladders (a head-and-shoulders-only description walked by the full-length
//! ladder, a short-only one, a blank full description), the period strip,
//! multi-type and type-less components, an empty `imagePrompt`, and the
//! aesthetic preamble (trimmed, whitespace-only, empty, exactly 600, 601, and
//! a surrogate pair straddling the 600-unit cap — compared as the bytes a
//! provider receives; see the oracle header).
//!
//! `resolveAesthetic` is the one seam (mocked in the oracle to answer each
//! case's `aesthetic`; the Rust builder takes it as an argument).
//!
//! Regenerate (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-wiip-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/wardrobe-item-image-prompt.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/wardrobe-item-image-prompt.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-wardrobe-item-image-prompt.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- "cases/wardrobe-item-image-prompt\.test\.ts$"
//! Run:
//!   QT_ORACLE_WIIP=/tmp/oracle-wardrobe-item-image-prompt.ndjson \
//!     cargo test -p quilltap-harness --test wardrobe_item_image_prompt_equivalence

use std::path::Path;

use quilltap_core::image_gen::Orientation;
use quilltap_core::services::avatar_prompt::{build_figure_identity_block, FigureFraming};
use quilltap_core::services::wardrobe_item_image_prompt::{
    build_wardrobe_item_cue, build_wardrobe_item_image_prompt,
};
use serde_json::{json, Value};

fn orientation_str(o: Orientation) -> &'static str {
    match o {
        Orientation::Portrait => "portrait",
        Orientation::Landscape => "landscape",
        Orientation::Square => "square",
    }
}

#[test]
fn wardrobe_item_image_prompt_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_WIIP") else {
        eprintln!("SKIP: set QT_ORACLE_WIIP to the oracle NDJSON (see header).");
        return;
    };
    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/wardrobe-item-image-prompt.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let cases = spec["cases"].as_array().unwrap();
    assert_eq!(oracle.len(), cases.len(), "oracle case count != corpus");

    let mut failures = Vec::new();
    for (case, want) in cases.iter().zip(&oracle) {
        let name = case["name"].as_str().unwrap();
        assert_eq!(want["name"].as_str(), Some(name), "oracle order");
        let components: Vec<Value> = case["components"].as_array().unwrap().clone();
        let owner = case.get("owner").filter(|o| !o.is_null());
        let built = build_wardrobe_item_image_prompt(
            &case["item"],
            &components,
            owner,
            case.get("aesthetic").and_then(Value::as_str),
        );
        let figure = |f| {
            let b = build_figure_identity_block(owner.unwrap(), f);
            json!({
                "subjectNoun": b.subject_noun,
                "physicalText": b.physical_text,
                "physBlock": b.phys_block,
            })
        };
        let got = json!({
            "name": name,
            "prompt": built.prompt,
            "utf16Length": built.prompt.encode_utf16().count(),
            "orientation": orientation_str(built.orientation),
            "subject": built.subject.as_str(),
            "cue": build_wardrobe_item_cue(&case["item"], &components),
            "figure": owner.map(|_| json!({
                "headAndShoulders": figure(FigureFraming::HeadAndShoulders),
                "fullLength": figure(FigureFraming::FullLength),
            })),
        });
        for key in ["prompt", "orientation", "subject", "cue", "figure"] {
            if got[key] != want[key] {
                failures.push(format!(
                    "{name}: {key}\n  rust:   {}\n  oracle: {}",
                    got[key], want[key]
                ));
            }
        }
        // The in-memory length agrees except where v4's slice split a pair
        // (its lone surrogate is one unit; the decoded U+FFFD is one unit too).
        if got["utf16Length"] != want["utf16Length"] {
            failures.push(format!(
                "{name}: utf16Length rust {} oracle {}",
                got["utf16Length"], want["utf16Length"]
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} divergence(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
