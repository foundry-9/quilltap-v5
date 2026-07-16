import { describe, expect, it } from 'vitest';

import vectors from './__fixtures__/text-transforms-vectors.json';
import { applySourceFormat } from './source-transforms';
import type { FormatAction } from './formatting-toolbar';

/**
 * The source-mode transform differential: byte-for-byte parity with vectors
 * RECORDED from v4's REAL `FormattingToolbar` rendered in jsdom over a real
 * `<textarea>`, wired exactly as v4's `MarkdownLexicalEditor` wires it in source
 * mode. Every expected value below was produced by v4's own code — the recorder
 * is `harness/oracle/cases/text-transforms.test.ts`, whose header carries the
 * regen recipe.
 *
 * If a case fails, fix `source-transforms.ts`, not the vectors.
 */

/** The recorder's `qt-formatting-button-<type>` suffix → the v5 FormatAction. */
function actionFor(button: string): FormatAction {
  if (button === 'code-block') return { kind: 'codeBlock' };
  if (/^h[1-6]$/.test(button)) return { kind: 'heading', level: Number(button[1]) };
  return { kind: button } as FormatAction;
}

describe('source-mode transforms — v4 recorded-vector parity', () => {
  for (const v of vectors) {
    it(`matches v4: ${v.name}`, () => {
      const result = applySourceFormat(
        { value: v.in.value, start: v.in.start, end: v.in.end },
        actionFor(v.button),
      );
      expect(result.value).toBe(v.out.value);
      expect(result.cursor).toBe(v.out.cursor);
    });
  }

  it('covers every toolbar button kind', () => {
    expect(new Set(vectors.map((v) => v.button))).toEqual(
      new Set(['bold', 'italic', 'h1', 'h2', 'h3', 'h4', 'h6', 'ul', 'ol', 'blockquote', 'code-block']),
    );
  });
});
