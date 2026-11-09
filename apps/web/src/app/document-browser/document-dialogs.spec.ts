import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { DocMountLinkLocation } from '../core/core-contract';
import type { DestinationMode } from '../item-browser/destination-picker-dialog';
import type { BrowserStore } from './document-actions';
import {
  CAPS,
  provideDocumentStoreTesting,
  settle,
  store,
  type Route,
} from './document-browser.testing';
import { DocumentDescriptionDialog, type DescribedFile } from './document-description-dialog';
import { DocumentDestinationDialog, type DocumentDestination } from './document-destination-dialog';
import { DocumentLinksDialog } from './document-links-dialog';

/**
 * v4 `DocumentFileDialogs.tsx` + `DocumentDestinationDialog.tsx`
 * (`c34986421`, pin `624d0d25c`; P4.D387 items 10–11): the links dialog's
 * notes, badges and `Go there` (close, then reveal), read only while a path is
 * set; the description form seeded per file with Save disabled while
 * unchanged; the destination dialog's eligible stores per mode, its default
 * destination, the select bytes and a cross-store submission.
 */

const loc = (fields: Partial<DocMountLinkLocation>): DocMountLinkLocation => ({
  linkId: 'l',
  mountPointId: 'store-a',
  mountPointName: 'Archive',
  relativePath: 'notes.md',
  grouped: false,
  self: false,
  ...fields,
});

@Component({
  imports: [DocumentLinksDialog],
  template: `<qt-document-links-dialog
    storeId="store-a"
    [path]="path()"
    (closed)="events.push('closed')"
    (reveal)="events.push('reveal:' + $event.mountPointId)"
  />`,
})
class LinksHost {
  readonly path = signal<string | null>(null);
  readonly events: string[] = [];
}

describe('DocumentLinksDialog (v4 DocumentFileDialogs.tsx)', () => {
  afterEach(() => {
    TestBed.resetTestingModule();
    document.body.innerHTML = '';
  });

  async function render(route: Route) {
    const { seen } = provideDocumentStoreTesting(route);
    TestBed.configureTestingModule({ imports: [LinksHost] });
    const fixture = TestBed.createComponent(LinksHost);
    fixture.detectChanges();
    await settle(fixture);
    return { fixture, seen, el: fixture.nativeElement as HTMLElement };
  }

  it('reads nothing while no path is asked about; a grouped set gets the hard-link note, badges and Go there', async () => {
    const { fixture, seen, el } = await render((req) =>
      req.type === 'mountFileLinks'
        ? {
            linkGroupId: 'g',
            locations: [
              loc({ linkId: 'a', self: true, grouped: true }),
              loc({
                linkId: 'b',
                mountPointId: 'store-b',
                mountPointName: 'Second Shelf',
                relativePath: 'In/notes.md',
                grouped: true,
              }),
              loc({ linkId: 'c', relativePath: 'copy.md' }),
            ],
          }
        : {},
    );
    expect(seen).toEqual([]);
    expect(el.querySelector('qt-modal')).toBeNull();
    fixture.componentInstance.path.set('notes.md');
    await settle(fixture);
    expect(seen).toEqual([{ type: 'mountFileLinks', mountPointId: 'store-a', path: 'notes.md' }]);
    expect(el.querySelector('.qt-dialog-title')!.textContent).toBe('Where else this appears');
    expect(el.textContent).toContain(
      'Hard-linked copies are one file: an edit through any of them reaches all. The others merely share identical contents and part ways at the next edit.',
    );
    const items = [...el.querySelectorAll('[data-testid="document-links-list"] li')];
    expect(
      items.map((li) => [...li.querySelectorAll('span')].map((s) => s.textContent!.trim())),
    ).toEqual([['This one'], ['Hard-linked'], []]);
    (items[1].querySelector('button') as HTMLButtonElement).click();
    expect(fixture.componentInstance.events).toEqual(['closed', 'reveal:store-b']);
  });

  it('an ungrouped set gets the identical-contents note', async () => {
    const { fixture, el } = await render(() => ({
      linkGroupId: null,
      locations: [loc({ self: true })],
    }));
    fixture.componentInstance.path.set('notes.md');
    await settle(fixture);
    expect(el.textContent).toContain(
      'These share identical contents only; each goes its own way at its next edit.',
    );
  });
});

@Component({
  imports: [DocumentDescriptionDialog],
  template: `<qt-document-description-dialog
    [file]="file()"
    [saveDescription]="save"
    (closed)="closes = closes + 1"
  />`,
})
class DescHost {
  readonly file = signal<DescribedFile | null>({
    relativePath: 'a.pdf',
    fileName: 'a.pdf',
    description: 'old',
  });
  closes = 0;
  readonly save = vi.fn(async () => true);
}

