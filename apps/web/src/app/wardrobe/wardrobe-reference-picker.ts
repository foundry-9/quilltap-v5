import { NgTemplateOutlet } from '@angular/common';
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
import type { WardrobeItemDto, WardrobeSlotType } from '../core/core-contract';
import type { WardrobeVantage } from '../pascal/tool-draft';
import { Icon } from '../ui/icon';
import { Modal } from '../ui/modal';
import { QtAutofocus } from './autofocus.directive';
import { selectComposedOutfits, selectGarments } from './composed-outfits';
import { isComposite } from './equipped-slots';
import { WARDROBE_SLOT_META, WARDROBE_SLOT_TYPES } from './slot-meta';
import { SlotBadge } from './slot-ui';
import { wardrobeOriginLabel, wardrobeReachLabel } from './wardrobe-container';
import { WardrobeDialogService } from './wardrobe-dialog.service';
import { WardrobeItemThumbnail } from './wardrobe-item-thumbnail';
import { WardrobeVantageItemsLoader } from './wardrobe-vantage-items';
import { formatWearLine, wearOf } from './wear-display';

/** How a picked item is written down: by its title (default) or its exact id. */
export type WardrobeReferenceForm = 'title' | 'id';

/** The one line that explains what choosing an id over a title means. */
export const REFERENCE_FORM_HINT =
  'An id names this exact garment and no other; a title names whichever garment of that name is nearest when the tool runs.';

/** One row as the template draws it — memoised per item so bindings stay stable. */
interface PickerRow {
  item: WardrobeItemDto;
  composite: boolean;
  originLabel: string;
  /** `Reach: <reach> · <wear line>`, or the wear line alone. */
  line: string;
}

/**
 * WardrobeReferencePicker — choose a garment or outfit to NAME, not to wear.
 * A port of v4 `components/wardrobe/wardrobe-reference-picker.tsx` (NEW at
 * `5cb0e0190`), whose header rides here:
 *
 * > A reading surface for any author who must write down a wardrobe reference
 * > (Pascal's Workbench today): browse what a vantage can reach — a
 * > character's pool, or one shared wardrobe — see where each item lives, who
 * > can reach it from there, which slots it fills and its picture, and click
 * > one to pick it. The picker knows nothing about drafts or files; it hands
 * > back the item and whether the author wants it referenced by title or by id.
 *
 * Built from the Salon's wardrobe primitives (`qt-wardrobe-item-thumbnail`,
 * `qt-slot-badge`, `wardrobeOriginLabel`, `selectComposedOutfits` /
 * `selectGarments`, `formatWearLine`) and deliberately NOT `WardrobeItemRow`,
 * whose kebab of mutations and wear buttons have no place here.
 *
 * The body renders only while `open` (v4 mounts it only while open), and every
 * filter (`search`, `showArchived`, `coversSlotOnly`, the expanded set) is a
 * `linkedSignal` over `open`, so each opening starts fresh — v4's "filters reset
 * per open". The header control (the vantage select) is projected through
 * `[qtPickerHeader]` and rendered FIRST.
 *
 * **Recorded mechanism divergences.** v4's `BaseModal` closes on Escape; `qt-modal`
 * does not, so a document Escape listener closes the PICKER (and only the
 * picker — the Workbench under it stays put). v4 renders `Open the Wardrobe`
 * only inside the wardrobe-dialog provider; v5's `WardrobeDialogService` is
 * root-provided, so the button always renders — the same output as v4's app
 * (P4.D320 R-8). The pool is read by {@link WardrobeVantageItemsLoader} (R-2).
 */
