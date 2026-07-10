//! P4.6d providers-listing differential (tier-1): `settings::provider_list` vs v4's
//! REAL provider plugins (the `GET /api/v1/providers` LLM rows).
//!
//! The Rust listing answers from the W4.7a manifests; the oracle emits the row v4's
//! route builds from each plugin's `metadata`/`capabilities`/`config`. The diff is
//! field-by-field over the manifest-covered fields, normalizing away the two fields
//! the manifest deliberately lacks (`icon`, `optionsSchema`) — both documented
//! absences (v4 exposes plugin icons + a per-provider options schema the v5 manifest
//! does not carry). Search providers are also absent (no search-provider manifest).
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   cd ~/source/quilltap-server
//!   npx tsx <worktree>/harness/oracle/cases/providers-listing.ts > /tmp/oracle-providers-listing.ndjson
//! Run:
//!   QT_ORACLE_PROVIDERS_LISTING=/tmp/oracle-providers-listing.ndjson \
//!     cargo test -p quilltap-harness --test providers_listing_equivalence

use quilltap_core::api::settings;
use quilltap_core::api::types::Response;
use serde_json::Value;

fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("SKIP: set {key} (see test header).");
            None
        }
    }
}

/// Strip the fields the manifest deliberately lacks so the diff is over the
/// manifest-covered surface only.
fn normalize(mut p: Value) -> Value {
    if let Some(obj) = p.as_object_mut() {
        obj.remove("icon");
        obj.remove("optionsSchema");
    }
    p
}

#[test]
fn providers_listing_matches_v4() {
    let Some(path) = env_or_skip("QT_ORACLE_PROVIDERS_LISTING") else {
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read oracle ndjson");
    let oracle: Value = serde_json::from_str(text.trim()).expect("parse oracle");

    let got = match settings::provider_list() {
        Response::Providers(v) => v,
        other => panic!("unexpected response: {other:?}"),
    };

    let oracle_providers = oracle["providers"].as_array().expect("oracle providers");
    let got_providers = got["providers"].as_array().expect("got providers");

    assert_eq!(
        oracle["count"], got["count"],
        "provider count mismatch (oracle {} vs got {})",
        oracle["count"], got["count"]
    );
    assert_eq!(
        oracle_providers.len(),
        got_providers.len(),
        "provider list length mismatch"
    );

    // Registration order matches (the manifest set is in v4's registration order).
    for (o, g) in oracle_providers.iter().zip(got_providers.iter()) {
        let g_norm = normalize(g.clone());
        assert_eq!(o, &g_norm, "provider mismatch for id {:?}", o.get("id"));
    }
}
