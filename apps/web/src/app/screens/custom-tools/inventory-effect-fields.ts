import { NgTemplateOutlet } from '@angular/common';
import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import {
  MAX_INVENTORY_EFFECT_ITEMS,
  MAX_INVENTORY_REF_LENGTH,
  type InventoryEffectAction,
} from '../../pascal/custom-tool-types';
import { resolveInventoryRef, type ResolvedInventoryPool } from '../../pascal/inventory-refs';
import type { DraftInventoryGesture, WardrobeVantage } from '../../pascal/tool-draft';
import { Icon } from '../../ui/icon';
import type { ResolvedInventoryItem } from '../../core/core-contract';
import { inventoryOriginLabel } from '../../inventory/inventory-container';
import { inventoryDisplayName, inventoryLabel } from '../../inventory/inventory-display';
import { InventoryPicture } from '../../inventory/inventory-glyph';
import { inventoryPickRule } from '../../inventory/inventory-reference-picker';

/** v4 `InventoryEffectFields.tsx:35-39` — the Action select's options (`pick up`'s apostrophe is curly). */
export const INVENTORY_ACTIONS: ReadonlyArray<{ value: InventoryEffectAction; label: string; title: string }> = [
  { value: 'equip', label: 'equip', title: 'The rolling character takes up these objects (or kits)' },
  { value: 'unequip', label: 'unequip', title: 'The rolling character sets these objects down' },
  { value: 'pick_up', label: 'pick up', title: 'Make an instance of each archetype in the rolling character’s vault' },
];

/** Row 2+'s placeholder — a `{{…}}` would close an Angular interpolation, hence a constant. */
const ITEM_PARAM_PLACEHOLDER = '{{params.loot}}';

/** The hint's code span, for the same reason. */
const ITEM_HINT_CODE = '{{params.x}}';

const NO_EQUIPPED: ReadonlySet<string> = new Set();

/** One object row, as the template draws it — memoised per row so bindings stay stable. */
interface ObjectRow {
  ref: string;
  /** The object the reference resolves to through the vantage, or null. */
  item: ResolvedInventoryItem | null;
  templated: boolean;
  /** Only for a resolved row: the reference is the object's id. */
  byId: boolean;
  displayName: string;
  /** The chip's `aria-label`. */
  chipLabel: string;
  origin: string | null;
  /** The per-action rule the object breaks, or null. */
  refusal: string | null;
  /** The unresolved row's advisory, or null. */
  unresolvedNote: string | null;
  /** The text row's placeholder. */
  placeholder: string;
}

/**
 * InventoryEffectFields — an Inventory side-effect row's fields: the action
 * (equip / unequip / pick up), up to eight object references, and the "equip on
 * pick-up" switch (v4 `components/custom-tools/InventoryEffectFields.tsx`,
 * `e1872c94f`).
 *
 * Each reference is read through the editor's VANTAGE (the same one the
 * wardrobe rows use, set by `qt-wardrobe-vantage-select`) and resolves by the
 * one rule the applier uses (`resolveInventoryRef`, archetypes and archived
 * included so the chip can say what it found). A resolved reference renders as
 * a chip with the object's picture, where it lives and whether it is an
 * archetype or a kit; a templated one keeps its text box ("chosen at run
 * time"); an unresolved one keeps its text box under an advisory — the lens is
 * partial and the applier fail-soft. *Choose…* asks the host to open the
 * Inventory picker.
 *
 * The first match whose per-action rule passes wins over the first match
 * (v4's `resolveRow`); each row is resolved once per `(ref, action, pool)` in a
 * `computed`, so the row objects the template binds are stable (NG0100).
 */
