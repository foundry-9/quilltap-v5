import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  OnInit,
  output,
  signal,
} from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute, Router } from '@angular/router';
import { map } from 'rxjs';

import { WORKSPACE_TAB_ID, onTabActivated } from '../../workspace/workspace-contract';
import { CoreClient } from '../../core/core-client';
import { FileManager } from '../../files/file-manager/file-manager';
import type { MountType } from '../../files/types';
import { formatBytes } from '../../ui/format-bytes';
import { Icon } from '../../ui/icon';
import { EditStoreDialog } from './edit-store-dialog';
import { FileTable } from './file-table';
import {
  fetchStore,
  fetchStoreFiles,
  scanStore,
  updateStore,
} from './scriptorium.api';
import type { DocumentStore, DocumentStoreFile, UpdateDocumentStoreData } from './scriptorium.api';

/**
 * The document-store detail page (v4 `[id]/DocumentStoreDetailView.tsx`): the
 * header (badges, mono basePath / "Stored encrypted…" line, scan/re-chunk +
 * edit), 4 info cards (files / size / last-scanned / embedding status), the
 * scan-error panel, include/exclude pattern cards (hidden for database mounts),
 * and the Indexed Files section with the classic {@link FileTable}. Scanning
 * re-fetches BOTH the store and its files.
 *
 * The file-manager toggle is the UNIFICATION wire of the P4.6z∥P4.6aa round
 * (Shared contract §4), pasted at unification: the Indexed-Files header gains
 * the "New file manager (beta)" / "Classic view" toggle (the v4
 * `DocumentStoreDetailView.tsx:229-236` affordance) and an
 * `@if (useFileManager())` branch renders the sibling lane's
 * {@link FileManager} (`@defer`-loaded) over the server-derived capability
 * bag; the classic FileTable stays the default.
 */
