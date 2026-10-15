//! The one cheap-LLM JSON fence-stripper every parser shares — v4
//! `stripCodeFences` (`lib/llm/llm-json.ts`).
//!
//! This module used to host the memory-extraction leaves of v4's
//! `cheap-llm-tasks/memory-tasks.ts`; v4 deleted that half in `ac6348c5f`
//! (memory refactor Phase D) and v5's port now lives in
//! [`crate::memory_extraction`] (work order P4.D296). [`strip_code_fences`]
//! stays here: seven non-extraction callers import it from this path (L-10).

use crate::jsstr::{js_trim, js_trim_end, js_trim_start};

/// Strip markdown code fences from LLM output before JSON parsing (v4
/// `stripCodeFences`, `lib/services/ai-import.service.ts` — the single source
/// for every cheap-LLM JSON parser). `^```json\s*` / `^```\s*` then
/// `\s*```$`, with JS `trim` / `\s` semantics.
pub fn strip_code_fences(text: &str) -> String {
    fn strip_trailing_fence(s: &str) -> &str {
        // `/\s*```$/` — removes the terminal fence plus the whitespace run
        // before it, but only when the string actually ends with a fence.
        match s.strip_suffix("```") {
            Some(rest) => js_trim_end(rest),
            None => s,
        }
    }

    let cleaned = js_trim(text);
    let cleaned = if let Some(rest) = cleaned.strip_prefix("```json") {
        strip_trailing_fence(js_trim_start(rest))
    } else if let Some(rest) = cleaned.strip_prefix("```") {
        strip_trailing_fence(js_trim_start(rest))
    } else {
        cleaned
    };
    js_trim(cleaned).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_code_fences_variants() {
        assert_eq!(strip_code_fences("```json\n[1]\n```"), "[1]");
        assert_eq!(strip_code_fences("``` [1] ```"), "[1]");
        assert_eq!(strip_code_fences("  [1]  "), "[1]");
        assert_eq!(strip_code_fences("```json[1]"), "[1]"); // no closing fence
        assert_eq!(strip_code_fences("``````"), "");
    }
}
