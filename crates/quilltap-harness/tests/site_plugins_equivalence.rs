//! Differential (P4.59, tier-1): the site-plugins gate
//! (`quilltap_core::provider_manifest::search::is_site_plugin_enabled`) vs v4's
//! REAL `lib/plugins/site-plugins.ts` `isSitePluginEnabled`.
//!
//! This is the predicate v4's manifest loader applies BEFORE `enabledByDefault`
//! is read, so it decides whether the bundled Serper search plugin registers at
//! all — and in v5 registration is the single source of truth feeding BOTH the
//! `/api/v1/providers` search row and the `search_web` runner. The corpus walks
//! unset / empty / whitespace-only values, the literal `all` in three casings,
//! comma lists with stray spaces and empty segments, and the disabled-wins
//! overlap.
//!
//! Regenerate the oracle + run:
//!   set -euo pipefail
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   npx tsx $V5W/harness/oracle/cases/site-plugins.ts > /tmp/oracle-site-plugins.ndjson
//!   cd $V5W
//!   QT_ORACLE_SITE_PLUGINS=/tmp/oracle-site-plugins.ndjson \
//!     cargo test -p quilltap-harness --test site_plugins_equivalence

use quilltap_core::provider_manifest::search::is_site_plugin_enabled;
use serde_json::Value;

#[test]
fn site_plugins_gate_matches_v4() {
    let Ok(path) = std::env::var("QT_ORACLE_SITE_PLUGINS") else {
        eprintln!("SKIP: set QT_ORACLE_SITE_PLUGINS (see test header).");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read site-plugins oracle");
    let mut cases = 0usize;
    let (mut trues, mut falses) = (0usize, 0usize);

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("oracle line parses");
        let label = row["label"].as_str().expect("label");
        let plugin = row["plugin"].as_str().expect("plugin");
        // `null` in the oracle row means the env var was UNSET (as against set
        // to the empty string, which v4 treats identically but which a port
        // that read `env::var().unwrap_or_default()` would collapse silently).
        let enabled = row["enabled"].as_str();
        let disabled = row["disabled"].as_str();
        let want = row["result"].as_bool().expect("result");

        let got = is_site_plugin_enabled(plugin, enabled, disabled);
        assert_eq!(got, want, "site-plugins gate mismatch for {label}");

        cases += 1;
        if want {
            trues += 1;
        } else {
            falses += 1;
        }
    }

    // Shape, not a hand count: a corpus that drifted to all-true (or all-false)
    // would otherwise pass green while proving only one arm of the predicate.
    assert!(cases >= 19, "expected the site-plugins corpus, got {cases}");
    assert!(
        trues >= 5 && falses >= 5,
        "expected both verdicts in the corpus ({trues} true / {falses} false)"
    );
    eprintln!("OK: site-plugins gate matched v4 over {cases} cases ({trues} enabled / {falses} disabled).");
}
