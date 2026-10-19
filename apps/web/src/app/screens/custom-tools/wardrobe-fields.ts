import { NgTemplateOutlet } from '@angular/common';
import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import {
  MAX_WARDROBE_EFFECT_ITEMS,
  MAX_WARDROBE_ITEM_REF_LENGTH,
  type WardrobeEffectAction,
  type WardrobeEffectMode,
  type WardrobeItemType,
} from '../../pascal/custom-tool-types';
import type { WardrobeItemDto } from '../../core/core-contract';
import type { DraftWardrobeGesture, WardrobeVantage } from '../../pascal/tool-draft';
import { Icon } from '../../ui/icon';
import { isComposite } from '../../wardrobe/equipped-slots';
import { WARDROBE_SLOT_META, WARDROBE_SLOT_TYPES, formatSlotLabels } from '../../wardrobe/slot-meta';
import { SlotBadge } from '../../wardrobe/slot-ui';
import { archivedWearMessage } from '../../wardrobe/staged-live-outfits';
import { wardrobeOriginLabel, wardrobeReachLabel } from '../../wardrobe/wardrobe-container';
import { WardrobeItemThumbnail } from '../../wardrobe/wardrobe-item-thumbnail';
import { REFERENCE_FORM_HINT } from '../../wardrobe/wardrobe-reference-picker';
import { isTemplatedWearableRef, resolveWearableRef } from '../../wardrobe/wearable-ref';

/** v4 `SideEffectsSection.tsx:288-292` — the Action select's options, labels lowercase. */
export const WARDROBE_ACTIONS: ReadonlyArray<{ value: WardrobeEffectAction; label: string }> = [
  { value: 'wear', label: 'wear' },
  { value: 'take_off', label: 'take off' },
  { value: 'clear_slot', label: 'clear a slot' },
];

/** v4 `SideEffectsSection.tsx:294-298` — the Mode select's options, each with its `<option title>`. */
export const WARDROBE_MODES: ReadonlyArray<{ value: WardrobeEffectMode; label: string; title: string }> = [
  { value: 'wear', label: 'as the item says', title: "Honour each item's own replace flag" },
  { value: 'replace', label: 'replace', title: 'Swap out whatever is in the slots the item covers' },
  { value: 'add_to_slot', label: 'layer into one slot', title: 'Add the item to one named slot' },
];

/** Row 2+'s placeholder — a `{{…}}` would close an Angular interpolation, hence a constant. */
const ITEM_PARAM_PLACEHOLDER = '{{params.garment}}';

/** The hint's code span, for the same reason. */
const ITEM_HINT_CODE = '{{params.x}}';

/**
 * True when the gesture's slot and its items must agree: `add_to_slot` layers
 * into the one slot, and a `take_off` may be restricted to one. (v4
 * `5cb0e0190`, `SideEffectsSection.tsx:420-426`.)
 */
export function slotIsCoupled(gesture: DraftWardrobeGesture): boolean {
  return (gesture.action === 'wear' && gesture.mode === 'add_to_slot') || gesture.action === 'take_off';
}

/** One item row, as the template draws it — memoised per row so bindings stay stable. */
interface ItemRow {
  ref: string;
  /** The garment the reference resolves to through the vantage, or null. */
  item: WardrobeItemDto | null;
  templated: boolean;
  /** Only for a resolved row: the reference is the item's id. */
  byId: boolean;
  composite: boolean;
  originLabel: string;
  reach: string | null;
  /** The resolved chip's `aria-label`. */
  chipLabel: string;
  toggleTitle: string;
  /** `⚠ Item "<title>" is archived and cannot be worn`, or null. */
  archivedWarning: string | null;
  /** The unresolved row's advisory, or null. */
  unresolvedNote: string | null;
}

