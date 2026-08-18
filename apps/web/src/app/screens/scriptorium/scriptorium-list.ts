import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  input,
  OnInit,
  signal,
  untracked,
} from '@angular/core';
import { Router } from '@angular/router';

import { WORKSPACE_TAB_ID, onTabActivated } from '../../workspace/workspace-contract';
import { LoadingState } from '../../ui/loading-state';
import { ConvertStoreDialog } from './convert-store-dialog';
import { CreateStoreDialog } from './create-store-dialog';
import { DeconvertStoreDialog } from './deconvert-store-dialog';
import { DeleteStoreDialog } from './delete-store-dialog';
import { EditStoreDialog } from './edit-store-dialog';
import { StoreCard } from './store-card';
import { StoreDetail } from './store-detail';
import { ScriptoriumStore } from './scriptorium.store';
import type {
  CreateDocumentStoreData,
  DocumentStore,
  UpdateDocumentStoreData,
} from './scriptorium.api';

/**
 * The Scriptorium list page (v4 `ScriptoriumView.tsx`): a title + Add Document
 * Store button, the card grid over `mountPointList`, and the five dialogs
 * (create / edit / delete / convert / deconvert). Scan/convert/deconvert use the
 * store's patch-not-refetch shape.
 *
 * **In-tab drill (p4.9j2, v4 `ScriptoriumView` `selectedStoreId`).** When hosted
 * as a workspace tab (`WORKSPACE_TAB_ID` non-null), a card's Open drills IN PLACE
 * via internal state (rendering `qt-store-detail` embedded) instead of routing to
 * `/scriptorium/:id`; the detail's `(back)` restores the list. Routed mode
 * navigates exactly as today.
 */
@Component({
  selector: 'qt-scriptorium-list',
  changeDetection: ChangeDetectionStrategy.OnPush,
  providers: [ScriptoriumStore],
  imports: [
    LoadingState,
    StoreCard,
    StoreDetail,
    CreateStoreDialog,
    EditStoreDialog,
    DeleteStoreDialog,
    ConvertStoreDialog,
    DeconvertStoreDialog,
  ],
  template: `
    @if (selectedStoreId(); as sid) {
      <qt-store-detail [storeId]="sid" (back)="onDrillBack()" />
    } @else {
    @if (store.loading()) {
      <div class="flex min-h-screen items-center justify-center">
        <qt-loading-state message="Loading document stores..." />
      </div>
    } @else if (store.error(); as err) {
      <div class="flex min-h-screen items-center justify-center">
        <p class="text-lg qt-text-destructive">Error: {{ err }}</p>
      </div>
    } @else {
      <div class="qt-page-container text-foreground">
        <div class="flex flex-wrap items-center justify-between gap-4 border-b qt-border-default/60 pb-6">
          <div>
            <h1 class="qt-page-title">The Scriptorium</h1>
            <p class="mt-1 qt-text-small">
              Mount external document directories as searchable knowledge sources
            </p>
          </div>
          <button type="button" class="qt-button-primary" (click)="createOpen.set(true)">
            Add Document Store
          </button>
        </div>

        @if (store.isEmpty()) {
          <div
            class="mt-12 rounded-2xl border border-dashed qt-border-default/70 qt-bg-card/80 px-8 py-12 text-center qt-shadow-sm"
          >
            <p class="mb-4 text-lg qt-text-secondary">No document stores yet</p>
            <p class="mb-6 qt-text-small max-w-md mx-auto">
              Mount external document directories as searchable knowledge sources. Connect filesystem
              paths or Obsidian vaults to make their contents available to your AI conversations.
            </p>
            <button type="button" class="qt-button-primary" (click)="createOpen.set(true)">
              Add your first document store
            </button>
          </div>
        } @else {
          <div class="mt-8 grid grid-cols-1 gap-6 md:grid-cols-2 lg:grid-cols-3">
            @for (s of store.stores(); track s.id) {
              <qt-store-card
                [store]="s"
                [scanning]="scanningIds().has(s.id)"
                (open)="openStore(s.id)"
                (edit)="editStore.set(s)"
                (delete)="deleteStoreId.set(s.id)"
                (scan)="handleScan(s.id)"
                (convert)="convertTarget.set(s)"
                (deconvert)="deconvertTarget.set(s)"
              />
            }
          </div>
        }
      </div>
    }

    @if (createOpen()) {
      <qt-create-store-dialog (close)="createOpen.set(false)" (submitted)="handleCreate($event)" />
    }
    @if (editStore(); as s) {
      <qt-edit-store-dialog [store]="s" (close)="editStore.set(null)" (submitted)="handleUpdate($event)" />
    }
    @if (deleteStoreId() !== null) {
      <qt-delete-store-dialog (close)="deleteStoreId.set(null)" (confirm)="handleDelete()" />
    }
    @if (convertTarget(); as s) {
      <qt-convert-store-dialog [store]="s" (close)="convertTarget.set(null)" (confirm)="handleConvert()" />
    }
    @if (deconvertTarget(); as s) {
      <qt-deconvert-store-dialog
        [store]="s"
        (close)="deconvertTarget.set(null)"
        (confirm)="handleDeconvert($event)"
      />
    }
    }
  `,
})
export class ScriptoriumList implements OnInit {
  protected readonly store = inject(ScriptoriumStore);
  private readonly router = inject(Router);
  /** Non-null ⇒ hosted as a workspace tab; drill in place instead of routing. */
  private readonly tabId = inject(WORKSPACE_TAB_ID, { optional: true });