@Component({
  selector: 'qt-store-detail',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, FileTable, FileManager, EditStoreDialog],
  template: `
    @if (loading()) {
      <div class="flex min-h-screen items-center justify-center">
        <p class="qt-section-title">Loading document store...</p>
      </div>
    } @else if (error() || !store()) {
      <div class="flex min-h-screen items-center justify-center">
        <div class="text-center">
          <p class="text-lg qt-text-destructive mb-4">{{ error() || 'Document store not found' }}</p>
          <button type="button" class="qt-button-secondary" (click)="goBack()">Back to The Scriptorium</button>
        </div>
      </div>
    } @else if (store(); as s) {
      <div class="qt-page-container text-foreground">
        <div class="mb-6">
          <button
            type="button"
            (click)="goBack()"
            class="inline-flex items-center gap-1 text-sm qt-text-secondary hover:text-foreground mb-4 transition-colors"
          >
            <qt-icon name="chevron-left" class="w-4 h-4" />
            Back to The Scriptorium
          </button>

          <div class="flex flex-wrap items-start justify-between gap-4">
            <div>
              <div class="flex items-center gap-3">
                <h1 class="qt-page-title">{{ s.name }}</h1>
                @if (!s.enabled) {
                  <span class="qt-badge-disabled inline-flex items-center">Disabled</span>
                }
                @if (s.mountType === 'database') {
                  <span class="qt-badge inline-flex items-center">Database-backed</span>
                }
              </div>
              @if (s.mountType === 'database') {
                <p class="mt-1 qt-text-small text-xs italic">Stored encrypted in the mount-index database.</p>
              } @else {
                <p class="mt-1 qt-text-small font-mono text-xs">{{ s.basePath }}</p>
              }
            </div>
            <div class="flex gap-2">
              <button
                type="button"
                (click)="handleScan()"
                [disabled]="scanDisabled()"
                class="qt-button-secondary inline-flex items-center gap-1.5"
                [class.opacity-50]="scanDisabled()"
                [class.cursor-not-allowed]="scanDisabled()"
                [title]="s.mountType === 'database' ? 'Rechunk all documents in this database-backed store' : ''"
              >
                <qt-icon name="refresh" class="w-4 h-4" [class.animate-spin]="scanning()" />
                {{ scanning() ? 'Scanning...' : s.mountType === 'database' ? 'Re-chunk' : 'Scan Now' }}
              </button>
              <button
                type="button"
                (click)="editOpen.set(true)"
                class="qt-button-secondary inline-flex items-center gap-1.5"
              >
                <qt-icon name="pencil" class="w-4 h-4" />
                Edit
              </button>
            </div>
          </div>
        </div>

        <div class="grid grid-cols-2 gap-4 mb-8 sm:grid-cols-4">
          <div class="rounded-xl qt-bg-card border qt-border-default p-4">
            <div class="qt-heading-2 text-foreground">{{ s.fileCount }}</div>
            <div class="text-xs qt-text-secondary">Indexed Files</div>
          </div>
          <div class="rounded-xl qt-bg-card border qt-border-default p-4">
            <div class="qt-heading-2 text-foreground">{{ sizeLabel() }}</div>
            <div class="text-xs qt-text-secondary">Total Size</div>
          </div>
          <div class="rounded-xl qt-bg-card border qt-border-default p-4">
            <div class="qt-label">{{ lastScanned() }}</div>
            <div class="text-xs qt-text-secondary">Last Scanned</div>
          </div>
          <div class="rounded-xl qt-bg-card border qt-border-default p-4">
            @if (s.chunkCount === 0) {
              <div class="qt-label qt-text-secondary">No chunks</div>
              <div class="text-xs qt-text-secondary">Embedding Status</div>
            } @else if (s.embeddedChunkCount === s.chunkCount) {
              <div class="flex items-center gap-1.5">
                <span class="h-2 w-2 rounded-full qt-dot-success"></span>
                <span class="qt-label qt-text-success">Complete</span>
              </div>
              <div class="text-xs qt-text-secondary">{{ s.embeddedChunkCount }}/{{ s.chunkCount }} chunks embedded</div>
            } @else {
              <div class="flex items-center gap-1.5">
                <span class="h-2 w-2 animate-pulse rounded-full qt-dot-warning"></span>
                <span class="qt-label qt-text-warning">{{ embeddedPct() }}%</span>
              </div>
              <div class="text-xs qt-text-secondary">{{ s.embeddedChunkCount }}/{{ s.chunkCount }} chunks embedded</div>
            }
          </div>
        </div>

        @if (s.scanStatus === 'error' && s.lastScanError) {
          <div class="mb-6 rounded-xl qt-border-destructive/30 qt-bg-destructive/10 border p-4">
            <h3 class="text-sm font-semibold qt-text-destructive mb-1">Last Scan Error</h3>
            <p class="text-sm qt-text-destructive">{{ s.lastScanError }}</p>
          </div>
        }

        @if (s.mountType !== 'database') {
          <div class="mb-6 grid grid-cols-1 gap-4 sm:grid-cols-2">
            <div class="rounded-xl qt-bg-card border qt-border-default p-4">
              <h3 class="qt-text-secondary uppercase tracking-wider mb-2">Include Patterns</h3>
              <div class="flex flex-wrap gap-1.5">
                @for (p of s.includePatterns; track $index) {
                  <span class="inline-flex rounded qt-bg-success/10 qt-text-success px-2 py-0.5 text-xs font-mono">{{ p }}</span>
                }
              </div>
            </div>
            <div class="rounded-xl qt-bg-card border qt-border-default p-4">
              <h3 class="qt-text-secondary uppercase tracking-wider mb-2">Exclude Patterns</h3>
              <div class="flex flex-wrap gap-1.5">
                @for (p of s.excludePatterns; track $index) {
                  <span class="inline-flex rounded qt-bg-destructive/10 qt-text-destructive px-2 py-0.5 text-xs font-mono">{{ p }}</span>
                }
              </div>
            </div>
          </div>
        }

        <!-- Indexed Files — the file-manager toggle (the P4.6z∥P4.6aa unification
             wire, contract §4). The FileTable stays the default view. -->
        <div class="border-t qt-border-default/60 pt-6">
          <div class="mb-4 flex items-center justify-between gap-3">
            <h2 class="qt-heading-3">Indexed Files</h2>
            @if (s.capabilities) {
              <button
                type="button"
                class="qt-button-ghost inline-flex items-center gap-1.5 text-sm"
                title="Preview the new file manager"
                (click)="useFileManager.set(!useFileManager())"
              >
                <qt-icon name="layers" class="w-4 h-4" />
                {{ useFileManager() ? 'Classic view' : 'New file manager (beta)' }}
              </button>
            }
          </div>
          @if (useFileManager() && s.capabilities) {
            @defer {
              <qt-file-manager
                [mountPointId]="s.id"
                [capabilities]="s.capabilities"
                [mountType]="mountKind()"
              />
            } @placeholder {
              <p class="qt-section-title p-6">Summoning the file manager…</p>
            }
          } @else {
            <qt-file-table
              [files]="files()"
              [loading]="filesLoading()"
              [mountPointId]="s.id"
              [mountType]="s.mountType"
              (refresh)="loadFiles()"
            />
          }
        </div>
      </div>

      @if (editOpen()) {
        <qt-edit-store-dialog [store]="s" (close)="editOpen.set(false)" (submitted)="handleUpdate($event)" />
      }
    }
  `,
})
export class StoreDetail implements OnInit {
  private readonly core = inject(CoreClient);
  private readonly route = inject(ActivatedRoute, { optional: true });
  private readonly router = inject(Router);

