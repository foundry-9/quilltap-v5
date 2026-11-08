import { describe, expect, it } from 'vitest';

import { isHandUp, type RaisedHandStatus } from './raised-hand';

/** One status per kind, so a new kind fails to type-check here until it is covered. */
const SAMPLES = {
  raised: { kind: 'raised' },
  asked: { kind: 'asked', characterName: 'Abigail' },
  yielded: { kind: 'yielded', characterName: 'Abigail' },
  declined: { kind: 'declined', characterName: 'Abigail' },
  unasked: { kind: 'unasked' },
} satisfies Record<RaisedHandStatus['kind'], RaisedHandStatus>;

const EXPECTED = {
  raised: true,
  asked: true,
  yielded: false,
  declined: false,
  unasked: false,
} satisfies Record<RaisedHandStatus['kind'], boolean>;

describe('isHandUp (v4 useSSEStreaming.ts:242-244)', () => {
  it.each(Object.keys(SAMPLES) as RaisedHandStatus['kind'][])('%s', (kind) => {
    expect(isHandUp(SAMPLES[kind])).toBe(EXPECTED[kind]);
  });

  it('is false with no status at all', () => {
    expect(isHandUp(null)).toBe(false);
  });
});
