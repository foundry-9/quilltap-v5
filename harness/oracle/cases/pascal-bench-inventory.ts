/**
 * Oracle case: the Proving Bench's mock bag (P4.D329, v4 `e1872c94f` +
 * `a53024d3c` — `lib/pascal/bench-inventory.ts` WHOLE).
 *
 * Drives the REAL code:
 *   MockInventorySchema.nullish().safeParse   (the preview / audit body field)
 *   mockInventoryContext                      (the bag the bench answers from)
 *
 * The parse rows carry v4's raw Zod ISSUE objects whole (`code`, `path`, each
 * code's own key order) — the bench's 400 is `badRequest('Invalid request
 * body', issues)`, and its `details` is exactly this list (v5's envelope
 * drops `details` — the standing P4.6bb deferral — so this family is where the
 * issue bytes are pinned).
 *
 * The context rows probe every method of the mock: `state`, `equippedKinCount`,
 * `item` (ALWAYS null — R-U), the three `scopeObjects`, and `characterId` /
 * `chatId`. Lookups are `ref.trim().toLowerCase()`; later rows win; a blank ref
 * is skipped; an equipped row's `count` defaults to 1, any other's to 0.
 *
 * Run from inside the server checkout (Node 24 `~/.nvm/versions/node/v24.13.1/bin`):
 *   cd ~/source/quilltap-server
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TZ=UTC npx tsx \
 *     "$V5W/harness/oracle/cases/pascal-bench-inventory.ts" \
 *     > /tmp/oracle-pascal-bench-inventory.ndjson
 */

import { MockInventorySchema, mockInventoryContext, type MockInventoryRow } from '@/lib/pascal/bench-inventory';

const rows: unknown[] = [];
const BodyField = MockInventorySchema.nullish();

// ------------------------------------------------------------------ parse
const long = 'x'.repeat(201);
const astral200 = '😀'.repeat(100); // 200 UTF-16 units, 100 code points
const astral201 = '😀'.repeat(100) + 'a';
const sixtyFive = Array.from({ length: 65 }, (_, i) => ({ ref: `r${i}`, state: 'carried' }));
const sixtyFour = Array.from({ length: 64 }, (_, i) => ({ ref: `r${i}`, state: 'carried' }));
const parseCases: Array<[string, unknown]> = [
  ['absent', undefined],
  ['null', null],
  ['empty', []],
  ['one-row', [{ ref: 'sword', state: 'equipped' }]],
  ['full-row', [{ ref: 'ring', state: 'equipped', count: 2, metadata: { power: 3, tags: ['a', 1, true], name: 'x' } }]],
  ['unknown-key', [{ ref: 'sword', state: 'equipped', colour: 'red' }]],
  ['two-unknown-keys', [{ ref: 'sword', state: 'equipped', colour: 'red', size: 2 }]],
  ['count-float', [{ ref: 'sword', state: 'equipped', count: 1.5 }]],
  ['count-negative', [{ ref: 'sword', state: 'equipped', count: -1 }]],
  ['count-negative-float', [{ ref: 'sword', state: 'equipped', count: -1.5 }]],
  ['count-too-big', [{ ref: 'sword', state: 'equipped', count: 1000 }]],
  ['count-string', [{ ref: 'sword', state: 'equipped', count: '2' }]],
  ['count-null', [{ ref: 'sword', state: 'equipped', count: null }]],
  ['state-worn', [{ ref: 'sword', state: 'worn' }]],
  ['state-missing', [{ ref: 'sword' }]],
  ['state-number', [{ ref: 'sword', state: 3 }]],
  ['ref-empty', [{ ref: '', state: 'carried' }]],
  ['ref-201', [{ ref: long, state: 'carried' }]],
  ['ref-astral-200-units', [{ ref: astral200, state: 'carried' }]],
  ['ref-astral-201-units', [{ ref: astral201, state: 'carried' }]],
  ['ref-number', [{ ref: 7, state: 'carried' }]],
  ['ref-missing', [{ state: 'carried' }]],
  ['metadata-object-value', [{ ref: 'sword', state: 'carried', metadata: { k: {} } }]],
  ['metadata-null-value', [{ ref: 'sword', state: 'carried', metadata: { k: null } }]],
  ['metadata-array-bad-entry', [{ ref: 'sword', state: 'carried', metadata: { k: ['ok', {}, null] } }]],
  ['metadata-not-record', [{ ref: 'sword', state: 'carried', metadata: ['x'] }]],
  ['row-not-object', ['sword']],
  ['rows-not-array', { ref: 'sword', state: 'carried' }],
  ['rows-string', 'sword'],
  ['sixty-four-rows', sixtyFour],
  ['sixty-five-rows', sixtyFive],
  ['two-bad-rows', [{ ref: '', state: 'worn' }, { ref: 'ok', state: 'carried', count: 1.5, extra: 1 }]],
];
for (const [id, input] of parseCases) {
  const parsed = BodyField.safeParse(input);
  rows.push({
    kind: 'parse',
    id: `parse-${id}`,
    ...(input === undefined ? {} : { input }),
    ok: parsed.success,
    ...(parsed.success ? { data: parsed.data ?? null } : { issues: parsed.error.issues }),
  });
}

// ---------------------------------------------------------------- context
const contextCases: Array<[string, MockInventoryRow[], string[]]> = [
  // v4's `the mock bag` (`inventory-aggregate.test.ts:121-140`)
  [
    'v4-mock-bag',
    [
      { ref: 'Lockpick', state: 'equipped', count: 2, metadata: { toolVersion: 3 } },
      { ref: 'Resonator', state: 'carried', metadata: { toolAbilities: ['suppress'] } },
      { ref: 'Rope', state: 'reachable' },
      { ref: 'Ghost', state: 'absent', metadata: { toolVersion: 9 } },
    ],
    ['lockpick', 'Lockpick', 'resonator', 'rope', 'ghost', 'axe'],
  ],
  [
    'later-row-wins-first-position',
    [
      { ref: 'sword', state: 'carried' },
      { ref: 'shield', state: 'equipped' },
      { ref: 'SWORD', state: 'equipped', count: 3 },
    ],
    ['sword', 'shield'],
  ],
  ['blank-ref-skipped', [{ ref: '   ', state: 'equipped' }, { ref: 'cup', state: 'carried' }], ['', '   ', 'cup']],
  ['trimmed-and-folded', [{ ref: '  Sword ', state: 'equipped' }], ['sword', ' SWORD', 'Sword  ']],
  ['equipped-count-zero', [{ ref: 'ring', state: 'equipped', count: 0 }], ['ring']],
  ['carried-count-ignored-for-copies', [{ ref: 'coin', state: 'carried', count: 5 }], ['coin']],
  ['empty', [], ['anything']],
  ['unicode-fold', [{ ref: 'Éclair', state: 'carried' }, { ref: 'İstanbul', state: 'equipped' }], ['éclair', 'i̇stanbul', 'istanbul']],
];
for (const [id, mockRows, probes] of contextCases) {
  const ctx = mockInventoryContext(mockRows);
  rows.push({
    kind: 'context',
    id: `context-${id}`,
    rows: mockRows,
    characterId: ctx.characterId,
    chatId: ctx.chatId,
    probes: probes.map((ref) => ({
      ref,
      state: ctx.state(ref),
      equippedKinCount: ctx.equippedKinCount(ref),
      item: ctx.item(ref),
    })),
    scopeObjects: {
      equipped: ctx.scopeObjects('equipped'),
      owned: ctx.scopeObjects('owned'),
      reachable: ctx.scopeObjects('reachable'),
    },
  });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
