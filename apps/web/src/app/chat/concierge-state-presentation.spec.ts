import { describe, expect, it } from 'vitest';

import type { ConciergeState } from './concierge-state';
import {
  CONCIERGE_STATE_PRESENTATION,
  type ConciergeTone,
  conciergeToneSuffix,
  conciergeToneTextClass,
  describeConciergeState,
} from './concierge-state-presentation';
import V4 from './concierge-state-presentation.v4.json';

/**
 * The Concierge presentation table (v4
 * `lib/services/dangerous-content/concierge-state-presentation.ts`, new at
 * `c43d3b1b4`).
 *
 * Two layers, because the table is the single source for every word the four
 * states wear and a copy edit on EITHER side must redden:
 *
 *  1. **The transcribed corpus** — v4's `__tests__/unit/lib/services/
 *     dangerous-content/concierge-state-presentation.test.ts`, 1:1, including
 *     its `it.each` tables (the client-parity precedent of P4.D132 / P4.D86).
 *  2. **The executed-v4 oracle** — `concierge-state-presentation.v4.json`,
 *     emitted by RUNNING v4's real module (its imports are all `import type`,
 *     which Node 24's type stripping erases, so it needs no bundler). Every row
 *     of v5's table, both tone functions over all four tones, and all twelve
 *     `describeConciergeState` shapes are diffed against v4's own bytes.
 *
 * Regen recipe for the oracle (drift-ledger §5.1: reading through `git show` at
 * the pin makes it independent of the v4 working tree):
 *
 * ```bash
 * export PATH=~/.nvm/versions/node/v24.13.1/bin:$PATH
 * node ~/source/quilltap-v5/harness/oracle/cases/concierge-presentation.mjs \
 *   > ~/source/quilltap-v5/apps/web/src/app/chat/concierge-state-presentation.v4.json
 * ```
 */

const ALL_STATES: ConciergeState[] = ['monitored', 'flagged', 'vouched', 'uncensored'];
const ALL_TONES: ConciergeTone[] = ['danger', 'muted', 'info', 'success'];

// ---------------------------------------------------------------------------
// 1. v4's corpus, transcribed 1:1
// ---------------------------------------------------------------------------

describe('CONCIERGE_STATE_PRESENTATION', () => {
  const ROWS: Array<[string, string, string, string]> = [
    ['monitored', 'Monitored', 'eye', 'success'],
    ['flagged', 'Flagged', 'alert-triangle', 'danger'],
    ['vouched', 'Vouched Safe', 'check-circle', 'muted'],
    ['uncensored', 'Uncensored', 'eye-off', 'info'],
  ];

  for (const [state, label, icon, tone] of ROWS) {
    it(`describes ${state} as ${label} / ${icon} / ${tone}`, () => {
      const presentation = CONCIERGE_STATE_PRESENTATION[state as ConciergeState];
      expect(presentation.label).toBe(label);
      expect(presentation.icon).toBe(icon);
      expect(presentation.tone).toBe(tone);
    });
  }

  it('covers all four states, each with a detail sentence and the same hint', () => {
    expect(Object.keys(CONCIERGE_STATE_PRESENTATION).sort()).toEqual([...ALL_STATES].sort());
    for (const state of ALL_STATES) {
      expect(CONCIERGE_STATE_PRESENTATION[state].detail.length).toBeGreaterThan(0);
      expect(CONCIERGE_STATE_PRESENTATION[state].hint).toBe(
        "Change it from the Salon sidebar's Chat section.",
      );
    }
  });

  it('keeps the sidebar helper sentences verbatim', () => {
    expect(CONCIERGE_STATE_PRESENTATION.monitored.detail).toBe(
      'The Concierge keeps watch, and will flip the switch himself if the conversation calls for it.',
    );
    expect(CONCIERGE_STATE_PRESENTATION.flagged.detail).toBe(
      'The Concierge has this chat down as dangerous, and routes it through the uncensored providers.',
    );
    expect(CONCIERGE_STATE_PRESENTATION.vouched.detail).toBe(
      'You have vouched for this chat. The Concierge stops watching; the ordinary providers still apply, and may still refuse.',
    );
    expect(CONCIERGE_STATE_PRESENTATION.uncensored.detail).toBe(
      'You have sent the Concierge away and opened the uncensored door yourself. Nothing is scanned, nothing is softened — the risk is yours.',
    );
  });

  it('gives every state a distinct label and icon', () => {
    const labels = ALL_STATES.map((s) => CONCIERGE_STATE_PRESENTATION[s].label);
    const icons = ALL_STATES.map((s) => CONCIERGE_STATE_PRESENTATION[s].icon);
    expect(new Set(labels).size).toBe(4);
    expect(new Set(icons).size).toBe(4);
  });
});

