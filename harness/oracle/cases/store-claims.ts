/**
 * Tier-1 oracle case — the restore's store claim map (P4.D273, v4 `ed8b15b50`
 * bug 185: `lib/backup/restore/store-claims.ts` `makeStoreClaimMap`).
 *
 * Drives v4's REAL module over the shared corpus
 * `harness/oracle/fixtures/store-claims-corpus.json`, which the Rust side reads
 * too: each case builds ONE claim map from `carried` and runs its `claims` in
 * order, so the holders a claim records are visible to the next one. The
 * corpus pins v4's jest cases, every reason, falsy / non-string pointers,
 * every `storeType` shape `kindFits`' `!storeType` tells apart (absent, null,
 * `''`, 0, false, a number, `'Character'`), a holder across kinds, a second
 * claim by the same entity, and a duplicated store id in both orders (the
 * `DUP_STORE_ID_FIRST_ROW` ruled divergence — v4 keeps the LAST row).
 *
 * The pointer sentinel `"__ABSENT__"` becomes a real `undefined`.
 *
 * Pure — no DB, no jest. Emits one NDJSON row per case:
 * `{ name, outcomes: [<v4's StoreClaim object>, …] }`.
 *
 * Run (Node 24, from the v4 checkout — the module arrived at `ed8b15b50`, past
 * the `f5e953a3f` baseline, so pin the checkout at or after it until the
 * baseline moves):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/store-claims.ts \
 *     > /tmp/oracle-store-claims.ndjson
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

import { makeStoreClaimMap, type StoreOwnerKind } from '@/lib/backup/restore/store-claims';

interface Claim {
  kind: StoreOwnerKind;
  entityId: string;
  pointer: unknown;
}

interface Case {
  name: string;
  carried: Array<{ id: string; storeType?: string | null }>;
  claims: Claim[];
}

const here = dirname(fileURLToPath(import.meta.url));
const corpus = JSON.parse(
  fs.readFileSync(join(here, '..', 'fixtures', 'store-claims-corpus.json'), 'utf8'),
) as { cases: Case[] };

const lines: string[] = [];
for (const c of corpus.cases) {
  const map = makeStoreClaimMap(c.carried);
  const outcomes = c.claims.map((call) => {
    const pointer = call.pointer === '__ABSENT__' ? undefined : call.pointer;
    return map.claim(call.kind, call.entityId, pointer as string | null | undefined);
  });
  lines.push(JSON.stringify({ name: c.name, outcomes }));
}

process.stdout.write(lines.join('\n') + '\n');
