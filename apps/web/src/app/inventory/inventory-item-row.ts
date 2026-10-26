import { DOCUMENT } from '@angular/common';
import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  computed,
  effect,
  inject,
  input,
  signal,
  viewChild,
} from '@angular/core';

import type { KitState, ResolvedInventoryItem } from '../core/core-contract';
import { inventoryOriginLabel } from './inventory-container';
import { InventoryGlyph } from './inventory-glyph';

/** v4 `InventoryRowActions` — what a row's ⋮ menu reaches. */
export interface InventoryRowActions {
  onEdit: (item: ResolvedInventoryItem) => void;
  onInstantiate: (item: ResolvedInventoryItem) => void;
  onDetach: (item: ResolvedInventoryItem) => void;
  onDuplicate: (item: ResolvedInventoryItem) => void;
  onMoveToFolder: (item: ResolvedInventoryItem) => void;
  onTransfer: (item: ResolvedInventoryItem, mode: 'move' | 'copy') => void;
  onToggleArchived: (item: ResolvedInventoryItem) => void;
  onDelete: (item: ResolvedInventoryItem) => void;
}

/** v4 `InventoryRowEquip` — present only in a chat with a character. */
export interface InventoryRowEquip {
  equipped: boolean;
  onEquip: (item: ResolvedInventoryItem) => void;
  onUnequip: (item: ResolvedInventoryItem) => void;
  /** A kit's state in the chat. */
  kitState?: KitState;
}

/** v4 `:56` — every menu entry's class. */
export const MENU_ITEM = 'block w-full text-left px-3 py-2 text-sm hover:qt-bg-muted disabled:opacity-50';
/** Room the ⋮ menu wants below its button before it opens upward instead (v4 `:58`). */
export const MENU_ROOM = 340;
/** v4 `:59`. */
export const MENU_GAP = 4;

/** Where the menu sits, in pixels (the style v4's `menuPlacement` returns). */
export interface MenuPlacement {
  position: 'fixed';
  right: number;
  top?: number;
  bottom?: number;
  maxHeight: number;
  overflowY: 'auto';
}

/**
 * Where the ⋮ menu sits: fixed to the viewport beside its button, so the
 * browser's scrolling list cannot clip it, opening upward when the room below
 * is short and the room above is longer (v4 `:66-78`, `102a23648`). DOM-free:
 * the button's rect and the viewport size come in as numbers (spec-pinned, R-15).
 */
export function menuPlacement(
  rect: { top: number; bottom: number; right: number },
  innerWidth: number,
  innerHeight: number,
): MenuPlacement {
  const below = innerHeight - rect.bottom - MENU_GAP;
  const above = rect.top - MENU_GAP;
  const up = below < MENU_ROOM && above > below;
  return {
    position: 'fixed',
    right: Math.max(MENU_GAP, innerWidth - rect.right),
    ...(up ? { bottom: innerHeight - rect.top + MENU_GAP } : { top: rect.bottom + MENU_GAP }),
    maxHeight: Math.max(120, up ? above : below),
    overflowY: 'auto',
  };
}

/**
 * One object in the Inventory browser — a port of v4
 * `components/inventory/inventory-item-row.tsx` (`322a5488e`, with
 * `102a23648`'s fixed menu).
 *
 * The glyph (never the picture), the title with the name beside it when both
 * differ, and chips in v4's order: *Archetype*, *Instance of X*, *Kit · n*
 * (*Kit · all/some/none* in a chat), *Equipped*, *Default*, the tier an object
 * is borrowed from, *archived*, and a search hit's folder. In a chat an Equip
 * / Unequip button sits on the row (never on an archetype). The ⋮ menu holds
 * Edit, Unequip the kit, Instantiate… (archetypes), Detach from archetype…
 * (instances), Duplicate (only when copyable), Move to folder…, Move / Copy to
 * another inventory…, Archive / Restore and Delete; a borrowed row keeps only
 * the cross-tier Copy (and Instantiate for an archetype).
 *
 * The menu is FIXED to the viewport (inline style bindings from
 * {@link menuPlacement}); any capture-phase scroll outside it, or a resize,
 * closes it — a fixed menu would drift from its button (trap 14). A press
 * outside closes it; Escape closes it on the CAPTURE phase and stops there so
 * the dialog behind keeps its own Escape (v4's `useClickOutside` with
 * `escapeCapture: true`). Every listener exists only while the menu is open.
 */
