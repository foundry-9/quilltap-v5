import { describe, expect, it } from 'vitest';

import {
  dismissedHandStatus,
  handStatusFromFloor,
  isHandUp,
  lowerIfOnlyRaised,
  settleHandAtStreamEnd,
  type RaisedHandStatus,
} from './raised-hand-status';

/** Every status, once — the machine's whole domain (plus `null`, no hand at all). */
const ALL: Array<RaisedHandStatus | null> = [
  null,
  { kind: 'raised' },
  { kind: 'asked', characterName: 'Friday' },
  { kind: 'yielded', characterName: 'Friday' },
  { kind: 'declined', characterName: 'Friday' },
  { kind: 'unasked' },
];

describe('raised-hand status machine (v4 9eab8d2da)', () => {
  it('the LANE-REMOVE stand-in carries exactly v4’s five kinds (a drifted stand-in fails here)', () => {
    // If P4.D385's `chat/raised-hand.ts` type drifts from this one, the union
    // swap makes this list stop type-checking or stop matching.
    const kinds: Array<RaisedHandStatus['kind']> = ['raised', 'asked', 'yielded', 'declined', 'unasked'];
    expect(ALL.filter((s) => s !== null).map((s) => s!.kind)).toEqual(kinds);
  });

  it.each(ALL.map((s) => [s?.kind ?? 'null', s] as const))('isHandUp(%s)', (kind, s) => {
    expect(isHandUp(s)).toBe(kind === 'raised' || kind === 'asked');
  });

  it.each([
    ['null', null, null],
    ['raised', { kind: 'raised' }, { kind: 'unasked' }],
    ['asked', { kind: 'asked', characterName: 'Friday' }, null],
    ['yielded', { kind: 'yielded', characterName: 'Friday' }, { kind: 'yielded', characterName: 'Friday' }],
    ['declined', { kind: 'declined', characterName: 'Friday' }, { kind: 'declined', characterName: 'Friday' }],
    ['unasked', { kind: 'unasked' }, { kind: 'unasked' }],
  ] as const)('settleHandAtStreamEnd(%s)', (_k, before, after) => {
    expect(settleHandAtStreamEnd(before as RaisedHandStatus | null)).toEqual(after);
  });

  it.each(ALL.map((s) => [s?.kind ?? 'null', s] as const))('dismissedHandStatus(%s) keeps only an up hand', (_k, s) => {
    expect(dismissedHandStatus(s)).toEqual(isHandUp(s) ? s : null);
  });

  it.each(ALL.map((s) => [s?.kind ?? 'null', s] as const))('lowerIfOnlyRaised(%s) lowers only `raised`', (_k, s) => {
    expect(lowerIfOnlyRaised(s)).toEqual(s?.kind === 'raised' ? null : s);
  });

  it('handStatusFromFloor: asked / yielded / declined, with v4’s two fallbacks', () => {
    expect(handStatusFromFloor({ kind: 'handAsked', characterName: 'Bo' })).toEqual({ kind: 'asked', characterName: 'Bo' });
    expect(handStatusFromFloor({ kind: 'handAsked', characterName: '' })).toEqual({
      kind: 'asked',
      characterName: 'The next speaker',
    });
    expect(handStatusFromFloor({ kind: 'handAnswered', characterName: 'Bo', yielded: true })).toEqual({
      kind: 'yielded',
      characterName: 'Bo',
    });
    expect(handStatusFromFloor({ kind: 'handAnswered', characterName: '', yielded: false })).toEqual({
      kind: 'declined',
      characterName: 'The speaker',
    });
  });
});
