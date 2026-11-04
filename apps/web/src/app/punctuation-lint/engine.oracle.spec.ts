import { describe, expect, it } from 'vitest';

import { classifyProse, lintPunctuation, type LintCaret } from './engine';
import V4 from './engine.v4.json';
import { isProseTemplate, type ProseTemplateShape } from './prose-mode';

/**
 * The executed-v4 oracle for the punctuation-lint engine (P4.D352, ruling R-1).
 *
 * `engine.v4.json` is emitted by RUNNING v4's real `lib/punctuation-lint/
 * engine.ts` + `prose-mode.ts` at the round pin
 * (`harness/oracle/cases/punctuation-lint-engine.mjs`) over both committed
 * vector corpora (the recorder asserts they agree with v4's executed answers),
 * a generated corpus of drafts linted with no caret and with the caret at every
 * offset in and out of prose mode, `classifyProse` over each draft, and
 * `isProseTemplate` over Novel's delimiter set and its near misses. Every
 * answer is diffed field by field.
 *
 * Regen recipe (the recorder reads through `git show` at the pin):
 *
 * ```bash
 * export PATH=~/.nvm/versions/node/v24.13.1/bin:$PATH
 * QT_V4_PIN=236c04e35 node ~/source/quilltap-v5/harness/oracle/cases/punctuation-lint-engine.mjs \
 *   > ~/source/quilltap-v5/apps/web/src/app/punctuation-lint/engine.v4.json
 * ```
 */

type Row =
  | {
      id: string;
      kind: 'lint';
      input: { blocks: (string | null)[]; caret: LintCaret | null; proseMode: boolean };
      out: unknown;
    }
  | { id: string; kind: 'classify'; input: { blocks: (string | null)[] }; out: unknown }
  | {
      id: string;
      kind: 'prose';
      input: { template: ProseTemplateShape | null | '__undefined__' };
      out: boolean;
    };

const ROWS = V4.rows as unknown as Row[];

describe('punctuation-lint agrees with v4 lib/punctuation-lint row for row', () => {
  it('carries the whole recorded corpus, at the round pin, from the engine v5 copied', () => {
    // A truncated fixture would make the it.each below vacuously green.
    expect(ROWS).toHaveLength(1214);
    expect(V4._source.pin).toBe('236c04e35');
    expect(V4._source.engineMd5).toBe('67b5ffa315c8c15928fb473b406e5b2e');
    expect(V4._source.proseModeMd5).toBe('029100cfb6877793f2f4607ab2b07514');
    expect(ROWS.filter((r) => r.kind === 'lint')).toHaveLength(1163);
    expect(ROWS.filter((r) => r.kind === 'classify')).toHaveLength(38);
    expect(ROWS.filter((r) => r.kind === 'prose')).toHaveLength(13);
  });

  it.each(ROWS.map((r, i) => ({ ...r, n: i })))('#$n $id', (row) => {
    switch (row.kind) {
      case 'lint':
        expect(
          lintPunctuation(row.input.blocks, {
            caret: row.input.caret,
            proseMode: row.input.proseMode,
          }),
        ).toEqual(row.out);
        break;
      case 'classify':
        expect(classifyProse(row.input.blocks)).toEqual(row.out);
        break;
      case 'prose': {
        const t = row.input.template;
        expect(isProseTemplate(t === '__undefined__' ? undefined : t)).toBe(row.out);
        break;
      }
    }
  });
});
