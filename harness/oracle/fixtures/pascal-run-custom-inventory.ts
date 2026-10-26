/**
 * P4.D328 (v4 `e1872c94f`) — the Inventory TOOLS' share of the committed
 * `pascal-run-custom-{main,mount}.db` pair's Inventory widen, in the
 * `pascal-run-custom-announcer.ts` idiom: the ids, the objects and the
 * carried set, plus ONE idempotent `addInventoryToolObjects(ctx)`.
 *
 * The ids are the ones P4.D328 and P4.D329 agreed (`INV_LANTERN_ID`,
 * `INV_EXCALIBUR_ID`, `INV_BOW_ARCHETYPE_ID`, `INV_KIT_ID`); P4.D329's own
 * share lives in `pascal-run-custom-inventory-pascal.ts` (its branch) and joins
 * `extend-pascal-run-custom-inventory.ts` on the union.
 *
 * The objects:
 *   - CHAR_A (Bertie): a TITLED lantern whose title carries an apostrophe
 *     (`Bertie's Lantern` — `cad017599`'s render row), and a KIT holding it;
 *   - CHAR_B (Jeeves): an UNTITLED `excalibur`;
 *   - The Ledger Office (the fixture's PROJECT): a bow ARCHETYPE on the
 *     project's own store.
 *   - the fixture chat's carried set: Bertie carries the lantern.
 *
 * ⚑ The project's roster is NOT narrowed to admit A and not B: the pair's
 * every family reads that project with both characters, and v4's roster gate
 * (`9753d0eb2`) would move Jeeves's tool access and custom-tool roster in
 * every one of them; no row in either lane reads a closed project shelf here
 * (the Inventory tools family builds its own closed project). Recorded in the
 * lane record.
 *
 * Everything goes through v4's REAL `resolveInventoryLocation(…, { ensure:
 * true }).create` and `setEquippedInventory`; every id is pinned.
 */

const CHAR_A = 'a1000000-0000-4000-8000-00000000000a';
const CHAR_B = 'a1000000-0000-4000-8000-00000000000b';
/** The fixture's PROJECT (The Ledger Office) and CHAT (both characters). */
const PROJECT = 'a3000000-0000-4000-8000-0000000000aa';
const CHAT = 'c1000000-0000-4000-8000-000000000001';

export const INV_LANTERN_ID = 'f5000000-0000-4000-8000-00000000000a';
export const INV_EXCALIBUR_ID = 'f5000000-0000-4000-8000-00000000000b';
export const INV_BOW_ARCHETYPE_ID = 'f5000000-0000-4000-8000-000000000001';
export const INV_KIT_ID = 'f5000000-0000-4000-8000-00000000001a';

/** The lantern's title — the apostrophe is the point. */
export const INV_LANTERN_TITLE = "Bertie's Lantern";

/** A full object record, every field explicit (the keystone fixture's shape). */
function record(
  id: string,
  ts: string,
  name: string,
  fields: Record<string, unknown> = {},
): Record<string, unknown> {
  return {
    id,
    name,
    title: null,
    description: null,
    imageDescription: null,
    kind: 'item',
    isArchetype: false,
    equippedByDefault: false,
    maxEquipped: 1,
    canCopy: true,
    emoji: null,
    svg: null,
    imageFileId: null,
    metadata: {},
    parentId: null,
    itemIds: [],
    folder: '',
    archivedAt: null,
    createdAt: ts,
    updatedAt: ts,
    ...fields,
  };
}

interface Ctx {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  repos: any;
  userId: string;
  ts: string;
}

/**
 * Add the share; `false` (and nothing written) when Bertie's vault already
 * holds the lantern.
 */
export async function addInventoryToolObjects({ repos, userId, ts }: Ctx): Promise<boolean> {
  const { resolveInventoryLocation } = await import('@/lib/inventory/location');
  const locationOf = async (scope: string, owner: string) => {
    const loc = await resolveInventoryLocation(scope as never, owner, repos, userId, { ensure: true });
    if (!loc) throw new Error(`no inventory location for ${scope}:${owner}`);
    return loc;
  };
  const vaultA = await locationOf('character', CHAR_A);
  if (await vaultA.findItem(INV_LANTERN_ID)) return false;

  await vaultA.create(record(INV_LANTERN_ID, ts, 'lantern', { title: INV_LANTERN_TITLE }) as never);
  await vaultA.create(record(INV_KIT_ID, ts, 'travel kit', { kind: 'kit', itemIds: [INV_LANTERN_ID] }) as never);
  await (await locationOf('character', CHAR_B)).create(record(INV_EXCALIBUR_ID, ts, 'excalibur') as never);
  await (await locationOf('project', PROJECT)).create(
    record(INV_BOW_ARCHETYPE_ID, ts, 'bow', { isArchetype: true, maxEquipped: 2 }) as never,
  );
  const ok = await repos.chats.setEquippedInventory(CHAT, CHAR_A, [INV_LANTERN_ID]);
  if (!ok) throw new Error('the carried-set plant failed');
  return true;
}
