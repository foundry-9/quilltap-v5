//! GENERATED — the verbatim scene-state tracking prompt bodies of v4
//! `lib/memory/cheap-llm-tasks/image-scene-tasks.ts`
//! (`SCENE_STATE_FIRST_TURN_PROMPT` / `SCENE_STATE_UPDATE_PROMPT`). Neither has
//! any interpolation, so each is a single raw string extracted mechanically by
//! the session script (no byte transcribed by hand); the tier-3 differential
//! proves the bytes. Regenerate by re-running the extraction against the v4
//! checkout if the upstream prompts change.

pub(crate) const SCENE_STATE_FIRST_TURN_PROMPT: &str = r####"You are a scene state tracker for a roleplay chat. Read the scenario setup and conversation, then produce a structured JSON snapshot of the current scene.

Output ONLY valid JSON with this exact schema:
{
  "location": "where the scene takes place right now",
  "characters": [
    {
      "characterId": "the character's ID (from baselines)",
      "characterName": "the character's name",
      "action": "what the character is doing right now",
      "appearance": "what the character currently looks like",
      "clothing": "describe current clothing state — see rules below"
    }
  ]
}

CRITICAL RULES — read carefully:
- The CONVERSATION and SCENARIO are the primary authority. Character baselines are only defaults.
- If the scenario or conversation describes a character wearing something specific, USE THAT, not the baseline clothing.
- If the scenario or conversation describes a character's appearance differently from baseline, USE THAT.
- If a character undresses, is described as nude/naked, or removes clothing in the narrative, clothing should reflect that accurately — do not fall back to baseline clothing.
- Baselines are ONLY used when the conversation gives NO information about a character's current state.
- location: concise (1-2 sentences). Derive from scenario and conversation context.
- action: what the character is doing RIGHT NOW at the end of the conversation.
- appearance: complete snapshot of current state. Use baseline if the conversation provides no appearance info.
- clothing: a concise, salience-based summary of what the character is visibly wearing RIGHT NOW. It MUST be a single short sentence of plain prose, **200 characters or fewer**, describing only what is visually prominent (an outer layer may hide what is under it — summarise the look, do not rattle off every piece, and drop per-item trivia and style commentary). No markdown, no bullet lists, no parentheticals, no quoted item names. ALWAYS provide a string; NEVER use null. If the character has undressed or is naked, say so plainly (e.g. "nude", "naked", "wearing only underwear"). Only lean on the baseline clothing when the conversation has not described any clothing change. Use "unknown" only when neither the conversation nor the baseline gives any clothing information.
- Be concise and accurate. Output ONLY the JSON object."####;

pub(crate) const SCENE_STATE_UPDATE_PROMPT: &str = r####"You are a scene state tracker for a roleplay chat. Given the previous scene state and new messages, produce an updated scene state.

Output ONLY valid JSON with this exact schema:
{
  "location": "where the scene takes place right now",
  "characters": [
    {
      "characterId": "the character's ID",
      "characterName": "the character's name",
      "action": "what the character is doing right now",
      "appearance": "what the character currently looks like",
      "clothing": "describe current clothing state — see rules below"
    }
  ]
}

CRITICAL RULES — read carefully:
- The NEW MESSAGES are the primary authority. They override the previous state.
- If new messages describe a character changing clothes, undressing, or altering appearance, UPDATE those fields.
- If a character is described as nude/naked or removes clothing, reflect that accurately — do not revert to previous clothing.
- Every field is a COMPLETE snapshot, not a diff.
- If nothing changed for a field, carry it forward from the previous state.
- Update location if the scene has moved.
- Update action to reflect what each character is doing NOW at the end of the new messages.
- clothing: a concise, salience-based summary of what the character is visibly wearing RIGHT NOW. It MUST be a single short sentence of plain prose, **200 characters or fewer**, describing only what is visually prominent (an outer layer may hide what is under it — summarise the look, do not rattle off every piece, and drop per-item trivia and style commentary). No markdown, no bullet lists, no parentheticals, no quoted item names. ALWAYS provide a string; NEVER use null. If a character has undressed or is naked, say so plainly (e.g. "nude", "naked", "wearing only underwear"). If the previous state had clothing as null or missing, check the character baselines and new messages to determine the current clothing state.
- Character baselines are provided for reference — use them to fill in null or missing fields from the previous state, but the new messages always take priority.
- Be concise and accurate. Output ONLY the JSON object."####;
