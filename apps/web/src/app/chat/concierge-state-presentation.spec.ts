import { describe, expect, it } from 'vitest';

import { CONCIERGE_STATES, type ConciergeState } from './concierge-state';
import {
  CONCIERGE_STATE_PRESENTATION,
  type ConciergeProvenanceNote,
  type ConciergeTone,
  conciergeToneSuffix,
  conciergeToneTextClass,
  describeConciergeState,
} from './concierge-state-presentation';
import V4 from './concierge-state-presentation.v4.json';

/**
 * The Concierge presentation table (v4
 * `lib/services/dangerous-content/concierge-state-presentation.ts` at
 * `ce2f1dabf`: three states since `4d370a90f`, the `info` tone retired at
 * `3b463d6b1`).
 *
 * Two layers, because the table is the single source for every word the
 * states wear and a copy edit on EITHER side must redden:
 *
 *  1. **The transcribed corpus** — v4's `__tests__/unit/lib/services/
 *     dangerous-content/concierge-state-presentation.test.ts`, 1:1 by name,
 *     including its `it.each` tables.
 *  2. **The executed-v4 oracle** — `concierge-state-presentation.v4.json`,
 *     emitted by RUNNING v4's real module at the round-target pin over every
 *     state × provenance note × category shape (144 `describe` rows).
 *
 * Regen recipe for the oracle (drift-ledger §5.1: reading through `git show` at
 * the pin makes it independent of the v4 working tree):
 *
 * ```bash
 * export PATH=~/.nvm/versions/node/v24.13.1/bin:$PATH
 * QT_V4_PIN=acadcc7cd node ~/source/quilltap-v5/harness/oracle/cases/concierge-presentation.mjs \
 *   > ~/source/quilltap-v5/apps/web/src/app/chat/concierge-state-presentation.v4.json
 * ```
 */

const ALL_STATES: readonly ConciergeState[] = CONCIERGE_STATES;
const ALL_TONES: ConciergeTone[] = ['danger', 'muted', 'success'];

// ---------------------------------------------------------------------------
// 1. v4's corpus, transcribed 1:1
// ---------------------------------------------------------------------------

describe('CONCIERGE_STATE_PRESENTATION', () => {
  it.each([
    ['moderated', 'Moderated', 'eye', 'success'],
    ['unmoderated', 'Unmoderated', 'eye-off', 'danger'],
    ['locked', 'Locked', 'shield', 'muted'],
  ])('describes %s as %s / %s / %s', (state, label, icon, tone) => {
    const presentation = CONCIERGE_STATE_PRESENTATION[state as ConciergeState];
    expect(presentation.label).toBe(label);
    expect(presentation.icon).toBe(icon);
    expect(presentation.tone).toBe(tone);
  });

  it('covers all three states, each with a detail sentence and the same hint', () => {
    expect(Object.keys(CONCIERGE_STATE_PRESENTATION).sort()).toEqual([...ALL_STATES].sort());
    for (const state of ALL_STATES) {
      expect(CONCIERGE_STATE_PRESENTATION[state].detail.length).toBeGreaterThan(0);
      expect(CONCIERGE_STATE_PRESENTATION[state].hint).toBe(
        "Change it from the Salon sidebar's Chat section.",
      );
    }
  });

  it('keeps the helper sentences verbatim', () => {
    expect(CONCIERGE_STATE_PRESENTATION.moderated.detail).toBe(
      'The Concierge sends everything to the usual providers first, and to the uncensored desk only when one of them refuses. After enough refusals he moves the whole chat himself.',
    );
    expect(CONCIERGE_STATE_PRESENTATION.unmoderated.detail).toBe(
      'You have opened the uncensored door yourself. Nothing here goes near a moderated provider.',
    );
    expect(CONCIERGE_STATE_PRESENTATION.locked.detail).toBe(
      'Only the usual providers, ever. If one refuses, the refusal stands. For the chat that must never reach an uncensored model.',
    );
  });

  it('gives every state a distinct label, icon and tone', () => {
    const labels = ALL_STATES.map((s) => CONCIERGE_STATE_PRESENTATION[s].label);
    const icons = ALL_STATES.map((s) => CONCIERGE_STATE_PRESENTATION[s].icon);
    const tones = ALL_STATES.map((s) => CONCIERGE_STATE_PRESENTATION[s].tone);
    expect(new Set(labels).size).toBe(3);
    expect(new Set(icons).size).toBe(3);
    expect(new Set(tones).size).toBe(3);
  });
});

