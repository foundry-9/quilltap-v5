/**
 * Tier-1 oracle — v4's document-store naming leaf (`lib/mount-index/
 * store-names.ts`, v4 `ed8b15b50`, bug 186, pure) and the shared suffixer
 * `nextUniqueMountPointName` (`lib/mount-index/unique-mount-point-name.ts`),
 * through their REAL exports (P4.D274 unit 1), so v5's
 * `quilltap_core::store_names` and `db::ensure_official_store::
 * next_unique_mount_point_name` are diffed rather than trusted.
 *
 * Inputs: the committed corpus `harness/oracle/fixtures/store-names-corpus.json`
 * (path in `QT_STORE_NAMES_CORPUS`). Output: ONE NDJSON line
 *   {case: "store-names", corpusSha256, vaultNames: [{in, out}],
 *    baseNames: [{in, out}], stamps: [{in, out}],
 *    retired: [{base, createdAt, out}], unique: [{taken, desired, out}],
 *    plans: [{label, renames, idempotent}]}
 * where `renames` is `planStoreNames(stores, characters)` verbatim (order and
 * key order — `id, from, to, reason` is spread into v4's INFO line) and
 * `idempotent` is the second plan over the applied result (`[]` every time).
 *
 * `TZ=UTC` is load-bearing: `storeNameTimestamp` runs `Date.parse`, which reads
 * a zone-less datetime as LOCAL time (the corpus's `2026-10-09` date-only and
 * the V8 legacy forms among the stamps — `Oct 9 2026`, the RFC-1123 one).
 *
 * Run (Node 24, from the round's pin or the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   rm -f /tmp/oracle-store-names.ndjson
 *   cd ~/source/quilltap-server
 *   TZ=UTC QT_STORE_NAMES_CORPUS=$V5W/harness/oracle/fixtures/store-names-corpus.json $N/npx tsx $V5W/harness/oracle/cases/store-names.ts > /tmp/oracle-store-names.ndjson
 */

import * as fs from 'fs';
import { createHash } from 'node:crypto';
import {
  characterVaultName,
  planStoreNames,
  retiredVaultName,
  storeNameTimestamp,
  vaultBaseName,
  type StoreNameRow,
  type VaultOwnerRow,
} from '@/lib/mount-index/store-names';
import { nextUniqueMountPointName } from '@/lib/mount-index/unique-mount-point-name';

const corpusPath = process.env.QT_STORE_NAMES_CORPUS;
if (!corpusPath) throw new Error('QT_STORE_NAMES_CORPUS is required (see header)');
const bytes = fs.readFileSync(corpusPath);
const corpus = JSON.parse(bytes.toString('utf8')) as {
  vaultNames: string[];
  baseNames: string[];
  stamps: string[];
  retired: { base: string; createdAt: string }[];
  unique: { taken: string[]; desired: string }[];
  plans: { label: string; stores: StoreNameRow[]; characters: VaultOwnerRow[] }[];
};

const out = {
  case: 'store-names',
  corpusSha256: createHash('sha256').update(bytes).digest('hex'),
  vaultNames: corpus.vaultNames.map((s) => ({ in: s, out: characterVaultName(s) })),
  baseNames: corpus.baseNames.map((s) => ({ in: s, out: vaultBaseName(s) })),
  stamps: corpus.stamps.map((s) => ({ in: s, out: storeNameTimestamp(s) })),
  retired: corpus.retired.map((r) => ({ ...r, out: retiredVaultName(r.base, r.createdAt) })),
  unique: corpus.unique.map((u) => ({ ...u, out: nextUniqueMountPointName(new Set(u.taken), u.desired) })),
  plans: corpus.plans.map((p) => {
    const renames = planStoreNames(p.stores, p.characters);
    const after = p.stores.map((s) => ({ ...s, name: renames.find((r) => r.id === s.id)?.to ?? s.name }));
    return { label: p.label, renames, idempotent: planStoreNames(after, p.characters) };
  }),
};

process.stdout.write(JSON.stringify(out) + '\n');
