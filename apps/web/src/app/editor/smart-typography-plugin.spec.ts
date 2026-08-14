import { history, undo } from 'prosemirror-history';
import { EditorState, TextSelection } from 'prosemirror-state';
import { EditorView } from 'prosemirror-view';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ELLIPSIS, EM_DASH, EN_DASH, type SmartTypographyOptions } from '../smart-typography/engine';
import { parseMarkdown } from './markdown-dialect';
import { smartTypographyPlugin } from './smart-typography-plugin';
import { compileRules, textReplacementPlugin } from './text-replacement';

/**
 * Smart typography, Part B — the ProseMirror adapter, with v4's
 * `__tests__/unit/components/chat/lexical/plugins/SmartTypographyPlugin.test.tsx`
 * (`48396682`) ported case-for-case. The engine itself is proven against v4's
 * corpus in `smart-typography/engine.spec.ts`; what is under test here is the
 * plumbing v4 rewrites for its own editor — the bails, the caret, the revert
 * window, and the interaction with Layer 1.5.
 *
 * Where v4 seeds a paragraph and dispatches KEY_DOWN_COMMAND, this drives the
 * plugin's `handleKeyDown` against a live `EditorView`, which is the same
 * shape: the plugin is what decides, and ordinary insertion is the browser's
 * job on both sides (so a keystroke the plugin declines leaves the text alone
 * in these tests, exactly as it does in v4's).
 */

const ON: SmartTypographyOptions = { dashes: true, ellipsis: true };

function makeEditor(markdown: string, options: SmartTypographyOptions | null = ON) {
  const mount = document.createElement('div');
  document.body.appendChild(mount);
  const plugin = smartTypographyPlugin(() => options);
  const state = EditorState.create({
    doc: parseMarkdown(markdown),
    plugins: [plugin, history()],
  });
  const view = new EditorView(mount, { state });
  // Caret at the end of the document's first block, which is where a writer
  // typing into it would be.
  const end = view.state.doc.resolve(1).end();
  view.dispatch(view.state.tr.setSelection(TextSelection.create(view.state.doc, end)));
  return { view, plugin, mount };
}

type Editor = ReturnType<typeof makeEditor>;

/** Press a key at the current caret. Returns whether the plugin claimed it. */
function press(
  editor: Editor,
  key: string,
  modifiers: { metaKey?: boolean; ctrlKey?: boolean; altKey?: boolean } = {},
): boolean {
  const event = new KeyboardEvent('keydown', { key, ...modifiers });
  return editor.plugin.props!.handleKeyDown!.call(editor.plugin, editor.view, event) === true;
}

/** What the browser would have done with a keystroke the plugin declined. */
function typeLiterally(editor: Editor, text: string): void {
  const { view } = editor;
  view.dispatch(view.state.tr.insertText(text, view.state.selection.$from.pos));
}

function text(editor: Editor): string {
  return editor.view.state.doc.textContent;
}

function caret(editor: Editor): number {
  return editor.view.state.selection.$from.parentOffset;
}