describe('conciergeToneSuffix', () => {
  it('leaves the danger base rule unsuffixed and names the one modifier', () => {
    expect(conciergeToneSuffix('danger')).toBe('');
    expect(conciergeToneSuffix('muted')).toBe('-muted');
  });

  it('falls through to the base for success (Moderated draws no badge and no mark)', () => {
    expect(conciergeToneSuffix('success')).toBe('');
  });

  it.each([
    ['unmoderated', ''],
    ['locked', '-muted'],
  ])('gives %s the class suffix "%s"', (state, suffix) => {
    expect(conciergeToneSuffix(CONCIERGE_STATE_PRESENTATION[state as ConciergeState].tone)).toBe(
      suffix,
    );
  });
});

describe('conciergeToneTextClass', () => {
  it.each([
    ['moderated', 'qt-text-success'],
    ['unmoderated', 'qt-text-danger'],
    ['locked', 'qt-text-muted'],
  ])('gives %s the text class %s', (state, expected) => {
    expect(
      conciergeToneTextClass(CONCIERGE_STATE_PRESENTATION[state as ConciergeState].tone),
    ).toBe(expected);
  });
});

describe('describeConciergeState', () => {
  it.each([...ALL_STATES])('reads %s straight off the table with no provenance', (state) => {
    const presentation = CONCIERGE_STATE_PRESENTATION[state];
    expect(describeConciergeState(state)).toEqual({
      title: presentation.label,
      detail: presentation.detail,
      categories: null,
      hint: presentation.hint,
    });
  });

  it("uses the operator's sentence for Unmoderated set by the operator", () => {
    expect(
      describeConciergeState('unmoderated', { setBy: 'operator', reason: 'manual' }).detail,
    ).toBe(CONCIERGE_STATE_PRESENTATION.unmoderated.detail);
  });

  it('names the refusal count when the Concierge moved the chat after refusals', () => {
    expect(
      describeConciergeState('unmoderated', {
        setBy: 'concierge',
        reason: 'refusals',
        refusalCount: 2,
      }).detail,
    ).toBe(
      'The Concierge moved this chat to the uncensored desk after two refusals. Set it back to Moderated if you disagree.',
    );
    expect(
      describeConciergeState('unmoderated', {
        setBy: 'concierge',
        reason: 'refusals',
        refusalCount: 1,
      }).detail,
    ).toBe(
      'The Concierge moved this chat to the uncensored desk after one refusal. Set it back to Moderated if you disagree.',
    );
  });

  it('still reads sensibly when the refusal count is unknown', () => {
    expect(
      describeConciergeState('unmoderated', { setBy: 'concierge', reason: 'refusals' }).detail,
    ).toBe(
      'The Concierge moved this chat to the uncensored desk after the usual providers refused it. Set it back to Moderated if you disagree.',
    );
  });

  it("says the classifier's reading when the Concierge moved the chat on the conversation", () => {
    expect(
      describeConciergeState('unmoderated', { setBy: 'concierge', reason: 'classifier' }).detail,
    ).toBe(
      'The Concierge moved this chat to the uncensored desk on reading the conversation. Set it back to Moderated if you disagree.',
    );
  });

  it("surfaces categories only for the classifier's own move", () => {
    expect(
      describeConciergeState('unmoderated', { setBy: 'concierge', reason: 'classifier' }, [
        'NSFW',
        'Violence',
      ]).categories,
    ).toEqual(['NSFW', 'Violence']);
    expect(
      describeConciergeState('unmoderated', { setBy: 'concierge', reason: 'classifier' }, [])
        .categories,
    ).toBeNull();
    expect(
      describeConciergeState('unmoderated', { setBy: 'concierge', reason: 'refusals' }, ['NSFW'])
        .categories,
    ).toBeNull();
    expect(
      describeConciergeState('unmoderated', { setBy: 'operator' }, ['NSFW']).categories,
    ).toBeNull();
  });

  it.each(['moderated', 'locked'] as ConciergeState[])(
    'never surfaces the preserved categories or a Concierge sentence on %s',
    (state) => {
      const description = describeConciergeState(
        state,
        { setBy: 'concierge', reason: 'classifier' },
        ['NSFW'],
      );
      expect(description.categories).toBeNull();
      expect(description.detail).toBe(CONCIERGE_STATE_PRESENTATION[state].detail);
    },
  );
});

