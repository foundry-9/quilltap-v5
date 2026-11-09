/**
 * Oracle case (P4.D367): the image transport budget's PURE plan — v4
 * `lib/chat/image-transport-budget.ts` `planImageTransport` (`29c1e2690`, at
 * the `624d0d25c` pin), driven REAL over the committed corpus
 * `harness/oracle/fixtures/image-transport-budget.json` (`plan` rows, each
 * with an explicit policy).
 *
 * v4's verdict `Map` is keyed by attachment OBJECT identity; it is projected
 * POSITIONALLY here — one entry per attachment per message, in array order, a
 * non-loaded entry (no verdict) as `null` — which is v5's grid. The ranked
 * order is projected the same way, from the order in which the plan's walk
 * SET each receivable image's verdict (a `Map` keeps insertion order, and the
 * ranked walk is the only writer of an image verdict).
 *
 * Every row carries `expect` (attachment id → verdict); the leaf THROWS on a
 * row that does not reach the verdict it was written for, so no row passes
 * vacuously (memory note `tier-r-vacuous-equal-rows`).
 *
 * Run from inside the v4 checkout pinned at `624d0d25c`:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/image-transport-budget-plan.ts \
 *     > /tmp/oracle-image-transport-budget-plan.ndjson
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

import { planImageTransport } from '@/lib/chat/image-transport-budget';

interface PlanRow {
  label: string;
  rule: string;
  policy: { maxImages: number; maxTotalBase64: number; canReceive: string[] };
  messages: Array<{ role: string; content: string; attachments?: unknown[] }>;
  expect: Record<string, string>;
}

/** `$dataLen: n` → `data: 'A' x n`, IN PLACE (the key keeps its position). */
export function expandData(v: unknown): unknown {
  if (Array.isArray(v)) return v.map(expandData);
  if (!v || typeof v !== 'object') return v;
  const out: Record<string, unknown> = {};
  for (const [k, val] of Object.entries(v as Record<string, unknown>)) {
    if (k === '$dataLen') out.data = 'A'.repeat(val as number);
    else out[k] = expandData(val);
  }
  return out;
}

function verdictName(v: { kind: string; reason?: string } | undefined): string | null {
  if (!v) return null;
  return v.kind === 'overflow' ? (v.reason as string) : v.kind;
}

const here = dirname(fileURLToPath(import.meta.url));
const corpus = JSON.parse(
  fs.readFileSync(join(here, '..', 'fixtures', 'image-transport-budget.json'), 'utf8'),
) as { plan: PlanRow[] };

for (const row of corpus.plan) {
  const messages = expandData(row.messages) as PlanRow['messages'];
  const receivable = new Set(row.policy.canReceive);
  const verdicts = planImageTransport(messages, {
    maxImages: row.policy.maxImages,
    maxTotalBase64: row.policy.maxTotalBase64,
    canReceive: (m: string) => receivable.has(m),
  });
  const grid = messages.map((m) =>
    (m.attachments ?? []).map((a) => {
      const v = verdicts.get(a) as { kind: string; reason?: string } | undefined;
      return v ? (v.kind === 'overflow' ? { kind: v.kind, reason: v.reason } : { kind: v.kind }) : null;
    }),
  );
  // The ranked order: the image verdicts in Map insertion order, as
  // [message, attachment] positions (an image's verdict is only ever SET by
  // the ranked walk; capability / non-image keep verdicts are set first, in
  // array order, and are filtered out by `isRankedImage`).
  const position = new Map<unknown, [number, number]>();
  messages.forEach((m, mi) => (m.attachments ?? []).forEach((a, ai) => position.set(a, [mi, ai])));
  const isRankedImage = (a: unknown): boolean =>
    !!a &&
    typeof a === 'object' &&
    typeof (a as { mimeType?: unknown }).mimeType === 'string' &&
    ((a as { mimeType: string }).mimeType).startsWith('image/') &&
    receivable.has((a as { mimeType: string }).mimeType);
  const ranked = [...verdicts.keys()].filter(isRankedImage).map((a) => position.get(a));

  // The row reaches the rule it was written for.
  messages.forEach((m) =>
    (m.attachments ?? []).forEach((a) => {
      const id = a && typeof a === 'object' ? (a as { id?: unknown }).id : undefined;
      if (typeof id !== 'string') return;
      const got = verdictName(verdicts.get(a) as { kind: string; reason?: string } | undefined);
      if ((row.expect[id] ?? null) !== got) {
        throw new Error(`${row.label}: ${id} reached ${got}, written for ${row.expect[id]}`);
      }
    }),
  );
  const expected = Object.keys(row.expect).length;
  const seen = messages.flatMap((m) => m.attachments ?? []).filter(
    (a) => a && typeof a === 'object' && typeof (a as { id?: unknown }).id === 'string' && verdicts.has(a),
  ).length;
  if (expected !== seen) throw new Error(`${row.label}: expect names ${expected}, the plan decided ${seen}`);

  process.stdout.write(JSON.stringify({ kind: 'plan', label: row.label, grid, ranked }) + '\n');
}
