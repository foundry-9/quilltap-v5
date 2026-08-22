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

/// Strip the ONE field the manifest deliberately lacks (`icon` — v4 exposes
/// plugin icons the manifest does not carry) so the diff is over the
/// manifest-covered surface only.
///
/// **`configRequirements` is the plugin's WHOLE `config` object since P4.D93**
/// (v4 bug 81's `acceptsApiKey`): v4's route spreads it verbatim, and the oracle
/// used to hand-pick the six fields the manifest models — a comparand blind to
/// any config key v4 adds. A key the v5 manifest does not carry is now a RED
/// diff, which is the tripwire this family is for.
///
/// **`optionsSchema` is NO LONGER normalized away (P4.D83, shared contract B).**
/// It was a documented absence; the manifests now carry it, extracted by the
/// generator from each plugin's `getProviderOptionsSchema()`, so all eight
/// declaring plugins' schemas are diffed byte-for-byte here — which is also
/// P4.D84's server-side proof that the panel renders what v4 renders.
fn normalize(mut p: Value) -> Value {
    if let Some(obj) = p.as_object_mut() {
        obj.remove("icon");
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
    let mut with_schema = 0usize;
    let mut without_schema = 0usize;
    let mut with_rule = 0usize;
    for (o, g) in oracle_providers.iter().zip(got_providers.iter()) {
        let g_norm = normalize(g.clone());
        assert_eq!(o, &g_norm, "provider mismatch for id {:?}", o.get("id"));

        // P4.D97 (v4 bug 85): `thinkingTurnRule` byte-for-byte, key ORDER
        // included — the profile editor evaluates the rule it receives, and
        // the wire's key order comes from the typed struct's field order, so
        // pin it the same way `optionsSchema` is pinned below.
        let (o_rule, g_rule) = (&o["thinkingTurnRule"], &g["thinkingTurnRule"]);
        assert_eq!(
            serde_json::to_string(o_rule).unwrap(),
            serde_json::to_string(g_rule).unwrap(),
            "thinkingTurnRule bytes (key ORDER included) differ for id {:?}",
            o.get("id")
        );
        if !o_rule.is_null() {
            with_rule += 1;
        }

        // P4.D83, shared contract B: `optionsSchema` byte-for-byte, ORDER
        // included. `Value` equality above is order-INDEPENDENT (preserve_order
        // gives an IndexMap, whose PartialEq ignores position), and the renderer
        // draws the fields in the order it receives them — so re-serialize both
        // sides and compare the strings.
        let (o_schema, g_schema) = (&o["optionsSchema"], &g["optionsSchema"]);
        assert_eq!(
            serde_json::to_string(o_schema).unwrap(),
            serde_json::to_string(g_schema).unwrap(),
            "optionsSchema bytes (field ORDER included) differ for id {:?}",
            o.get("id")
        );
        if o_schema.is_null() {
            without_schema += 1;
        } else {
            with_schema += 1;
        }
    }

    // Shape, not a hand count: at the `93ed8abf` pin EIGHT of the nine built-ins
    // declare a schema and exactly one (google) does not. An oracle that lost the
    // getter — or a generator that silently emitted `null` — would otherwise pass
    // green with the whole field untested.
    assert_eq!(
        without_schema, 1,
        "expected exactly one provider with NO options schema (google)"
    );
    assert!(
        with_schema >= 9,
        "expected every other built-in to declare an options schema, got {with_schema}"
    );
    // Shape: at the `4cb1035e` pin exactly THREE built-ins declare a
    // thinking-turn rule (deepseek + ollama from v4 bug 85; nanogpt from
    // `d5830439`). An oracle regenerated from a pre-bug-85 tree — or a
    // generator that dropped the field — would otherwise pass green with the
    // whole field untested. Moved 2 → 3 by P4.D101: a designed tripwire firing
    // as designed, not a weakened assertion.
    assert_eq!(
        with_rule, 3,
        "expected exactly three providers with a thinkingTurnRule (deepseek + ollama + nanogpt)"
    );
    eprintln!(
        "OK: providers listing matched v4 ({with_schema} options schemas byte-for-byte,          {without_schema} null, {with_rule} thinking-turn rules)"
    );
}
