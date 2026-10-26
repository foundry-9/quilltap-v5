import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

import type { ReferencesAtPin } from '../screens/custom-tools/p4d330-stand-ins';
import type { CustomToolReferences } from './custom-tools.api';

/** The aggregate sentence's lead (v4 `CustomToolRunDialog.tsx:1062`, `a53024d3c`). */
export const INVENTORY_AGGREGATE_LEAD = 'It takes stock of everything they carry, reading';

/** Its close — the same as the reads sentence's (`:1070`). */
export const INVENTORY_AGGREGATE_TAIL = '. Run as nobody, it finds no inventory at all.';

/**
 * The `inventoryAggregate` keys a tool tests, in the server's order, verbatim
 * (`equipped.names`, `owned.metadata.toolVersion.max`…). Absent on an older
 * server, which reads as none. The key is read through {@link ReferencesAtPin}
 * until the unifier adds `inventoryAggregates?` to `CustomToolReferences` in
 * place (P4.D334 R-10).
 */
export function inventoryAggregateKeys(refs: CustomToolReferences | undefined): string[] {
  return (refs as ReferencesAtPin | undefined)?.inventoryAggregates ?? [];
}

/**
 * The run dialog's aggregate paragraph (v4 `CustomToolRunDialog.tsx:1060-1071`,
 * `a53024d3c`): a tool that tests `inventoryAggregate` keys takes stock of the
 * running character's whole bag, and says so. One paragraph, the keys as
 * `<code>` joined by `, `; NOTHING when the list is empty.
 *
 * P4.D330's `qt-custom-tool-inventory-lines` renders the reads paragraph and the
 * three gesture lines; v4 puts THIS paragraph between them, which the unifier
 * arranges (`HANDOFF:`, R-9). The `custom-tool-wardrobe-lines.ts` precedent.
 */
@Component({
  selector: 'qt-custom-tool-aggregate-line',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (keys().length > 0) {
      <p class="text-xs qt-text-secondary mt-4 leading-relaxed">
        {{ LEAD }}
        @for (key of keys(); track key; let i = $index) {
          <span
            >@if (i > 0) {, }<code class="font-mono qt-text">{{ key }}</code></span
          >
        }{{ TAIL }}
      </p>
    }
  `,
})
export class CustomToolAggregateLine {
  readonly references = input<CustomToolReferences | undefined>();

  protected readonly keys = computed(() => inventoryAggregateKeys(this.references()));
  protected readonly LEAD = INVENTORY_AGGREGATE_LEAD;
  protected readonly TAIL = INVENTORY_AGGREGATE_TAIL;
}