/**
 * WardrobeFields — a wardrobe effect row's fields: action, items, and the mode
 * and slot when the action wants them (v4 `SideEffectsSection.tsx:308-426`,
 * `f0db8cbb8`, pinned at `25b8fab6a`; its own component there too, so its
 * visibility rules are proven without a draft scaffold).
 *
 * Mode shows only on `wear`; Slot on `clear_slot`, `take_off`, and `wear` +
 * `add_to_slot` — optional on `take_off` (its blank option reads "every slot
 * it covers"), required elsewhere ("choose a slot"); item rows on everything
 * but `clear_slot`, never fewer than one. The slot list is the wardrobe
 * registry's (`WARDROBE_SLOT_TYPES` + `WARDROBE_SLOT_META`), imported, never
 * minted.
 *
 * Every edit reads the gesture input ONCE into a local and emits the spread of
 * it — v4's `set(partial)` spreads the PREVIOUS gesture, and a signal port must
 * not re-read state it is about to replace.
 *
 * **The wardrobe lens (v4 `5cb0e0190`).** Each item reference resolves by the
 * one rule (`resolveWearableRef`) against the vantage's pool and renders as a
 * chip — picture, slots, origin, reach, `archived`, and a `by title` / `by id`
 * toggle; a templated reference keeps its text box ("chosen at run time"); an
 * unresolved one keeps its text box under a `not in …` advisory that is never a
 * draft error — the lens is partial and the applier fail-soft. When the
 * gesture couples items to a slot and every concrete row resolved, slots the
 * items do not cover read ` (not covered)` and a mismatch earns a `⚠ “…” covers
 * … — not …` line. `choose` asks the host to open the picker for a row.
 */
@Component({
  selector: 'qt-wardrobe-fields',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, NgTemplateOutlet, SlotBadge, WardrobeItemThumbnail],
  template: `
    <div class="space-y-2 text-sm">
      <div class="flex items-center gap-2 flex-wrap">
        <label class="flex items-center gap-2">
          Action
          <select
            [value]="gesture().action"
            (change)="set({ action: asAction(selectValue($event)) })"
            [disabled]="disabled()"
            class="qt-select qt-select-sm"
            aria-label="Wardrobe action"
          >
            @for (action of ACTIONS; track action.value) {
              <option [value]="action.value" [selected]="action.value === gesture().action">
                {{ action.label }}
              </option>
            }
          </select>
        </label>
        @if (gesture().action === 'wear') {
          <label class="flex items-center gap-2">
            Mode
            <select
              [value]="gesture().mode"
              (change)="set({ mode: asMode(selectValue($event)) })"
              [disabled]="disabled()"
              class="qt-select qt-select-sm"
              aria-label="Wear mode"
            >
              @for (mode of MODES; track mode.value) {
                <option [value]="mode.value" [title]="mode.title" [selected]="mode.value === gesture().mode">
                  {{ mode.label }}
                </option>
              }
            </select>
          </label>
        }
        @if (wantsSlot()) {
          <label class="flex items-center gap-2">
            Slot
            <select
              [value]="gesture().slot"
              (change)="set({ slot: asSlot(selectValue($event)) })"
              [disabled]="disabled()"
              class="qt-select qt-select-sm"
              aria-label="Wardrobe slot"
            >
              <option value="" [selected]="gesture().slot === ''">
                {{ slotOptional() ? 'every slot it covers' : 'choose a slot' }}
              </option>
              @for (option of slotOptions(); track option.slot) {
                <option [value]="option.slot" [selected]="option.slot === gesture().slot">{{
                  option.label
                }}</option>
              }
            </select>
          </label>
        }
      </div>

      @for (advisory of slotAdvisories(); track $index) {
        <p class="text-xs qt-text-secondary" data-testid="wardrobe-slot-advisory">{{ advisory }}</p>
      }

      @if (wantsItems()) {
        <div class="space-y-1">
          @for (row of rows(); track $index; let index = $index) {
            @if (row.item; as item) {
              <div class="space-y-1">
                <div class="flex items-center gap-2">
                  <button
                    type="button"
                    class="flex flex-1 min-w-48 items-start gap-2 rounded border qt-border-default px-2 py-1 text-left hover:opacity-80"
                    (click)="choose.emit({ index, highlightId: item.id })"
                    [disabled]="disabled()"
                    [attr.aria-label]="row.chipLabel"
                    data-testid="wardrobe-reference-chip"
                  >
                    <qt-wardrobe-item-thumbnail [fileId]="item.imageFileId" [size]="28" />
                    <span class="min-w-0 flex-1 space-y-0.5">
                      <span class="block break-words text-sm text-foreground"
                        >{{ item.title }}@if (row.composite) {<span class="qt-text-secondary"> · outfit</span>}</span
                      >
                      <span class="flex flex-wrap items-center gap-1">
                        @for (slot of item.types; track slot) {
                          <qt-slot-badge [slot]="slot" />
                        }
                        <span class="qt-badge qt-badge-wardrobe-shared">{{ row.originLabel }}</span>
                        @if (row.reach) {
                          <span class="qt-text-xs qt-text-secondary">{{ row.reach }}</span>
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
                    (click)="setItem(index, row.byId ? item.title : item.id)"
                    [disabled]="disabled()"
                    [title]="row.toggleTitle"
                    [attr.aria-label]="row.byId ? 'Reference by title instead' : 'Reference by id instead'"
                  >
                    {{ row.byId ? 'by id' : 'by title' }}
                  </button>
                  <ng-container *ngTemplateOutlet="trash; context: { $implicit: index }" />
                </div>
                @if (row.archivedWarning) {
                  <p class="text-xs qt-text-secondary">{{ row.archivedWarning }}</p>
                }
              </div>
            } @else {
              <div class="space-y-1">
                <div class="flex items-center gap-2 flex-wrap">
                  <input
                    type="text"
                    [value]="row.ref"
                    [attr.maxlength]="MAX_ITEM_LENGTH"
                    (input)="setItem(index, inputValue($event))"
                    [placeholder]="index === 0 ? 'Cloak of Night' : ITEM_PARAM_PLACEHOLDER"
                    [disabled]="disabled()"
                    class="qt-input flex-1 min-w-48"
                    [class.qt-input-error]="hasErrors()"
                    [attr.aria-label]="'Item ' + (index + 1)"
                  />
                  @if (row.unresolvedNote) {
                    <span class="qt-badge qt-badge-secondary" data-testid="wardrobe-reference-unresolved">{{
                      row.unresolvedNote
                    }}</span>
                  }
                  <button
                    type="button"
                    class="qt-button qt-button-secondary qt-button-sm"
                    (click)="choose.emit({ index, highlightId: null })"
                    [disabled]="disabled()"
                    title="Browse the wardrobe and pick a garment or outfit"
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
              [attr.title]="items().length >= MAX_ITEMS ? atMostItemsTitle : null"
            >
              <qt-icon name="plus" class="w-3.5 h-3.5" />
              Add item
            </button>
            <span class="qt-hint">
              a title or id — <code class="font-mono">{{ ITEM_HINT_CODE }}</code> lets a parameter choose
            </span>
          </div>
        </div>
      }
    </div>

    <ng-template #trash let-index>
      <button
        type="button"
        class="qt-button qt-button-ghost qt-button-sm"
        (click)="removeItem(index)"
        [disabled]="disabled() || items().length <= 1"
        title="Remove this item"
      >
        <qt-icon name="trash" class="w-4 h-4" />
      </button>
    </ng-template>
  `,
})
export class WardrobeFields {
  readonly gesture = input.required<DraftWardrobeGesture>();
  readonly hasErrors = input(false);
  readonly disabled = input(false);
  /** The vantage's pool, nearest tier first, archived included. */
  readonly pool = input<readonly WardrobeItemDto[]>([]);
  /** False until the pool's reads settle — no advisory before then. */
  readonly poolFetched = input(false);
  readonly vantage = input<WardrobeVantage>({ kind: 'none' });
  readonly vantageName = input<string | null>(null);
  readonly characterName = input<string | null>(null);

