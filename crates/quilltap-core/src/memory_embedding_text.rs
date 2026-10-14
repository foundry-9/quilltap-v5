//! The one text a memory is embedded as — v4
//! `lib/memory/index-store/embedding-text.ts` (+ `buildMemoryAnchorLine` from
//! `lib/memory/episodic.ts`) at the pin `83334b9a5` (P4.D289, C1-A item 7).
//!
//! `summary\n\ncontent`, plus the episodic anchor line
//! (`(when: 2026-07-14 · place: Lighthouse Point)`) when the memory carries
//! any anchor. Admission, the create paths, re-embeds, reinforcement,
//! `EMBEDDING_GENERATE`, the TF-IDF refit and the anchor probe all call this:
//! a writer that embeds a different shape gives the gate vectors it never
//! compares like with like (bug 204).
//!
//! The Built-in (TF-IDF) profile embeds the same text without its anchor line:
//! see [`lexical_embedding_text_for`].
//!
//! Pure and I/O-free. The zoned anchor line lives HERE, not in `episodic.rs`
//! (work order LR-12): at the pin `buildMemoryAnchorLine`'s only caller is
//! `embeddingTextFor`. The memory-calendar zone is a REQUIRED argument —
//! v4's `timeZone ?? getMemoryTimeZone()` fallback is never taken (every v4
//! caller resolves the zone with `refreshMemoryTimeZone()` first), and v5's
//! core never reads an ambient zone.
//!
//! `EmbeddingTextSource` is this module's own BORROWED view rather than
//! `episodic::EpisodicAnchorView`: that struct owns its strings and types
//! `entities` as `Vec<String>`, so it can neither borrow a row's fields nor
//! carry v4's runtime `typeof e === 'string'` arm (a JSON entities array
//! holding a number or `null` renders that element as `''`, which the
//! empties filter then drops — see [`AnchorEntities`]).

use std::sync::OnceLock;

use regex::Regex;
use serde_json::Value;

use crate::jsstr::{js_trim, utf16_truncate};
use crate::memory_time::calendar::zoned_date_stamp;

/// The version of [`lexical_embedding_text_for`]'s shape, stamped on each
/// TF-IDF vocabulary row (`tfidf_vocabularies.textShapeVersion`) by the refit.
/// Bump it whenever the text the BUILTIN path embeds changes, and the refit
/// policy re-fits and re-embeds once. 0 is the anchored shape from before
/// version 1. (v4's `TfidfVocabularySchema` comment calls it
/// `BUILTIN_TEXT_SHAPE_VERSION`; no such symbol exists — this is the name.)
pub const LEXICAL_TEXT_SHAPE_VERSION: i64 = 1;

/// Cap on entities rendered into the anchor line so it stays one line.
const ANCHOR_MAX_ENTITIES: usize = 4;

/// The labels an anchor line's parts open with, in the order they are written.
const ANCHOR_LABELS: [&str; 3] = ["when", "story time", "place"];

/// A memory's `entities`, borrowed in whichever shape the caller holds them.
///
/// v4 types the field `readonly string[] | null` but reads it defensively —
/// `(view.entities ?? []).map(e => typeof e === 'string' ? e.trim() : '')` —
/// so a non-string element (a raw JSON row) becomes `''` and is dropped by the
/// empties filter BEFORE the four-entity cap. [`AnchorEntities::Json`] carries
/// that arm; [`AnchorEntities::Strings`] is the typed rows' shape.
#[derive(Debug, Clone, Copy, Default)]
pub enum AnchorEntities<'a> {
    /// `entities` absent or `null` (v4's `?? []`).
    #[default]
    None,
    /// A typed `string[]`.
    Strings(&'a [String]),
    /// A raw JSON array; a non-string element renders as `''`.
    Json(&'a [Value]),
}

impl<'a> From<&'a [String]> for AnchorEntities<'a> {
    fn from(v: &'a [String]) -> Self {
        AnchorEntities::Strings(v)
    }
}

impl<'a> From<&'a Vec<String>> for AnchorEntities<'a> {
    fn from(v: &'a Vec<String>) -> Self {
        AnchorEntities::Strings(v.as_slice())
    }
}

impl<'a> From<Option<&'a Vec<String>>> for AnchorEntities<'a> {
    fn from(v: Option<&'a Vec<String>>) -> Self {
        v.map_or(AnchorEntities::None, |v| {
            AnchorEntities::Strings(v.as_slice())
        })
    }
}

