import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { ToastService } from '../ui/toast.service';
import {
  BrowserTogglesAfter,
  FolderBrowserShell,
  type BrowserFolderFiling,
  type BrowserToggle,
} from './folder-browser-shell';
import { createFolderBrowserState } from './folder-browser-state';
import { OWN_TIER, type BrowserOrigin } from './folder-view';

/**
 * v4 `components/item-browser/FolderBrowserShell.tsx` at the pin `624d0d25c`
 * (`b2b783b47`, `c34986421`'s `manageFolders`, `600c2906f`'s toggles and
 * folder filing). v4 has no jest file for the shell itself (its wardrobe /
 * Inventory browser tests drive it, transcribed in commit 2); these rows pin
 * its layout, test ids, strings and precedence directly through a host that
 * renders it with an `itemTemplate`.
 */

interface Item {
  id: string;
  title: string;
  folder?: string | null;
  archivedAt?: string | null;
  origin?: BrowserOrigin | null;
}

const PROJECT: BrowserOrigin = { scope: 'project', id: 'p1', name: 'Blackwood' };
const tierOf = (i: Item): string => (i.origin ? `${i.origin.scope}:${i.origin.id}` : OWN_TIER);

@Component({
  imports: [FolderBrowserShell, BrowserTogglesAfter],
  template: `
    <qt-folder-browser-shell
      [state]="state"
      [hasContainer]="hasContainer()"
      [loading]="loading()"
      testIdPrefix="wardrobe"
      rootLabel="Wardrobe"
      [countNoun]="countNoun()"
      [text]="text"
      [toggles]="toggles()"
      [itemTemplate]="row"
      [folderFiling]="filing()"
      [folderHandlers]="handlers"
      [manageFolders]="manageFolders()"
      [scrollList]="scrollList()"
    >
      <input qtBrowserFilters class="search" />
      @if (afterSlot()) {
        <label qtBrowserTogglesAfter class="never-worn">Never worn</label>
      }
      <button qtBrowserSelectActions type="button" class="extra-select">Transfer…</button>
      <button qtBrowserFooter type="button" class="footer-new">New item</button>
      <p class="mounted-child">dialogs</p>
    </qt-folder-browser-shell>
    <ng-template #row let-item>
      <div class="item-row" [attr.data-id]="item.id">{{ item.title }}</div>
    </ng-template>
  `,
})
class Host {
  readonly containerKey = signal('character:c1');
  readonly items = signal<Item[]>([]);
  readonly filtered = signal<Item[]>([]);
  readonly showShared = signal(true);
  readonly searching = signal(false);
  readonly state = createFolderBrowserState<Item>({
    containerKey: this.containerKey,
    items: this.items,
    filtered: this.filtered,
    tree: signal([]),
    tierOf: () => tierOf,
    characterView: signal(true),
    showShared: this.showShared,
    searching: this.searching,
  });
  readonly hasContainer = signal(true);
  readonly loading = signal(false);
  readonly countNoun = signal('item');
  readonly text = { noContainer: 'Pick a wardrobe', folderEmpty: 'Nothing here', noMatch: 'No match' };
  readonly toggles = signal<{ archived?: BrowserToggle; shared?: BrowserToggle } | undefined>(undefined);
  readonly filing = signal<BrowserFolderFiling<Item> | undefined>(undefined);
  readonly manageFolders = signal<boolean | undefined>(undefined);
  readonly scrollList = signal(true);
  readonly afterSlot = signal(false);
  readonly handlers = {
    submitNewFolder: vi.fn(async () => {}),
    renameFolder: vi.fn(async () => true),
    deleteFolder: vi.fn(async () => {}),
  };
}

function render(setup: (h: Host) => void = () => {}) {
  TestBed.configureTestingModule({
    imports: [Host],
    providers: [{ provide: ToastService, useValue: { showSuccess: vi.fn(), showError: vi.fn() } }],
  });
  const fixture = TestBed.createComponent(Host);
  setup(fixture.componentInstance);
  fixture.detectChanges();
  const el = fixture.nativeElement as HTMLElement;
  const q = (sel: string) => el.querySelector(sel) as HTMLElement | null;
  const all = (sel: string) => [...el.querySelectorAll<HTMLElement>(sel)];
  const text = () => el.textContent!.replace(/\s+/g, ' ');
  const buttonByText = (t: string) => all('button').find((b) => b.textContent!.trim() === t) as HTMLButtonElement;
  return { fixture, el, host: fixture.componentInstance, q, all, text, buttonByText };
}