  readonly gestureChange = output<DraftWardrobeGesture>();
  /** Open the picker for item row `index`, highlighting the item it names. */
  readonly choose = output<{ index: number; highlightId: string | null }>();

  protected readonly ACTIONS = WARDROBE_ACTIONS;
  protected readonly MODES = WARDROBE_MODES;
  protected readonly MAX_ITEMS = MAX_WARDROBE_EFFECT_ITEMS;
  protected readonly MAX_ITEM_LENGTH = MAX_WARDROBE_ITEM_REF_LENGTH;
  protected readonly ITEM_PARAM_PLACEHOLDER = ITEM_PARAM_PLACEHOLDER;
  protected readonly ITEM_HINT_CODE = ITEM_HINT_CODE;
  protected readonly atMostItemsTitle = `At most ${MAX_WARDROBE_EFFECT_ITEMS} items`;

  protected readonly wantsItems = computed(() => this.gesture().action !== 'clear_slot');
  protected readonly wantsSlot = computed(() => {
    const g = this.gesture();
    return g.action === 'clear_slot' || g.action === 'take_off' || (g.action === 'wear' && g.mode === 'add_to_slot');
  });
  protected readonly slotOptional = computed(() => this.gesture().action === 'take_off');
  /** v4's `items.length > 0 ? items : ['']` — the form always shows one row. */
  protected readonly items = computed(() => {
    const items = this.gesture().items;
    return items.length > 0 ? items : [''];
  });

