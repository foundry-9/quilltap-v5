/**
 * P4.D97 unit 1 — the thinking-turn evaluator (v4 `97d2fcb5`).
 *
 * Drives v4's REAL module:
 *   evaluateThinkingTurn (lib/llm/thinking-turn.ts)
 *
 * The corpus is a full product: rules × parameter shapes × model facts. What
 * it pins beyond the obvious:
 *
 *   - `disabledValues` is checked BEFORE `enabledValues` — a value both lists
 *     claim answers `false` (the `both-lists` rule rows);
 *   - `isUnset` treats the EMPTY STRING like absent/null — it is how the
 *     options schema spells "(model default)", so it falls through to the
 *     model habit rather than matching nothing-and-answering-false;
 *   - a set-but-unmatched value ALSO falls through to `thinksByDefault`
 *     (the `case-shifted` / `unmatched` rows);
 *   - the value test is JS `===`: `'1'` never matches `1`, `true` never
 *     matches `1`, and casing matters;
 *   - `supportsThinking` alone is capability, not habit — it never turns the
 *     answer on.
 *
 * Run from inside the pinned v4 worktree:
 *   cd /tmp/qt-v4-pin-p4d97-12fe3e6f
 *   ~/.nvm/versions/node/v24.13.1/bin/npx tsx \
 *     ~/source/quilltap-v5/harness/oracle/cases/thinking-turn.ts \
 *     > /tmp/oracle-thinking-turn.ndjson
 */

import { evaluateThinkingTurn, type ThinkingTurnRule } from '@/lib/llm/thinking-turn';

const rows: unknown[] = [];

/**
 * Rules, including the two real plugin declarations at `12fe3e6f`, a rule
 * claiming a value in BOTH lists (the order pin), one-sided rules, and a
 * numeric rule. `null` and `absent` are distinct inputs to the same falsy
 * branch.
 */
const RULES: Array<[string, ThinkingTurnRule | null | undefined]> = [
  [
    'deepseek',
    { optionKey: 'thinking', enabledValues: ['enabled'], disabledValues: ['disabled'] },
  ],
  [
    'ollama',
    { optionKey: 'enable_thinking', enabledValues: [true], disabledValues: [false] },
  ],
  ['both-lists', { optionKey: 'thinking', enabledValues: ['both'], disabledValues: ['both'] }],
  ['enabled-only', { optionKey: 'thinking', enabledValues: ['on'] }],
  ['disabled-only', { optionKey: 'thinking', disabledValues: ['off'] }],
  ['numeric', { optionKey: 'thinking', enabledValues: [1], disabledValues: [0] }],
  // The empty string IN a list is the only shape that can tell "'' is unset"
  // from "'' is set but matches nothing" — both fall through to the model
  // habit otherwise. `isUnset` must win: the row with `thinking: ''` skips
  // this list and answers the habit, never the list's `false`.
  ['empty-in-disabled', { optionKey: 'thinking', enabledValues: ['on'], disabledValues: [''] }],
  // P4.D101 (v4 `d5830439`): NanoGPT's real declaration, and the corpus's
  // FIRST multi-value enabled list — every other rule here carries exactly one
  // enabled value. It is also the shape where the empty string is in NEITHER
  // list, which is what makes "(model default)" fall through to the model's
  // own `thinksByDefault` habit (the `:thinking` catalogue entries).
  [
    'nanogpt',
    {
      optionKey: 'reasoning_effort',
      enabledValues: ['minimal', 'low', 'medium', 'high', 'xhigh'],
      disabledValues: ['none'],
    },
  ],
  ['null', null],
  ['absent', undefined],
];

/**
 * Parameter bags. Every rule above (deliberately, except ollama) reads the
 * key `thinking`, so the value shapes hit each rule's own lists; the ollama
 * rows exercise the whole grid against a bag that never carries its key plus
 * its own boolean rows below.
 */
const PARAMETERS: Array<[string, Record<string, unknown> | null | undefined]> = [
  ['absent', undefined],
  ['null', null],
  ['empty-bag', {}],
  ['empty-string', { thinking: '' }],
  ['value-null', { thinking: null }],
  ['enabled', { thinking: 'enabled' }],
  ['disabled', { thinking: 'disabled' }],
  ['both', { thinking: 'both' }],
  ['on', { thinking: 'on' }],
  ['off', { thinking: 'off' }],
  ['case-shifted', { thinking: 'ENABLED' }],
  ['unmatched', { thinking: 'sideways' }],
  ['bool-true', { thinking: true }],
  ['bool-false', { thinking: false }],
  ['number-1', { thinking: 1 }],
  ['number-0', { thinking: 0 }],
  ['string-1', { thinking: '1' }],
  ['unrelated-key', { reasoning_effort: 'high' }],
  // P4.D101 — the reasoning_effort scale against NanoGPT's rule: the LAST
  // enabled value (a multi-value list must not stop at the first), the
  // disabled value, the blank that belongs to neither list, and a value the
  // editor cannot store but a hand-edited profile could.
  ['effort-xhigh', { reasoning_effort: 'xhigh' }],
  ['effort-minimal', { reasoning_effort: 'minimal' }],
  ['effort-none', { reasoning_effort: 'none' }],
  ['effort-blank', { reasoning_effort: '' }],
  ['effort-unknown', { reasoning_effort: 'ludicrous' }],
  ['ollama-true', { enable_thinking: true }],
  ['ollama-false', { enable_thinking: false }],
  ['ollama-string-true', { enable_thinking: 'true' }],
];

/** Model facts: habit on/off, capability-only, and the two falsy shapes. */
const MODELS: Array<
  [string, { supportsThinking?: boolean; thinksByDefault?: boolean } | null | undefined]
> = [
  ['absent', undefined],
  ['null', null],
  ['thinks', { supportsThinking: true, thinksByDefault: true }],
  ['thinks-only', { thinksByDefault: true }],
  ['thinks-false', { supportsThinking: true, thinksByDefault: false }],
  ['supports-only', { supportsThinking: true }],
];

for (const [rid, rule] of RULES) {
  for (const [pid, parameters] of PARAMETERS) {
    for (const [mid, model] of MODELS) {
      rows.push({
        id: `${rid}/${pid}/${mid}`,
        rule: rule === undefined ? '<absent>' : rule,
        parameters: parameters === undefined ? '<absent>' : parameters,
        model: model === undefined ? '<absent>' : model,
        out: evaluateThinkingTurn({ rule, parameters, model }),
      });
    }
  }
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