describe('the v4 signature move (provenance 2nd, categories 3rd)', () => {
  it('refuses, at compile time, a stale two-argument call that passes the categories second', () => {
    // The test builder typechecks specs, so each `@ts-expect-error` is a
    // compile-time assertion: were the provenance parameter loosened enough to
    // accept an array, the directive would be unused and the BUILD would fail.
    // @ts-expect-error — a string[] is not a ConciergeProvenanceNote.
    const stale = describeConciergeState('unmoderated', ['NSFW']);
    // Runtime: the array reads as "no setBy", so the operator's sentence.
    expect(stale.categories).toBeNull();
    const note: ConciergeProvenanceNote = { setBy: 'concierge', reason: 'classifier' };
    expect(describeConciergeState('unmoderated', note, ['NSFW']).categories).toEqual(['NSFW']);
  });
});

// ---------------------------------------------------------------------------
// 2. The executed-v4 oracle — a copy edit on EITHER side reddens
// ---------------------------------------------------------------------------

describe('the presentation table against v4’s own module (executed at acadcc7cd)', () => {
  it('was emitted from the round-target pin', () => {
    expect(V4._source.pin).toBe('acadcc7cd');
    expect(V4._source.file).toBe('lib/services/dangerous-content/concierge-state-presentation.ts');
  });

  it('carries the same three states and nothing more', () => {
    expect(Object.keys(CONCIERGE_STATE_PRESENTATION)).toEqual(Object.keys(V4.presentation));
  });

  it('carries the whole describe corpus', () => {
    // A truncated fixture would make the loop below vacuously green.
    expect(V4.describe).toHaveLength(144);
  });

  for (const state of ALL_STATES) {
    it(`matches v4 string for string on ${state}`, () => {
      expect(CONCIERGE_STATE_PRESENTATION[state]).toEqual(
        (V4.presentation as Record<string, unknown>)[state],
      );
    });
  }

  for (const tone of ALL_TONES) {
    it(`agrees with v4 on the ${tone} suffix and text class`, () => {
      expect(conciergeToneSuffix(tone)).toBe((V4.toneSuffix as Record<string, string>)[tone]);
      expect(conciergeToneTextClass(tone)).toBe(
        (V4.toneTextClass as Record<string, string>)[tone],
      );
    });
  }

  it('records no tone beyond the three', () => {
    expect(Object.keys(V4.toneSuffix).sort()).toEqual([...ALL_TONES].sort());
  });

  for (const [index, row] of V4.describe.entries()) {
    it(`#${index} describes ${row.state} / ${row.provenance} exactly as v4 does (categories=${JSON.stringify(row.dangerCategories)})`, () => {
      const note = (row.note ?? undefined) as ConciergeProvenanceNote | undefined;
      const categories = (row.dangerCategories ?? undefined) as string[] | undefined;
      expect(describeConciergeState(row.state as ConciergeState, note, categories)).toEqual(
        row.result,
      );
    });
  }
});
