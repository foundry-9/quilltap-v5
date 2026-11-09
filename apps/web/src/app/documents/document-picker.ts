import { ChangeDetectionStrategy, Component, computed, inject, input, OnInit, output, signal } from '@angular/core';
import { NgTemplateOutlet } from '@angular/common';

import { Icon } from '../ui/icon';
import { Modal } from '../ui/modal';
import { CollapsibleCard } from '../ui/collapsible-card';
import { ToastService } from '../ui/toast.service';
import type {
  AccessibleStoreDto,
  DocumentScope,
  LibraryTargetDto,
  ProjectLibraryTargetDto,
  RecentDocumentDto,
} from '../core/core-contract';
import { DocumentStoreBrowser } from '../document-browser/document-store-browser';
import type { DocumentEntry } from '../document-browser/document-store.source';
import { DocumentApi } from './document-api';
import { StandaloneDocumentApi } from './standalone-wire';

/** How many recent documents the picker shows (v4 `MAX_RECENT_DOCUMENTS`). */
const MAX_RECENT_DOCUMENTS = 10;

/** The parameters an open-document selection carries (v4 `onSelectDocument`). */
export interface DocumentSelection {
  filePath?: string;
  title?: string;
  scope?: DocumentScope;
  mountPoint?: string;
  targetFolder?: string;
}

interface SelectedMount {
  id: string;
  name: string;
}

/**
 * `qt-document-picker` — the Open-Document modal (v4 `DocumentPickerModal`).
 * Two steps: (1) source — new blank, the project and General libraries,
 * recents, and the store accordions (+ look-everywhere); (2) browse — since v4
 * `5efe53ea7`, the Scriptorium's own document-store browser in pick mode
 * (`qt-document-store-browser`: opening a file opens it in Document Mode, its
 * ⋮ menus stay), with "New document here" in its footer (P4.D387 item 16).
 *
 * Serves BOTH entries v4's one modal serves (P4.9J4): a `chatId` scopes the
 * chat-side picker (recents/stores from the chat's reach via {@link DocumentApi});
 * a NULL `chatId` is the standalone (chat-less) rail surface — recents + stores
 * come from {@link StandaloneDocumentApi} (`/api/v1/documents`), every enabled
 * store shows ("look everywhere" is implicit, its checkbox hidden), exactly as
 * v4's `chatId === null` arm.
 *
 * The project library is browsed as a STORE (v4 `5efe53ea7` dropped the
 * legacy `project`-scope fallback): its official store is an ordinary
 * `document_store` browse, like any shelf.
 *
 * Widths follow v4 (ruling L-6): `4xl` for the source step, `lg` for the
 * browse step — v5's single `3xl` was a pre-existing drift.
 */
