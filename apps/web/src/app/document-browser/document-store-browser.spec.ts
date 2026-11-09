import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { AlertModalService } from '../ui/alert-modal.service';
import { ToastService } from '../ui/toast.service';
import {
  CAPS,
  DEFAULT_ROUTE,
  FILES,
  provideDocumentStoreTesting,
  settle,
  store,
  type Route,
} from './document-browser.testing';
import type { DocumentEntry } from './document-store.source';
import { DocumentStoreBrowser } from './document-store-browser';

/**
 * v4 `__tests__/unit/components/document-browser/document-store-browser.test.tsx`
 * (`c34986421`, pin `624d0d25c`; seven cases) translated case for case onto
 * `qt-document-store-browser` over v5's dispatch (P4.D387 item 12): v4's
 * mocked `/api/v1/mount-points` routes become `mountPointGet` /
 * `mountFilesList` / `mountPointList`, and its asserted REST calls the
 * matching verbs.
 */

@Component({
  imports: [DocumentStoreBrowser],
  template: `
    <qt-document-store-browser
      storeId="store-a"
      [pickMode]="pick()"
      [footer]="withFooter() ? footerTpl : undefined"
      (pickFile)="picked.push($event)"
    />
    <ng-template #footerTpl let-folder>
      <button type="button">New here {{ folder || '(top)' }}</button>
    </ng-template>
  `,
})
class Host {
  readonly pick = signal(false);
  readonly withFooter = signal(false);
  readonly picked: DocumentEntry[] = [];
}

async function render(
  route: Route = DEFAULT_ROUTE,
  opts: { pick?: boolean; footer?: boolean } = {},
) {
  const confirm = vi.fn(async () => 'Confirm' as string | null);
  const toasts = {
    showSuccess: vi.fn(),
    showError: vi.fn(),
    showInfo: vi.fn(),
    showWarning: vi.fn(),
  };
  const { seen } = provideDocumentStoreTesting(route, [
    provideRouter([]),
    { provide: AlertModalService, useValue: { show: confirm, current: signal(null) } },
    { provide: ToastService, useValue: toasts },
  ]);
  const fixture = TestBed.createComponent(Host);
  fixture.componentInstance.pick.set(opts.pick ?? false);
  fixture.componentInstance.withFooter.set(opts.footer ?? false);
  document.body.appendChild(fixture.nativeElement);
  fixture.detectChanges();
  await settle(fixture);
  const el = fixture.nativeElement as HTMLElement;
  return { fixture, el, seen, confirm, toasts };
}

const buttonNamed = (root: ParentNode, name: string): HTMLButtonElement | undefined =>
  [...root.querySelectorAll('button')].find(
    (b) => b.getAttribute('aria-label') === name || b.textContent!.trim() === name,
  ) as HTMLButtonElement | undefined;

function openMenu(el: HTMLElement, fixture: { detectChanges(): void }, label: string): HTMLElement {
  buttonNamed(el, label)!.click();
  fixture.detectChanges();
  return document.querySelector(`[role="menu"][aria-label="${label}"]`) as HTMLElement;
}
const itemsOf = (menu: HTMLElement) =>
  [...menu.querySelectorAll('[role="menuitem"]')].map((m) => m.textContent!.trim());
const writes = (seen: Array<{ type: string }>) =>
  seen.filter((r) => !['mountPointGet', 'mountFilesList', 'mountPointList'].includes(r.type));

