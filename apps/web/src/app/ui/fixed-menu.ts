import { DOCUMENT } from '@angular/common';
import { effect, inject, signal, type Signal } from '@angular/core';

/**
 * A ⋮-style menu fixed to the viewport beside its button — a port of v4
 * `hooks/useFixedMenu.ts` at the pin `624d0d25c` (NEW at `29ba4d3f8`).
 *
 * v4's module doc, carried: a menu positioned `absolute` inside a scrolling
 * list is clipped by that list and painted under whatever sits below it (a
 * dialog footer, the next row). Fixing it to the viewport lets it escape both;
 * it opens upward when the room below is short and the room above is longer.
 * A fixed menu would drift from its button, so any scroll outside the
 * container, or a resize, closes it.
 *
 * The card holding the button must not be transformed while the menu is open
 * (a transformed ancestor captures `position: fixed`): give the button
 * `aria-haspopup` and `aria-expanded`, and the `qt-card` rule that lifts a
 * card with an open popup also clears its hover transform (P4.D383's
 * `_surfaces.css` rule).
 *
 * v5 shape: the placement is DOM-free (the button's rect and the viewport size
 * come in as numbers — v5's R-15 shape, so the arithmetic is pinned as pure
 * rows), and `useFixedMenu` becomes {@link createFixedMenu}, a signal factory
 * called in an injection context.
 *
 * @module ui/fixed-menu
 */

/** Room the menu wants below its button before it opens upward instead (v4 `DEFAULT_ROOM`). */
export const FIXED_MENU_DEFAULT_ROOM = 340;
/** v4 `GAP`. */
export const FIXED_MENU_GAP = 4;

/** Where the menu sits, in pixels — the style v4's `fixedMenuPlacement` returns. */
export interface FixedMenuPlacement {
  position: 'fixed';
  right: number;
  top?: number;
  bottom?: number;
  maxHeight: number;
  overflowY: 'auto';
}

/** Where a fixed menu sits beside a button at `rect`, right-aligned to it. */
export function fixedMenuPlacement(
  rect: { top: number; bottom: number; right: number },
  innerWidth: number,
  innerHeight: number,
  room: number = FIXED_MENU_DEFAULT_ROOM,
): FixedMenuPlacement {
  const below = innerHeight - rect.bottom - FIXED_MENU_GAP;
  const above = rect.top - FIXED_MENU_GAP;
  const up = below < room && above > below;
  return {
    position: 'fixed',
    right: Math.max(FIXED_MENU_GAP, innerWidth - rect.right),
    ...(up ? { bottom: innerHeight - rect.top + FIXED_MENU_GAP } : { top: rect.bottom + FIXED_MENU_GAP }),
    maxHeight: Math.max(120, up ? above : below),
    overflowY: 'auto',
  };
}

export interface FixedMenuOptions {
  /** Wraps the button and the menu; scrolling inside it (the menu's own overflow) does not close the menu. */
  container: () => HTMLElement | undefined;
  /** The button the menu opens from. */
  anchor: () => HTMLElement | undefined;
  /** Room the menu wants below its button (v4 `room`); undefined is the default. */
  room?: () => number | undefined;
}

export interface FixedMenu {
  /** The menu's placement; null while closed (v4 `menuStyle`). */
  readonly style: Signal<FixedMenuPlacement | null>;
  readonly open: Signal<boolean>;
  /** Open (placed beside the anchor) or close; opening with no anchor stays closed. */
  setOpen(next: boolean): void;
  close(): void;
}

/**
 * v4 `useFixedMenu(containerRef, room)`. While open, a capture-phase `scroll`
 * OUTSIDE the container closes it, and any `resize` closes it — the listeners
 * exist only while the menu is open.
 */
export function createFixedMenu(opts: FixedMenuOptions): FixedMenu {
  const doc = inject(DOCUMENT);
  const style = signal<FixedMenuPlacement | null>(null);
  const open = signal(false);

  const close = (): void => {
    style.set(null);
    open.set(false);
  };

  const setOpen = (next: boolean): void => {
    const anchor = next ? opts.anchor() : undefined;
    const view = anchor?.ownerDocument.defaultView ?? doc.defaultView;
    if (!anchor || !view) {
      close();
      return;
    }
    style.set(fixedMenuPlacement(anchor.getBoundingClientRect(), view.innerWidth, view.innerHeight, opts.room?.()));
    open.set(true);
  };

  effect((onCleanup) => {
    if (!open()) return;
    const win = doc.defaultView;
    if (!win) return;
    const onScroll = (event: Event): void => {
      const container = opts.container();
      if (container && event.target instanceof Node && container.contains(event.target)) return;
      close();
    };
    win.addEventListener('scroll', onScroll, true);
    win.addEventListener('resize', close);
    onCleanup(() => {
      win.removeEventListener('scroll', onScroll, true);
      win.removeEventListener('resize', close);
    });
  });

  return { style: style.asReadonly(), open: open.asReadonly(), setOpen, close };
}
