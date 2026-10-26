import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  input,
  linkedSignal,
  output,
  untracked,
} from '@angular/core';

import { CoreClient } from '../core/core-client';
import type { ResolvedInventoryItem } from '../core/core-contract';
import type { WardrobeVantage } from '../pascal/tool-draft';
import { Modal } from '../ui/modal';
import { inventoryOriginLabel } from './inventory-container';
import { inventoryDisplayName } from './inventory-display';
import { InventoryPicture } from './inventory-glyph';
import { InventoryVantageItemsLoader } from './inventory-vantage-items';

/** How a picked object is written down: by its label (title, else name) or its exact id. */
export type InventoryReferenceForm = 'label' | 'id';

/**
 * The gestures a pick serves — structurally C4's `InventoryEffectAction`
 * (P4.D333 owns that grammar twin; this lane imports nothing new from
 * `pascal/**`, so P4.D334 passes its own value through).
 */
export type InventoryPickAction = 'equip' | 'unequip' | 'pick_up';

/**
 * Whether `action` may name this object, and why not when it may not (v4
 * `inventory-reference-picker.tsx:49-53`, spec-pinned — R-15).
 */
export function inventoryPickRule(item: Pick<ResolvedInventoryItem, 'isArchetype'>, action: InventoryPickAction): string | null {
  if (action === 'pick_up') return item.isArchetype ? null : 'Pick up names an archetype';
  if (item.isArchetype) return 'An archetype is picked up, not equipped';
  return null;
}

/** One row as the template draws it — memoised per item so bindings stay stable. */
interface PickerRow {
  item: ResolvedInventoryItem;
  name: string;
  refusal: string | null;
  origin: string | null;
}

/**
 * InventoryReferencePicker — choose an Inventory object to NAME, not to carry.
 * A port of v4 `components/inventory/inventory-reference-picker.tsx`
 * (`e1872c94f`), the Inventory twin of `qt-wardrobe-reference-picker`, for
 * Pascal's Workbench (C10 — P4.D334 mounts it):
 *
 * > browse what a vantage can reach, each object with its picture (a picker is
 * > a selection surface), where it lives and whether it is an archetype or a
 * > kit, and click one to pick it. `pick_up` names archetypes, so only
 * > archetypes are pickable for it; `equip` and `unequip` grey archetypes out.
 * > The picker knows nothing about drafts; it hands back the object and whether
 * > the author wants it referenced by its label (title, else name) or by id.
 *
 * The eligible objects sort first (a stable sort, so the vantage's order holds
 * within each half). The body renders only while `open`; the search and
 * Show archived reset per open (`linkedSignal` over `open`). The header control
 * (the vantage select) is projected through `[qtPickerHeader]`.
 *
 * **Recorded mechanism divergence** (the wardrobe picker's): v4's `BaseModal`
 * closes on Escape; `qt-modal` does not, so a document Escape listener closes
 * the PICKER only.
 */
