/**
 * Tier-1 oracle case — the avatar configuration cache KEY derivation (v4
 * `lib/wardrobe/avatar-cache.ts` `deriveAvatarCacheKey` /
 * `deriveLegacyAvatarCacheKey` / `deriveAvatarCacheKeys`), P4.D184 / v4
 * `7fbf8a55b`.
 *
 * Drives v4's REAL module over the shared corpus
 * `harness/oracle/fixtures/avatar-cache-key.json`, which the Rust side reads
 * too — so both implementations hash the SAME inputs and the hex digests are
 * compared exactly. The corpus pins, among ~31 shapes: key order inside the
 * params object (irrelevant), array order inside a LoRA list (significant),
 * an explicit `undefined` against its absent-key sibling (identical), an
 * explicit `null` against both (different), nested unsorted objects, a
 * supplementary-plane key against a BMP one (UTF-16 code-unit order, where
 * UTF-8 byte order disagrees), a Unicode/escape-heavy prompt, JS number
 * rendering, and `modelName` null vs absent on the v0 key.
 *
 * Two sentinels express what JSON cannot (see the corpus's own `$note`):
 * `"__UNDEFINED__"` becomes a real `undefined` here, and `"__ABSENT__"` means
 * the key is omitted entirely.
 *
 * Pure functions — no DB, no jest. Emits one NDJSON row per case:
 * `{ name, kind, key }` for v1/v0, `{ name, kind, key, legacyKey }` for a pair.
 *
 * Run (Node 24, from the v4 checkout — the module arrived at `7fbf8a55b`, past
 * the `f4ad2c8d1` baseline, so pin the checkout at or after it until the
 * baseline moves):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/avatar-cache-key.ts \
 *     > /tmp/oracle-avatar-cache-key.ndjson
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

import {
  deriveAvatarCacheKey,
  deriveAvatarCacheKeys,
  deriveLegacyAvatarCacheKey,
} from '@/lib/wardrobe/avatar-cache';
import type { ImageGenParams } from '@quilltap/plugin-types';

interface Case {
  name: string;
  kind: 'v1' | 'v0' | 'pair';
  provider?: string;
  imageProfileId?: string;
  params?: Record<string, unknown>;
  modelName?: string | null;
  prompt?: string;
}

/**
 * Turn the `"__UNDEFINED__"` sentinel into a real `undefined`, recursively.
 * A key whose value is `undefined` is PRESENT in the object (so `Object.entries`
 * yields it) — which is exactly the shape v4's `canonicalJson` filter exists to
 * handle, and which JSON alone cannot express.
 */
function hydrate(value: unknown): unknown {
  if (value === '__UNDEFINED__') return undefined;
  if (Array.isArray(value)) return value.map(hydrate);
  if (value !== null && typeof value === 'object') {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
      out[k] = hydrate(v);
    }
    return out;
  }
  return value;
}

const here = dirname(fileURLToPath(import.meta.url));
const corpus = JSON.parse(
  fs.readFileSync(join(here, '..', 'fixtures', 'avatar-cache-key.json'), 'utf8'),
) as { cases: Case[] };

const lines: string[] = [];
for (const c of corpus.cases) {
  if (c.kind === 'v0') {
    const row: Record<string, unknown> = { name: c.name, kind: c.kind };
    // `__ABSENT__` means the key is not on the object at all; v4's `?? null`
    // must make that identical to an explicit null.
    const input =
      c.modelName === '__ABSENT__'
        ? ({ prompt: c.prompt as string } as { modelName?: string | null; prompt: string })
        : { modelName: c.modelName ?? null, prompt: c.prompt as string };
    row.key = deriveLegacyAvatarCacheKey(input as never);
    lines.push(JSON.stringify(row));
    continue;
  }

  const params = hydrate(c.params ?? {}) as ImageGenParams;
  const input = {
    provider: c.provider as string,
    imageProfileId: c.imageProfileId as string,
    params,
  };
  if (c.kind === 'v1') {
    lines.push(JSON.stringify({ name: c.name, kind: c.kind, key: deriveAvatarCacheKey(input) }));
  } else {
    const keys = deriveAvatarCacheKeys(input);
    lines.push(
      JSON.stringify({ name: c.name, kind: c.kind, key: keys.key, legacyKey: keys.legacyKey }),
    );
  }
}

process.stdout.write(lines.join('\n') + '\n');
