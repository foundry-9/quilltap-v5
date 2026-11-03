/**
 * The v4-side recorder behind
 * `src/app/chat/system-message-labels.intervention.oracle.spec.ts` (P4.D346
 * item 4, ruling R-9).
 *
 * `839575d89` gives a Pascal roll the operator changed while it was held an
 * "adjusted" badge whose tooltip is `getInterventionSummary`
 * (`app/salon/[id]/components/system-message-labels.ts:210-257`): each changed
 * field as *dealt → final*, numbers to four significant digits, presentation
 * changes in words. This drives v4's REAL function over v4's own twelve test
 * cases (`system-message-labels.intervention.test.ts`, the fixture `pascal()`
 * transcribed from it) plus the edges a transcription could mis-spell.
 *
 * It ALSO records v4's REAL `describeSpan` (`lib/pascal/intervention/spans.ts`,
 * `8ae8a127c` — client-safe, imports types only), the phrasing the hold
 * dialog prints under the roll and the value. Rows carry `fn`.
 *
 * This file lives OUTSIDE `src/` on purpose: it imports v4's `@/app/...` and
 * `@/lib/...`. The bracketed path must be quoted in any shell that touches it
 * (memory note `v4-path-quoting-traps`); the import below is a string, so tsx
 * is unbothered.
 *
 * Each row: `{ fn: 'getInterventionSummary', id, message, out }` (`out` is
 * the string or null) or `{ fn: 'describeSpan', id, span, out }`.
 *
 * Run it from a pinned v4 worktree at `236c04e35` with its own `npm ci
 * --offline` (Node 24):
 *
 * ```bash
 * PIN=${PIN:?a detached v4 worktree at 236c04e35}
 * V5W=${V5W:-$HOME/source/quilltap-v5}
 * cp "$V5W/apps/web/oracle/intervention-summary.recorder.ts" "$PIN/"
 * OUT="$V5W/apps/web/src/testing/fixtures/intervention-summary.oracle.ndjson"
 * rm -f "$OUT"
 * (cd "$PIN" && PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH \
 *   npx tsx intervention-summary.recorder.ts > "$OUT")
 * wc -l "$OUT"                                         # expect the header count
 * grep -c 'Adjusted before it was dealt:' "$OUT"       # > 0
 * grep -c "the oracle's answer replaced" "$OUT"        # > 0
 * ```
 *
 * Expect 38 lines (24 `getInterventionSummary` rows + 14 `describeSpan` rows).
 */

import { getInterventionSummary } from '@/app/salon/[id]/components/system-message-labels';
import { describeSpan } from '@/lib/pascal/intervention/spans';

type Msg = Parameters<typeof getInterventionSummary>[0];
type Meta = NonNullable<Msg['pascalMeta']>;
type Span = Parameters<typeof describeSpan>[0];

const emit = (row: Record<string, unknown>) => console.log(JSON.stringify(row));

/** v4's own fixture (`system-message-labels.intervention.test.ts`), verbatim. */
function pascal(
  changed: string[],
  final: Partial<Meta> = {},
  dealt: Record<string, unknown> = {},
  sender: Msg['systemSender'] = 'pascal',
): Msg {
  return {
    systemSender: sender,
    pascalMeta: {
      tool: 'wager',
      definitionTier: 'global',
      definitionMountId: 'mount-1',
      params: { bonus: 4 },
      rollForm: 'range',
      raw: 0.8,
      value: 80,
      state: 'success',
      outcomeIndex: 1,
      invokedBy: 'llm',
      intervention: {
        resolvedAt: '2026-05-02T00:00:00.000Z',
        decision: 'deal',
        changed,
        dealt: {
          params: { bonus: 0 },
          raw: 0.123456,
          value: 12,
          outcomeIndex: 0,
          state: 'failure',
          message: 'dealt',
          visibility: 'public',
          announcer: 'pascal',
          ...dealt,
        },
      },
      ...final,
    } as Meta,
  };
}

const noRecord = pascal(['roll']);
delete (noRecord.pascalMeta as Meta).intervention;

