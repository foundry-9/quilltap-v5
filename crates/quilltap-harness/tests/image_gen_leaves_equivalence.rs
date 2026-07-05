//! Differential test (W4.1d5, tier-1, DB-free): the image-generation pure leaves
//! `parse_placeholders` + `resolve_orientation`
//! (`quilltap_core::image_gen`) vs v4's REAL `parsePlaceholders` / `resolveOrientation`
//! (with the plugin registry mocked to canned declarations). Diffs
//! `serde_json::to_string` against v4's `JSON.stringify`.
//!
//! Generate the oracle (Node 24, from the v4 checkout; STAGE outside `.claude/`):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; WT=<worktree> ; STAGE=/tmp/qt-oracle-stage
//!   rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases
//!   cp $WT/harness/oracle/cases/image-gen-leaves.test.ts $STAGE/harness/oracle/cases/
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-image-gen-leaves.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- image-gen-leaves
//! Run:
//!   QT_ORACLE_IMAGE_GEN_LEAVES=/tmp/oracle-image-gen-leaves.ndjson \
//!     cargo test -p quilltap-harness --test image_gen_leaves_equivalence

use std::collections::HashMap;

use quilltap_core::image_gen::{
    parse_placeholders, resolve_orientation, ModelInfo, Orientation, OrientationMapping,
    OrientationStrategy, OrientationSupport,
};
use serde_json::Value;

fn load_oracle(path: &str) -> HashMap<String, Value> {
    let mut map = HashMap::new();
    for line in std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read oracle {path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let row: Value = serde_json::from_str(line).expect("oracle line parses");
        map.insert(row["label"].as_str().unwrap().to_string(), row);
    }
    map
}

fn m(
    size: Option<&str>,
    ar: Option<&str>,
    hint: Option<&str>,
    nw: Option<f64>,
    nh: Option<f64>,
) -> OrientationMapping {
    OrientationMapping {
        size: size.map(str::to_string),
        aspect_ratio: ar.map(str::to_string),
        prompt_hint: hint.map(str::to_string),
        nominal_width: nw,
        nominal_height: nh,
    }
}

fn size_support() -> OrientationSupport {
    OrientationSupport {
        strategy: OrientationStrategy::Size,
        portrait: m(Some("1024x1792"), None, None, Some(1024.0), Some(1792.0)),
        landscape: m(Some("1792x1024"), None, None, Some(1792.0), Some(1024.0)),
        square: Some(m(Some("1024x1024"), None, None, None, None)),
    }
}
fn aspect_support() -> OrientationSupport {
    OrientationSupport {
        strategy: OrientationStrategy::AspectRatio,
        portrait: m(None, Some("3:4"), None, None, None),
        landscape: m(None, Some("4:3"), None, None, None),
        square: None,
    }
}
fn prompt_support() -> OrientationSupport {
    OrientationSupport {
        strategy: OrientationStrategy::Prompt,
        portrait: m(None, None, Some("a tall vertical framing"), None, None),
        landscape: m(None, None, Some("a wide framing"), None, None),
        square: None,
    }
}
fn degrade_support() -> OrientationSupport {
    OrientationSupport {
        strategy: OrientationStrategy::Size,
        portrait: m(None, None, Some("custom portrait hint"), None, None),
        landscape: m(Some("1792x1024"), None, None, None, None),
        square: None,
    }
}
fn no_square_support() -> OrientationSupport {
    OrientationSupport {
        strategy: OrientationStrategy::Size,
        portrait: m(Some("1024x1792"), None, None, None, None),
        landscape: m(Some("1792x1024"), None, None, None, None),
        square: None,
    }
}
fn model(id: &str, support: OrientationSupport) -> ModelInfo {
    ModelInfo {
        id: id.to_string(),
        orientation_support: Some(support),
    }
}

