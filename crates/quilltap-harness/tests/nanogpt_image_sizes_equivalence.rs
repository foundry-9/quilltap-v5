//! Tier-1 differential (P4.D356 item 10): v4's `fitImageShape`
//! (`plugins/dist/qtap-plugin-nanogpt/image-sizes.ts`, `4b0d1cb04` — bug 269)
//! ↔ `quilltap_core::model::nanogpt_image_sizes::fit_image_shape`, EXACT on
//! every row: v4's ten vectors, a catalog × size × aspect-ratio grid, the JS
//! `\s` / `\d` / non-unicode `/i` edges (U+FEFF and U+00A0 are whitespace,
//! U+0085 is not, a KELVIN SIGN is not a `k`), digit runs past `Number`'s
//! range, the tie-breaks, and the `logdist-*` rows where a linear ratio
//! distance would pick differently (the `ratioDistance` mutation's witness).
//!
//! Generate the oracle output, then run:
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/npx tsx "$V5W/harness/oracle/cases/nanogpt-image-sizes.ts" > /tmp/oracle-nanogpt-image-sizes.ndjson
//!   cd "$V5W"
//!   QT_ORACLE_NANOGPT_IMAGE_SIZES=/tmp/oracle-nanogpt-image-sizes.ndjson \
//!     cargo test -p quilltap-harness --test nanogpt_image_sizes_equivalence -- --nocapture

use quilltap_core::model::nanogpt_image_sizes::{fit_image_shape, NanoGptSizeCatalog};
use serde_json::Value;

#[test]
fn fit_image_shape_matches_v4() {
    let path = match std::env::var("QT_ORACLE_NANOGPT_IMAGE_SIZES") {
        Ok(p) => p,
        Err(_) => {
            println!(
                "SKIP: set QT_ORACLE_NANOGPT_IMAGE_SIZES to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).expect("read oracle");
    let mut rows = 0usize;
    let mut vectors = 0usize;
    let mut failures = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("oracle row");
        let id = row["id"].as_str().expect("id").to_string();
        let catalog: Option<NanoGptSizeCatalog> = row
            .get("catalog")
            .map(|c| serde_json::from_value(c.clone()).expect("catalog"));
        let size = row.get("size").and_then(Value::as_str);
        let ratio = row.get("aspectRatio").and_then(Value::as_str);
        let got = serde_json::to_string(&fit_image_shape(catalog.as_ref(), size, ratio)).unwrap();
        let want = serde_json::to_string(&row["result"]).unwrap();
        if got != want {
            failures.push(format!("{id}: v4 {want} / v5 {got}"));
        }
        rows += 1;
        let tail = id.strip_prefix('v').unwrap_or("");
        if !tail.is_empty()
            && tail
                .trim_end_matches(['a', 'b'])
                .bytes()
                .all(|b| b.is_ascii_digit())
        {
            vectors += 1;
        }
    }
    println!("nanogpt_image_sizes: {rows} rows ({vectors} v4 vectors)");
    assert!(
        failures.is_empty(),
        "{} of {rows} rows differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
    // v4's ten vectors (v10 as two rows) + the 12 × 15 × 8 grid + the edges.
    assert_eq!(vectors, 11, "the v4 vectors must all be present");
    assert!(rows >= 11 + 12 * 15 * 8 + 40, "a short corpus: {rows} rows");
}
