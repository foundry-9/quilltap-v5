import { NgTemplateOutlet } from '@angular/common';
import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

import { CustomToolAggregateLine } from './custom-tool-aggregate-line';
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
 * sentence rides `a53024d3c`'s key; the component below places it between
 * the reads sentence and the gestures, as v4 does.)
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
 * v4's order (`CustomToolRunDialog.tsx:949-954`, R-9): the reads paragraph,
 * then the aggregate paragraph (`qt-custom-tool-aggregate-line`, which renders
 * nothing without keys), then the three gesture lines. The custom-tool popup
 * (`custom-tools-popup.ts`) mounts this after the wardrobe lines.
 */
@Component({
  selector: 'qt-custom-tool-inventory-lines',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [NgTemplateOutlet, CustomToolAggregateLine],
  template: `
    <ng-template #sentence let-line>
      <p class="text-xs qt-text-secondary mt-4 leading-relaxed">
        {{ line.lead }}
        @for (value of line.values; track value; let i = $index) {
          <span
            >@if (i > 0) {, }<code class="font-mono qt-text">{{ value }}</code></span
          >
        }{{ line.tail }}
      </p>
    </ng-template>
    @for (line of reads(); track line.key) {
      <ng-container *ngTemplateOutlet="sentence; context: { $implicit: line }" />
    }
    <qt-custom-tool-aggregate-line [references]="references()" />
    @for (line of gestures(); track line.key) {
      <ng-container *ngTemplateOutlet="sentence; context: { $implicit: line }" />
    }
  `,
})
export class CustomToolInventoryLines {
  readonly references = input<CustomToolReferences | undefined>();

  protected readonly lines = computed(() => inventoryReferenceLines(this.references()));
  protected readonly reads = computed(() => this.lines().filter((line) => line.key === 'inventoryReads'));
  protected readonly gestures = computed(() => this.lines().filter((line) => line.key !== 'inventoryReads'));
}