  /**
   * In-tab drill (p4.9j2, v4 `ScriptoriumView` `selectedStoreId`): the list
   * supplies `storeId` (wins over the route `:id`) and binds `(back)` to restore
   * the list. Null ⇒ routed `/scriptorium/:id`.
   */
  readonly storeIdInput = input<string | null>(null, { alias: 'storeId' });
  readonly back = output<void>();
  protected readonly embedded = computed(() => this.storeIdInput() != null);

  private readonly routeStoreId = this.route
    ? toSignal(this.route.paramMap.pipe(map((p) => p.get('id') ?? '')), { initialValue: '' })
    : undefined;
  protected readonly storeId = computed(
    () => this.storeIdInput() ?? this.routeStoreId?.() ?? '',
  );

  protected readonly store = signal<DocumentStore | null>(null);
  protected readonly files = signal<DocumentStoreFile[]>([]);
  protected readonly loading = signal(true);
  protected readonly filesLoading = signal(true);
  protected readonly error = signal<string | null>(null);
  protected readonly scanning = signal(false);
  protected readonly editOpen = signal(false);
  /** The unification toggle (contract §4): file manager vs the classic table. */
  protected readonly useFileManager = signal(false);
  /** `DocMountPointDto.mountType` is `string`; the widget input is the union. */
  protected readonly mountKind = computed(
    () => (this.store()?.mountType ?? 'filesystem') as MountType,
  );

  protected readonly sizeLabel = computed(() => formatBytes(this.store()?.totalSizeBytes ?? 0));
  protected readonly embeddedPct = computed(() => {
    const s = this.store();
    return s ? Math.round((s.embeddedChunkCount / s.chunkCount) * 100) : 0;
  });
  protected readonly scanDisabled = computed(
    () => this.scanning() || this.store()?.scanStatus === 'scanning',
  );
  protected readonly lastScanned = computed(() => {
    const at = this.store()?.lastScannedAt;
    return at ? new Date(at).toLocaleString() : 'Never';
  });

  constructor() {
    // Navigating back to the containing workspace tab refreshes the store and
    // its file list in place (silent store load — no loading flip, which would
    // swap the page for its loading state; v4 leaves the FILE load loud, as
    // `DocumentStoreDetailView.tsx:63-70` does). v5's document stores are
    // hand-rolled signals, so this hook IS the refresh for this surface.
    onTabActivated(() => {
      void this.loadStore({ silent: true });
      void this.loadFiles();
    });
  }

  ngOnInit(): void {
    void this.loadStore();
    void this.loadFiles();
  }

  protected goBack(): void {
    // Drill mode ⇒ hand control back to the list; routed ⇒ navigate.
    if (this.embedded()) {
      this.back.emit();
      return;
    }
    void this.router.navigate(['/scriptorium']);
  }

  /**
   * `silent` refreshes in place without flipping `loading` (tab re-activation)
   * — v4 `useDocumentStoreDetail.fetchStore({ silent })`.
   */
  protected async loadStore(opts?: { silent?: boolean }): Promise<void> {
    if (!opts?.silent) {
      this.loading.set(true);
    }
    this.error.set(null);
    try {
      this.store.set(await fetchStore(this.core, this.storeId()));
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : 'An error occurred');
    } finally {
      this.loading.set(false);
    }
  }

  protected async loadFiles(): Promise<void> {
    this.filesLoading.set(true);
    try {
      this.files.set(await fetchStoreFiles(this.core, this.storeId()));
    } catch {
      /* v4 swallows the files-fetch error (the store still renders). */
    } finally {
      this.filesLoading.set(false);
    }
  }

  protected async handleUpdate(event: { id: string; data: UpdateDocumentStoreData }): Promise<void> {
    try {
      const updated = await updateStore(this.core, this.storeId(), event.data);
      this.store.set(updated);
      this.editOpen.set(false);
    } catch {
      /* the edit dialog stays open; the error surfaces via the list flash on the next visit. */
    }
  }

  protected async handleScan(): Promise<void> {
    this.scanning.set(true);
    this.store.update((prev) => (prev ? { ...prev, scanStatus: 'scanning' } : prev));
    try {
      await scanStore(this.core, this.storeId());
      // Refresh store AND files after scan (v4 `useDocumentStoreDetail.scanStore`).
      const [store, files] = await Promise.all([
        fetchStore(this.core, this.storeId()),
        fetchStoreFiles(this.core, this.storeId()),
      ]);
      this.store.set(store);
      this.files.set(files);
    } catch {
      this.store.update((prev) => (prev ? { ...prev, scanStatus: 'error' } : prev));
    } finally {
      this.scanning.set(false);
    }
  }
}
