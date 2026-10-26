import { describe, expect, it } from 'vitest';

import {
  characterSortFields,
  compareScenarios,
  scenarioSortKey,
  sortScenarios,
  storeSortFields,
} from './scenario-sort';

/**
 * Transcription parity for v4 `lib/scenarios/sort.ts` (`1d0a31132`, pinned at
 * `322a5488e`) — the six pure `it`s of v4's `scenario-sort.test.ts`. The corpus
 * differential against v4's REAL module is `scenario-sort.oracle.spec.ts`.
 */
describe('scenarioSortKey (v4 lib/scenarios/sort.ts)', () => {
  it('drops a leading "The", whatever its case', () => {
    expect(scenarioSortKey('The Gilded Cage')).toBe('Gilded Cage');
    expect(scenarioSortKey('  THE Gilded Cage ')).toBe('Gilded Cage');
    expect(scenarioSortKey('the-attic')).toBe('attic');
    expect(scenarioSortKey('the_attic')).toBe('attic');
  });

  it('leaves "The" inside a word or mid-title alone', () => {
    expect(scenarioSortKey('Theatre Night')).toBe('Theatre Night');
    expect(scenarioSortKey('Into the Woods')).toBe('Into the Woods');
  });

  it('keeps a title that is only "The"', () => {
    expect(scenarioSortKey('The')).toBe('The');
    expect(scenarioSortKey('The-')).toBe('The-');
  });
});

describe('sortScenarios (v4 lib/scenarios/sort.ts)', () => {
  const sc = (title: string, filename = title, path?: string) => ({ title, filename, path });

  it('sorts by title with the leading article ignored, case-insensitively', () => {
    const titles = ['The Zeppelin', 'casino', 'The Ballroom', 'Atrium'];
    expect(sortScenarios(titles, (t) => sc(t))).toEqual([
      'Atrium',
      'The Ballroom',
      'casino',
      'The Zeppelin',
    ]);
  });

  it('breaks a title tie on the filename, article ignored there too', () => {
    const rows = [
      { t: 'same', f: 'the-zed' },
      { t: 'same', f: 'attic' },
    ];
    expect(sortScenarios(rows, (r) => sc(r.t, r.f)).map((r) => r.f)).toEqual(['attic', 'the-zed']);
  });

  it('is a total order — identical keys fall back to the raw title, then the path', () => {
    const a = sc('gala', 'gala', 'Scenarios/a.md');
    const b = sc('Gala', 'Gala', 'Scenarios/A.md');
    expect(Math.sign(compareScenarios(a, b))).toBe(-1);
    expect(Math.sign(compareScenarios(b, a))).toBe(1);
    expect(compareScenarios(a, a)).toBe(0);
  });

  it('returns a sorted copy, leaving the input alone', () => {
    const input = ['b', 'a'];
    const out = sortScenarios(input, (t) => sc(t));
    expect(out).toEqual(['a', 'b']);
    expect(input).toEqual(['b', 'a']);
  });
});

describe('the picker’s sort fields (v4 ScenarioSelect.tsx:40-42)', () => {
  it('store tiers sort on name, filename, path', () => {
    expect(
      storeSortFields({
        path: 'Scenarios/the-attic.md',
        filename: 'the-attic',
        name: 'The Attic',
        isDefault: false,
        body: '',
      }),
    ).toEqual({ title: 'The Attic', filename: 'the-attic', path: 'Scenarios/the-attic.md' });
  });

  it('a character scenario has no filename, so the title stands in and the id is the path', () => {
    expect(characterSortFields({ id: 'sc-1', title: 'A Duel', content: 'x' })).toEqual({
      title: 'A Duel',
      filename: 'A Duel',
      path: 'sc-1',
    });
  });
});
