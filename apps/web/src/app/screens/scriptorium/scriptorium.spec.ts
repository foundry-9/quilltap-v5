import { signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type { DocMountPointDto } from '../../core/core-contract';
import { WORKSPACE_TAB_ID, WORKSPACE_TAB_VISIBLE } from '../../workspace/workspace-contract';
import { CreateStoreDialog } from './create-store-dialog';
import { DirectoryPicker } from './directory-picker';
import { FileTable } from './file-table';
import { ScriptoriumList } from './scriptorium-list';
import { ScriptoriumStore } from './scriptorium.store';
import type { DocumentStoreFile } from './scriptorium.api';
import { ToastService } from '../../ui/toast.service';

interface DispatchReq {
  type: string;
  [k: string]: unknown;
}

function stubClient(handler: (req: DispatchReq) => unknown): Partial<CoreClient> {
  return {
    dispatchData: (async (req: DispatchReq) => {
      const out = handler(req);
      if (out instanceof Error) {
        throw out;
      }
      return (out ?? {}) as Record<string, unknown>;
    }) as CoreClient['dispatchData'],
  };
}

async function settle(fixture: ComponentFixture<unknown>, ticks = 8): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

function store(over: Partial<DocMountPointDto> = {}): DocMountPointDto {
  return {
    id: 'm1',
    userId: 'u1',
    name: 'My Vault',
    basePath: '/docs/vault',
    mountType: 'filesystem',
    storeType: 'documents',
    includePatterns: ['*.md'],
    excludePatterns: ['.git'],
    enabled: true,
    lastScannedAt: null,
    scanStatus: 'idle',
    lastScanError: null,
    conversionStatus: 'idle',
    conversionError: null,
    fileCount: 0,
    chunkCount: 0,
    totalSizeBytes: 0,
    createdAt: '2024-01-01T00:00:00Z',
    updatedAt: '2024-01-01T00:00:00Z',
    embeddedChunkCount: 0,
    ...over,
  };
}

function file(over: Partial<DocumentStoreFile> = {}): DocumentStoreFile {
  return {
    id: 'f1',
    mountPointId: 'm1',
    relativePath: 'notes/intro.md',
    fileName: 'intro.md',
    fileType: 'markdown',
    sha256: 'abc',
    fileSizeBytes: 1024,
    lastModified: '2024-01-01T00:00:00Z',
    conversionStatus: 'converted',
    conversionError: null,
    plainTextLength: 100,
    chunkCount: 3,
    createdAt: '2024-01-01T00:00:00Z',
    updatedAt: '2024-01-01T00:00:00Z',
    ...over,
  };
}

// ---------------------------------------------------------------------------
// ScriptoriumStore — the patch-not-refetch shape + loud refusal surfacing.
// ---------------------------------------------------------------------------
/** The toast stack this render raised, newest last. */
function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

describe('ScriptoriumStore', () => {
  function make(client: Partial<CoreClient>): ScriptoriumStore {
    TestBed.configureTestingModule({
      providers: [ScriptoriumStore, { provide: CoreClient, useValue: client }],
    });
    return TestBed.inject(ScriptoriumStore);
  }

  it('create PREPENDS and flashes success (or the warning as an error)', async () => {
    const s = make(
      stubClient((r) => {
        if (r.type === 'mountPointList') return { mountPoints: [store({ id: 'a', name: 'A' })] };
        if (r.type === 'mountPointCreate') return { mountPoint: store({ id: 'b', name: 'B' }) };
        return {};
      }),
    );
    await s.fetchStores();
    const created = await s.createStore({ name: 'B', mountType: 'database' });
    expect(created?.id).toBe('b');
    expect(s.stores().map((x) => x.id)).toEqual(['b', 'a']); // prepended
    expect(toasts().at(-1)).toEqual({
      type: 'success',
      message: 'Document store created successfully!',
    });
  });

  it('create surfaces the base-path-availability warning as an error flash', async () => {
    const s = make(
      stubClient((r) =>
        r.type === 'mountPointCreate'
          ? { mountPoint: store({ id: 'b' }), warning: 'Base path not accessible.' }
          : {},
      ),
    );
    await s.createStore({ name: 'B', mountType: 'filesystem', basePath: '/nope' });
    expect(toasts().at(-1)).toEqual({ type: 'error', message: 'Base path not accessible.' });
  });

  it('scan optimistically marks scanning, then re-GETs the ONE store and splices it', async () => {
    const seen: string[] = [];
    const s = make(
      stubClient((r) => {
        seen.push(r.type);
        if (r.type === 'mountPointList') return { mountPoints: [store({ id: 'a', fileCount: 0 })] };
        if (r.type === 'mountScan')
          return { scanResult: { filesScanned: 3 }, embeddingJobsEnqueued: 2 };
        if (r.type === 'mountPointGet') return { mountPoint: store({ id: 'a', fileCount: 3 }) };
        return {};
      }),
    );
    await s.fetchStores();
    await s.scanStore('a');
    expect(s.stores()[0].fileCount).toBe(3); // spliced from the re-GET, not a full refetch
    expect(seen).toContain('mountScan');
    expect(seen).toContain('mountPointGet');
    // Only the ONE initial list fetch — scan does NOT trigger a full-list refetch.
    expect(seen.filter((t) => t === 'mountPointList')).toHaveLength(1);
  });

  it('convert surfaces the live typed refusal loudly (error flash) and marks error', async () => {
    const s = make(
      stubClient((r) => {
        if (r.type === 'mountPointList') return { mountPoints: [store({ id: 'a' })] };
        if (r.type === 'mountConvert') return new Error('Conversion is not available.');
        return {};
      }),
    );
    await s.fetchStores();
    const ok = await s.convertStore('a');
    expect(ok).toBe(false);
    expect(toasts().at(-1)?.type).toBe('error');
    expect(toasts().at(-1)?.message).toContain('not available');
    expect(s.stores()[0].conversionStatus).toBe('error');
  });
});

// ---------------------------------------------------------------------------
// ScriptoriumList — empty state + the grid.
// ---------------------------------------------------------------------------
describe('ScriptoriumList', () => {
  async function render(client: Partial<CoreClient>): Promise<ComponentFixture<ScriptoriumList>> {
    TestBed.configureTestingModule({
      imports: [ScriptoriumList],
      providers: [provideRouter([]), { provide: CoreClient, useValue: client }],
    });
    const fixture = TestBed.createComponent(ScriptoriumList);
    fixture.detectChanges();
    await settle(fixture);
    return fixture;
  }

  it('shows the empty state when there are no stores', async () => {
    const fixture = await render(
      stubClient((r) => (r.type === 'mountPointList' ? { mountPoints: [] } : {})),
    );
    expect(fixture.nativeElement.textContent).toContain('No document stores yet');
  });

  it('renders a card with the store name and stats', async () => {
    const fixture = await render(
      stubClient((r) =>
        r.type === 'mountPointList' ? { mountPoints: [store({ name: 'Grimoire' })] } : {},
      ),
    );
    expect(fixture.nativeElement.textContent).toContain('Grimoire');
    expect(fixture.nativeElement.textContent).toContain('The Scriptorium');
  });
});

/**
 * In-tab drill (p4.9j2, v4 `ScriptoriumView` `selectedStoreId`): hosted as a
 * workspace tab, a card's Open drills IN PLACE (renders the store detail
 * embedded); the detail's back restores the list.
 */
describe('ScriptoriumList (in-tab drill)', () => {
  function handler(r: DispatchReq): unknown {
    switch (r.type) {
      case 'mountPointList':
        return { mountPoints: [store({ id: 'm1', name: 'Grimoire' })] };
      case 'mountPointGet':
        return { mountPoint: store({ id: 'm1', name: 'Grimoire' }) };
      case 'mountFilesList':
        return { files: [] };
      default:
        return {};
    }
  }

  async function render(): Promise<ComponentFixture<ScriptoriumList>> {
    TestBed.configureTestingModule({
      imports: [ScriptoriumList],
      providers: [
        provideRouter([]),
        { provide: CoreClient, useValue: stubClient(handler) },
        { provide: WORKSPACE_TAB_ID, useValue: 'tab-s' },
      ],
    });
    const fixture = TestBed.createComponent(ScriptoriumList);
    fixture.detectChanges();
    await settle(fixture);
    return fixture;
  }

  it('a card click drills in place and the detail back restores the list', async () => {
    const fixture = await render();
    // Click the card body (not an action button) → open emits → drill.
    const card = fixture.nativeElement.querySelector('.qt-entity-card') as HTMLElement;
    card.click();
    await settle(fixture);
    expect(fixture.nativeElement.querySelector('qt-store-detail')).toBeTruthy();
    expect(fixture.nativeElement.textContent).toContain('Indexed Files');

    // The detail's back button restores the list.
    const back = [...fixture.nativeElement.querySelectorAll('button')].find(
      (b: HTMLButtonElement) => b.textContent?.includes('Back to The Scriptorium'),
    ) as HTMLButtonElement;
    expect(back).toBeTruthy();
    back.click();
    await settle(fixture);
    expect(fixture.nativeElement.querySelector('qt-store-detail')).toBeNull();
    expect(fixture.nativeElement.textContent).toContain('Add Document Store');
  });
});

// ---------------------------------------------------------------------------
// CreateStoreDialog — database hides the path + pattern fields.
// ---------------------------------------------------------------------------
describe('Scriptorium tab re-activation (P4.D90, v4 ScriptoriumView.tsx:67-72 + DocumentStoreDetailView.tsx:63-70)', () => {
  function handler(r: DispatchReq): unknown {
    switch (r.type) {
      case 'mountPointList':
        return { mountPoints: [store({ id: 'm1', name: 'Grimoire' })] };
      case 'mountPointGet':
        return { mountPoint: store({ id: 'm1', name: 'Grimoire' }) };
      case 'mountFilesList':
        return { files: [] };
      default:
        return {};
    }
  }

  it('re-lists the stores silently — the list never flips back to its loading state', async () => {
    const seen: DispatchReq[] = [];
    const visible = signal(true);
    TestBed.configureTestingModule({
      imports: [ScriptoriumList],
      providers: [
        provideRouter([]),
        {
          provide: CoreClient,
          useValue: stubClient((r) => {
            seen.push(r);
            return handler(r);
          }),
        },
        { provide: WORKSPACE_TAB_ID, useValue: 'tab-s' },
        { provide: WORKSPACE_TAB_VISIBLE, useValue: visible },
      ],
    });
    const fixture = TestBed.createComponent(ScriptoriumList);
    fixture.detectChanges();
    await settle(fixture);
    const listCalls = () => seen.filter((r) => r.type === 'mountPointList').length;
    expect(listCalls()).toBe(1);

    visible.set(false);
    fixture.detectChanges();
    expect(listCalls()).toBe(1); // hiding refreshes nothing

    visible.set(true);
    fixture.detectChanges();
    // In flight, with the current list still on screen (v4's `{ silent: true }`).
    expect(fixture.nativeElement.textContent).toContain('Grimoire');
    expect(fixture.nativeElement.querySelector('qt-loading-state')).toBeNull();

    await settle(fixture);
    expect(listCalls()).toBe(2);
  });
});

describe('CreateStoreDialog', () => {
  function render(): ComponentFixture<CreateStoreDialog> {
    TestBed.configureTestingModule({
      imports: [CreateStoreDialog],
      providers: [{ provide: CoreClient, useValue: {} }],
    });
    const fixture = TestBed.createComponent(CreateStoreDialog);
    fixture.detectChanges();
    return fixture;
  }

  it('shows the DirectoryPicker + patterns for filesystem, hides them for database', () => {
    const fixture = render();
    // default filesystem → picker + include patterns visible
    expect(fixture.nativeElement.querySelector('qt-directory-picker')).toBeTruthy();
    expect(fixture.nativeElement.textContent).toContain('Include Patterns');

    fixture.componentInstance['mountType'].set('database');
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('qt-directory-picker')).toBeFalsy();
    expect(fixture.nativeElement.textContent).toContain("encrypted mount-index database");
  });

  it('emits a database create with an empty basePath and no patterns', () => {
    const fixture = render();
    const emitted: unknown[] = [];
    fixture.componentInstance.submitted.subscribe((d) => emitted.push(d));
    fixture.componentInstance['name'].set('DB Store');
    fixture.componentInstance['mountType'].set('database');
    fixture.detectChanges();
    fixture.componentInstance['submit']();
    expect(emitted[0]).toEqual({
      name: 'DB Store',
      basePath: '',
      mountType: 'database',
      storeType: 'documents',
    });
  });
});

