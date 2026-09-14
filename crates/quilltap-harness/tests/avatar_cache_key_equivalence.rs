//! P4.D184 tier-1 differential: the avatar configuration cache KEY derivation
//! (v4 `lib/wardrobe/avatar-cache.ts` `deriveAvatarCacheKey` /
//! `deriveLegacyAvatarCacheKey` / `deriveAvatarCacheKeys`, v4 `7fbf8a55b`) vs
//! `quilltap_core::services::avatar_cache`.
//!
//! Both sides read the SAME corpus (`harness/oracle/fixtures/avatar-cache-key
//! .json`) so the inputs cannot drift, and the hex digests are compared EXACTLY.
//! The corpus's `$note` documents its two sentinels; the identities it exists to
//! pin are asserted here as well as diffed, because "both sides agree" and "both
//! sides agree AND the rule holds" are different claims:
//!
//!   - an explicit `undefined` and its absent-key sibling are ONE key (v4 filters
//!     `undefined`; v5 has no such value, and `to_key_value` omits every `None`);
//!   - an explicit `null` is NEITHER of them — a real value, in the preimage;
//!   - a reordered params object is one key, a reordered LoRA list is two;
//!   - `modelName` null and `modelName` absent are one v0 key;
//!   - a v1 key never collides with the v0 key over the same prompt and model.
//!
//! ⚠ The module this drives arrived at v4 `7fbf8a55b`, which is PAST the
//! `f4ad2c8d1` oracle baseline — so until the baseline moves, regenerate through
//! the sweep driver with a pin at or after that commit
//! (`recipe_sweep.py --v4 <pinned worktree> --run avatar_cache_key_equivalence`),
//! which rewrites the `cd` below. Against a checkout still at the baseline the
//! import fails outright; it cannot pass stale.
//!
//! Generate the oracle output (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/avatar-cache-key.ts \
//!     > /tmp/oracle-avatar-cache-key.ndjson
//! Run:
//!   QT_ORACLE_AVATAR_CACHE_KEY=/tmp/oracle-avatar-cache-key.ndjson \
//!     cargo test -p quilltap-harness --test avatar_cache_key_equivalence

use std::collections::HashMap;

use quilltap_core::services::avatar_cache::{
    derive_avatar_cache_key, derive_avatar_cache_keys, derive_legacy_avatar_cache_key,
};
use serde_json::Value;

/// The corpus's `"__UNDEFINED__"` sentinel means "this key is present with the
/// value `undefined`". A `serde_json::Value` cannot hold that, and neither can
/// v5's params object: `ImageGenParams::to_key_value` omits every `None`. So on
/// this side the sentinel means REMOVE THE KEY — and the corpus's absent-key
/// sibling case is what proves that is the same answer v4 reaches by filtering.
fn strip_undefined(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                if v.as_str() == Some("__UNDEFINED__") {
                    continue;
                }
                out.insert(k.clone(), strip_undefined(v));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(strip_undefined).collect()),
        leaf => leaf.clone(),
    }
}

fn corpus_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/avatar-cache-key.json")
}

