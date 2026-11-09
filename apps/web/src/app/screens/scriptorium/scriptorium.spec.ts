import { signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type { DocMountPointDto } from '../../core/core-contract';
import { WORKSPACE_TAB_ID, WORKSPACE_TAB_VISIBLE } from '../../workspace/workspace-contract';
import { CreateStoreDialog } from './create-store-dialog';
import { DirectoryPicker } from './directory-picker';
import { DocumentStoreBrowser } from '../../document-browser/document-store-browser';
import { AlertModalService } from '../../ui/alert-modal.service';
import { ScriptoriumList } from './scriptorium-list';
import { ScriptoriumStore } from './scriptorium.store';
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
        provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
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
    // v4 `c34986421`: the section is `Files`, the document-store browser.
    expect(fixture.nativeElement.querySelector('qt-document-store-browser')).toBeTruthy();

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
        provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
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
// The store's files — RE-AIMED from the retired FileTable (D18 retirement,
// ruling R-D) onto the document-store browser (v4 `c34986421`). Each case
// names the v4 line it now pins. REMOVED, no counterpart after `c34986421`:
// the column-sort toggle (`handleSort` / `sortDir`) — v4's browser always
// sorts by name, case-insensitively (`DocumentStoreBrowser.tsx:90-93`).
// ---------------------------------------------------------------------------
describe('the store browser (re-aimed from FileTable; v4 DocumentStoreBrowser.tsx)', () => {
  const caps = (write: boolean) => ({
    canWrite: write,
    canDelete: write,
    canCreateFolder: write,
    canMoveIn: write,
    canMoveOut: write,
    canConvert: write,
  });
  const row = (relativePath: string, over: Record<string, unknown> = {}) => ({
    id: `l-${relativePath}`,
    mountPointId: 'm1',
    relativePath,
    fileName: relativePath.split('/').pop(),
    fileType: 'markdown',
    fileSizeBytes: 1024,
    lastModified: '2024-01-01T00:00:00Z',
    createdAt: '2024-01-01T00:00:00Z',
    updatedAt: '2024-01-01T00:00:00Z',
    conversionStatus: 'converted',
    linkGroupId: null,
    source: 'database',
    ...over,
  });

  async function render(write: boolean, files: unknown[], seen: DispatchReq[] = [], confirm = 'Confirm') {
    TestBed.configureTestingModule({
      imports: [DocumentStoreBrowser],
      providers: [
        provideRouter([]),
        provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
        {
          provide: CoreClient,
          useValue: stubClient((r) => {
            seen.push(r);
            if (r.type === 'mountPointGet') return { mountPoint: { ...store({ mountType: 'database' }), capabilities: caps(write) } };
            if (r.type === 'mountFilesList') return { files, folders: [] };
            if (r.type === 'mountPointList') return { mountPoints: [] };
            return {};
          }),
        },
        { provide: AlertModalService, useValue: { show: async () => confirm, current: signal(null) } },
      ],
    });
    const fixture = TestBed.createComponent(DocumentStoreBrowser);
    fixture.componentRef.setInput('storeId', 'm1');
    document.body.appendChild(fixture.nativeElement);
    fixture.detectChanges();
    await settle(fixture);
    return fixture;
  }

  it('renders file rows and hides Upload where the store cannot be written (v4 :413 caps.canWrite)', async () => {
    const fixture = await render(false, [row('intro.md')]);
    expect(fixture.nativeElement.textContent).toContain('intro.md');
    expect(fixture.nativeElement.textContent).toContain('1 file · 1.0 KB');
    expect(fixture.nativeElement.querySelector('input[type="file"]')).toBeFalsy();
    document.body.innerHTML = '';
    TestBed.resetTestingModule();
  });

  it('shows Upload here on a writable store, and the empty text (v4 :333-335, :413-427)', async () => {
    const fixture = await render(true, []);
    expect(fixture.nativeElement.querySelector('input[type="file"]')).toBeTruthy();
    expect(fixture.nativeElement.textContent).toContain('Upload here');
    expect(fixture.nativeElement.textContent).toContain('Nothing filed here yet. Upload something, or make a folder.');
    document.body.innerHTML = '';
    TestBed.resetTestingModule();
  });

  it('filters by the store search (v4 :94-97 — lower-cased relativePath includes)', async () => {
    const fixture = await render(false, [row('apple.md'), row('Drafts/zeta.md')]);
    const search = fixture.nativeElement.querySelector('input[type="search"]') as HTMLInputElement;
    search.value = 'ZETA';
    search.dispatchEvent(new Event('input'));
    await settle(fixture);
    const names = [...fixture.nativeElement.querySelectorAll('[data-testid="document-row"]')].map(
      (r) => (r as HTMLElement).getAttribute('data-path'),
    );
    expect(names).toEqual(['Drafts/zeta.md']);
    document.body.innerHTML = '';
    TestBed.resetTestingModule();
  });

  it('delete confirms then dispatches mountFileDelete (v4 :165-185)', async () => {
    const seen: DispatchReq[] = [];
    const fixture = await render(true, [row('a.pdf', { fileType: 'pdf' })], seen);
    (fixture.nativeElement.querySelector('[aria-label="Actions for a.pdf"]') as HTMLButtonElement).click();
    fixture.detectChanges();
    const del = [...document.querySelectorAll('[role="menuitem"]')].find((b) => b.textContent!.trim() === 'Delete') as HTMLButtonElement;
    del.click();
    await settle(fixture);
    expect(seen.some((c) => c.type === 'mountFileDelete' && c['path'] === 'a.pdf')).toBe(true);
    document.body.innerHTML = '';
    TestBed.resetTestingModule();
  });
});
