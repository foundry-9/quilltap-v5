import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  input,
  signal,
} from '@angular/core';
import { NgTemplateOutlet } from '@angular/common';

import { CoreClient } from '../../core/core-client';
import { Icon } from '../../ui/icon';
import { formatBytes } from '../../ui/format-bytes';
import { FileManagerAdapter, type AdapterCallbacks } from '../file-manager-adapter';
import { CoreFileManagerTransport, loadMountTree } from '../file-manager-transport';
import type { ResolvedNode } from '../event-wire-map';
import { relBasename, relDirname, relJoin } from '../node-id';
import type { FileNode, MountCapabilities, MountType } from '../types';

/** What the cut/copy clipboard is holding, and the intended paste mode. */
interface Clipboard {
  node: ResolvedNode;
  mode: 'move' | 'copy';
  /** The basename, for the "Paste <name> here" affordance. */
  name: string;
}

/**
 * `qt-file-manager` — the reusable document-store file browser (work order
 * P4.6aa, the D18 bespoke build; the ngx-explorer spike ran GREEN on its gating
 * checks but was rejected — no move/copy verb — see the order status header).
 *
 * It owns its own load/reload choreography (the v4 `SvarFileManager` shape) over
 * the ported adapter helpers: a navigable folder tree + a listing over
 * `mountFilesList`, with rename / create-folder / delete / move / copy within
 * the mount / upload / open / download — every affordance capability-gated by
 * the SERVER-derived `capabilities` bag (never re-derived here). Move/copy use a
 * cut/copy → "paste here" clipboard (ngx-explorer couldn't express the gesture;
 * this is the bespoke build's answer). The two backend gaps — copy-folder and
 * cross-mount folder move — surface as v4's steampunk refusals, never a request.
 *
 * Styling is native `qt-*` (the bespoke-build payoff: no second theming engine,
 * no icon-font swap — the exact D18 cost avoided).
 */
