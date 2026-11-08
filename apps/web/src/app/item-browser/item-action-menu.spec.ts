import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { FLOATING_LAYER_ATTR } from '../ui/floating-layer';
import { Modal } from '../ui/modal';
import {
  ITEM_ACTION_ENTRY_CLASS,
  ItemActionMenu,
  presentEntries,
  type MenuEntryLike,
  type MenuTrigger,
} from './item-action-menu';

/**
 * v4 `components/item-browser/ItemActionMenu.tsx` at the pin `624d0d25c`
 * (`b2b783b47`, `c34986421`'s `preventScroll`, `600c2906f`'s `trigger`).
 * Transcribed from v4 `__tests__/unit/components/item-browser/
 * item-action-menu.test.tsx` — the `ItemActionMenu` suite (6) and
 * `ItemActionMenu with a visible trigger` (2); the `ItemFolderRow` pair lives
 * in `item-folder-row.spec.ts`. Plus the order's own rows: `qt-bg-popover`,
 * `data-action`, the `preventScroll` focus, Escape inside an enclosing
 * `qt-modal`, and a click inside the floating layer.
 */

@Component({
  imports: [ItemActionMenu, Modal],
  template: `
    <p class="elsewhere">elsewhere</p>
    @if (inModal()) {
      <qt-modal title="Dialog" (close)="modalClosed = modalClosed + 1">
        <qt-item-action-menu [entries]="entries()" [label]="label()" [trigger]="trigger()" [disabled]="disabled()" />
      </qt-modal>
    } @else {
      <qt-item-action-menu
        [entries]="entries()"
        [label]="label()"
        [trigger]="trigger()"
        [disabled]="disabled()"
        [room]="room()"
        [minWidthClass]="minWidth()"
      />
    }
  `,
})
class Host {
  readonly entries = signal<readonly MenuEntryLike[]>([]);
  readonly label = signal('More actions');
  readonly trigger = signal<MenuTrigger | undefined>(undefined);
  readonly disabled = signal(false);
  readonly inModal = signal(false);
  readonly room = signal<number | undefined>(undefined);
  readonly minWidth = signal('min-w-[14rem]');
  modalClosed = 0;
}

function render(setup: (h: Host) => void) {
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  setup(fixture.componentInstance);
  document.body.appendChild(fixture.nativeElement);
  fixture.detectChanges();
  const el = fixture.nativeElement as HTMLElement;
  const button = (name = 'More actions') =>
    [...el.querySelectorAll('button')].find(
      (b) => (b.getAttribute('aria-label') ?? b.textContent?.trim()) === name,
    ) as HTMLButtonElement;
  const menu = () => el.querySelector('[role="menu"]') as HTMLElement | null;
  const items = () => [...el.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')];
  const settle = () => {
    fixture.detectChanges();
    TestBed.tick();
  };
  const open = (name?: string) => {
    button(name).click();
    settle();
  };
  const key = (target: EventTarget, k: string) => {
    target.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true }));
    settle();
  };
  return { fixture, el, host: fixture.componentInstance, button, menu, items, open, key, settle };
}

const entry = (key: string, label: string, extra: Partial<{ disabled: boolean; destructive: boolean }> = {}) => ({
  key,
  label,
  onSelect: vi.fn(),
  ...extra,
});

