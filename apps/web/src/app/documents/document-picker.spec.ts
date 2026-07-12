import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import type { AccessibleStoreDto, DocMountFileDto, RecentDocumentDto } from '../core/core-contract';
import { DocumentApi } from './document-api';
import { DocumentPicker, type DocumentSelection } from './document-picker';

class FakeApi {
  recents: RecentDocumentDto[] = [];
  stores: AccessibleStoreDto[] = [];
  mountFiles: DocMountFileDto[] = [];
  mountFolders: string[] = [];
  lookEverywhereCalls: boolean[] = [];

  fetchRecentDocuments = vi.fn(async () => this.recents);
  fetchAccessibleStores = vi.fn(async (_c: string, all: boolean) => {
    this.lookEverywhereCalls.push(all);
    return { stores: this.stores, projectLibrary: null };
  });
  listMountFiles = vi.fn(async () => ({ files: this.mountFiles, folders: this.mountFolders }));
}

function store(over: Partial<AccessibleStoreDto>): AccessibleStoreDto {
  return {
    mountPointId: over.mountPointId ?? 'm1',
    name: over.name ?? 'Store',
    label: over.label ?? 'Store',
    kind: over.kind ?? 'document-store',
    mountType: over.mountType ?? 'database',
    storeType: over.storeType ?? 'documents',
    characterId: over.characterId,
    isGroupStore: over.isGroupStore,
  };
}

async function render(api: FakeApi): Promise<{
  fixture: ComponentFixture<DocumentPicker>;
  selections: DocumentSelection[];
}> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [DocumentPicker],
    providers: [{ provide: DocumentApi, useValue: api }],
  });
  const fixture = TestBed.createComponent(DocumentPicker);
  fixture.componentRef.setInput('chatId', 'c1');
  const selections: DocumentSelection[] = [];
  fixture.componentInstance.selectDocument.subscribe((s) => selections.push(s));
  fixture.detectChanges();
  await flush();
  fixture.detectChanges();
  return { fixture, selections };
}

/** Drain the ngOnInit fetch microtasks (zoneless whenStable doesn't await them). */
async function flush(): Promise<void> {
  for (let i = 0; i < 8; i++) await Promise.resolve();
}

describe('DocumentPicker', () => {
  it('lists recents and store buckets on open', async () => {
    const api = new FakeApi();
    api.recents = [
      { id: 'r1', filePath: 'a.md', scope: 'project', displayTitle: 'Alpha', isActive: true },
    ];
    api.stores = [
      store({ mountPointId: 'v1', label: 'Ada Vault', storeType: 'character', kind: 'character' }),
      store({ mountPointId: 'd1', label: 'Library', storeType: 'documents', mountType: 'database' }),
    ];
    const { fixture } = await render(api);
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('New blank document');
    expect(text).toContain('Alpha');
    expect(text).toContain('Continue editing · Project');
    expect(text).toContain('Ada Vault');
    expect(text).toContain('Library');
  });

  it('New blank document emits an empty selection', async () => {
    const api = new FakeApi();
    const { fixture, selections } = await render(api);
    (fixture.nativeElement.querySelector('.qt-doc-open-button') as HTMLButtonElement).click();
    expect(selections).toEqual([{}]);
  });

  it('a recent row emits its path/scope/mountPoint', async () => {
    const api = new FakeApi();
    api.recents = [
      { id: 'r1', filePath: 'notes/x.md', scope: 'document_store', mountPoint: 'Lib', displayTitle: 'X' },
    ];
    const { fixture, selections } = await render(api);
    const recentBtn = [...fixture.nativeElement.querySelectorAll('button')].find((b: HTMLElement) =>
      b.textContent?.includes('X'),
    ) as HTMLButtonElement;
    recentBtn.click();
    expect(selections).toEqual([{ filePath: 'notes/x.md', scope: 'document_store', mountPoint: 'Lib' }]);
  });

  it('browsing a store lists its files + derived folders and opens one', async () => {
    const api = new FakeApi();
    api.stores = [store({ mountPointId: 'd1', name: 'Lib', label: 'Library' })];
    api.mountFiles = [
      { id: 'f1', relativePath: 'top.md' },
      { id: 'f2', relativePath: 'sub/deep.md' },
    ];
    const { fixture, selections } = await render(api);

    // drill into the store
    const storeBtn = [...fixture.nativeElement.querySelectorAll('button')].find((b: HTMLElement) =>
      b.textContent?.trim().startsWith('Library'),
    ) as HTMLButtonElement;
    storeBtn.click();
    await flush();
    fixture.detectChanges();

    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('New document here');
    expect(text).toContain('sub'); // derived folder
    expect(text).toContain('top.md'); // file at root

    const fileBtn = [...fixture.nativeElement.querySelectorAll('button')].find((b: HTMLElement) =>
      b.textContent?.includes('top.md'),
    ) as HTMLButtonElement;
    fileBtn.click();
    expect(selections).toEqual([{ filePath: 'top.md', scope: 'document_store', mountPoint: 'Lib' }]);
  });

  it('"New document here" emits a targetFolder creation for the store', async () => {
    const api = new FakeApi();
    api.stores = [store({ mountPointId: 'd1', name: 'Lib', label: 'Library' })];
    const { fixture, selections } = await render(api);
    const storeBtn = [...fixture.nativeElement.querySelectorAll('button')].find((b: HTMLElement) =>
      b.textContent?.trim().startsWith('Library'),
    ) as HTMLButtonElement;
    storeBtn.click();
    await flush();
    fixture.detectChanges();

    const newHere = [...fixture.nativeElement.querySelectorAll('button')].find((b: HTMLElement) =>
      b.textContent?.includes('New document here'),
    ) as HTMLButtonElement;
    newHere.click();
    expect(selections).toEqual([{ scope: 'document_store', mountPoint: 'Lib', targetFolder: undefined }]);
  });

  it('look-everywhere refetches stores with all=true', async () => {
    const api = new FakeApi();
    const { fixture } = await render(api);
    const checkbox = fixture.nativeElement.querySelector(
      'input[type="checkbox"]',
    ) as HTMLInputElement;
    checkbox.checked = true;
    checkbox.dispatchEvent(new Event('change'));
    await flush();
    expect(api.lookEverywhereCalls).toEqual([false, true]);
  });
});