#[test]
fn avatar_cache_keys_match_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_AVATAR_CACHE_KEY") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_AVATAR_CACHE_KEY to the oracle NDJSON (see header).");
            return;
        }
    };
    let body = std::fs::read_to_string(&oracle_path).expect("read oracle");
    let corpus: Value =
        serde_json::from_str(&std::fs::read_to_string(corpus_path()).expect("read corpus"))
            .expect("parse corpus");
    let cases = corpus["cases"].as_array().expect("corpus cases").clone();

    // name -> the case's own spec, so the oracle row drives the lookup.
    let by_name: HashMap<String, Value> = cases
        .iter()
        .map(|c| {
            (
                c["name"].as_str().expect("case name").to_string(),
                c.clone(),
            )
        })
        .collect();
    assert_eq!(
        by_name.len(),
        cases.len(),
        "corpus case names must be unique (a duplicate would silently shadow)"
    );

    let mut keys_by_name: HashMap<String, String> = HashMap::new();
    let mut rows = 0usize;
    for line in body.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("parse oracle row");
        let name = row["name"].as_str().expect("name").to_string();
        let kind = row["kind"].as_str().expect("kind");
        let want = row["key"].as_str().expect("key").to_string();
        let case = by_name
            .get(&name)
            .unwrap_or_else(|| panic!("oracle row {name} has no corpus case"));

        let got = match kind {
            "v0" => {
                // `__ABSENT__` and an explicit null are one value in Rust.
                let model = case["modelName"].as_str().filter(|m| *m != "__ABSENT__");
                derive_legacy_avatar_cache_key(model, case["prompt"].as_str().unwrap_or(""))
            }
            "v1" | "pair" => {
                let params = strip_undefined(&case["params"]);
                let provider = case["provider"].as_str().expect("provider");
                let profile_id = case["imageProfileId"].as_str().expect("imageProfileId");
                if kind == "v1" {
                    derive_avatar_cache_key(provider, profile_id, &params)
                } else {
                    let pair = derive_avatar_cache_keys(provider, profile_id, &params);
                    let want_legacy = row["legacyKey"].as_str().expect("legacyKey");
                    assert_eq!(
                        pair.legacy_key, want_legacy,
                        "legacy half of pair case {name}"
                    );
                    pair.key
                }
            }
            other => panic!("unknown oracle kind {other}"),
        };

        assert_eq!(got, want, "case {name} ({kind})");
        keys_by_name.insert(name, got);
        rows += 1;
    }

    assert_eq!(
        rows,
        cases.len(),
        "every corpus case must appear in the oracle NDJSON"
    );
    assert!(
        rows >= 20,
        "the order's floor: at least 20 shapes, got {rows}"
    );

    // ---- the identities, asserted on OUR OWN digests -----------------------
    // A pure diff would stay green if BOTH sides broke the same rule the same
    // way; these say what the rule is.
    let k = |n: &str| -> &str {
        keys_by_name
            .get(n)
            .unwrap_or_else(|| panic!("missing case {n}"))
            .as_str()
    };

    assert_eq!(
        k("v1-baseline"),
        k("v1-baseline-repeat"),
        "identical input must be stable across calls"
    );
    assert_eq!(
        k("v1-baseline"),
        k("v1-baseline-key-order-shuffled"),
        "params key order must not reach the digest"
    );
    assert_ne!(
        k("v1-baseline"),
        k("v1-prompt-changed"),
        "the prompt is part of the configuration"
    );
    assert_ne!(
        k("v1-baseline"),
        k("v1-model-changed"),
        "the model is part of the configuration"
    );
    assert_ne!(
        k("v1-baseline"),
        k("v1-profile-changed-same-model"),
        "two profiles on one model are two configurations"
    );
    assert_ne!(
        k("v1-baseline"),
        k("v1-provider-changed-same-model"),
        "the provider is part of the configuration"
    );
    assert_ne!(
        k("v1-loras-ab"),
        k("v1-loras-ba"),
        "a LoRA list is ordered — two orderings are two configurations"
    );
    assert_eq!(
        k("v1-loras-ab"),
        k("v1-lora-entry-keys-shuffled"),
        "key order INSIDE a LoRA entry is still just key order"
    );
    assert_eq!(
        k("v1-explicit-undefined"),
        k("v1-explicit-undefined-absent-sibling"),
        "an explicit undefined and a missing key mean the same thing to a provider"
    );
    assert_ne!(
        k("v1-explicit-undefined"),
        k("v1-explicit-null-not-the-same-as-undefined"),
        "a null is a real value and must survive into the preimage"
    );
    assert_eq!(
        k("v1-nested-undefined-inside-profile-parameters"),
        k("v1-nested-undefined-absent-sibling"),
        "the undefined filter is recursive, not top-level only"
    );
    assert_eq!(
        k("v1-nested-unsorted-object"),
        k("v1-nested-sorted-object-same-content"),
        "the key sort is recursive"
    );
    assert_ne!(
        k("v1-nested-unsorted-object"),
        k("v1-nested-array-order-matters"),
        "arrays keep their order at every depth"
    );
    assert_eq!(
        k("v0-null-model"),
        k("v0-absent-model"),
        "v4's `modelName ?? null` makes a null and a missing model one key"
    );
    assert_ne!(
        k("v0-baseline"),
        k("v0-null-model"),
        "the model still discriminates when it is present"
    );
    assert_ne!(
        k("pair-baseline"),
        k("v0-collides-with-nothing-v1"),
        "the `v` discriminator is what keeps a v1 key off a v0 key"
    );
}
