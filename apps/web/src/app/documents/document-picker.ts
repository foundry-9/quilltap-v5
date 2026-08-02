import { ChangeDetectionStrategy, Component, computed, inject, input, OnInit, output, signal } from '@angular/core';
import { NgTemplateOutlet } from '@angular/common';

import { Icon } from '../ui/icon';
import { Modal } from '../ui/modal';
import { CollapsibleCard } from '../ui/collapsible-card';
import { formatBytes } from '../ui/format-bytes';
import type {
  AccessibleStoreDto,
  DocMountFileDto,
  DocumentScope,
  ProjectLibraryTargetDto,
  RecentDocumentDto,
} from '../core/core-contract';
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
 * Two steps: (1) source — new blank, recents, and the store accordions
 * (+ look-everywhere); (2) browse — a mount point's folder tree over lane A's
 * `mountFilesList`, with folder navigation and "new document here".
 *
 * Serves BOTH entries v4's one modal serves (P4.9J4): a `chatId` scopes the
 * chat-side picker (recents/stores from the chat's reach via {@link DocumentApi});
 * a NULL `chatId` is the standalone (chat-less) rail surface — recents + stores
 * come from {@link StandaloneDocumentApi} (`/api/v1/documents`), every enabled
 * store shows ("look everywhere" is implicit, its checkbox hidden), exactly as
 * v4's `chatId === null` arm.
 *
 * DEFERRED LOUDLY this round (enumerated in the status header): the
 * project/general **FileBrowser** path (needs a project/general file-listing
 * endpoint the SPA doesn't consume this round) and the in-picker **new-folder**
 * control (needs `mountFolderCreate`, not a consumed variant). New blank
 * documents (project scope) and store browsing cover the round's opening paths.
 *
 * The deferral above once swallowed the **Project library** button whole
 * (dogfood #50): v4 renders it for either arm, but only its legacy-scope arm
 * needs the deferred endpoint — the store-backed arm is an ordinary mount
 * browse. Since the server withholds the chat project's own store from
 * `stores` on the strength of this button existing, its absence made that
 * store unreachable by any path. The button is now rendered for the
 * store-backed arm; a project with NO official store still falls to the
 * deferral, and shows no button rather than a broken one.
 */
@Component({
  selector: 'qt-document-picker',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, Icon, CollapsibleCard, NgTemplateOutlet],
  template: `
    <qt-modal
      [title]="step() === 'source' ? 'Open Document' : 'Select File'"
      maxWidth="3xl"
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
                (click)="browseProjectLibrary(library)"
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
            <span class="text-sm qt-text-muted">{{ selectedMount()?.name || 'Document Store' }}</span>
          </div>

          <div class="flex-1 overflow-y-auto p-2">
            @if (filesLoading()) {
              <div class="text-center py-8 qt-text-muted text-sm">Loading files...</div>
            } @else {
              <div class="space-y-0.5">
                @if (currentFolder()) {
                  <div class="flex items-center gap-1 px-3 py-1.5 mb-1 text-xs qt-text-muted">
                    <button class="hover:qt-text-primary" (click)="currentFolder.set('')">
                      {{ selectedMount()?.name || 'Root' }}
                    </button>
                    @for (segment of breadcrumbs(); track $index) {
                      <span class="flex items-center gap-1">
                        <span>/</span>
                        <button class="hover:qt-text-primary" (click)="navigateToCrumb($index)">{{ segment }}</button>
                      </span>
                    }
                  </div>

                  <button
                    type="button"
                    class="w-full flex items-center gap-3 px-3 py-2 rounded-md hover:qt-bg-muted text-left"
                    (click)="navigateUp()"
                  >
                    <qt-icon name="arrow-left" class="w-4 h-4 flex-shrink-0 qt-text-secondary" />
                    <span class="text-sm qt-text-secondary">..</span>
                  </button>
                }

                <button
                  type="button"
                  class="w-full flex items-center gap-3 px-3 py-2 rounded-md hover:qt-bg-muted text-left"
                  (click)="pickNewBlankInFolder()"
                >
                  <qt-icon name="file-plus" class="w-4 h-4 flex-shrink-0 qt-text-secondary" />
                  <span class="text-sm qt-text-secondary">New document here</span>
                </button>

                @for (folder of entries().folders; track folder) {
                  <button
                    type="button"
                    class="w-full flex items-center gap-3 px-3 py-2 rounded-md hover:qt-bg-muted text-left"
                    (click)="navigateInto(folder)"
                  >
                    <qt-icon name="folder" class="w-4 h-4 flex-shrink-0 qt-text-secondary" />
                    <span class="flex-1 min-w-0 text-sm qt-text-primary font-medium truncate">{{ folder }}</span>
                    <qt-icon name="chevron-right" class="w-3 h-3 flex-shrink-0 qt-text-muted" />
                  </button>
                }

                @for (file of entries().files; track file.id) {
                  <button
                    type="button"
                    class="w-full flex items-center gap-3 px-3 py-2 rounded-md hover:qt-bg-muted text-left"
                    (click)="pickMountFile(file)"
                  >
                    <qt-icon name="file" class="w-4 h-4 flex-shrink-0 qt-text-secondary" />
                    <span class="flex-1 min-w-0 text-sm qt-text-primary truncate" [title]="file.relativePath">
                      {{ basename(file.relativePath) }}
                    </span>
                    @if (file.size != null) {
                      <span class="text-xs qt-text-muted flex-shrink-0">{{ formatSize(file.size) }}</span>
                    }
                  </button>
                }

                @if (entries().folders.length === 0 && entries().files.length === 0) {
                  <div class="text-center py-4 qt-text-muted text-sm">This folder is empty.</div>
                }
              </div>
            }
          </div>
        </div>
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

  /** The chat whose reach scopes the picker; `null` = the standalone surface. */
  readonly chatId = input<string | null>(null);

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
  protected readonly mountFiles = signal<DocMountFileDto[]>([]);
  protected readonly mountFolders = signal<string[]>([]);
  protected readonly filesLoading = signal(false);
  protected readonly currentFolder = signal('');

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
      } else {
        this.stores.set(await this.standaloneApi.fetchStores());
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

  protected browseStore(store: AccessibleStoreDto): Promise<void> {
    return this.openMount(store.mountPointId, store.name);
  }

  /**
   * v4 routes the Project library button to the project's official store when
   * it has one (`handleSelectScope('document_store', …)`), which is the same
   * mount browse a store row performs. v4's other arm — the legacy `project`
   * scope, taken when a project has NO official store — remains the deferred
   * FileBrowser path named in this file's header.
   */
  protected browseProjectLibrary(library: ProjectLibraryTargetDto): Promise<void> {
    return this.openMount(library.mountPointId, library.name);
  }

  private async openMount(mountPointId: string, name: string): Promise<void> {
    this.selectedMount.set({ id: mountPointId, name });
    this.step.set('browse');
    this.currentFolder.set('');
    this.filesLoading.set(true);
    try {
      const chatId = this.chatId();
      const result = chatId
        ? await this.api!.listMountFiles(mountPointId)
        : await this.standaloneApi.listMountFiles(mountPointId);
      this.mountFiles.set(result.files);
      this.mountFolders.set(result.folders);
    } catch {
      this.mountFiles.set([]);
      this.mountFolders.set([]);
    } finally {
      this.filesLoading.set(false);
    }
  }

  protected back(): void {
    this.step.set('source');
    this.mountFiles.set([]);
    this.mountFolders.set([]);
  }

  /** Folders + files at the current folder level (v4 `mountPointEntries`). */
  protected readonly entries = computed<{ folders: string[]; files: DocMountFileDto[] }>(() => {
    const folder = this.currentFolder();
    const prefix = folder ? folder + '/' : '';
    const folderSet = new Set<string>();
    const filesAtLevel: DocMountFileDto[] = [];

    for (const file of this.mountFiles()) {
      if (!file.relativePath.startsWith(prefix)) continue;
      const remainder = file.relativePath.substring(prefix.length);
      const slash = remainder.indexOf('/');
      if (slash === -1) filesAtLevel.push(file);
      else folderSet.add(remainder.substring(0, slash));
    }

    for (const fullPath of this.mountFolders()) {
      const lastSlash = fullPath.lastIndexOf('/');
      const parent = lastSlash === -1 ? '' : fullPath.substring(0, lastSlash);
      const name = lastSlash === -1 ? fullPath : fullPath.substring(lastSlash + 1);
      if (parent === folder && name) folderSet.add(name);
    }

    const folders = Array.from(folderSet).sort((a, b) => a.localeCompare(b));
    filesAtLevel.sort((a, b) => this.basename(a.relativePath).localeCompare(this.basename(b.relativePath)));
    return { folders, files: filesAtLevel };
  });

  protected readonly breadcrumbs = computed(() => {
    const folder = this.currentFolder();
    return folder ? folder.split('/') : [];
  });

  protected navigateInto(folder: string): void {
    this.currentFolder.update((prev) => (prev ? `${prev}/${folder}` : folder));
  }

  protected navigateUp(): void {
    this.currentFolder.update((prev) => {
      const lastSlash = prev.lastIndexOf('/');
      return lastSlash === -1 ? '' : prev.substring(0, lastSlash);
    });
  }

  protected navigateToCrumb(index: number): void {
    this.currentFolder.set(this.breadcrumbs().slice(0, index + 1).join('/'));
  }

  // --- selections ---

  protected pickNewBlank(): void {
    this.selectDocument.emit({});
  }

  protected pickRecent(doc: RecentDocumentDto): void {
    this.selectDocument.emit({ filePath: doc.filePath, scope: doc.scope, mountPoint: doc.mountPoint || undefined });
  }

  protected pickMountFile(file: DocMountFileDto): void {
    this.selectDocument.emit({
      filePath: file.relativePath,
      scope: 'document_store',
      mountPoint: this.selectedMount()?.name || undefined,
    });
  }

  protected pickNewBlankInFolder(): void {
    this.selectDocument.emit({
      scope: 'document_store',
      mountPoint: this.selectedMount()?.name || undefined,
      targetFolder: this.currentFolder() || undefined,
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

  protected basename(path: string): string {
    return path.split('/').pop() || path;
  }

  protected formatSize(bytes: number): string {
    return formatBytes(bytes);
  }
}