const ROWS: Array<[string, Msg]> = [
  // v4's twelve cases, by name (the two-assertion first case as two rows).
  ['is null for a message with no pascalMeta at all (null)', { systemSender: 'pascal', pascalMeta: null }],
  ['is null for a message with no pascalMeta at all (absent)', { systemSender: 'pascal' }],
  ['is null when the roll was never held (no intervention record)', noRecord],
  ['is null when nothing changed', pascal([])],
  ['is null for a message not from Pascal, whatever it carries', pascal(['roll'], {}, {}, 'host')],
  ['lists a roll change as dealt → final, to four significant digits', pascal(['roll'])],
  ['lists dice faces when both records have them', pascal(['roll'], { diceRolls: [6, 6] }, { diceRolls: [1, 2] })],
  ['lists an outcome change by one-based row number', pascal(['outcome'])],
  ['lists parameter changes by name', pascal(['params.bonus'])],
  ['joins several changes in the order recorded', pascal(['roll', 'outcome', 'params.bonus'])],
  ['reads the other fields in words, never quoting a rewritten message', pascal(['value', 'state', 'message', 'llmAnswer', 'effects.1'], {}, { message: 'DEALT-TEXT' })],
  ['names presentation changes', pascal(['visibility', 'announcer', 'title', 'chipLabel'], { toolTitle: 'New', chipLabel: 'B' }, { toolTitle: 'Old', chipLabel: 'A' })],
  ['passes an unknown key through rather than hiding it', pascal(['something-new'])],
  // the edges
  ['a float value to four significant digits, an integer-valued float plain', pascal(['value'], { value: 0.123456 }, { value: 70.0 })],
  ['a roll where only the dealt side has dice falls to raw', pascal(['roll'], {}, { diceRolls: [1, 2] })],
  ['a roll where only the final side has dice falls to raw', pascal(['roll'], { diceRolls: [3, 4] })],
  ['chip label and title with absent dealt sides', pascal(['chipLabel', 'title'], { toolTitle: 'New', chipLabel: 'B' })],
  ['chip label and title absent on both sides', pascal(['chipLabel', 'title'])],
  ['visibility from a whisper', pascal(['visibility'], {}, { visibility: 'whisper' })],
  ['the announcer from the character', pascal(['announcer'], {}, { announcer: 'character' })],
  ['the fourth effect', pascal(['effects.3'])],
  ['a parameter whose final value is a boolean', pascal(['params.flag'], { params: { flag: true } }, { params: { flag: false } })],
  ['a string value with a quote', pascal(['params.say', 'chipLabel'], { params: { say: 'he said "no"' }, chipLabel: 'a "b"' }, { params: { say: "it's" }, chipLabel: 'x' })],
  ['an as-rolled record that names nothing changed', (() => {
    const m = pascal([]);
    (m.pascalMeta as Meta).intervention!.decision = 'as-rolled';
    return m;
  })()],
];
for (const [id, message] of ROWS) {
  emit({ fn: 'getInterventionSummary', id, message, out: getInterventionSummary(message) });
}

const SPANS: Array<[string, Span]> = [
  ['exactly', { min: 5, max: 5 }],
  ['closed', { min: 1, max: 20 }],
  ['half-open', { min: 0, max: 1, maxExclusive: true }],
  ['half-open hundred', { min: 0, max: 100, maxExclusive: true }],
  ['open below', { min: -42, max: -2, minExclusive: true }],
  ['open both', { min: 0, max: 10, minExclusive: true, maxExclusive: true }],
  ['equal but open above', { min: 3, max: 3, maxExclusive: true }],
  ['equal but open below', { min: 3, max: 3, minExclusive: true }],
  ['floats to four digits', { min: 0.123456, max: 9.87654 }],
  ['an integer-valued float', { min: 70.0, max: 140.0 }],
  ['negative closed', { min: -10, max: -1 }],
  ['large', { min: 1, max: 1e21 }],
  ['tiny float', { min: 0.000012345, max: 0.5, maxExclusive: true }],
  ['dice', { min: 5, max: 20 }],
];
for (const [id, span] of SPANS) {
  emit({ fn: 'describeSpan', id, span, out: describeSpan(span) });
}