@Component({
  selector: 'qt-document-picker',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, Icon, CollapsibleCard, NgTemplateOutlet, DocumentStoreBrowser],
  template: `
    <qt-modal
      [title]="step() === 'source' ? 'Open Document' : 'Select File'"
      [maxWidth]="step() === 'source' ? '4xl' : 'lg'"
      (close)="close.emit()"
    >
      @if (step() === 'source') {
        <div class="flex flex-col gap-4 md:flex-row md:items-start">
          <!-- Left column: new-blank + recents -->
          <div class="space-y-3 md:w-72 md:flex-shrink-0">
            <button
              type="button"
              class="qt-doc-open-button w-full flex items-center gap-3 p-4 rounded-lg border border-dashed qt-border-default hover:qt-bg-muted text-left"
              (click)="pickNewBlank()"
            >
              <span class="flex-shrink-0 w-10 h-10 rounded-lg flex items-center justify-center qt-bg-muted">
                <qt-icon name="plus" class="w-5 h-5 qt-text-secondary" />
              </span>
              <span>
                <span class="block font-medium qt-text-primary">New blank document</span>
                <span class="block text-sm qt-text-secondary">Create an empty Markdown document</span>
              </span>
            </button>

            <!-- Project library (v4 DocumentPickerModal.tsx:487-501). The
                 chat's own project store is DELIBERATELY held back from the
                 accessible-store list server-side ("surfaced separately —
                 left-column button", api/documents.rs:1131) and stays held back
                 under look-everywhere, so without this button the store the
                 conversation lives in is reachable by no path at all. -->
            @if (projectLibrary(); as library) {
              <button
                type="button"
                class="qt-doc-project-library w-full flex items-center gap-3 p-4 rounded-lg border qt-border-default hover:qt-bg-muted text-left"
                (click)="browseLibrary(library)"
              >
                <span class="flex-shrink-0 w-10 h-10 rounded-lg flex items-center justify-center qt-bg-muted">
                  <qt-icon name="folder" class="w-5 h-5 qt-text-secondary" />
                </span>
                <span class="min-w-0">
                  <span class="block font-medium qt-text-primary">Project library</span>
                  <span class="block text-sm qt-text-secondary truncate">{{ library.name }}</span>
                </span>
              </button>
            }

            <!-- General library: Quilltap General (v4 DocumentPickerModal.tsx:320-334, 5efe53ea7). -->
            @if (generalLibrary(); as general) {
              <button
                type="button"
                class="qt-doc-general-library w-full flex items-center gap-3 p-4 rounded-lg border qt-border-default hover:qt-bg-muted text-left"
                (click)="browseLibrary(general)"
              >
                <span class="flex-shrink-0 w-10 h-10 rounded-lg flex items-center justify-center qt-bg-muted">
                  <qt-icon name="scriptorium" class="w-5 h-5 qt-text-secondary" />
                </span>
                <span class="min-w-0">
                  <span class="block font-medium qt-text-primary">General library</span>
                  <span class="block text-sm qt-text-secondary">Quilltap General, the shelf every chat shares</span>
                </span>
              </button>
            }

            @if (ready()) {
            <qt-collapsible-card
              title="Recent"
              description="Documents you've lately had open"
              icon="clock"
              [defaultOpen]="recents().length > 0"
            >
              <div class="space-y-1">
                @if (recents().length === 0) {
                  <div class="px-3 py-2 text-sm qt-text-muted">No recent documents yet.</div>
                }
                @for (doc of recents().slice(0, maxRecent); track doc.id) {
                  <button
                    type="button"
                    class="w-full flex items-center gap-3 px-4 py-3 rounded-lg border qt-border-default hover:qt-bg-muted text-left"
                    (click)="pickRecent(doc)"
                  >
                    <qt-icon [name]="doc.isActive ? 'file' : 'clock'" class="w-5 h-5 flex-shrink-0 qt-text-secondary" />
                    <span class="flex-1 min-w-0">
                      <span class="block font-medium qt-text-primary truncate">{{ doc.displayTitle || doc.filePath }}</span>
                      <span class="block text-xs qt-text-secondary">{{ recentLabel(doc) }}</span>
                    </span>
                  </button>
                }
              </div>
            </qt-collapsible-card>
            }
          </div>

          <!-- Right column: store accordions -->
          <div class="flex-1 min-w-0 space-y-2">
            <!-- The standalone surface is always "everywhere" (v4 hides the
                 toggle when chatId === null). -->
            @if (!standalone()) {
              <label class="flex items-start gap-3 px-1 pb-1 cursor-pointer">
                <input
                  type="checkbox"
                  class="qt-checkbox mt-0.5"
                  [checked]="lookEverywhere()"
                  (change)="setLookEverywhere($any($event.target).checked)"
                />
                <span>
                  <span class="block text-sm font-medium qt-text-primary">Look everywhere</span>
                  <span class="block text-xs qt-text-secondary"
                    >Show every store, not just the ones reachable from this chat</span
                  >
                </span>
              </label>
            }

            @if (ready()) {
            <qt-collapsible-card
              title="Character Vaults"
              [description]="everywhere() ? 'Every character vault' : 'Vaults of characters in this chat'"
              icon="characters"
              [defaultOpen]="buckets().characterVaults.length > 0"
            >
              <div class="space-y-0.5">
                @for (store of buckets().characterVaults; track store.mountPointId) {
                  <ng-container [ngTemplateOutlet]="storeRow" [ngTemplateOutletContext]="{ $implicit: store }" />
                } @empty {
                  <div class="px-3 py-2 text-sm qt-text-muted">None available here.</div>
                }
              </div>
            </qt-collapsible-card>

            @if (buckets().groupStores.length > 0) {
              <qt-collapsible-card
                title="Group Files"
                description="Stores shared with characters' groups"
                icon="users"
                [defaultOpen]="true"
              >
                <div class="space-y-0.5">
                  @for (store of buckets().groupStores; track store.mountPointId) {
                    <ng-container [ngTemplateOutlet]="storeRow" [ngTemplateOutletContext]="{ $implicit: store }" />
                  }
                </div>
              </qt-collapsible-card>
            }

            <qt-collapsible-card
              title="Database-backed document stores"
              [description]="everywhere() ? 'Every store kept inside Quilltap' : 'Project stores kept inside Quilltap'"
              icon="database"
              [defaultOpen]="buckets().databaseStores.length > 0"
            >
              <div class="space-y-0.5">
                @for (store of buckets().databaseStores; track store.mountPointId) {
                  <ng-container [ngTemplateOutlet]="storeRow" [ngTemplateOutletContext]="{ $implicit: store }" />
                } @empty {
                  <div class="px-3 py-2 text-sm qt-text-muted">None available here.</div>
                }
              </div>
            </qt-collapsible-card>

            <qt-collapsible-card
              title="Filesystem-backed document stores"
              [description]="everywhere() ? 'Every store backed by folders on disk' : 'Project stores backed by folders on disk'"
              icon="folder"
              [defaultOpen]="buckets().filesystemStores.length > 0"
            >
              <div class="space-y-0.5">
                @for (store of buckets().filesystemStores; track store.mountPointId) {
                  <ng-container [ngTemplateOutlet]="storeRow" [ngTemplateOutletContext]="{ $implicit: store }" />
                } @empty {
                  <div class="px-3 py-2 text-sm qt-text-muted">None available here.</div>
                }
              </div>
            </qt-collapsible-card>

            }
            @if (loading()) {
              <div class="text-center py-2 qt-text-muted text-sm">Loading stores…</div>
            }
          </div>
        </div>
      } @else {
        <div class="flex flex-col" style="height: 60vh">
          <div class="flex items-center gap-2 px-1 py-2 border-b qt-border-default">
            <button
              type="button"
              class="flex items-center gap-1 text-sm qt-text-secondary hover:qt-text-primary"
              (click)="back()"
            >
              <qt-icon name="arrow-left" class="w-4 h-4" />
              Back
            </button>
            <span class="text-sm qt-text-muted">{{ browseLabel() }}</span>
          </div>

          <div class="flex-1 overflow-y-auto">
            @if (selectedMount(); as mount) {
              <!-- The Scriptorium's folder browser, in pick mode: opening a
                   file opens it in Document Mode; its ⋮ menus stay. -->
              <div class="p-2">
                @for (id of [mount.id]; track id) {
                  <qt-document-store-browser
                    [storeId]="id"
                    [pickMode]="true"
                    [footer]="newHere"
                    [opensStores]="true"
                    (pickFile)="pickMountFile($event)"
                    (openStore)="openOtherStore($event)"
                  />
                }
              </div>
            }
          </div>
        </div>
        <ng-template #newHere let-folder>
          <button
            type="button"
            class="qt-button-secondary qt-button-sm inline-flex items-center gap-1.5"
            (click)="pickNewBlankInFolder(folder)"
          >
            <qt-icon name="file-plus" class="w-4 h-4" />
            New document here
          </button>
        </ng-template>
      }
    </qt-modal>

    <ng-template #storeRow let-store>
      <button
        type="button"
        class="w-full flex items-center gap-3 px-3 py-2 rounded-md hover:qt-bg-muted text-left"
        (click)="browseStore(store)"
      >
        <qt-icon name="file" class="w-4 h-4 flex-shrink-0 qt-text-secondary" />
        <span class="flex-1 min-w-0 text-sm qt-text-primary font-medium truncate" [title]="store.label">{{ store.label }}</span>
        <qt-icon name="chevron-right" class="w-3 h-3 flex-shrink-0 qt-text-muted" />
      </button>
    </ng-template>
  `,
})
export class DocumentPicker implements OnInit {
  // Chat mode uses the per-conversation DocumentApi (optional: absent on the
  // rail); standalone mode uses the root StandaloneDocumentApi.
  private readonly api = inject(DocumentApi, { optional: true });
  private readonly standaloneApi = inject(StandaloneDocumentApi);
  private readonly toasts = inject(ToastService);