describe('DocumentDescriptionDialog (v4 DocumentFileDialogs.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('seeds from the file, keeps Save disabled while unchanged, saves and closes', async () => {
    TestBed.configureTestingModule({ imports: [DescHost] });
    const fixture = TestBed.createComponent(DescHost);
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;
    expect(el.querySelector('.qt-dialog-title')!.textContent).toBe('Describe a.pdf');
    expect(el.querySelector('label[for="document-description"]')!.textContent!.trim()).toBe(
      'Description or transcript (searchable)',
    );
    const area = el.querySelector('#document-description') as HTMLTextAreaElement;
    expect(area.value).toBe('old');
    const save = [...el.querySelectorAll('button')].find(
      (b) => b.textContent!.trim() === 'Save',
    ) as HTMLButtonElement;
    expect(save.disabled).toBe(true);
    area.value = 'new';
    area.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    expect(save.disabled).toBe(false);
    save.click();
    await settle(fixture);
    expect(fixture.componentInstance.save).toHaveBeenCalledWith('new');
    expect(fixture.componentInstance.closes).toBe(1);
  });

  it('a different file reseeds the draft (no stale text)', () => {
    TestBed.configureTestingModule({ imports: [DescHost] });
    const fixture = TestBed.createComponent(DescHost);
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;
    const area = () => el.querySelector('#document-description') as HTMLTextAreaElement;
    area().value = 'typed';
    area().dispatchEvent(new Event('input'));
    fixture.componentInstance.file.set({
      relativePath: 'b.pdf',
      fileName: 'b.pdf',
      description: 'bee',
    });
    fixture.detectChanges();
    expect(area().value).toBe('bee');
  });
});

const FS = { ...store('fs-1', 'Notebook'), mountType: 'filesystem' };
const BUSY = { ...store('busy', 'Busy'), conversionStatus: 'converting' };
const OFF = { ...store('off', 'Off'), enabled: false };
const CATALOG: Route = (req) => {
  if (req.type === 'mountPointList')
    return {
      mountPoints: [store('store-b', 'second Shelf'), store('store-a', 'Archive'), FS, BUSY, OFF],
    };
  if (req.type === 'mountFilesList') return { files: [], folders: ['b', 'A'] };
  return {};
};

@Component({
  imports: [DocumentDestinationDialog],
  template: `<qt-document-destination-dialog
    [mode]="mode()"
    [source]="source()"
    subject='"notes.md"'
    currentFolder=""
    (submitted)="sent.push($event)"
  />`,
})
class DestHost {
  readonly mode = signal<DestinationMode>('move');
  readonly source = signal<BrowserStore>({
    id: 'store-a',
    mountType: 'database',
    capabilities: CAPS,
  });
  readonly sent: DocumentDestination[] = [];
}

describe('DocumentDestinationDialog (v4 DocumentDestinationDialog.tsx)', () => {
  afterEach(() => {
    TestBed.resetTestingModule();
    document.body.innerHTML = '';
  });

  async function render(mode: DestinationMode) {
    const { seen } = provideDocumentStoreTesting(CATALOG);
    TestBed.configureTestingModule({ imports: [DestHost] });
    const fixture = TestBed.createComponent(DestHost);
    fixture.componentInstance.mode.set(mode);
    document.body.appendChild(fixture.nativeElement);
    fixture.detectChanges();
    await settle(fixture);
    const el = fixture.nativeElement as HTMLElement;
    const select = el.querySelector('#document-destination-store') as HTMLSelectElement;
    return { fixture, seen, el, select };
  }
  const optionsOf = (s: HTMLSelectElement) => [...s.options].map((o) => o.textContent!.trim());

  it('move: every enabled store that can take it in, sorted, this store marked and chosen by default', async () => {
    const { select, el } = await render('move');
    expect(optionsOf(select)).toEqual(['Archive (this store)', 'Notebook', 'second Shelf']);
    expect(select.value).toBe('store-a');
    expect(el.querySelector('label[for="document-destination-store"]')!.textContent!.trim()).toBe(
      'Store',
    );
    expect(el.textContent).toContain(
      'Choose a store and a folder. A file of the same name already there stops the move.',
    );
    expect(el.textContent).toContain('Top of the store');
  });

  it('link: only stores canLinkBetween allows (database ↔ database)', async () => {
    const { select, el } = await render('link');
    expect(optionsOf(select)).toEqual(['Archive (this store)', 'second Shelf']);
    expect(el.textContent).toContain(
      'A hard link is the same file in two places: an edit through either reaches both.',
    );
  });

  it('a cross-store pick submits {mountPointId, folder} and reads that store listing', async () => {
    const { fixture, seen, el, select } = await render('copy');
    expect(el.textContent).toContain(
      'Choose a store and a folder. The copy is independent of the original.',
    );
    select.value = 'store-b';
    select.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(seen).toContainEqual({ type: 'mountFilesList', mountPointId: 'store-b' });
    const options = [...el.querySelectorAll('[role="option"]')];
    expect(options.map((o) => o.textContent!.trim())).toEqual(['Top of the store', 'A', 'b']);
    (options[1] as HTMLElement).click();
    fixture.detectChanges();
    (
      [...el.querySelectorAll('button')].find(
        (b) => b.textContent!.trim() === 'Copy',
      ) as HTMLButtonElement
    ).click();
    expect(fixture.componentInstance.sent).toEqual([{ mountPointId: 'store-b', folder: 'A' }]);
  });
});
