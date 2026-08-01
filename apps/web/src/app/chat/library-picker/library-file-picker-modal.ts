import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../core/core-client';
import { apiUrl } from '../../core/api-url';
import { Modal } from '../../ui/modal';
import type { UploadedChatFile } from '../chat-files.api';
import { LibraryBrowsePanel, type BrowseScope } from './library-browse-panel';
import {
  attachMountFile,
  fetchPickerAlbums,
  fetchPickerGallery,
  fetchPickerGroupStores,
  fetchPickerMountPoints,
  fetchPickerProjects,
  libraryPickerKeys,
  linkLibraryFile,
} from './library-picker.api';
import {
  isMountFile,
  pickableDocStores,
  pickedFileName,
  resolveUserPersonaAlbum,
  type PickerFile,
  type PickerGalleryEntry,
  type PickerStoreSummary,
} from './library-picker.model';
import { ToastService } from '../../ui/toast.service';

/** A legacy library file linked to the chat. */
export interface LinkedLibraryFile {
  file: UploadedChatFile;
}

type Step = 'scope' | 'browse-project' | 'browse-gallery' | 'browse-mount';

/**
 * The Library file picker (v4 `components/chat/LibraryFilePickerModal.tsx`,
 * 616 LOC) — the composer gutter's "Attach file from library" entry, and the
 * last dialog the chat surface deferred by name.
 *
 * Two steps. **Scope** lists, in v4's render order: General and the gallery
 * (always), then Group Files, then Projects, then Document Stores. **Browse**
 * hands a project / store scope to `qt-library-browse-panel`, or renders the
 * bespoke gallery grid.
 *
 * ## The two completion callbacks do DIFFERENT things (v4 `:29-49`, `ChatModals :250-266`)
 *
 * - A **legacy** library file is LINKED (`?action=link`) and the result goes into
 *   the composer's pending-attachment tray, so the operator's next message
 *   carries it.
 * - A **document-store** file is PINNED via a Librarian attachment announcement
 *   (`?action=attach-mount-file`). There is no tray hand-off — the announcement
 *   is already a transcript message — so the parent only refetches the chat.
 *
 * The discriminator is the row itself (`isMountFile`), not the scope it came
 * from: a project scope can resolve to either mode. **Gallery picks always
 * attach**, since gallery entries are mount files by construction (`:299-320`).
 *
 * ## v5 divergences, both deliberate
 *
 * - **The browse panel is read-only** — see below.
 * - **The browse panel is read-only** — see `library-browse-panel.ts`'s header
 *   for the full reasoning and what it deliberately drops.
 *
 * v4 resets its state when `isOpen` goes false (`:161-171`); v5 MOUNTS the dialog
 * only while open (the merge-conversation precedent), which means the same thing
 * without a reset effect.
 */
