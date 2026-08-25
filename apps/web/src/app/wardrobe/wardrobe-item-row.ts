import { ChangeDetectionStrategy, Component, computed, input, output, signal } from '@angular/core';

import type { WardrobeItemDto, WardrobeSlotType } from '../core/core-contract';
import { WARDROBE_SLOT_META } from './slot-meta';

/**
 * One line in the dialog's wardrobe list (v4
 * `components/wardrobe/wardrobe-item-row.tsx`). Shows the item title (allowed
 * to wrap to two lines, hover tooltip for the full title), slot-color chips,
 * and three controls:
 *
 *  - Primary equip button (`Wear` / `Try on`, label depends on which right-
 *    column tab is active).
 *  - `[+]` icon that adds the item to a slot. Single-slot items target their
 *    only slot directly; multi-slot items open a small popover picker.
 *  - `⋮` kebab menu with secondary actions: Edit, toggle the default-outfit
 *    flag, Duplicate, Move, Copy, and Delete. Which of these appear is
 *    governed by the `canManage` predicate: items living in the container
 *    being browsed get the full set, items merged in from another shared
 *    tier keep only Move and Copy.
 *
 * Composite items keep a `▶/▼` expander so the user can peek at the
 * components without entering the editor (nested rows are read-only browse:
 * `inChat=false`, v4 `:397`).
 */