  /** The chat whose reach scopes the picker; `null` = the standalone surface. */
  readonly chatId = input<string | null>(null);
  /** The chat's project name, for the browse header (v4 `projectName || 'Project'`). */
  readonly projectName = input<string | null>(null);

  readonly selectDocument = output<DocumentSelection>();
  readonly close = output<void>();

  /** True when there is no chat context (the rail Document-Mode entry). */
  protected readonly standalone = computed(() => this.chatId() == null);

  protected readonly maxRecent = MAX_RECENT_DOCUMENTS;

  protected readonly step = signal<'source' | 'browse'>('source');
  protected readonly recents = signal<RecentDocumentDto[]>([]);
  protected readonly stores = signal<AccessibleStoreDto[]>([]);
  /**
   * The chat project's own document store, which the accessible-store list
   * never carries (server-side it is moved out of `stores` into this field).
   * Null on the standalone surface, and on a chat with no project.
   */
  protected readonly projectLibrary = signal<ProjectLibraryTargetDto | null>(null);
  /** Quilltap General (C14, v4 `5efe53ea7`) — null until the server names it. */
  protected readonly generalLibrary = signal<LibraryTargetDto | null>(null);
  protected readonly loading = signal(false);
  /**
   * Gates the collapsible cards until the first fetch settles: a card seeds its
   * open/closed state from `defaultOpen` in its own `ngOnInit` (which fires
   * before the async load resolves), so mounting the cards only once the data
   * has landed reproduces v4's key-remount-on-data-arrival (the P4.6l lesson).
   */
  protected readonly ready = signal(false);
  protected readonly lookEverywhere = signal(false);
  /** Standalone is implicitly everywhere; chat mode follows the checkbox. */
  protected readonly everywhere = computed(() => this.standalone() || this.lookEverywhere());