@Component({
  selector: 'qt-library-file-picker-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { '(document:keydown.escape)': 'onEscape()' },
  imports: [Modal, LibraryBrowsePanel],
  template: `
    <qt-modal
      [title]="title()"
      maxWidth="4xl"
      [closeOnBackdrop]="!linking()"
      (close)="onClose()"
    >

      @switch (step()) {
        @case ('scope') {
          <div class="space-y-4">
            <section class="space-y-2">
              <button type="button" class="qt-card w-full text-left p-4 flex items-center gap-3"
                (click)="pickGeneral()">
                <span class="text-xl">{{ generalIcon }}</span>
                <span>
                  <span class="font-medium text-foreground block">General</span>
                  <span class="qt-text-muted text-sm">Files not assigned to any project</span>
                </span>
              </button>
              <button type="button" class="qt-card w-full text-left p-4 flex items-center gap-3"
                (click)="pickGallery()">
                <span class="text-xl">{{ galleryIcon }}</span>
                <span>
                  <span class="font-medium text-foreground block">{{ galleryTitle() }}</span>
                  <span class="qt-text-muted text-sm">{{ gallerySubtitle() }}</span>
                </span>
              </button>
            </section>

            @if (groupStores().length > 0) {
              <section class="space-y-2">
                <h3 class="qt-text-label">Group Files</h3>
                @for (store of groupStores(); track store.id) {
                  <button type="button" class="qt-card w-full text-left p-4 flex items-center gap-3"
                    (click)="pickMount(store)">
                    <span class="text-xl">{{ groupIcon }}</span>
                    <span>
                      <span class="font-medium text-foreground block">{{ store.name }}</span>
                      <span class="qt-text-muted text-sm">Shared with your group</span>
                    </span>
                  </button>
                }
              </section>
            }

            @if (groupStoresQuery.isPending()) {
              <p class="qt-text-secondary py-2 text-center text-sm">Loading groups…</p>
            }

            @if (projects().length > 0) {
              <section class="space-y-2">
                <h3 class="qt-text-label">Projects</h3>
                @for (project of projects(); track project.id) {
                  <button type="button" class="qt-card w-full text-left p-4 flex items-center gap-3"
                    (click)="pickProject(project.id, project.name)">
                    <span class="text-xl">{{ project.icon || projectIcon }}</span>
                    <span class="font-medium text-foreground">{{ project.name }}</span>
                  </button>
                }
              </section>
            }

            @if (projectsQuery.isPending()) {
              <p class="qt-text-secondary py-2 text-center text-sm">Loading projects…</p>
            }

            @if (docStores().length > 0) {
              <section class="space-y-2">
                <h3 class="qt-text-label">Document Stores</h3>
                @for (store of docStores(); track store.id) {
                  <button type="button" class="qt-card w-full text-left p-4 flex items-center gap-3"
                    (click)="pickMount(store)">
                    <span class="text-xl">{{ storeIcon }}</span>
                    <span>
                      <span class="font-medium text-foreground block">{{ store.name }}</span>
                      <span class="qt-text-muted text-sm">Database-backed store</span>
                    </span>
                  </button>
                }
              </section>
            }

            @if (mountsQuery.isPending()) {
              <p class="qt-text-secondary py-2 text-center text-sm">Loading document stores…</p>
            }
          </div>
        }

        @case ('browse-gallery') {
          @if (galleryQuery.isPending()) {
            <p class="qt-text-secondary py-8 text-center">Loading your gallery…</p>
          } @else if (galleryQuery.isError()) {
            <p class="qt-text-error py-8 text-center">
              Couldn&rsquo;t load gallery: {{ galleryError() }}
            </p>
          } @else if (galleryEntries().length === 0) {
            <p class="qt-text-muted py-8 text-center text-sm">{{ galleryEmptyCopy() }}</p>
          } @else {
            <div class="relative min-h-[50vh]">
              @if (linking()) {
                <div
                  class="absolute inset-0 z-10 flex items-center justify-center"
                  style="background: var(--qt-dialog-backdrop)"
                  data-testid="library-picker-linking"
                >
                  <p class="qt-text-secondary">Attaching…</p>
                </div>
              }
              <ul class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-3">
                @for (entry of galleryEntries(); track entry.linkId) {
                  <li>
                    <button
                      type="button"
                      class="qt-card w-full text-left p-2 disabled:opacity-50"
                      [disabled]="linking()"
                      [title]="entry.caption || entry.fileName"
                      (click)="onGalleryPick(entry)"
                    >
                      <img
                        [src]="blobSrc(entry)"
                        [alt]="entry.caption || entry.fileName"
                        loading="lazy"
                        class="w-full h-32 object-cover rounded"
                      />
                      <p class="qt-text-xs qt-text-muted mt-2 truncate">{{ entryLabel(entry) }}</p>
                    </button>
                  </li>
                }
              </ul>
            </div>
          }
        }

        @default {
          <qt-library-browse-panel
            [scope]="browseScope()!"
            [linking]="linking()"
            (pick)="onFileClick($event)"
          />
        }
      }

      <div qt-modal-footer class="flex" [class.justify-between]="step() !== 'scope'"
        [class.justify-end]="step() === 'scope'">
        @if (step() !== 'scope') {
          <button
            type="button"
            class="qt-button qt-button-secondary"
            [disabled]="linking()"
            (click)="onBack()"
          >
            Back
          </button>
        }
        <button
          type="button"
          class="qt-button qt-button-secondary"
          [disabled]="linking()"
          (click)="onClose()"
        >
          Cancel
        </button>
      </div>
    </qt-modal>
  `,
})
export class LibraryFilePickerModal {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  readonly chatId = input.required<string>();

