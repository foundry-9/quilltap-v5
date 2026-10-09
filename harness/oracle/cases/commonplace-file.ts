/**
 * Tier-1 oracle — v4's Commonplace vault-file leaf (`lib/memory/commonplace-
 * file.ts`, v4 `d58548051`, 166 lines, pure) through its REAL exports (P4.D265),
 * so v5's `quilltap_core::commonplace_file` is diffed rather than trusted.
 *
 * Four functions + four constants:
 *   - `commonplacePathFor('self' | {id, name})` — the `sanitizeFileName` stem
 *     (`lib/mount-index/character-vault.ts`), the `Self` collision suffix
 *     (` (<id[0..8]>)`, case-insensitive);
 *   - `commonplaceFileToCanonText(raw)` — frontmatter strip, `/^#\s/` heading
 *     drop (`#h` and `## h` survive, the regex wants a space after ONE `#`),
 *     JS `trim`;
 *   - `truncateToTokenBudget(text, budget)` — whole lines while they fit, else
 *     a proportional UTF-16 cut + `…`;
 *   - `renderCommonplaceFile(input)` — the stable `reinforcedImportance` DESC
 *     sort, the 1,500-token budget with the first entry ALWAYS written, the
 *     `\s*\n\s*` → ` ` collapse (JS `\s` is the Unicode whitespace set: NBSP,
 *     U+2028 …), the episodic `[YYYY-MM-DD]` prefix (a UTF-16 slice), and the
 *     `JSON.stringify` frontmatter scalars.
 *
 * Output: one NDJSON line `{case: "commonplace-file", constants, paths: [{in,
 * out}], canon: [{in, out}], truncate: [{text, budget, out}], render: [{in,
 * out}]}` (`undefined` inputs are written as `null` — the corpus passes none).
 *
 * ⚠ Lone surrogates. A UTF-16 cut through a surrogate pair (the sanitizer's
 * 100-unit slice, the episodic 10-unit date slice, the proportional cut) leaves
 * JS a LONE surrogate, which `JSON.stringify` writes as a `\udXXX` escape that
 * no Rust `String` (and no `serde_json` parse) can hold. Every string in the
 * output therefore goes through `toWellFormed()` — lone surrogate → U+FFFD,
 * exactly what v5's `String::from_utf16_lossy` cuts produce — and each such
 * string is ALSO listed under `loneSurrogates` (its JSON path), so the
 * representational difference is recorded by the oracle, never silent.
 *
 * Run (Node 24, from the v4 checkout or the round's pin):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   rm -f /tmp/oracle-commonplace-file.ndjson
 *   cd ~/source/quilltap-server
 *   $N/npx tsx $V5W/harness/oracle/cases/commonplace-file.ts > /tmp/oracle-commonplace-file.ndjson
 */

import * as cf from '@/lib/memory/commonplace-file';

const ID = 'aaaaaaaa-1111-4111-8111-aaaaaaaaaaaa';

type Subject = 'self' | { id: string; name: string };

const PATHS: Subject[] = [
  'self',
  { id: ID, name: 'Abigail Fenwick' },
  { id: ID, name: 'a/b\\c:d*e?f"g<h>i|j' },
  { id: ID, name: 'Self' },
  { id: ID, name: 'self' },
  { id: ID, name: 'SELF' },
  { id: ID, name: '  Self  ' },
  { id: ID, name: 'Self:' },
  { id: ID, name: 'x'.repeat(120) },
  // An astral emoji straddling the 100-code-unit cut.
  { id: ID, name: 'y'.repeat(99) + '\u{1F600}tail' },
  { id: ID, name: '' },
  { id: 'bbbbbbbb-2222-4222-8222-bbbbbbbbbbbb', name: 'sElF' },
];

const CANON: Array<string | null> = [
  null,
  '',
  '- plain line\n- another',
  '---\ntype: commonplace-digest\n',
  '---\ntype: commonplace-digest\nupdatedAt: "x"\n---\n# Self\n\n- kept\n',
  '---\ntype: commonplace-digest\n---',
  '---\ntype: commonplace-digest\n---\n',
  '# h\n#h\n## h\n- body',
  '# Self\n\n',
  '\t- tabbed\n\t',
  '- crlf\r\n- line\r\n',
  '# only\n#  two spaces\n',
  'no frontmatter\n---\nstill body',
  '---x\n# h\nkept',
];

const LONG = Array.from({ length: 40 }, (_, i) => `- digest line number ${i} with some words`).join('\n');
const TRUNCATE: Array<[string, number]> = [
  ['short', 0],
  ['short', -1],
  ['short', 10],
  [LONG, 50],
  [LONG, 3],
  ['x'.repeat(400), 20],
  ['\u{1F600}'.repeat(200) + '\nsecond', 15],
  ['a\nb', 1],
  ['', 5],
];

