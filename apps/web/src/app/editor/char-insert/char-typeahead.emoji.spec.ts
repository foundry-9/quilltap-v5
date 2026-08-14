import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import datasetPayload from '../../../../public/emoji/emoji-index.v1.json';
import { compileRules } from '../text-replacement';
import {
  mountHarness,
  stubFetch,
  type FetchStub,
  type Harness,
} from './char-typeahead-harness';
import { EMOJI_PROFILE } from './profiles/emoji';
import { buildIndex, searchChars } from './search';

/**
 * The composer emoji typeahead — plugin-level coverage of the EMOJI profile,
 * ported case-for-case from v4's
 * `__tests__/unit/components/chat/lexical/plugins/CharTypeaheadPlugin.emoji.test.tsx`.
 *
 * Runs against a REAL ProseMirror editor and the REAL committed dataset (served
 * through the fetch stub, exactly as the browser would fetch it), so these
 * assertions exercise the whole adapter: lazy load, trigger detection, the
 * closing-colon commit, the bail conditions, the menu, and the undo contract.
 *
 * The `:name` MATCH RULES themselves are pinned by the shared corpora in
 * `search.spec.ts` / `trigger.spec.ts`; this suite is about the wiring.
 *
 * @module editor/char-insert/char-typeahead.emoji.spec
 */

/** 😄 — the dataset's `:smile:`. Spelled by lookup so no invisible codepoint is transcribed. */
const SMILE = (datasetPayload.emoji as { c: string; s: string[] }[]).find((entry) =>
  entry.s.includes('smile'),
)!.c;

/**
 * What the MENU should show for `:smi`, taken from the engine rather than
 * transcribed: the ranking itself is pinned by the shared corpus in
 * `search.spec.ts`, and this suite is about the wiring showing and committing
 * exactly that order (`:smi` is an alias PREFIX for several faces, so the first
 * row is not `:smile:`).
 */
const SMI_ROWS = searchChars(buildIndex(datasetPayload), 'smi', 10);