  /** A legacy library file was linked — the parent puts it in the composer tray. */
  readonly fileLinked = output<LinkedLibraryFile>();
  /**
   * A document-store file was pinned via a Librarian announcement. No tray
   * hand-off: the parent refetches the chat so the announcement appears. Carries
   * v4's toast sentence.
   */
  readonly mountFileAttached = output<void>();
  readonly close = output<void>();

  protected readonly generalIcon = '\u{1F4C1}';
  protected readonly galleryIcon = '\u{1F5BC}\u{FE0F}';
  protected readonly groupIcon = '\u{1F465}';
  protected readonly projectIcon = '\u{1F4C2}';
  protected readonly storeIcon = '\u{1F4DA}';

  protected readonly step = signal<Step>('scope');
  protected readonly linking = signal(false);
  private readonly selectedScopeName = signal('General');
  private readonly selectedProjectId = signal<string | null>(null);
  private readonly selectedMount = signal<PickerStoreSummary | null>(null);

  // --- step 1's four reads (v4's four `enabled: isOpen` queries) -------------

  protected readonly projectsQuery = injectQuery(() => ({
    queryKey: [...libraryPickerKeys.scopes(this.chatId()), 'projects'],
    queryFn: () => fetchPickerProjects(this.core),
  }));
  protected readonly mountsQuery = injectQuery(() => ({
    queryKey: [...libraryPickerKeys.scopes(this.chatId()), 'mounts'],
    queryFn: () => fetchPickerMountPoints(this.core),
  }));
  protected readonly groupStoresQuery = injectQuery(() => ({
    queryKey: [...libraryPickerKeys.scopes(this.chatId()), 'group-stores'],
    queryFn: () => fetchPickerGroupStores(this.core, this.chatId()),
  }));
  private readonly albumsQuery = injectQuery(() => ({
    queryKey: [...libraryPickerKeys.scopes(this.chatId()), 'albums'],
    queryFn: () => fetchPickerAlbums(this.core, this.chatId()),
  }));

  protected readonly projects = computed(() => this.projectsQuery.data() ?? []);
  protected readonly groupStores = computed(() => this.groupStoresQuery.data() ?? []);
  /** Database stores that aren't private character vaults (v4 :133-135). */
  protected readonly docStores = computed(() => pickableDocStores(this.mountsQuery.data() ?? []));

  /**
   * The chat's user persona (v4 :156-159). Albums are read ONLY for this — to
   * name the gallery and to choose which photo endpoint it reads.
   */
  private readonly userPersonaAlbum = computed(() =>
    resolveUserPersonaAlbum(this.albumsQuery.data() ?? []),
  );

  protected readonly galleryTitle = computed(() => {
    const name = this.userPersonaAlbum()?.name;
    return name ? `${name}'s Photos` : 'My Gallery';
  });
  protected readonly gallerySubtitle = computed(() => {
    const name = this.userPersonaAlbum()?.name;
    return name ? `Photos saved to ${name}'s vault` : "Photos you've saved from chats";
  });

  // --- the gallery grid (v4 GalleryPanel, :538-608) --------------------------

  protected readonly galleryQuery = injectQuery(() => {
    const characterId = this.userPersonaAlbum()?.characterId ?? null;
    return {
      queryKey: libraryPickerKeys.gallery(characterId) as unknown as unknown[],
      queryFn: () => fetchPickerGallery(this.core, characterId),
      enabled: this.step() === 'browse-gallery',
    };
  });
  protected readonly galleryEntries = computed(() => this.galleryQuery.data() ?? []);

  protected galleryError(): string {
    const err = this.galleryQuery.error();
    return err instanceof Error ? err.message : String(err);
  }