const items: Item[] = [
  { id: 'a', title: 'Coat', folder: 'Formal' },
  { id: 'b', title: 'Hat', folder: '' },
  { id: 'p', title: 'Cape', folder: '', origin: PROJECT },
];

describe('FolderBrowserShell', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('lays out filters, the folder bar, folder + tier rows, the items, and the footer with the four test ids', () => {
    const r = render((h) => {
      h.items.set(items);
      h.filtered.set(items);
    });
    expect(r.q('input.search')).not.toBeNull();
    expect(r.q('[data-testid="wardrobe-folder-bar"]')).not.toBeNull();
    expect(r.q('[data-testid="wardrobe-folder-bar"] nav')!.getAttribute('aria-label')).toBe('Wardrobe folders');
    expect(r.all('[data-testid="wardrobe-folder-row"]').map((e) => e.textContent!.trim())).toEqual([
      expect.stringContaining('Formal'),
    ]);
    expect(r.all('[data-testid="wardrobe-tier-row"]')).toHaveLength(1);
    expect(r.all('.item-row').map((e) => e.dataset['id'])).toEqual(['b']);
    const footer = r.q('button.footer-new')!.parentElement!;
    expect(footer.getAttribute('class')).toBe(
      'sticky bottom-0 -mx-1 px-1 pt-2 pb-1 qt-bg-default border-t qt-border-default flex flex-wrap items-center gap-2 justify-end',
    );
    expect(r.q('p.mounted-child')).not.toBeNull();
    expect(r.q('section')!.getAttribute('class')).toBe('flex flex-col min-h-0 relative');
  });

  it('the select bar carries its test id; Move to folder… / Copy to folder… then the extras; copyBlocked titles + disables Copy', () => {
    const open = vi.fn();
    const r = render((h) => {
      h.items.set(items);
      h.filtered.set(items);
      h.filing.set({ open, copyBlocked: (sel) => (sel.length > 0 ? 'Some of these objects cannot be copied' : undefined) });
    });
    expect(r.q('[data-testid="wardrobe-select-bar"]')).toBeNull();
    r.buttonByText('Select').click();
    r.fixture.detectChanges();
    const bar = r.q('[data-testid="wardrobe-select-bar"]')!;
    expect(bar).not.toBeNull();
    const labels = [...bar.querySelectorAll('button')].map((b) => b.textContent!.trim());
    expect(labels).toEqual(['Move to folder…', 'Copy to folder…', 'Transfer…', 'Cancel']);
    expect(r.buttonByText('Move to folder…').disabled).toBe(true);
    expect(r.buttonByText('Copy to folder…').disabled).toBe(true);
    expect(r.buttonByText('Copy to folder…').hasAttribute('title')).toBe(false);
    r.host.state.toggleSelected(items[1]);
    r.fixture.detectChanges();
    expect(r.buttonByText('Move to folder…').disabled).toBe(false);
    expect(r.buttonByText('Copy to folder…').disabled).toBe(true);
    expect(r.buttonByText('Copy to folder…').getAttribute('title')).toBe('Some of these objects cannot be copied');
    r.buttonByText('Move to folder…').click();
    expect(open).toHaveBeenCalledWith([items[1]], 'move');
    r.buttonByText('Cancel').click();
    r.fixture.detectChanges();
    expect(r.q('[data-testid="wardrobe-select-bar"]')).toBeNull();
  });

  it('the select bar is sticky only when the list scrolls', () => {
    const r = render((h) => {
      h.items.set(items);
      h.filtered.set(items);
      h.scrollList.set(false);
    });
    r.buttonByText('Select').click();
    r.fixture.detectChanges();
    expect(r.q('[data-testid="wardrobe-select-bar"]')!.getAttribute('class')).not.toContain('sticky');
  });

  it('Show archived / Show shared render only when asked; unticking Show shared in a borrowed tier goes home', () => {
    const archived = { checked: false, onChange: vi.fn() };
    const shared = { checked: true, onChange: vi.fn() };
    const r = render((h) => {
      h.items.set(items);
      h.filtered.set(items);
    });
    expect(r.text()).not.toContain('Show archived');
    r.host.toggles.set({ archived, shared });
    r.fixture.detectChanges();
    const boxes = r.all('input[type="checkbox"]') as HTMLInputElement[];
    expect(r.all('label').map((l) => l.textContent!.trim())).toEqual(['Show archived', 'Show shared']);
    expect(boxes.map((b) => b.className)).toEqual(['qt-checkbox', 'qt-checkbox']);
    boxes[0].checked = true;
    boxes[0].dispatchEvent(new Event('change'));
    expect(archived.onChange).toHaveBeenCalledWith(true);
    r.host.state.navigate('project:p1', '');
    r.fixture.detectChanges();
    boxes[1].checked = false;
    boxes[1].dispatchEvent(new Event('change'));
    expect(shared.onChange).toHaveBeenCalledWith(false);
    expect(r.host.state.currentTier()).toBe(OWN_TIER);
  });

  it('a [qtBrowserTogglesAfter] slot alone renders the toggles row', () => {
    const r = render((h) => h.afterSlot.set(true));
    const label = r.q('label.never-worn')!;
    expect(label.parentElement!.getAttribute('class')).toBe('flex flex-wrap items-center gap-x-4 gap-y-1');
  });

  it('no container: the noContainer line and no folder bar', () => {
    const r = render((h) => h.hasContainer.set(false));
    expect(r.q('[data-testid="wardrobe-folder-bar"]')).toBeNull();
    expect(r.text()).toContain('Pick a wardrobe');
  });

  it('Loading… while loading', () => {
    const r = render((h) => h.loading.set(true));
    expect(r.text()).toContain('Loading…');
  });

  it('an empty folder says folderEmpty; a filter that hides every row says noMatch', () => {
    const r = render((h) => h.items.set([]));
    expect(r.text()).toContain('Nothing here');
    r.host.items.set([{ id: 'b', title: 'Hat', folder: '' }]);
    r.host.filtered.set([]);
    r.fixture.detectChanges();
    expect(r.text()).toContain('No match');
    expect(r.text()).not.toContain('Nothing here');
  });

  it('manageFolders = false hides New folder, Select and the folder ⋮', () => {
    const r = render((h) => {
      h.items.set(items);
      h.filtered.set(items);
      h.manageFolders.set(false);
    });
    expect(r.buttonByText('New folder')).toBeUndefined();
    expect(r.buttonByText('Select')).toBeUndefined();
    expect(r.q('[aria-label="Folder actions for Formal"]')).toBeNull();
    r.host.manageFolders.set(undefined);
    r.fixture.detectChanges();
    expect(r.q('[aria-label="Folder actions for Formal"]')).not.toBeNull();
  });

  it('New folder opens the inline input; Enter submits through the handlers', () => {
    const r = render((h) => {
      h.items.set(items);
      h.filtered.set(items);
    });
    r.buttonByText('New folder').click();
    r.fixture.detectChanges();
    const input = r.q('input[aria-label="New folder name"]') as HTMLInputElement;
    expect(input).not.toBeNull();
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    expect(r.host.handlers.submitNewFolder).toHaveBeenCalled();
  });

  it('the list caps its height when scrollList, and grows when not', () => {
    const r = render();
    const list = () => r.q('section > div:nth-of-type(2)')!.getAttribute('class');
    expect(list()).toBe('flex-1 space-y-1 pb-12 overflow-y-auto max-h-[55vh]');
    r.host.scrollList.set(false);
    r.fixture.detectChanges();
    expect(list()).toBe('flex-1 space-y-1 pb-12 ');
  });

  it('the count noun reaches folder and tier rows', () => {
    const r = render((h) => {
      h.items.set(items);
      h.filtered.set(items);
      h.countNoun.set('object');
    });
    expect(r.q('[data-testid="wardrobe-folder-row"]')!.textContent).toContain('1 object');
    expect(r.q('[data-testid="wardrobe-tier-row"]')!.textContent).toContain('1 object');
  });
});