@Component({
  selector: 'qt-inventory-effect-fields',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, NgTemplateOutlet, InventoryPicture],
  template: `
    <div class="space-y-2 text-sm">
      <div class="flex items-center gap-2 flex-wrap">
        <label class="flex items-center gap-2">
          Action
          <select
            (change)="set({ action: asAction(selectValue($event)) })"
            [disabled]="disabled()"
            class="qt-select qt-select-sm"
            aria-label="Inventory action"
          >
            @for (action of ACTIONS; track action.value) {
              <option [value]="action.value" [title]="action.title" [selected]="action.value === gesture().action">
                {{ action.label }}
              </option>
            }
          </select>
        </label>
        @if (gesture().action === 'pick_up') {
          <label class="flex items-center gap-2">
            <input
              type="checkbox"
              class="qt-checkbox"
              [checked]="gesture().equip"
              (change)="set({ equip: checkedValue($event) })"
              [disabled]="disabled()"
            />
            equip it at once
          </label>
        }
      </div>

      <div class="space-y-1">
        @for (row of rows(); track $index; let index = $index) {
          @if (row.item; as item) {
            <div class="space-y-1">
              <div class="flex items-center gap-2">
                <button
                  type="button"
                  class="flex flex-1 min-w-48 items-center gap-2 rounded border qt-border-default px-2 py-1 text-left hover:opacity-80"
                  (click)="choose.emit({ index, highlightId: item.id })"
                  [disabled]="disabled()"
                  [attr.aria-label]="row.chipLabel"
                  data-testid="inventory-reference-chip"
                >
                  <qt-inventory-picture [item]="item" [size]="28" />
                  <span class="min-w-0 flex-1 space-y-0.5">
                    <span class="block break-words text-sm text-foreground">{{ row.displayName }}</span>
                    <span class="flex flex-wrap items-center gap-1">
                      @if (item.isArchetype) {
                        <span class="qt-badge qt-badge-info">archetype</span>
                      }
                      @if (item.kind === 'kit') {
                        <span class="qt-badge qt-badge-outline">kit</span>
                      }
                      @if (row.origin) {
                        <span class="qt-badge qt-badge-secondary">{{ row.origin }}</span>
                      }
                      @if (item.archivedAt) {
                        <span class="qt-badge qt-badge-secondary">archived</span>
                      }
                    </span>
                  </span>
                </button>
                <button
                  type="button"
                  class="qt-button qt-button-ghost qt-button-sm"
                  (click)="setItem(index, row.byId ? labelOf(item) : item.id)"
                  [disabled]="disabled()"
                  [title]="row.byId ? 'Referenced by id — click to name it instead' : 'Referenced by name — click to use its id'"
                >
                  {{ row.byId ? 'by id' : 'by name' }}
                </button>
                <ng-container *ngTemplateOutlet="trash; context: { $implicit: index }" />
              </div>
              @if (row.refusal) {
                <p class="text-xs qt-text-secondary">⚠ {{ row.refusal }}</p>
              }
            </div>
          } @else {
            <div class="space-y-1">
              <div class="flex items-center gap-2 flex-wrap">
                <input
                  type="text"
                  [value]="row.ref"
                  [attr.maxlength]="MAX_REF_LENGTH"
                  (input)="setItem(index, inputValue($event))"
                  [placeholder]="row.placeholder"
                  [disabled]="disabled()"
                  class="qt-input flex-1 min-w-48"
                  [class.qt-input-error]="hasErrors()"
                  [attr.aria-label]="'Object ' + (index + 1)"
                />
                @if (row.unresolvedNote) {
                  <span class="qt-badge qt-badge-secondary" data-testid="inventory-reference-unresolved">{{
                    row.unresolvedNote
                  }}</span>
                }
                <button
                  type="button"
                  class="qt-button qt-button-secondary qt-button-sm"
                  (click)="choose.emit({ index, highlightId: null })"
                  [disabled]="disabled()"
                  title="Browse the inventory and pick an object"
                >
                  Choose…
                </button>
                <ng-container *ngTemplateOutlet="trash; context: { $implicit: index }" />
              </div>
              @if (row.templated) {
                <p class="qt-hint">chosen at run time</p>
              }
            </div>
          }
        }
        <div class="flex items-center gap-2">
          <button
            type="button"
            class="qt-button qt-button-ghost qt-button-sm"
            (click)="addItem()"
            [disabled]="disabled() || items().length >= MAX_ITEMS"
            [attr.title]="items().length >= MAX_ITEMS ? atMostTitle : null"
          >
            <qt-icon name="plus" class="w-3.5 h-3.5" />
            Add object
          </button>
          <span class="qt-hint">
            an id, title or name — <code class="font-mono">{{ ITEM_HINT_CODE }}</code> lets a parameter choose
          </span>
        </div>
      </div>
    </div>

    <ng-template #trash let-index>
      <button
        type="button"
        class="qt-button qt-button-ghost qt-button-sm"
        (click)="removeItem(index)"
        [disabled]="disabled() || items().length <= 1"
        title="Remove this object"
      >
        <qt-icon name="trash" class="w-4 h-4" />
      </button>
    </ng-template>
  `,
})
export class InventoryEffectFields {
  readonly gesture = input.required<DraftInventoryGesture>();
  readonly hasErrors = input(false);
  readonly disabled = input(false);
  /** The vantage's pool (v4 `ResolvedInventoryPool`), archived included. */
  readonly pool = input<ResolvedInventoryPool>({ items: [], owns: () => false });
  /** False until the pool's read settles — no advisory before then. */
  readonly poolFetched = input(false);
  readonly vantage = input<WardrobeVantage>({ kind: 'none' });
  readonly vantageName = input<string | null>(null);

