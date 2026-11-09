import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute, Router } from '@angular/router';
import { QueryClient, injectQuery } from '@tanstack/angular-query-experimental';
import { map } from 'rxjs';

import { onTabActivated } from '../../workspace/workspace-contract';
import { CoreClient, coreErrorMessage } from '../../core/core-client';
import { DocumentStoreBrowser } from '../../document-browser/document-store-browser';
import { mountPointKeys } from '../../document-browser/document-store.keys';
import { formatBytes } from '../../ui/format-bytes';
import { Icon } from '../../ui/icon';
import { ToastService } from '../../ui/toast.service';
import { EditStoreDialog } from './edit-store-dialog';
import { fetchStore, scanStore, updateStore } from './scriptorium.api';
import type { DocumentStore, UpdateDocumentStoreData } from './scriptorium.api';

/**
 * The document-store detail page (v4 `[id]/DocumentStoreDetailView.tsx`): the
 * header (badges, mono basePath / "Stored encrypted…" line, scan/re-chunk +
 * edit), 4 info cards (files / size / last-scanned / embedding status), the
 * scan-error panel, include/exclude pattern cards (hidden for database mounts),
 * and the **Files** section — v4 `c34986421`'s document-store browser
 * (`qt-document-store-browser`), which retired the classic file table and the
 * D18 file manager (ruling R-D; P4.D387 item 14).
 *
 * The record is the TanStack query the browser reads too
 * (`mountPointKeys.detail`, v4 `useDocumentStoreDetail`), so the `mountPoints`
 * realtime topic keeps the page's counts current as files come and go. A
 * "where else this appears" location in another store opens that store: in the
 * Scriptorium tab the list drills to it (`openStore`), on the routed page the
 * router goes there (v4 `DocumentStoreDetailPageClient.tsx:21`).
 */
@Component({
  selector: 'qt-store-detail',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, DocumentStoreBrowser, EditStoreDialog],
  template: `
    @if (loading()) {
      <div class="flex min-h-screen items-center justify-center">
        <p class="qt-section-title">Loading document store...</p>
      </div>
    } @else if (error() || !store()) {
      <div class="flex min-h-screen items-center justify-center">
        <div class="text-center">
          <p class="text-lg qt-text-destructive mb-4">
            {{ error() || 'Document store not found' }}
          </p>
          <button type="button" class="qt-button-secondary" (click)="goBack()">
            Back to The Scriptorium
          </button>
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
                <p class="mt-1 qt-text-small text-xs italic">
                  Stored encrypted in the mount-index database.
                </p>
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
                [title]="
                  s.mountType === 'database'
                    ? 'Rechunk all documents in this database-backed store'
                    : ''
                "
              >
                <qt-icon name="refresh" class="w-4 h-4" [class.animate-spin]="scanning()" />
                {{
                  scanning() ? 'Scanning...' : s.mountType === 'database' ? 'Re-chunk' : 'Scan Now'
                }}
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
              <div class="text-xs qt-text-secondary">
                {{ s.embeddedChunkCount }}/{{ s.chunkCount }} chunks embedded
              </div>
            } @else {
              <div class="flex items-center gap-1.5">
                <span class="h-2 w-2 animate-pulse rounded-full qt-dot-warning"></span>
                <span class="qt-label qt-text-warning">{{ embeddedPct() }}%</span>
              </div>
              <div class="text-xs qt-text-secondary">
                {{ s.embeddedChunkCount }}/{{ s.chunkCount }} chunks embedded
              </div>
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
                  <span
                    class="inline-flex rounded qt-bg-success/10 qt-text-success px-2 py-0.5 text-xs font-mono"
                    >{{ p }}</span
                  >
                }
              </div>
            </div>
            <div class="rounded-xl qt-bg-card border qt-border-default p-4">
              <h3 class="qt-text-secondary uppercase tracking-wider mb-2">Exclude Patterns</h3>
              <div class="flex flex-wrap gap-1.5">
                @for (p of s.excludePatterns; track $index) {
                  <span
                    class="inline-flex rounded qt-bg-destructive/10 qt-text-destructive px-2 py-0.5 text-xs font-mono"
                    >{{ p }}</span
                  >
                }
              </div>
            </div>
          </div>
        }

        <!-- Files and folders — text documents, pdf/docx extracts and binary
             blobs alike, browsed like the wardrobe (v4 c34986421). -->
        <div class="border-t qt-border-default/60 pt-6">
          <h2 class="qt-heading-3 mb-4">Files</h2>
          @for (id of [s.id]; track id) {
            <qt-document-store-browser
              [storeId]="id"
              [opensStores]="true"
              (openStore)="openOtherStore($event)"
            />
          }
        </div>
      </div>

      @if (editOpen()) {
        <qt-edit-store-dialog
          [store]="s"
          (close)="editOpen.set(false)"
          (submitted)="handleUpdate($event)"
        />
      }
    }
  `,
})
export class StoreDetail {
  private readonly core = inject(CoreClient);
  private readonly queryClient = inject(QueryClient);
  private readonly toasts = inject(ToastService);
  private readonly route = inject(ActivatedRoute, { optional: true });
  private readonly router = inject(Router);

