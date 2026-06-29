/**
 * Tier-1 oracle case — `stableUuidFromString` (the vault-overlay id leaf).
 *
 * Drives v4's REAL `stableUuidFromString`
 * (`lib/database/repositories/vault-overlay/parsers.ts`) over a fixed corpus and
 * emits one NDJSON row per source string. The Rust port
 * (`quilltap_core::vault_overlay::stable_uuid_from_string`) must produce the
 * exact same UUID for every input (tier-1 exact equality).
 *
 * The corpus covers the real prefixed forms the vault derives ids from
 * (`prompt:` / `scenario:` / `wardrobe-item:` over `<mountPointId>:<relativePath>`),
 * an empty string, and a non-ASCII path — SHA-256 runs over UTF-8 bytes on both
 * sides, so the accented source must agree too (no case mapping here).
 *
 * Run from the v4 server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/stable-uuid.ts \
 *     > /tmp/oracle-stable-uuid.ndjson
 */

import { stableUuidFromString } from '@/lib/database/repositories/vault-overlay/parsers';

const MP = '11111111-1111-4111-8111-111111111111';

const sources: Array<{ id: string; source: string }> = [
  { id: 'empty', source: '' },
  { id: 'single-char', source: 'a' },
  { id: 'prompt', source: `prompt:${MP}:Prompts/intro.md` },
  { id: 'scenario', source: `scenario:${MP}:Scenarios/first-meeting.md` },
  { id: 'wardrobe', source: `wardrobe-item:${MP}:Wardrobe/casual.md` },
  { id: 'nested-path', source: `prompt:${MP}:Prompts/deep/nested/file.md` },
  // Non-ASCII path: SHA-256 over UTF-8 bytes must agree byte-for-byte.
  { id: 'unicode', source: `wardrobe-item:${MP}:Wardrobe/Café Münster — 日本語.md` },
  { id: 'different-mp', source: `prompt:22222222-2222-4222-8222-222222222222:Prompts/intro.md` },
];

for (const { id, source } of sources) {
  process.stdout.write(
    JSON.stringify({ id, source, out: stableUuidFromString(source) }) + '\n'
  );
}