  /** v4's two empty-gallery sentences (`:570-578`). */
  protected readonly galleryEmptyCopy = computed(() => {
    const persona = this.userPersonaAlbum();
    return persona
      ? `${persona.name}'s photo gallery is empty. Use Save Image on any chat message and pick ${persona.name} as the album to add photos here.`
      : 'Your gallery is empty. Save an image from any chat via "Save to my gallery" and it’ll appear here.';
  });

  /** v4 `PhotoCard`'s label chain — caption, else the prompt excerpt, else the name. */
  protected entryLabel(entry: PickerGalleryEntry): string {
    return entry.caption || entry.generationPromptExcerpt || entry.fileName;
  }

  /** `blobUrl` arrives server-RELATIVE (dogfood #12) — resolve it, never inline. */
  protected blobSrc(entry: PickerGalleryEntry): string {
    return apiUrl(entry.blobUrl);
  }

  // --- navigation -----------------------------------------------------------

  protected readonly title = computed(() =>
    this.step() === 'scope'
      ? 'Choose File Source'
      : `Browse Files — ${this.selectedScopeName()}`,
  );

  protected readonly browseScope = computed<BrowseScope | null>(() => {
    const step = this.step();
    if (step === 'browse-mount') {
      const mount = this.selectedMount();
      return mount ? { projectId: null, mountPoint: mount } : null;
    }
    if (step === 'browse-project') {
      // No `mountPoint` key at all — that is what tells the panel to
      // auto-resolve (v4 distinguishes `undefined` from `null`).
      return { projectId: this.selectedProjectId() };
    }
    return null;
  });

  protected pickGeneral(): void {
    this.selectedProjectId.set(null);
    this.selectedScopeName.set('General');
    this.selectedMount.set(null);
    this.step.set('browse-project');
  }

  protected pickProject(projectId: string, name: string): void {
    this.selectedProjectId.set(projectId);
    this.selectedScopeName.set(name);
    this.selectedMount.set(null);
    this.step.set('browse-project');
  }

  protected pickGallery(): void {
    this.selectedScopeName.set('My Gallery');
    this.step.set('browse-gallery');
  }

  protected pickMount(mount: PickerStoreSummary): void {
    this.selectedMount.set(mount);
    this.selectedScopeName.set(mount.name);
    this.step.set('browse-mount');
  }

  protected onBack(): void {
    this.step.set('scope');
    this.selectedMount.set(null);
  }

  protected onClose(): void {
    if (this.linking()) return;
    this.close.emit();
  }

  /** v4 passes `closeOnEscape={!linking}` (`:366`). */
  protected onEscape(): void {
    this.onClose();
  }

  // --- picking (v4 handleFileClick :228-297 / handleGalleryPick :299-320) ----

  protected async onFileClick(file: PickerFile): Promise<void> {
    if (this.linking()) return;
    const filename = pickedFileName(file);
    const mountFile = isMountFile(file);

    this.linking.set(true);
    try {
      if (mountFile) {
        await this.attach(file.mountPointId!, file.relativePath!, filename);
        return;
      }
      const linked = await linkLibraryFile(this.chatId(), file.id);
      this.toasts.showSuccess(`Linked "${filename}" to chat`);
      this.fileLinked.emit({ file: linked });
      this.close.emit();
    } catch (err) {
      this.toasts.showError(errorText(err) || 'Failed to attach file');
    } finally {
      this.linking.set(false);
    }
  }

  /** Gallery entries are mount files by construction — they ALWAYS attach. */
  protected async onGalleryPick(entry: PickerGalleryEntry): Promise<void> {
    if (this.linking()) return;
    const displayName = entry.caption || entry.fileName;
    this.linking.set(true);
    try {
      await this.attach(entry.mountPointId, entry.relativePath, displayName);
    } catch (err) {
      this.toasts.showError(errorText(err) || 'Failed to attach photo');
    } finally {
      this.linking.set(false);
    }
  }

  private async attach(
    mountPointId: string,
    relativePath: string,
    displayName: string,
  ): Promise<void> {
    await attachMountFile(this.chatId(), mountPointId, relativePath);
    this.toasts.showSuccess(`Attached "${displayName}" — the Librarian has noted it`);
    this.mountFileAttached.emit();
    this.close.emit();
  }
}

function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
