/**
 * Oracle case: the moderation finish-reason recogniser (P4.D106; v4 `a14a1811`
 * bug 93, `lib/llm/moderation-finish-reason.ts`).
 *
 * Drives v4's REAL `isModerationFinishReason` + `describeModerationRefusal`
 * over a fixed corpus: every literal in the set (all ten, incl. the hyphenated
 * `content-filter` the v4 docblock doesn't call out), case/whitespace variants
 * (incl. JS-only whitespace — NBSP, U+2028 — to pin the `.trim()` twin),
 * ordinary stops, the no-substring-guessing traps, null/empty, and the
 * describe template with assorted provider/model pairs.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/moderation-finish-reason.ts \
 *     > /tmp/oracle-moderation-finish-reason.ndjson
 */

import {
  isModerationFinishReason,
  describeModerationRefusal,
} from '@/lib/llm/moderation-finish-reason';
import { getEmptyResponseReason } from '@/lib/services/chat-message/provider-failover.service';

interface Case {
  label: string;
  reason: string | null;
  provider: string;
  modelName: string;
}

const CASES: Case[] = [
  // The ten literals, verbatim.
  { label: 'lit_sensitive', reason: 'sensitive', provider: 'Z_AI', modelName: 'glm-5v-turbo' },
  { label: 'lit_content_filter', reason: 'content_filter', provider: 'OPENAI', modelName: 'gpt-5' },
  { label: 'lit_content_filter_hyphen', reason: 'content-filter', provider: 'OPENAI', modelName: 'gpt-5' },
  { label: 'lit_refusal', reason: 'refusal', provider: 'OPENAI', modelName: 'gpt-5' },
  { label: 'lit_safety', reason: 'safety', provider: 'GOOGLE', modelName: 'gemini-3-pro' },
  { label: 'lit_prohibited_content', reason: 'prohibited_content', provider: 'GOOGLE', modelName: 'gemini-3-pro' },
  { label: 'lit_blocklist', reason: 'blocklist', provider: 'GOOGLE', modelName: 'gemini-3-pro' },
  { label: 'lit_spii', reason: 'spii', provider: 'GOOGLE', modelName: 'gemini-3-pro' },
  { label: 'lit_image_safety', reason: 'image_safety', provider: 'GOOGLE', modelName: 'gemini-3-flash' },
  { label: 'lit_recitation', reason: 'recitation', provider: 'GOOGLE', modelName: 'gemini-3-pro' },
  // Case variants (Google reports its five in UPPERCASE).
  { label: 'upper_safety', reason: 'SAFETY', provider: 'GOOGLE', modelName: 'gemini-3-pro' },
  { label: 'upper_prohibited', reason: 'PROHIBITED_CONTENT', provider: 'GOOGLE', modelName: 'gemini-3-pro' },
  { label: 'upper_recitation', reason: 'RECITATION', provider: 'GOOGLE', modelName: 'gemini-3-pro' },
  { label: 'mixed_sensitive', reason: 'Sensitive', provider: 'Z_AI', modelName: 'glm-5v-turbo' },
  { label: 'mixed_content_filter_hyphen', reason: 'Content-Filter', provider: 'AZURE', modelName: 'gpt-5' },
  // Whitespace variants — the describe template interpolates the RAW reason,
  // so a padded recognised reason keeps its padding in the sentence.
  { label: 'ws_padded', reason: '  Sensitive  ', provider: 'Z_AI', modelName: 'glm-5v-turbo' },
  { label: 'ws_tab_newline', reason: '\tcontent_filter\n', provider: 'OPENAI', modelName: 'gpt-5' },
  { label: 'ws_nbsp', reason: '\u00a0refusal\u00a0', provider: 'OPENAI', modelName: 'o4' },
  { label: 'ws_line_sep', reason: '\u2028safety\u2028', provider: 'GOOGLE', modelName: 'gemini-3-pro' },
  { label: 'ws_only', reason: '   ', provider: 'OPENAI', modelName: 'gpt-5' },
  // Ordinary stops stay unrecognised.
  { label: 'stop', reason: 'stop', provider: 'OPENAI', modelName: 'gpt-5' },
  { label: 'length', reason: 'length', provider: 'OPENAI', modelName: 'gpt-5' },
  { label: 'tool_calls', reason: 'tool_calls', provider: 'OPENAI', modelName: 'gpt-5' },
  { label: 'end_turn', reason: 'end_turn', provider: 'ANTHROPIC', modelName: 'claude-sonnet-5' },
  { label: 'completed', reason: 'completed', provider: 'OPENAI', modelName: 'gpt-5' },
  { label: 'max_tokens_stop', reason: 'MAX_TOKENS', provider: 'GOOGLE', modelName: 'gemini-3-pro' },
  // No substring guessing.
  { label: 'trap_insensitive_stop', reason: 'insensitive_stop', provider: 'OPENAI', modelName: 'gpt-5' },
  { label: 'trap_no_content_filter', reason: 'no_content_filter_applied', provider: 'OPENAI', modelName: 'gpt-5' },
  { label: 'trap_safety_prefix', reason: 'safety_check_passed', provider: 'GOOGLE', modelName: 'gemini-3-pro' },
  { label: 'trap_internal_space', reason: 'content filter', provider: 'OPENAI', modelName: 'gpt-5' },
  { label: 'trap_embedded', reason: 'xsensitivex', provider: 'Z_AI', modelName: 'glm-5v-turbo' },
  // Null / empty.
  { label: 'null', reason: null, provider: 'OPENAI', modelName: 'gpt-5' },
  { label: 'empty', reason: '', provider: 'OPENAI', modelName: 'gpt-5' },
  // Unicode case folding: JS `'İ'.toLowerCase()` is `'i' + U+0307`, which does
  // NOT equal `'i'` — the dotted-capital spelling stays unrecognised on both
  // sides (Rust `str::to_lowercase` proven byte-identical to JS in Phase 1).
  { label: 'unicode_dotted_capital', reason: 'SENSİTİVE', provider: 'Z_AI', modelName: 'glm-5v-turbo' },
  // The describe template with an empty provider/model (callers default them,
  // but the module itself interpolates whatever it is handed).
  { label: 'empty_provider_model', reason: 'sensitive', provider: '', modelName: '' },
];