  protected readonly selectedMount = signal<SelectedMount | null>(null);

  /** v4 `:470-472` — the browse header names the project, the General library, or the store. */
  protected readonly browseLabel = computed(() => {
    const mount = this.selectedMount();
    if (mount && mount.id === this.projectLibrary()?.mountPointId) return this.projectName() || 'Project';
    if (mount && mount.id === this.generalLibrary()?.mountPointId) return 'General Library';
    return mount?.name || 'Document Store';
  });

  ngOnInit(): void {
    void Promise.all([this.loadRecents(), this.loadStores()]).finally(() => this.ready.set(true));
  }

  private async loadRecents(): Promise<void> {
    try {
      const chatId = this.chatId();
      this.recents.set(
        chatId ? await this.api!.fetchRecentDocuments(chatId) : await this.standaloneApi.fetchRecent(),
      );
    } catch {
      // leave recents empty
    }
  }

  private async loadStores(): Promise<void> {
    this.loading.set(true);
    try {
      const chatId = this.chatId();
      if (chatId) {
        const result = await this.api!.fetchAccessibleStores(chatId, this.lookEverywhere());
        this.stores.set(result.stores);
        // Carried across a look-everywhere toggle: the server computes it in
        // both modes, and the project's own store is withheld from `stores` in
        // both, so dropping it here would make the button flicker away.
        this.projectLibrary.set(result.projectLibrary);
        this.generalLibrary.set(result.generalLibrary ?? null);
      } else {
        const result = await this.standaloneApi.fetchStores();
        this.stores.set(result.stores);
        this.generalLibrary.set(result.generalLibrary);
      }
    } catch {
      // leave stores empty
    } finally {
      this.loading.set(false);
    }
  }

