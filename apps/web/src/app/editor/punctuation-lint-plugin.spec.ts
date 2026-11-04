import type { Node as PMNode } from 'prosemirror-model';
import { EditorState, TextSelection } from 'prosemirror-state';
import { EditorView } from 'prosemirror-view';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import './jsdom-range-shim';
import { PUNCTUATION_RULE_MESSAGES } from '../punctuation-lint/engine';
import { dialectSchema, serializeMarkdown } from './markdown-dialect';
import {
  HIGHLIGHT_PRIORITY,
  NARRATION_HIGHLIGHT_NAME,
  NO_DIALOGUE_HIGHLIGHT_NAME,
  OOC_HIGHLIGHT_NAME,
  PUNCTUATION_HIGHLIGHT_NAME,
  PUNCTUATION_LINT_REFRESH,
  punctuationLintKey,
  punctuationLintPlugin,
  type PunctuationLintInputs,
} from './punctuation-lint-plugin';

/**
 * Composer punctuation warnings — the ProseMirror adapter (P4.D352), with v4's
 * `__tests__/unit/components/chat/lexical/plugins/PunctuationLintPlugin.test.tsx`
 * (236c04e35) ported case for case. The rules are pinned by the engine's corpus
 * (`punctuation-lint/*.spec.ts`), so this suite proves only what the adapter
 * adds: that the editor is turned into the right blocks, that the painted
 * ranges land on the right characters, that the text is never touched, and that
 * the setting turns it off.
 *
 * Where v4 reads ranges out of a fake `CSS.highlights` registry, this reads the
 * plugin's `DecorationSet` (the characters each decoration covers) and, for the
 * hover and nesting cases, the spans ProseMirror renders into the view.
 */

const S = dialectSchema;
const ON: PunctuationLintInputs = { enabled: true, proseMode: false };

interface Harness {
  view: EditorView;
  mount: HTMLElement;
  inputs: { current: PunctuationLintInputs | null };
}

let harnesses: Harness[] = [];

function open(inputs: PunctuationLintInputs | null = ON, doc?: PMNode): Harness {
  const mount = document.createElement('div');
  document.body.appendChild(mount);
  const box = { current: inputs };
  const state = EditorState.create({
    doc: doc ?? S.node('doc', null, [S.node('paragraph')]),
    plugins: [punctuationLintPlugin(() => box.current)],
  });
  const view = new EditorView(mount, { state });
  const h = { view, mount, inputs: box };
  harnesses.push(h);
  return h;
}

/** v4 `seedParagraph`: one paragraph holding `text`, the caret at its end. */
function seedParagraph(h: Harness, text: string): void {
  const doc = S.node('doc', null, [S.node('paragraph', null, text ? [S.text(text)] : [])]);
  const tr = h.view.state.tr.replaceWith(0, h.view.state.doc.content.size, doc.content);
  const end = tr.doc.resolve(1).end();
  h.view.dispatch(tr.setSelection(TextSelection.create(tr.doc, end)));
}

/** v4 `seedCodeBlock`: one code block holding `text`, the caret at its end. */
function seedCodeBlock(h: Harness, text: string): void {
  const doc = S.node('doc', null, [S.node('code_block', null, [S.text(text)])]);
  const tr = h.view.state.tr.replaceWith(0, h.view.state.doc.content.size, doc.content);
  const end = tr.doc.resolve(1).end();
  h.view.dispatch(tr.setSelection(TextSelection.create(tr.doc, end)));
}

/** The decorations of one class name, in document order (the plugin tags each with its name). */
function decorated(h: Harness, name: string = PUNCTUATION_HIGHLIGHT_NAME) {
  const set = punctuationLintKey.getState(h.view.state)!.decorations;
  return set
    .find(undefined, undefined, (spec: { name?: string }) => spec.name === name)
    .sort((a, b) => a.from - b.from);
}

/** The characters currently decorated under one class name, in document order. */
function paintedText(h: Harness, name: string = PUNCTUATION_HIGHLIGHT_NAME): string[] {
  return decorated(h, name).map((d) => h.view.state.doc.textBetween(d.from, d.to));
}