#[test]
fn image_gen_leaves_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_IMAGE_GEN_LEAVES") else {
        eprintln!("SKIP: set QT_ORACLE_IMAGE_GEN_LEAVES (see test header).");
        return;
    };
    let oracle = load_oracle(&oracle_path);

    // ---- parse_placeholders ----
    let placeholder_cases: Vec<(&str, &str)> = vec![
        ("p_two", "A scene with {{me}} and {{Aurora}}."),
        ("p_none", "no placeholders here"),
        ("p_ws", "{{ me }}"),
        ("p_adjacent", "{{a}}{{b}}"),
        ("p_empty", "{{}}"),
        ("p_multi", "before {{x}} middle {{y}} after {{z}}"),
        ("p_unclosed", "a {{ dangling name and {{good}}"),
    ];
    for (label, prompt) in &placeholder_cases {
        let got = serde_json::to_string(&parse_placeholders(prompt)).unwrap();
        let want = oracle
            .get(*label)
            .unwrap_or_else(|| panic!("oracle missing {label}"));
        assert_eq!(
            got.as_str(),
            want["json"].as_str().unwrap(),
            "placeholders {label}"
        );
    }

    // ---- resolve_orientation ----
    struct OCase {
        label: &'static str,
        model: Option<&'static str>,
        orientation: Orientation,
        models: Option<Vec<ModelInfo>>,
        constraints: Option<OrientationSupport>,
    }
    let ocases = vec![
        OCase {
            label: "o_fallback_portrait",
            model: None,
            orientation: Orientation::Portrait,
            models: None,
            constraints: None,
        },
        OCase {
            label: "o_fallback_square",
            model: None,
            orientation: Orientation::Square,
            models: None,
            constraints: None,
        },
        OCase {
            label: "o_size_portrait",
            model: Some("dall-e-3"),
            orientation: Orientation::Portrait,
            models: Some(vec![model("dall-e-3", size_support())]),
            constraints: None,
        },
        OCase {
            label: "o_size_landscape",
            model: Some("dall-e-3"),
            orientation: Orientation::Landscape,
            models: Some(vec![model("dall-e-3", size_support())]),
            constraints: None,
        },
        OCase {
            label: "o_prefix_match",
            model: Some("dall-e-3-mini"),
            orientation: Orientation::Portrait,
            models: Some(vec![
                model("dall-e-3", size_support()),
                model("dall-e-2", aspect_support()),
            ]),
            constraints: None,
        },
        OCase {
            label: "o_aspect",
            model: Some("flux-pro"),
            orientation: Orientation::Landscape,
            models: Some(vec![model("flux-pro", aspect_support())]),
            constraints: None,
        },
        OCase {
            label: "o_prompt_strategy",
            model: Some("m"),
            orientation: Orientation::Portrait,
            models: Some(vec![model("m", prompt_support())]),
            constraints: None,
        },
        OCase {
            label: "o_degrade_to_hint",
            model: Some("m"),
            orientation: Orientation::Portrait,
            models: Some(vec![model("m", degrade_support())]),
            constraints: None,
        },
        OCase {
            label: "o_declared_absent_square",
            model: Some("m"),
            orientation: Orientation::Square,
            models: Some(vec![model("m", no_square_support())]),
            constraints: None,
        },
        OCase {
            label: "o_provider_level",
            model: None,
            orientation: Orientation::Portrait,
            models: None,
            constraints: Some(size_support()),
        },
        OCase {
            label: "o_no_match_falls_provider",
            model: Some("unknown-model"),
            orientation: Orientation::Landscape,
            models: Some(vec![model("other", aspect_support())]),
            constraints: Some(prompt_support()),
        },
    ];
    for c in &ocases {
        let got = serde_json::to_string(&resolve_orientation(
            c.models.as_deref(),
            c.model,
            c.constraints.as_ref(),
            c.orientation,
        ))
        .unwrap();
        let want = oracle
            .get(c.label)
            .unwrap_or_else(|| panic!("oracle missing {}", c.label));
        assert_eq!(
            got.as_str(),
            want["json"].as_str().unwrap(),
            "orientation {}",
            c.label
        );
    }

    eprintln!("OK: image-gen-leaves differential matched the oracle.");
}
