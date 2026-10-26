/**
 * The v4-side recorder behind `src/app/inventory/inventory-refusal.oracle.spec.ts`
 * (P4.D330).
 *
 * `describeInventoryRefusal` (`components/inventory/hooks/
 * useInventoryItemActions.ts:41-61` at the pin `322a5488e`, incl.
 * `7263105c6`'s title-lint join) turns a refused Inventory write into the
 * words a toast or an alert shows: the `CAP_EXCEEDED` and `NOT_COPYABLE`
 * sentences built from `details`, the multi-line title-lint refusals joined
 * with `; `, any other coded refusal's own message, a plain `Error`'s
 * message, and the fallback for anything else. This drives v4's REAL
 * function and its REAL `InventoryRequestError` (`lib/hooks/use-inventory.ts`);
 * the hook module's React / toast imports load under the pin's `node_modules`.
 *
 * This file lives OUTSIDE `src/` on purpose: it imports v4's `@/...`.
 *
 * Rows: `{ id, input, fallback, out }` — `input.kind` is `request` (an
 * `InventoryRequestError(message, 400, code, details)`; the SPA twin builds a
 * `CoreDispatchError` with the same `code` / `details`), `error` (a plain
 * `Error(message)`) or `other` (the raw `value`, thrown as-is).
 *
 * Run it from a pinned v4 worktree at `322a5488e` (Node 24):
 *
 * ```bash
 * PIN=/tmp/qt-v4-pin-p4d330-322a5488e
 * V5W=${V5W:-$HOME/source/quilltap-v5}
 * cp "$V5W/apps/web/oracle/inventory-refusal.recorder.ts" "$PIN/zz-p4d330-inventory-refusal.recorder.ts"
 * OUT="$V5W/apps/web/src/testing/fixtures/inventory-refusal.oracle.ndjson"
 * rm -f "$OUT"
 * (cd "$PIN" && PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH TZ=UTC \
 *   npx tsx zz-p4d330-inventory-refusal.recorder.ts > "$OUT")
 * wc -l "$OUT"                                   # expect 24
 * ```
 */

import { describeInventoryRefusal } from '@/components/inventory/hooks/useInventoryItemActions';
import { InventoryRequestError } from '@/lib/hooks/use-inventory';

type Input =
  | { kind: 'request'; message: string; code: string | null; details: Record<string, unknown> }
  | { kind: 'error'; message: string }
  | { kind: 'other'; value: unknown };

const rows: Array<{ id: string; input: Input; fallback: string }> = [];
const FALLBACK = 'The object would not change';
const req = (id: string, code: string | null, details: Record<string, unknown>, message = 'The server refused') =>
  rows.push({ id, input: { kind: 'request', message, code, details }, fallback: FALLBACK });

// CAP_EXCEEDED — full / partial / missing details.
req('cap-full', 'CAP_EXCEEDED', { itemName: 'Excalibur', cap: 1 });
req('cap-three', 'CAP_EXCEEDED', { itemName: 'arrow', cap: 3 });
req('cap-no-name', 'CAP_EXCEEDED', { cap: 2 });
req('cap-no-cap', 'CAP_EXCEEDED', { itemName: 'Excalibur' });
req('cap-empty', 'CAP_EXCEEDED', {});
req('cap-string-cap', 'CAP_EXCEEDED', { itemName: 'lamp', cap: '4' });
req('cap-null-name', 'CAP_EXCEEDED', { itemName: null, cap: 1 });
req('cap-numeric-name', 'CAP_EXCEEDED', { itemName: 7, cap: 1 });
// NOT_COPYABLE — absent / one / two / not an array.
req('copy-absent', 'NOT_COPYABLE', {});
req('copy-one', 'NOT_COPYABLE', { itemIds: ['a'] });
req('copy-two', 'NOT_COPYABLE', { itemIds: ['a', 'b'] });
req('copy-empty', 'NOT_COPYABLE', { itemIds: [] });
req('copy-not-array', 'NOT_COPYABLE', { itemIds: 'a,b' });
// The title lint — multi-line, blank lines, padding.
req('title-shadow-multi', 'TITLE_SHADOW', {}, '"Excalibur" shadows "sword" in Vivienne\'s vault\n"Excalibur" shadows "sword" in Quilltap General');
req('title-shadow-blank-padded', 'TITLE_SHADOW', {}, '  first line  \n\n   second line\t\n');
req('title-shadow-one', 'TITLE_SHADOW', {}, 'Only one collision');
req('title-check-unavailable', 'TITLE_CHECK_UNAVAILABLE', {}, 'The title check could not read a store\n  try again later');
// Any other coded refusal, and no code at all.
req('archetype-in-use', 'ARCHETYPE_IN_USE', { dependents: 2, holders: [] }, '"magic bow" has 2 instances');
req('unknown-code', 'KIT_NESTING', {}, 'A kit cannot hold a kit');
req('no-code', null, {}, 'Something went wrong\nwith two lines');
// Not an InventoryRequestError.
rows.push({ id: 'plain-error', input: { kind: 'error', message: 'Network down' }, fallback: FALLBACK });
rows.push({ id: 'plain-error-empty', input: { kind: 'error', message: '' }, fallback: FALLBACK });
rows.push({ id: 'non-error-string', input: { kind: 'other', value: 'a string' }, fallback: FALLBACK });
rows.push({ id: 'non-error-null', input: { kind: 'other', value: null }, fallback: 'The archetype would not be deleted' });

for (const row of rows) {
  const input = row.input;
  const error =
    input.kind === 'request'
      ? new InventoryRequestError(input.message, 400, input.code, input.details)
      : input.kind === 'error'
        ? new Error(input.message)
        : input.value;
  process.stdout.write(`${JSON.stringify({ ...row, out: describeInventoryRefusal(error, row.fallback) })}\n`);
}