describe('DocumentStoreBrowser (v4 document-store-browser.test.tsx)', () => {
  afterEach(() => {
    TestBed.resetTestingModule();
    document.body.innerHTML = '';
  });

  it('lists the top folder: folder rows (empty ones too) and the files directly inside', async () => {
    const { el, fixture } = await render();
    const folders = [...el.querySelectorAll('[data-testid="documents-folder-row"]')].map(
      (r) => r.textContent!,
    );
    expect(folders).toHaveLength(2);
    expect(folders[0]).toContain('Drafts');
    expect(folders[1]).toContain('Empty');
    expect(el.querySelectorAll('[data-testid="document-row"]')).toHaveLength(1);

    buttonNamed(el, 'Drafts')!.click();
    await settle(fixture);
    expect(el.textContent).toContain('chapter-one.md');
    expect(el.textContent).not.toContain('notes.md');
    expect(el.querySelector('.qt-badge-info')!.textContent!.trim()).toBe('Hard-linked');
  });

  it('offers the link entries on a hard-linked file only', async () => {
    const { el, fixture } = await render();
    const plain = itemsOf(openMenu(el, fixture, 'Actions for notes.md'));
    expect(plain.some((t) => t.startsWith('Where else'))).toBe(false);
    expect(plain).toContain('Hard link to…');
    buttonNamed(el, 'Actions for notes.md')!.click();
    fixture.detectChanges();

    buttonNamed(el, 'Drafts')!.click();
    await settle(fixture);
    const linked = itemsOf(openMenu(el, fixture, 'Actions for chapter-one.md'));
    expect(linked).toContain('Where else this appears…');
    expect(linked).toContain('Unlink from its twins');
  });

  it('renames a file in place through the item verb', async () => {
    const { el, fixture, seen } = await render();
    const menu = openMenu(el, fixture, 'Actions for notes.md');
    buttonNamed(menu, 'Rename')!.click();
    fixture.detectChanges();
    const input = el.querySelector('input[aria-label="Rename notes.md"]') as HTMLInputElement;
    input.value = 'journal.md';
    input.dispatchEvent(new Event('input'));
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await settle(fixture);
    expect(writes(seen)).toContainEqual({
      type: 'mountFileUpdate',
      mountPointId: 'store-a',
      path: 'notes.md',
      rename: 'journal.md',
    });
  });

  it('deletes a folder with everything in it, after saying so', async () => {
    const { el, fixture, seen, confirm } = await render();
    const row = el.querySelectorAll('[data-testid="documents-folder-row"]')[0] as HTMLElement;
    buttonNamed(row, 'Folder actions for Drafts')!.click();
    fixture.detectChanges();
    buttonNamed(document, 'Delete')!.click();
    await settle(fixture);
    expect(writes(seen)).toContainEqual({
      type: 'mountFolderDelete',
      mountPointId: 'store-a',
      path: 'Drafts',
      recursive: true,
    });
    expect((confirm.mock.calls as unknown[][])[0][0]).toContain('the 2 files inside it');
  });

  it('moves a file to a folder of another store', async () => {
    const { el, fixture, seen } = await render();
    const menu = openMenu(el, fixture, 'Actions for notes.md');
    buttonNamed(menu, 'Move to…')!.click();
    await settle(fixture);
    const dialog = el.querySelector('qt-document-destination-dialog') as HTMLElement;
    const select = dialog.querySelector('#document-destination-store') as HTMLSelectElement;
    expect([...select.options].map((o) => o.textContent!.trim())).toContain('Second Shelf');
    select.value = 'store-b';
    select.dispatchEvent(new Event('change'));
    await settle(fixture);
    const inbox = [...dialog.querySelectorAll('[role="option"]')].find((o) =>
      o.textContent!.includes('Inbox'),
    ) as HTMLElement;
    inbox.click();
    fixture.detectChanges();
    buttonNamed(dialog, 'Move')!.click();
    await settle(fixture);
    expect(writes(seen)).toContainEqual({
      type: 'mountFileMove',
      mountPointId: 'store-a',
      sourcePath: 'notes.md',
      destMountPointId: 'store-b',
      destPath: 'Inbox/notes.md',
    });
  });

  it('in pick mode, opening a file hands it over and the footer gets the extra buttons', async () => {
    const { el, fixture } = await render(DEFAULT_ROUTE, { pick: true, footer: true });
    expect(buttonNamed(el, 'New here (top)')).toBeDefined();
    buttonNamed(el, 'notes.md')!.click();
    expect(fixture.componentInstance.picked.map((e) => e.relativePath)).toEqual(['notes.md']);
    buttonNamed(el, 'Drafts')!.click();
    await settle(fixture);
    expect(buttonNamed(el, 'New here Drafts')).toBeDefined();
  });

  it('hides every write on a store that allows none', async () => {
    const closed = {
      ...CAPS,
      canWrite: false,
      canDelete: false,
      canMoveIn: false,
      canMoveOut: false,
    };
    const { el, fixture } = await render((req) => {
      if (req.type === 'mountPointGet') return { mountPoint: store('store-a', 'Archive', closed) };
      if (req.type === 'mountFilesList') return { files: FILES, folders: ['Drafts'] };
      return { mountPoints: [] };
    });
    expect(itemsOf(openMenu(el, fixture, 'Actions for notes.md'))).toEqual(['Open', 'Download']);
    expect(el.textContent).not.toContain('Upload here');
    expect(buttonNamed(el, 'Folder actions for Drafts')).toBeUndefined();
  });
});

describe('DocumentStoreBrowser — v5 extras over v4 bytes', () => {
  afterEach(() => {
    TestBed.resetTestingModule();
    document.body.innerHTML = '';
  });

  it('shows the store name, the search box and the count line', async () => {
    const { el } = await render();
    expect(el.querySelector('input[type="search"]')!.getAttribute('aria-label')).toBe(
      'Search this store',
    );
    expect(el.textContent).toContain('3 files · 300 B');
    expect(el.textContent).toContain('Archive');
    expect(el.textContent).toContain('Upload here');
  });

  it('confirms a single delete with v4 text and toasts the count', async () => {
    const { el, fixture, seen, confirm, toasts } = await render();
    const menu = openMenu(el, fixture, 'Actions for notes.md');
    buttonNamed(menu, 'Delete')!.click();
    await settle(fixture);
    expect((confirm.mock.calls as unknown[][])[0]).toEqual([
      'Delete "notes.md"? It goes for good, and links to it elsewhere will find nothing.',
      ['Cancel', 'Confirm'],
    ]);
    expect(writes(seen)).toContainEqual({
      type: 'mountFileDelete',
      mountPointId: 'store-a',
      path: 'notes.md',
    });
    expect(toasts.showSuccess).toHaveBeenCalledWith('1 file deleted');
  });
});
