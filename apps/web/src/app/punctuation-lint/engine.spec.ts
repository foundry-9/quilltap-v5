import { beforeAll, describe, expect, it } from 'vitest';

import {
  classifyProse,
  lintPunctuation,
  PUNCTUATION_RULE_MESSAGES,
  type LintCaret,
  type ProseSpan,
  type PunctuationIssue,
} from './engine';
import vectors from './fixtures/punctuation-vectors.json';
import proseVectors from './fixtures/prose-vectors.json';

/**
 * Punctuation warnings engine — the v5 half of the contract, transcribed case
 * for case from v4 `lib/punctuation-lint/__tests__/engine.test.ts` at
 * `236c04e35` (P4.D352).
 *
 * Both corpora (`fixtures/punctuation-vectors.json`, `fixtures/prose-vectors.json`)
 * are copied from v4 BYTE-FOR-BYTE ("quilltap-v5 copies this byte-for-byte and
 * asserts identical output"): if the port disagrees with a row, the port is
 * what gets fixed. `engine.oracle.spec.ts` adds the corpus recorded out of v4's
 * real engine.
 */

interface VectorCase {
  name: string;
  blocks: (string | null)[];
  caret?: LintCaret;
  expect: PunctuationIssue[];
}

/** The slice of `node:fs` the no-imports case reads. */
interface NodeFs {
  readFileSync(path: string, encoding: 'utf8'): string;
}

let fs: NodeFs;

beforeAll(async () => {
  // Loaded at RUN time through a non-literal specifier the browser-platform
  // bundler leaves alone (the `inventory-icon.spec.ts` mechanism).
  const specifier = 'node:fs';
  fs = (await import(/* @vite-ignore */ specifier)) as NodeFs;
});

/** A path under `apps/web`, from the runner's working directory. */
function webPath(rel: string): string {
  const cwd = (globalThis as unknown as { process: { cwd(): string } }).process.cwd();
  return `${cwd.replace(/\\/g, '/')}/${rel}`;
}

describe('lintPunctuation', () => {
  describe('corpus (punctuation-lint/fixtures/punctuation-vectors.json — v4 byte-copy)', () => {
    const cases = vectors.cases as unknown as VectorCase[];

    it('has a corpus to run', () => {
      // v4 asserts `> 20`; the byte-copy at 236c04e35 carries exactly 35, so a
      // truncated or mangled import cannot pass the it.each below vacuously.
      expect(cases.length).toBe(35);
    });

    it.each(cases.map((c) => [c.name, c] as const))('%s', (_name, testCase) => {
      expect(lintPunctuation(testCase.blocks, { caret: testCase.caret ?? null })).toEqual(
        testCase.expect,
      );
    });
  });

  it('has a message for every rule it can return', () => {
    const cases = vectors.cases as unknown as VectorCase[];
    for (const c of cases) {
      for (const issue of c.expect) expect(PUNCTUATION_RULE_MESSAGES[issue.rule]).toBeTruthy();
    }
  });

  it('reports offsets relative to the block, not the paragraph', () => {
    const issues = lintPunctuation(['Fine.\n\nShe said "hi "  there.']);
    expect(issues).toHaveLength(1);
    expect(issues[0].start).toBe('Fine.\n\nShe said "hi '.length);
  });

  it('never flags a mark inside a null (code) block', () => {
    expect(lintPunctuation([null, null])).toEqual([]);
  });

  it('without a caret, a quote typed last is judged as it stands', () => {
    // The adapter passes no caret when the editor has no selection; the same
    // text with a caret after the quote is the "still typing" case.
    expect(lintPunctuation(['He said "'])).toEqual([
      { block: 0, start: 8, end: 9, rule: 'floating-quote' },
    ]);
    expect(lintPunctuation(['He said "'], { caret: { block: 0, offset: 9 } })).toEqual([]);
  });
});

describe('prose mode', () => {
  interface ProseLintCase {
    name: string;
    blocks: (string | null)[];
    caret?: LintCaret;
    expect: PunctuationIssue[];
  }
  interface ProseClassifyCase {
    name: string;
    blocks: (string | null)[];
    expect: ProseSpan[];
  }

  describe('corpus (punctuation-lint/fixtures/prose-vectors.json — v4 byte-copy)', () => {
    const lintCases = proseVectors.lint as unknown as ProseLintCase[];
    const classifyCases = proseVectors.classify as unknown as ProseClassifyCase[];

    it('has both tables to run', () => {
      expect(lintCases.length).toBe(10);
      expect(classifyCases.length).toBe(8);
    });

    it.each(lintCases.map((c) => [c.name, c] as const))('lint: %s', (_name, testCase) => {
      expect(
        lintPunctuation(testCase.blocks, { caret: testCase.caret ?? null, proseMode: true }),
      ).toEqual(testCase.expect);
    });

    it.each(classifyCases.map((c) => [c.name, c] as const))('classify: %s', (_name, testCase) => {
      expect(classifyProse(testCase.blocks)).toEqual(testCase.expect);
    });
  });

  it('never flags a missing dialogue outside prose mode', () => {
    expect(lintPunctuation(['She set the cup down.'])).toEqual([]);
  });

  it('has a message for the no-dialogue rule', () => {
    expect(PUNCTUATION_RULE_MESSAGES['no-dialogue']).toBeTruthy();
  });
});

describe('the engine imports nothing (v4 eslint.config.mjs:124-129 no-restricted-imports, as a spec)', () => {
  // v4 fences lib/punctuation-lint/** with an ESLint override; the SPA has no
  // ESLint config to grow, so the same contract is asserted over the raw source.
  it.each(['engine.ts', 'prose-mode.ts'])('%s has no import statement', (file) => {
    const source = fs.readFileSync(webPath(`src/app/punctuation-lint/${file}`), 'utf8');
    expect(source.length).toBeGreaterThan(500);
    expect(source).not.toMatch(/^\s*import\s/m);
  });
});
