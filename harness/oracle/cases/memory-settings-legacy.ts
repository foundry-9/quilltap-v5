/**
 * Tier-1 oracle — v4's legacy memory-settings translator (P4.D288 unit 2,
 * C1-5; `lib/memory/settings-legacy.ts`, `e0ac7493a`, at the `83334b9a5` pin)
 * through its REAL exports:
 *
 *   - `legacyHousekeepingToMaintenance(raw)` — the per-user
 *     `chat_settings.autoHousekeepingSettings` value (a JSON string or an
 *     object) → the `memoryMaintenance` value `move-memory-maintenance-
 *     settings-v1` writes (`JSON.stringify`'d here, so the comparand is the
 *     BYTES v4 stores) or `null`;
 *   - `withMemoryMaintenanceFromLegacy(instanceSettings, chatSettings)` — the
 *     restore's twin: the rows it answers, in order.
 *
 * Rows: a string / an object / malformed JSON / an array / `null` / a scalar;
 * caps as strings, booleans, `null`, out of bounds both ways, fractional,
 * non-finite (`"Infinity"`, `NaN` via a string), `1e400` text; overrides kept
 * when clampable and DROPPED otherwise, in source order, with integer-like keys
 * (V8's own-key order); `enabled` only when `=== true`; the restore twin's
 * FIRST candidate (not the latest), the single user preferred, key PRESENCE
 * with a `null` value, the backup's own key winning, the dead key dropped, the
 * derived row LAST.
 *
 * Output: NDJSON, one `{fn, label, input, output}` line per row.
 *
 * Run (Node 24, from the pinned v4 worktree):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   rm -f /tmp/oracle-memory-settings-legacy.ndjson
 *   cd ~/source/quilltap-server
 *   $N/npx tsx $V5W/harness/oracle/cases/memory-settings-legacy.ts \
 *     > /tmp/oracle-memory-settings-legacy.ndjson
 */

const SINGLE = 'ffffffff-ffff-ffff-ffff-ffffffffffff';

const LEGACY_INPUTS: Array<[string, unknown]> = [
  ['object', { enabled: true, perCharacterCap: 500, perCharacterCapOverrides: { a: 150 }, autoMergeSimilarThreshold: 0.9, mergeSimilar: true }],
  ['string', JSON.stringify({ enabled: true, perCharacterCap: 700 })],
  ['malformed', '{not json'],
  ['array-string', '[1,2]'],
  ['array', [1, 2]],
  ['null', null],
  ['null-string', 'null'],
  ['number', 5],
  ['empty-object', {}],
  ['enabled-truthy', { enabled: 1 }],
  ['enabled-string', { enabled: 'true' }],
  ['cap-string', { perCharacterCap: '2500' }],
  ['cap-string-padded', { perCharacterCap: ' 300 ' }],
  ['cap-empty-string', { perCharacterCap: '' }],
  ['cap-bool', { perCharacterCap: true }],
  ['cap-null', { perCharacterCap: null }],
  ['cap-low', { perCharacterCap: 5 }],
  ['cap-high', { perCharacterCap: 5000000 }],
  ['cap-fraction', { perCharacterCap: 250.9 }],
  ['cap-negative-fraction', { perCharacterCap: -0.5 }],
  ['cap-infinity-string', { perCharacterCap: 'Infinity' }],
  ['cap-nan-string', { perCharacterCap: 'abc' }],
  ['cap-hex-string', { perCharacterCap: '0x200' }],
  ['cap-array', { perCharacterCap: [400] }],
  ['cap-object', { perCharacterCap: {} }],
  ['cap-huge-text', '{"perCharacterCap": 1e400}'],
  ['overrides-mixed', { perCharacterCapOverrides: { a: 150, b: 'x', c: '250', d: null, e: 1e9, g: [] } }],
  ['overrides-int-keys', '{"perCharacterCapOverrides": {"b": 300, "10": 200, "2": 250, "x": "nope", "1": 400}}'],
  ['overrides-array', { perCharacterCapOverrides: [150, 250] }],
  ['overrides-null', { perCharacterCapOverrides: null }],
  ['overrides-string', { perCharacterCapOverrides: 'x' }],
  ['dup-keys-text', '{"enabled": false, "enabled": true}'],
];

type Row = { key: string; value: string };

const RESTORE_INPUTS: Array<[string, Row[], unknown[]]> = [
  ['none', [], []],
  ['derive-single-user', [{ key: 'generalMountPointId', value: 'g' }], [
    { userId: 'u-other', autoHousekeepingSettings: JSON.stringify({ enabled: true, perCharacterCap: 800 }) },
    { userId: SINGLE, autoHousekeepingSettings: { enabled: true, perCharacterCap: 900 } },
  ]],
  ['first-not-latest', [], [
    { userId: 'u1', updatedAt: '2020-01-01T00:00:00.000Z', autoHousekeepingSettings: { perCharacterCap: 300 } },
    { userId: 'u2', updatedAt: '2026-01-01T00:00:00.000Z', autoHousekeepingSettings: { perCharacterCap: 600 } },
  ]],
  ['key-presence-null', [], [
    { userId: SINGLE, autoHousekeepingSettings: null },
    { userId: 'u2', autoHousekeepingSettings: { perCharacterCap: 600 } },
  ]],
  ['no-candidate', [{ key: 'a', value: '1' }], [{ userId: SINGLE }]],
  ['backup-key-wins', [
    { key: 'memoryMaintenance', value: '{"enabled":true}' },
    { key: 'memoryExtractionConcurrency', value: '3' },
  ], [{ userId: SINGLE, autoHousekeepingSettings: { perCharacterCap: 600 } }]],
  ['dead-key-dropped-derived-last', [
    { key: 'memoryExtractionConcurrency', value: '3' },
    { key: 'zzz', value: 'last-before-derived' },
  ], [{ userId: SINGLE, autoHousekeepingSettings: '{"enabled":true,"perCharacterCapOverrides":{"q":101}}' }]],
  ['malformed-source', [{ key: 'k', value: 'v' }], [{ userId: SINGLE, autoHousekeepingSettings: '{bad' }]],
  ['non-object-rows', [], [null, 5, 'x', [], { userId: 'u9', autoHousekeepingSettings: { enabled: true } }]],
];

async function main(): Promise<void> {
  process.env.LOG_LEVEL = 'error';
  const { legacyHousekeepingToMaintenance, withMemoryMaintenanceFromLegacy } = await import(
    '@/lib/memory/settings-legacy'
  );
  const emit = (row: unknown) => process.stdout.write(JSON.stringify(row) + '\n');
  for (const [label, input] of LEGACY_INPUTS) {
    const out = legacyHousekeepingToMaintenance(input);
    // The comparand is the STORED bytes (`JSON.stringify`), as the migration writes them.
    emit({ fn: 'legacy', label, input, output: out === null ? null : JSON.stringify(out) });
  }
  for (const [label, rows, chatSettings] of RESTORE_INPUTS) {
    emit({ fn: 'restore', label, input: { rows, chatSettings }, output: withMemoryMaintenanceFromLegacy(rows, chatSettings) });
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`memory-settings-legacy oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
