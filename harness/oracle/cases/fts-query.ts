/**
 * Oracle case: the FTS5 query translator (P4.D204, tier 1).
 *
 * Drives the REAL functions from the v4 server's
 * `lib/database/repositories/fts-query.ts` — `tokenizeLikeUnicode61` and
 * `buildFtsMatchExpression` — plus `escapeLikeLiteral` from the sibling
 * `lib/database/repositories/like-escape.ts` module `fts-query.ts` imports
 * it from since `ad1c4c37f` — over the COMMITTED
 * corpus at `harness/oracle/fixtures/fts-query.json`, and prints one NDJSON
 * row per query on stdout. The Rust differential (`fts_query_equivalence`)
 * feeds the same corpus through `quilltap_core::db::fts_query` and asserts
 * field-equal results.
 *
 * IMPORTANT — this imports the actual app code; it does not reimplement it,
 * and it is NOT a transcription of v4's `fts-query.test.ts` (§R.6: a new pure
 * module lands with a corpus recorded from v4's REAL module). Run it from
 * inside the server checkout, or from a detached worktree pinned at the
 * lane's baseline, so `@/` resolves:
 *
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/fts-query.ts \
 *     ~/source/quilltap-v5/harness/oracle/fixtures/fts-query.json \
 *     > /tmp/oracle-fts-query.ndjson
 *
 * Each row carries, for one query:
 *
 *  - `tokens`      — `tokenizeLikeUnicode61`. The `\p{L}\p{N}` seam: V8's ICU
 *                    tables against the Rust `regex` crate's.
 *  - `escaped`     — `escapeLikeLiteral`, the raw escape with no `%…%` wrap
 *                    (v5 folds this onto its single `like_escape` home, so
 *                    this row is what proves the fold is v4-faithful).
 *  - `plan`        — `buildFtsMatchExpression`, verbatim: `kind` plus either
 *                    `match` or `likePattern` + `reason`.
 *  - `tokenLengths`— each token's JS `.length` (UTF-16 code units), because
 *                    that is what the `MIN_USEFUL_TOKEN_LENGTH` gate reads and
 *                    a `chars().count()` port would silently disagree on for
 *                    astral text.
 *
 * The corpus is a fixed committed file (no randomness, no clock), so the
 * oracle is reproducible and the Rust side reads the identical bytes.
 */

import { readFileSync } from 'fs';

import {
  buildFtsMatchExpression,
  tokenizeLikeUnicode61,
} from '@/lib/database/repositories/fts-query';
// v4 `ad1c4c37f` deletes `fts-query.ts`'s own `escapeLikePattern` and imports
// `escapeLikeLiteral` from `./like-escape` instead (same regex, no lowercasing).
import { escapeLikeLiteral } from '@/lib/database/repositories/like-escape';

const fixturePath = process.argv[2];
if (!fixturePath) {
  throw new Error('usage: fts-query.ts <fixtures/fts-query.json>');
}
const corpus = JSON.parse(readFileSync(fixturePath, 'utf-8')) as { queries: string[] };

const emit = (row: unknown) => process.stdout.write(JSON.stringify(row) + '\n');

for (const query of corpus.queries) {
  const tokens = tokenizeLikeUnicode61(query);
  emit({
    query,
    tokens,
    tokenLengths: tokens.map((t) => t.length),
    escaped: escapeLikeLiteral(query),
    plan: buildFtsMatchExpression(query),
  });
}
