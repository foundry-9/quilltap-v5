//! P4.D155 tier-1 differential: the cheap-LLM SELECTION shape — v4
//! `selectionFromProfile`, the five-rung `getCheapLLMProvider` ladder and
//! `resolveUncensoredCheapLLMSelection` (v4 `0506517d3` correction (a)).
//!
//! This is the family the lane identifies as driving
//! `cheap_llm::get_cheap_llm_provider`: nothing else in the harness did.
//! `cheap_llm_fallback_equivalence` drives `buildCheapFallbackSelections`, and
//! every tier-3 family that resolves a selection records a canned key of
//! `provider|model|temperature|messages` — which carries neither of the fields
//! `0506517d3` corrected. So both priority-5 branches' missing
//! `profileParameters`, and the provider-derived `isLocal`, were unmeasurable
//! until this family existed.
//!
//! The corpus lives in `harness/oracle/fixtures/cheap-llm-selection.json` and is
//! read by BOTH sides — the profile shapes (a `parameters` bag, a `maxContext`
//! for the Ollama `num_ctx` injection, a blank base URL) are exactly what a
//! transcribed corpus would drift on.
//!
//! **`deadlineMs` rides every row on purpose.** `isLocal`'s only two readers are
//! the API-key resolver and `deadlineFor`; the second is pure, so emitting the
//! background deadline turns the boolean into a NUMBER the diff can see.
//!
//! **The plugin registry is empty on both sides, deliberately.** v4's
//! `getCheapestModel` consults `getCheapModelConfig` first and nothing
//! initializes the registry under `tsx`, so priority 5 resolves through
//! `LEGACY_CHEAPEST_MODEL_MAP`; the Rust side passes
//! `registry_cheapest_for_current: None` to match. `p5_cheapest_deepseek_params`
//! is the out-of-vocabulary row that map does not cover: v4 returns `undefined`
//! (so `JSON.stringify` OMITS `modelName`), where v5 surfaces the empty string —
//! the divergence `get_cheap_llm_provider`'s own comment records. The reader
//! below defaults an absent `modelName` to `""` for exactly that row.
//!
//! Generate the oracle (Node 24, from the v4 checkout):
//!   cd ~/source/quilltap-server
//!   TZ=UTC ~/.nvm/versions/node/v24.13.1/bin/npx tsx \
//!     <V5W>/harness/oracle/cases/cheap-llm-selection.ts \
//!     > /tmp/oracle-cheap-llm-selection.ndjson
//! Run:
//!   QT_ORACLE_CHEAP_LLM_SELECTION=/tmp/oracle-cheap-llm-selection.ndjson \
//!     cargo test -p quilltap-harness --test cheap_llm_selection_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::cheap_llm::{
    get_cheap_llm_provider, resolve_uncensored_cheap_llm_selection, selection_from_profile,
    CheapLlmConfig, CheapLlmProfile, CheapLlmSelection, DangerousContentSettings,
};
use quilltap_core::services::cheap_llm_exec::{cheap_llm_deadline_for, CheapLlmLatencyClass};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CorpusProfile {
    id: String,
    provider: String,
    model_name: String,
    base_url: Option<String>,
    is_cheap: bool,
    is_dangerous_compatible: bool,
    parameters: Option<Value>,
    max_context: Option<f64>,
}

