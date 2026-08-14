import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import datasetPayload from '../../../../public/unicode/unicode-index.v1.json';
import { compileRules } from '../text-replacement';
import { mountHarness, stubFetch, type FetchStub, type Harness } from './char-typeahead-harness';
import { UNICODE_PROFILE } from './profiles/unicode';
import { buildIndex, findByAlias } from './search';

/**
 * The composer Unicode typeahead — plugin-level coverage of the UNICODE
 * profile, ported case-for-case from v4's
 * `__tests__/unit/components/chat/lexical/plugins/CharTypeaheadPlugin.unicode.test.tsx`.
 *
 * Same adapter, same engine, second dataset: the trailing-space commit, the
 * math bail, alias CASE, code points, and the "never fetch from the space bar"
 * rule that keeps the most-pressed key on the keyboard from pulling down 300 KB.
 *
 * @module editor/char-insert/char-typeahead.unicode.spec
 */

const index = buildIndex(datasetPayload);
/** Spelled by lookup, so no invisible codepoint is transcribed into a spec. */
const ARROW = findByAlias(index, 'to', { caseSensitive: true })!.char;
const PHI_LOWER = findByAlias(index, 'phi', { caseSensitive: true })!.char;
const PHI_UPPER = findByAlias(index, 'Phi', { caseSensitive: true })!.char;

