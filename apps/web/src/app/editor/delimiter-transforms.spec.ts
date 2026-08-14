import { EditorState, TextSelection } from 'prosemirror-state';
import { describe, expect, it } from 'vitest';

import oracle from './__fixtures__/delimiter-oracle.json';
import {
  applyDelimiterCommand,
  applySourceDelimiter,
  getDelimiterTooltip,
  visibleDelimiters,
} from './delimiter-transforms';
import { dialectSchema, serializeMarkdown } from './markdown-dialect';
import type { NarrationDelimiters, TemplateDelimiter } from '../core/core-contract';

/**
 * The roleplay-delimiter differential (P4.9L). Every expectation below was
 * RECORDED from v4's own code — `getDelimiterTooltip` and the pure transforms
 * called directly, the source-mode branch by clicking the REAL
 * `FormattingToolbar` rendered in jsdom, the narration synthesis by reading the
 * rendered buttons off the DOM, and the rich-text branch by dispatching v4's
 * REAL `APPLY_DELIMITER_COMMAND` into a headless Lexical editor and exporting
 * composer markdown. The recorder + its regen recipe live in
 * `apps/web/oracle/delimiter-transforms.test.ts`.
 *
 * If a case fails, fix `delimiter-transforms.ts`, not the vectors.
 *
 * @module editor/delimiter-transforms.spec
 */

interface TooltipVector {
  name: string;
  delimiter: TemplateDelimiter;
  tooltip: string;
}
interface SourceVector {
  name: string;
  delimiter: TemplateDelimiter;
  narration: NarrationDelimiters | null;
  in: { value: string; start: number; end: number };
  out: { value: string; cursor: number };
}
interface ButtonListVector {
  name: string;
  template: TemplateDelimiter[];
  narration: NarrationDelimiters | null;
  buttons: { label: string; title: string; className: string }[];
  dividers: number;
}
interface RichVector {
  name: string;
  delimiter: TemplateDelimiter;
  in: {
    shape: 'paragraph' | 'twoParagraphs' | 'codeBlock' | 'boldRun';
    text: string;
    second: string | null;
    start: number;
    end: number;
  };
  out: { markdown: string; anchorOffset: number | null; applied: boolean };
}

const vectors = oracle as unknown as {
  tooltips: TooltipVector[];
  source: SourceVector[];
  buttonLists: ButtonListVector[];
  rich: RichVector[];
};

describe('delimiter tooltips — v4 recorded-vector parity', () => {
  for (const v of vectors.tooltips) {
    it(`matches v4: ${v.name}`, () => {
      expect(getDelimiterTooltip(v.delimiter)).toBe(v.tooltip);
    });
  }

  it('covers every delimiter kind', () => {
    expect(new Set(vectors.tooltips.map((v) => v.delimiter.kind))).toEqual(
      new Set(['wrap', 'linePrefix', 'tagPrefix']),
    );
  });
});

describe('source-mode delimiter transforms — v4 recorded-vector parity', () => {
  for (const v of vectors.source) {
    it(`matches v4: ${v.name}`, () => {
      const result = applySourceDelimiter(
        { value: v.in.value, start: v.in.start, end: v.in.end },
        v.delimiter,
      );
      expect(result.value).toBe(v.out.value);
      expect(result.cursor).toBe(v.out.cursor);
    });
  }

  it('covers every delimiter kind', () => {
    expect(new Set(vectors.source.map((v) => v.delimiter.kind))).toEqual(
      new Set(['wrap', 'linePrefix', 'tagPrefix']),
    );
  });
});

describe('the narration button and its dedupe — v4 recorded-vector parity', () => {
  for (const v of vectors.buttonLists) {
    it(`matches v4: ${v.name}`, () => {
      const shown = visibleDelimiters(v.template, v.narration);
      expect(shown.map((d) => d.buttonName)).toEqual(v.buttons.map((b) => b.label));
      expect(shown.map((d) => getDelimiterTooltip(d))).toEqual(v.buttons.map((b) => b.title));
    });
  }

  it('records at least one case where the dedupe fires and one where it does not', () => {
    const sizes = vectors.buttonLists.map((v) => v.buttons.length);
    expect(new Set(sizes).size).toBeGreaterThan(1);
  });
});

