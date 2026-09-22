//! Tier-1 differential: the RIFF chunk walk behind `is_lossless_webp`
//! (P4.D209 / v4 bug 159, `186eb09cb`).
//!
//! A lossless WebP — a `VP8L` chunk — is worth re-encoding to lossy at quality
//! 85 once it passes the 512 KiB floor; a lossy one (`VP8 `) never is, because
//! lossy→lossy is generation loss for a modest saving. Everything downstream of
//! that verdict is a write: a wrong `true` discards a deliberate lossless
//! encoding, and a wrong `false` leaves v4's measured 112 MB of oversized
//! lossless plates in the store.
//!
//! The corpus is hand-built so each arm of the walk has a case of its own: the
//! two fourcc verdicts, the extended container that hides the verdict behind
//! `VP8X`/`ICCP`/`ALPH`/`ANIM`, RIFF's odd-size pad byte (with a twin that
//! OMITS the pad, so a walk that forgets it lands mid-fourcc and must terminate
//! false rather than find `VP8L` by luck), the zero-length chunk, the
//! over-length guard, and every shape that is not a WebP at all. The same hex
//! buffers reach v4's real function and the Rust port; the verdicts are
//! compared exactly.
//!
//! `LOSSLESS_WEBP_REENCODE_MIN_BYTES` rides along so the floor cannot drift on
//! one side unseen.
//!
//! Generate the oracle output (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/blob-transcode-lossless-webp.ts \
//!     > /tmp/oracle-lossless-webp.ndjson
//! Run:
//!   QT_ORACLE_LOSSLESS_WEBP=/tmp/oracle-lossless-webp.ndjson \
//!     cargo test -p quilltap-harness --test blob_transcode_lossless_webp_equivalence -- --nocapture

use std::path::{Path, PathBuf};

use quilltap_core::services::mount_index::blob_transcode::{
    is_lossless_webp, LOSSLESS_WEBP_REENCODE_MIN_BYTES,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Spec {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    #[serde(rename = "dataHex")]
    data_hex: String,
    why: String,
}

#[derive(Deserialize)]
struct Oracle {
    #[serde(rename = "losslessWebpReencodeMinBytes")]
    lossless_webp_reencode_min_bytes: usize,
    results: Vec<OracleResult>,
}

#[derive(Deserialize)]
struct OracleResult {
    name: String,
    lossless: bool,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/blob-transcode-lossless-webp.json")
}

fn from_hex(h: &str) -> Vec<u8> {
    (0..h.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&h[i..i + 2], 16).expect("hex byte"))
        .collect()
}

#[test]
fn is_lossless_webp_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_LOSSLESS_WEBP") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_LOSSLESS_WEBP to the oracle NDJSON (see header).");
            return;
        }
    };
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read corpus: {e}")),
    )
    .expect("parse corpus");
    let oracle: Oracle = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle");

    assert_eq!(
        oracle.lossless_webp_reencode_min_bytes, LOSSLESS_WEBP_REENCODE_MIN_BYTES,
        "the re-encode floor drifted: v4 {} vs v5 {}",
        oracle.lossless_webp_reencode_min_bytes, LOSSLESS_WEBP_REENCODE_MIN_BYTES
    );
    assert_eq!(
        oracle.results.len(),
        spec.cases.len(),
        "oracle covers {} cases, the corpus has {} — regenerate",
        oracle.results.len(),
        spec.cases.len()
    );

    let mut mismatches: Vec<String> = Vec::new();
    let mut true_count = 0usize;
    for (case, theirs) in spec.cases.iter().zip(oracle.results.iter()) {
        assert_eq!(
            case.name, theirs.name,
            "corpus and oracle disagree on case ORDER — regenerate"
        );
        let ours = is_lossless_webp(&from_hex(&case.data_hex));
        if theirs.lossless {
            true_count += 1;
        }
        if ours != theirs.lossless {
            mismatches.push(format!(
                "  {}: v4 {} / v5 {} — {}",
                case.name, theirs.lossless, ours, case.why
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "is_lossless_webp diverged on {} of {} cases:\n{}",
        mismatches.len(),
        spec.cases.len(),
        mismatches.join("\n")
    );

    // A corpus that answered `false` everywhere would pass against a `fn(_) ->
    // false` port; the split is asserted so the instrument cannot go blind.
    assert!(
        true_count >= 4 && true_count < spec.cases.len(),
        "the corpus must carry both verdicts in quantity: {true_count} true of {}",
        spec.cases.len()
    );

    println!(
        "OK: is_lossless_webp matched v4 on {} cases ({true_count} lossless, {} not); \
         floor {LOSSLESS_WEBP_REENCODE_MIN_BYTES} bytes.",
        spec.cases.len(),
        spec.cases.len() - true_count
    );
}
