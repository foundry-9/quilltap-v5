/**
 * Tier-1 ORACLE for the subprompts PURE helpers (v4 `2f4254b42`,
 * `lib/subprompts/subprompts.ts` — P4.D163 unit 2).
 *
 * Drives v4's REAL exports — `isValidSubpromptId`, `slugifySubpromptTitle`,
 * `composeSubpromptContent`, `parseSubpromptContent`, `subpromptPathForId` —
 * over the committed corpus (`harness/oracle/fixtures/subprompts-helpers.json`)
 * and emits one NDJSON row per case:
 *   { kind: 'id',      name, input, valid }
 *   { kind: 'slug',    name, title, slug }
 *   { kind: 'compose', name, title, content, composed }
 *   { kind: 'parse',   name, id, raw, updatedAt, parsed: {id,path,title,content,updatedAt} }
 *   { kind: 'path',    name, id, path }
 *
 * The rows are byte-exact strings (tier 1): the Rust family
 * (`crates/quilltap-harness/tests/subprompts_helpers_equivalence.rs`) reads the
 * SAME corpus and compares field for field. What the corpus asks that a
 * transcription cannot answer on its own: UTF-16 `.length` vs code points on
 * the 120-cap (astral ids), the JS `trim` whitespace set (NBSP / U+3000 /
 * U+FEFF), `normalize('NFKD')` + the U+0300–036F strip (fullwidth, ligatures,
 * Turkish İ, the Kelvin/Ångström signs), JS `toLowerCase` over the folded form,
 * the 60-byte slice landing on a hyphen, and `parseFrontmatter`'s UTF-16
 * `bodyStartOffset` on CRLF / astral files.
 *
 * Run (Node 24, from the v4 checkout; a pinned worktree while v4 HEAD is past
 * the baseline — the driver rewrites the `cd`):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   $N/node --import tsx $V5W/harness/oracle/cases/subprompts-helpers.ts \
 *     > /tmp/oracle-subprompts-helpers.ndjson
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

import {
  isValidSubpromptId,
  slugifySubpromptTitle,
  composeSubpromptContent,
  parseSubpromptContent,
  subpromptPathForId,
} from '@/lib/subprompts/subprompts';

interface Corpus {
  ids: Array<{ name: string; input: unknown }>;
  slugs: Array<{ name: string; title: string }>;
  compose: Array<{ name: string; title: string; content: string }>;
  parse: Array<{ name: string; id: string; raw: string; updatedAt: string }>;
  paths: Array<{ name: string; id: string }>;
}

const here = dirname(fileURLToPath(import.meta.url));
const corpus = JSON.parse(
  fs.readFileSync(join(here, '..', 'fixtures', 'subprompts-helpers.json'), 'utf8'),
) as Corpus;

const out = (row: Record<string, unknown>) => process.stdout.write(JSON.stringify(row) + '\n');

for (const c of corpus.ids) {
  out({ kind: 'id', name: c.name, input: c.input, valid: isValidSubpromptId(c.input) });
}
for (const c of corpus.slugs) {
  out({ kind: 'slug', name: c.name, title: c.title, slug: slugifySubpromptTitle(c.title) });
}
for (const c of corpus.compose) {
  out({
    kind: 'compose',
    name: c.name,
    title: c.title,
    content: c.content,
    composed: composeSubpromptContent(c.title, c.content),
  });
}
for (const c of corpus.parse) {
  const p = parseSubpromptContent(c.id, c.raw, c.updatedAt);
  out({
    kind: 'parse',
    name: c.name,
    id: c.id,
    raw: c.raw,
    updatedAt: c.updatedAt,
    parsed: { id: p.id, path: p.path, title: p.title, content: p.content, updatedAt: p.updatedAt },
  });
}
for (const c of corpus.paths) {
  out({ kind: 'path', name: c.name, id: c.id, path: subpromptPathForId(c.id) });
}