@Component({
  selector: 'qt-wardrobe-reference-picker',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, Modal, NgTemplateOutlet, QtAutofocus, SlotBadge, WardrobeItemThumbnail],
  host: {
    '(document:keydown.escape)': 'onEscape()',
  },
  template: `
    @if (open()) {
      <qt-modal title="Choose a garment or outfit" maxWidth="2xl" (close)="closed.emit()">
        <div class="space-y-3">
          <div><ng-content select="[qtPickerHeader]"></ng-content></div>

          @if (vantage().kind === 'none') {
            <!-- Nothing but the prompt in the header. -->
          } @else if (emptyVantage()) {
            <div class="space-y-2 text-sm">
              <p class="qt-text-secondary">Nothing hangs here yet. Items are added from the Wardrobe.</p>
              <button
                type="button"
                class="qt-button qt-button-secondary qt-button-sm"
                (click)="openWardrobe()"
              >
                Open the Wardrobe
              </button>
            </div>
          } @else {
            <div class="flex items-center gap-3 flex-wrap text-sm">
              <input
                qtAutofocus
                type="search"
                [value]="search()"
                (input)="search.set(inputValue($event))"
                placeholder="Search by title…"
                class="qt-input qt-input-sm flex-1 min-w-48"
                aria-label="Search the wardrobe"
              />
              <label class="flex items-center gap-1">
                <input
                  type="checkbox"
                  class="qt-checkbox"
                  [checked]="showArchived()"
                  (change)="showArchived.set(checkedValue($event))"
                />
                Show archived
              </label>
              @if (slot(); as s) {
                <label class="flex items-center gap-1">
                  <input
                    type="checkbox"
                    class="qt-checkbox"
                    [checked]="coversSlotOnly()"
                    (change)="coversSlotOnly.set(checkedValue($event))"
                  />
                  Covers {{ slotLabel(s) }} only
                </label>
              }
            </div>

            @if (loader.loading() && !loader.fetched()) {
              <p class="text-sm qt-text-secondary">Opening the wardrobe…</p>
            }

            @if (loader.fetched() && outfitRows().length === 0 && garmentGroups().length === 0) {
              <p class="text-sm qt-text-secondary">Nothing here answers to that.</p>
            }

            @if (outfitRows().length > 0) {
              <section aria-label="Outfits" class="space-y-1">
                <h3 class="qt-card-title text-sm">Outfits</h3>
                <ul class="divide-y qt-border-default">
                  @for (row of outfitRows(); track row.item.id) {
                    <ng-container *ngTemplateOutlet="rowTpl; context: { $implicit: row }" />
                  }
                </ul>
              </section>
            }

            @if (garmentGroups().length > 0) {
              <section aria-label="Garments" class="space-y-2">
                <h3 class="qt-card-title text-sm">Garments</h3>
                @for (group of garmentGroups(); track group.slot) {
                  <div class="space-y-1">
                    <h4 class="qt-text-xs qt-text-secondary uppercase tracking-wide">
                      {{ group.groupLabel }}
                    </h4>
                    <ul class="divide-y qt-border-default">
                      @for (row of group.rows; track row.item.id) {
                        <ng-container *ngTemplateOutlet="rowTpl; context: { $implicit: row }" />
                      }
                    </ul>
                  </div>
                }
              </section>
            }
          }
        </div>

        <div qt-modal-footer class="flex items-center justify-between gap-3 flex-wrap w-full">
          <div class="space-y-1">
            <div class="flex items-center gap-3 text-sm" role="radiogroup" aria-label="Reference by">
              <span>Reference by:</span>
              <label class="flex items-center gap-1">
                <input
                  type="radio"
                  name="wardrobe-reference-form"
                  [checked]="reference() === 'title'"
                  (change)="referenceChange.emit('title')"
                />
                title
              </label>
              <label class="flex items-center gap-1">
                <input
                  type="radio"
                  name="wardrobe-reference-form"
                  [checked]="reference() === 'id'"
                  (change)="referenceChange.emit('id')"
                />
                id
              </label>
            </div>
            @if (reference() === 'id') {
              <p class="qt-hint">{{ hint }}</p>
            }
          </div>
          <button type="button" class="qt-button qt-button-secondary" (click)="closed.emit()">
            Cancel
          </button>
        </div>
      </qt-modal>
    }

    <ng-template #rowTpl let-row>
      <li [class]="isHighlighted(row.item) ? 'qt-bg-muted rounded' : ''">
        <div class="flex items-start gap-2 px-2 py-2">
          <button
            type="button"
            (click)="pick(row.item)"
            class="flex flex-1 min-w-0 items-start gap-3 text-left rounded hover:opacity-80"
            [attr.aria-label]="'Pick ' + row.item.title"
            [attr.aria-current]="isHighlighted(row.item) ? 'true' : null"
            data-testid="wardrobe-reference-option"
          >
            <qt-wardrobe-item-thumbnail [fileId]="row.item.imageFileId" [size]="40" />
            <span class="min-w-0 flex-1 space-y-1">
              <span class="block break-words text-sm text-foreground">
                @if (row.item.isDefault) {
                  <span title="Default outfit" aria-label="Default outfit">★ </span>
                }{{ row.item.title }}@if (row.composite) {<span class="qt-text-secondary"> · outfit</span>}
              </span>
              <span class="flex flex-wrap items-center gap-1">
                @for (t of row.item.types; track t) {
                  <qt-slot-badge [slot]="t" />
                }
                <span class="qt-badge qt-badge-wardrobe-shared">{{ row.originLabel }}</span>
                @if (row.item.archivedAt) {
                  <span class="qt-badge qt-badge-secondary">archived</span>
                }
              </span>
              <span class="block qt-text-xs qt-text-secondary">{{ row.line }}</span>
            </span>
          </button>
          @if (row.composite) {
            <button
              type="button"
              class="qt-button qt-button-ghost qt-button-sm"
              (click)="toggleExpanded(row.item.id)"
              [attr.aria-expanded]="isExpanded(row.item.id)"
              [attr.aria-label]="
                (isExpanded(row.item.id) ? 'Hide what ' : 'Show what ') + row.item.title + ' is made of'
              "
            >
              <qt-icon
                [name]="isExpanded(row.item.id) ? 'chevron-down' : 'chevron-right'"
                class="w-4 h-4"
              />
            </button>
          }
        </div>
        @if (row.composite && isExpanded(row.item.id)) {
          <ul
            class="pl-14 pb-2 space-y-0.5 qt-text-xs qt-text-secondary"
            [attr.aria-label]="row.item.title + ' components'"
          >
            @for (componentId of row.item.componentItemIds ?? []; track componentId) {
              <li>
                @if (componentLine(componentId); as text) {
                  {{ text }}
                } @else {
                  <em>a piece no longer in reach</em>
                }
              </li>
            }
          </ul>
        }
      </li>
    </ng-template>
  `,
})
export class WardrobeReferencePicker {
  private readonly core = inject(CoreClient);
  private readonly wardrobeDialog = inject(WardrobeDialogService);

