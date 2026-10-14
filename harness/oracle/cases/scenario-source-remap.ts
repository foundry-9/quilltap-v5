/**
 * Tier-1 oracle — P4.D288 (C1-11): v4's REAL `lib/scenarios/source-remap.ts`
 * (`545e299bb`, at the `83334b9a5` pin) — `readScenarioSource` over every
 * shape a bundle carries (an object, a JSON string from a raw row, nothing,
 * malformed text, a non-object) and `ScenarioSourceSchema`'s edges (a 65-char
 * hash, a 501-unit path, an astral path at the 500-unit boundary either side,
 * a bad uuid, an extra key — stripped); then `remapScenarioSource` with a
 * mount map (mapped, unmapped → null, mapped to `''` → null).
 *
 * Pure. One NDJSON line per case.
 *
 * Run (Node 24, from the round's pin):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   $N/npx tsx $V5W/harness/oracle/cases/scenario-source-remap.ts > /tmp/oracle-scenario-source-remap.ndjson
 */

const MOUNT = 'e2910000-0000-4000-8000-0000000000a1';
const OTHER = 'e2910000-0000-4000-8000-0000000000a2';
const NEW = 'e2910000-0000-4000-8000-0000000000b1';
const SHA = 'ab'.repeat(32);
const ASTRAL = '\u{1F4DC}'; // 2 UTF-16 units

async function main(): Promise<void> {
  const { readScenarioSource, remapScenarioSource } = await import('@/lib/scenarios/source-remap');
  const ok = { mountPointId: MOUNT, path: 'Scenarios/a.md', bodySha256: SHA };
  const values: Array<{ label: string; value?: unknown }> = [
    { label: 'object', value: ok },
    { label: 'string', value: JSON.stringify(ok) },
    { label: 'extra key', value: { bodySha256: SHA, extra: 1, path: 'Scenarios/b.md', mountPointId: MOUNT } },
    { label: 'null', value: null },
    { label: 'absent' },
    { label: 'empty string', value: '' },
    { label: 'malformed string', value: '{"mountPointId":' },
    { label: 'a JSON number string', value: '42' },
    { label: 'a JSON null string', value: 'null' },
    { label: 'number', value: 7 },
    { label: 'array', value: [ok] },
    { label: 'hash 65', value: { ...ok, bodySha256: SHA + 'c' } },
    { label: 'hash 63', value: { ...ok, bodySha256: SHA.slice(1) } },
    { label: 'path empty', value: { ...ok, path: '' } },
    { label: 'path 500', value: { ...ok, path: 'p'.repeat(500) } },
    { label: 'path 501', value: { ...ok, path: 'p'.repeat(501) } },
    { label: 'astral path 500 units', value: { ...ok, path: ASTRAL.repeat(250) } },
    { label: 'astral path 501 units', value: { ...ok, path: 'p' + ASTRAL.repeat(250) } },
    { label: 'bad uuid', value: { ...ok, mountPointId: 'not-a-uuid' } },
    { label: 'missing hash', value: { mountPointId: MOUNT, path: 'Scenarios/a.md' } },
  ];
  const out: string[] = [];
  for (const v of values) {
    out.push(
      JSON.stringify({
        case: 'read',
        label: v.label,
        ...('value' in v ? { value: v.value } : {}),
        result: readScenarioSource(v.value),
      }),
    );
  }
  const maps: Array<{ label: string; map: Record<string, string> }> = [
    { label: 'mapped', map: { [MOUNT]: NEW } },
    { label: 'unmapped', map: { [OTHER]: NEW } },
    { label: 'mapped to empty', map: { [MOUNT]: '' } },
  ];
  for (const m of maps) {
    for (const v of [values[0], values[1], values[2], values[3], values[11]]) {
      out.push(
        JSON.stringify({
          case: 'remap',
          label: `${m.label} / ${v.label}`,
          value: v.value,
          map: m.map,
          result: remapScenarioSource(v.value, (id) => (id in m.map ? m.map[id] : null)),
        }),
      );
    }
  }
  process.stdout.write(out.join('\n') + '\n');
}

main().catch((err) => {
  process.stderr.write(`scenario-source-remap oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