impl<'a> From<&'a [Value]> for AnchorEntities<'a> {
    fn from(v: &'a [Value]) -> Self {
        AnchorEntities::Json(v)
    }
}

impl<'a> AnchorEntities<'a> {
    /// v4's `.map(e => typeof e === 'string' ? e.trim() : '')
    /// .filter(e => e.length > 0).slice(0, ANCHOR_MAX_ENTITIES)`.
    fn rendered(&self) -> Vec<&'a str> {
        let trimmed: Box<dyn Iterator<Item = &'a str>> = match *self {
            AnchorEntities::None => Box::new(std::iter::empty()),
            AnchorEntities::Strings(v) => Box::new(v.iter().map(|e| js_trim(e))),
            AnchorEntities::Json(v) => Box::new(v.iter().map(|e| match e {
                Value::String(s) => js_trim(s),
                _ => "",
            })),
        };
        trimmed
            .filter(|e| !e.is_empty())
            .take(ANCHOR_MAX_ENTITIES)
            .collect()
    }
}

/// The fields a memory's embedded text is built from (v4
/// `EmbeddingTextSource extends EpisodicAnchorView`). Anchors are optional;
/// `None` covers both v4's `undefined` and `null`.
#[derive(Debug, Clone, Copy, Default)]
pub struct EmbeddingTextSource<'a> {
    pub summary: &'a str,
    pub content: &'a str,
    pub occurred_at: Option<&'a str>,
    pub narrative_time: Option<&'a str>,
    pub entities: AnchorEntities<'a>,
}

/// v4's `/^\d{4}-\d{2}-\d{2}$/` — no `u` flag, so `\d` is ASCII `[0-9]`; no
/// `m` flag, so `$` is the end of input only.
fn date_only_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}$").unwrap())
}

/// Render the anchor line appended to a memory's embedded text, e.g.
/// `(when: 2026-07-14 · place: Lighthouse Point)` — v4
/// `buildMemoryAnchorLine(view, timeZone)`. Returns `""` when the memory
/// carries no anchors, so callers can append unconditionally. The date is
/// rendered as the local calendar date on the memory calendar `tz`
/// (time-of-day adds noise, not signal, to the embedding); `narrativeTime`
/// rides along verbatim (trimmed) when present. `summary` / `content` are not
/// read.
pub fn memory_anchor_line(view: &EmbeddingTextSource<'_>, tz: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(occurred_at) = view.occurred_at.map(js_trim) {
        if !occurred_at.is_empty() {
            // The calendar date on the instance's memory calendar (bug 212): a
            // local midnight stored as UTC east of Greenwich would otherwise
            // read a day early. Legacy UTC-midnight point dates were re-stamped
            // at local midnight by the rezone-legacy-memory-occurred-at-v1
            // migration, so they read their own day.
            //
            // `Date.parse` reads a zone-less datetime as UTC here where v4 reads
            // it in the process zone (the ruled `ZONELESS_PARSE_AS_UTC`, LR-10).
            let date_only = match crate::episodic::js_date_parse_ms(occurred_at) {
                Some(parsed) => zoned_date_stamp(parsed as f64, tz),
                // `occurredAt.slice(0, 10)` — UTF-16 code units. A cut through
                // a surrogate pair decodes lossily, which the ASCII date test
                // refuses exactly as it refuses JS's lone surrogate.
                None => utf16_truncate(occurred_at, 10),
            };
            // The zoned stamp can fail this too: v4 renders the year unpadded
            // (`0050-06-01` → `50-06-01`, year 10000 → five digits), and those
            // lose their `when` part.
            if date_only_re().is_match(&date_only) {
                parts.push(format!("{}: {date_only}", ANCHOR_LABELS[0]));
            }
        }
    }
    if let Some(narrative_time) = view.narrative_time.map(js_trim) {
        if !narrative_time.is_empty() {
            parts.push(format!("{}: {narrative_time}", ANCHOR_LABELS[1]));
        }
    }
    let entities = view.entities.rendered();
    if !entities.is_empty() {
        parts.push(format!("{}: {}", ANCHOR_LABELS[2], entities.join(", ")));
    }
    if parts.is_empty() {
        return String::new();
    }
    format!("({})", parts.join(" · "))
}