describe('charTypeaheadPlugin — unicode profile', () => {
  let harness: Harness;
  let fetchStub: FetchStub;

  beforeEach(() => {
    UNICODE_PROFILE.loader.resetForTests();
    window.localStorage.clear();
    fetchStub = stubFetch('unicode-index.v1.json', datasetPayload);
  });

  afterEach(() => {
    harness?.destroy();
    fetchStub.restore();
  });

  function mount(options: { enabled?: boolean; rules?: ReturnType<typeof compileRules> } = {}) {
    harness = mountHarness({ profile: UNICODE_PROFILE, ...options });
    return harness;
  }

  /**
   * Warm the lazy dataset the way the app does. The `\` typeahead fetches from
   * the TRIGGER path, never from the space bar, so drive it with a typed query.
   */
  async function warmIndex(h: Harness) {
    h.seed('warm \\ab');
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  }

  describe('the trailing-space commit', () => {
    it('commits an exact alias and keeps the space the writer typed', async () => {
      const h = mount();
      await warmIndex(h);

      h.seed('go \\to');
      expect(h.pressKey(' ').handled).toBe(true);
      expect(h.text()).toBe(`go ${ARROW} `);
    });

    it('leaves the caret after the space', async () => {
      const h = mount();
      await warmIndex(h);

      h.seed('\\to');
      h.pressKey(' ');

      expect(h.text()).toBe(`${ARROW} `);
      expect(h.caretOffset()).toBe(`${ARROW} `.length);
    });

    /** ⚠ The correctness point the whole spec is organized around. */
    it('respects alias CASE — \\phi is φ and \\Phi is Φ', async () => {
      const h = mount();
      await warmIndex(h);

      h.seed('\\phi');
      h.pressKey(' ');
      expect(h.text()).toBe(`${PHI_LOWER} `);

      h.seed('\\Phi');
      h.pressKey(' ');
      expect(h.text()).toBe(`${PHI_UPPER} `);

      expect(PHI_LOWER).not.toBe(PHI_UPPER);
    });

    it('commits a code point with no table lookup', async () => {
      const h = mount();
      await warmIndex(h);

      h.seed('\\u{1D538}');
      h.pressKey(' ');

      expect(h.text()).toBe('\u{1D538} ');
    });

    it('lets an unknown alias through as literal text', async () => {
      const h = mount();
      await warmIndex(h);

      h.seed('hello \\notacommand');
      expect(h.pressKey(' ').handled).toBe(false);
      expect(h.text()).toBe('hello \\notacommand');
    });

    it('records the pick in the Unicode recents, not the emoji ones', async () => {
      const h = mount();
      await warmIndex(h);

      h.seed('\\to');
      h.pressKey(' ');

      expect(JSON.parse(window.localStorage.getItem(UNICODE_PROFILE.recentsStorageKey)!)).toEqual([
        ARROW,
      ]);
      expect(window.localStorage.getItem('quilltap.emoji.recents.v1')).toBeNull();
    });

    it('reverts to the literal typed text in ONE undo', async () => {
      const h = mount();
      await warmIndex(h);

      h.seed('go \\to');
      h.pressKey(' ');
      expect(h.text()).toBe(`go ${ARROW} `);

      h.undo();

      expect(h.text()).toBe('go \\to');
    });
  });

  describe('triggers that must never fire', () => {
    const cases: [string, string][] = [
      ['a markdown escape', 'literal \\*'],
      ['an escaped underscore', 'literal \\_'],
      ['an escaped bracket', 'literal \\['],
      ['a Windows path', 'open C:\\User'],
      ['a lone backslash', 'a \\'],
    ];
    for (const [label, text] of cases) {
      it(`does not commit on ${label}`, async () => {
        const h = mount();
        await warmIndex(h);

        h.seed(text);
        expect(h.pressKey(' ').handled).toBe(false);
        expect(h.text()).toBe(text);
      });
    }

    it('does not commit when the backslash is glued to a preceding bold run', async () => {
      const h = mount();
      await warmIndex(h);

      h.seedAfterBoldRun('bold', '\\to');
      expect(h.pressKey(' ').handled).toBe(false);
      expect(h.text()).toBe('bold\\to');
    });
  });

  /** The hazard the spec singles out: a `\` typeahead that eats people's LaTeX. */
  describe('the math bail', () => {
    const cases: [string, string][] = [
      ['display math', '$$\\to'],
      ['single-dollar math', '$\\to'],
      ['a backslash-paren span', '\\(\\to'],
      ['a backslash-bracket span', '\\[\\to'],
    ];
    for (const [label, text] of cases) {
      it(`leaves ${label} alone`, async () => {
        const h = mount();
        await warmIndex(h);

        h.seed(text);
        expect(h.pressKey(' ').handled).toBe(false);
        expect(h.text()).toBe(text);
        expect(h.menuRows()).toBeNull();
      });
    }

    it('still fires after a currency amount, which is not math', async () => {
      const h = mount();
      await warmIndex(h);

      h.seed('costs $5 and \\to');
      expect(h.pressKey(' ').handled).toBe(true);
      expect(h.text()).toBe(`costs $5 and ${ARROW} `);
    });

    it('still fires after a CLOSED formula', async () => {
      const h = mount();
      await warmIndex(h);

      h.seed('$$x$$ and \\to');
      h.pressKey(' ');

      expect(h.text()).toBe(`$$x$$ and ${ARROW} `);
    });
  });

  describe('bail conditions', () => {
    it('does nothing inside a fenced code block', async () => {
      const h = mount();
      await warmIndex(h);

      h.seedCodeBlock('const x = \\to');
      expect(h.pressKey(' ').handled).toBe(false);
      expect(h.text()).toBe('const x = \\to');
    });

    it('does nothing inside an inline code run', async () => {
      const h = mount();
      await warmIndex(h);

      h.seedInlineCode('\\to');
      expect(h.pressKey(' ').handled).toBe(false);
      expect(h.text()).toBe('\\to');
    });

    it('does nothing while an IME composition is active', async () => {
      const h = mount();
      await warmIndex(h);

      h.seed('\\to');
      h.setComposing(true);

      expect(h.pressKey(' ').handled).toBe(false);
      expect(h.text()).toBe('\\to');
    });

    it('is inert when composerUnicode is off — and never even fetches the dataset', async () => {
      const h = mount({ enabled: false });

      h.seed('\\to');
      expect(h.pressKey(' ').handled).toBe(false);
      expect(h.text()).toBe('\\to');
      expect(h.menuRows()).toBeNull();
      expect(fetchStub.datasetFetches()).toBe(0);
    });

    it('is inert, without throwing, when the dataset cannot be loaded', async () => {
      fetchStub.restore();
      fetchStub = stubFetch('unicode-index.v1.json', datasetPayload, { datasetFails: true });
      const h = mount();
      await warmIndex(h);

      h.seed('\\to');
      expect(h.pressKey(' ').handled).toBe(false);
      expect(h.text()).toBe('\\to');
      expect(h.menuRows()).toBeNull();
    });
  });

  describe('the lazy dataset', () => {
    /**
     * The space bar is the commit key, and it is the most-pressed key there is.
     * Fetching from it would mean every instance downloads 300 KB the moment
     * anyone types a sentence.
     */
    it('is NEVER fetched from the space bar', () => {
      const h = mount();

      h.seed('no backslashes here');
      h.pressKey(' ');
      h.pressKey(' ');

      expect(fetchStub.datasetFetches()).toBe(0);
    });

    it('is fetched once a `\\` query is in play, and only once', async () => {
      const h = mount();
      await warmIndex(h);
      expect(fetchStub.datasetFetches()).toBe(1);

      h.seed('\\to');
      h.pressKey(' ');
      h.seed('\\phi');
      h.pressKey(' ');

      expect(fetchStub.datasetFetches()).toBe(1);
    });
  });

  describe('the menu', () => {
    it('puts a code-point query first, ahead of the name matches', async () => {
      const h = mount();
      await warmIndex(h);

      h.seed('\\u2192');
      expect(h.menuRows()![0]).toBe('rightwards arrow');
    });

    it('commits the highlighted row on Enter with NO trailing space', async () => {
      const h = mount();
      await warmIndex(h);

      h.seed('go \\u2192');
      expect(h.pressKey('Enter').handled).toBe(true);
      // The menu path swallowed no keystroke, so it adds no space — unlike the
      // space-bar commit above.
      expect(h.text()).toBe(`go ${ARROW}`);
    });

    it('never opens inside a math span', async () => {
      const h = mount();
      await warmIndex(h);

      h.seed('$$\\phi');
      expect(h.menuRows()).toBeNull();
    });
  });

  describe('interaction with Layer 1.5 text replacement', () => {
    it('lets a space typed after a replaceable word still fire the replacement', async () => {
      const h = mount({
        rules: compileRules([
          {
            id: 'r1',
            fromText: 'fn',
            toText: 'function',
            enabled: true,
            caseSensitive: false,
            sortOrder: 0,
            createdAt: '',
            updatedAt: '',
          },
        ]),
      });
      await warmIndex(h);

      h.seed('fn');
      h.pressKey(' ');

      // Unicode returned false (no `\` trigger to commit), so the
      // lower-priority replacement handler still got the keystroke.
      expect(h.text()).toBe('function ');
    });
  });
});