  readonly open = input(false);
  /** Whose wardrobe to browse. */
  readonly vantage = input.required<WardrobeVantage>();
  /** The character's display name, for "Imogen only" and "Imogen's own". */
  readonly characterName = input<string | null>(null);
  /**
   * The gesture the pick is for. `take_off` shows archived items by default
   * (archived items may come off); anything else hides them.
   */
  readonly action = input<'wear' | 'take_off' | undefined>(undefined);
  /** The slot the gesture names, if any — offers a "Covers <slot> only" filter. */
  readonly slot = input<WardrobeSlotType | null>(null);
  /** The id of the item the reference currently names, highlighted. */
  readonly highlightId = input<string | null>(null);
  /** The title/id switch, controlled so it lasts the editor session. */
  readonly reference = input<WardrobeReferenceForm>('title');

  readonly closed = output<void>();
  readonly referenceChange = output<WardrobeReferenceForm>();
  readonly picked = output<{ item: WardrobeItemDto; reference: WardrobeReferenceForm }>();

  protected readonly hint = REFERENCE_FORM_HINT;
  protected readonly loader = new WardrobeVantageItemsLoader(this.core);

  // Every filter resets when the picker (re)opens — v4 remounts its body.
  protected readonly search = linkedSignal({ source: this.open, computation: () => '' });
  protected readonly showArchived = linkedSignal({
    source: this.open,
    computation: () => untracked(this.action) === 'take_off',
  });
  protected readonly coversSlotOnly = linkedSignal({ source: this.open, computation: () => true });
  protected readonly expanded = linkedSignal<boolean, ReadonlySet<string>>({
    source: this.open,
    computation: () => new Set<string>(),
  });

