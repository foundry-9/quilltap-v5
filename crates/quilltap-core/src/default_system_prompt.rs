//! Port of v4's `lib/characters/default-system-prompt.ts` (v4 `baa85e19b`,
//! bug 154) — **which system prompt does a character start with?**
//!
//! The answer is recorded twice: the character's `defaultSystemPromptId` column
//! and the `isDefault` flag on the prompt itself. It used to be resolved by hand
//! at five call sites, three of which disagreed about what to do when the column
//! named a prompt the character no longer had; two of those quietly gave up and
//! started the chat with no prompt at all. v4 states the order once, structurally
//! typed and import-free so the client picker and the server's chat initializer
//! can share it.
//!
//! **The order (v4's `resolveDefaultSystemPrompt`, and this module's contract):**
//!
//!   1. `prompts = systemPrompts ?? []`; an empty list answers `None`.
//!   2. the column **when it is JS-truthy AND names a prompt that exists** —
//!      the existence check is the fix: a stale column no longer wins.
//!   3. else the first prompt whose `isDefault` is JS-truthy.
//!   4. else the first prompt.
//!   5. else `None`.
//!
//! The write side of the same fact is
//! [`crate::db::vault_character_arrays::set_default_system_prompt`] and the
//! shared patch every system-prompt write applies (v4's `systemPromptsPatch`).
//!
//! ## JS truthiness, spelled rather than assumed
//!
//! v4 tests the column with `if (character.defaultSystemPromptId)` and the flag
//! with `prompts.find((p) => p.isDefault)` — plain JS truthiness in both places,
//! not `=== true` and not "is a string". The shapes the vault reader actually
//! produces are a `string`/`null`/absent column and a `bool` flag, so the wider
//! spelling is unobservable in production; it is written out anyway (and asked
//! by the corpus) so a future reader cannot mistake a narrowing for the contract.
//! The column match is JS `===`, so a truthy NON-string column can never equal a
//! string `id` and falls through exactly as a falsy one would.

use serde_json::Value;

/// JS truthiness for a `serde_json::Value` (the `if (x)` / `find(p => p.flag)`
/// test): `''`, `0`, `false`, `null` and an absent key are falsy; every other
/// value — including an empty array or object — is truthy.
fn js_truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Number(n)) => n.as_f64().map(|f| f != 0.0 && !f.is_nan()).unwrap_or(true),
        Some(Value::Array(_)) | Some(Value::Object(_)) => true,
    }
}

/// The prompt a character starts with, or `None` when they have none at all
/// (v4 `resolveDefaultSystemPrompt`).
///
/// `character` is any object carrying `systemPrompts` and `defaultSystemPromptId`
/// — a hydrated character from the read overlay, or the slim row plus its vault
/// prompts. A `systemPrompts` that is absent, `null` or not an array reads as the
/// empty list (v4's `?? []`; the non-array shape is unreachable from the vault
/// reader, whose projection always yields an array).
pub fn resolve_default_system_prompt(character: &Value) -> Option<&Value> {
    let prompts = character
        .get("systemPrompts")
        .and_then(Value::as_array)
        .filter(|p| !p.is_empty())?;

    // The column FIRST — but only when it names a prompt that is still there.
    // (Before `baa85e19b` three of the five hand-rolled copies honoured a stale
    // column; two of them then seeded a chat with no system prompt at all.)
    let column = character.get("defaultSystemPromptId");
    if js_truthy(column) {
        if let Some(named) = prompts.iter().find(|p| p.get("id") == column) {
            return Some(named);
        }
    }

    prompts
        .iter()
        .find(|p| js_truthy(p.get("isDefault")))
        .or_else(|| prompts.first())
}

/// The id of [`resolve_default_system_prompt`], for the pickers that store one
/// (v4 `resolveDefaultSystemPromptId` — `resolve(...)?.id ?? null`).
pub fn resolve_default_system_prompt_id(character: &Value) -> Option<String> {
    resolve_default_system_prompt(character)
        .and_then(|p| p.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// The CONTENT of [`resolve_default_system_prompt`], the server-only form v4
/// spells inline as `lib/chat/initialize.ts`'s `getDefaultSystemPrompt`:
/// `resolveDefaultSystemPrompt(character)?.content ?? ''`.
///
/// ⚠ The `??` is load-bearing and the commit message never mentions it: before
/// `baa85e19b` the flag arm read `defaultPrompt?.content || systemPrompts[0]?.
/// content || ''`, so a default prompt with EMPTY content fell through to the
/// first prompt's content. It now answers `''` — the resolved prompt is the
/// answer even when its content is empty.
pub fn resolve_default_system_prompt_content(character: &Value) -> String {
    resolve_default_system_prompt(character)
        .and_then(|p| p.get("content"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The `??` in the CONTENT form, pinned here because no differential can
    /// reach it. v4 guards an empty-content prompt THREE times over at
    /// `baa85e19b` — all three MEASURED by running them against the pinned
    /// worktree while this unit was ported:
    ///
    ///   1. `CharacterSystemPromptSchema.content` is `.min(1)`, so
    ///      `repos.characters.create` refuses it ("Too small: expected string to
    ///      have >=1 characters").
    ///   2. `parsePromptFile` SKIPS a `Prompts/*.md` whose body is empty
    ///      ("Prompts/*.md body is empty; skipping"), so planting the file drops
    ///      the prompt rather than carrying it.
    ///   3. `findById` re-VALIDATES the hydrated character, so planting it into a
    ///      vault-less character's slim `systemPrompts` column makes the whole
    ///      character unreadable — `buildChatContext` then answers
    ///      `Character not found`.
    ///
    /// So `chat_context_init_equivalence` cannot carry the arm (the work order
    /// predicted it would be RED-FIRST there; the prediction is refuted by
    /// measurement), and this is the P4.D112 idiom: port the semantics v4 wrote,
    /// pin them where they are reachable, and record why they are not.
    #[test]
    fn an_empty_content_default_does_not_fall_through() {
        let character = json!({
            "systemPrompts": [
                { "id": "alpha", "isDefault": false, "content": "the first prompt's content" },
                { "id": "beta", "isDefault": true, "content": "" },
            ],
        });
        // The resolved prompt IS beta…
        assert_eq!(
            resolve_default_system_prompt_id(&character).as_deref(),
            Some("beta")
        );
        // …and its empty content is the answer. The pre-fix chain
        // (`defaultPrompt?.content || systemPrompts[0]?.content || ''`) answered
        // "the first prompt's content" here.
        assert_eq!(resolve_default_system_prompt_content(&character), "");
    }

    /// The same `??` where the key is absent rather than empty.
    #[test]
    fn a_default_with_no_content_key_answers_the_empty_string() {
        let character = json!({
            "systemPrompts": [
                { "id": "alpha", "isDefault": true },
                { "id": "beta", "isDefault": false, "content": "not this" },
            ],
        });
        assert_eq!(resolve_default_system_prompt_content(&character), "");
    }
}