describe('conciergeToneSuffix', () => {
  it('leaves the danger base rule unsuffixed and names the two modifiers', () => {
    expect(conciergeToneSuffix('danger')).toBe('');
    expect(conciergeToneSuffix('muted')).toBe('-muted');
    expect(conciergeToneSuffix('info')).toBe('-info');
  });

  it('falls through to the base for success (Monitored draws no badge and no mark)', () => {
    expect(conciergeToneSuffix('success')).toBe('');
  });

  for (const [state, suffix] of [
    ['flagged', ''],
    ['vouched', '-muted'],
    ['uncensored', '-info'],
  ] as const) {
    it(`gives ${state} the class suffix "${suffix}"`, () => {
      expect(conciergeToneSuffix(CONCIERGE_STATE_PRESENTATION[state].tone)).toBe(suffix);
    });
  }
});

describe('conciergeToneTextClass', () => {
  for (const [state, expected] of [
    ['monitored', 'qt-text-success'],
    ['flagged', 'qt-text-danger'],
    ['vouched', 'qt-text-muted'],
    ['uncensored', 'qt-text-info'],
  ] as const) {
    it(`gives ${state} the text class ${expected}`, () => {
      expect(conciergeToneTextClass(CONCIERGE_STATE_PRESENTATION[state].tone)).toBe(expected);
    });
  }
});

describe('describeConciergeState', () => {
  for (const state of ALL_STATES) {
    it(`reads ${state} straight off the table`, () => {
      const presentation = CONCIERGE_STATE_PRESENTATION[state];
      expect(describeConciergeState(state)).toEqual({
        title: presentation.label,
        detail: presentation.detail,
        categories: null,
        hint: presentation.hint,
      });
    });
  }

  it('surfaces categories for Flagged when the chat carries any', () => {
    expect(describeConciergeState('flagged', ['NSFW', 'Violence']).categories).toEqual([
      'NSFW',
      'Violence',
    ]);
  });

  it('omits an empty category list even for Flagged', () => {
    expect(describeConciergeState('flagged', []).categories).toBeNull();
    expect(describeConciergeState('flagged', undefined).categories).toBeNull();
  });

  for (const state of ['monitored', 'vouched', 'uncensored'] as ConciergeState[]) {
    it(`never surfaces the preserved categories on ${state}`, () => {
      // The categories are the classifier's reasons; on the two operator states
      // (and on Monitored) they are a stale artefact, not a live verdict.
      expect(describeConciergeState(state, ['NSFW']).categories).toBeNull();
    });
  }
});

// ---------------------------------------------------------------------------
// 2. The executed-v4 oracle — a copy edit on EITHER side reddens
// ---------------------------------------------------------------------------

describe('the presentation table against v4’s own module (executed at c43d3b1b4)', () => {
  it('was emitted from the pin this lane ports', () => {
    expect(V4._source.pin).toBe('c43d3b1b4');
    expect(V4._source.file).toBe(
      'lib/services/dangerous-content/concierge-state-presentation.ts',
    );
  });

  it('carries the same four states and nothing more', () => {
    expect(Object.keys(CONCIERGE_STATE_PRESENTATION)).toEqual(Object.keys(V4.presentation));
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

  for (const [index, row] of V4.describe.entries()) {
    it(`describes ${row.state} exactly as v4 does (categories=${JSON.stringify(row.dangerCategories)})`, () => {
      expect(
        describeConciergeState(row.state as ConciergeState, row.dangerCategories ?? undefined),
      ).toEqual(V4.describe[index].result);
    });
  }
});
