//! Tier-1 differential: the image-transport predicate pair (P4.D106; v4
//! `a14a1811` bug 91, `lib/llm/image-transport.ts` +
//! `lib/llm/attachment-support.ts` `staticProviderCanTransportImages`), ported
//! as `quilltap_core::files::image_transport::provider_can_transport_images` +
//! `quilltap_core::files::attachment_support::static_provider_can_transport_images`.
//!
//! The oracle drives v4's REAL code in three sections (see the case header):
//! `static` (the client-safe map), `full_uninit` (the predicate with the plugin
//! registry NOT initialized — v4's startup/tests/job-child fallback arm), and
//! `full_init` (the registry initialized with all TEN real dist plugins — the
//! production truth). **The registry-vs-static collapse in v5, measured:** v5's
//! manifests are baked, so `provider_can_transport_images` ALWAYS answers the
//! registry tier for a manifest-known provider and the static tier only for a
//! name the manifest set lacks — there is no uninitialized state. The mapping is
//! therefore:
//!   * `static` rows      → `static_provider_can_transport_images`;
//!   * `full_uninit` rows → `static_provider_can_transport_images` (v4's
//!     uninitialized fallback IS the static fn; recorded to pin that v4 arm);
//!   * `full_init` rows   → `provider_can_transport_images` (v5's only mode).
//!
//! ⚠ ONE CROSS-LANE PENDING ROW: v4's NanoGPT plugin 1.1.0 declares image
//! transport, so the `full_init` NANOGPT rows are `true` — but the v5 manifest
//! regeneration from plugin 1.1.0 is P4.D107's deliverable. Until it lands,
//! `P4D107_NANOGPT_MANIFEST_LANDED = false` asserts v5 == false for those rows
//! (the pre-D107 manifest). AT UNIFICATION: flip the constant to true — the
//! rows become plain equalities. If D107's manifest lands and the constant is
//! left false, this test goes RED by design (the tripwire self-arms).
//!
//! Regenerate the oracle (from the v4 checkout):
//!   cd ~/source/quilltap-server
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   npx tsx $V5W/harness/oracle/cases/image-transport.ts > /tmp/oracle-image-transport.ndjson
//! Run:
//!   QT_ORACLE_IMAGE_TRANSPORT=/tmp/oracle-image-transport.ndjson cargo test -p quilltap-harness --test image_transport_equivalence -- --nocapture

use quilltap_core::files::attachment_support::static_provider_can_transport_images;
use quilltap_core::files::image_transport::provider_can_transport_images;
use serde::Deserialize;

/// Flip to `true` at the P4.D107 unification (the regenerated NanoGPT manifest
/// carries `supportsAttachments: true` + the four image MIME types).
/// FLIPPED at the a14a1811-round unification (2026-08-23): D107's manifest is
/// on the unify branch, so the full_init NANOGPT rows are plain equalities.
const P4D107_NANOGPT_MANIFEST_LANDED: bool = true;

#[derive(Deserialize)]
struct Row {
    kind: String,
    provider: String,
    result: bool,
}

#[test]
fn image_transport_predicates_match_v4() {
    let Ok(path) = std::env::var("QT_ORACLE_IMAGE_TRANSPORT") else {
        eprintln!("QT_ORACLE_IMAGE_TRANSPORT not set; skipping");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read oracle NDJSON");
    let (mut n_static, mut n_uninit, mut n_init) = (0usize, 0usize, 0usize);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).expect("parse oracle row");
        match row.kind.as_str() {
            "static" => {
                n_static += 1;
                assert_eq!(
                    static_provider_can_transport_images(&row.provider),
                    row.result,
                    "static mismatch for {:?}",
                    row.provider
                );
            }
            "full_uninit" => {
                // v4's uninitialized-registry arm falls back to the static map;
                // v5 has no uninitialized state, so its twin is the static fn.
                n_uninit += 1;
                assert_eq!(
                    static_provider_can_transport_images(&row.provider),
                    row.result,
                    "full_uninit mismatch for {:?}",
                    row.provider
                );
            }
            "full_init" => {
                n_init += 1;
                let got = provider_can_transport_images(&row.provider);
                let is_nanogpt = row.provider.to_uppercase() == "NANOGPT";
                if is_nanogpt && !P4D107_NANOGPT_MANIFEST_LANDED {
                    // The §C1 pending: v4 plugin 1.1.0 answers true; v5's
                    // manifest regen is P4.D107's. Assert BOTH sides' current
                    // truth so the row cannot rot silently in either direction.
                    assert!(row.result, "v4 NANOGPT registry row expected true");
                    assert!(
                        !got,
                        "v5 NANOGPT manifest answered true — P4.D107 landed; \
                         flip P4D107_NANOGPT_MANIFEST_LANDED"
                    );
                } else {
                    assert_eq!(got, row.result, "full_init mismatch for {:?}", row.provider);
                }
            }
            other => panic!("unknown row kind {other}"),
        }
    }
    // Corpus-shape floor: all three sections present, each covering the ten
    // built-ins + case variants + unknowns.
    assert!(n_static >= 15, "static section short: {n_static}");
    assert!(n_uninit >= 15, "full_uninit section short: {n_uninit}");
    assert!(n_init >= 15, "full_init section short: {n_init}");
    println!("image_transport_equivalence: {n_static}+{n_uninit}+{n_init} rows OK");
}
