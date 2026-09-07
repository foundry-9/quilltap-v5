//! Tier-1 differential: the character optimizer's PURE arms (P4.9K1, unit 1) —
//! `coerceSuggestionArray` (bug 119's fix, v4 `15573c3a1`),
//! `buildCharacterContext`, `buildMemoryContext`, `getAnalysisPrompt` and the
//! six suggestion-prompt builders — byte-exact against v4's REAL exports.
//!
//! Every comparand is a whole string: these prompts reach a paid model
//! verbatim, so a one-byte drift is a real defect. The embedded
//! `JSON.stringify(analysis, null, 2)` makes the corpus's analyses a
//! pretty-printer comparand too, key ORDER included (one corpus analysis is
//! deliberately non-alphabetical with nested empties and odd scalars).
//!
//! Coverage is asserted by SHAPE in both directions off the oracle's final
//! `coverage` row.
//!
//! Regenerate the oracle (the corpus is committed — nothing to build):
//!   cd ~/source/quilltap-server
//!   npx tsx $V5W/harness/oracle/cases/character-optimizer-prompts.ts > /tmp/oracle-character-optimizer-prompts.ndjson
//! Run:
//!   QT_ORACLE_CHARACTER_OPTIMIZER_PROMPTS=/tmp/oracle-character-optimizer-prompts.ndjson cargo test -p quilltap-harness --test character_optimizer_prompts_equivalence
//!
//! While the v4 checkout sits on `bugfix` (drift-ledger §1) the regen needs a
//! worktree pinned at the baseline — the sweep driver's job
//! (`recipe_sweep.py --run character_optimizer_prompts_equivalence --v4 <pin>`),
//! never a path baked into this header.

use quilltap_core::generators::optimizer::{
    build_character_context, build_memory_context, coerce_suggestion_array, get_analysis_prompt,
    get_general_fields_suggestions_prompt, get_new_system_prompts_suggestion_prompt,
    get_physical_description_suggestion_prompt, get_properties_suggestion_prompt,
    get_scenario_suggestion_prompt, get_system_prompt_suggestion_prompt,
    get_wardrobe_suggestion_prompt,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "coerce")]
    Coerce {
        id: String,
        value: Value,
        out: Vec<Value>,
    },
    #[serde(rename = "context", rename_all = "camelCase")]
    Context {
        id: String,
        character: Value,
        wardrobe_items: Option<Vec<Value>>,
        out: String,
    },
    #[serde(rename = "memory")]
    Memory {
        id: String,
        entries: Vec<Value>,
        out: String,
    },
    #[serde(rename = "analysis_prompt")]
    AnalysisPrompt { id: String, out: String },
    #[serde(rename = "general_prompt")]
    GeneralPrompt {
        id: String,
        analysis: Value,
        out: String,
    },
    #[serde(rename = "new_prompts_prompt")]
    NewPromptsPrompt {
        id: String,
        analysis: Value,
        out: String,
    },
    #[serde(rename = "scenario_prompt")]
    ScenarioPrompt {
        id: String,
        analysis: Value,
        scenario: Value,
        out: String,
    },
    #[serde(rename = "system_prompt_prompt")]
    SystemPromptPrompt {
        id: String,
        analysis: Value,
        prompt: Value,
        out: String,
    },
    #[serde(rename = "physical_prompt")]
    PhysicalPrompt {
        id: String,
        analysis: Value,
        physical: Value,
        out: String,
    },
    #[serde(rename = "wardrobe_prompt")]
    WardrobePrompt {
        id: String,
        analysis: Value,
        items: Vec<Value>,
        out: String,
    },
    #[serde(rename = "properties_prompt")]
    PropertiesPrompt {
        id: String,
        analysis: Value,
        character: Value,
        out: String,
    },
    #[serde(rename = "coverage", rename_all = "camelCase")]
    Coverage {
        coerce: Vec<String>,
        contexts: Vec<String>,
        memories: Vec<String>,
        analyses: Vec<String>,
        scenarios: Vec<String>,
        system_prompts: Vec<String>,
        physicals: Vec<String>,
        wardrobes: Vec<String>,
    },
}

/// Show the first differing line of two multi-line prompts — a 6 KB
/// `assert_eq!` of two prompts is unreadable otherwise.
fn diff_report(label: &str, got: &str, want: &str) -> String {
    if got == want {
        return String::new();
    }
    let mut g = got.lines();
    let mut w = want.lines();
    let mut n = 1;
    loop {
        match (g.next(), w.next()) {
            (Some(a), Some(b)) if a == b => n += 1,
            (a, b) => {
                return format!(
                    "{label}: first difference at line {n}\n  v5:   {:?}\n  v4:   {:?}\n  (lengths {} vs {})",
                    a.unwrap_or("<eof>"),
                    b.unwrap_or("<eof>"),
                    got.len(),
                    want.len()
                );
            }
        }
    }
}

fn eq(label: &str, got: String, want: &str) {
    if got != want {
        panic!("{}", diff_report(label, &got, want));
    }
}

