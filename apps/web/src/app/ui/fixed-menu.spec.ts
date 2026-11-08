import { TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import {
  FIXED_MENU_DEFAULT_ROOM,
  FIXED_MENU_GAP,
  createFixedMenu,
  fixedMenuPlacement,
  type FixedMenu,
} from './fixed-menu';

/**
 * v4 `hooks/useFixedMenu.ts` (`29ba4d3f8`, at the pin `624d0d25c`) — the
 * viewport-fixed ⋮ menu shared by the wardrobe, Inventory, scenario and folder
 * rows. The placement arithmetic is DOM-free here (the R-15 shape: the
 * button's rect and the viewport size come in as numbers); the lifecycle is
 * `createFixedMenu`, a signal factory called in an injection context.
 *
 * Transcribed from v4 `__tests__/unit/hooks/useFixedMenu.test.ts` (9 cases:
 * 4 `fixedMenuPlacement`, 5 `useFixedMenu`), plus a `room` row per caller
 * (`120` folder rows, `200` scenario rows, `240` the slot picker) and the four
 * `menuPlacement` cases that moved here from `inventory-item-row.spec.ts`.
 */

/** v4's `anchorAt(top, bottom, right = 900)`, as the rect alone. */
const rectAt = (top: number, bottom: number, right = 900) => ({ top, bottom, right });

/** v4's jest viewport (`beforeEach`: 800 high, 1000 wide). */
const W = 1000;
const H = 800;

describe('fixedMenuPlacement (v4 "fixedMenuPlacement")', () => {
  it('pins v4’s constants: 340 px of room, a 4 px gap', () => {
    expect(FIXED_MENU_DEFAULT_ROOM).toBe(340);
    expect(FIXED_MENU_GAP).toBe(4);
  });

  it('opens downward, right-aligned, when there is room below', () => {
    expect(fixedMenuPlacement(rectAt(100, 124), W, H)).toEqual({
      position: 'fixed',
      right: 100,
      top: 128,
      maxHeight: 800 - 124 - 4,
      overflowY: 'auto',
    });
  });

  it('opens upward when the room below is short and above is longer', () => {
    const style = fixedMenuPlacement(rectAt(700, 724), W, H);
    expect(style.bottom).toBe(800 - 700 + 4);
    expect(style.top).toBeUndefined();
    expect(style.maxHeight).toBe(696);
  });

  it('stays downward when short below but shorter still above', () => {
    const style = fixedMenuPlacement(rectAt(100, 124), W, H, 2000);
    expect(style.top).toBe(128);
    expect(style.bottom).toBeUndefined();
  });

  it('keeps a minimum height and a gap from the right edge', () => {
    const style = fixedMenuPlacement(rectAt(770, 790, 1000), W, H);
    expect(style.right).toBe(4);
    expect(style.maxHeight).toBeGreaterThanOrEqual(120);
  });

  // The four cases formerly in `inventory-item-row.spec.ts` (`menuPlacement`, R-15).
  it('opens below with room to spare (moved from the inventory row)', () => {
    expect(fixedMenuPlacement({ top: 100, bottom: 120, right: 900 }, 1000, 800)).toEqual({
      position: 'fixed',
      right: 100,
      top: 124,
      maxHeight: 676,
      overflowY: 'auto',
    });
  });

  it('opens upward when the room below is short and the room above is longer (moved)', () => {
    expect(fixedMenuPlacement({ top: 700, bottom: 720, right: 990 }, 1000, 800)).toEqual({
      position: 'fixed',
      right: 10,
      bottom: 104,
      maxHeight: 696,
      overflowY: 'auto',
    });
  });

  it('stays below when short both ways but below is longer; maxHeight floors at 120 (moved)', () => {
    expect(fixedMenuPlacement({ top: 50, bottom: 70, right: 500 }, 1000, 150)).toEqual({
      position: 'fixed',
      right: 500,
      top: 74,
      maxHeight: 120,
      overflowY: 'auto',
    });
  });

  it('right floors at the 4px gap (moved)', () => {
    expect(fixedMenuPlacement({ top: 0, bottom: 20, right: 1100 }, 1000, 800).right).toBe(4);
  });

  // One row per caller's `room` (v4 `room={120}` / `{200}` / `{240}`): a button
  // 300 px above the viewport's foot with 400 px above it.
  for (const [caller, room, up] of [
    ['a folder row', 120, false],
    ['a scenario row', 200, false],
    ['the slot picker', 240, false],
    ['the default', FIXED_MENU_DEFAULT_ROOM, true],
  ] as const) {
    it(`room ${room} (${caller}) decides the direction against 296 px below`, () => {
      const style = fixedMenuPlacement(rectAt(400, 500), W, H, room);
      expect(style.top === undefined).toBe(up);
      expect(style.bottom !== undefined).toBe(up);
    });
  }
});

describe('createFixedMenu (v4 "useFixedMenu")', () => {
  let container: HTMLDivElement;
  let inside: HTMLDivElement;
  let anchor: HTMLButtonElement | undefined;

  beforeEach(() => {
    container = document.createElement('div');
    inside = document.createElement('div');
    container.appendChild(inside);
    document.body.appendChild(container);
    Object.defineProperty(window, 'innerHeight', { configurable: true, value: H });
    Object.defineProperty(window, 'innerWidth', { configurable: true, value: W });
    anchor = document.createElement('button');
    anchor.getBoundingClientRect = () =>
      ({ top: 100, bottom: 124, right: 900, left: 876, width: 24, height: 24, x: 876, y: 100 }) as DOMRect;
  });

  afterEach(() => {
    container.remove();
    TestBed.resetTestingModule();
  });

  function make(withAnchor = true): FixedMenu {
    const menu = TestBed.runInInjectionContext(() =>
      createFixedMenu({ container: () => container, anchor: () => (withAnchor ? anchor : undefined) }),
    );
    return menu;
  }

  function openMenu(): FixedMenu {
    const menu = make();
    menu.setOpen(true);
    TestBed.tick();
    return menu;
  }

  it('starts closed and opens with a placement style', () => {
    const menu = make();
    expect(menu.open()).toBe(false);
    expect(menu.style()).toBeNull();
    menu.setOpen(true);
    TestBed.tick();
    expect(menu.open()).toBe(true);
    expect(menu.style()).toMatchObject({ position: 'fixed', top: 128 });
  });

  it('stays closed when opened without an anchor', () => {
    const menu = make(false);
    menu.setOpen(true);
    TestBed.tick();
    expect(menu.open()).toBe(false);
  });

  it('ignores a scroll inside its container', () => {
    const menu = openMenu();
    inside.dispatchEvent(new Event('scroll'));
    expect(menu.open()).toBe(true);
  });

  it('closes on a scroll outside its container', () => {
    const menu = openMenu();
    document.dispatchEvent(new Event('scroll'));
    expect(menu.open()).toBe(false);
  });

  it('closes on resize', () => {
    const menu = openMenu();
    window.dispatchEvent(new Event('resize'));
    expect(menu.open()).toBe(false);
  });

  it('close() and setOpen(false) both shut it; the listeners go with it', () => {
    const menu = openMenu();
    menu.close();
    TestBed.tick();
    expect(menu.open()).toBe(false);
    menu.setOpen(true);
    TestBed.tick();
    menu.setOpen(false);
    TestBed.tick();
    // A resize after close changes nothing (and throws nothing).
    window.dispatchEvent(new Event('resize'));
    expect(menu.open()).toBe(false);
  });

  it('places by the room it was given', () => {
    anchor!.getBoundingClientRect = () =>
      ({ top: 400, bottom: 500, right: 900, left: 876, width: 24, height: 100, x: 876, y: 400 }) as DOMRect;
    const menu = TestBed.runInInjectionContext(() =>
      createFixedMenu({ container: () => container, anchor: () => anchor, room: () => 120 }),
    );
    menu.setOpen(true);
    expect(menu.style()?.top).toBe(504);
  });
});