@Component({
  selector: 'qt-wardrobe-item-row',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { '(document:keydown.escape)': 'onEscape($event)' },
  template: `
    <div
      class="qt-card-interactive py-2 px-3"
      [style.margin-left]="depth() > 0 ? depth() * 12 + 'px' : null"
    >
      <div class="flex items-start gap-2">
        @if (isComposite()) {
          <button
            type="button"
            class="qt-text-secondary hover:text-foreground mt-0.5"
            [attr.aria-label]="expanded() ? 'Collapse components' : 'Expand components'"
            (click)="expanded.set(!expanded())"
          >
            {{ expanded() ? '▼' : '▶' }}
          </button>
        } @else {
          <span class="inline-block w-3" aria-hidden="true"></span>
        }

        <div class="flex-1 min-w-0">
          <div class="flex items-center gap-2 flex-wrap">
            <span
              class="qt-text-sm text-foreground"
              style="display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; word-break: break-word; max-width: 100%; min-width: 0"
              [title]="item().title"
            >
              {{ item().title }}
            </span>
            @if (isComposite()) {
              <span class="qt-text-xs qt-text-secondary">· bundle</span>
            }
            @if (!manageable()) {
              <span class="qt-text-xs qt-text-secondary">· shared</span>
            }
            @if (item().isDefault) {
              <span class="qt-text-xs qt-text-secondary">· default</span>
            }
            @for (t of item().types; track t) {
              <span class="qt-badge" [class]="slotMeta[t].badgeClass">{{ t }}</span>
            }
          </div>
          @if (item().appropriateness) {
            <div class="qt-text-xs qt-text-secondary truncate mt-0.5">
              {{ item().appropriateness }}
            </div>
          }
        </div>

        <div class="flex items-center gap-1 flex-shrink-0">
          <!-- Primary equip button — label depends on the active right-column
               tab (Wear in Live outfit, Try on in Outfit Builder). -->
          @if (inChat()) {
            <button
              type="button"
              (click)="equip.emit(item())"
              class="qt-button-ghost qt-button-sm"
              title="Puts it on across every slot it covers — layers or replaces per the item's setting"
            >
              {{ equipLabel() }}
            </button>

            <!-- Single-icon add button: single-typed adds directly, multi-typed
                 opens the slot picker (v4 :143-150). -->
            <div class="relative">
              <button
                type="button"
                (click)="handleAddClick()"
                class="qt-button-ghost qt-button-sm"
                [title]="addTooltip()"
                [attr.aria-label]="addTooltip()"
              >
                +
              </button>
              @if (slotPickerOpen() && item().types.length > 1) {
                <div
                  class="absolute right-0 top-full mt-1 z-30 min-w-[10rem] rounded border qt-border-default qt-bg-default shadow-md"
                >
                  <ul class="divide-y qt-border-default">
                    @for (slot of item().types; track slot) {
                      <li>
                        <button
                          type="button"
                          (click)="pickSlot(slot)"
                          class="flex w-full items-center justify-between gap-2 px-3 py-2 text-left text-sm hover:qt-bg-muted"
                        >
                          <span>{{ slotMeta[slot].label }}</span>
                          <span class="qt-badge" [class]="slotMeta[slot].badgeClass">{{
                            slot
                          }}</span>
                        </button>
                      </li>
                    }
                  </ul>
                </div>
              }
            </div>
          }

          <!-- Kebab menu — item actions (v4 :271-380). -->
          <div class="relative">
            <button
              type="button"
              (click)="kebabOpen.set(!kebabOpen())"
              class="qt-button-ghost qt-button-sm"
              aria-label="More actions"
              title="More actions"
              aria-haspopup="menu"
              [attr.aria-expanded]="kebabOpen()"
            >
              ⋮
            </button>
            @if (kebabOpen()) {
              <div
                role="menu"
                class="absolute right-0 top-full mt-1 z-30 min-w-[14rem] rounded border qt-border-default qt-bg-default shadow-md"
              >
                <ul class="divide-y qt-border-default">
                  @if (manageable()) {
                    <li>
                      <button
                        type="button"
                        role="menuitem"
                        (click)="menuAction(edit)"
                        class="block w-full text-left px-3 py-2 text-sm hover:qt-bg-muted"
                      >
                        Edit
                      </button>
                    </li>
                    <li>
                      <button
                        type="button"
                        role="menuitem"
                        [disabled]="isUpdatingDefault()"
                        (click)="menuAction(toggleDefault)"
                        class="block w-full text-left px-3 py-2 text-sm hover:qt-bg-muted disabled:opacity-50"
                      >
                        {{
                          item().isDefault
                            ? '☆ Unmark as default'
                            : '★ Mark as default outfit item'
                        }}
                      </button>
                    </li>
                    <li>
                      <button
                        type="button"
                        role="menuitem"
                        (click)="menuAction(duplicate)"
                        class="block w-full text-left px-3 py-2 text-sm hover:qt-bg-muted"
                      >
                        Duplicate
                      </button>
                    </li>
                  }
                  <li>
                    <button
                      type="button"
                      role="menuitem"
                      (click)="menuAction(move)"
                      class="block w-full text-left px-3 py-2 text-sm hover:qt-bg-muted"
                    >
                      Move
                    </button>
                  </li>
                  <li>
                    <button
                      type="button"
                      role="menuitem"
                      (click)="menuAction(copyItem)"
                      class="block w-full text-left px-3 py-2 text-sm hover:qt-bg-muted"
                    >
                      Copy
                    </button>
                  </li>
                  @if (manageable()) {
                    <li>
                      <button
                        type="button"
                        role="menuitem"
                        (click)="menuAction(delete)"
                        class="block w-full text-left px-3 py-2 text-sm qt-text-destructive hover:qt-bg-muted"
                      >
                        Delete
                      </button>
                    </li>
                  }
                </ul>
              </div>
            }
          </div>
        </div>
      </div>

      <!-- Nested components (read-only here; click Edit to change). -->
      @if (isComposite() && expanded()) {
        <div class="mt-2 border-l-2 qt-border-default pl-2 space-y-1">
          @if (components().length === 0) {
            <div class="qt-text-xs qt-text-secondary px-2 py-1">
              Components missing from this wardrobe.
            </div>
          } @else {
            @for (c of components(); track c.id) {
              <qt-wardrobe-item-row
                [item]="c"
                [allItems]="allItems()"
                [inChat]="false"
                [canManage]="canManage()"
                [depth]="depth() + 1"
                (toggleDefault)="toggleDefault.emit($event)"
                (edit)="edit.emit($event)"
                (duplicate)="duplicate.emit($event)"
                (move)="move.emit($event)"
                (copyItem)="copyItem.emit($event)"
                (delete)="delete.emit($event)"
              />
            }
          }
        </div>
      }
    </div>
  `,
})
export class WardrobeItemRow {
  readonly item = input.required<WardrobeItemDto>();
  /** All items in the cache (this character + shared archetypes) — used to
   *  render composite components inline. */
  readonly allItems = input.required<WardrobeItemDto[]>();
  /** When set, equip controls are visible. */
  readonly inChat = input.required<boolean>();
  /**
   * Whether an item can be managed (edited / starred / duplicated / deleted)
   * from the current view — true when the item lives in the container being
   * browsed, false when it was merged in from a shared tier elsewhere. Items
   * failing this check keep only Move and Copy, and are badged `· shared`.
   * Defaults to the character-view rule: manageable iff character-owned
   * (v4 `wardrobe-item-row.tsx:36-40`, `:83-85`).
   */
  readonly canManage = input<((item: WardrobeItemDto) => boolean) | undefined>(undefined);
  /** Label for the equip-replace button (v4 default "Wear"). */
  readonly equipLabel = input('Wear');
  /** Whether `[+]` is framed as "layer onto" (Live) or "add to" (Builder) —
   *  tooltip only (v4 `:33-37`). */
  readonly addAction = input<'layer' | 'add'>('layer');
  readonly isUpdatingDefault = input(false);
  /** Nesting depth for composite components — indentation. */
  readonly depth = input(0);