@Component({
  selector: 'qt-inventory-item-row',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [InventoryGlyph],
  template: `
    <div [attr.class]="rowClass()" data-testid="inventory-item-row">
      @if (selection() && manageable()) {
        <input
          type="checkbox"
          class="qt-checkbox"
          [checked]="selection()!.selected"
          (change)="selection()!.onToggle(item())"
          [attr.aria-label]="'Select ' + label()"
        />
      }
      <qt-inventory-glyph [item]="item()" />
      <div class="flex-1 min-w-0">
        <div class="text-sm text-foreground truncate" [attr.title]="label()">
          {{ label() }}
          @if (showName()) {
            <span class="qt-text-xs qt-text-secondary ml-1">({{ item().name }})</span>
          }
        </div>
        <div class="flex flex-wrap items-center gap-1 mt-0.5">
          @if (item().isArchetype) {
            <span class="qt-badge qt-badge-primary">Archetype</span>
          }
          @if (isInstance()) {
            <span
              class="qt-badge qt-badge-outline"
              [attr.title]="item().parentMissing ? 'Its archetype could not be found' : null"
              >{{ instanceChip() }}</span
            >
          }
          @if (item().kind === 'kit') {
            <span class="qt-badge qt-badge-info">{{ kitChip() }}</span>
          }
          @if (equip()?.equipped) {
            <span class="qt-badge qt-badge-success">Equipped</span>
          }
          @if (item().equippedByDefault && !item().isArchetype) {
            <span class="qt-badge qt-badge-secondary">Default</span>
          }
          @if (borrowed(); as b) {
            <span class="qt-badge qt-badge-wardrobe-shared">{{ b }}</span>
          }
          @if (item().archivedAt) {
            <span class="qt-badge qt-badge-secondary">archived</span>
          }
          @if (folderCrumb(); as crumb) {
            <span class="qt-text-xs qt-text-secondary" data-testid="inventory-folder-crumb">{{ crumb }}</span>
          }
        </div>
      </div>

      @if (equip(); as eq) {
        @if (!item().isArchetype && !item().archivedAt) {
          @if (item().kind === 'kit' || !eq.equipped) {
            <button
              type="button"
              class="qt-button-secondary qt-button-sm"
              [disabled]="!!selection()"
              (click)="eq.onEquip(item())"
            >
              Equip
            </button>
          } @else {
            <button
              type="button"
              class="qt-button-ghost qt-button-sm"
              [disabled]="!!selection()"
              (click)="eq.onUnequip(item())"
            >
              Unequip
            </button>
          }
        }
      }

      <div class="relative" #menuHost>
        <button
          #menuButton
          type="button"
          (click)="toggleMenu()"
          [disabled]="!!selection()"
          class="qt-button-ghost qt-button-sm"
          aria-label="More actions"
          title="More actions"
          aria-haspopup="menu"
          [attr.aria-expanded]="menuOpen()"
        >
          ⋮
        </button>
        @if (placement(); as p) {
          <div
            #menu
            role="menu"
            [style.position]="p.position"
            [style.right.px]="p.right"
            [style.top.px]="p.top ?? null"
            [style.bottom.px]="p.bottom ?? null"
            [style.max-height.px]="p.maxHeight"
            [style.overflow-y]="p.overflowY"
            class="z-30 min-w-[14rem] rounded border qt-border-default qt-bg-default shadow-md"
          >
            <ul class="divide-y qt-border-default">
              @if (manageable()) {
                <li>
                  <button type="button" role="menuitem" [attr.class]="menuItem" (click)="run(actions().onEdit)">Edit</button>
                </li>
              }
              @if (equip(); as eq) {
                @if (item().kind === 'kit' && eq.kitState && eq.kitState !== 'none') {
                  <li>
                    <button type="button" role="menuitem" [attr.class]="menuItem" (click)="run(eq.onUnequip)">
                      Unequip the kit
                    </button>
                  </li>
                }
              }
              @if (item().isArchetype && !item().archivedAt) {
                <li>
                  <button type="button" role="menuitem" [attr.class]="menuItem" (click)="run(actions().onInstantiate)">
                    Instantiate…
                  </button>
                </li>
              }
              @if (manageable() && isInstance()) {
                <li>
                  <button type="button" role="menuitem" [attr.class]="menuItem" (click)="run(actions().onDetach)">
                    Detach from archetype…
                  </button>
                </li>
              }
              @if (manageable() && item().canCopy) {
                <li>
                  <button type="button" role="menuitem" [attr.class]="menuItem" (click)="run(actions().onDuplicate)">
                    Duplicate
                  </button>
                </li>
              }
              @if (manageable()) {
                <li>
                  <button type="button" role="menuitem" [attr.class]="menuItem" (click)="run(actions().onMoveToFolder)">
                    Move to folder…
                  </button>
                </li>
                <li>
                  <button type="button" role="menuitem" [attr.class]="menuItem" (click)="transfer('move')">
                    Move to another inventory…
                  </button>
                </li>
              }
              @if (item().canCopy) {
                <li>
                  <button type="button" role="menuitem" [attr.class]="menuItem" (click)="transfer('copy')">
                    Copy to another inventory…
                  </button>
                </li>
              }
              @if (manageable()) {
                <li>
                  <button type="button" role="menuitem" [attr.class]="menuItem" (click)="run(actions().onToggleArchived)">
                    {{ item().archivedAt ? 'Restore' : 'Archive' }}
                  </button>
                </li>
                <li>
                  <button
                    type="button"
                    role="menuitem"
                    [attr.class]="menuItem + ' qt-text-destructive'"
                    (click)="run(actions().onDelete)"
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
  `,
})
export class InventoryItemRow {
  readonly item = input.required<ResolvedInventoryItem>();
  /** The object lives in the tier being browsed. */
  readonly manageable = input(false);
  readonly actions = input.required<InventoryRowActions>();
  /** Present only when the browser is opened from a chat with a character. */
  readonly equip = input<InventoryRowEquip | undefined>(undefined);
  readonly selection = input<
    { selected: boolean; onToggle: (item: ResolvedInventoryItem) => void } | undefined
  >(undefined);
  /** A search hit's folder. */
  readonly folderCrumb = input<string | null>(null);