  /** Each row, resolved by the one rule the applier uses (none → nothing resolves). */
  protected readonly rows = computed<ItemRow[]>(() => {
    const vantage = this.vantage();
    const pool = this.pool();
    const characterName = this.characterName();
    const vantageName = this.vantageName();
    const action = this.gesture().action;
    const poolFetched = this.poolFetched();
    const unreachableNote =
      vantage.kind === 'character'
        ? `not in ${vantageName ?? 'this character'}\u2019s reach`
        : `not in ${vantageName ?? 'this wardrobe'}`;
    return this.items().map((ref, index) => {
      const item = vantage.kind === 'none' ? undefined : resolveWearableRef(pool, ref);
      const templated = isTemplatedWearableRef(ref);
      if (!item) {
        const unresolved = !templated && vantage.kind !== 'none' && poolFetched && ref.trim() !== '';
        return {
          ref,
          item: null,
          templated,
          byId: false,
          composite: false,
          originLabel: '',
          reach: null,
          chipLabel: '',
          toggleTitle: '',
          archivedWarning: null,
          unresolvedNote: unresolved ? unreachableNote : null,
        };
      }
      const byId = ref.trim() === item.id;
      return {
        ref,
        item,
        templated,
        byId,
        composite: isComposite(item),
        originLabel:
          wardrobeOriginLabel(item.origin) ??
          `${characterName ?? 'Their'}${characterName ? '\u2019s' : ''} own`,
        reach: wardrobeReachLabel(item.origin, characterName),
        chipLabel: `Item ${index + 1}: ${item.title} — choose another`,
        toggleTitle: `${byId ? 'Referenced by id' : 'Referenced by title'} — click to switch. ${REFERENCE_FORM_HINT}`,
        archivedWarning:
          item.archivedAt && action === 'wear' ? `⚠ ${archivedWearMessage(item.title)}` : null,
        unresolvedNote: null,
      };
    });
  });

  /**
   * Slot coupling (v4 §3.2): only when every concrete (non-blank, non-templated)
   * row resolved — a templated row may yet cover the slot.
   */
  private readonly concrete = computed(() =>
    this.rows().filter((row) => row.ref.trim() !== '' && !isTemplatedWearableRef(row.ref)),
  );
  private readonly coupled = computed(() => {
    const concrete = this.concrete();
    return (
      this.wantsItems() &&
      slotIsCoupled(this.gesture()) &&
      concrete.length > 0 &&
      concrete.every((row) => row.item !== null)
    );
  });

  protected readonly slotOptions = computed(() => {
    const coupled = this.coupled();
    const covered = new Set<string>(
      coupled ? this.concrete().flatMap((row) => (row.item ? row.item.types : [])) : [],
    );
    return WARDROBE_SLOT_TYPES.map((slot) => ({
      slot,
      label: WARDROBE_SLOT_META[slot].label + (coupled && !covered.has(slot) ? ' (not covered)' : ''),
    }));
  });

  protected readonly slotAdvisories = computed(() => {
    const slot = this.gesture().slot;
    if (!this.coupled() || !slot) return [];
    return this.concrete()
      .filter((row) => row.item && !row.item.types.includes(slot))
      .map(
        (row) =>
          `⚠ \u201c${row.item!.title}\u201d covers ${formatSlotLabels(row.item!.types)} — not ${WARDROBE_SLOT_META[slot].label}`,
      );
  });

  set(partial: Partial<DraftWardrobeGesture>): void {
    const current = this.gesture();
    this.gestureChange.emit({ ...current, ...partial });
  }

  setItem(index: number, value: string): void {
    const items = this.items();
    this.set({ items: items.map((item, i) => (i === index ? value : item)) });
  }

  /** Never below one blank row. */
  removeItem(index: number): void {
    const next = this.items().filter((_, i) => i !== index);
    this.set({ items: next.length > 0 ? next : [''] });
  }

  addItem(): void {
    this.set({ items: [...this.items(), ''] });
  }

  protected asAction(value: string): WardrobeEffectAction {
    return value as WardrobeEffectAction;
  }

  protected asMode(value: string): WardrobeEffectMode {
    return value as WardrobeEffectMode;
  }

  protected asSlot(value: string): WardrobeItemType | '' {
    return value as WardrobeItemType | '';
  }

  protected inputValue(event: Event): string {
    return (event.target as HTMLInputElement).value;
  }

  protected selectValue(event: Event): string {
    return (event.target as HTMLSelectElement).value;
  }
}
