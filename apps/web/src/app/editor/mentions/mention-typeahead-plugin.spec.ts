import { afterEach, describe, expect, it } from 'vitest';

import type { MentionCandidate } from '../../chat/mentions/mention-typeahead';
import { mountMentionHarness, type MentionHarness } from './mention-typeahead-harness';
import {
  EMPTY_LABEL,
  ERROR_LABEL,
  LISTBOX_ID,
  LOADING_LABEL,
  MENU_LIMIT,
} from './mention-typeahead-plugin';

/**
 * The composer's `@` character typeahead against a REAL ProseMirror editor —
 * ported case-for-case from v4's
 * `__tests__/unit/components/chat/lexical/plugins/MentionTypeaheadPlugin.test.tsx`
 * (`3376b3dfa`), same cast, same keystrokes, same expected text.
 *
 * Match rules and the keep-or-strip verdict are pinned by the v4 corpus in
 * `chat/mentions/mention-typeahead.oracle.spec.ts`; this suite is about the
 * wiring — that the menu opens, that Enter / Tab / Space complete the
 * highlighted name, and that the `@` is dropped everywhere except a line-start
 * Carina query.
 *
 * @module editor/mentions/mention-typeahead-plugin.spec
 */

const CHARACTERS: MentionCandidate[] = [
  { id: 'c-aris', name: 'Aristarchus', title: 'the astronomer' },
  { id: 'c-arab', name: 'Arabella' },
  { id: 'c-barn', name: 'Barnaby' },
  { id: 'c-jean', name: 'Jean-Luc' },
];

const answers = async () => CHARACTERS;
const never = () => new Promise<readonly MentionCandidate[]>(() => undefined);
const fails = async (): Promise<readonly MentionCandidate[]> => {
  throw new Error('HTTP 500');
};

