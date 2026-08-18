/**
 * Tier-1 oracle case — the equipped-outfit hash (v4 `lib/wardrobe/outfit-hash.ts`
 * `hashEquippedSlots` + `hasEquippedItems`), P4.D87.
 *
 * The hash keys the cached clothing summary in `chat.sceneState`. v4 `4423ad10`
 * added the `hair` key to the normalized preimage UNCONDITIONALLY (its design
 * doc forbids conditional key omission), so every chat with a pre-hair cached
 * hash misses exactly once and re-derives — the accepted upgrade cost. This
 * corpus pins the five-key preimage exactly, including the identity that makes
 * the miss a one-time event: a four-key legacy row and its explicit
 * `hair: []` five-key equivalent hash IDENTICALLY.
 *
 * Pure function — no DB, no jest. Emits one NDJSON row per case:
 * `{ name, hash, has }`.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/outfit-hash.ts > /tmp/oracle-outfit-hash.ndjson
 */

import { hashEquippedSlots, hasEquippedItems } from '@/lib/wardrobe/outfit-hash';
import type { EquippedSlots } from '@/lib/schemas/wardrobe.types';

const A = 'a0000000-0000-4000-8000-000000000001';
const B = 'a0000000-0000-4000-8000-000000000002';
const H = 'a0000000-0000-4000-8000-000000000003';

const cases: Array<{ name: string; slots: unknown }> = [
  { name: 'null', slots: null },
  { name: 'empty-object', slots: {} },
  {
    name: 'legacy-four-key',
    slots: { top: [A], bottom: [], footwear: [], accessories: [] },
  },
  {
    name: 'five-key-equivalent',
    slots: { top: [A], bottom: [], footwear: [], accessories: [], hair: [] },
  },
  { name: 'hair-only', slots: { top: [], bottom: [], footwear: [], accessories: [], hair: [H] } },
  { name: 'layered-ab', slots: { top: [A, B], bottom: [], footwear: [], accessories: [], hair: [] } },
  { name: 'layered-ba', slots: { top: [B, A], bottom: [], footwear: [], accessories: [], hair: [] } },
  {
    name: 'full-five-slot',
    slots: { top: [A], bottom: [B], footwear: [A], accessories: [B], hair: [H] },
  },
];

const lines: string[] = [];
for (const c of cases) {
  const slots = c.slots as EquippedSlots | null;
  lines.push(
    JSON.stringify({
      name: c.name,
      hash: hashEquippedSlots(slots),
      has: hasEquippedItems(slots),
    })
  );
}
process.stdout.write(lines.join('\n') + '\n');