  readonly toggleDefault = output<WardrobeItemDto>();
  readonly edit = output<WardrobeItemDto>();
  readonly duplicate = output<WardrobeItemDto>();
  readonly move = output<WardrobeItemDto>();
  readonly copyItem = output<WardrobeItemDto>();
  readonly delete = output<WardrobeItemDto>();
  readonly equip = output<WardrobeItemDto>();
  readonly addToSlot = output<{ item: WardrobeItemDto; slot: WardrobeSlotType }>();

  protected readonly slotMeta = WARDROBE_SLOT_META;

  protected readonly expanded = signal(false);
  protected readonly slotPickerOpen = signal(false);
  protected readonly kebabOpen = signal(false);

  protected readonly isComposite = computed(() => (this.item().componentItemIds ?? []).length > 0);
  /**
   * v4 `:83-85` — without an explicit predicate, fall back to the
   * character-view rule: personal items are manageable, shared-tier items are
   * Move/Copy only.
   */
  protected readonly manageable = computed(() => {
    const predicate = this.canManage();
    return predicate ? predicate(this.item()) : Boolean(this.item().characterId);
  });

  protected readonly components = computed(() => {
    if (!this.isComposite()) return [];
    const byId = new Map(this.allItems().map((i) => [i.id, i]));
    return (this.item().componentItemIds ?? [])
      .map((id) => byId.get(id))
      .filter((c): c is WardrobeItemDto => Boolean(c));
  });

  /** v4 `:152-157`. */
  protected readonly addTooltip = computed(() => {
    const types = this.item().types;
    if (types.length === 1) {
      return `${this.addAction() === 'layer' ? 'Layer onto' : 'Add to'} ${WARDROBE_SLOT_META[types[0]].label.toLowerCase()}`;
    }
    return this.addAction() === 'layer' ? 'Layer onto a slot' : 'Add to a slot';
  });

  /** v4 `:143-150`. */
  protected handleAddClick(): void {
    const types = this.item().types;
    if (types.length === 1) {
      this.addToSlot.emit({ item: this.item(), slot: types[0] });
      return;
    }
    this.slotPickerOpen.set(!this.slotPickerOpen());
  }

  protected pickSlot(slot: WardrobeSlotType): void {
    this.addToSlot.emit({ item: this.item(), slot });
    this.slotPickerOpen.set(false);
  }

  // The kebab entries close the menu then emit (v4 wraps each onClick).
  protected menuAction(out: { emit(value: WardrobeItemDto): void }): void {
    this.kebabOpen.set(false);
    out.emit(this.item());
  }

  /** v4 `:114-133` — Escape closes the popovers without dismissing the modal. */
  protected onEscape(event: Event): void {
    if (!this.slotPickerOpen() && !this.kebabOpen()) return;
    event.stopPropagation();
    event.preventDefault();
    this.slotPickerOpen.set(false);
    this.kebabOpen.set(false);
  }
}