@Component({
  selector: 'qt-file-manager',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, NgTemplateOutlet],
  template: `
    <div class="qt-file-manager flex flex-col" data-testid="qt-file-manager" style="height: 70vh">
      @if (indexing()) {
        <div class="qt-alert-info mb-2 flex items-center justify-between gap-3 rounded-md p-2 text-sm">
          <span data-testid="fm-indexing"
            >That document is being re-indexed so its words become searchable — it’ll be along
            shortly.</span
          >
          <button class="qt-button-ghost" (click)="indexing.set(false)" aria-label="Dismiss">✕</button>
        </div>
      }
      @if (error()) {
        <div
          class="qt-alert-warning mb-2 flex items-center justify-between gap-3 rounded-md p-2 text-sm"
          data-testid="fm-error"
        >
          <span>{{ error() }}</span>
          <button class="qt-button-ghost" (click)="error.set(null)" aria-label="Dismiss">✕</button>
        </div>
      }

      <!-- Toolbar: breadcrumbs + create/upload/paste -->
      <div class="flex flex-wrap items-center gap-2 border-b qt-border-default px-1 pb-2">
        <div class="flex flex-1 items-center gap-1 text-xs qt-text-muted" data-testid="fm-breadcrumbs">
          <button type="button" class="hover:qt-text-primary" (click)="goTo('')">Root</button>
          @for (segment of breadcrumbs(); track $index) {
            <span>/</span>
            <button type="button" class="hover:qt-text-primary" (click)="goToCrumb($index)">
              {{ segment }}
            </button>
          }
        </div>

        @if (capabilities().canCreateFolder) {
          @if (newFolderOpen()) {
            <span class="flex items-center gap-1">
              <input
                #nf
                class="qt-input h-7 w-40 text-sm"
                placeholder="Folder name"
                data-testid="fm-new-folder-input"
                (keyup.enter)="submitNewFolder(nf.value)"
                (keyup.escape)="newFolderOpen.set(false)"
              />
              <button
                type="button"
                class="qt-button-secondary h-7 px-2 text-xs"
                data-testid="fm-new-folder-confirm"
                (click)="submitNewFolder(nf.value)"
              >
                Add
              </button>
              <button type="button" class="qt-button-ghost h-7 px-2 text-xs" (click)="newFolderOpen.set(false)">
                Cancel
              </button>
            </span>
          } @else {
            <button
              type="button"
              class="qt-button-secondary flex h-7 items-center gap-1 px-2 text-xs"
              data-testid="fm-new-folder"
              (click)="newFolderOpen.set(true)"
            >
              <qt-icon name="folder-plus" class="h-4 w-4" />
              New folder
            </button>
          }
        }

        @if (capabilities().canWrite) {
          <button
            type="button"
            class="qt-button-secondary flex h-7 items-center gap-1 px-2 text-xs"
            data-testid="fm-upload"
            (click)="fileInput.click()"
          >
            <qt-icon name="upload" class="h-4 w-4" />
            Upload
          </button>
          <input
            #fileInput
            type="file"
            class="hidden"
            data-testid="fm-file-input"
            (change)="onUpload($any($event.target))"
          />
        }

        @if (clipboard(); as clip) {
          <button
            type="button"
            class="qt-button-primary flex h-7 items-center gap-1 px-2 text-xs"
            data-testid="fm-paste"
            (click)="pasteHere()"
          >
            <qt-icon [name]="clip.mode === 'copy' ? 'copy' : 'swap'" class="h-4 w-4" />
            {{ clip.mode === 'copy' ? 'Copy' : 'Move' }} “{{ clip.name }}” here
          </button>
          <button type="button" class="qt-button-ghost h-7 px-2 text-xs" (click)="clipboard.set(null)">
            Clear
          </button>
        }
      </div>

      <!-- Listing -->
      <div class="flex-1 overflow-y-auto p-1" data-testid="fm-listing">
        @if (loading()) {
          <div class="py-10 text-center text-sm qt-text-muted" data-testid="fm-loading">
            Summoning the file manager…
          </div>
        } @else {
          @if (currentFolder()) {
            <button
              type="button"
              class="flex w-full items-center gap-3 rounded-md px-3 py-2 text-left hover:qt-bg-muted"
              (click)="navigateUp()"
            >
              <qt-icon name="arrow-left" class="h-4 w-4 flex-shrink-0 qt-text-secondary" />
              <span class="text-sm qt-text-secondary">..</span>
            </button>
          }

          @for (folder of entries().folders; track folder.path) {
            <div
              class="group flex items-center gap-3 rounded-md px-3 py-2 hover:qt-bg-muted"
              data-testid="fm-folder-row"
            >
              @if (renamingPath() === folder.path) {
                <ng-container
                  [ngTemplateOutlet]="renameEditor"
                  [ngTemplateOutletContext]="{ $implicit: folder }"
                />
              } @else {
                <button
                  type="button"
                  class="flex min-w-0 flex-1 items-center gap-3 text-left"
                  (click)="navigateInto(folder)"
                >
                  <qt-icon name="folder" class="h-4 w-4 flex-shrink-0 qt-text-secondary" />
                  <span class="min-w-0 flex-1 truncate text-sm font-medium qt-text-primary">{{
                    basename(folder.path)
                  }}</span>
                </button>
                <ng-container
                  [ngTemplateOutlet]="rowActions"
                  [ngTemplateOutletContext]="{ $implicit: folder }"
                />
              }
            </div>
          }

          @for (file of entries().files; track file.path) {
            <div
              class="group flex items-center gap-3 rounded-md px-3 py-2 hover:qt-bg-muted"
              data-testid="fm-file-row"
            >
              @if (renamingPath() === file.path) {
                <ng-container
                  [ngTemplateOutlet]="renameEditor"
                  [ngTemplateOutletContext]="{ $implicit: file }"
                />
              } @else {
                <button
                  type="button"
                  class="flex min-w-0 flex-1 items-center gap-3 text-left"
                  [title]="file.path"
                  (click)="open(file)"
                >
                  <qt-icon name="file" class="h-4 w-4 flex-shrink-0 qt-text-secondary" />
                  <span class="min-w-0 flex-1 truncate text-sm qt-text-primary">{{
                    basename(file.path)
                  }}</span>
                  @if (file.size != null) {
                    <span class="flex-shrink-0 text-xs qt-text-muted">{{ size(file.size) }}</span>
                  }
                </button>
                <ng-container
                  [ngTemplateOutlet]="rowActions"
                  [ngTemplateOutletContext]="{ $implicit: file }"
                />
              }
            </div>
          }

          @if (entries().folders.length === 0 && entries().files.length === 0) {
            <div class="py-8 text-center text-sm qt-text-muted" data-testid="fm-empty">
              This drawer stands empty.
            </div>
          }
        }
      </div>
    </div>

    <!-- Per-row action glyphs (capability-gated) -->
    <ng-template #rowActions let-node>
      <span class="flex flex-shrink-0 items-center gap-1 opacity-0 group-hover:opacity-100">
        @if (node.type === 'file') {
          <button
            type="button"
            class="qt-button-ghost h-6 w-6"
            title="Download"
            data-testid="fm-download"
            (click)="download(node)"
          >
            <qt-icon name="download" class="h-4 w-4" />
          </button>
        }
        @if (capabilities().canMoveOut) {
          <button
            type="button"
            class="qt-button-ghost h-6 w-6"
            title="Move"
            data-testid="fm-cut"
            (click)="cut(node, 'move')"
          >
            <qt-icon name="swap" class="h-4 w-4" />
          </button>
          <button
            type="button"
            class="qt-button-ghost h-6 w-6"
            title="Copy"
            data-testid="fm-copy"
            (click)="cut(node, 'copy')"
          >
            <qt-icon name="copy" class="h-4 w-4" />
          </button>
        }
        @if (capabilities().canWrite) {
          <button
            type="button"
            class="qt-button-ghost h-6 w-6"
            title="Rename"
            data-testid="fm-rename"
            (click)="startRename(node)"
          >
            <qt-icon name="pencil" class="h-4 w-4" />
          </button>
        }
        @if (capabilities().canDelete) {
          <button
            type="button"
            class="qt-button-ghost h-6 w-6"
            title="Delete"
            data-testid="fm-delete"
            (click)="remove(node)"
          >
            <qt-icon name="trash" class="h-4 w-4" />
          </button>
        }
      </span>
    </ng-template>

    <ng-template #renameEditor let-node>
      <qt-icon
        [name]="node.type === 'folder' ? 'folder' : 'file'"
        class="h-4 w-4 flex-shrink-0 qt-text-secondary"
      />
      <input
        #rn
        class="qt-input h-7 min-w-0 flex-1 text-sm"
        data-testid="fm-rename-input"
        [value]="basename(node.path)"
        (keyup.enter)="submitRename(node, rn.value)"
        (keyup.escape)="renamingPath.set(null)"
      />
      <button
        type="button"
        class="qt-button-secondary h-7 px-2 text-xs"
        data-testid="fm-rename-confirm"
        (click)="submitRename(node, rn.value)"
      >
        Rename
      </button>
      <button type="button" class="qt-button-ghost h-7 px-2 text-xs" (click)="renamingPath.set(null)">
        Cancel
      </button>
    </ng-template>
  `,
})
export class FileManager {
  private readonly core = inject(CoreClient);
  private readonly transport = new CoreFileManagerTransport(this.core);