  readonly gestureChange = output<DraftInventoryGesture>();
  /** Open the picker for object row `index`, highlighting the object it names. */
  readonly choose = output<{ index: number; highlightId: string | null }>();

  protected readonly ACTIONS = INVENTORY_ACTIONS;
  protected readonly MAX_ITEMS = MAX_INVENTORY_EFFECT_ITEMS;
  protected readonly MAX_REF_LENGTH = MAX_INVENTORY_REF_LENGTH;
  protected readonly ITEM_HINT_CODE = ITEM_HINT_CODE;
  protected readonly atMostTitle = `At most ${MAX_INVENTORY_EFFECT_ITEMS} objects`;
  protected readonly labelOf = inventoryLabel;

  /** v4's `items.length > 0 ? items : ['']` — the form always shows one row. */
  protected readonly items = computed(() => {
    const items = this.gesture().items;
    return items.length > 0 ? items : [''];
  });

  /** Each row, resolved by the one rule the applier uses (none → nothing resolves). */
  protected readonly rows = computed<ObjectRow[]>(() => {
    const vantage = this.vantage();
    const pool = this.pool();
    const action = this.gesture().action;
    const poolFetched = this.poolFetched();
    const vantageName = this.vantageName();
    return this.items().map((ref, index) => {
      const templated = ref.includes('{{');
      const item = vantage.kind === 'none' || templated ? null : resolveRow(ref, pool, action);
      if (!item) {
        const unresolved = !templated && vantage.kind !== 'none' && poolFetched && ref.trim() !== '';
        return {
          ref,
          item: null,
          templated,
          byId: false,
          displayName: '',
          chipLabel: '',
          origin: null,
          refusal: null,
          unresolvedNote: unresolved ? `not in ${vantageName ?? 'this inventory'}` : null,
          placeholder: index === 0 ? (action === 'pick_up' ? 'magic bow' : 'Excalibur') : ITEM_PARAM_PLACEHOLDER,
        };
      }
      return {
        ref,
        item,
        templated,
        byId: ref.trim() === item.id,
        displayName: inventoryDisplayName(item),
        chipLabel: `Object ${index + 1}: ${inventoryLabel(item)} — choose another`,
        origin: inventoryOriginLabel(item.origin),
        refusal: inventoryPickRule(item, action),
        unresolvedNote: null,
        placeholder: '',
      };
    });
  });

  set(partial: Partial<DraftInventoryGesture>): void {
    const current = this.gesture();
    this.gestureChange.emit({ ...current, ...partial });
  }

  setItem(index: number, value: string): void {
    this.set({ items: this.items().map((item, i) => (i === index ? value : item)) });
  }

  /** Never below one blank row. */
  removeItem(index: number): void {
    const next = this.items().filter((_, i) => i !== index);
    this.set({ items: next.length > 0 ? next : [''] });
  }

  addItem(): void {
    this.set({ items: [...this.items(), ''] });
  }

  protected asAction(value: string): InventoryEffectAction {
    return value as InventoryEffectAction;
  }

  protected inputValue(event: Event): string {
    return (event.target as HTMLInputElement).value;
  }

  protected selectValue(event: Event): string {
    return (event.target as HTMLSelectElement).value;
  }

  protected checkedValue(event: Event): boolean {
    return (event.target as HTMLInputElement).checked;
  }
}

/**
 * v4's `resolveRow`: the first match whose per-action rule passes, else the
 * first match, over the reference rule with archetypes and archived objects
 * included so the chip can say what it found.
 */
function resolveRow(
  ref: string,
  pool: ResolvedInventoryPool,
  action: InventoryEffectAction,
): ResolvedInventoryItem | null {
  const matches = resolveInventoryRef(ref, pool, NO_EQUIPPED, { includeArchetypes: true, includeArchived: true });
  return matches.find((match) => inventoryPickRule(match.item, action) === null)?.item ?? matches[0]?.item ?? null;
}