  /** v4 `ScriptoriumView` `selectedStoreId` — the in-tab drill target. */
  protected readonly selectedStoreId = signal<string | null>(null);
  protected readonly inTab = computed(() => this.tabId != null);

  /**
   * Deep-link target (v4 `8d86847a` `ScriptoriumViewProps.initialStoreId`):
   * `/scriptorium/<id>` redirects into the Scriptorium tab carrying this id, and
   * a RE-open of the already-open tab refreshes the payload — so the effect
   * follows every truthy change, not just the first (v4 does the same with its
   * derive-from-prop-change guard). A drill-back leaves the input untouched, so
   * the effect does not re-run and the list stays put.
   */
  readonly initialStoreId = input<string | null>(null);

  protected readonly createOpen = signal(false);
  protected readonly editStore = signal<DocumentStore | null>(null);
  protected readonly deleteStoreId = signal<string | null>(null);
  protected readonly convertTarget = signal<DocumentStore | null>(null);
  protected readonly deconvertTarget = signal<DocumentStore | null>(null);
  protected readonly scanningIds = signal<Set<string>>(new Set());

  constructor() {
    effect(() => {
      const id = this.initialStoreId();
      if (id) untracked(() => this.selectedStoreId.set(id));
    });

    // Navigating back to this tab refreshes the list in place (silent — no
    // loading flip, which would unmount an in-place store detail). v4
    // `ScriptoriumView.tsx:67-72`. v5's stores are hand-rolled signals, so the
    // tab-activation map has no entry for this kind — this hook IS the refresh.
    onTabActivated(() => {
      void this.store.fetchStores({ silent: true });
    });
  }

  ngOnInit(): void {
    void this.store.fetchStores();
  }

  protected openStore(id: string): void {
    if (this.tabId != null) {
      this.selectedStoreId.set(id);
      return;
    }
    void this.router.navigate(['/scriptorium', id]);
  }

  /** Drilled detail's back — restore the list and refetch (v4 refetches on remount). */
  protected onDrillBack(): void {
    this.selectedStoreId.set(null);
    void this.store.fetchStores();
  }

  protected async handleCreate(data: CreateDocumentStoreData): Promise<void> {
    const result = await this.store.createStore(data);
    if (result) {
      this.createOpen.set(false);
    }
  }

  protected async handleUpdate(event: { id: string; data: UpdateDocumentStoreData }): Promise<void> {
    const result = await this.store.updateStore(event.id, event.data);
    if (result) {
      this.editStore.set(null);
    }
  }

  protected async handleDelete(): Promise<void> {
    const id = this.deleteStoreId();
    if (!id) {
      return;
    }
    const ok = await this.store.deleteStore(id);
    if (ok) {
      this.deleteStoreId.set(null);
    }
  }

  protected async handleScan(id: string): Promise<void> {
    this.scanningIds.update((prev) => new Set(prev).add(id));
    await this.store.scanStore(id);
    this.scanningIds.update((prev) => {
      const next = new Set(prev);
      next.delete(id);
      return next;
    });
  }

  protected async handleConvert(): Promise<void> {
    const target = this.convertTarget();
    if (!target) {
      return;
    }
    this.convertTarget.set(null);
    await this.store.convertStore(target.id);
  }

  protected async handleDeconvert(targetPath: string): Promise<void> {
    const target = this.deconvertTarget();
    if (!target) {
      return;
    }
    this.deconvertTarget.set(null);
    await this.store.deconvertStore(target.id, targetPath);
  }
}
