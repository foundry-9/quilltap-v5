/**
 * P4.D329 (v4 `e1872c94f`) — the workbench pair's ONE `$inventory`-rolled
 * definition, shared by the builder (`build-workbench-fixture.ts`, so a future
 * full rebuild keeps it) and the in-place widen
 * (`extend-workbench-inventory-roll.ts`). Its library entry reads
 * `rollForm: "inventory"` (v4 `workbench.ts`).
 */

export const INVENTORY_ROLL_PATH = 'Tools/kappa_inventory.tool.json';

export const KAPPA_INVENTORY = {
  name: 'kappa_inventory',
  description: 'Rolls the damage the sword in hand carries.',
  roll: { $inventory: 'sword.metadata.damage', fallback: '1d4' },
  outcomes: [
    { when: { gte: 7 }, message: 'A telling blow ({{dice}}).', state: 'success' },
    { when: true, message: 'A glancing blow ({{dice}}).', state: 'failure' },
  ],
};
