/**
 * P4.D222 oracle — the help-section size proof (v4 `492771aff`,
 * `__tests__/unit/lib/help/help-doc-size.test.ts`), recorded so the port can
 * make the same check WITHOUT a tokenizer of its own.
 *
 * Mirrors v4's test exactly: every `help/*.md` in the flat, sorted directory
 * listing, `trim()`med, through v4's REAL `parseFrontmatter` → `extractTitle`
 * → `buildHelpDocChunks` → `helpChunkEmbeddingText`, and the composed text
 * counted in REAL `js-tiktoken` `cl100k_base` tokens. Emits:
 *   { kind: 'meta', maxTokens, files, sections }   (the constant + counts)
 *   { kind: 'section', file, chunkIndex, heading, text, tokens }  (per section)
 *
 * The Rust side (`help_section_size_equivalence`) re-slices the EMBEDDED tree
 * through the production sync, composes the same texts with v5's
 * `help_chunk_embedding_text`, asserts they are v4's byte for byte, and holds
 * every recorded count to `HELP_SECTION_EMBEDDING_MAX_TOKENS`.
 *
 * Needs `js-tiktoken` in the v4 checkout's `node_modules` (a devDependency
 * since `492771aff` — present only after the human's `npm install`).
 *
 * Run from the v4 checkout (or a pinned worktree):
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/help-section-size.ts \
 *     > /tmp/oracle-help-section-size.ndjson
 */

import { readFileSync, readdirSync } from 'node:fs';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { extractTitle, parseFrontmatter } from '@/lib/help/help-doc-sync';
import {
  HELP_SECTION_EMBEDDING_MAX_TOKENS,
  buildHelpDocChunks,
  helpChunkEmbeddingText,
} from '@/lib/help/help-doc-chunking';

// A bare `import 'js-tiktoken'` resolves from THIS file's directory (outside
// the v4 tree) and fails; `@/` resolves through the cwd's tsconfig, a package
// name does not — so require it relative to the checkout.
const { getEncoding } = createRequire(join(process.cwd(), 'package.json'))(
  'js-tiktoken',
) as typeof import('js-tiktoken');

const HELP_DIR = join(process.cwd(), 'help');
const encoder = getEncoding('cl100k_base');
const rows: unknown[] = [];

const files = readdirSync(HELP_DIR)
  .filter((name) => name.endsWith('.md'))
  .sort();
let measuredFiles = 0;
for (const file of files) {
  const raw = readFileSync(join(HELP_DIR, file), 'utf-8').trim();
  if (!raw) continue;
  measuredFiles++;

  const { body } = parseFrontmatter(raw);
  const title = extractTitle(body, `help/${file}`);

  for (const chunk of buildHelpDocChunks(body)) {
    const text = helpChunkEmbeddingText(title, chunk.heading, chunk.content);
    rows.push({
      kind: 'section',
      file,
      chunkIndex: chunk.chunkIndex,
      heading: chunk.heading,
      text,
      tokens: encoder.encode(text).length,
    });
  }
}

process.stdout.write(
  JSON.stringify({
    kind: 'meta',
    maxTokens: HELP_SECTION_EMBEDDING_MAX_TOKENS,
    files: measuredFiles,
    sections: rows.length,
  }) + '\n',
);
for (const row of rows) process.stdout.write(JSON.stringify(row) + '\n');
process.exit(0);
