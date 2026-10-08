//! Tier-1 differential (P4.D262 item 16): the wardrobe tools' picture decision
//! — v4 `lib/wardrobe/tool-image-generation.ts` (`b3f937076`) — and the pure
//! tool formatters the wear ledger added (`3ee3b1342`), against v4's REAL
//! functions.
//!
//! - `wardrobeToolImagesEnabled` over eight settings shapes →
//!   `services::tool_image_generation::wardrobe_tool_images_enabled`;
//! - `maybeQueueWardrobeToolImage`'s decision table (enabled × requested ×
//!   defaultWhenEnabled, no usable profile) → `wanted` + the result sentences
//!   (`undefined` / `not-enabled` / `no-image-profile` — the enqueue arm is
//!   the wardrobe_tools family's switch-ON scenario);
//! - `formatWardrobeToolImageLine` / `formatWardrobeImageHandle`;
//! - `formatRelativeDays` (`format_time::format_relative_days`) at the rungs;
//! - `formatWardrobeListWearNote` (`tools::wardrobe_list::
//!   format_wardrobe_list_wear_note`) and `formatWardrobeWearParagraph`
//!   (`tools::wardrobe_read::format_wardrobe_wear_paragraph`) under ONE pinned
//!   `nowMs`, incl. v4's own `wardrobe-wear-readout.test.ts` lines.
//!
//! `patchChangesLook` is module-private in v4 (`wardrobe-update-handler.ts`),
//! so it cannot be imported here; its arms are pinned through v4's real
//! `executeWardrobeUpdateTool` in `wardrobe_tools_equivalence`'s switch-ON
//! scenario.
//!
//! Generate (Node 24, from the v4 checkout or the round's pin):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   TZ=UTC $N/node --import tsx $V5/harness/oracle/cases/tool-image-generation.ts \
//!     > /tmp/oracle-tool-image-generation.ndjson
//! Run:
//!   QT_ORACLE_TOOL_IMAGE_GENERATION=/tmp/oracle-tool-image-generation.ndjson \
//!     cargo test -p quilltap-harness --test tool_image_generation_equivalence -- --nocapture

use quilltap_core::format_time::format_relative_days;
use quilltap_core::services::tool_image_generation::{
    format_wardrobe_image_handle, format_wardrobe_tool_image_line, wanted,
    wardrobe_tool_images_enabled, WardrobeToolImageResult, WardrobeToolImageStatus,
};
use quilltap_core::tools::wardrobe_list::format_wardrobe_list_wear_note;
use quilltap_core::tools::wardrobe_read::{format_wardrobe_wear_paragraph, WardrobeReadWearResult};
use serde_json::{json, Value};

const BASELINE: &str = "p4.d262-tool-image-generation";
const DAY: f64 = 86_400_000.0;

fn status_of(s: &str) -> WardrobeToolImageStatus {
    match s {
        "queued" => WardrobeToolImageStatus::Queued,
        "not-enabled" => WardrobeToolImageStatus::NotEnabled,
        "no-image-profile" => WardrobeToolImageStatus::NoImageProfile,
        "failed" => WardrobeToolImageStatus::Failed,
        other => panic!("unknown status {other}"),
    }
}

#[test]
fn tool_image_generation_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_TOOL_IMAGE_GENERATION") else {
        eprintln!("SKIP: set QT_ORACLE_TOOL_IMAGE_GENERATION to the oracle NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let rows: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let meta = &rows[0];
    assert_eq!(
        meta["baseline"],
        json!(BASELINE),
        "a stale oracle — regenerate it"
    );
    let now = meta["nowMs"].as_f64().unwrap();

    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    let mut failed = Vec::new();
    for row in &rows[1..] {
        let kind = row["kind"].as_str().unwrap().to_string();
        *counts.entry(kind.clone()).or_default() += 1;
        let want = &row["out"];
        let got: Value = match kind.as_str() {
            "enabled" => {
                let settings = &row["settings"];
                json!(wardrobe_tool_images_enabled(
                    (!settings.is_null()).then_some(settings)
                ))
            }
            "decision" => {
                let enabled = row["enabled"].as_bool().unwrap();
                let requested = row["requested"].as_bool();
                let default = row["defaultWhenEnabled"].as_bool().unwrap();
                // No usable profile is configured in the oracle's stub repos.
                let out = if !wanted(enabled, requested, default) {
                    None
                } else if !enabled {
                    Some(WardrobeToolImageResult::of(
                        WardrobeToolImageStatus::NotEnabled,
                    ))
                } else {
                    Some(WardrobeToolImageResult::of(
                        WardrobeToolImageStatus::NoImageProfile,
                    ))
                };
                serde_json::to_value(out).unwrap()
            }
            "line" => {
                let result = row["status"].as_str().map(|s| WardrobeToolImageResult {
                    status: status_of(s),
                    message: row["message"].as_str().unwrap().to_string(),
                });
                json!(format_wardrobe_tool_image_line(result.as_ref()))
            }
            "handle" => json!(format_wardrobe_image_handle(row["id"].as_str().unwrap())),
            "relative" => {
                let days = row["days"].as_f64().unwrap();
                json!(format_relative_days(now - days * DAY, now))
            }
            "list_note" => json!(format_wardrobe_list_wear_note(
                row["wear_count"].as_i64().unwrap(),
                row["last_worn_at"].as_str(),
                now
            )),
            "paragraph" => {
                let wear: Option<WardrobeReadWearResult> =
                    serde_json::from_value(row["wear"].clone()).unwrap();
                json!(format_wardrobe_wear_paragraph(wear.as_ref(), now))
            }
            other => panic!("unknown row kind {other}"),
        };
        if &got != want {
            eprintln!("[{kind}] MISMATCH {row}\n   v5: {got}");
            failed.push(row.to_string());
        }
    }
    assert!(failed.is_empty(), "{} rows differ", failed.len());
    for (kind, min) in [
        ("enabled", 8),
        ("decision", 12),
        ("line", 5),
        ("handle", 1),
        ("relative", 21),
        ("list_note", 12),
        ("paragraph", 11),
    ] {
        assert!(
            counts.get(kind).copied().unwrap_or(0) >= min,
            "{kind}: {counts:?}"
        );
    }
    eprintln!("OK: tool-image-generation matched oracle ({counts:?}).");
}