for (const c of CASES) {
  const isModeration = isModerationFinishReason(c.reason);
  const description = describeModerationRefusal(c.reason, c.provider, c.modelName);
  process.stdout.write(
    JSON.stringify({
      kind: 'reason',
      label: c.label,
      reason: c.reason,
      provider: c.provider,
      modelName: c.modelName,
      isModeration,
      description,
    }) + '\n',
  );
}

// ---------------------------------------------------------------------------
// The `getEmptyResponseReason` wiring (provider-failover.service.ts): the
// moderation first branch must beat every pre-existing sentence, the
// uncensored-retry suffix must append, and the five pre-existing sentences
// must be byte-unchanged for a non-moderation stop. `finishReason` absent vs
// null are the same JS falsy arm; `provider`/`modelName` default to
// 'The provider'/'model' inside the branch.
// ---------------------------------------------------------------------------

interface EmptyCase {
  label: string;
  uncensored: boolean;
  same: boolean;
  flagged: boolean;
  finishReason?: string | null;
  provider?: string;
  modelName?: string;
}

const EMPTY_CASES: EmptyCase[] = [
  { label: 'mod_plain', uncensored: false, same: false, flagged: false, finishReason: 'sensitive', provider: 'Z_AI', modelName: 'glm-5v-turbo' },
  { label: 'mod_uncensored_suffix', uncensored: true, same: false, flagged: false, finishReason: 'sensitive', provider: 'Z_AI', modelName: 'glm-5v-turbo' },
  { label: 'mod_beats_both_empty', uncensored: true, same: true, flagged: false, finishReason: 'content_filter', provider: 'OPENAI', modelName: 'gpt-5' },
  { label: 'mod_beats_twice', uncensored: false, same: true, flagged: false, finishReason: 'sensitive', provider: 'Z_AI', modelName: 'glm-5v-turbo' },
  { label: 'mod_beats_concierge', uncensored: false, same: false, flagged: true, finishReason: 'SAFETY', provider: 'GOOGLE', modelName: 'gemini-3-pro' },
  { label: 'mod_defaults', uncensored: false, same: false, flagged: false, finishReason: 'SAFETY' },
  { label: 'mod_padded_raw', uncensored: false, same: false, flagged: false, finishReason: ' Sensitive ', provider: 'Z_AI', modelName: 'glm-5v-turbo' },
  { label: 'plain_default', uncensored: false, same: false, flagged: false, finishReason: 'stop', provider: 'OPENAI', modelName: 'gpt-5' },
  { label: 'plain_both_filtered', uncensored: true, same: true, flagged: false, finishReason: 'stop', provider: 'OPENAI', modelName: 'gpt-5' },
  { label: 'plain_uncensored', uncensored: true, same: false, flagged: false, finishReason: null },
  { label: 'plain_twice', uncensored: false, same: true, flagged: false },
  { label: 'plain_concierge', uncensored: false, same: false, flagged: true, finishReason: 'length', provider: 'GOOGLE', modelName: 'gemini-3-pro' },
];

for (const c of EMPTY_CASES) {
  const reason = getEmptyResponseReason({
    uncensoredRetryAttempted: c.uncensored,
    sameProviderRetryAttempted: c.same,
    contentWasFlaggedDangerous: c.flagged,
    finishReason: c.finishReason,
    provider: c.provider,
    modelName: c.modelName,
  });
  process.stdout.write(
    JSON.stringify({
      kind: 'empty',
      label: c.label,
      uncensored: c.uncensored,
      same: c.same,
      flagged: c.flagged,
      finishReason: c.finishReason ?? null,
      provider: c.provider ?? null,
      modelName: c.modelName ?? null,
      result: reason,
    }) + '\n',
  );
}