  /** The mount whose files this pane browses. */
  readonly mountPointId = input.required<string>();
  /** The SERVER-derived capability bag (`mountPointGet`) — never re-derived here. */
  readonly capabilities = input.required<MountCapabilities>();
  /** The mount's kind (contract §2; kept for future kind-specific affordances). */
  readonly mountType = input.required<MountType>();

  protected readonly nodes = signal<FileNode[]>([]);
  protected readonly loading = signal(true);
  protected readonly error = signal<string | null>(null);
  protected readonly indexing = signal(false);
  protected readonly currentFolder = signal('');
  protected readonly clipboard = signal<Clipboard | null>(null);
  protected readonly renamingPath = signal<string | null>(null);
  protected readonly newFolderOpen = signal(false);

  private readonly callbacks: AdapterCallbacks = {
    onError: (message) => this.error.set(message),
    onReloadNeeded: () => void this.reload(),
    onIndexing: () => this.indexing.set(true),
  };

  /** The adapter, rebuilt when the mount or its capabilities change. */
  private readonly adapter = computed(
    () =>
      new FileManagerAdapter(
        this.transport,
        { mountId: this.mountPointId(), capabilities: this.capabilities() },
        this.callbacks,
      ),
  );

  constructor() {
    // Load (and reset navigation) whenever the mount changes.
    effect(() => {
      this.mountPointId();
      this.currentFolder.set('');
      this.clipboard.set(null);
      this.renamingPath.set(null);
      void this.reload();
    });
  }

