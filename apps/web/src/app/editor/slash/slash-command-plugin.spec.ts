import { afterEach, describe, expect, it } from 'vitest';

import type { SlashBuiltInToolSource, SlashCustomToolSource } from '../../chat/slash-commands';
import { mountSlashHarness, type SlashHarness, type SlashHarnessOptions } from './slash-command-harness';
import {
  EMPTY_LABEL,
  LISTBOX_ID,
  LOADING_LABEL,
  MENU_LIMIT,
} from './slash-command-plugin';

/**
 * The composer's `/` command menu against a REAL ProseMirror editor — ported
 * case for case from v4's
 * `__tests__/unit/components/chat/lexical/plugins/SlashCommandPlugin.test.tsx`
 * (`4877fc2da`, 265 lines at `25b8fab6a`: 13 `it` + one `it.each` of 3 rows —
 * counted by file), same fixtures, same keystrokes, same expected text. v4's
 * cases: "opens on a bare / with actions, custom tools and available
 * built-ins", "narrows as the writer types", "finds a tool by a word of its
 * id", "stays shut in prose: %j" (×3), "never mounts while the setting is off",
 * "Enter removes the typed text and runs the action", "keeps the surrounding
 * text", "one undo brings the typed text back", "Escape closes the menu and
 * leaves the text", "opens Run Tool on the form of a built-in with parameters",
 * "runs a built-in with no parameters at once", "opens the custom-tool dialog
 * for a Pascal tool with parameters", "runs a Pascal tool with no parameters at
 * once", "/roll opens Run Tool on rng; /d20 is the quick roll".
 *
 * Trigger, ranking and registry rules are pinned by the v4 corpus in
 * `chat/slash-commands/slash-commands.oracle.spec.ts`; this suite is about the
 * wiring. The v5-shape rows after v4's (the loading label and the held keys,
 * the custom list's gate, the cap, the glyphs) are the `@` typeahead's.
 *
 * @module editor/slash/slash-command-plugin.spec
 */

const BUILT_INS: SlashBuiltInToolSource[] = [
  { id: 'self_inventory', name: 'Self Inventory', description: 'Look in your pockets', category: 'utility', parameters: { type: 'object', properties: {} } },
  { id: 'wardrobe_wear', name: 'Wear Wardrobe Items', description: 'Put things on', category: 'wardrobe', parameters: { type: 'object', properties: { operations: { type: 'array' } } } },
  { id: 'generate_image', name: 'Generate Image', description: 'Paint', available: false, unavailableReason: 'no profile' },
];

const ROSTER: SlashCustomToolSource[] = [
  { name: 'unlock', title: 'Force the Lock', description: 'Pick it', parameters: { bonus: { type: 'number' } }, asCharacterId: 'char-1' },
  { name: 'wear_regalia', title: 'Wear Regalia', description: 'Dress up', parameters: {}, asCharacterId: 'char-1' },
];

const answer = <T,>(list: readonly T[]) => async () => list;
const never = <T,>() => () => new Promise<readonly T[]>(() => undefined);

