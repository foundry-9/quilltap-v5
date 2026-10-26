/**
 * The words for a refused Inventory write — v4 `describeInventoryRefusal`
 * (`components/inventory/hooks/useInventoryItemActions.ts:41-61` at
 * `322a5488e`, with `7263105c6`'s title-lint join). Pinned row for row
 * against v4's REAL function by `inventory-refusal.oracle.spec.ts`.
 *
 * v4 tests `instanceof InventoryRequestError`; the dispatch transports throw
 * a {@link CoreDispatchError} carrying the server's `code` and, by R-L, its
 * `details` — so that is the class read here. v4's `status` has no twin and
 * nothing branches on it (every branch is on `code`).
 *
 * @module inventory/inventory-refusal
 */

import { CoreDispatchError } from '../core/core-contract';

/** v4 `useInventoryItemActions.ts:39`. */
export const plural = (n: number, word: string): string => `${n} ${word}${n === 1 ? '' : 's'}`;

/** A refused write's `details` (R-L) — `{}` when the server sent none. */
export function refusalDetails(error: CoreDispatchError): Record<string, unknown> {
  return error.details ?? {};
}

/** The words for a refusal the server answered with a code. */
export function describeInventoryRefusal(error: unknown, fallback: string): string {
  if (error instanceof CoreDispatchError) {
    const details = refusalDetails(error);
    if (error.code === 'CAP_EXCEEDED') {
      const name = String(details['itemName'] ?? 'That object');
      const cap = Number(details['cap'] ?? 1);
      return `"${name}" can be carried at most ${cap} at a time — set one down before taking up another.`;
    }
    if (error.code === 'NOT_COPYABLE') {
      const itemIds = details['itemIds'];
      const count = Array.isArray(itemIds) ? itemIds.length : 1;
      return count === 1
        ? 'That object is one of a kind and cannot be copied — tick "Can be copied" in its editor first, or move it instead.'
        : `${count} of those objects are one of a kind and cannot be copied — tick "Can be copied" in their editors first, or move them instead.`;
    }
    // The title lint's refusal is one line per collision; a toast shows it as one line.
    if (error.code === 'TITLE_SHADOW' || error.code === 'TITLE_CHECK_UNAVAILABLE') {
      return error.message
        .split('\n')
        .map((line) => line.trim())
        .join('; ');
    }
    return error.message;
  }
  return error instanceof Error ? error.message : fallback;
}
