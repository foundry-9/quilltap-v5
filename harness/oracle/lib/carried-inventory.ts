/**
 * [4fbdad186] P4.D326 — plant what a character CARRIES on this case's fixture
 * copy, the twin of the Rust families' `common::plant_carried_inventory`: v4's
 * Inventory columns (C1's `plantInventoryColumns`), ONE object file in the
 * character's vault through the REAL `writeDatabaseDocument`, and the chat's
 * `equippedInventory` cell. The committed pairs are never widened.
 */
export interface CarrySpec {
  characterId: string;
  fileName: string;
  content: string;
  equippedInventory: string;
}

export async function plantCarriedInventory(chatId: string, carry: CarrySpec): Promise<void> {
  const { plantInventoryColumns } = await import('./v4-migrations');
  const { rawQuery } = await import('@/lib/database/manager');
  const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');
  await plantInventoryColumns();
  const rows = (await rawQuery('SELECT "characterDocumentMountPointId" AS m FROM characters WHERE id = ?', [
    carry.characterId,
  ])) as Array<{ m: string | null }>;
  const vault = rows[0]?.m;
  if (!vault) throw new Error(`plantCarriedInventory: no vault for ${carry.characterId}`);
  await rawQuery('UPDATE chats SET "equippedInventory" = ? WHERE id = ?', [carry.equippedInventory, chatId]);
  await writeDatabaseDocument(vault, `Inventory/${carry.fileName}`, carry.content);
}