// ---------------------------------------------------------------------------
// DirectoryPicker — browse populates, Select writes the value back.
// ---------------------------------------------------------------------------
describe('DirectoryPicker', () => {
  it('browses over systemBrowseDirectory and Select writes the path to value', async () => {
    const client = stubClient((r) =>
      r.type === 'systemBrowseDirectory'
        ? {
            path: '/home/docs',
            parent: '/home',
            directories: [
              { name: 'alpha', path: '/home/docs/alpha' },
              { name: 'beta', path: '/home/docs/beta' },
            ],
          }
        : {},
    );
    TestBed.configureTestingModule({
      imports: [DirectoryPicker],
      providers: [{ provide: CoreClient, useValue: client }],
    });
    const fixture = TestBed.createComponent(DirectoryPicker);
    fixture.componentRef.setInput('value', '');
    fixture.detectChanges();

    fixture.componentInstance['openBrowser']();
    await settle(fixture);
    expect(fixture.nativeElement.textContent).toContain('alpha');
    expect(fixture.nativeElement.textContent).toContain('beta');

    fixture.componentInstance['select']();
    fixture.detectChanges();
    expect(fixture.componentInstance.value()).toBe('/home/docs');
  });
});

// ---------------------------------------------------------------------------
// FileTable — rows, filter, sort, upload gating, and the delete confirm.
// ---------------------------------------------------------------------------
describe('FileTable', () => {
  function render(
    files: DocumentStoreFile[],
    mountType: string,
    client: Partial<CoreClient> = {},
  ): ComponentFixture<FileTable> {
    TestBed.configureTestingModule({
      imports: [FileTable],
      providers: [{ provide: CoreClient, useValue: client }],
    });
    const fixture = TestBed.createComponent(FileTable);
    fixture.componentRef.setInput('files', files);
    fixture.componentRef.setInput('mountPointId', 'm1');
    fixture.componentRef.setInput('mountType', mountType);
    fixture.detectChanges();
    return fixture;
  }

  it('renders file rows and hides Upload for a filesystem mount', () => {
    const fixture = render([file({ fileName: 'intro.md' })], 'filesystem');
    expect(fixture.nativeElement.textContent).toContain('intro.md');
    expect(fixture.nativeElement.textContent).toContain('1 files');
    expect(fixture.nativeElement.querySelector('input[type="file"]')).toBeFalsy();
  });

  it('shows Upload for a database mount', () => {
    const fixture = render([], 'database');
    expect(fixture.nativeElement.querySelector('input[type="file"]')).toBeTruthy();
    expect(fixture.nativeElement.textContent).toContain('No files yet');
  });

  it('filters by name and toggles sort direction', () => {
    const fixture = render([file({ id: 'a', fileName: 'apple.md' }), file({ id: 'b', fileName: 'zeta.md' })], 'filesystem');
    fixture.componentInstance['filter'].set('zeta');
    fixture.detectChanges();
    expect(fixture.componentInstance['filteredAndSorted']().map((f) => f.id)).toEqual(['b']);

    fixture.componentInstance['filter'].set('');
    fixture.componentInstance['handleSort']('fileName');
    // already default asc on fileName → toggles to desc
    expect(fixture.componentInstance['sortDir']()).toBe('desc');
    expect(fixture.componentInstance['filteredAndSorted']().map((f) => f.fileName)).toEqual([
      'zeta.md',
      'apple.md',
    ]);
  });

  it('delete confirms then dispatches mountFileDelete + refresh', async () => {
    const calls: DispatchReq[] = [];
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true);
    const fixture = render(
      [file({ fileType: 'pdf', relativePath: 'a.pdf' })],
      'database',
      stubClient((r) => {
        calls.push(r);
        return {};
      }),
    );
    const refreshed = vi.fn();
    fixture.componentInstance.refresh.subscribe(refreshed);
    await fixture.componentInstance['handleBlobDelete'](file({ fileType: 'pdf', relativePath: 'a.pdf' }));
    expect(confirmSpy).toHaveBeenCalled();
    expect(calls.some((c) => c.type === 'mountFileDelete' && c['path'] === 'a.pdf')).toBe(true);
    expect(refreshed).toHaveBeenCalled();
    confirmSpy.mockRestore();
  });
});