  protected async reload(): Promise<void> {
    this.loading.set(true);
    try {
      this.nodes.set(await loadMountTree(this.core, this.mountPointId()));
    } catch {
      this.error.set('I couldn’t read that repository just now. Do try again.');
    } finally {
      this.loading.set(false);
    }
  }

  /** Folders + files whose parent is the current folder (v4 `mountPointEntries`). */
  protected readonly entries = computed<{ folders: FileNode[]; files: FileNode[] }>(() => {
    const cur = this.currentFolder();
    const folders: FileNode[] = [];
    const files: FileNode[] = [];
    for (const n of this.nodes()) {
      if (relDirname(n.path) !== cur) continue;
      (n.type === 'folder' ? folders : files).push(n);
    }
    const byName = (a: FileNode, b: FileNode) =>
      this.basename(a.path).localeCompare(this.basename(b.path));
    folders.sort(byName);
    files.sort(byName);
    return { folders, files };
  });

  protected readonly breadcrumbs = computed(() =>
    this.currentFolder() ? this.currentFolder().split('/') : [],
  );

  // ── Navigation ────────────────────────────────────────────────────────────
  protected navigateInto(folder: FileNode): void {
    this.currentFolder.set(folder.path);
  }
  protected navigateUp(): void {
    this.currentFolder.set(relDirname(this.currentFolder()));
  }
  protected goTo(path: string): void {
    this.currentFolder.set(path);
  }
  protected goToCrumb(index: number): void {
    this.currentFolder.set(
      this.breadcrumbs()
        .slice(0, index + 1)
        .join('/'),
    );
  }

  // ── Mutations ─────────────────────────────────────────────────────────────
  protected submitNewFolder(name: string): void {
    const trimmed = name.trim();
    if (!trimmed) return;
    this.newFolderOpen.set(false);
    void this.adapter().createFolder(this.currentFolderNode(), trimmed);
  }

  protected onUpload(el: HTMLInputElement): void {
    const file = el.files?.[0];
    el.value = ''; // allow re-selecting the same file
    if (file) void this.adapter().uploadFile(this.currentFolderNode(), file);
  }

  protected startRename(node: FileNode): void {
    this.renamingPath.set(node.path);
  }
  protected submitRename(node: FileNode, newName: string): void {
    const trimmed = newName.trim();
    this.renamingPath.set(null);
    if (!trimmed || trimmed === this.basename(node.path)) return;
    void this.adapter().rename(this.resolved(node), trimmed);
  }

  protected remove(node: FileNode): void {
    void this.adapter().delete([this.resolved(node)]);
  }

  protected cut(node: FileNode, mode: 'move' | 'copy'): void {
    this.clipboard.set({ node: this.resolved(node), mode, name: this.basename(node.path) });
  }

  protected pasteHere(): void {
    const clip = this.clipboard();
    if (!clip) return;
    this.clipboard.set(null);
    const dest = this.currentFolderNode() ?? this.rootNode();
    // Skip a no-op paste into the item's own directory.
    if (relJoin(dest.relativePath, clip.name) === clip.node.relativePath) return;
    if (clip.mode === 'copy') void this.adapter().copy([clip.node], dest);
    else void this.adapter().move([clip.node], dest);
  }

  // ── Reads (navigate the browser to the raw item route) ────────────────────
  protected open(node: FileNode): void {
    const url = this.adapter().openUrl(this.resolved(node));
    if (url) window.open(url, '_blank', 'noopener');
  }
  protected download(node: FileNode): void {
    const url = this.adapter().downloadUrl(this.resolved(node));
    if (url) window.open(url, '_blank', 'noopener');
  }

  // ── Helpers ───────────────────────────────────────────────────────────────
  private resolved(node: FileNode): ResolvedNode {
    return { mountId: this.mountPointId(), relativePath: node.path, type: node.type };
  }
  private rootNode(): ResolvedNode {
    return { mountId: this.mountPointId(), relativePath: '', type: 'folder' };
  }
  /** The current folder as a resolved node, or null at the root (quirk 4). */
  private currentFolderNode(): ResolvedNode | null {
    const cur = this.currentFolder();
    return cur ? { mountId: this.mountPointId(), relativePath: cur, type: 'folder' } : null;
  }

  protected basename(path: string): string {
    return relBasename(path);
  }
  protected size(bytes: number): string {
    return formatBytes(bytes);
  }
}