  protected setLookEverywhere(value: boolean): void {
    this.lookEverywhere.set(value);
    void this.loadStores();
  }

  /** Bucket the accessible stores for the accordions (v4 `storeBuckets`). */
  protected readonly buckets = computed(() => {
    const all = this.stores();
    return {
      characterVaults: all.filter((s) => s.storeType === 'character'),
      groupStores: all.filter((s) => s.isGroupStore),
      databaseStores: all.filter(
        (s) => s.storeType === 'documents' && s.mountType === 'database' && !s.isGroupStore,
      ),
      filesystemStores: all.filter(
        (s) => s.storeType === 'documents' && s.mountType !== 'database' && !s.isGroupStore,
      ),
    };
  });

  /** v4 `handleSelectStore` — browse a store in the browser. */
  protected browseStore(store: AccessibleStoreDto): void {
    this.openMount(store.mountPointId, store.name);
  }

  /**
   * v4 `browseLibrary` — the project library (its official store) or the
   * General library, browsed as a store.
   */
  protected browseLibrary(library: LibraryTargetDto): void {
    this.openMount(library.mountPointId, library.name);
  }

  private openMount(mountPointId: string, name: string): void {
    this.selectedMount.set({ id: mountPointId, name });
    this.step.set('browse');
  }

  /**
   * v4 `handleOpenOtherStore` — a "where else this appears" Go there: a store
   * on the shelves is browsed, a library is browsed, anything else is out of
   * reach from this chat.
   */
  protected openOtherStore(mountPointId: string): void {
    const store = this.stores().find((s) => s.mountPointId === mountPointId);
    const library = [this.projectLibrary(), this.generalLibrary()].find((l) => l?.mountPointId === mountPointId);
    if (store) this.browseStore(store);
    else if (library) this.browseLibrary(library);
    else this.toasts.showError("That store is not on this chat's shelves. Tick Look everywhere to reach it.");
  }

  protected back(): void {
    this.step.set('source');
  }

  // --- selections ---

  protected pickNewBlank(): void {
    this.selectDocument.emit({});
  }

  protected pickRecent(doc: RecentDocumentDto): void {
    this.selectDocument.emit({ filePath: doc.filePath, scope: doc.scope, mountPoint: doc.mountPoint || undefined });
  }

  /** v4 `handleMountPointFileSelect` (`:205-212`). */
  protected pickMountFile(file: DocumentEntry): void {
    this.selectDocument.emit({
      filePath: file.relativePath,
      scope: 'document_store',
      mountPoint: this.selectedMount()?.name || undefined,
    });
  }

  /**
   * v4 `handleNewBlankInFolder` (`:217-225`): a brand-new "Untitled
   * Document.md" inside the folder being browsed — the server picks the actual
   * filename (with collision numbering); this hands it the scope and folder.
   */
  protected pickNewBlankInFolder(currentFolder: string): void {
    const mount = this.selectedMount();
    if (!mount) return;
    this.selectDocument.emit({
      scope: 'document_store',
      mountPoint: mount.name,
      targetFolder: currentFolder || undefined,
    });
  }

  protected recentLabel(doc: RecentDocumentDto): string {
    const scopeLabel =
      doc.scope === 'document_store' && doc.mountPoint
        ? doc.mountPoint
        : doc.scope === 'project'
          ? 'Project'
          : 'General';
    const action = doc.isActive ? 'Continue editing' : 'Reopen';
    return `${action} · ${scopeLabel}`;
  }
}