  /**
   * In-tab drill (p4.9j2, v4 `ScriptoriumView` `selectedStoreId`): the list
   * supplies `storeId` (wins over the route `:id`) and binds `(back)` to restore
   * the list. Null ⇒ routed `/scriptorium/:id`.
   */
  readonly storeIdInput = input<string | null>(null, { alias: 'storeId' });
  readonly back = output<void>();
  /** In-tab: open another store in place (v4 `ScriptoriumView.tsx:146`'s `setSelectedStoreId`). */
  readonly openStore = output<string>();
  protected readonly embedded = computed(() => this.storeIdInput() != null);

  private readonly routeStoreId = this.route
    ? toSignal(this.route.paramMap.pipe(map((p) => p.get('id') ?? '')), { initialValue: '' })
    : undefined;
  protected readonly storeId = computed(() => this.storeIdInput() ?? this.routeStoreId?.() ?? '');

  /** v4 `useDocumentStoreDetail`'s record query — the SAME key the browser reads. */
  private readonly storeQuery = injectQuery(() => ({
    queryKey: mountPointKeys.detail(this.storeId()),
    queryFn: () => fetchStore(this.core, this.storeId()),
    enabled: this.storeId() !== '',
  }));

  protected readonly store = computed<DocumentStore | null>(() => this.storeQuery.data() ?? null);
  protected readonly loading = computed(() => this.storeQuery.isLoading());
  protected readonly error = computed(() =>
    this.storeQuery.error()
      ? coreErrorMessage(this.storeQuery.error(), 'Failed to fetch document store')
      : null,
  );
  protected readonly scanning = signal(false);
  protected readonly editOpen = signal(false);

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
    // its listing in place (a background refetch — the page stays on screen;
    // v4 `DocumentStoreDetailView`'s `useOnTabActivated(() => refresh())`).
    onTabActivated(() => {
      void this.refresh();
    });
  }

  /** v4 `useDocumentStoreDetail`'s `invalidate` / `refresh`: this store's reads and the store lists. */
  protected refresh(): Promise<void> {
    const id = this.storeId();
    return Promise.all([
      this.queryClient.invalidateQueries({ queryKey: mountPointKeys.store(id) }),
      this.queryClient.invalidateQueries({ queryKey: mountPointKeys.lists }),
    ]).then(() => undefined);
  }

  protected goBack(): void {
    // Drill mode ⇒ hand control back to the list; routed ⇒ navigate.
    if (this.embedded()) {
      this.back.emit();
      return;
    }
    void this.router.navigate(['/scriptorium']);
  }

  /** The browser's "where else this appears" → another store (v4 `onOpenStore`, both hosts). */
  protected openOtherStore(id: string): void {
    if (this.embedded()) {
      this.openStore.emit(id);
      return;
    }
    void this.router.navigate(['/scriptorium', id]);
  }

  protected async handleUpdate(event: {
    id: string;
    data: UpdateDocumentStoreData;
  }): Promise<void> {
    try {
      await updateStore(this.core, this.storeId(), event.data);
      await this.refresh();
      this.toasts.showSuccess('Document store updated successfully!');
      this.editOpen.set(false);
    } catch (err) {
      this.toasts.showError(coreErrorMessage(err, 'Failed to update document store'));
    }
  }

  protected async handleScan(): Promise<void> {
    this.scanning.set(true);
    try {
      const result = await scanStore(this.core, this.storeId());
      await this.refresh();
      this.toasts.showSuccess(
        `Scan complete: ${result.scanResult.filesScanned} files scanned, ${result.embeddingJobsEnqueued} embedding jobs queued`,
      );
    } catch (err) {
      this.toasts.showError(coreErrorMessage(err, 'Failed to scan document store'));
      await this.refresh();
    } finally {
      this.scanning.set(false);
    }
  }
}