describe('ItemActionMenu', () => {
  afterEach(() => {
    document.body.innerHTML = '';
    TestBed.resetTestingModule();
  });

  it('drops falsy entries and renders nothing when none remain', () => {
    const r = render((h) => h.entries.set([false, null, undefined]));
    expect(r.el.querySelector('qt-item-action-menu')!.children).toHaveLength(0);
    r.host.entries.set([false, entry('edit', 'Edit')]);
    r.settle();
    r.open();
    expect(r.items().map((b) => b.textContent!.trim())).toEqual(['Edit']);
  });

  it('focuses the first enabled entry and moves with the arrow keys, skipping disabled ones', () => {
    const r = render((h) =>
      h.entries.set([
        entry('a', 'Alpha', { disabled: true }),
        entry('b', 'Bravo'),
        entry('c', 'Charlie'),
        entry('d', 'Delta'),
      ]),
    );
    r.open();
    const menu = r.menu()!;
    const active = () => document.activeElement?.textContent?.trim();
    expect(active()).toBe('Bravo');
    r.key(menu, 'ArrowDown');
    expect(active()).toBe('Charlie');
    r.key(menu, 'End');
    expect(active()).toBe('Delta');
    r.key(menu, 'ArrowDown');
    expect(active()).toBe('Bravo');
    r.key(menu, 'ArrowUp');
    expect(active()).toBe('Delta');
    r.key(menu, 'Home');
    expect(active()).toBe('Bravo');
  });

  it('closes on Escape and gives focus back to the button', () => {
    const r = render((h) => h.entries.set([entry('edit', 'Edit')]));
    r.open();
    r.key(document, 'Escape');
    expect(r.menu()).toBeNull();
    expect(document.activeElement).toBe(r.button());
  });

  it('closes on an outside click', () => {
    const r = render((h) => h.entries.set([entry('edit', 'Edit')]));
    r.open();
    r.el.querySelector('.elsewhere')!.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    r.settle();
    expect(r.menu()).toBeNull();
  });

  it('runs the chosen entry and closes', () => {
    const del = entry('delete', 'Delete', { destructive: true });
    const r = render((h) => h.entries.set([del]));
    r.open();
    const item = r.items()[0];
    expect(item.classList.contains('qt-text-destructive')).toBe(true);
    item.click();
    r.settle();
    expect(del.onSelect).toHaveBeenCalledTimes(1);
    expect(r.menu()).toBeNull();
  });

  it('stays shut while disabled', () => {
    const r = render((h) => {
      h.disabled.set(true);
      h.entries.set([entry('edit', 'Edit')]);
    });
    expect(r.button().disabled).toBe(true);
  });

  // ---- the order's own rows -------------------------------------------------

  it('the menu is opaque (qt-bg-popover), fixed, and each entry carries data-action, tabindex -1 and v4’s class', () => {
    const r = render((h) => {
      h.entries.set([entry('edit', 'Edit'), entry('delete', 'Delete', { destructive: true })]);
      h.minWidth.set('min-w-[10rem]');
    });
    const btn = r.button();
    expect(btn.getAttribute('aria-haspopup')).toBe('menu');
    expect(btn.getAttribute('aria-expanded')).toBe('false');
    expect(btn.getAttribute('title')).toBe('More actions');
    expect(btn.getAttribute('class')).toBe('qt-button-ghost qt-button-sm');
    expect(btn.textContent!.trim()).toBe('⋮');
    r.open();
    expect(btn.getAttribute('aria-expanded')).toBe('true');
    const wrap = r.menu()!.parentElement!;
    expect(wrap.getAttribute('role')).toBe('presentation');
    expect(wrap.getAttribute('class')).toBe('z-30 min-w-[10rem] rounded border qt-border-default qt-bg-popover shadow-md');
    expect(wrap.style.position).toBe('fixed');
    expect(wrap.style.overflowY).toBe('auto');
    expect(r.menu()!.getAttribute('aria-label')).toBe('More actions');
    expect(r.menu()!.getAttribute('class')).toBe('divide-y qt-border-default');
    expect(r.menu()!.querySelector('li')!.getAttribute('role')).toBe('none');
    const [edit, del] = r.items();
    expect(edit.dataset['action']).toBe('edit');
    expect(del.dataset['action']).toBe('delete');
    expect(edit.getAttribute('tabindex')).toBe('-1');
    expect(edit.getAttribute('class')).toBe(ITEM_ACTION_ENTRY_CLASS);
    expect(del.getAttribute('class')).toBe(`${ITEM_ACTION_ENTRY_CLASS} qt-text-destructive`);
    expect(ITEM_ACTION_ENTRY_CLASS).toBe('block w-full text-left px-3 py-2 text-sm hover:qt-bg-muted disabled:opacity-50');
  });

  it('opening focuses without scrolling (c34986421 — a provoked scroll would close the fixed menu)', () => {
    const spy = vi.spyOn(HTMLElement.prototype, 'focus');
    const r = render((h) => h.entries.set([entry('edit', 'Edit')]));
    r.open();
    expect(spy).toHaveBeenCalledWith({ preventScroll: true });
    spy.mockRestore();
  });

  it('ArrowDown / ArrowUp on the CLOSED button opens it', () => {
    const r = render((h) => h.entries.set([entry('a', 'Alpha'), entry('b', 'Bravo')]));
    r.key(r.button(), 'ArrowDown');
    expect(r.menu()).not.toBeNull();
    r.key(document, 'Escape');
    r.key(r.button(), 'ArrowUp');
    expect(r.menu()).not.toBeNull();
  });

  it('Tab closes it', () => {
    const r = render((h) => h.entries.set([entry('a', 'Alpha')]));
    r.open();
    r.key(r.menu()!, 'Tab');
    expect(r.menu()).toBeNull();
  });

  it('Escape inside an enclosing qt-modal closes only the MENU (swallowed in the capture phase)', () => {
    const r = render((h) => {
      h.inModal.set(true);
      h.entries.set([entry('a', 'Alpha')]);
    });
    r.open();
    r.key(document.activeElement!, 'Escape');
    expect(r.menu()).toBeNull();
    expect(r.host.modalClosed).toBe(0);
    // The menu is shut: the next Escape is the dialog's.
    r.key(document.activeElement!, 'Escape');
    expect(r.host.modalClosed).toBe(1);
  });

  it('a press or an Escape inside the floating layer leaves it open', () => {
    const r = render((h) => h.entries.set([entry('a', 'Alpha')]));
    const layer = document.createElement('div');
    layer.setAttribute(FLOATING_LAYER_ATTR, '');
    const inner = document.createElement('button');
    layer.appendChild(inner);
    document.body.appendChild(layer);
    r.open();
    inner.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    r.key(inner, 'Escape');
    expect(r.menu()).not.toBeNull();
  });

  it('a press inside the menu does not close it', () => {
    const r = render((h) => h.entries.set([entry('a', 'Alpha')]));
    r.open();
    r.items()[0].dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    r.settle();
    expect(r.menu()).not.toBeNull();
  });
});

