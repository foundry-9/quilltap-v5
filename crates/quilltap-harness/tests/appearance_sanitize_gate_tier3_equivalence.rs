//! Tier-3 differential: v4 `lib/image-gen/appearance-resolution.ts`
//! `sanitizeAppearancesIfNeeded` — the five-rule appearance sanitize gate,
//! ported as
//! `quilltap_core::services::appearance_resolution::sanitize_appearances_if_needed`.
//!
//! [cc65d6bfc / bug 133] The family the P4.D178 survey found MISSING. Before it,
//! no harness family and no oracle case drove that function directly (measured:
//! zero hits for either spelling under `crates/quilltap-harness/tests` and
//! `harness/oracle/cases`). The story and image-generation tier-3 families reach
//! it only incidentally, and the image-generation corpus keeps the Concierge OFF
//! throughout — which is precisely why its fourth parameter could mean the wrong
//! thing for a whole phase without a single red row.
//!
//! Both sides run the same 27-case grid (`appearance-sanitize-gate.json`): mode
//! {OFF, DETECT_ONLY, AUTO_ROUTE} x `isDangerousChat` {t,f} x
//! `routesDangerousToUncensored` {t,f} x classification {safe, dangerous}, plus a
//! `customClassificationPrompt` row and two sanitizer-answer edges. Compared per
//! case: the returned appearances field-for-field, AND the NUMBER of completion
//! calls — which is what says WHICH rule fired (rules 1 and 2 make none, rule
//! 3-safe one, rule 4 one, rule 5 two). A gate that returned the right
//! appearances by a different route is a different bug, and the call count is
//! what catches it.
//!
//! MODEL SEAMS (pinned identically both sides): the completion boundary
//! ([`CannedCompletionProvider`], keyed on the exact recorded
//! `provider|model|temperature|messages`), and no moderation provider
//! ([`NoModerationProvider`]) so `classify_content` always falls to the cheap LLM.
//! The `Db` is a scratch instance carrying only `llm_logs` — this family does NOT
//! compare that projection (see the corpus `$comment`); the handle exists because
//! the classify path writes fire-and-forget rows through it.
//!
//! ⚠ Every case carries its own token inside the appearance text. The
//! classification cache is keyed by a sha256 of the content and is process-global
//! on BOTH sides, so two rows sharing text would make the second one's call-count
//! comparand measure the cache instead of the gate.
//!
//! Generate the oracle (Node 24, from the v4 checkout; stage OUTSIDE any
//! `.claude` path). See the oracle header. Run:
//!   QT_ORACLE_APPEARANCE_GATE=/tmp/oracle-appearance-sanitize-gate.ndjson \
//!     cargo test -p quilltap-harness --test appearance_sanitize_gate_tier3_equivalence

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use quilltap_core::cheap_llm::CheapLlmSelection;
use quilltap_core::db::chat_settings::DangerousContentSettings;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::Writer;
use quilltap_core::model::completion::{
    CannedCompletionProvider, CompletionError, CompletionMessage, CompletionParams,
    CompletionProvider, CompletionResponse, CompletionRole,
};
use quilltap_core::services::appearance_resolution::{
    sanitize_appearances_if_needed, ResolvedCharacterAppearance,
};
use quilltap_core::services::cheap_llm_exec::CheapLlmTaskExecutor;
use quilltap_core::services::dangerous_content::gatekeeper::NoModerationProvider;
use serde::Deserialize;
use serde_json::Value;

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaseSpec {
    label: String,
    token: String,
    mode: String,
    is_dangerous_chat: bool,
    routes_dangerous_to_uncensored: bool,
    #[serde(default)]
    custom_classification_prompt: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CharacterSpec {
    character_id: String,
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheapSelectionSpec {
    provider: String,
    model_name: String,
    connection_profile_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    user_id: String,
    threshold: f64,
    #[serde(rename = "cheapLLMSelection")]
    cheap_llm_selection: CheapSelectionSpec,
    characters: Vec<CharacterSpec>,
    cases: Vec<CaseSpec>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OracleAppearance {
    character_id: String,
    character_name: String,
    physical_description: String,
    physical_description_name: String,
    clothing_description: String,
    clothing_source: String,
    was_sanitized: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResultRow {
    label: String,
    completion_calls: usize,
    appearances: Vec<OracleAppearance>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CannedRow {
    provider: String,
    model: String,
    temperature: Option<f64>,
    messages: Vec<CannedMessage>,
    response: String,
}

#[derive(Deserialize)]
struct CannedMessage {
    role: String,
    content: String,
}

/// A [`CompletionProvider`] that counts the calls and delegates to the canned
/// one. The count is the comparand that says WHICH rule fired.
struct CountingCompletion {
    inner: CannedCompletionProvider,
    calls: Arc<AtomicUsize>,
}

impl CompletionProvider for CountingCompletion {
    async fn send_message(
        &self,
        provider: &str,
        base_url: Option<&str>,
        params: &CompletionParams,
    ) -> Result<CompletionResponse, CompletionError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.send_message(provider, base_url, params).await
    }
}

fn open_scratch_db(dir: &std::path::Path) -> Db {
    let main_path = dir.join("main.db");
    let ll_path = dir.join("llm-logs.db");
    drop(Writer::open_writable(&main_path, PEPPER).unwrap());
    {
        let w = Writer::open_writable(&ll_path, PEPPER).unwrap();
        w.connection()
            .execute_batch(
                "CREATE TABLE llm_logs (\
                   id TEXT PRIMARY KEY, userId TEXT, type TEXT, messageId TEXT, \
                   chatId TEXT, characterId TEXT, autonomousRunId TEXT, provider TEXT, \
                   modelName TEXT, connectionProfileId TEXT, imageProfileId TEXT, \
                   request TEXT, response TEXT, usage TEXT, \
                   cacheUsage TEXT, rawProviderUsage TEXT, requestHashes TEXT, \
                   durationMs REAL, createdAt TEXT, updatedAt TEXT);",
            )
            .unwrap();
    }
    Db::open(
        DbPaths {
            main: main_path,
            mount_index: None,
            llm_logs: Some(ll_path),
        },
        PEPPER,
    )
    .unwrap()
}

/// The appearance pair a case starts from — the oracle's `appearancesFor`, in
/// Rust. The token rides in the physical description so every case's combined
/// text (and therefore its classification-cache key) is distinct.
fn appearances_for(spec: &Spec, token: &str) -> Vec<ResolvedCharacterAppearance> {
    spec.characters
        .iter()
        .enumerate()
        .map(|(i, ch)| ResolvedCharacterAppearance {
            character_id: ch.character_id.clone(),
            character_name: ch.name.clone(),
            physical_description: format!(
                "{} ({token}), {}",
                ch.name,
                if i == 0 {
                    "a woman with copper hair"
                } else {
                    "a tall woman with dark braided hair"
                }
            ),
            physical_description_name: "Portrait".to_string(),
            clothing_description: if i == 0 {
                "wearing nothing at all".to_string()
            } else {
                "in a sheer slip".to_string()
            },
            clothing_source: "stored".to_string(),
            was_sanitized: false,
        })
        .collect()
}

fn danger_settings(case: &CaseSpec, threshold: f64) -> DangerousContentSettings {
    DangerousContentSettings {
        mode: case.mode.clone(),
        threshold,
        scan_text_chat: true,
        scan_image_prompts: false,
        scan_image_generation: false,
        uncensored_text_profile_id: None,
        uncensored_image_profile_id: None,
        display_mode: "SHOW".to_string(),
        show_warning_badges: true,
        custom_classification_prompt: case.custom_classification_prompt.clone(),
    }
}

#[tokio::test]
async fn appearance_sanitize_gate_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_APPEARANCE_GATE") else {
        eprintln!("SKIP: QT_ORACLE_APPEARANCE_GATE unset");
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/appearance-sanitize-gate.json"),
        )
        .expect("read the corpus"),
    )
    .expect("parse the corpus");

    let mut canned = CannedCompletionProvider::new();
    let mut results: std::collections::HashMap<String, ResultRow> =
        std::collections::HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle {oracle_path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        match v.get("kind").and_then(Value::as_str) {
            Some("canned") => {
                let row: CannedRow = serde_json::from_value(v).expect("parse canned row");
                let messages: Vec<CompletionMessage> = row
                    .messages
                    .iter()
                    .map(|m| CompletionMessage {
                        role: match m.role.as_str() {
                            "system" => CompletionRole::System,
                            "assistant" => CompletionRole::Assistant,
                            _ => CompletionRole::User,
                        },
                        content: m.content.clone(),
                    })
                    .collect();
                canned = canned.with_response(
                    &row.provider,
                    &row.model,
                    row.temperature,
                    &messages,
                    row.response,
                    None,
                );
            }
            Some("result") => {
                let row: ResultRow = serde_json::from_value(v).expect("parse result row");
                results.insert(row.label.clone(), row);
            }
            other => panic!("unexpected oracle row kind {other:?}"),
        }
    }
    assert_eq!(
        results.len(),
        spec.cases.len(),
        "the oracle must carry a result row for every corpus case"
    );

    let dir = tempfile::tempdir().expect("scratch dir");
    let db = open_scratch_db(dir.path());
    let moderation = NoModerationProvider;
    let executor = CheapLlmTaskExecutor::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let completion = CountingCompletion {
        inner: canned,
        calls: calls.clone(),
    };
    let selection = CheapLlmSelection {
        provider: spec.cheap_llm_selection.provider.clone(),
        model_name: spec.cheap_llm_selection.model_name.clone(),
        base_url: None,
        connection_profile_id: Some(spec.cheap_llm_selection.connection_profile_id.clone()),
        is_local: false,
        profile_parameters: None,
    };

    for case in &spec.cases {
        let expected = results
            .get(&case.label)
            .unwrap_or_else(|| panic!("no oracle result row for {}", case.label));
        calls.store(0, Ordering::SeqCst);
        let chat_id = format!("chat-{}", case.token);
        let got = sanitize_appearances_if_needed(
            &db,
            &executor,
            &moderation,
            &completion,
            appearances_for(&spec, &case.token),
            &danger_settings(case, spec.threshold),
            case.is_dangerous_chat,
            case.routes_dangerous_to_uncensored,
            &selection,
            &spec.user_id,
            Some(&chat_id),
        )
        .await;

        assert_eq!(
            calls.load(Ordering::SeqCst),
            expected.completion_calls,
            "{}: completion-call count diverged (which rule fired)",
            case.label
        );
        assert_eq!(
            got.len(),
            expected.appearances.len(),
            "{}: appearance count diverged",
            case.label
        );
        for (g, e) in got.iter().zip(expected.appearances.iter()) {
            assert_eq!(
                g.character_id, e.character_id,
                "{}: characterId",
                case.label
            );
            assert_eq!(
                g.character_name, e.character_name,
                "{}: characterName",
                case.label
            );
            assert_eq!(
                g.physical_description, e.physical_description,
                "{}: physicalDescription",
                case.label
            );
            assert_eq!(
                g.physical_description_name, e.physical_description_name,
                "{}: physicalDescriptionName",
                case.label
            );
            assert_eq!(
                g.clothing_description, e.clothing_description,
                "{}: clothingDescription",
                case.label
            );
            assert_eq!(
                g.clothing_source, e.clothing_source,
                "{}: clothingSource",
                case.label
            );
            assert_eq!(
                g.was_sanitized, e.was_sanitized,
                "{}: wasSanitized",
                case.label
            );
        }
    }

    // A coverage floor: the grid must actually exercise all five rules. Without
    // it a corpus edit could quietly leave every row at "zero calls, unchanged"
    // and the family would stay green having measured nothing.
    let counts: Vec<usize> = spec
        .cases
        .iter()
        .map(|c| results[&c.label].completion_calls)
        .collect();
    assert!(
        counts.iter().filter(|&&n| n == 0).count() >= 8,
        "rules 1 and 2 (no completion call) must be represented"
    );
    assert!(
        counts.iter().filter(|&&n| n == 1).count() >= 6,
        "rules 3-safe and 4 (one completion call) must be represented"
    );
    assert!(
        counts.iter().filter(|&&n| n == 2).count() >= 4,
        "rule 5 (classify + sanitize) must be represented"
    );
    assert!(
        results
            .values()
            .any(|r| r.appearances.iter().any(|a| a.was_sanitized)),
        "at least one row must actually come back sanitized"
    );

    println!(
        "OK: appearance sanitize gate matched the oracle across {} cases.",
        spec.cases.len()
    );
}