describe('punctuationLintPlugin', () => {
  let debug: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    debug = vi.spyOn(console, 'debug').mockImplementation(() => undefined);
  });

  afterEach(() => {
    for (const h of harnesses) {
      h.view.destroy();
      h.mount.remove();
    }
    harnesses = [];
    vi.restoreAllMocks();
  });

  it('paints a double quote opened inside open dialogue, and nothing else', () => {
    const h = open();
    seedParagraph(h, '"He told me "go away" and left."');
    expect(paintedText(h)).toEqual(['"', '"']);
    // Block offsets 12 and 20 — the paragraph's content starts at position 1.
    expect(decorated(h).map((d) => d.from)).toEqual([1 + 12, 1 + 20]);
  });

  it('never changes the text', () => {
    const h = open();
    const text = '"He told me "go away" and *left.';
    seedParagraph(h, text);
    const before = h.view.state.doc;
    expect(paintedText(h).length).toBeGreaterThan(0);
    expect(h.view.state.doc.textContent).toBe(text);
    expect(h.view.state.doc.eq(before)).toBe(true);
    expect(serializeMarkdown(h.view.state.doc)).toBe(serializeMarkdown(before));
  });

  it('clears the paint once the mistake is fixed', () => {
    const h = open();
    seedParagraph(h, '"He told me "go away" and left."');
    expect(paintedText(h)).toHaveLength(2);
    seedParagraph(h, '"He told me \'go away\' and left."');
    expect(paintedText(h)).toEqual([]);
    expect(h.view.dom.querySelector(`.${PUNCTUATION_HIGHLIGHT_NAME}`)).toBeNull();
  });

  it('does not flag an opener the caret has just typed', () => {
    const h = open();
    seedParagraph(h, 'He said "');
    expect(paintedText(h)).toEqual([]);
  });

  it('maps offsets across text nodes and soft line breaks', () => {
    const doc = S.node('doc', null, [
      S.node('paragraph', null, [
        S.text('"Bold', [S.marks['strong'].create()]),
        S.node('hard_break'),
        S.text('then "nested" and done."'),
      ]),
    ]);
    const h = open(ON, doc);
    // v4 `root.selectStart()`.
    h.view.dispatch(h.view.state.tr.setSelection(TextSelection.create(h.view.state.doc, 1)));
    // The second text node starts after `"Bold` (5) and the break (1).
    const secondText = 1 + 5 + 1;
    expect(decorated(h).map((d) => d.from - secondText)).toEqual([5, 12]);
    expect(paintedText(h)).toEqual(['"', '"']);
  });

  it('leaves code blocks alone', () => {
    const h = open();
    seedCodeBlock(h, 'echo "a "b" c"');
    expect(paintedText(h)).toEqual([]);
    // …and inline code too: a masked run the engine cannot be misled by.
    const doc = S.node('doc', null, [
      S.node('paragraph', null, [
        S.text('run `', []),
        S.text('a "b" c', [S.marks['code'].create()]),
      ]),
    ]);
    const h2 = open(ON, doc);
    expect(paintedText(h2)).toEqual([]);
  });

  it('paints nothing when the setting is off', () => {
    const h = open({ enabled: false, proseMode: false });
    seedParagraph(h, '"He told me "go away" and left."');
    expect(paintedText(h)).toEqual([]);
  });

  describe('prose mode', () => {
    it('paints nothing of the kind outside prose mode', () => {
      const h = open();
      seedParagraph(h, 'She waits. ((brb))');
      expect(paintedText(h, NO_DIALOGUE_HIGHLIGHT_NAME)).toEqual([]);
      expect(paintedText(h, NARRATION_HIGHLIGHT_NAME)).toEqual([]);
      expect(paintedText(h, OOC_HIGHLIGHT_NAME)).toEqual([]);
    });

    it('previews narration and asides, and flags a draft with no dialogue', () => {
      const h = open({ enabled: true, proseMode: true });
      seedParagraph(h, 'You came, Theo said. ((brb))');
      expect(paintedText(h, NARRATION_HIGHLIGHT_NAME)).toEqual(['You came, Theo said.']);
      expect(paintedText(h, OOC_HIGHLIGHT_NAME)).toEqual(['((brb))']);
      expect(paintedText(h, NO_DIALOGUE_HIGHLIGHT_NAME)).toEqual(['You came, Theo said.']);
    });

    it('drops the no-dialogue tint once a quote opens, keeping the preview', () => {
      const h = open({ enabled: true, proseMode: true });
      seedParagraph(h, '"You came," Theo said.');
      expect(paintedText(h, NO_DIALOGUE_HIGHLIGHT_NAME)).toEqual([]);
      expect(paintedText(h, NARRATION_HIGHLIGHT_NAME)).toEqual(['Theo said.']);
    });

    it('keeps the preview but drops the tint when warnings are off', () => {
      const h = open({ enabled: false, proseMode: true });
      seedParagraph(h, 'She waits.');
      expect(paintedText(h, NO_DIALOGUE_HIGHLIGHT_NAME)).toEqual([]);
      expect(paintedText(h, NARRATION_HIGHLIGHT_NAME)).toEqual(['She waits.']);
    });

    it('ranks a specific warning above the tint above the preview', () => {
      expect(HIGHLIGHT_PRIORITY[PUNCTUATION_HIGHLIGHT_NAME]).toBeGreaterThan(
        HIGHLIGHT_PRIORITY[NO_DIALOGUE_HIGHLIGHT_NAME],
      );
      expect(HIGHLIGHT_PRIORITY[NO_DIALOGUE_HIGHLIGHT_NAME]).toBeGreaterThan(
        HIGHLIGHT_PRIORITY[OOC_HIGHLIGHT_NAME],
      );
      expect(HIGHLIGHT_PRIORITY[OOC_HIGHLIGHT_NAME]).toBeGreaterThan(
        HIGHLIGHT_PRIORITY[NARRATION_HIGHLIGHT_NAME],
      );
      // v4's draft: the stray `"` (a specific warning) sits inside the narrated
      // run, which also wears the no-dialogue tint and the narration preview.
      const h = open({ enabled: true, proseMode: true });
      seedParagraph(h, 'She waits." ((brb))');
      expect(paintedText(h)).toEqual(['"']);
      expect(paintedText(h, NO_DIALOGUE_HIGHLIGHT_NAME)).toEqual(['She waits."']);
      expect(paintedText(h, NARRATION_HIGHLIGHT_NAME)).toEqual(['She waits."']);
      expect(paintedText(h, OOC_HIGHLIGHT_NAME)).toEqual(['((brb))']);
      // In the view (measured): ProseMirror MERGES inline decorations over the
      // same run into ONE span carrying every class, and the attribute added
      // last — the warning's `title`, since decorations go in ascending
      // priority — is the one it keeps. The stylesheet declares the four rules
      // in the same ascending order, so where they disagree the warning's
      // declarations win the cascade: `Highlight.priority`, by other means.
      const warning = h.view.dom.querySelector<HTMLElement>(`span.${PUNCTUATION_HIGHLIGHT_NAME}`)!;
      expect(warning.textContent).toBe('"');
      expect([...warning.classList].sort()).toEqual(
        [NARRATION_HIGHLIGHT_NAME, NO_DIALOGUE_HIGHLIGHT_NAME, PUNCTUATION_HIGHLIGHT_NAME].sort(),
      );
      expect(warning.getAttribute('title')).toBe(PUNCTUATION_RULE_MESSAGES['stray-closing-quote']);
      expect(warning.querySelector('span')).toBeNull();
    });
  });

  // --- v5-authored --------------------------------------------------------

  it('the hover explanation is the warning span’s own title, byte for byte', () => {
    const h = open({ enabled: true, proseMode: true });
    seedParagraph(h, 'She waits." ((brb))');
    const warning = h.view.dom.querySelector<HTMLElement>(`span.${PUNCTUATION_HIGHLIGHT_NAME}`)!;
    expect(warning.getAttribute('title')).toBe(PUNCTUATION_RULE_MESSAGES['stray-closing-quote']);
    const tint = h.view.dom.querySelector<HTMLElement>(`span.${NO_DIALOGUE_HIGHLIGHT_NAME}`)!;
    expect(tint.getAttribute('title')).toBe(PUNCTUATION_RULE_MESSAGES['no-dialogue']);
    // The preview is not a warning: it explains nothing.
    const preview = [...h.view.dom.querySelectorAll<HTMLElement>(`span.${OOC_HIGHLIGHT_NAME}`)];
    expect(preview.length).toBeGreaterThan(0);
    for (const el of preview) expect(el.getAttribute('title')).toBeNull();
  });

  it('logs mounted once and the warning count only when it changes', () => {
    const h = open();
    expect(debug).toHaveBeenCalledWith('[punctuation-lint] mounted', {
      warnings: true,
      proseMode: false,
    });
    expect(debug).toHaveBeenCalledWith('[punctuation-lint] 0 warning(s)');
    seedParagraph(h, 'She said "he said "x');
    const countLines = () =>
      debug.mock.calls.filter((c: unknown[]) => /warning\(s\)$/.test(String(c[0]))).length;
    const afterFirst = countLines();
    // Three more characters that keep the count: no new line.
    for (const ch of ['y', 'z', 'w']) {
      const { from } = h.view.state.selection;
      h.view.dispatch(h.view.state.tr.insertText(ch, from));
    }
    expect(countLines()).toBe(afterFirst);
    expect(
      debug.mock.calls.filter((c: unknown[]) => c[0] === '[punctuation-lint] mounted'),
    ).toHaveLength(1);
  });

  it('a null input paints nothing and logs nothing', () => {
    const h = open(null);
    seedParagraph(h, '"He told me "go away" and left."');
    expect(paintedText(h)).toEqual([]);
    expect(debug).not.toHaveBeenCalled();
  });

  it('repaints on an input flip with no keystroke (the refresh meta)', () => {
    const h = open({ enabled: false, proseMode: false });
    seedParagraph(h, '"He told me "go away" and left."');
    expect(paintedText(h)).toEqual([]);
    h.inputs.current = ON;
    h.view.dispatch(h.view.state.tr.setMeta(punctuationLintKey, PUNCTUATION_LINT_REFRESH));
    expect(paintedText(h)).toEqual(['"', '"']);
    expect(debug).toHaveBeenCalledWith('[punctuation-lint] mounted', {
      warnings: true,
      proseMode: false,
    });
    expect(debug).toHaveBeenCalledWith('[punctuation-lint] 2 warning(s)');
  });
});