interface Digest {
  content: string;
  kind: 'semantic' | 'episodic';
  occurredAt: string | null;
  reinforcedImportance: number;
}

const D = (content: string, ri: number, kind: 'semantic' | 'episodic' = 'semantic', occurredAt: string | null = null): Digest => ({
  content,
  kind,
  occurredAt,
  reinforcedImportance: ri,
});

const RENDER: Array<{
  subjectCharacterId: string;
  subjectName: string;
  isSelf: boolean;
  digests: Digest[];
  updatedAt: string;
}> = [
  {
    subjectCharacterId: ID,
    subjectName: 'ignored',
    isSelf: true,
    digests: [D('Likes tea.', 0.4), D('Keeps a ledger.', 0.9)],
    updatedAt: '2026-10-09T12:00:00.000Z',
  },
  {
    subjectCharacterId: ID,
    subjectName: '  Abigail  ',
    isSelf: false,
    digests: [
      D('Met at the fair.', 0.5, 'episodic', '2026-03-04T05:06:07.000Z'),
      D('No date.', 0.5, 'episodic', null),
      D('Tie A', 0.5),
      D('Tie B', 0.5),
    ],
    updatedAt: '2026-10-09T12:00:00.000Z',
  },
  { subjectCharacterId: ID, subjectName: '', isSelf: false, digests: [], updatedAt: '2026-10-09T12:00:00.000Z' },
  { subjectCharacterId: ID, subjectName: '  \t ', isSelf: false, digests: [D('x', 1)], updatedAt: 'u' },
  {
    subjectCharacterId: ID,
    subjectName: 'Multi',
    isSelf: false,
    digests: [
      D('  line one \n  line two \n three four\t\n\tfive  ', 0.7),
      D('a\r\nb', 0.6),
      D('\n\n', 0.1),
    ],
    updatedAt: '2026-10-09T12:00:00.000Z',
  },
  {
    subjectCharacterId: ID,
    subjectName: 'Budget',
    isSelf: false,
    digests: [D('w'.repeat(6000), 0.9), D('small', 0.8), D('another', 0.7)],
    updatedAt: '2026-10-09T12:00:00.000Z',
  },
  {
    subjectCharacterId: ID,
    subjectName: 'Many',
    isSelf: false,
    digests: Array.from({ length: 80 }, (_, i) => D(`fact ${i} ${'z'.repeat(60)}`, (i % 7) / 7)),
    updatedAt: '2026-10-09T12:00:00.000Z',
  },
  {
    subjectCharacterId: 'id "quoted" \\ back\u0001',
    subjectName: 'Escapes',
    isSelf: false,
    digests: [D('e', 0.2, 'episodic', '\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}x')],
    updatedAt: 'stamp\n"with"  ',
  },
];

async function main(): Promise<void> {
  const constants = {
    COMMONPLACE_FOLDER: cf.COMMONPLACE_FOLDER,
    COMMONPLACE_FRONTMATTER_TYPE: cf.COMMONPLACE_FRONTMATTER_TYPE,
    COMMONPLACE_SELF_STEM: cf.COMMONPLACE_SELF_STEM,
    COMMONPLACE_FILE_TOKEN_BUDGET: cf.COMMONPLACE_FILE_TOKEN_BUDGET,
  };
  const paths = PATHS.map((s) => ({ in: s, out: cf.commonplacePathFor(s) }));
  const canon = CANON.map((raw) => ({ in: raw, out: cf.commonplaceFileToCanonText(raw) }));
  const truncate = TRUNCATE.map(([text, budget]) => ({
    text,
    budget,
    out: cf.truncateToTokenBudget(text, budget),
  }));
  const render = RENDER.map((input) => ({ in: input, out: cf.renderCommonplaceFile(input) }));
  const loneSurrogates: string[] = [];
  const wellFormed = (v: unknown, at: string): unknown => {
    if (typeof v === 'string') {
      if (!v.isWellFormed()) loneSurrogates.push(at);
      return v.toWellFormed();
    }
    if (Array.isArray(v)) return v.map((x, i) => wellFormed(x, `${at}[${i}]`));
    if (v && typeof v === 'object') {
      return Object.fromEntries(
        Object.entries(v as Record<string, unknown>).map(([k, x]) => [k, wellFormed(x, `${at}.${k}`)]),
      );
    }
    return v;
  };
  const body = wellFormed({ constants, paths, canon, truncate, render }, '$') as Record<string, unknown>;
  process.stdout.write(JSON.stringify({ case: 'commonplace-file', ...body, loneSurrogates }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`commonplace-file oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