describe('charTypeaheadPlugin — emoji profile', () => {
  let harness: Harness;
  let fetchStub: FetchStub;

  beforeEach(() => {
    EMOJI_PROFILE.loader.resetForTests();
    window.localStorage.clear();
    fetchStub = stubFetch('emoji-index.v1.json', datasetPayload);
  });

  afterEach(() => {
    harness?.destroy();
    fetchStub.restore();
  });

  function mount(options: { enabled?: boolean; rules?: ReturnType<typeof compileRules> } = {}) {
    harness = mountHarness({ profile: EMOJI_PROFILE, ...options });
    return harness;
  }

  /**
   * Warm the lazy dataset the way the app does — the plugin only fetches once a
   * `:` is actually in play, so type one and let the promise settle.
   */
  async function warmIndex(h: Harness) {
    h.seed('warm :ab');
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  }

  describe('the closing-colon commit', () => {
    it('commits an exact shortcode without the menu ever mattering', async () => {
      const h = mount();
      await warmIndex(h);

      h.seed('hello :smile');
      expect(h.pressKey(':').handled).toBe(true);
      expect(h.text()).toBe(`hello ${SMILE}`);
    });

    it('inserts NO trailing space and leaves the caret after the emoji', async () => {
      const h = mount();
      await warmIndex(h);

      h.seed(':smile');
      h.pressKey(':');

      expect(h.text()).toBe(SMILE);
      expect(h.caretOffset()).toBe(SMILE.length);
    });

    it('lets an unknown shortcode through as literal text', async () => {
      const h = mount();
      await warmIndex(h);

      h.seed('hello :notanemoji');
      expect(h.pressKey(':').handled).toBe(false);
      expect(h.text()).toBe('hello :notanemoji');
    });

    it('records the pick in recents', async () => {
      const h = mount();
      await warmIndex(h);

      h.seed(':smile');
      h.pressKey(':');

      expect(JSON.parse(window.localStorage.getItem(EMOJI_PROFILE.recentsStorageKey)!)).toEqual([
        SMILE,
      ]);
    });

    it('reverts to the literal typed text in ONE undo', async () => {
      const h = mount();
      await warmIndex(h);

      h.seed('hello :smile');
      h.pressKey(':');
      expect(h.text()).toBe(`hello ${SMILE}`);

      h.undo();

      expect(h.text()).toBe('hello :smile');
    });
  });

  describe('triggers that must never fire', () => {
    const cases: [string, string][] = [
      ['a URL scheme', 'see http:/'],
      ['a clock time', 'meet at 10:3'],
      ['a Windows path', 'open C:\\User'],
      ['an emoticon', 'hah :'],
      ['a colon glued to a word', 'ratio a:b'],
    ];
    for (const [label, text] of cases) {
      it(`does not commit on ${label}`, async () => {
        const h = mount();
        await warmIndex(h);

        h.seed(text);
        expect(h.pressKey(':').handled).toBe(false);
        expect(h.text()).toBe(text);
      });
    }

    it('does not commit when the colon is glued to a preceding bold run', async () => {
      const h = mount();
      await warmIndex(h);

      // The caret's own inline run starts at `:`, so a naive run-local check
      // would read it as the start of the block. It is not — `bold` precedes it.
      h.seedAfterBoldRun('bold', ':smile');
      expect(h.pressKey(':').handled).toBe(false);
      expect(h.text()).toBe('bold:smile');
    });
  });

  describe('bail conditions', () => {
    it('does nothing inside a fenced code block', async () => {
      const h = mount();
      await warmIndex(h);

      h.seedCodeBlock('const x = :smile');
      expect(h.pressKey(':').handled).toBe(false);
      expect(h.text()).toBe('const x = :smile');
    });

    it('does nothing inside an inline code run', async () => {
      const h = mount();
      await warmIndex(h);

      h.seedInlineCode(':smile');
      expect(h.pressKey(':').handled).toBe(false);
      expect(h.text()).toBe(':smile');
    });

    it('does nothing while an IME composition is active', async () => {
      const h = mount();
      await warmIndex(h);

      h.seed(':smile');
      h.setComposing(true);

      expect(h.pressKey(':').handled).toBe(false);
      expect(h.text()).toBe(':smile');
    });

    it('is inert when composerEmoji is off — and never even fetches the dataset', async () => {
      const h = mount({ enabled: false });

      h.seed(':smile');
      expect(h.pressKey(':').handled).toBe(false);
      expect(h.text()).toBe(':smile');
      expect(h.menuRows()).toBeNull();
      expect(fetchStub.datasetFetches()).toBe(0);
    });

    it('is inert, without throwing, when the dataset cannot be loaded', async () => {
      fetchStub.restore();
      fetchStub = stubFetch('emoji-index.v1.json', datasetPayload, { datasetFails: true });
      const h = mount();
      await warmIndex(h);

      h.seed(':smile');
      expect(h.pressKey(':').handled).toBe(false);
      expect(h.text()).toBe(':smile');
      expect(h.menuRows()).toBeNull();
    });
  });

  describe('the lazy dataset', () => {
    it('is not fetched until a `:` is in play', async () => {
      const h = mount();
      h.seed('no colons here');
      h.pressKey(' ');

      expect(fetchStub.datasetFetches()).toBe(0);

      await warmIndex(h);
      expect(fetchStub.datasetFetches()).toBe(1);
    });

    it('is fetched exactly once no matter how many colons are typed', async () => {
      const h = mount();
      await warmIndex(h);

      h.seed(':smile');
      h.pressKey(':');
      h.seed(':tada');
      h.pressKey(':');

      expect(fetchStub.datasetFetches()).toBe(1);
    });
  });

  describe('the menu', () => {
    it('opens on a live query and lists matches by name', async () => {
      const h = mount();
      await warmIndex(h);

      h.seed('hello :smi');
      const rows = h.menuRows();
      expect(rows).not.toBeNull();
      expect(rows!.length).toBe(SMI_ROWS.length);
      expect(rows).toEqual(SMI_ROWS.map((entry) => entry.name));
    });

    it('commits the highlighted row on Enter, consuming the keystroke', async () => {
      const h = mount();
      await warmIndex(h);

      h.seed('hello :smi');
      expect(h.pressKey('Enter').handled).toBe(true);
      expect(h.text()).toBe(`hello ${SMI_ROWS[0].char}`);
      expect(h.menuRows()).toBeNull();
      expect(JSON.parse(window.localStorage.getItem(EMOJI_PROFILE.recentsStorageKey)!)).toEqual([
        SMI_ROWS[0].char,
      ]);
    });

    it('moves the highlight with the arrows and commits the second row', async () => {
      const h = mount();
      await warmIndex(h);

      h.seed('hello :smi');
      expect(h.menuRows()![1]).toBe(SMI_ROWS[1].name);
      h.pressKey('ArrowDown');
      h.pressKey('Enter');

      expect(h.text()).toBe(`hello ${SMI_ROWS[1].char}`);
    });

    it('dismisses on Escape and leaves the typed text alone', async () => {
      const h = mount();
      await warmIndex(h);

      h.seed('hello :smi');
      expect(h.pressKey('Escape').handled).toBe(true);
      expect(h.menuRows()).toBeNull();
      expect(h.text()).toBe('hello :smi');
    });

    it('leaves Enter alone when it is shut — the composer still submits', async () => {
      const h = mount();
      await warmIndex(h);

      h.seed('hello there');
      expect(h.menuRows()).toBeNull();
      expect(h.pressKey('Enter').handled).toBe(false);
    });

    it('the EMPTY menu owns nothing but Escape — a typo plus Enter still sends (the 4.8.2-round review find)', async () => {
      // v4's Lexical typeahead returns false from Enter/Tab with no selectable
      // option and skips preventDefault on the arrows, so `:smiel` (no matches,
      // the empty-label menu showing) never traps the writer. Consuming these
      // was the unification review's one blocking find.
      const h = mount();
      await warmIndex(h);

      h.seed('hello :zzqx');
      expect(h.menuRows()).toEqual([]); // open, in its empty-label state
      expect(h.pressKey('Enter').handled).toBe(false);
      expect(h.pressKey('Tab').handled).toBe(false);
      expect(h.pressKey('ArrowDown').handled).toBe(false);
      expect(h.pressKey('ArrowUp').handled).toBe(false);
      expect(h.pressKey('Escape').handled).toBe(true);
      expect(h.menuRows()).toBeNull();
    });

    it('stays shut on a CLOSED trigger — no jack-in-the-box after `:smile:`', async () => {
      const h = mount();
      await warmIndex(h);

      h.seed('hello :notanemoji:');
      expect(h.menuRows()).toBeNull();
    });
  });

  describe('interaction with Layer 1.5 text replacement', () => {
    it('lets a `:` typed after a replaceable word still fire the replacement', async () => {
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
      h.pressKey(':');

      // Emoji returned false (no exact shortcode to commit), so the
      // lower-priority replacement handler still got the keystroke.
      expect(h.text()).toBe('function:');
    });

    it('swallows the `:` when it commits, so no replacement also runs', async () => {
      const h = mount({
        rules: compileRules([
          {
            id: 'r1',
            fromText: 'smile',
            toText: 'GRIN',
            enabled: true,
            caseSensitive: false,
            sortOrder: 0,
            createdAt: '',
            updatedAt: '',
          },
        ]),
      });
      await warmIndex(h);

      h.seed(':smile');
      h.pressKey(':');

      expect(h.text()).toBe(SMILE);
    });
  });
});
