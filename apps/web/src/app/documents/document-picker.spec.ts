import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { describe, expect, it, vi } from 'vitest';

import type { AccessibleStoreDto, ProjectLibraryTargetDto, RecentDocumentDto } from '../core/core-contract';
import { file, provideDocumentStoreTesting, settle, store as storeRecord } from '../document-browser/document-browser.testing';
import { ToastService } from '../ui/toast.service';
import { DocumentApi } from './document-api';
import { DocumentPicker, type DocumentSelection } from './document-picker';
import { StandaloneDocumentApi } from './standalone-wire';

class FakeApi {
  recents: RecentDocumentDto[] = [];
  stores: AccessibleStoreDto[] = [];
  /** The browsed store's listing, served to the store browser over dispatch. */
  mountFiles: string[] = [];
  mountFolders: string[] = [];
  lookEverywhereCalls: boolean[] = [];

  projectLibrary: ProjectLibraryTargetDto | null = null;
  generalLibrary: ProjectLibraryTargetDto | null = null;

  fetchRecentDocuments = vi.fn(async () => this.recents);
  fetchAccessibleStores = vi.fn(async (_c: string, all: boolean) => {
    this.lookEverywhereCalls.push(all);
    return { stores: this.stores, projectLibrary: this.projectLibrary, generalLibrary: this.generalLibrary };
  });
}

/** The chat-less (standalone) API — the picker uses this when chatId is null. */
class FakeStandaloneApi {
  recents: RecentDocumentDto[] = [];
  stores: AccessibleStoreDto[] = [];
  generalLibrary: ProjectLibraryTargetDto | null = null;
  mountFiles: string[] = [];
  mountFolders: string[] = [];

  fetchRecent = vi.fn(async () => this.recents);
  fetchStores = vi.fn(async () => ({ stores: this.stores, generalLibrary: this.generalLibrary }));
}

/**
 * The store browser the browse step mounts (P4.D387) reads over dispatch: a
 * `mountPointGet` record (writable), the `mountFilesList` listing from the
 * fake's `mountFiles` / `mountFolders`, an empty catalogue.
 */