#[test]
fn character_optimizer_prompts_match_oracle() {
    let path = match std::env::var("QT_ORACLE_CHARACTER_OPTIMIZER_PROMPTS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CHARACTER_OPTIMIZER_PROMPTS to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let (mut s_coerce, mut s_ctx, mut s_mem, mut s_analyses) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut s_scen, mut s_sp, mut s_phys, mut s_ward) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut s_props: Vec<String> = Vec::new();
    let mut saw_analysis_prompt = false;
    let mut coverage: Option<Row> = None;

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("bad oracle row: {e}\n{}", &line[..line.len().min(200)]));
        match row {
            Row::Coerce { id, value, out } => {
                assert_eq!(
                    coerce_suggestion_array(&value),
                    out,
                    "coerceSuggestionArray '{id}'"
                );
                s_coerce.push(id);
            }
            Row::Context {
                id,
                character,
                wardrobe_items,
                out,
            } => {
                eq(
                    &format!("buildCharacterContext '{id}'"),
                    build_character_context(&character, wardrobe_items.as_deref()),
                    &out,
                );
                s_ctx.push(id);
            }
            Row::Memory { id, entries, out } => {
                eq(
                    &format!("buildMemoryContext '{id}'"),
                    build_memory_context(&entries),
                    &out,
                );
                s_mem.push(id);
            }
            Row::AnalysisPrompt { id, out } => {
                eq(
                    &format!("getAnalysisPrompt '{id}'"),
                    get_analysis_prompt(),
                    &out,
                );
                saw_analysis_prompt = true;
            }
            Row::GeneralPrompt { id, analysis, out } => {
                eq(
                    &format!("getGeneralFieldsSuggestionsPrompt '{id}'"),
                    get_general_fields_suggestions_prompt(&analysis),
                    &out,
                );
                s_analyses.push(id);
            }
            Row::NewPromptsPrompt { id, analysis, out } => {
                eq(
                    &format!("getNewSystemPromptsSuggestionPrompt '{id}'"),
                    get_new_system_prompts_suggestion_prompt(&analysis),
                    &out,
                );
            }
            Row::ScenarioPrompt {
                id,
                analysis,
                scenario,
                out,
            } => {
                eq(
                    &format!("getScenarioSuggestionPrompt '{id}'"),
                    get_scenario_suggestion_prompt(&analysis, &scenario),
                    &out,
                );
                s_scen.push(id);
            }
            Row::SystemPromptPrompt {
                id,
                analysis,
                prompt,
                out,
            } => {
                eq(
                    &format!("getSystemPromptSuggestionPrompt '{id}'"),
                    get_system_prompt_suggestion_prompt(&analysis, &prompt),
                    &out,
                );
                s_sp.push(id);
            }
            Row::PhysicalPrompt {
                id,
                analysis,
                physical,
                out,
            } => {
                eq(
                    &format!("getPhysicalDescriptionSuggestionPrompt '{id}'"),
                    get_physical_description_suggestion_prompt(&analysis, Some(&physical)),
                    &out,
                );
                s_phys.push(id);
            }
            Row::WardrobePrompt {
                id,
                analysis,
                items,
                out,
            } => {
                eq(
                    &format!("getWardrobeSuggestionPrompt '{id}'"),
                    get_wardrobe_suggestion_prompt(&analysis, &items),
                    &out,
                );
                s_ward.push(id);
            }
            Row::PropertiesPrompt {
                id,
                analysis,
                character,
                out,
            } => {
                eq(
                    &format!("getPropertiesSuggestionPrompt '{id}'"),
                    get_properties_suggestion_prompt(&analysis, &character),
                    &out,
                );
                s_props.push(id);
            }
            r @ Row::Coverage { .. } => coverage = Some(r),
        }
    }

    let Some(Row::Coverage {
        coerce,
        contexts,
        memories,
        analyses,
        scenarios,
        system_prompts,
        physicals,
        wardrobes,
    }) = coverage
    else {
        panic!("the oracle emitted no coverage row");
    };

    assert!(saw_analysis_prompt, "getAnalysisPrompt was never driven");
    assert_eq!(s_coerce, coerce, "coerce coverage");
    assert_eq!(s_ctx, contexts, "context coverage");
    assert_eq!(s_mem, memories, "memory coverage");
    assert_eq!(s_analyses, analyses, "analysis coverage");
    assert_eq!(s_scen, scenarios, "scenario coverage");
    assert_eq!(s_sp, system_prompts, "system-prompt coverage");
    assert_eq!(s_phys, physicals, "physical coverage");
    assert_eq!(s_ward, wardrobes, "wardrobe coverage");
    // The properties prompt is driven once per CONTEXT case (it takes a whole
    // character), so its coverage is the context list.
    assert_eq!(s_props, contexts, "properties coverage");

    // Floors, so a future corpus trim is loud.
    assert!(coerce.len() >= 15, "coerce corpus: {}", coerce.len());
    assert!(contexts.len() >= 5, "context corpus: {}", contexts.len());
    assert!(analyses.len() >= 3, "analysis corpus: {}", analyses.len());

    eprintln!(
        "OK: character-optimizer prompts matched oracle ({} coerce / {} contexts / {} memories / {} analyses / {} scenarios / {} system prompts / {} physicals / {} wardrobes).",
        coerce.len(), contexts.len(), memories.len(), analyses.len(),
        scenarios.len(), system_prompts.len(), physicals.len(), wardrobes.len()
    );
}
