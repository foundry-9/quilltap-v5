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
//! Both sides run the same 30-case grid (`appearance-sanitize-gate.json`): the
//! Concierge policy (v4 `3b463d6b1`, #76 — resolved per row from its stored
//! `concierge` settings WITH its `chat`) {off duty, Moderated + pre-screen,
//! Unmoderated} x `isDangerousChat` {t,f} x `routesDangerousToUncensored`
//! {t,f} x classification {safe, dangerous}, plus a
//! `customClassificationPrompt` row, two sanitizer-answer edges, and #76's
//! three pass-throughs (Moderated without the pre-screen, Locked, exempt). Compared per
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
//! ⚠ Only the DANGEROUS rows pin the canned completion KEY. `CannedCompletionProvider`
//! answers a key miss with `Err`, which the classify step degrades to "safe →
//! return unchanged, one call" — byte-identical to every `…_safe` row. A corpus
//! trim that dropped the dangerous rows would leave this family green while
//! measuring nothing (the §3 unification review's N2). Two v4 error arms are
//! deliberately NOT in the grid: a thrown classify (`appearance-resolution.ts`
//! warn + return) and a failed sanitize task — both exist in v5, neither has a
//! corpus row (N3, recorded, not ordered). [97b25fc53] P4.D239 CLOSED the second
//! half: `sanitizeThrows` rows (AGCASE39/40) throw on v4's side and MISS the
//! canned key on v5's (no row is recorded), so both fail the task and take the
//! `Sanitization failed` WARN arm. The first half is UNREACHABLE in v4 itself
//! (measured at `97b25fc53`: `classifyContent` wraps `runClassification`, a
//! total try/catch) — there is no row to write.
//!
//! [97b25fc53] P4.D239 — the drape. Rows AGCASE31..40 pass the sanitize MODE as
//! the 8th argument (absent = `Redress`, v4's default) and pose the conceal
//! reply's `undressed`. Three comparands joined the per-row diff:
//!   - `needsConcealment` per appearance (the oracle key is present only when
//!     `true` — asserted never `false`, the v4 invariant);
//!   - `concealPromptCalls` — how many completion calls carried the CONCEAL
//!     sanitizer prompt. Both prompts open with the same sentence, so the
//!     oracle's mock needs a second probe (`Do NOT invent clothing`) and so does
//!     this counter;
//!   - `logLines` — v4's REAL logger calls for the gate's `[AppearanceResolution]`
//!     lines and the sanitizer's pre-call `[CheapLLM] Sanitizing appearances`
//!     DEBUG, recorded by a delegating spy, compared against this side's
//!     thread-scoped capture IN ORDER (lines and fields). v4 camelCase keys map
//!     to v5's snake_case fields (`FIELD_MAP`); v4's `categories` array rides the
//!     file layer's `categoriesJson` convention. One field is compared by
//!     PRESENCE only: the failure WARN's `error` (v4's is the mock's thrown
//!     message, v5's the canned-miss text — the arm is the comparand, not the
//!     provider's wording).
//!
//! The `Db` is a scratch instance carrying only `llm_logs` — this family does NOT
//! compare that projection (see the corpus `$comment`); the handle exists because
//! the classify path writes fire-and-forget rows through it.
//!
//! ⚠ Every case carries its own token inside the appearance text. The
//! classification cache is keyed by a sha256 of the content and is process-global
//! on BOTH sides, so two rows sharing text would make the second one's call-count
//! comparand measure the cache instead of the gate.
//!
//! Generate the oracle (Node 24, from the v4 checkout — PIN REQUIRED: from a
//! detached worktree at the round target; stage OUTSIDE any `.claude` path).
//! DB-free on the oracle side, so no fixture-vintage rule applies. See the
//! oracle header. Run:
//!   QT_ORACLE_APPEARANCE_GATE=/tmp/oracle-appearance-sanitize-gate.ndjson \
//!     cargo test -p quilltap-harness --test appearance_sanitize_gate_tier3_equivalence

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use quilltap_core::services::image_scene_tasks::AppearanceSanitizeMode;

