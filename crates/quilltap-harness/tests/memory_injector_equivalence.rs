//! Tier-1 differential test (chat-orchestration wave 1.3; RE-AIMED by
//! P4.D291): the scene-state context formatter, from v4's
//! lib/chat/context/scene-state.ts.
//!
//! Covers formatCurrentSceneState — the formatter `memory_injector.rs` keeps
//! byte-for-byte (R-L14). `formatSummaryForContext` is gone at v4 `bdd09b75b`
//! and its rows with it (P4.D347). Every memory
//! formatter this family once drove moved with v4 Phase E to
//! `lib/memory/recall/render.ts` and is pinned by `recall_render_equivalence`.
//! Every string / count / hash is compared exactly.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/memory-injector.ts \
//!     > /tmp/oracle-memory-injector.ndjson
//! Run:
//!   QT_ORACLE_MEMORY_INJECTOR=/tmp/oracle-memory-injector.ndjson \
//!     cargo test -p quilltap-harness --test memory_injector_equivalence

use quilltap_core::memory_injector::{
    format_current_scene_state, SceneState, SceneStateCharacter, SceneStateEmissionEntry,
};
use serde::Deserialize;

// ---------------------------------------------------------------------------
// Wire shapes
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct WireSceneChar {
    #[serde(rename = "characterId")]
    character_id: String,
    #[serde(rename = "characterName")]
    character_name: String,
    action: String,
    clothing: Option<String>,
}

#[derive(Deserialize)]
struct WireScene {
    location: String,
    characters: Vec<WireSceneChar>,
}

#[derive(Deserialize)]
struct WireEmission {
    #[serde(rename = "characterId")]
    character_id: String,
    #[serde(rename = "actionHash")]
    action_hash: String,
    #[serde(rename = "clothingHash")]
    clothing_hash: String,
    #[serde(rename = "emittedAt")]
    emitted_at: String,
}

#[derive(Deserialize)]
struct WirePriorEmission(String, WirePriorEntry);

#[derive(Deserialize)]
struct WirePriorEntry {
    #[serde(rename = "actionHash")]
    action_hash: String,
    #[serde(rename = "clothingHash")]
    clothing_hash: String,
    #[serde(rename = "emittedAt")]
    emitted_at: String,
}

#[derive(Deserialize)]
struct WireSceneOut {
    content: String,
    #[serde(rename = "tokenCount")]
    token_count: i64,
    #[serde(rename = "emittedByCharacter")]
    emitted_by_character: Vec<WireEmission>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "scene")]
    Scene {
        id: String,
        scene: Option<WireScene>,
        time: Option<String>,
        #[serde(rename = "nowIso")]
        now_iso: String,
        #[serde(rename = "liveClothing")]
        live_clothing: Vec<(String, String)>,
        #[serde(rename = "priorEmission")]
        prior_emission: Vec<WirePriorEmission>,
        out: WireSceneOut,
    },
}

#[test]
fn memory_injector_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_MEMORY_INJECTOR") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_MEMORY_INJECTOR to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut scene_rows = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("parse error on line: {e}\n{line}"));
        match row {
            Row::Scene {
                id,
                scene,
                time,
                now_iso,
                live_clothing,
                prior_emission,
                out,
            } => {
                let scene = scene.map(|s| SceneState {
                    location: s.location,
                    characters: s
                        .characters
                        .into_iter()
                        .map(|c| SceneStateCharacter {
                            character_id: c.character_id,
                            character_name: c.character_name,
                            action: c.action,
                            clothing: c.clothing,
                        })
                        .collect(),
                });
                let live: std::collections::HashMap<String, String> =
                    live_clothing.into_iter().collect();
                let prior: std::collections::HashMap<String, SceneStateEmissionEntry> =
                    prior_emission
                        .into_iter()
                        .map(|WirePriorEmission(k, e)| {
                            (
                                k,
                                SceneStateEmissionEntry {
                                    action_hash: e.action_hash,
                                    clothing_hash: e.clothing_hash,
                                    emitted_at: e.emitted_at,
                                },
                            )
                        })
                        .collect();

                let got = format_current_scene_state(
                    scene.as_ref(),
                    time.as_deref(),
                    &now_iso,
                    &|cid| live.get(cid).cloned(),
                    &|cid| prior.get(cid).cloned(),
                );
                assert_eq!(got.content, out.content, "scene '{id}' content");
                assert_eq!(got.token_count, out.token_count, "scene '{id}' tokenCount");
                assert_eq!(
                    got.emitted_by_character.len(),
                    out.emitted_by_character.len(),
                    "scene '{id}' emitted len"
                );
                for (g, w) in got
                    .emitted_by_character
                    .iter()
                    .zip(out.emitted_by_character.iter())
                {
                    assert_eq!(g.0, w.character_id, "scene '{id}' emitted id");
                    assert_eq!(g.1.action_hash, w.action_hash, "scene '{id}' actionHash");
                    assert_eq!(
                        g.1.clothing_hash, w.clothing_hash,
                        "scene '{id}' clothingHash"
                    );
                    assert_eq!(g.1.emitted_at, w.emitted_at, "scene '{id}' emittedAt");
                }
                scene_rows += 1;
            }
        }
    }

    // 13 scene rows (incl. the unchanged / changed emission-cache pair): a
    // regenerated oracle that lost an arm must not pass.
    assert!(scene_rows >= 13, "scene rows lost: {scene_rows} < 13");
    eprintln!("OK: scene-state matched oracle ({scene_rows} scene rows).");
}