describe('smartTypographyPlugin', () => {
  let editors: Editor[] = [];
  const open = (markdown: string, options: SmartTypographyOptions | null = ON): Editor => {
    const editor = makeEditor(markdown, options);
    editors.push(editor);
    return editor;
  };

  beforeEach(() => {
    vi.spyOn(console, 'debug').mockImplementation(() => undefined);
  });

  afterEach(() => {
    for (const e of editors) e.view.destroy();
    editors = [];
    vi.restoreAllMocks();
  });

  describe('the dash ladder', () => {
    it('turns a second hyphen into an en dash', () => {
      const editor = open('1920-');
      expect(press(editor, '-')).toBe(true);
      expect(text(editor)).toBe(`1920${EN_DASH}`);
    });

    it('promotes the en dash to an em dash on the third hyphen', () => {
      const editor = open(`wait${EN_DASH}`);
      press(editor, '-');
      expect(text(editor)).toBe(`wait${EM_DASH}`);
    });

    it('walks the whole ladder from a bare word', () => {
      const editor = open('wait');
      // The first hyphen is ordinary typing — the plugin must not swallow it.
      expect(press(editor, '-')).toBe(false);
      typeLiterally(editor, '-');

      expect(press(editor, '-')).toBe(true);
      expect(text(editor)).toBe(`wait${EN_DASH}`);

      expect(press(editor, '-')).toBe(true);
      expect(text(editor)).toBe(`wait${EM_DASH}`);
    });

    it('stops at the em dash — a fourth hyphen is the escape hatch', () => {
      const editor = open(`wait${EM_DASH}`);
      expect(press(editor, '-')).toBe(false);
      expect(text(editor)).toBe(`wait${EM_DASH}`);
    });
  });

  describe('the ellipsis', () => {
    it('turns a third dot into an ellipsis', () => {
      const editor = open('Well..');
      expect(press(editor, '.')).toBe(true);
      expect(text(editor)).toBe(`Well${ELLIPSIS}`);
    });

    it('leaves a second dot alone', () => {
      const editor = open('Well.');
      expect(press(editor, '.')).toBe(false);
      expect(text(editor)).toBe('Well.');
    });

    it('leaves a dot after an existing ellipsis alone', () => {
      const editor = open(`Well${ELLIPSIS}`);
      press(editor, '.');
      expect(text(editor)).toBe(`Well${ELLIPSIS}`);
    });

    it('places the caret immediately after the substitution', () => {
      const editor = open('Well..');
      press(editor, '.');
      // "Well" + one ellipsis character.
      expect(caret(editor)).toBe(5);
    });
  });

  describe('bail conditions', () => {
    it('ignores a non-trigger keystroke', () => {
      const editor = open('wait-');
      expect(press(editor, 'x')).toBe(false);
      expect(text(editor)).toBe('wait-');
    });

    it('does not fire inside a fenced code block', () => {
      const editor = open('```\nnpm run build -\n```');
      expect(press(editor, '-')).toBe(false);
      expect(text(editor)).toBe('npm run build -');
    });

    it('does not fire inside inline code', () => {
      const editor = open('`--verbose..`');
      expect(press(editor, '.')).toBe(false);
      expect(text(editor)).toBe('--verbose..');
    });

    it('does not fire during IME composition', () => {
      const editor = open('wait-');
      Object.defineProperty(editor.view, 'composing', { value: true, configurable: true });
      expect(press(editor, '-')).toBe(false);
      expect(text(editor)).toBe('wait-');
    });

    it('does not fire at the start of a block', () => {
      const editor = open('');
      expect(press(editor, '-')).toBe(false);
      expect(text(editor)).toBe('');
    });

    it('does not fire on a modified trigger key (a shortcut, not typing)', () => {
      const editor = open('wait-');
      expect(press(editor, '-', { metaKey: true })).toBe(false);
      expect(press(editor, '-', { ctrlKey: true })).toBe(false);
      expect(press(editor, '-', { altKey: true })).toBe(false);
      expect(text(editor)).toBe('wait-');
    });

    it('does not fire on a non-collapsed selection', () => {
      const editor = open('wait-');
      const { view } = editor;
      view.dispatch(
        view.state.tr.setSelection(TextSelection.create(view.state.doc, 1, view.state.doc.resolve(1).end())),
      );
      expect(press(editor, '-')).toBe(false);
      expect(text(editor)).toBe('wait-');
    });
  });

  describe('per-rule gating', () => {
    it('honors dashes off / ellipsis on', () => {
      const editor = open('wait-', { dashes: false, ellipsis: true });
      expect(press(editor, '-')).toBe(false);
      const dots = open('Well..', { dashes: false, ellipsis: true });
      expect(press(dots, '.')).toBe(true);
      expect(text(dots)).toBe(`Well${ELLIPSIS}`);
    });

    it('honors ellipsis off / dashes on', () => {
      const editor = open('Well..', { dashes: true, ellipsis: false });
      expect(press(editor, '.')).toBe(false);
      const dash = open('wait-', { dashes: true, ellipsis: false });
      expect(press(dash, '-')).toBe(true);
      expect(text(dash)).toBe(`wait${EN_DASH}`);
    });

    it('does nothing at all when both are off', () => {
      const editor = open('wait-', { dashes: false, ellipsis: false });
      expect(press(editor, '-')).toBe(false);
      const dots = open('Well..', { dashes: false, ellipsis: false });
      expect(press(dots, '.')).toBe(false);
    });

    it('is inert when the host passes null (every non-composer editor)', () => {
      const editor = open('wait-', null);
      expect(press(editor, '-')).toBe(false);
      expect(text(editor)).toBe('wait-');
    });

    it('reads the options LIVE, so a settings change needs no rebuild', () => {
      let options: SmartTypographyOptions | null = { dashes: false, ellipsis: true };
      const mount = document.createElement('div');
      document.body.appendChild(mount);
      const plugin = smartTypographyPlugin(() => options);
      const state = EditorState.create({ doc: parseMarkdown('wait-'), plugins: [plugin] });
      const view = new EditorView(mount, { state });
      view.dispatch(view.state.tr.setSelection(TextSelection.create(view.state.doc, 6)));
      const editor = { view, plugin, mount };
      editors.push(editor);

      expect(press(editor, '-')).toBe(false);
      options = { dashes: true, ellipsis: true };
      expect(press(editor, '-')).toBe(true);
      expect(text(editor)).toBe(`wait${EN_DASH}`);
    });
  });

  describe('undo and escape', () => {
    it('reverts in ONE undo', () => {
      const editor = open('Well..');
      press(editor, '.');
      expect(text(editor)).toBe(`Well${ELLIPSIS}`);

      undo(editor.view.state, editor.view.dispatch);

      // v4's own expectation, byte for byte: the state BEFORE the substitution,
      // which is `Well..` — the third dot's keystroke was consumed, never
      // inserted, so an undo cannot bring it back. (Backspace is the affordance
      // that restores the full literal; see below.)
      expect(text(editor)).toBe('Well..');
    });

    it('is its OWN undo step — a run of typing before it survives the undo', () => {
      const editor = open('Well');
      // A run of ordinary typing, as one history group.
      typeLiterally(editor, '.');
      typeLiterally(editor, '.');
      expect(text(editor)).toBe('Well..');

      press(editor, '.');
      expect(text(editor)).toBe(`Well${ELLIPSIS}`);

      undo(editor.view.state, editor.view.dispatch);

      // Without `closeHistory` the substitution would merge into the adjacent
      // typing group and one undo would swallow the dots as well.
      expect(text(editor)).toBe('Well..');
    });

    it('reverts on an immediate Backspace, restoring the literal characters', () => {
      const editor = open('Well..');
      press(editor, '.');
      expect(text(editor)).toBe(`Well${ELLIPSIS}`);

      expect(press(editor, 'Backspace')).toBe(true);
      expect(text(editor)).toBe('Well...');
      expect(caret(editor)).toBe(7);
    });

    it('restores the en dash literal, not a bare hyphen', () => {
      const editor = open('1920-');
      press(editor, '-');
      press(editor, 'Backspace');
      expect(text(editor)).toBe('1920--');
    });

    it('restores what was typed before an em dash, keeping the en dash', () => {
      const editor = open(`wait${EN_DASH}`);
      press(editor, '-');
      expect(text(editor)).toBe(`wait${EM_DASH}`);

      press(editor, 'Backspace');
      // Exactly the buffer that would have existed without the substitution.
      expect(text(editor)).toBe(`wait${EN_DASH}-`);
    });

    it('does not revert an en dash the writer typed themselves', () => {
      // No substitution preceded this, so there is no memo and nothing to undo.
      const editor = open(`1920${EN_DASH}`);
      expect(press(editor, 'Backspace')).toBe(false);
      expect(text(editor)).not.toContain('--');
    });

    it('never reverts on a modifier-Backspace (that deletes a word or line)', () => {
      const editor = open('Well..');
      press(editor, '.');
      expect(press(editor, 'Backspace', { metaKey: true })).toBe(false);
      expect(text(editor)).toBe(`Well${ELLIPSIS}`);
      // ...and the window is closed, so a plain Backspace after it is ordinary.
      expect(press(editor, 'Backspace')).toBe(false);
    });

    it('forgets the substitution once anything else happens', () => {
      const editor = open('Well..');
      press(editor, '.');
      expect(text(editor)).toBe(`Well${ELLIPSIS}`);

      // Any other keystroke invalidates the memo — here, the writer typing on.
      press(editor, 'x');
      typeLiterally(editor, 'x');

      expect(press(editor, 'Backspace')).toBe(false);
      expect(text(editor)).not.toContain('...');
    });

    it('ends the window on a keystroke that changes nothing at all', () => {
      const editor = open('Well..');
      press(editor, '.');
      // A bare modifier keydown: the caret has not moved and the document is
      // untouched, so every one of the revert's OWN safety checks would still
      // pass. Only the invalidation policy — v4's "any key that is not the
      // immediate Backspace ends the revert window" — stands between the
      // writer and a surprise reversion.
      press(editor, 'Shift');

      expect(press(editor, 'Backspace')).toBe(false);
      expect(text(editor)).toBe(`Well${ELLIPSIS}`);
    });

    it('never revives a memo for a second Backspace', () => {
      const editor = open('Well..');
      press(editor, '.');
      press(editor, 'Backspace');
      expect(text(editor)).toBe('Well...');

      // A second revert would have to re-insert the ellipsis it just removed.
      expect(press(editor, 'Backspace')).toBe(false);
      expect(text(editor)).not.toContain(ELLIPSIS);
    });

    it('declines to revert once the caret has moved off the substitution', () => {
      const editor = open('Well..');
      press(editor, '.');
      const { view } = editor;
      view.dispatch(view.state.tr.setSelection(TextSelection.create(view.state.doc, 2)));
      expect(press(editor, 'Backspace')).toBe(false);
      expect(text(editor)).toBe(`Well${ELLIPSIS}`);
    });

    it('ends the revert window on blur', () => {
      const editor = open('Well..');
      press(editor, '.');
      const blur = editor.plugin.props!.handleDOMEvents!['blur']!;
      blur.call(editor.plugin, editor.view, new FocusEvent('blur'));
      expect(press(editor, 'Backspace')).toBe(false);
      expect(text(editor)).toBe(`Well${ELLIPSIS}`);
    });
  });

  describe('logging', () => {
    it('logs once per substitution and never on an ordinary keystroke', () => {
      const editor = open('Well.');
      press(editor, '.');
      expect(console.debug).not.toHaveBeenCalled();

      const second = open('Well..');
      press(second, '.');

      expect(console.debug).toHaveBeenCalledTimes(1);
      expect(vi.mocked(console.debug).mock.calls[0][0]).toContain('[smart-typography]');
      expect(vi.mocked(console.debug).mock.calls[0][0]).toContain('ellipsis');
    });
  });

  /**
   * v4's "Interaction with Layer 1.5" table, driven through the real pair of
   * plugins in the pinned `buildPlugins()` order (smart typography ABOVE text
   * replacement). `.` is a word-boundary trigger for text replacement as well,
   * so the order is the whole specification.
   */
  describe('interaction with Layer 1.5 text replacement', () => {
    const RULES = compileRules([
      {
        id: 'r1',
        fromText: 'Aris',
        toText: 'Aristarchus the Wise',
        caseSensitive: false,
        enabled: true,
        sortOrder: 0,
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-01T00:00:00.000Z',
      },
    ]);

    function pair(markdown: string) {
      const mount = document.createElement('div');
      document.body.appendChild(mount);
      const typography = smartTypographyPlugin(() => ON);
      const replacement = textReplacementPlugin(() => RULES);
      const state = EditorState.create({
        doc: parseMarkdown(markdown),
        plugins: [typography, replacement, history()],
      });
      const view = new EditorView(mount, { state });
      const end = view.state.doc.resolve(1).end();
      view.dispatch(view.state.tr.setSelection(TextSelection.create(view.state.doc, end)));
      const editor = { view, plugin: typography, mount };
      editors.push(editor);
      // The keystroke goes to typography first, then to replacement — the
      // registration order the editor installs.
      const key = (k: string): void => {
        const event = new KeyboardEvent('keydown', { key: k });
        if (typography.props!.handleKeyDown!.call(typography, view, event) === true) return;
        if (replacement.props!.handleKeyDown!.call(replacement, view, event) === true) return;
        view.dispatch(view.state.tr.insertText(k, view.state.selection.$from.pos));
      };
      return { editor, key };
    }

    it('resolves `Aris...` as replacement, then replacement-miss, then ellipsis', () => {
      const { editor, key } = pair('Aris');
      key('.');
      expect(text(editor)).toBe('Aristarchus the Wise.');
      key('.');
      expect(text(editor)).toBe('Aristarchus the Wise..');
      key('.');
      expect(text(editor)).toBe(`Aristarchus the Wise${ELLIPSIS}`);
    });

    it('leaves a plain text replacement untouched', () => {
      const { editor, key } = pair('Aris');
      key(' ');
      expect(text(editor)).toBe('Aristarchus the Wise ');
    });
  });
});
