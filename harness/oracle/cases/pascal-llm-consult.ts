/**
 * Oracle case (P4.d8 unit 3): the PURE leaves of the custom-tool LLM consult.
 * Drives v4's REAL `lib/pascal/llm-consult.ts` exports:
 *
 *   CONSULT_TIMEOUT_MS, consultMaxTokens
 *
 * ## What this can and cannot pin
 *
 * `consultMaxTokens` is the whole of the module's arithmetic and it is pure, so
 * it is diffed exactly here — including the two clamps, which are the parts a
 * port gets subtly wrong (the floor applies AFTER the ceil, and the ceiling is
 * 32,768 rather than a token count derived from the character ceiling).
 *
 * The rest of the module — the fresh per-invocation settings/profile
 * resolution, the danger reroute, the `'custom-tool-consult'` task type — is
 * plumbing over the cheap-LLM pipeline and is proven through the run_custom
 * HANDLER differential with a canned provider on both sides, not here. The
 * private `withTimeout` rejection message is not exported and so cannot be
 * driven from outside; the port pins it with an in-crate test against the
 * string read from v4 at `616930db`
 * (`the consult timed out after ${Math.round(ms / 1000)}s`).
 *
 * Run (v4 @ 616930db, Node 24):
 *   cd ~/source/quilltap-server
 *   npx tsx <V5W>/harness/oracle/cases/pascal-llm-consult.ts \
 *     > /tmp/oracle-pascal-llm-consult.ndjson
 */

import { CONSULT_TIMEOUT_MS, consultMaxTokens } from '@/lib/pascal/llm-consult';
import {
  MAX_LLM_OUTPUT_CEILING,
  MAX_LLM_OUTPUT_LENGTH,
} from '@/lib/pascal/custom-tool.types';

const rows: unknown[] = [];

rows.push({ kind: 'constant', id: 'CONSULT_TIMEOUT_MS', out: CONSULT_TIMEOUT_MS });
rows.push({ kind: 'constant', id: 'MAX_LLM_OUTPUT_LENGTH', out: MAX_LLM_OUTPUT_LENGTH });
rows.push({ kind: 'constant', id: 'MAX_LLM_OUTPUT_CEILING', out: MAX_LLM_OUTPUT_CEILING });

const inputs = [
  // The floor's whole span: everything at or under 6,144 chars clamps to 2,048.
  0, 1, 2, 3, 4, 100, 2047, 2048, 2049, 6141, 6142, 6143, 6144,
  // Just past it — now the ceil governs, and the /3 remainder is what to get right.
  6145, 6146, 6147, 6148,
  // The default cap and the authored ceiling.
  MAX_LLM_OUTPUT_LENGTH, 10_000, 50_000, MAX_LLM_OUTPUT_CEILING,
  // The upper clamp's boundary: 98,304 chars is exactly 32,768 tokens.
  98_301, 98_302, 98_303, 98_304, 98_305, 200_000, 1_000_000,
];
for (const n of inputs) {
  rows.push({ kind: 'consultMaxTokens', id: String(n), input: n, out: consultMaxTokens(n) });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
