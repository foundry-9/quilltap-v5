import { DOCUMENT, NgTemplateOutlet } from '@angular/common';
import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  afterRenderEffect,
  computed,
  effect,
  inject,
  input,
  viewChild,
  type TemplateRef,
} from '@angular/core';

import { createFixedMenu } from '../ui/fixed-menu';
import { isInFloatingLayer } from '../ui/floating-layer';

/** One entry of a row's ⋮ menu (v4 `MenuEntry`). */
export interface MenuEntry {
  /** Stable key; also the entry's `data-action`. */
  key: string;
  /** The entry's text — its accessible name (L-8: a `string`, never markup). */
  label: string;
  onSelect: () => void;
  disabled?: boolean;
  /** Rendered in the destructive colour (Delete). */
  destructive?: boolean;
  /**
   * Rich content rendered INSIDE the menuitem in place of the text (the slot
   * picker's badge row, v4 pin `wardrobe-item-row.tsx:306-311`, where v4's
   * `label` is a `ReactNode`). `label` still names the entry.
   */
  template?: TemplateRef<unknown>;
}

/** An entry, or a falsy hole a row writes inline (`manageable && { … }`). */
export type MenuEntryLike = MenuEntry | false | null | undefined;

/** A visible menu button in place of the ⋮ glyph (v4 `MenuTrigger`). */
export interface MenuTrigger {
  /** What the button shows (every v4 caller passes text: `+`, `Reset…`). */
  content: string;
  /** The button's tooltip; defaults to the menu's `label`. */
  title?: string;
  /** The button's class; defaults to the ghost small button. */
  className?: string;
  /**
   * The button's accessible name when `content` does not say it (an icon);
   * omitted, the visible content names the button.
   */
  ariaLabel?: string;
}

/** v4 `ENTRY_CLASS` — every menu entry's class. */
export const ITEM_ACTION_ENTRY_CLASS = 'block w-full text-left px-3 py-2 text-sm hover:qt-bg-muted disabled:opacity-50';

/** The entries a row declared, with the falsy ones dropped (v4 `presentEntries`). */
export function presentEntries(entries: readonly MenuEntryLike[]): MenuEntry[] {
  return entries.filter((e): e is MenuEntry => Boolean(e));
}

/**
 * The ⋮ menu every folder-of-items browser row carries (the wardrobe's, the
 * Inventory's, the Scriptorium's — items and folders alike), stated once. A
 * port of v4 `components/item-browser/ItemActionMenu.tsx` at the pin
 * `624d0d25c` (NEW at `b2b783b47`; `c34986421`'s focus without scrolling;
 * `600c2906f`'s `trigger`).
 *
 * v4's module doc, carried: a row declares its actions as `MenuEntry` values;
 * entries that are `false`, `null` or `undefined` are dropped, so a row can
 * write `manageable && { … }` inline. The menu is fixed to the viewport beside
 * its button ({@link createFixedMenu}, so a scrolling list cannot clip it),
 * and behaves as a WAI-ARIA menu button:
 *
 *   - opening it focuses the first enabled entry (without scrolling — a
 *     scroll the focus provoked in an enclosing pane, a workspace tab, would
 *     close the fixed menu again);
 *   - ↓ / ↑ move between enabled entries (wrapping), Home / End jump to the
 *     ends;
 *   - Escape closes it and returns focus to the ⋮ button — swallowed in the
 *     capture phase, so an enclosing dialog stays open (v4's
 *     `useClickOutside(…, { escapeCapture: true })`);
 *   - Tab, an outside press, a scroll outside the menu or a resize close it.
 *
 * Choosing an entry closes the menu, puts focus back on the button (a dialog
 * the action opens restores focus there when it closes), then runs it.
 *
 * The button is a ⋮ by default; a `trigger` gives it visible content instead
 * (the Outfit Builder's "Reset…", a garment row's `+` slot picker), keeping
 * every menu behaviour above. With no entry left the component renders
 * nothing.
 *
 * Both document listeners (the outside press, the capture-phase Escape) open
 * with the floating-layer guard — they are rows of `ui/floating-layer-guards.
 * spec.ts`, as v4's one `useClickOutside` is guarded (`99463fc56`).
 */
@Component({
  selector: 'qt-item-action-menu',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [NgTemplateOutlet],
  template: `
    @if (items().length > 0) {
      <div class="relative" #container>
        <button
          #anchor
          type="button"
          [attr.class]="trigger()?.className ?? 'qt-button-ghost qt-button-sm'"
          [attr.aria-label]="buttonAriaLabel()"
          [attr.title]="trigger()?.title ?? label()"
          aria-haspopup="menu"
          [attr.aria-expanded]="menu.open()"
          [disabled]="disabled()"
          (click)="menu.setOpen(!menu.open())"
          (keydown)="onButtonKeydown($event)"
        >
          {{ trigger() ? trigger()!.content : '⋮' }}
        </button>
        @if (menu.style(); as p) {
          <div
            role="presentation"
            [style.position]="p.position"
            [style.right.px]="p.right"
            [style.top.px]="p.top ?? null"
            [style.bottom.px]="p.bottom ?? null"
            [style.max-height.px]="p.maxHeight"
            [style.overflow-y]="p.overflowY"
            [attr.class]="'z-30 ' + minWidthClass() + ' rounded border qt-border-default qt-bg-popover shadow-md'"
          >
            <ul
              #list
              role="menu"
              [attr.aria-label]="label()"
              class="divide-y qt-border-default"
              (keydown)="onMenuKeydown($event)"
            >
              @for (entry of items(); track entry.key) {
                <li role="none">
                  <button
                    type="button"
                    role="menuitem"
                    [attr.data-action]="entry.key"
                    tabindex="-1"
                    [disabled]="!!entry.disabled"
                    [attr.class]="entry.destructive ? entryClass + ' qt-text-destructive' : entryClass"
                    (click)="choose(entry)"
                  >
                    @if (entry.template) {
                      <ng-container [ngTemplateOutlet]="entry.template" />
                    } @else {
                      {{ entry.label }}
                    }
                  </button>
                </li>
              }
            </ul>
          </div>
        }
      </div>
    }
  `,
})
export class ItemActionMenu {
  private readonly doc = inject(DOCUMENT);

