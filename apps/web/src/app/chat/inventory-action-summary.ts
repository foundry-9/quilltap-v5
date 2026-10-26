/**
 * The Salon tool row's Inventory notice — a port of v4's module-private
 * `INVENTORY_ACTION_TOOLS`, `inventoryRowLabel`, `withArticle` and
 * `buildInventoryActionSummary` (`components/chat/ToolMessage.tsx:175-245`,
 * `e1872c94f`, at the pin `322a5488e`), lifted into their own module so the
 * corpus differential can import them (`inventory-action-summary.oracle.spec.ts`
 * replays v4's REAL functions row for row). `tool-message.ts` chains it after
 * the wardrobe's notice, as v4's `:335-338` does.
 *
 * **Recorded pair (R-6).** `withArticle` is the CLIENT rule verbatim —
 * `/^[aeiou]/i` → `an` — so a picked-up `unicorn horn` reads `an unicorn
 * horn` here while the server's Pascal line (`equip-effects.ts`
 * `indefiniteArticle`, with its `uni` / `use` / `one` exceptions) says `a`.
 * Not "fixed": v4 renders exactly this.
 *
 * @module chat/inventory-action-summary
 */

/** The parsed envelope fields the notice reads (v4 `ToolResult`). */
export interface InventorySummaryToolData {
  toolName?: string;
  success?: boolean;
  result?: unknown;
}

/** Inventory tool names that should show an action notice (v4 `:175-183`) — NOT list / read. */
export const INVENTORY_ACTION_TOOLS = new Set([
  'inventory_equip',
  'inventory_unequip',
  'inventory_pick_up',
  'inventory_create',
  'inventory_update',
  'inventory_archive',
  'inventory_give',
]);

/** A tool result's object row — the SPA reads only `name` / `title` (v4 `:185`). */
export interface InventorySummaryRow {
  name?: string;
  title?: string | null;
}

/** v4 `:187-190`. */
export function inventoryRowLabel(row: InventorySummaryRow | undefined): string | null {
  if (!row) return null;
  return row.title?.trim() || row.name || null;
}

/** "a magic bow" for an untitled object, "Excalibur" for a titled one (v4 `:192-197`). */
export function withArticle(row: InventorySummaryRow): string {
  if (row.title?.trim()) return row.title.trim();
  const name = row.name ?? 'object';
  return /^[aeiou]/i.test(name) ? `an ${name}` : `a ${name}`;
}

/**
 * Build a human-readable Inventory action summary from tool result data, one
 * line per tool ("Equipped Excalibur.", "Picked up a magic bow.", "Handed the
 * lantern to Marcus."). Null when not an Inventory action or not successful.
 * (v4 `:199-245`.)
 */
export function buildInventoryActionSummary(
  toolData: InventorySummaryToolData,
): { label: string; lines: string[] } | null {
  if (!toolData.success || !toolData.toolName || !INVENTORY_ACTION_TOOLS.has(toolData.toolName)) {
    return null;
  }
  const result = toolData.result as Record<string, unknown> | undefined;
  if (!result || typeof result !== 'object') return null;
  const lines: string[] = [];
  const item = result['item'] as InventorySummaryRow | undefined;
  const label = inventoryRowLabel(item);

  switch (toolData.toolName) {
    case 'inventory_equip': {
      const names = ((result['equipped'] as InventorySummaryRow[] | undefined) ?? [])
        .map(inventoryRowLabel)
        .filter(Boolean);
      if (names.length > 0) lines.push(`Equipped ${names.join(', ')}.`);
      break;
    }
    case 'inventory_unequip': {
      const names = ((result['unequipped'] as InventorySummaryRow[] | undefined) ?? [])
        .map(inventoryRowLabel)
        .filter(Boolean);
      if (names.length > 0) lines.push(`Set down ${names.join(', ')}.`);
      break;
    }
    case 'inventory_pick_up':
      if (item) lines.push(`Picked up ${withArticle(item)}${result['equipped'] ? ' and equipped it' : ''}.`);
      break;
    case 'inventory_create':
      if (label) lines.push(`Made "${label}"${result['equipped'] ? ' and equipped it' : ''}.`);
      break;
    case 'inventory_update':
      if (label) lines.push(`Updated "${label}".`);
      break;
    case 'inventory_archive':
      if (label) lines.push(result['archived'] ? `Put away "${label}".` : `Restored "${label}".`);
      break;
    case 'inventory_give': {
      const recipient = (result['recipient'] as { name?: string } | undefined)?.name;
      if (item && recipient) {
        lines.push(`Handed ${item.title?.trim() || `the ${item.name ?? 'object'}`} to ${recipient}.`);
      }
      break;
    }
  }
  return lines.length > 0 ? { label: 'Inventory', lines } : null;
}