// ---------------------------------------------------------------------------
// The rich-text branch
// ---------------------------------------------------------------------------

/**
 * Rebuild the oracle's seeded document as a ProseMirror state. The shapes come
 * from the recorder's four seeders; the two frames share a single coordinate
 * system inside one textblock, which is why the caret is compared as an offset
 * from the block start rather than as a raw position.
 */
function stateFor(v: RichVector): EditorState {
  const s = dialectSchema;
  const p = (text: string) => s.nodes['paragraph'].create(null, text ? s.text(text) : undefined);

  switch (v.in.shape) {
    case 'twoParagraphs': {
      const doc = s.nodes['doc'].create(null, [p(v.in.text), p(v.in.second ?? '')]);
      const state = EditorState.create({ doc, schema: s });
      // p1 text starts at 1; p2 opens at `1 + len1 + 1`, its text at +1 again.
      const from = 1 + v.in.start;
      const to = v.in.text.length + 3 + v.in.end;
      return state.apply(state.tr.setSelection(TextSelection.create(doc, from, to)));
    }
    case 'codeBlock': {
      const doc = s.nodes['doc'].create(null, [
        s.nodes['code_block'].create(null, v.in.text ? s.text(v.in.text) : undefined),
      ]);
      const state = EditorState.create({ doc, schema: s });
      return state.apply(
        state.tr.setSelection(TextSelection.create(doc, 1 + v.in.start, 1 + v.in.end)),
      );
    }
    case 'boldRun': {
      const bold = s.text(v.in.text, [s.marks['strong'].create()]);
      const rest = v.in.second ? [s.text(v.in.second)] : [];
      const doc = s.nodes['doc'].create(null, [
        s.nodes['paragraph'].create(null, [bold, ...rest]),
      ]);
      const state = EditorState.create({ doc, schema: s });
      return state.apply(
        state.tr.setSelection(TextSelection.create(doc, 1 + v.in.start, 1 + v.in.end)),
      );
    }
    default: {
      const doc = s.nodes['doc'].create(null, [p(v.in.text)]);
      const state = EditorState.create({ doc, schema: s });
      return state.apply(
        state.tr.setSelection(TextSelection.create(doc, 1 + v.in.start, 1 + v.in.end)),
      );
    }
  }
}

describe('rich-text delimiter application — v4 recorded-vector parity', () => {
  for (const v of vectors.rich) {
    it(`matches v4: ${v.name}`, () => {
      let next = stateFor(v);
      const applied = applyDelimiterCommand(v.delimiter)(next, (tr) => {
        next = next.apply(tr);
      });

      expect(applied).toBe(v.out.applied);
      expect(serializeMarkdown(next.doc)).toBe(v.out.markdown);

      // The caret, as an offset from the caret block's start — the only frame
      // the Lexical and ProseMirror coordinate systems share. A selection that
      // spanned two blocks is skipped: v4 lands it inside whichever text node
      // `insertRawText` left it in, which is a mechanism detail, not a contract
      // (the exported bytes above ARE the contract).
      if (v.in.shape !== 'twoParagraphs' && v.out.anchorOffset !== null) {
        expect(next.selection.from - next.selection.$from.start()).toBe(v.out.anchorOffset);
      }
    });
  }

  it('covers a refusal, a mark-dropping insert, and a multi-block wrap', () => {
    const shapes = new Set(vectors.rich.map((v) => v.in.shape));
    expect(shapes).toEqual(
      new Set(['paragraph', 'twoParagraphs', 'codeBlock', 'boldRun']),
    );
    expect(vectors.rich.some((v) => !v.out.applied)).toBe(true);
  });
});
