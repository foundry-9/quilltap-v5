import { TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { WorkspaceTab } from '../workspace-contract';
import { TabStrip } from './tab-strip';

/**
 * Workspace tab strip keyboard access (WCAG 2.1.1, 4.1.2) — v4
 * `__tests__/unit/components/workspace/tab-strip-keyboard.test.tsx`
 * (`94f041764`, read at `624d0d25c`), its five `it`s transcribed one to one
 * (P4.D383 item 10): each tab is a real `role="tab"` button — reachable by
 * Tab when active, arrows / Home / End move and select, Delete closes — and
 * the close cross sits beside the tab rather than nested inside it, hidden
 * from assistive tech (a tablist may own only tabs).
 *
 * Plus the two v5 hooks R12 keeps on the OUTER element: `data-tab-id` and the
 * `qt-tab` / `qt-tab-active` classes (every e2e tab selector and the
 * active-tab `scrollIntoView` effect read them).
 */

const tabs: Record<string, WorkspaceTab> = {
  a: { id: 'a', kind: 'home', title: 'Home' },
  b: { id: 'b', kind: 'home', title: 'Second' },
  c: { id: 'c', kind: 'home', title: 'Third' },
};

function renderStrip(activeTabId = 'a') {
  TestBed.configureTestingModule({ imports: [TabStrip] });
  const fixture = TestBed.createComponent(TabStrip);
  fixture.componentRef.setInput('pane', 'left');
  fixture.componentRef.setInput('paneState', { order: ['a', 'b', 'c'], activeTabId });
  fixture.componentRef.setInput('tabs', tabs);
  const onSelect = vi.fn();
  const onClose = vi.fn();
  fixture.componentInstance.select.subscribe(onSelect);
  fixture.componentInstance.close.subscribe(onClose);
  fixture.detectChanges();
  const root = fixture.nativeElement as HTMLElement;
  const allTabs = () => [...root.querySelectorAll<HTMLElement>('[role="tab"]')];
  const tabNamed = (name: string) => allTabs().find((t) => t.textContent?.trim() === name)!;
  const keyDown = (el: HTMLElement, key: string) => {
    el.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }));
    fixture.detectChanges();
  };
  return { fixture, root, allTabs, tabNamed, keyDown, onSelect, onClose };
}

// jsdom ships no `CSS.escape`, which the active-tab scroll effect calls; a
// minimal stand-in for the plain ids these fixtures use.
beforeEach(() => {
  if (typeof (globalThis as { CSS?: { escape?: unknown } }).CSS?.escape !== 'function') {
    vi.stubGlobal('CSS', { escape: (v: string) => v.replace(/["\\]/g, '\\$&') });
  }
});

afterEach(() => {
  TestBed.resetTestingModule();
  vi.unstubAllGlobals();
});

describe('TabStrip keyboard access', () => {
  // v4 tab-strip-keyboard.test.tsx:37-44
  it('renders each tab as a button with role tab, only the active one tabbable', () => {
    const { allTabs } = renderStrip('b');
    const rendered = allTabs();
    expect(rendered).toHaveLength(3);
    expect(rendered.map((t) => t.tagName)).toEqual(['BUTTON', 'BUTTON', 'BUTTON']);
    expect(rendered.map((t) => t.getAttribute('tabindex'))).toEqual(['-1', '0', '-1']);
    expect(rendered[1].getAttribute('aria-selected')).toBe('true');
  });

  // v4 tab-strip-keyboard.test.tsx:46-56
  it("keeps the close cross out of the tab and out of the tablist's accessible children", () => {
    const { tabNamed, onClose } = renderStrip();
    const tab = tabNamed('Home');
    const close = tab.parentElement!.querySelector('.qt-tab-close') as HTMLElement;
    expect(close).not.toBeNull();
    expect(tab.contains(close)).toBe(false);
    expect(close.getAttribute('aria-hidden')).toBe('true');
    expect(close.getAttribute('tabindex')).toBe('-1');
    expect(tab.getAttribute('aria-keyshortcuts')).toBe('Delete');
    close.click();
    expect(onClose).toHaveBeenCalledWith('a');
  });

  // v4 tab-strip-keyboard.test.tsx:58-67
  it('moves and selects with the arrow keys, wrapping at the ends', () => {
    const { allTabs, keyDown, onSelect } = renderStrip('a');
    const [first, , third] = allTabs();
    keyDown(first, 'ArrowRight');
    expect(onSelect).toHaveBeenLastCalledWith('b');
    keyDown(first, 'ArrowLeft');
    expect(onSelect).toHaveBeenLastCalledWith('c');
    keyDown(third, 'ArrowRight');
    expect(onSelect).toHaveBeenLastCalledWith('a');
  });

  // v4 tab-strip-keyboard.test.tsx:69-78
  it('jumps with Home and End and closes with Delete', () => {
    const { allTabs, keyDown, onSelect, onClose } = renderStrip('b');
    const [, second] = allTabs();
    keyDown(second, 'End');
    expect(onSelect).toHaveBeenLastCalledWith('c');
    keyDown(second, 'Home');
    expect(onSelect).toHaveBeenLastCalledWith('a');
    keyDown(second, 'Delete');
    expect(onClose).toHaveBeenCalledWith('b');
  });

  // v4 tab-strip-keyboard.test.tsx:80-84
  it('selects on click', () => {
    const { tabNamed, onSelect } = renderStrip('a');
    tabNamed('Third').click();
    expect(onSelect).toHaveBeenCalledWith('c');
  });
});

describe('TabStrip v5 hooks (R12)', () => {
  it('keeps data-tab-id and the qt-tab / qt-tab-active classes on the OUTER element', () => {
    const { root, tabNamed } = renderStrip('b');
    const outer = [...root.querySelectorAll<HTMLElement>('.qt-tab')];
    expect(outer.map((el) => el.getAttribute('data-tab-id'))).toEqual(['a', 'b', 'c']);
    expect(outer.map((el) => el.classList.contains('qt-tab-active'))).toEqual([false, true, false]);
    const inner = tabNamed('Second');
    expect(inner.closest('.qt-tab')).toBe(outer[1]);
    expect(inner.classList.contains('qt-tab-select')).toBe(true);
    expect(outer[1].getAttribute('role')).toBe('presentation');
  });

  it('scrolls the active tab, found by .qt-tab[data-tab-id], into view', async () => {
    const spy = vi.fn();
    const proto = HTMLElement.prototype as unknown as { scrollIntoView?: unknown };
    const had = Object.prototype.hasOwnProperty.call(proto, 'scrollIntoView');
    const prior = proto.scrollIntoView;
    proto.scrollIntoView = spy;
    try {
      const { fixture } = renderStrip('c');
      await fixture.whenStable();
      await new Promise((r) => setTimeout(r, 0));
      expect(spy).toHaveBeenCalledTimes(1);
      const target = spy.mock.contexts[0] as HTMLElement;
      expect(target.classList.contains('qt-tab')).toBe(true);
      expect(target.getAttribute('data-tab-id')).toBe('c');
    } finally {
      if (had) proto.scrollIntoView = prior;
      else delete proto.scrollIntoView;
    }
  });
});
