//! Tier-1 differential: `resolveDefaultSystemPrompt` / `…Id` — which system
//! prompt a character starts with (v4 `baa85e19b`, bug 154).
//!
//! Exact on all three comparands: the resolved prompt OBJECT, its id, and the
//! CONTENT form. The corpus (`harness/oracle/fixtures/default-system-prompt.json`)
//! carries v4's own five test vectors verbatim plus the shapes that suite does not
//! ask — JS truthiness on BOTH the column and the `isDefault` flag, the strict
//! `===` column match against a truthy non-string column, duplicate flags, a
//! resolved prompt with empty content, and a resolved prompt with no `id` key
//! (where the two forms disagree by design).
//!
//! ⚠ The `content` comparand pins [`resolve_default_system_prompt_content`]
//! against v4's resolver output, but the oracle TRANSCRIBES v4's
//! `lib/chat/initialize.ts:146` (`getDefaultSystemPrompt` is module-private —
//! `a-v4-service-that-exports-only-its-runner-has-no-tier-1-path`). The DRIVING
//! proof of the content fold is `chat_context_init_equivalence`'s
//! `empty_content_default` case, which reaches it through `buildChatContext`.
//!
//! ⚠ PIN REQUIRED at the TARGET `baa85e19b` until the baseline moves past it:
//! `lib/characters/default-system-prompt.ts` does not exist at `89fcc3c0d`, so a
//! baseline-pinned regen fails to resolve the import — that failure IS the pin
//! verification.
//!
//! Regenerate + run (self-contained; a pure tsx oracle, no fixture DB):
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -f /tmp/oracle-default-system-prompt.ndjson
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/default-system-prompt.ts \
//!     > /tmp/oracle-default-system-prompt.ndjson
//!   cd $V5W
//!   QT_ORACLE_DEFAULT_SYSTEM_PROMPT=/tmp/oracle-default-system-prompt.ndjson \
//!     cargo test -p quilltap-harness --test default_system_prompt_equivalence -- --nocapture

use std::collections::HashMap;

use quilltap_core::default_system_prompt::{
    resolve_default_system_prompt, resolve_default_system_prompt_content,
    resolve_default_system_prompt_id,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Case {
    id: String,
    character: Value,
}

#[derive(Deserialize)]
struct Spec {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct OracleRow {
    #[serde(rename = "case")]
    case: String,
    prompt: Option<Value>,
    id: Option<String>,
    content: String,
}

#[test]
fn default_system_prompt_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_DEFAULT_SYSTEM_PROMPT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_DEFAULT_SYSTEM_PROMPT to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let rows: HashMap<String, OracleRow> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let row: OracleRow = serde_json::from_str(l).unwrap_or_else(|e| panic!("row {l}: {e}"));
            (row.case.clone(), row)
        })
        .collect();

    let spec_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../harness/oracle/fixtures/default-system-prompt.json"
    );
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path).expect("read default-system-prompt.json"),
    )
    .expect("parse default-system-prompt.json");

    assert_eq!(
        rows.len(),
        spec.cases.len(),
        "the oracle ran a different number of cases than the corpus carries — stale NDJSON?"
    );

    // The shapes the corpus must actually contain, so a future trimmed regen
    // cannot go green having stopped asking the questions.
    let (mut saw_null, mut saw_empty_content, mut saw_prompt_without_id) = (false, false, false);
    let (mut saw_truthy_non_string_column, mut saw_truthy_non_bool_flag) = (false, false);

    for case in &spec.cases {
        let want = rows
            .get(&case.id)
            .unwrap_or_else(|| panic!("no oracle row for case '{}'", case.id));

        let got_prompt = resolve_default_system_prompt(&case.character).cloned();
        let got_id = resolve_default_system_prompt_id(&case.character);
        let got_content = resolve_default_system_prompt_content(&case.character);

        assert_eq!(got_prompt, want.prompt, "case '{}': prompt", case.id);
        assert_eq!(got_id, want.id, "case '{}': id", case.id);
        assert_eq!(got_content, want.content, "case '{}': content", case.id);

        if want.prompt.is_none() {
            saw_null = true;
        }
        if want.prompt.is_some() && want.content.is_empty() {
            saw_empty_content = true;
        }
        if want.prompt.is_some() && want.id.is_none() {
            saw_prompt_without_id = true;
        }
        match case.character.get("defaultSystemPromptId") {
            Some(Value::Number(n)) if n.as_f64() != Some(0.0) => {
                saw_truthy_non_string_column = true
            }
            _ => {}
        }
        if case
            .character
            .get("systemPrompts")
            .and_then(Value::as_array)
            .is_some_and(|ps| {
                ps.iter()
                    .any(|p| matches!(p.get("isDefault"), Some(Value::String(s)) if !s.is_empty()))
            })
        {
            saw_truthy_non_bool_flag = true;
        }
    }

    assert!(
        spec.cases.len() >= 20,
        "corpus too small: {}",
        spec.cases.len()
    );
    assert!(saw_null, "corpus has no row resolving to null");
    assert!(
        saw_empty_content,
        "corpus asks no resolved prompt with empty content (the `??` vs `||` arm)"
    );
    assert!(
        saw_prompt_without_id,
        "corpus asks no resolved prompt lacking an `id` (where the two forms disagree)"
    );
    assert!(
        saw_truthy_non_string_column,
        "corpus asks no truthy NON-STRING column (the `===` arm)"
    );
    assert!(
        saw_truthy_non_bool_flag,
        "corpus asks no truthy NON-BOOL `isDefault` (the flag's JS truthiness)"
    );
    eprintln!(
        "OK: default-system-prompt matched oracle ({} cases).",
        spec.cases.len()
    );
}
