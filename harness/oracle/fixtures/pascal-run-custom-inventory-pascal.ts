/**
 * P4.D329 (v4 `cad017599` — "Measure Twice", the first inventory-gated custom
 * tool) — the Pascal share of the committed `pascal-run-custom-{main,mount}.db`
 * pair's Inventory widen, in the `pascal-run-custom-announcer.ts` idiom: the
 * ids, the objects and the definitions, plus ONE idempotent
 * `addPascalInventoryShelf(ctx)`.
 *
 * v4's `custom-tools-measure-twice.test.ts` runs over an in-memory fake store;
 * this mirrors its seven `it`s over the REAL pair instead (R-9): on the trigger
 * room's Trigger Shelf (the project tier of `CHAT_TRIGGERS`) the archetypes
 * `tape measure` (`length_feet: 16`), `ansible resonator`, `ansible
 * walkie-talkie` and `ansible spyglass`; in CHAR_A's vault a TITLED tape
 * measure (`Friday's Yellow Tape Measure`, `length_feet: 25`), an untitled
 * resonator, a titled one (`Charlie's Resonator`) and an untitled
 * walkie-talkie; in CHAR_B's vault an UNTITLED tape measure; the shelf's five
 * definitions (`measure_twice` + the four `probe_*` gates); and the room's
 * carried sets (`{ A: [tape, both resonators, walkie], B: [tape] }`).
 *
 * `measure_twice` keeps v4's message but rolls `min === max` (the handler
 * family draws through the OS byte source on the v5 side, so `1d20` would
 * not diff) — `{{roll}}` still renders the raw draw.
 *
 * Everything goes through v4's REAL `resolveInventoryLocation` + `create`,
 * `storeMountFile` (the caller's `writeVaultFile`) and `setEquippedInventory`;
 * every id is pinned. P4.D328's extender (`extend-pascal-run-custom-inventory.ts`)
 * imports this module and makes ONE call on the union.
 */

import { CHAT_TRIGGERS, PROJECT_TRIGGERS, SHELF_MP } from './pascal-run-custom-triggers';

const CHAR_A = 'a1000000-0000-4000-8000-00000000000a';
const CHAR_B = 'a1000000-0000-4000-8000-00000000000b';

export const MEASURE_TWICE_TITLE = "Friday's Yellow Tape Measure";

/** Pinned object ids (`inventory` items carry their own `id`). */
export const INV_PASCAL_IDS = {
  tapeArchetype: 'f4000000-0000-4000-8000-000000000001',
  resonatorArchetype: 'f4000000-0000-4000-8000-000000000002',
  walkieArchetype: 'f4000000-0000-4000-8000-000000000003',
  spyglassArchetype: 'f4000000-0000-4000-8000-000000000004',
  tapeA: 'f4000000-0000-4000-8000-00000000000a',
  resonatorA: 'f4000000-0000-4000-8000-00000000001a',
  resonatorTitledA: 'f4000000-0000-4000-8000-00000000002a',
  walkieA: 'f4000000-0000-4000-8000-00000000003a',
  tapeB: 'f4000000-0000-4000-8000-00000000000b',
} as const;

const I = INV_PASCAL_IDS;

/** An archetype record, every field explicit (the keystone fixture's shape). */
function archetype(id: string, ts: string, name: string, metadata: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    id,
    name,
    title: null,
    description: null,
    imageDescription: null,
    kind: 'item',
    isArchetype: true,
    equippedByDefault: false,
    maxEquipped: 1,
    canCopy: true,
    emoji: null,
    svg: null,
    imageFileId: null,
    metadata,
    parentId: null,
    itemIds: [],
    folder: '',
    archivedAt: null,
    createdAt: ts,
    updatedAt: ts,
  };
}

/** An instance record: its overrides only, everything else inherited. */
function instance(id: string, ts: string, parentId: string, overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return { id, parentId, ...overrides, folder: '', createdAt: ts, updatedAt: ts };
}

/** The objects, by tier: [scope, owner, record]. */
export function pascalInventoryObjects(ts: string): Array<['project' | 'character', string, Record<string, unknown>]> {
  return [
    ['project', PROJECT_TRIGGERS, archetype(I.tapeArchetype, ts, 'tape measure', { length_feet: 16 })],
    ['project', PROJECT_TRIGGERS, archetype(I.resonatorArchetype, ts, 'ansible resonator')],
    ['project', PROJECT_TRIGGERS, archetype(I.walkieArchetype, ts, 'ansible walkie-talkie')],
    ['project', PROJECT_TRIGGERS, archetype(I.spyglassArchetype, ts, 'ansible spyglass')],
    ['character', CHAR_A, instance(I.tapeA, ts, I.tapeArchetype, { title: MEASURE_TWICE_TITLE, metadata: { length_feet: 25 } })],
    ['character', CHAR_A, instance(I.resonatorA, ts, I.resonatorArchetype)],
    ['character', CHAR_A, instance(I.resonatorTitledA, ts, I.resonatorArchetype, { title: "Charlie's Resonator" })],
    ['character', CHAR_A, instance(I.walkieA, ts, I.walkieArchetype)],
    ['character', CHAR_B, instance(I.tapeB, ts, I.tapeArchetype)],
  ];
}

/** The carried sets the room is seeded with. */
export const PASCAL_INVENTORY_EQUIPPED: Record<string, string[]> = {
  [CHAR_A]: [I.tapeA, I.resonatorA, I.resonatorTitledA, I.walkieA],
  [CHAR_B]: [I.tapeB],
};