@Component({
  selector: 'qt-inventory-reference-picker',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, InventoryPicture],
  host: { '(document:keydown.escape)': 'onEscape()' },
  template: `
    @if (open()) {
      <qt-modal title="Choose an object" maxWidth="2xl" (close)="closed.emit()">
        <div class="space-y-3">
          <div><ng-content select="[qtPickerHeader]"></ng-content></div>
          @if (vantage().kind === 'none') {
            <p class="text-sm qt-text-secondary">Choose whose inventory to browse above.</p>
          } @else if (loader.fetched() && loader.items().length === 0) {
            <p class="text-sm qt-text-secondary">Nothing is kept here yet. Objects are added from the Inventory.</p>
          } @else {
            <div class="flex items-center gap-2 flex-wrap">
              <input
                type="search"
                [value]="query()"
                (input)="query.set($any($event.target).value)"
                placeholder="Find an object"
                class="qt-input flex-1 min-w-48 text-sm"
                aria-label="Find an object"
              />
              <label class="flex items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  class="qt-checkbox"
                  [checked]="showArchived()"
                  (change)="showArchived.set($any($event.target).checked)"
                />
                Show archived
              </label>
            </div>
            <ul class="space-y-1" data-testid="inventory-reference-picker-list">
              @for (row of rows(); track row.item.id) {
                <li>
                  <button
                    type="button"
                    [attr.class]="
                      'flex w-full items-center gap-3 rounded border px-2 py-1 text-left ' +
                      (row.item.id === highlightId() ? 'qt-border-primary' : 'qt-border-default') +
                      ' ' +
                      (row.refusal ? 'opacity-50' : 'hover:opacity-80')
                    "
                    [disabled]="row.refusal !== null"
                    [attr.title]="row.refusal"
                    (click)="pick(row.item)"
                  >
                    <qt-inventory-picture [item]="row.item" [size]="40" />
                    <span class="min-w-0 flex-1">
                      <span class="block break-words text-sm text-foreground">{{ row.name }}</span>
                      <span class="flex flex-wrap items-center gap-1">
                        @if (row.item.isArchetype) {
                          <span class="qt-badge qt-badge-info">archetype</span>
                        }
                        @if (row.item.kind === 'kit') {
                          <span class="qt-badge qt-badge-outline">kit</span>
                        }
                        @if (row.origin) {
                          <span class="qt-badge qt-badge-secondary">{{ row.origin }}</span>
                        }
                        @if (row.item.archivedAt) {
                          <span class="qt-badge qt-badge-secondary">archived</span>
                        }
                      </span>
                    </span>
                  </button>
                </li>
              }
            </ul>
          }
        </div>
        <div qt-modal-footer class="flex items-center justify-between w-full gap-2 flex-wrap">
          <label class="flex items-center gap-2 text-sm">
            Write it down by
            <select
              class="qt-select qt-select-sm"
              aria-label="Reference form"
              (change)="referenceChange.emit($any($event.target).value)"
            >
              <option value="label" [selected]="reference() === 'label'">title or name</option>
              <option value="id" [selected]="reference() === 'id'">id</option>
            </select>
          </label>
          <button type="button" class="qt-button qt-button-secondary qt-button-sm" (click)="closed.emit()">Done</button>
        </div>
      </qt-modal>
    }
  `,
})
export class InventoryReferencePicker {
  private readonly core = inject(CoreClient);

  readonly open = input(false);
  readonly vantage = input.required<WardrobeVantage>();
  /** The gesture the pick is for: pick_up wants archetypes; the others want anything but. */
  readonly action = input.required<InventoryPickAction>();
  readonly highlightId = input<string | null>(null);
  readonly reference = input<InventoryReferenceForm>('label');
  readonly referenceChange = output<InventoryReferenceForm>();
  /** v4 `onPick(item, { reference })` — the picker closes after. */
  readonly picked = output<{ item: ResolvedInventoryItem; reference: InventoryReferenceForm }>();
  readonly closed = output<void>();

  protected readonly loader = new InventoryVantageItemsLoader(this.core);
  protected readonly query = linkedSignal({ source: this.open, computation: () => '' });
  protected readonly showArchived = linkedSignal({ source: this.open, computation: () => false });

  /** v4 `:69-75` — archived hidden unless asked, the search on the display name, the eligible first. */
  protected readonly rows = computed<PickerRow[]>(() => {
    const q = this.query().trim().toLowerCase();
    const action = this.action();
    const showArchived = this.showArchived();
    return this.loader
      .items()
      .filter((item) => showArchived || !item.archivedAt)
      .filter((item) => !q || inventoryDisplayName(item).toLowerCase().includes(q))
      .sort((a, b) => Number(b.isArchetype === (action === 'pick_up')) - Number(a.isArchetype === (action === 'pick_up')))
      .map((item) => ({
        item,
        name: inventoryDisplayName(item),
        refusal: inventoryPickRule(item, action),
        origin: inventoryOriginLabel(item.origin),
      }));
  });

  constructor() {
    // Read while open, on every vantage change (v4 mounts the body — and its
    // hook — only while open).
    effect(() => {
      const open = this.open();
      const vantage = this.vantage();
      untracked(() => {
        if (open) void this.loader.load(vantage);
      });
    });
  }

  protected pick(item: ResolvedInventoryItem): void {
    this.picked.emit({ item, reference: this.reference() });
    this.closed.emit();
  }

  protected onEscape(): void {
    if (this.open()) this.closed.emit();
  }
}
