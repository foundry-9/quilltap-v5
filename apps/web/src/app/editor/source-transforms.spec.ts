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

/**
 * The `indent`/`outdent` dispatch wiring (P4.D40, v4 `handleIndentClick`'s
 * source arm, `FormattingToolbar.tsx:315-331`). The underlying
 * `detectListIndentUnit`/`indentSourceLines`/`outdentSourceLines` functions
 * are already differential-tested against v4's real module
 * (`list-indentation.oracle.spec.ts`); these cases pin the WIRING —
 * `applySourceFormat` reads the unit off `state.value` itself, not a fixed
 * default, matching v4's `detectListIndentUnit(textarea.value)` call.
 */
describe('source-mode transforms — indent/outdent wiring', () => {
  it('indents the caret line at the document\'s own (non-default) unit', () => {
    const value = '- a\n   - b\n- c'; // 3-space document
    const result = applySourceFormat({ value, start: 12, end: 12 }, { kind: 'indent' });
    expect(result.value).toBe('- a\n   - b\n   - c');
  });

  it('outdents the caret line at the document\'s own (non-default) unit', () => {
    const value = '- a\n    - b\n        - c'; // 4-space document, two levels deep
    const result = applySourceFormat({ value, start: 20, end: 20 }, { kind: 'outdent' });
    expect(result.value).toBe('- a\n    - b\n    - c');
  });

  it('leaves a non-list line alone on indent', () => {
    const value = 'prose\n- a';
    const result = applySourceFormat({ value, start: 0, end: 0 }, { kind: 'indent' });
    expect(result.value).toBe(value);
  });
});
