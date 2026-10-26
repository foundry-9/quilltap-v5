import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

import type { CustomToolReferences } from './custom-tools.api';

/** One inventory sentence of the run dialog's reference panel. */
export interface InventoryReferenceLine {
  key: 'inventoryReads' | 'equips' | 'unequips' | 'picksUp';
  lead: string;
  values: string[];
  /** The sentence's close, after the values. */
  tail: string;
}

/** The reads sentence's close (v4 `:1056`). */
export const INVENTORY_READS_TAIL = '. Run as nobody, it finds no inventory at all.';
/** The three gesture sentences' close, with its em dash (v4 `:1081`). */
export const INVENTORY_LINE_TAIL = '. Only when you run it as a character — a run made as nobody carries nothing.';

/**
 * The run dialog's inventory sentences (v4 `CustomToolRunDialog.tsx:948-955`,
 * `e1872c94f`, pinned at `322a5488e`): what a tool reads of the running
 * character's inventory, and what its inventory effects may hand them or take
 * away — from the roster's `references` (the server's tool vocabulary). The
 * reads sentence comes first, then equipping, unequipping and picking up, each
 * its own promise; a list with nothing in it gets no sentence. An older server
 * sends no keys, which reads as no lines. (v4's `inventoryAggregates`
 * sentence rides `a53024d3c`'s key, which is not this lane's — P4.D334's
 * popup mounts it beside these.)
 */
export function inventoryReferenceLines(refs: CustomToolReferences | undefined): InventoryReferenceLine[] {
  const lines: InventoryReferenceLine[] = [
    { key: 'inventoryReads', lead: 'It reads the inventory for', values: refs?.inventoryReads ?? [], tail: INVENTORY_READS_TAIL },
    { key: 'equips', lead: 'It may equip the running character with', values: refs?.equips ?? [], tail: INVENTORY_LINE_TAIL },
    { key: 'unequips', lead: 'It may unequip them of', values: refs?.unequips ?? [], tail: INVENTORY_LINE_TAIL },
    { key: 'picksUp', lead: 'It may pick up for them', values: refs?.picksUp ?? [], tail: INVENTORY_LINE_TAIL },
  ];
  return lines.filter((line) => line.values.length > 0);
}

/**
 * The inventory sentences as rendered markup — one paragraph per line, in the
 * popup's writes-paragraph idiom (the values as `<code>`, joined by `, `).
 * Renders NOTHING when every list is empty or `references` is undefined.
 *
 * Standalone so it is proven alone: the custom-tool popup (`custom-tools-
 * popup.ts`, P4.D334's file this round) mounts it — `HANDOFF: P4.D334` with
 * three edits: the import, `|| inventoryLines().length > 0` on the reference
 * panel's `@if`, and `<qt-custom-tool-inventory-lines
 * [references]="tool.references" />` after the wardrobe lines.
 */
@Component({
  selector: 'qt-custom-tool-inventory-lines',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @for (line of lines(); track line.key) {
      <p class="text-xs qt-text-secondary mt-4 leading-relaxed">
        {{ line.lead }}
        @for (value of line.values; track value; let i = $index) {
          <span
            >@if (i > 0) {, }<code class="font-mono qt-text">{{ value }}</code></span
          >
        }{{ line.tail }}
      </p>
    }
  `,
})
export class CustomToolInventoryLines {
  readonly references = input<CustomToolReferences | undefined>();

  protected readonly lines = computed(() => inventoryReferenceLines(this.references()));
}