  readonly entries = input.required<readonly MenuEntryLike[]>();
  /** The menu's accessible name (and the ⋮ button's). */
  readonly label = input('More actions');
  /** Show this instead of the ⋮ glyph. */
  readonly trigger = input<MenuTrigger | undefined>(undefined);
  /** Disable the ⋮ button (a browser's select mode). */
  readonly disabled = input(false);
  /** Room the menu wants below its button before it opens upward. */
  readonly room = input<number | undefined>(undefined);
  /** Tailwind min-width of the menu. */
  readonly minWidthClass = input('min-w-[14rem]');

  protected readonly entryClass = ITEM_ACTION_ENTRY_CLASS;
  protected readonly items = computed(() => presentEntries(this.entries()));
  /** v4 `aria-label={trigger ? trigger.ariaLabel : label}` — an absent ariaLabel omits the attribute. */
  protected readonly buttonAriaLabel = computed(() => {
    const trigger = this.trigger();
    return trigger ? (trigger.ariaLabel ?? null) : this.label();
  });

  private readonly container = viewChild<ElementRef<HTMLElement>>('container');
  private readonly anchor = viewChild<ElementRef<HTMLButtonElement>>('anchor');
  private readonly list = viewChild<ElementRef<HTMLUListElement>>('list');

  protected readonly menu = createFixedMenu({
    container: () => this.container()?.nativeElement,
    anchor: () => this.anchor()?.nativeElement,
    room: () => this.room(),
  });

  /** An outside press closes the menu (v4 `useClickOutside`'s `handleClickOutside`). */
  private readonly onDocumentMousedown = (event: MouseEvent): void => {
    if (isInFloatingLayer(event.target)) return;
    const container = this.container()?.nativeElement;
    if (container && event.target instanceof Node && container.contains(event.target)) return;
    this.menu.close();
  };

  /** Escape closes and refocuses, swallowed in the capture phase (v4 `escapeCapture: true`). */
  private readonly onDocumentKeydown = (event: KeyboardEvent): void => {
    if (isInFloatingLayer(event.target)) return;
    if (event.key !== 'Escape') return;
    event.stopPropagation();
    event.preventDefault();
    this.closeAndRefocus();
  };

  constructor() {
    effect((onCleanup) => {
      if (!this.menu.open()) return;
      const doc = this.doc;
      doc.addEventListener('mousedown', this.onDocumentMousedown);
      doc.addEventListener('keydown', this.onDocumentKeydown, true);
      onCleanup(() => {
        doc.removeEventListener('mousedown', this.onDocumentMousedown);
        doc.removeEventListener('keydown', this.onDocumentKeydown, true);
      });
    });

    // Opening the menu puts focus on its first enabled entry — without
    // scrolling (`c34986421`): the menu is fixed to the viewport, and a scroll
    // the focus provoked in an enclosing pane would close it again.
    afterRenderEffect({
      write: () => {
        const list = this.list()?.nativeElement;
        if (this.menu.open() && list) this.enabledItems(list)[0]?.focus({ preventScroll: true });
      },
    });
  }

  protected onButtonKeydown(event: KeyboardEvent): void {
    if (!this.menu.open() && (event.key === 'ArrowDown' || event.key === 'ArrowUp')) {
      event.preventDefault();
      this.menu.setOpen(true);
    }
  }

  protected onMenuKeydown(event: KeyboardEvent): void {
    const list = this.list()?.nativeElement;
    if (!list) return;
    const buttons = this.enabledItems(list);
    if (buttons.length === 0) return;
    const at = buttons.indexOf(this.doc.activeElement as HTMLButtonElement);
    let next: number | null = null;
    if (event.key === 'ArrowDown') next = at < 0 ? 0 : (at + 1) % buttons.length;
    else if (event.key === 'ArrowUp') next = at < 0 ? buttons.length - 1 : (at - 1 + buttons.length) % buttons.length;
    else if (event.key === 'Home') next = 0;
    else if (event.key === 'End') next = buttons.length - 1;
    else if (event.key === 'Tab') {
      this.menu.close();
      return;
    }
    if (next === null) return;
    event.preventDefault();
    buttons[next].focus({ preventScroll: true });
  }

  /** v4: close, refocus the button, THEN run the entry. */
  protected choose(entry: MenuEntry): void {
    this.closeAndRefocus();
    entry.onSelect();
  }

  private closeAndRefocus(): void {
    this.menu.close();
    this.anchor()?.nativeElement.focus();
  }

  private enabledItems(list: HTMLUListElement): HTMLButtonElement[] {
    return Array.from(list.querySelectorAll<HTMLButtonElement>('button[role="menuitem"]:not(:disabled)'));
  }
}