  constructor() {
    // Read the vantage's wardrobe while open (and again when the vantage moves).
    effect(() => {
      const vantage = this.open() ? this.vantage() : null;
      untracked(() => void this.loader.load(vantage));
    });
  }

  protected readonly emptyVantage = computed(
    () => this.loader.fetched() && this.loader.items().length === 0,
  );

  private readonly visible = computed(() => {
    const term = this.search().trim().toLowerCase();
    const showArchived = this.showArchived();
    const slot = this.slot();
    const coversSlotOnly = this.coversSlotOnly();
    return this.loader.items().filter((item) => {
      if (!showArchived && item.archivedAt) return false;
      if (slot && coversSlotOnly && !item.types.includes(slot)) return false;
      if (term && !item.title.toLowerCase().includes(term)) return false;
      return true;
    });
  });

  private readonly rowsById = computed(() => {
    const characterName = this.characterName();
    const rows = new Map<string, PickerRow>();
    for (const item of this.loader.items()) {
      const reach = wardrobeReachLabel(item.origin, characterName);
      rows.set(item.id, {
        item,
        composite: isComposite(item),
        originLabel:
          wardrobeOriginLabel(item.origin) ??
          (characterName ? `${characterName}'s own` : 'their own'),
        line: `${reach ? `Reach: ${reach}` : ''}${reach ? ' · ' : ''}${formatWearLine(wearOf(item))}`,
      });
    }
    return rows;
  });

  private row(item: WardrobeItemDto, composite: boolean): PickerRow {
    const base = this.rowsById().get(item.id)!;
    return composite === base.composite ? base : { ...base, composite };
  }

  protected readonly outfitRows = computed(() =>
    selectComposedOutfits(this.visible()).map((item) => this.row(item, true)),
  );

  protected readonly garmentGroups = computed(() => {
    const garments = selectGarments(this.visible()).sort((a, b) => a.title.localeCompare(b.title));
    return WARDROBE_SLOT_TYPES.map((groupSlot) => ({
      slot: groupSlot,
      groupLabel: WARDROBE_SLOT_META[groupSlot].groupLabel,
      // A multi-slot garment hangs under its first slot only, every badge shown.
      rows: garments
        .filter((item) => WARDROBE_SLOT_TYPES.find((s) => item.types.includes(s)) === groupSlot)
        .map((item) => this.row(item, isComposite(item))),
    })).filter((group) => group.rows.length > 0);
  });

  private readonly byId = computed(
    () => new Map(this.loader.resolutionItems().map((item) => [item.id, item] as const)),
  );

  protected componentLine(componentId: string): string | null {
    const component = this.byId().get(componentId);
    if (!component) return null;
    return component.types.length > 0
      ? `${component.title} — ${component.types.map((t) => WARDROBE_SLOT_META[t].label).join(', ')}`
      : component.title;
  }

  protected slotLabel(slot: WardrobeSlotType): string {
    return WARDROBE_SLOT_META[slot].label;
  }

  protected isHighlighted(item: WardrobeItemDto): boolean {
    return this.highlightId() === item.id;
  }

  protected isExpanded(id: string): boolean {
    return this.expanded().has(id);
  }

  protected toggleExpanded(id: string): void {
    const next = new Set(this.expanded());
    if (next.has(id)) next.delete(id);
    else next.add(id);
    this.expanded.set(next);
  }

  protected pick(item: WardrobeItemDto): void {
    this.picked.emit({ item, reference: this.reference() });
    this.closed.emit();
  }

  protected openWardrobe(): void {
    const vantage = this.vantage();
    this.wardrobeDialog.open(
      vantage.kind === 'character' ? { characterId: vantage.characterId } : undefined,
    );
    this.closed.emit();
  }

  protected onEscape(): void {
    if (this.open()) this.closed.emit();
  }

  protected inputValue(event: Event): string {
    return (event.target as HTMLInputElement).value;
  }

  protected checkedValue(event: Event): boolean {
    return (event.target as HTMLInputElement).checked;
  }
}
