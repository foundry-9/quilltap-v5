import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { BrowserAction } from './document-actions';
import { file } from './document-browser.testing';
import { DocumentRow, type DocumentRowAction } from './document-row';
import type { DocumentEntry } from './document-store.source';

/**
 * v4 `components/document-browser/DocumentRow.tsx` (`c34986421`, pin
 * `624d0d25c`; P4.D387 item 8): the ⋮ labels for the actions handed in, the
 * in-place rename (Enter submits, Escape reverts and stops, blur submits), the
 * chips, the image thumbnail on the raw leg, and the select-mode toggle.
 */

const entryOf = (path: string, fields: Partial<DocumentEntry> = {}): DocumentEntry => ({
  ...file(path, fields),
  folder: path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '',
  ...fields,
});

@Component({
  imports: [DocumentRow],
  template: `
    <qt-document-row
      [entry]="entry()"
      [actions]="actions()"
      [rename]="rename"
      [selectMode]="selectMode()"
      [showFolder]="showFolder()"
      (acted)="acted.push($event)"
      (toggled)="toggles = toggles + 1"
    />
  `,
})
class Host {
  readonly entry = signal(entryOf('notes.md'));
  readonly actions = signal<readonly BrowserAction[]>([
    'open',
    'rename',
    'move',
    'download',
    'delete',
  ]);
  readonly selectMode = signal(false);
  readonly showFolder = signal(false);
  readonly acted: DocumentRowAction[] = [];
  toggles = 0;
  renamed: string[] = [];
  readonly rename = vi.fn(async (_e: DocumentEntry, name: string) => {
    this.renamed.push(name);
    return true;
  });
}

function render(setup?: (h: Host) => void) {
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  setup?.(fixture.componentInstance);
  document.body.appendChild(fixture.nativeElement);
  fixture.detectChanges();
  return { fixture, host: fixture.componentInstance, el: fixture.nativeElement as HTMLElement };
}

function menuItems(el: HTMLElement, fixture: { detectChanges(): void }): HTMLButtonElement[] {
  (el.querySelector('[aria-label="Actions for notes.md"]') as HTMLButtonElement).click();
  fixture.detectChanges();
  return [...document.querySelectorAll('[role="menuitem"]')] as HTMLButtonElement[];
}

