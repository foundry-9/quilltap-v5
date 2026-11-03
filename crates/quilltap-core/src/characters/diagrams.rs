//! The server half of v4 `lib/markdown/diagrams.ts` (`6588972d5`): the two
//! values a character's `diagramStyle` may take and the lenient reader.
//!
//! v4's module is the single source of truth for which fence languages count as
//! diagrams AND for the style values. The fence half (`DIAGRAM_LANGUAGES`,
//! `diagramEngineFor`, `hasDiagramFence`) is the client's — the Salon draws the
//! pictures — and lives in the SPA (P4.D353); only the style half is read on the
//! server (the vault `properties.json` writers / readers and the wire DTOs).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// v4 `DIAGRAM_STYLES`. `handDrawn` sketches Mermaid (its own hand-drawn look)
/// and Graphviz (re-drawn with rough.js); `clean` leaves both as their engines
/// drew them. Raw SVG ignores the style.
pub const DIAGRAM_STYLES: [&str; 2] = ["handDrawn", "clean"];

/// v4 `DEFAULT_DIAGRAM_STYLE` — what an absent (or unrecognized) `diagramStyle`
/// reads as.
pub const DEFAULT_DIAGRAM_STYLE: &str = "handDrawn";

/// v4 `DiagramStyle`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagramStyle {
    #[default]
    #[serde(rename = "handDrawn")]
    HandDrawn,
    #[serde(rename = "clean")]
    Clean,
}

impl DiagramStyle {
    pub fn as_str(self) -> &'static str {
        match self {
            DiagramStyle::HandDrawn => "handDrawn",
            DiagramStyle::Clean => "clean",
        }
    }
}

/// v4 `diagramStyleOrDefault`: read a stored `diagramStyle` leniently.
/// `properties.json` is hand-editable, so anything that isn't one of the two
/// known values reads as the default rather than failing the strict parse and
/// hollowing every managed field.
pub fn diagram_style_or_default(value: Option<&Value>) -> DiagramStyle {
    match value.and_then(Value::as_str) {
        Some("clean") => DiagramStyle::Clean,
        // `handDrawn`, any other string, absent, null, a number, an object …
        _ => DiagramStyle::HandDrawn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_two_known_values_read_as_themselves() {
        assert_eq!(
            diagram_style_or_default(Some(&json!("handDrawn"))),
            DiagramStyle::HandDrawn
        );
        assert_eq!(
            diagram_style_or_default(Some(&json!("clean"))),
            DiagramStyle::Clean
        );
    }

    #[test]
    fn anything_else_reads_as_the_default_and_never_panics() {
        let odd = [
            None,
            Some(json!(null)),
            Some(json!("crayon")),
            Some(json!("Clean")),
            Some(json!(" clean")),
            Some(json!("")),
            Some(json!(5)),
            Some(json!(true)),
            Some(json!([])),
            Some(json!(["clean"])),
            Some(json!({"style": "clean"})),
        ];
        for v in &odd {
            assert_eq!(
                diagram_style_or_default(v.as_ref()),
                DiagramStyle::HandDrawn,
                "{v:?}"
            );
        }
        assert_eq!(DEFAULT_DIAGRAM_STYLE, DiagramStyle::default().as_str());
        assert_eq!(DIAGRAM_STYLES, ["handDrawn", "clean"]);
    }

    #[test]
    fn serde_spells_v4s_values() {
        assert_eq!(
            serde_json::to_string(&DiagramStyle::HandDrawn).unwrap(),
            "\"handDrawn\""
        );
        assert_eq!(
            serde_json::to_string(&DiagramStyle::Clean).unwrap(),
            "\"clean\""
        );
    }
}