impl From<&CorpusProfile> for CheapLlmProfile {
    fn from(p: &CorpusProfile) -> Self {
        CheapLlmProfile {
            id: p.id.clone(),
            provider: p.provider.clone(),
            model_name: p.model_name.clone(),
            base_url: p.base_url.clone(),
            is_cheap: p.is_cheap,
            is_dangerous_compatible: p.is_dangerous_compatible,
            parameters: p.parameters.clone(),
            max_tokens: None,
            max_context: p.max_context,
            model_class: None,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FromProfileCase {
    name: String,
    profile: String,
    local_base_url_fallback: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CorpusConfig {
    strategy: String,
    fallback_to_local: bool,
    #[serde(default)]
    user_defined_profile_id: Option<String>,
    #[serde(default)]
    default_cheap_profile_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LadderCase {
    name: String,
    current: String,
    config: CorpusConfig,
    available: Vec<String>,
    ollama_available: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CorpusDanger {
    mode: String,
    #[serde(default)]
    uncensored_text_profile_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UncensoredCase {
    name: String,
    standard: String,
    is_dangerous_chat: bool,
    danger_settings: CorpusDanger,
    available: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Corpus {
    profiles: HashMap<String, CorpusProfile>,
    from_profile: Vec<FromProfileCase>,
    ladder: Vec<LadderCase>,
    uncensored: Vec<UncensoredCase>,
}

fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/cheap-llm-selection.json")
}

/// The emitted shape, matching the oracle's `emit()` key for key.
fn row(name: &str, kind: &str, s: &CheapLlmSelection) -> Value {
    json!({
        "name": name,
        "kind": kind,
        "provider": s.provider,
        "modelName": s.model_name,
        "baseUrl": s.base_url,
        "connectionProfileId": s.connection_profile_id,
        "isLocal": s.is_local,
        "profileParameters": s.profile_parameters,
        "deadlineMs": cheap_llm_deadline_for(s, None, CheapLlmLatencyClass::Background),
    })
}

/// v4 emits `undefined` fields by omission; normalize the two that can be
/// absent so an omitted key and an explicit null compare the same way. See the
/// header for `modelName` — only the out-of-vocabulary priority-5 row hits it.
fn normalize_oracle(mut v: Value) -> Value {
    let o = v.as_object_mut().expect("oracle row is an object");
    o.entry("modelName").or_insert_with(|| json!(""));
    o.entry("baseUrl").or_insert(Value::Null);
    o.entry("profileParameters").or_insert(Value::Null);
    v
}

#[test]
fn cheap_llm_selection_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_CHEAP_LLM_SELECTION") else {
        eprintln!("SKIP: set QT_ORACLE_CHEAP_LLM_SELECTION (see test header).");
        return;
    };
    let corpus: Corpus = serde_json::from_str(
        &std::fs::read_to_string(corpus_path()).expect("read cheap-llm-selection.json"),
    )
    .expect("parse corpus");

    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle {oracle_path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        let name = v["name"].as_str().expect("row name").to_string();
        oracle.insert(name, normalize_oracle(v));
    }

    let profile = |key: &str| -> CheapLlmProfile {
        CheapLlmProfile::from(
            corpus
                .profiles
                .get(key)
                .unwrap_or_else(|| panic!("no such corpus profile: {key}")),
        )
    };

    let mut failed: Vec<String> = Vec::new();
    let mut ran = 0usize;
    let mut check = |got: Value| {
        let name = got["name"].as_str().unwrap().to_string();
        match oracle.get(&name) {
            None => failed.push(format!("{name}: no oracle row")),
            Some(want) if want == &got => {}
            Some(want) => failed.push(format!("{name}:\n  rust:   {got}\n  oracle: {want}")),
        }
    };

    for c in &corpus.from_profile {
        let s = selection_from_profile(&profile(&c.profile), c.local_base_url_fallback);
        check(row(&c.name, "fromProfile", &s));
        ran += 1;
    }

    for c in &corpus.ladder {
        let available: Vec<CheapLlmProfile> = c.available.iter().map(|k| profile(k)).collect();
        let config = CheapLlmConfig {
            strategy: c.config.strategy.clone(),
            user_defined_profile_id: c.config.user_defined_profile_id.clone(),
            default_cheap_profile_id: c.config.default_cheap_profile_id.clone(),
            fallback_to_local: c.config.fallback_to_local,
        };
        // `None` = the empty plugin registry the oracle also runs under.
        let s = get_cheap_llm_provider(
            &profile(&c.current),
            &config,
            &available,
            c.ollama_available,
            None,
        );
        check(row(&c.name, "ladder", &s));
        ran += 1;
    }

    for c in &corpus.uncensored {
        let available: Vec<CheapLlmProfile> = c.available.iter().map(|k| profile(k)).collect();
        let standard = selection_from_profile(&profile(&c.standard), false);
        let danger = DangerousContentSettings {
            mode: c.danger_settings.mode.clone(),
            uncensored_text_profile_id: c.danger_settings.uncensored_text_profile_id.clone(),
        };
        let s = resolve_uncensored_cheap_llm_selection(
            standard,
            c.is_dangerous_chat,
            Some(&danger),
            &available,
        );
        check(row(&c.name, "uncensored", &s));
        ran += 1;
    }

    assert_eq!(
        ran,
        oracle.len(),
        "corpus and oracle disagree on the row count — regenerate the oracle"
    );
    assert!(ran >= 26, "coverage floor: {ran} rows");
    assert!(
        failed.is_empty(),
        "cheap-LLM selection mismatches:\n{}",
        failed.join("\n")
    );
}