describe('DocumentRow (v4 DocumentRow.tsx)', () => {
  afterEach(() => {
    TestBed.resetTestingModule();
    document.body.innerHTML = '';
  });

  it('labels each handed action with v4 ACTION_LABELS, Delete destructive', () => {
    const { el, fixture, host } = render((h) =>
      h.actions.set([
        'open',
        'rename',
        'move',
        'copy',
        'link',
        'show-links',
        'unlink',
        'describe',
        'workbench',
        'download',
        'delete',
      ]),
    );
    const items = menuItems(el, fixture);
    expect(items.map((b) => b.textContent!.trim())).toEqual([
      'Open',
      'Rename',
      'Move to…',
      'Copy to…',
      'Hard link to…',
      'Where else this appears…',
      'Unlink from its twins',
      'Edit description…',
      "Open in Pascal's Workbench",
      'Download',
      'Delete',
    ]);
    items[8].click();
    expect(host.acted.map((a) => a.action)).toEqual(['workbench']);
  });

  it('carries the row test id + path, and the name button opens', () => {
    const { el, host } = render();
    const row = el.querySelector('[data-testid="document-row"]')!;
    expect(row.getAttribute('data-path')).toBe('notes.md');
    (row.querySelector('button[title="notes.md"]') as HTMLButtonElement).click();
    expect(host.acted.map((a) => a.action)).toEqual(['open']);
  });

  it('renames in place: Enter submits the trimmed name', async () => {
    const { el, fixture, host } = render();
    menuItems(el, fixture)[1].click();
    fixture.detectChanges();
    const input = el.querySelector('input[aria-label="Rename notes.md"]') as HTMLInputElement;
    expect(input.value).toBe('notes.md');
    input.value = '  journal.md ';
    input.dispatchEvent(new Event('input'));
    input.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }),
    );
    await Promise.resolve();
    expect(host.renamed).toEqual(['journal.md']);
  });

  it('Escape reverts and stops there (never reaching a dialog beneath)', () => {
    const { el, fixture, host } = render();
    menuItems(el, fixture)[1].click();
    fixture.detectChanges();
    const input = el.querySelector('input[aria-label="Rename notes.md"]') as HTMLInputElement;
    input.value = 'other.md';
    input.dispatchEvent(new Event('input'));
    const reached = vi.fn();
    document.addEventListener('keydown', reached);
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    document.removeEventListener('keydown', reached);
    fixture.detectChanges();
    expect(reached).not.toHaveBeenCalled();
    expect(el.querySelector('input[aria-label="Rename notes.md"]')).toBeNull();
    expect(host.renamed).toEqual([]);
  });

  it('blur submits; an unchanged name just closes the field', async () => {
    const { el, fixture, host } = render();
    menuItems(el, fixture)[1].click();
    fixture.detectChanges();
    let input = el.querySelector('input[aria-label="Rename notes.md"]') as HTMLInputElement;
    input.dispatchEvent(new Event('blur'));
    await Promise.resolve();
    fixture.detectChanges();
    expect(host.renamed).toEqual([]);
    expect(el.querySelector('input[aria-label="Rename notes.md"]')).toBeNull();
    menuItems(el, fixture)[1].click();
    fixture.detectChanges();
    input = el.querySelector('input[aria-label="Rename notes.md"]') as HTMLInputElement;
    input.value = 'b.md';
    input.dispatchEvent(new Event('input'));
    input.dispatchEvent(new Event('blur'));
    await Promise.resolve();
    expect(host.renamed).toEqual(['b.md']);
  });

  it('shows the chips: Hard-linked, Conversion failed, Converting (pending pdf/docx only)', () => {
    const { el, fixture, host } = render((h) =>
      h.entry.set(
        entryOf('a.pdf', { fileType: 'pdf', linkGroupId: 'g', conversionStatus: 'pending' }),
      ),
    );
    const chips = () =>
      [...el.querySelectorAll('[class^="qt-badge"]')].map((c) => c.textContent!.trim());
    expect(chips()).toEqual(['Hard-linked', 'Converting']);
    expect(el.querySelector('.qt-badge-info')!.getAttribute('title')).toBe(
      'Hard-linked: an edit here reaches every twin',
    );
    host.entry.set(entryOf('a.md', { conversionStatus: 'pending' }));
    fixture.detectChanges();
    expect(chips()).toEqual([]);
    host.entry.set(
      entryOf('a.pdf', {
        fileType: 'pdf',
        conversionStatus: 'failed',
        conversionError: 'bad xref',
      }),
    );
    fixture.detectChanges();
    expect(chips()).toEqual(['Conversion failed']);
    expect(el.querySelector('.qt-badge-destructive')!.getAttribute('title')).toBe('bad xref');
  });

  it('an image row shows its own bytes on the raw leg', () => {
    const { el } = render((h) =>
      h.entry.set(entryOf('pics/a.png', { fileType: 'blob', originalMimeType: 'image/png' })),
    );
    expect(el.querySelector('img')!.getAttribute('src')).toBe(
      '/api/v1/mount-points/store-a/files/pics/a.png?raw=1',
    );
  });

  it('select mode: a checkbox, the name toggles, the menu is disabled; a search hit shows its folder', () => {
    const { el, host } = render((h) => {
      h.selectMode.set(true);
      h.showFolder.set(true);
      h.entry.set(entryOf('Drafts/old/a.md'));
    });
    expect(el.querySelector('input[type="checkbox"]')!.getAttribute('aria-label')).toBe(
      'Select a.md',
    );
    (el.querySelector('button[title="Drafts/old/a.md"]') as HTMLButtonElement).click();
    expect(host.toggles).toBe(1);
    expect(host.acted).toEqual([]);
    expect(
      (el.querySelector('[aria-label="Actions for a.md"]') as HTMLButtonElement).disabled,
    ).toBe(true);
    expect(el.textContent).toContain('Drafts');
  });
});
