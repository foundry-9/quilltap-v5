import { describe, expect, it } from 'vitest';

import {
  ACTIVITY_CHIPS,
  ACTIVITY_KINDS,
  blippedKinds,
  coerceCounts,
  emptyActivityCounts,
  hasActivity,
} from './activity-kinds';

/**
 * Transcription parity against v4 `lib/background-jobs/activity-kinds.ts` at
 * `664cfca84` — the chip metadata table, the defensive count coercion, and the
 * pulse rule the chips read `startedByKind` through.
 */

describe('ACTIVITY_CHIPS (v4\'s table, verbatim)', () => {
  it('is the five chips in render order, with v4\'s labels', () => {
    expect(ACTIVITY_CHIPS.map((c) => c.kind)).toEqual([
      'memory',
      'embedding',
      'summary',
      'danger',
      'image',
    ]);
    expect(ACTIVITY_CHIPS.map((c) => c.label)).toEqual(['Mem', 'Emb', 'Sum', 'Dgr', 'Img']);
  });

  it('carries v4\'s NEW titles, byte for byte', () => {
    expect(ACTIVITY_CHIPS.map((c) => c.title)).toEqual([
      'Memory work (extraction, regeneration, housekeeping)',
      'Embedding work (indexing, refits, live query embeddings)',
      'Summarization and post-turn processing (summaries, titles, scene state, rendering)',
      'the Concierge classification (per-message and chat-level)',
      'Image work, end to end (prompt crafting, generation, landing)',
    ]);
  });

  it('keeps the `image` → `qt-queue-badge-story` class quirk', () => {
    expect(ACTIVITY_CHIPS.map((c) => c.badgeClass)).toEqual([
      'qt-queue-badge-memory',
      'qt-queue-badge-embedding',
      'qt-queue-badge-summary',
      'qt-queue-badge-danger',
      'qt-queue-badge-story',
    ]);
  });

  it('renders one chip per kind, in the §A.2 key order', () => {
    expect(ACTIVITY_KINDS).toEqual(['memory', 'embedding', 'summary', 'danger', 'image']);
    expect(ACTIVITY_CHIPS.map((c) => c.kind)).toEqual([...ACTIVITY_KINDS]);
  });
});

describe('coerceCounts / hasActivity (v4\'s defensive read)', () => {
  it('zeroes every kind for an absent or wrong-shaped map', () => {
    const zeros = emptyActivityCounts();
    expect(coerceCounts(undefined)).toEqual(zeros);
    expect(coerceCounts(null)).toEqual(zeros);
    expect(coerceCounts('nope')).toEqual(zeros);
    expect(coerceCounts(42)).toEqual(zeros);
  });

  it('reads only the five known kinds, ignoring anything else present', () => {
    expect(coerceCounts({ memory: 3, danger: 1, somethingNew: 9 })).toEqual({
      memory: 3,
      embedding: 0,
      summary: 0,
      danger: 1,
      image: 0,
    });
  });

  it('rejects non-finite and non-numeric values (an old server\'s shape)', () => {
    expect(coerceCounts({ memory: '3', embedding: NaN, summary: Infinity })).toEqual(
      emptyActivityCounts(),
    );
  });

  it('hasActivity needs a positive count somewhere', () => {
    expect(hasActivity(emptyActivityCounts())).toBe(false);
    expect(hasActivity({ ...emptyActivityCounts(), image: 1 })).toBe(true);
  });
});

describe('blippedKinds (the pulse rule)', () => {
  const base = emptyActivityCounts();

  it('a FIRST read is a delta base — never a pulse', () => {
    expect(blippedKinds(null, { ...base, memory: 7, image: 2 })).toEqual([]);
  });

  it('an advance pulses exactly the kinds that moved', () => {
    expect(
      blippedKinds({ ...base, memory: 1, summary: 4 }, { ...base, memory: 3, summary: 4 }),
    ).toEqual(['memory']);
  });

  it('an unchanged counter does not pulse', () => {
    expect(blippedKinds({ ...base, memory: 5 }, { ...base, memory: 5 })).toEqual([]);
  });

  it('a DECREASE is a server restart — a fresh baseline, not a blip', () => {
    expect(blippedKinds({ ...base, memory: 900 }, { ...base, memory: 0 })).toEqual([]);
  });

  it('reports the blipped kinds in the render order', () => {
    expect(
      blippedKinds(base, { memory: 1, embedding: 1, summary: 1, danger: 1, image: 1 }),
    ).toEqual(['memory', 'embedding', 'summary', 'danger', 'image']);
  });
});