describe('slashCommandPlugin', () => {
  let harness: SlashHarness;

  afterEach(() => harness?.destroy());

  function mount(overrides: Partial<SlashHarnessOptions> = {}): SlashHarness {
    harness = mountSlashHarness({
      builtIns: answer(BUILT_INS),
      customs: answer(ROSTER),
      // v4 `makeContext`: attach, library, inform, d20; customToolsAvailable.
      actions: ['attach', 'library', 'inform', 'd20'],
      customToolsAvailable: true,
      ...overrides,
    });
    return harness;
  }

  async function openMenu(h: SlashHarness, text: string): Promise<void> {
    h.seed(text);
    await h.settle();
  }

  it("carries v4's constants (the listbox id with v5's qt- prefix)", () => {
    expect(MENU_LIMIT).toBe(12);
    expect(LISTBOX_ID).toBe('qt-slash-command-listbox');
    expect(EMPTY_LABEL).toBe('No such command');
    expect(LOADING_LABEL).toBe('Consulting the tool room…');
  });

  describe('the menu', () => {
    it('opens on a bare / with actions, custom tools and available built-ins', async () => {
      const h = mount();
      await openMenu(h, '/');
      const labels = h.optionLabels();
      expect(labels.some((l) => l.includes('/attach'))).toBe(true);
      expect(labels.some((l) => l.includes('/unlock'))).toBe(true);
      expect(labels.some((l) => l.includes('/wardrobe_wear'))).toBe(true);
      expect(labels.some((l) => l.includes('/generate_image'))).toBe(false);
      expect(labels.some((l) => l.includes('/pascal'))).toBe(true);
    });

    it('narrows as the writer types', async () => {
      const h = mount();
      await openMenu(h, 'then /un');
      expect(h.optionLabels()).toEqual([expect.stringContaining('/unlock')]);
    });

    it('finds a tool by a word of its id', async () => {
      const h = mount();
      await openMenu(h, '/wear');
      expect(h.optionLabels()[0]).toContain('/wear_regalia');
      expect(h.optionLabels().some((l) => l.includes('/wardrobe_wear'))).toBe(true);
    });

    it.each(['and/or', '10/15', 'see http://x'])('stays shut in prose: %j', async (text) => {
      const h = mount();
      await openMenu(h, text);
      expect(h.optionLabels()).toHaveLength(0);
      expect(document.getElementById(LISTBOX_ID)).toBeNull();
    });

    it('never mounts while the setting is off', async () => {
      const h = mount({ enabled: false });
      await openMenu(h, '/att');
      expect(h.optionLabels()).toHaveLength(0);
      // Enter reaches the editor (a new paragraph here; a send in the composer).
      expect(h.pressKey('Enter').handled).toBe(true);
      expect(h.text().startsWith('/att')).toBe(true);
      expect(h.text()).toBe('/att\n\n');
      expect(h.callsTo('attach')).toHaveLength(0);
      // Nothing was asked for, either.
      expect(h.fetches).toEqual({ builtIns: 0, customs: 0 });
    });
  });

  describe('picking a command', () => {
    it('Enter removes the typed text and runs the action', async () => {
      const h = mount();
      await openMenu(h, '/atta');
      expect(h.pressKey('Enter').handled).toBe(true);
      expect(h.text()).toBe('');
      expect(h.callsTo('attach')).toHaveLength(1);
    });

    it('keeps the surrounding text', async () => {
      const h = mount();
      await openMenu(h, 'hello /infor');
      h.pressKey('Enter');
      expect(h.text()).toBe('hello ');
      expect(h.callsTo('inform')).toHaveLength(1);
    });

    it('one undo brings the typed text back', async () => {
      const h = mount();
      await openMenu(h, '/atta');
      h.pressKey('Enter');
      h.undo();
      expect(h.text()).toBe('/atta');
    });

    it('one undo brings back a TYPED /query, not the whole typing run (R-L3)', async () => {
      // Typed within prosemirror-history's 500 ms window, the removal would
      // merge into the typing without its own closeHistory — and an undo would
      // take the whole word away (v4's reason for NOT using HISTORY_MERGE_TAG).
      const h = mount();
      h.seed('hi ');
      h.type('/');
      h.type('atta');
      await h.settle();
      h.pressKey('Enter');
      expect(h.text()).toBe('hi ');
      h.undo();
      expect(h.text()).toBe('hi /atta');
    });

    it('Escape closes the menu and leaves the text', async () => {
      const h = mount();
      await openMenu(h, '/atta');
      expect(h.pressKey('Escape').handled).toBe(true);
      expect(h.optionLabels()).toHaveLength(0);
      expect(h.text()).toBe('/atta');
      expect(h.callsTo('attach')).toHaveLength(0);
    });

    it('opens Run Tool on the form of a built-in with parameters', async () => {
      const h = mount();
      await openMenu(h, '/wardrobe_w');
      h.pressKey('Enter');
      expect(h.callsTo('openRunTool')).toEqual([['wardrobe_wear']]);
      expect(h.callsTo('runBuiltInNow')).toHaveLength(0);
    });

    it('runs a built-in with no parameters at once', async () => {
      const h = mount();
      await openMenu(h, '/self_inventory');
      h.pressKey('Enter');
      expect(h.callsTo('runBuiltInNow')).toEqual([['self_inventory']]);
      expect(h.callsTo('openRunTool')).toHaveLength(0);
    });

    it('opens the custom-tool dialog for a Pascal tool with parameters', async () => {
      const h = mount();
      await openMenu(h, '/unlock');
      h.pressKey('Enter');
      expect(h.callsTo('openCustomTool')).toEqual([[{ name: 'unlock', asCharacterId: 'char-1' }]]);
    });

    it('runs a Pascal tool with no parameters at once', async () => {
      const h = mount();
      await openMenu(h, '/wear_regalia');
      h.pressKey('Enter');
      expect(h.callsTo('runCustomNow')).toEqual([[{ name: 'wear_regalia', asCharacterId: 'char-1' }]]);
    });

    it('/roll opens Run Tool on rng; /d20 is the quick roll', async () => {
      const h = mount();
      await openMenu(h, '/roll');
      h.pressKey('Enter');
      expect(h.callsTo('openRunTool')).toEqual([['rng']]);

      await openMenu(h, '/d20');
      h.pressKey('Enter');
      expect(h.callsTo('d20')).toHaveLength(1);
    });
  });

  describe('v5 shape (the @ typeahead precedent)', () => {
    it('/tool and /pascal open their dialogs with nothing pre-selected', async () => {
      const h = mount();
      await openMenu(h, '/tool');
      h.pressKey('Enter');
      await openMenu(h, '/pascal');
      h.pressKey('Enter');
      expect(h.calls).toEqual([['openRunTool', undefined], ['openCustomTool', undefined]]);
    });

    it('shows the loading label and HOLDS Enter / Tab while a list is pending with nothing matched', async () => {
      const h = mount({ builtIns: never(), customs: never() });
      await openMenu(h, '/self_inv');
      expect(h.optionLabels()).toEqual([]);
      expect(h.menuText()).toBe(LOADING_LABEL);
      expect(h.pressKey('Enter').handled).toBe(true);
      expect(h.pressKey('Tab').handled).toBe(true);
      // Held, not committed and not passed to the editor.
      expect(h.text()).toBe('/self_inv');
      expect(h.calls).toEqual([]);
    });

    it('does NOT hold Enter while loading when an action already matches', async () => {
      const h = mount({ builtIns: never(), customs: never() });
      await openMenu(h, '/atta');
      expect(h.optionLabels()[0]).toContain('/attach');
      h.pressKey('Enter');
      expect(h.callsTo('attach')).toHaveLength(1);
    });

    it('turns the loading label into rows once the lists arrive', async () => {
      let release: (tools: readonly SlashBuiltInToolSource[]) => void = () => undefined;
      const pending = new Promise<readonly SlashBuiltInToolSource[]>((resolve) => (release = resolve));
      const h = mount({ builtIns: () => pending });
      await openMenu(h, '/self_inv');
      expect(h.menuText()).toBe(LOADING_LABEL);
      release(BUILT_INS);
      await h.settle();
      expect(h.optionLabels()).toEqual(['/self_inventory — Self Inventory']);
    });

    it('an empty SETTLED menu says so and lets Enter through', async () => {
      const h = mount();
      await openMenu(h, '/zzz');
      expect(h.menuText()).toBe(EMPTY_LABEL);
      expect(h.pressKey('Enter').handled).toBe(true);
      // The base keymap's Enter, not the menu's.
      expect(h.text()).toBe('/zzz\n\n');
      expect(h.calls).toEqual([]);
    });

    it('leaves Pascal alone while the chat has no roster: no /pascal, no customs, no fetch', async () => {
      const h = mount({ customToolsAvailable: false });
      await openMenu(h, '/');
      const labels = h.optionLabels();
      expect(labels.some((l) => l.includes('/pascal'))).toBe(false);
      expect(labels.some((l) => l.includes('/unlock'))).toBe(false);
      expect(labels.some((l) => l.includes('/wardrobe_wear'))).toBe(true);
      expect(h.fetches.customs).toBe(0);
    });

    it('fetches only while the menu is open', async () => {
      const h = mount();
      h.seed('plain words');
      await h.settle();
      expect(h.fetches).toEqual({ builtIns: 0, customs: 0 });
      await openMenu(h, '/');
      expect(h.fetches).toEqual({ builtIns: 1, customs: 1 });
    });

    it("re-asks Pascal on every open (staleTime 0) but keeps Prospero's list for a minute", async () => {
      const h = mount();
      await openMenu(h, '/');
      h.pressKey('Escape');
      h.seed('closed');
      await h.settle();
      await openMenu(h, '/');
      expect(h.fetches).toEqual({ builtIns: 1, customs: 2 });
    });

    it('caps a typed query at 12 rows; a bare / lists every command', async () => {
      const many: SlashCustomToolSource[] = Array.from({ length: 15 }, (_, i) => ({
        name: `dice_${String(i).padStart(2, '0')}`,
        title: `Dice ${i}`,
        description: 'd',
        parameters: {},
      }));
      const h = mount({ customs: answer(many) });
      await openMenu(h, '/dice');
      expect(h.optionLabels()).toHaveLength(12);
      await openMenu(h, '/');
      // 4 wired actions + tool + roll + pascal, 15 customs, 2 available built-ins.
      expect(h.optionLabels()).toHaveLength(7 + 15 + 2);
    });

    it("renders v4's label, source and icon per row (`glyphFor`, `ACTION_ICONS`)", async () => {
      const h = mount();
      await openMenu(h, '/we');
      expect(h.optionLabels()).toEqual([
        '/wear_regalia \u2014 Wear Regalia',
        '/wardrobe_wear \u2014 Wear Wardrobe Items',
      ]);
      expect(h.optionDetails()).toEqual(['Pascal', 'Prospero \u00b7 wardrobe']);
      expect(h.optionIcons()).toEqual(['wand', 'wrench']);
      await openMenu(h, '/atta');
      // `/attach` and `/library` ("Attach from library").
      expect(h.optionDetails()).toEqual(['Composer', 'Composer']);
      expect(h.optionIcons()).toEqual(['paperclip', 'file-plus']);
      // A quick roll has no entry of its own: v4's `dice` fallback.
      await openMenu(h, '/d2');
      expect(h.optionIcons()).toEqual(['dice']);
      await openMenu(h, '/infor');
      expect(h.optionIcons()).toEqual(['info']);
      await openMenu(h, '/too');
      expect(h.optionIcons()[0]).toBe('wrench');
      await openMenu(h, '/pasc');
      expect(h.optionIcons()).toEqual(['wand']);
    });

    it('ArrowDown moves the pick; Tab commits like Enter', async () => {
      const h = mount();
      await openMenu(h, '/we');
      expect(h.pressKey('ArrowDown').handled).toBe(true);
      expect(h.pressKey('Tab').handled).toBe(true);
      expect(h.callsTo('openRunTool')).toEqual([['wardrobe_wear']]);
    });

    it('Space is not a commit key: the trigger dissolves and the text stays as prose', async () => {
      const h = mount();
      await openMenu(h, '/un');
      expect(h.pressKey(' ').handled).toBe(false);
      h.type(' ');
      await h.settle();
      expect(h.optionLabels()).toHaveLength(0);
      expect(document.getElementById(LISTBOX_ID)).toBeNull();
      expect(h.text()).toBe('/un ');
      expect(h.calls).toEqual([]);
    });
  });
});