const toasts = { showSuccess: vi.fn(), showError: vi.fn(), showInfo: vi.fn(), showWarning: vi.fn() };
function provideBrowser(source: { mountFiles: string[]; mountFolders: string[] }) {
  return provideDocumentStoreTesting(
    (req) => {
      if (req.type === 'mountPointGet') return { mountPoint: storeRecord(String(req['mountPointId']), 'Lib') };
      if (req.type === 'mountFilesList')
        return { files: source.mountFiles.map((p) => file(p)), folders: source.mountFolders };
      if (req.type === 'mountPointList') return { mountPoints: [] };
      return {};
    },
    [provideRouter([]), { provide: ToastService, useValue: toasts }],
  );
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
  provideBrowser(api);
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
  provideBrowser(api);
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

function buttonWith(fixture: ComponentFixture<DocumentPicker>, text: string): HTMLButtonElement {
  return [...fixture.nativeElement.querySelectorAll('button')].find(
    (b: HTMLElement) => b.textContent?.trim() === text || b.textContent?.includes(text),
  ) as HTMLButtonElement;
}

function clickStore(fixture: ComponentFixture<DocumentPicker>, label: string): void {
  ([...fixture.nativeElement.querySelectorAll('button')].find((b: HTMLElement) =>
    b.textContent?.trim().startsWith(label),
  ) as HTMLButtonElement).click();
}

/** Drain the ngOnInit fetch microtasks (zoneless whenStable doesn't await them). */
async function flush(): Promise<void> {
  for (let i = 0; i < 8; i++) await Promise.resolve();
}

describe('DocumentPicker', () => {
  it('the General library button when generalLibrary is set (v4 5efe53ea7 :320-334); absent otherwise', async () => {
    const api = new FakeApi();
    api.generalLibrary = { mountPointId: 'gen', name: 'Quilltap General', mountType: 'database' };
    const { fixture } = await render(api);
    const button = fixture.nativeElement.querySelector('.qt-doc-general-library') as HTMLButtonElement;
    expect(button.textContent).toContain('General library');
    expect(button.textContent).toContain('Quilltap General, the shelf every chat shares');
    expect(button.querySelector('[data-icon]')!.getAttribute('data-icon')).toBe('scriptorium');
    button.click();
    await settle(fixture);
    expect(fixture.nativeElement.textContent).toContain('General Library');
    expect(fixture.nativeElement.querySelector('qt-document-store-browser')).not.toBeNull();

    const none = await render(new FakeApi());
    expect(none.fixture.nativeElement.querySelector('.qt-doc-general-library')).toBeNull();
  });

  it('the source step is 4xl and the browse step lg (v4 widths, ruling L-6)', async () => {
    const api = new FakeApi();
    api.stores = [store({ mountPointId: 'd1', name: 'Lib', label: 'Library' })];
    const { fixture } = await render(api);
    const width = () => (fixture.nativeElement.querySelector('.qt-dialog') as HTMLElement).style.maxWidth;
    expect(width()).toBe('56rem');
    clickStore(fixture, 'Library');
    await settle(fixture);
    expect(width()).toBe('32rem');
  });

  it("an unreachable store in Go there toasts v4's line; a store on the shelves is browsed", async () => {
    const api = new FakeApi();
    api.stores = [store({ mountPointId: 'd1', name: 'Lib', label: 'Library' }), store({ mountPointId: 'd2', name: 'Other', label: 'Other' })];
    const { fixture } = await render(api);
    clickStore(fixture, 'Library');
    await settle(fixture);
    const picker = fixture.componentInstance as unknown as { openOtherStore(id: string): void };
    toasts.showError.mockClear();
    picker.openOtherStore('elsewhere');
    expect(toasts.showError).toHaveBeenCalledWith("That store is not on this chat's shelves. Tick Look everywhere to reach it.");
    picker.openOtherStore('d2');
    await settle(fixture);
    expect(fixture.nativeElement.textContent).toContain('Other');
  });

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

  it('browsing a store mounts the store browser in pick mode; a picked file opens as document_store', async () => {
    const api = new FakeApi();
    api.stores = [store({ mountPointId: 'd1', name: 'Lib', label: 'Library' })];
    api.mountFiles = ['top.md', 'sub/deep.md'];
    api.mountFolders = ['sub'];
    const { fixture, selections } = await render(api);

    clickStore(fixture, 'Library');
    await settle(fixture);

    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('New document here');
    expect(text).toContain('sub'); // a folder row
    expect(text).toContain('top.md'); // a file at the top

    buttonWith(fixture, 'top.md').click();
    expect(selections).toEqual([{ filePath: 'top.md', scope: 'document_store', mountPoint: 'Lib' }]);
  });

  it('"New document here" in the footer carries the folder being browsed', async () => {
    const api = new FakeApi();
    api.stores = [store({ mountPointId: 'd1', name: 'Lib', label: 'Library' })];
    api.mountFolders = ['Drafts'];
    const { fixture, selections } = await render(api);
    clickStore(fixture, 'Library');
    await settle(fixture);

    buttonWith(fixture, 'New document here').click();
    expect(selections).toEqual([{ scope: 'document_store', mountPoint: 'Lib', targetFolder: undefined }]);

    buttonWith(fixture, 'Drafts').click();
    await settle(fixture);
    buttonWith(fixture, 'New document here').click();
    expect(selections[1]).toEqual({ scope: 'document_store', mountPoint: 'Lib', targetFolder: 'Drafts' });
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

    it('browses the project library as a STORE (v4 5efe53ea7 dropped the project-scope fallback)', async () => {
      const api = new FakeApi();
      api.projectLibrary = library;
      api.mountFiles = ['estate/notes.md'];
      api.mountFolders = ['estate'];
      const { fixture, selections } = await render(api);

      (fixture.nativeElement.querySelector('.qt-doc-project-library') as HTMLButtonElement).click();
      await settle(fixture);

      const text = fixture.nativeElement.textContent as string;
      // The browse header names the project (v4 `projectName || 'Project'`).
      expect(text).toContain('Project');
      expect(text).toContain('estate');
      buttonWith(fixture, 'estate').click();
      await settle(fixture);
      buttonWith(fixture, 'notes.md').click();
      expect(selections).toEqual([
        { filePath: 'estate/notes.md', scope: 'document_store', mountPoint: 'Project Files: The Estate' },
      ]);
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

    it('browsing a store mounts the store browser; a picked file opens as document_store', async () => {
      const api = new FakeStandaloneApi();
      api.stores = [store({ mountPointId: 'd1', name: 'Lib', label: 'Library' })];
      api.mountFiles = ['top.md'];
      const { fixture, selections } = await renderStandalone(api);
      clickStore(fixture, 'Library');
      await settle(fixture);
      buttonWith(fixture, 'top.md').click();
      expect(selections).toEqual([{ filePath: 'top.md', scope: 'document_store', mountPoint: 'Lib' }]);
    });

    it('shows the General library from the standalone stores body (C14)', async () => {
      const api = new FakeStandaloneApi();
      api.generalLibrary = { mountPointId: 'gen', name: 'Quilltap General', mountType: 'database' };
      const { fixture } = await renderStandalone(api);
      expect(fixture.nativeElement.querySelector('.qt-doc-general-library')).not.toBeNull();
    });
  });
});