/// Build the canonical embedded text for a memory — v4
/// `embeddingTextFor(memory, timeZone)`: the lexical text, plus `\n` and the
/// anchor line when the line is non-empty.
///
/// `tz` is the memory-calendar zone the anchor date renders in; callers
/// resolve it with the memory-calendar refresh first, so a cold job embeds
/// the same date any other writer would.
///
/// Legacy rows embedded before anchors existed keep their anchorless vectors
/// until they are next re-embedded; mixed rows are an accepted state.
pub fn embedding_text_for(row: &EmbeddingTextSource<'_>, tz: &str) -> String {
    let base = lexical_embedding_text_for(row);
    let anchor_line = memory_anchor_line(row, tz);
    if anchor_line.is_empty() {
        base
    } else {
        format!("{base}\n{anchor_line}")
    }
}

/// The text a lexical (TF-IDF) embedder sees for a memory: `summary\n\ncontent`,
/// the canonical text without its anchor line — v4 `lexicalEmbeddingTextFor`.
/// Callers hand it to the embedding seam as `lexical_text` beside
/// [`embedding_text_for`]; only the BUILTIN path uses it, so neural vectors are
/// unchanged.
///
/// In a bag-of-words vector the anchor line's rare tokens — dates, place
/// names, and the bigrams joining them to the content — carry a large share of
/// a short memory's weight, so two tellings of one fact on different days
/// score far apart and unrelated facts sharing a place score close. A neural
/// embedder reads the line as context; a lexical one cannot. Admission's date
/// guard reads `occurredAt` directly and recall finds entity names by text, so
/// nothing lexical needs the line. Built from the fields, never by trimming
/// the canonical text, so content that happens to end in "(place: …)" keeps
/// its last line.
pub fn lexical_embedding_text_for(row: &EmbeddingTextSource<'_>) -> String {
    format!("{}\n\n{}", row.summary, row.content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn memory<'a>(entities: &'a [String]) -> EmbeddingTextSource<'a> {
        EmbeddingTextSource {
            summary: "Lorian fears deep water",
            content: "Lorian nearly drowned as a boy in the millpond.",
            occurred_at: Some("2026-07-14T12:00:00.000Z"),
            narrative_time: Some("second evening"),
            entities: entities.into(),
        }
    }

    #[test]
    fn lexical_is_the_canonical_text_without_its_anchor_line() {
        let ents = vec!["Lighthouse Point".to_string(), "Bram".to_string()];
        let m = memory(&ents);
        let text = embedding_text_for(&m, "UTC");
        assert_eq!(
            text,
            "Lorian fears deep water\n\nLorian nearly drowned as a boy in the millpond.\n\
             (when: 2026-07-14 · story time: second evening · place: Lighthouse Point, Bram)"
        );
        assert!(text.starts_with(&lexical_embedding_text_for(&m)));
    }

    #[test]
    fn east_of_utc_local_midnight_reads_its_own_day() {
        let m = EmbeddingTextSource {
            occurred_at: Some("2026-07-13T15:00:00.000Z"),
            ..Default::default()
        };
        assert_eq!(memory_anchor_line(&m, "Asia/Tokyo"), "(when: 2026-07-14)");
        assert_eq!(memory_anchor_line(&m, "UTC"), "(when: 2026-07-13)");
    }

    #[test]
    fn non_string_and_blank_entities_drop_before_the_cap() {
        let ents = [
            json!(42),
            json!(" A "),
            json!(null),
            json!(""),
            json!("B"),
            json!("C"),
            json!("D"),
            json!("E"),
        ];
        let m = EmbeddingTextSource {
            entities: AnchorEntities::Json(&ents),
            ..Default::default()
        };
        assert_eq!(memory_anchor_line(&m, "UTC"), "(place: A, B, C, D)");
    }

    #[test]
    fn no_anchor_no_line() {
        let m = EmbeddingTextSource {
            summary: "Garden",
            content: "We sat outside.\n(place: the garden)",
            occurred_at: Some("  "),
            ..Default::default()
        };
        assert_eq!(
            embedding_text_for(&m, "UTC"),
            "Garden\n\nWe sat outside.\n(place: the garden)"
        );
        assert_eq!(LEXICAL_TEXT_SHAPE_VERSION, 1);
    }
}