describe('ItemActionMenu with a visible trigger', () => {
  afterEach(() => {
    document.body.innerHTML = '';
    TestBed.resetTestingModule();
  });

  it('shows the trigger content, names the menu by its label, and keeps the menu behaviour', () => {
    const defaults = entry('defaults', 'Reset to defaults');
    const r = render((h) => {
      h.label.set('Reset the staged composition');
      h.trigger.set({ content: 'Reset…' });
      h.entries.set([defaults, entry('clear', 'Clear all')]);
    });
    const button = r.button('Reset…');
    expect(button.hasAttribute('aria-label')).toBe(false);
    expect(button.getAttribute('title')).toBe('Reset the staged composition');
    r.open('Reset…');
    expect(r.menu()!.getAttribute('aria-label')).toBe('Reset the staged composition');
    expect(document.activeElement?.textContent?.trim()).toBe('Reset to defaults');
    r.items()[0].click();
    r.settle();
    expect(defaults.onSelect).toHaveBeenCalledTimes(1);
    expect(r.menu()).toBeNull();
    expect(document.activeElement).toBe(button);
  });

  it('takes an accessible name for an icon trigger', () => {
    const r = render((h) => {
      h.label.set('Add to a slot');
      h.trigger.set({ content: '+', ariaLabel: 'Add to a slot' });
      h.entries.set([entry('top', 'Top')]);
    });
    r.open('Add to a slot');
    r.key(document, 'Escape');
    expect(r.menu()).toBeNull();
  });

  it('takes the trigger’s class and title over the defaults', () => {
    const r = render((h) => {
      h.trigger.set({ content: '+', className: 'qt-button-secondary qt-button-sm', title: 'Add', ariaLabel: 'Add' });
      h.entries.set([entry('top', 'Top')]);
    });
    const b = r.button('Add');
    expect(b.getAttribute('class')).toBe('qt-button-secondary qt-button-sm');
    expect(b.getAttribute('title')).toBe('Add');
    expect(b.textContent!.trim()).toBe('+');
  });
});

describe('presentEntries', () => {
  it('drops false, null and undefined, keeping order', () => {
    const a = entry('a', 'A');
    const b = entry('b', 'B');
    expect(presentEntries([false, a, null, undefined, b])).toEqual([a, b]);
  });
});