use quilltap_core::cheap_llm::CheapLlmSelection;
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
use quilltap_core::services::dangerous_content::resolver::resolve_stored_concierge_settings;
use serde::Deserialize;
use serde_json::Value;

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaseSpec {
    label: String,
    token: String,
    /// v4 `3b463d6b1` (#76): the stored `conciergeSettings` + the chat the
    /// policy resolves WITH.
    concierge: serde_json::Value,
    chat: serde_json::Value,
    is_dangerous_chat: bool,
    routes_dangerous_to_uncensored: bool,
    /// [97b25fc53] `"redress"` / `"conceal"`; absent = v4's default (`Redress`).
    #[serde(default)]
    mode: Option<String>,
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
    /// [97b25fc53] present only when `true` (v4 never writes `false`).
    #[serde(default)]
    needs_concealment: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OracleLogLine {
    level: String,
    message: String,
    context: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResultRow {
    label: String,
    completion_calls: usize,
    conceal_prompt_calls: usize,
    log_lines: Vec<OracleLogLine>,
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
/// one. The count is the comparand that says WHICH rule fired; the second
/// counter is the conceal-prompt probe (the oracle mock's twin).
struct CountingCompletion {
    inner: CannedCompletionProvider,
    calls: Arc<AtomicUsize>,
    conceal_calls: Arc<AtomicUsize>,
}

impl CompletionProvider for CountingCompletion {
    async fn send_message(
        &self,
        provider: &str,
        base_url: Option<&str>,
        params: &CompletionParams,
    ) -> Result<CompletionResponse, CompletionError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if params.messages.iter().any(|m| {
            m.role == CompletionRole::System && m.content.contains("Do NOT invent clothing")
        }) {
            self.conceal_calls.fetch_add(1, Ordering::SeqCst);
        }
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
            needs_concealment: false,
        })
        .collect()
}

/// v4 context key → v5 tracing field name. Anything absent here is a key the
/// port never carries — a loud panic, not a silent skip.
const FIELD_MAP: &[(&str, &str)] = &[
    ("context", "context"),
    ("chatId", "chat_id"),
    ("conciergeSource", "concierge_source"),
    ("score", "score"),
    ("categories", "categoriesJson"),
    (
        "routesDangerousToUncensored",
        "routes_dangerous_to_uncensored",
    ),
    ("characterCount", "character_count"),
    ("mode", "mode"),
    ("count", "count"),
    ("error", "error"),
];

/// Render one v4 logger call the way the thread-scoped capture renders v5's
/// event: `LEVEL target message k=v k=v…`, keys in v4's order. The `error`
/// value is elided on BOTH sides (compared by presence — see the header).
fn render_v4_line(l: &OracleLogLine) -> String {
    let target = if l.message.starts_with("[CheapLLM]") {
        "quilltap::cheap_llm"
    } else {
        "quilltap::appearance_resolution"
    };
    let mut out = format!("{} {target} {}", l.level.to_uppercase(), l.message);
    let Value::Object(ctx) = &l.context else {
        panic!("v4 log context is not an object: {:?}", l.context);
    };
    for (k, v) in ctx {
        let field = FIELD_MAP
            .iter()
            .find(|(v4, _)| v4 == k)
            .unwrap_or_else(|| panic!("unmapped v4 log key {k:?} on {:?}", l.message))
            .1;
        let rendered = match v {
            _ if field == "error" => "<elided>".to_string(),
            Value::String(s) => s.clone(),
            Value::Array(_) => serde_json::to_string(v).unwrap(),
            other => other.to_string(),
        };
        out.push_str(&format!(" {field}={rendered}"));
    }
    out
}

/// The v5 side's lines of interest, with the `error` value elided.
fn v5_lines(captured: &[String]) -> Vec<String> {
    captured
        .iter()
        .filter(|l| {
            l.contains(" [AppearanceResolution] ")
                || l.contains(" [CheapLLM] Sanitizing appearances")
        })
        .map(|l| match l.find(" error=") {
            Some(i) => format!("{} error=<elided>", &l[..i]),
            None => l.clone(),
        })
        .collect()
}

/// A plain `#[test]` driving a current-thread runtime INSIDE the thread-scoped
/// capture, per case (the `ai_import_tier3` shape).
#[test]
fn appearance_sanitize_gate_matches_oracle() {
    // P4.133 OUT-OF-MANDATE (dogfood #133): the oracle mocks
    // `getApiKeyForCheapLLMSelection` to a constant; the twin (v5 now resolves
    // the cheap selection's own key — and the Concierge classifier's — and
    // refuses without one).
    let _canned_key = quilltap_core::test_support::CannedCheapLlmKey::install("test-key");
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
                        role: CompletionRole::from_v4_wire(m.role.as_str())
                            .unwrap_or(CompletionRole::User),
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
    let mut log_lines_compared = 0usize;
    let moderation = NoModerationProvider;
    let executor = CheapLlmTaskExecutor::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let conceal_calls = Arc::new(AtomicUsize::new(0));
    let completion = CountingCompletion {
        inner: canned,
        calls: calls.clone(),
        conceal_calls: conceal_calls.clone(),
    };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
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
        conceal_calls.store(0, Ordering::SeqCst);
        let chat_id = format!("chat-{}", case.token);
        let mode = match case.mode.as_deref() {
            None => AppearanceSanitizeMode::default(),
            Some("redress") => AppearanceSanitizeMode::Redress,
            Some("conceal") => AppearanceSanitizeMode::Conceal,
            Some(other) => panic!("{}: unknown mode {other:?}", case.label),
        };
        let (got, captured) = quilltap_core::test_support::captured_with(|| {
            rt.block_on(sanitize_appearances_if_needed(
                &db,
                &executor,
                &moderation,
                &completion,
                appearances_for(&spec, &case.token),
                &resolve_stored_concierge_settings(Some(&case.concierge), Some(&case.chat)),
                case.is_dangerous_chat,
                case.routes_dangerous_to_uncensored,
                &selection,
                &spec.user_id,
                Some(&chat_id),
                mode,
            ))
        });

        assert_eq!(
            calls.load(Ordering::SeqCst),
            expected.completion_calls,
            "{}: completion-call count diverged (which rule fired)",
            case.label
        );
        assert_eq!(
            conceal_calls.load(Ordering::SeqCst),
            expected.conceal_prompt_calls,
            "{}: conceal-prompt call count diverged (which prompt the mode picked)",
            case.label
        );
        let want: Vec<String> = expected.log_lines.iter().map(render_v4_line).collect();
        assert_eq!(
            v5_lines(&captured),
            want,
            "{}: log lines diverged (order, level, message, fields)",
            case.label
        );
        log_lines_compared += want.len();
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
            assert_ne!(
                e.needs_concealment,
                Some(false),
                "{}: v4 never writes needsConcealment: false",
                case.label
            );
            assert_eq!(
                g.needs_concealment,
                e.needs_concealment.unwrap_or(false),
                "{}: needsConcealment",
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
    // [97b25fc53] The drape's floors: a flagged character, one flagged WITHOUT
    // a text change (the echo arm that keeps its clothing), the conceal prompt
    // actually sent, the failure WARN actually logged, and the log comparand
    // actually compared something.
    let all_appearances = || results.values().flat_map(|r| r.appearances.iter());
    assert!(
        all_appearances().any(|a| a.needs_concealment == Some(true)),
        "at least one appearance must come back needsConcealment"
    );
    assert!(
        all_appearances().any(|a| a.needs_concealment == Some(true) && !a.was_sanitized),
        "the unchanged-echo + undressed arm must be represented"
    );
    assert!(
        results
            .values()
            .filter(|r| r.conceal_prompt_calls > 0)
            .count()
            >= 5,
        "the conceal prompt must actually reach the wire"
    );
    assert!(
        results.values().any(|r| r
            .log_lines
            .iter()
            .any(|l| l.message
                == "[AppearanceResolution] Sanitization failed, passing through original")),
        "the sanitize-failure WARN arm must be represented"
    );
    assert!(
        log_lines_compared >= 60,
        "only {log_lines_compared} log lines compared"
    );

    println!(
        "OK: appearance sanitize gate matched the oracle across {} cases ({} log lines).",
        spec.cases.len(),
        log_lines_compared
    );
}