describe('mentionTypeaheadPlugin', () => {
  let harness: MentionHarness;

  afterEach(() => harness?.destroy());

  function mount(
    options: { priority?: string[]; respond?: () => Promise<readonly MentionCandidate[]> } = {},
  ): MentionHarness {
    harness = mountMentionHarness({
      respond: options.respond ?? answers,
      priorityCharacterIds: options.priority,
    });
    return harness;
  }

  async function openMenu(h: MentionHarness, ...lines: string[]): Promise<void> {
    h.seed(...lines);
    await h.settle();
  }

  it("carries v4's constants", () => {
    expect(MENU_LIMIT).toBe(10);
    expect(LISTBOX_ID).toBe('qt-mention-typeahead-listbox');
    expect(EMPTY_LABEL).toBe('No such personage in the register');
    expect(LOADING_LABEL).toBe('Consulting the register\u2026');
    expect(ERROR_LABEL).toBe('The register could not be reached');
  });

  describe('the menu', () => {
    it('opens on a bare @ with every character', async () => {
      const h = mount();
      await openMenu(h, 'hello @');
      expect(h.optionLabels()).toHaveLength(CHARACTERS.length);
    });

    it('narrows as the writer types', async () => {
      const h = mount();
      await openMenu(h, 'hello @ar');
      expect(h.optionLabels()).toEqual(['Arabella', 'Aristarchus']);
    });

    it('puts the chat cast first', async () => {
      const h = mount({ priority: ['c-aris'] });
      await openMenu(h, '@ar');
      expect(h.optionLabels()[0]).toBe('Aristarchus');
    });

    it('stays shut inside an email address', async () => {
      const h = mount();
      await openMenu(h, 'mail name@ar');
      expect(h.optionLabels()).toHaveLength(0);
      expect(h.menuText()).toBeNull();
    });

    it('shows the glyph, and the cast / title detail, on each row', async () => {
      const h = mount({ priority: ['c-arab'] });
      await openMenu(h, 'hello @ar');
      const rows = Array.from(document.querySelectorAll('.qt-typeahead-menu [role="option"]'));
      expect(rows.map((row) => row.querySelector('.qt-typeahead-option-glyph')?.textContent)).toEqual([
        '@',
        '@',
      ]);
      expect(rows.map((row) => row.querySelector('.qt-typeahead-option-detail')?.textContent)).toEqual([
        'in this chat',
        'the astronomer',
      ]);
      expect(document.querySelector('.qt-typeahead-menu')?.id).toBe(LISTBOX_ID);
      expect(h.view.dom.getAttribute('aria-controls')).toBe(LISTBOX_ID);
      expect(h.view.dom.getAttribute('aria-expanded')).toBe('true');
      expect(h.view.dom.getAttribute('aria-activedescendant')).toBe(`${LISTBOX_ID}-option-0`);
    });
  });

  describe('while the list is loading or unreachable', () => {
    it('says it is consulting the register, not that nobody matches', async () => {
      const h = mount({ respond: never });
      await openMenu(h, 'hello @ari');
      expect(h.menuText()).toContain('Consulting the register');
      expect(h.menuText()).not.toContain('No such personage');
    });

    it('holds Enter rather than sending the half-typed name', async () => {
      const h = mount({ respond: never });
      await openMenu(h, 'hello @ari');
      expect(h.pressKey('Enter').handled).toBe(true);
      expect(h.text()).toBe('hello @ari');
    });

    it('holds Tab too', async () => {
      const h = mount({ respond: never });
      await openMenu(h, 'hello @ari');
      expect(h.pressKey('Tab').handled).toBe(true);
    });

    it('says so when the register cannot be reached, and lets Enter through', async () => {
      const h = mount({ respond: fails });
      await openMenu(h, 'hello @ari');
      expect(h.menuText()).toContain('could not be reached');
      // Not held: Enter reaches the editor (here, a new paragraph; in the
      // composer, the send).
      h.pressKey('Enter');
      expect(h.text()).not.toBe('hello @ari');
    });

    it('says nobody matches once the list is in and nothing does', async () => {
      const h = mount();
      await openMenu(h, 'hello @zz');
      expect(h.menuText()).toBe(EMPTY_LABEL);
    });
  });

  describe('Brahma', () => {
    it('is offered at the start of a line', async () => {
      const h = mount();
      await openMenu(h, '@bra');
      expect(h.optionLabels()).toEqual(['Brahma']);
    });

    it('is not offered mid-line', async () => {
      const h = mount();
      await openMenu(h, 'ask @bra');
      expect(h.optionLabels()).toEqual([]);
    });

    it('completes into a Brahma query', async () => {
      const h = mount();
      await openMenu(h, 'first', '@bra');
      h.pressKey('Enter');
      h.type('?');
      h.type(' ');
      h.type('how many chats?');
      expect(h.text()).toBe('first\n@Brahma? how many chats?');
    });
  });

  describe('completing mid-line', () => {
    it('Enter takes the first name and drops the @', async () => {
      const h = mount();
      await openMenu(h, 'hello @ari');
      expect(h.pressKey('Enter').handled).toBe(true);
      expect(h.text()).toBe('hello Aristarchus');
    });

    it('Tab does the same', async () => {
      const h = mount();
      await openMenu(h, 'hello @bar');
      h.pressKey('Tab');
      expect(h.text()).toBe('hello Barnaby');
    });

    it('Space completes and keeps its space', async () => {
      const h = mount();
      await openMenu(h, 'hello @ari');
      expect(h.pressKey(' ').handled).toBe(true);
      expect(h.text()).toBe('hello Aristarchus ');
      expect(h.caretOffset()).toBe('hello Aristarchus '.length);
    });

    it('Space with a modifier is just a space', async () => {
      const h = mount();
      await openMenu(h, 'hello @ari');
      expect(h.pressKey(' ', { shiftKey: true }).handled).toBe(false);
      expect(h.text()).toBe('hello @ari');
    });

    it('Space after a bare @ is just a space', async () => {
      const h = mount();
      await openMenu(h, 'meet me @');
      expect(h.pressKey(' ').handled).toBe(false);
      expect(h.text()).toBe('meet me @');

      // Not consumed, so the browser inserts the space itself — apply it the
      // way the editor would and confirm nothing else happens to the text.
      h.type(' ');
      await h.settle();
      expect(h.text()).toBe('meet me @ ');
      expect(h.optionLabels()).toHaveLength(0);
    });
  });

  describe('completing at the start of a line', () => {
    it('keeps the @ while undecided', async () => {
      const h = mount();
      await openMenu(h, '@ari');
      h.pressKey('Enter');
      await h.settle();
      expect(h.text()).toBe('@Aristarchus');
      // and does not reopen the menu on the completed name
      expect(h.optionLabels()).toHaveLength(0);
      expect(h.menuText()).toBeNull();
    });

    it.each([':', '?'])('keeps the @ for a Carina query (%s)', async (separator) => {
      const h = mount();
      await openMenu(h, '@ari');
      h.pressKey('Enter');
      h.type(separator);
      h.type(' ');
      h.type('what is the hour');
      expect(h.text()).toBe(`@Aristarchus${separator} what is the hour`);
    });

    it('drops the @ when anything else follows the name', async () => {
      const h = mount();
      await openMenu(h, '@ari');
      h.pressKey('Enter');
      h.type(',');
      expect(h.text()).toBe('Aristarchus,');
      expect(h.caretOffset()).toBe('Aristarchus,'.length);
    });

    it('drops the @ when the separator is not followed by a space', async () => {
      const h = mount();
      await openMenu(h, '@ari');
      h.pressKey('Enter');
      h.type(':');
      h.type('x');
      expect(h.text()).toBe('Aristarchus:x');
    });

    it('Space completion drops the @ straight away', async () => {
      const h = mount();
      await openMenu(h, '@ari');
      h.pressKey(' ');
      expect(h.text()).toBe('Aristarchus ');
    });

    it('drops the @ at once for a name the Carina parser cannot address', async () => {
      const h = mount();
      await openMenu(h, '@jea');
      h.pressKey('Enter');
      expect(h.text()).toBe('Jean-Luc');
    });

    it('and that completion is one undo step, like any other', async () => {
      // Without the at-once drop the `@Jean-Luc` would wait for the watcher,
      // whose strip then lands as an undo step of its own.
      const h = mount();
      await openMenu(h, '@jea');
      h.pressKey('Enter');
      h.undo();
      expect(h.text()).toBe('@jea');
    });

    it('treats the line after a soft break as a line start', async () => {
      const h = mount();
      await openMenu(h, 'first line', '@ari');
      h.pressKey('Enter');
      h.type(':');
      h.type(' ');
      expect(h.text()).toBe('first line\n@Aristarchus: ');
    });

    it('drops the @ on the right line after a soft break', async () => {
      const h = mount();
      await openMenu(h, 'first line', '@ari');
      h.pressKey('Enter');
      h.type(',');
      expect(h.text()).toBe('first line\nAristarchus,');
      expect(h.caretOffset()).toBe('first line\nAristarchus,'.length);
    });

    it('judges a restored @ again after an undo', async () => {
      const h = mount();
      await openMenu(h, '@ari');
      h.pressKey('Enter');
      await h.settle();
      h.type(',');
      await h.settle();
      expect(h.text()).toBe('Aristarchus,');

      h.undo();
      expect(h.text()).toBe('@Aristarchus');

      h.type('.');
      expect(h.text()).toBe('Aristarchus.');
    });

    it('keeps a restored @ that goes on to become a query', async () => {
      const h = mount();
      await openMenu(h, '@ari');
      h.pressKey('Enter');
      await h.settle();
      h.type(',');
      await h.settle();
      h.undo();

      h.type(':');
      h.type(' ');
      expect(h.text()).toBe('@Aristarchus: ');
    });

    it('one undo takes back the keystroke and the dropped @ together', async () => {
      const h = mount();
      await openMenu(h, '@ari');
      h.pressKey('Enter');
      await h.settle();
      h.type(',');
      await h.settle();
      expect(h.text()).toBe('Aristarchus,');

      h.undo();
      expect(h.text()).toBe('@Aristarchus');
    });

    it('one undo takes back the keystroke even when typed at once (the history grouping)', async () => {
      // No settle between the completion and the keystroke: prosemirror-history
      // would fold a keystroke inside its 500 ms window into the completion's
      // event, and one undo would then go back to `@ari`.
      const h = mount();
      await openMenu(h, '@ari');
      h.pressKey('Enter');
      h.type(',');
      h.undo();
      expect(h.text()).toBe('@Aristarchus');
    });
  });
});