/** v4's `MEASURE_TWICE`, rolled `min === max` (see the module doc). */
export const MEASURE_TWICE = {
  name: 'measure_twice',
  description:
    'Take the tape measure off the belt and check a measurement. Small household tool; primary purpose is live-testing inventory-gated custom tools end to end.',
  availableWhen: { inventory: { [MEASURE_TWICE_TITLE]: { eq: 'equipped' } } },
  roll: { min: 0.35, max: 0.35 },
  outcomes: [
    {
      when: true,
      state: 'info',
      message: `{{character}} unclips {{inventory.${MEASURE_TWICE_TITLE}.title}} — {{inventory.${MEASURE_TWICE_TITLE}.metadata.length_feet}} feet of yellow steel. The tape agrees with itself: {{roll}}. Measured twice.`,
    },
  ],
};

/** v4's `probe(availableWhen)`, named per gate. */
function probe(name: string, availableWhen: unknown): Record<string, unknown> {
  return {
    name,
    description: 'A minimal tool definition for exercising one gate shape.',
    availableWhen,
    roll: { min: 0.5, max: 0.5 },
    outcomes: [{ when: true, message: 'ok', state: 'success' }],
  };
}

/**
 * The run service's bag read (v4 `loadRunInventory`): ungated tools that name
 * the bag through ONE door each — a message placeholder, a parameter default,
 * a consult prompt — so a run reads the bag; `spell` (the shelf's) names none.
 */
const READS_IN_MESSAGE = {
  name: 'reads_in_message',
  description: 'Names the bag in its message only.',
  roll: { min: 0.5, max: 0.5 },
  outcomes: [{ when: true, message: 'The tape reads {{inventory.tape measure.metadata.length_feet}} feet.', state: 'info' }],
};
const READS_IN_DEFAULT = {
  name: 'reads_in_default',
  description: 'Names the bag in a parameter default only.',
  parameters: { feet: { type: 'number', default: { $inventory: 'tape measure.metadata.length_feet', fallback: 1 } } },
  roll: { min: 0.5, max: 0.5 },
  outcomes: [{ when: true, message: 'Measured {{params.feet}} feet.', state: 'info' }],
};
const READS_IN_PROMPT = {
  name: 'reads_in_prompt',
  description: 'Names the bag in its consult prompt only.',
  roll: { min: 0.5, max: 0.5 },
  llm: { prompt: 'Is {{inventory.tape measure.title}} long enough? Answer YES or NO.', errorMessage: 'The tape is silent.' },
  outcomes: [{ when: true, message: 'Consulted: {{llm}}', state: 'info' }],
};

/**
 * The trigger runner's path (v4 `runTriggeredCustomTools`): a spoken phrase
 * firing an `inventory` `pick_up` gesture. The runner passes NO pending map,
 * so the change posts its own Pascal line (`announcement: 'posted now'`).
 */
const POCKET_TAPE = {
  name: 'pocket_tape',
  description: 'Pocket a tape measure from the shelf.',
  triggers: ['pocket the tape'],
  roll: { min: 0.5, max: 0.5 },
  effects: [{ inventory: { action: 'pick_up', items: ['tape measure'], equip: false } }],
  outcomes: [{ when: true, message: 'Pocketed.', state: 'info' }],
};

export const PASCAL_INVENTORY_TOOLS: Array<Record<string, unknown>> = [
  MEASURE_TWICE,
  probe('probe_resonator', { inventory: { 'ansible resonator': { eq: 'equipped' } } }),
  probe('probe_resonator_title', { inventory: { "Charlie's Resonator": { eq: 'equipped' } } }),
  probe('probe_walkie', { inventory: { 'ansible walkie-talkie': { eq: 'equipped' } } }),
  probe('probe_spyglass', { inventory: { 'ansible spyglass': { eq: 'equipped' } } }),
  READS_IN_MESSAGE,
  READS_IN_DEFAULT,
  READS_IN_PROMPT,
  POCKET_TAPE,
];

export const pascalInventoryToolPath = (name: unknown): string => `Tools/${String(name)}.tool.json`;

export interface PascalInventoryShelfContext {
  repos: any; // eslint-disable-line @typescript-eslint/no-explicit-any
  writeVaultFile: (mountPointId: string, relativePath: string, body: unknown) => Promise<void>;
  userId: string;
  ts: string;
}

/**
 * Write the objects, the definitions and the carried sets. Idempotent: a pair
 * whose shelf already links `measure_twice` is left alone (returns false).
 */
export async function addPascalInventoryShelf(ctx: PascalInventoryShelfContext): Promise<boolean> {
  const { repos, writeVaultFile, userId, ts } = ctx;
  if (await repos.docMountFileLinks.findByMountPointAndPath(SHELF_MP, pascalInventoryToolPath(MEASURE_TWICE.name))) {
    return false;
  }
  const { resolveInventoryLocation } = await import('@/lib/inventory/location');
  for (const [scope, owner, rec] of pascalInventoryObjects(ts)) {
    const loc = await resolveInventoryLocation(scope as never, owner, repos, userId, { ensure: true });
    if (!loc) throw new Error(`no inventory location for ${scope}:${owner}`);
    await loc.create(rec as never);
  }
  for (const tool of PASCAL_INVENTORY_TOOLS) {
    await writeVaultFile(SHELF_MP, pascalInventoryToolPath(tool.name), tool);
  }
  for (const [characterId, ids] of Object.entries(PASCAL_INVENTORY_EQUIPPED)) {
    if (!(await repos.chats.setEquippedInventory(CHAT_TRIGGERS, characterId, ids))) {
      throw new Error(`carried-set plant failed for ${characterId}`);
    }
  }
  return true;
}