  protected readonly menuItem = MENU_ITEM;
  protected readonly placement = signal<MenuPlacement | null>(null);
  protected readonly menuOpen = computed(() => this.placement() !== null);

  private readonly menuHost = viewChild<ElementRef<HTMLElement>>('menuHost');
  private readonly menuButton = viewChild<ElementRef<HTMLButtonElement>>('menuButton');
  private readonly menu = viewChild<ElementRef<HTMLElement>>('menu');

  /** v4 `:103-106`. */
  protected readonly label = computed(() => this.item().title || this.item().name);
  protected readonly showName = computed(() => {
    const item = this.item();
    return !!item.title && item.title.trim().toLowerCase() !== item.name.trim().toLowerCase();
  });
  protected readonly borrowed = computed(() => (this.manageable() ? null : inventoryOriginLabel(this.item().origin)));
  protected readonly isInstance = computed(() => !!this.item().parentId);
  protected readonly rowClass = computed(
    () => `qt-card py-2 px-3 flex items-center gap-2 ${this.item().archivedAt ? 'opacity-60' : ''}`,
  );
  protected readonly instanceChip = computed(() =>
    this.item().parentMissing ? 'Instance (archetype missing)' : `Instance of ${this.item().parentName ?? 'an archetype'}`,
  );
  protected readonly kitChip = computed(() => {
    const state = this.equip()?.kitState;
    return `Kit${state ? ` · ${state}` : ` · ${this.item().itemIds.length}`}`;
  });

  constructor() {
    const doc = inject(DOCUMENT);
    effect((onCleanup) => {
      if (!this.menuOpen()) return;
      const win = doc.defaultView;
      const close = (): void => this.placement.set(null);
      const mousedown = (event: MouseEvent): void => {
        const host = this.menuHost()?.nativeElement;
        if (!(event.target instanceof Node && host?.contains(event.target))) close();
      };
      const keydown = (event: KeyboardEvent): void => {
        if (event.key !== 'Escape') return;
        event.stopPropagation();
        event.preventDefault();
        close();
      };
      // A fixed menu would drift from its button, so any scroll or resize outside it closes it.
      const scroll = (event: Event): void => {
        const menu = this.menu()?.nativeElement;
        if (menu && event.target instanceof Node && menu.contains(event.target)) return;
        close();
      };
      doc.addEventListener('mousedown', mousedown);
      doc.addEventListener('keydown', keydown, true);
      win?.addEventListener('scroll', scroll, true);
      win?.addEventListener('resize', close);
      onCleanup(() => {
        doc.removeEventListener('mousedown', mousedown);
        doc.removeEventListener('keydown', keydown, true);
        win?.removeEventListener('scroll', scroll, true);
        win?.removeEventListener('resize', close);
      });
    });
  }

  protected toggleMenu(): void {
    if (this.menuOpen()) {
      this.placement.set(null);
      return;
    }
    const button = this.menuButton()?.nativeElement;
    const view = button?.ownerDocument.defaultView;
    this.placement.set(
      button && view ? menuPlacement(button.getBoundingClientRect(), view.innerWidth, view.innerHeight) : null,
    );
  }

  /** v4 `run(fn)` — close the menu, then act on the row. */
  protected run(fn: (item: ResolvedInventoryItem) => void): void {
    this.placement.set(null);
    fn(this.item());
  }

  protected transfer(mode: 'move' | 'copy'): void {
    this.placement.set(null);
    this.actions().onTransfer(this.item(), mode);
  }
}
