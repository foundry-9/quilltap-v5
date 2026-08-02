import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import type {
  AccessibleStoreDto,
  DocMountFileDto,
  ProjectLibraryTargetDto,
  RecentDocumentDto,
} from '../core/core-contract';
import { DocumentApi } from './document-api';
import { DocumentPicker, type DocumentSelection } from './document-picker';
import { StandaloneDocumentApi } from './standalone-wire';

class FakeApi {
  recents: RecentDocumentDto[] = [];
  stores: AccessibleStoreDto[] = [];
  mountFiles: DocMountFileDto[] = [];
  mountFolders: string[] = [];
  lookEverywhereCalls: boolean[] = [];

  projectLibrary: ProjectLibraryTargetDto | null = null;

  fetchRecentDocuments = vi.fn(async () => this.recents);
  fetchAccessibleStores = vi.fn(async (_c: string, all: boolean) => {
    this.lookEverywhereCalls.push(all);
    return { stores: this.stores, projectLibrary: this.projectLibrary };
  });
  listMountFiles = vi.fn(async () => ({ files: this.mountFiles, folders: this.mountFolders }));
}

/** The chat-less (standalone) API — the picker uses this when chatId is null. */
class FakeStandaloneApi {
  recents: RecentDocumentDto[] = [];
  stores: AccessibleStoreDto[] = [];
  mountFiles: DocMountFileDto[] = [];
  mountFolders: string[] = [];

  fetchRecent = vi.fn(async () => this.recents);
  fetchStores = vi.fn(async () => this.stores);
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
    providers: [
      { provide: DocumentApi, useValue: api },
      // Stubbed so the picker's root StandaloneDocumentApi injection never pulls
      // in the real CoreClient during a chat-mode test.
      { provide: StandaloneDocumentApi, useValue: new FakeStandaloneApi() },
    ],
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

async function renderStandalone(api: FakeStandaloneApi): Promise<{
  fixture: ComponentFixture<DocumentPicker>;
  selections: DocumentSelection[];
}> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [DocumentPicker],
    providers: [{ provide: StandaloneDocumentApi, useValue: api }],
  });
  const fixture = TestBed.createComponent(DocumentPicker);
  // chatId defaults to null → the standalone (chat-less) surface.
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

  /**
   * Dogfood #50. The server withholds the chat project's OWN store from the
   * accessible-store list — in look-everywhere mode too — because v4 surfaces
   * it as this button. Without the button that store is reachable by no path,
   * which is what a real instance showed: every other project's store listed,
   * and only the conversation's own project missing.
   */
  describe('the project library button', () => {
    const library: ProjectLibraryTargetDto = {
      mountPointId: 'pm1',
      name: 'Project Files: The Estate',
      mountType: 'database',
    };

    it('renders the project store the accessible-store list never carries', async () => {
      const api = new FakeApi();
      api.projectLibrary = library;
      // The store list is what a real project chat returns: every store EXCEPT
      // the chat's own project.
      api.stores = [store({ mountPointId: 'other', name: 'Project Files: Church' })];
      const { fixture } = await render(api);
      const text = fixture.nativeElement.textContent as string;
      expect(text).toContain('Project library');
      expect(text).toContain('Project Files: The Estate');
    });

    it('browses the project mount — the path that did not exist', async () => {
      const api = new FakeApi();
      api.projectLibrary = library;
      api.mountFiles = [{ id: 'f1', relativePath: 'estate/notes.md' }];
      const { fixture } = await render(api);

      (
        fixture.nativeElement.querySelector('.qt-doc-project-library') as HTMLButtonElement
      ).click();
      await flush();
      fixture.detectChanges();

      expect(api.listMountFiles).toHaveBeenCalledWith('pm1');
      const text = fixture.nativeElement.textContent as string;
      expect(text).toContain('Project Files: The Estate');
      expect(text).toContain('estate');
    });

    it('survives a look-everywhere toggle', async () => {
      const api = new FakeApi();
      api.projectLibrary = library;
      const { fixture } = await render(api);
      const checkbox = fixture.nativeElement.querySelector(
        'input[type="checkbox"]',
      ) as HTMLInputElement;
      checkbox.checked = true;
      checkbox.dispatchEvent(new Event('change'));
      await flush();
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('.qt-doc-project-library')).not.toBeNull();
    });

    it('stays absent when the chat has no project library', async () => {
      const api = new FakeApi();
      api.stores = [store({ mountPointId: 'd1', name: 'Lib' })];
      const { fixture } = await render(api);
      expect(fixture.nativeElement.querySelector('.qt-doc-project-library')).toBeNull();
      expect(fixture.nativeElement.textContent).not.toContain('Project library');
    });

    it('stays absent on the standalone surface', async () => {
      const api = new FakeStandaloneApi();
      api.stores = [store({ mountPointId: 'd1', name: 'Lib' })];
      const { fixture } = await renderStandalone(api);
      expect(fixture.nativeElement.querySelector('.qt-doc-project-library')).toBeNull();
    });
  });

  describe('standalone (chat-less) surface', () => {
    it('lists recents and stores from the standalone API', async () => {
      const api = new FakeStandaloneApi();
      api.recents = [
        { id: 'r1', filePath: 'x.md', scope: 'general', displayTitle: 'Ex' },
      ];
      api.stores = [
        store({ mountPointId: 'd1', label: 'Everything', storeType: 'documents', mountType: 'database' }),
      ];
      const { fixture } = await renderStandalone(api);
      expect(api.fetchRecent).toHaveBeenCalled();
      expect(api.fetchStores).toHaveBeenCalled();
      const text = fixture.nativeElement.textContent as string;
      expect(text).toContain('New blank document');
      expect(text).toContain('Ex');
      expect(text).toContain('Everything');
    });

    it('hides the look-everywhere checkbox (always everywhere)', async () => {
      const { fixture } = await renderStandalone(new FakeStandaloneApi());
      expect(fixture.nativeElement.querySelector('input[type="checkbox"]')).toBeNull();
    });

    it('browsing a store lists its files via the standalone mount listing', async () => {
      const api = new FakeStandaloneApi();
      api.stores = [store({ mountPointId: 'd1', name: 'Lib', label: 'Library' })];
      api.mountFiles = [{ id: 'f1', relativePath: 'top.md' }];
      const { fixture, selections } = await renderStandalone(api);
      const storeBtn = [...fixture.nativeElement.querySelectorAll('button')].find((b: HTMLElement) =>
        b.textContent?.trim().startsWith('Library'),
      ) as HTMLButtonElement;
      storeBtn.click();
      await flush();
      fixture.detectChanges();
      expect(api.listMountFiles).toHaveBeenCalledWith('d1');

      const fileBtn = [...fixture.nativeElement.querySelectorAll('button')].find((b: HTMLElement) =>
        b.textContent?.includes('top.md'),
      ) as HTMLButtonElement;
      fileBtn.click();
      expect(selections).toEqual([{ filePath: 'top.md', scope: 'document_store', mountPoint: 'Lib' }]);
    });
  });
});
