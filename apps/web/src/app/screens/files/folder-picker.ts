import {
  afterRenderEffect,
  ChangeDetectionStrategy,
  Component,
  computed,
  ElementRef,
  inject,
  input,
  output,
  signal,
  viewChild,
} from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../core/core-client';
import type { FolderEntry } from '../../core/core-contract';
import { projectKeys } from '../prospero/projects.api';

/** One row of the dropdown (v4 `FolderPicker.tsx` `FolderInfo`). */
interface FolderInfo {
  path: string;
  name: string;
  depth: number;
  fileCount: number;
  id?: string;
  isDbFolder?: boolean;
}

/** Only `folderPath` is read off a file row (v4's own narrowing). */
interface FolderPathOnly {
  folderPath?: string | null;
}

/**
 * Folder dropdown for Move to Project (v4 `components/files/FolderPicker.tsx`,
 * in its bug-113-fixed shape at `a00e18f0d`).
 *
 * The list is DERIVED — a `computed` over the two queries, rendered directly.
 * v4's pre-fix version mirrored the derivation into component state behind an
 * "only if empty" guard; Root is seeded unconditionally, before any data is
 * consulted, so the still-loading first render produced a one-entry list that
 * satisfied the guard and sealed the mirror against every later update,
 * including a change of destination. An `effect` that writes a folders signal
 * would reintroduce exactly that latch, so there is none here: the queries are
 * the only state, and a destination change re-derives by construction.
 *
 * The one piece of state left is `localFolders` — folders created while the
 * create call was unreachable — scoped to the project they were created under,
 * so switching destinations drops them rather than offering a folder that
 * belongs to somewhere else.
 */
@Component({
  selector: 'qt-folder-picker',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { class: 'block' },
  template: `
    <div class="space-y-2">
      <div class="flex gap-2">
        <select
          #folderSelect
          id="move-folder"
          class="qt-select flex-1"
          [attr.data-qt-value]="value()"
          [disabled]="disabled() || loading()"
          (change)="valueChange.emit($any($event.target).value)"
        >
          @if (loading()) {
            <option value="/">Loading...</option>
          } @else {
            @for (folder of folders(); track folder.path) {
              <!--
                Non-breaking spaces: an <option> collapses ordinary whitespace, so
                plain spaces left every depth looking identical.
              -->
              <option [value]="folder.path">{{ optionLabel(folder) }}</option>
            }
          }
        </select>
        <button
          type="button"
          class="qt-button qt-button-secondary px-3"
          title="Create new folder"
          [disabled]="disabled()"
          (click)="showNewFolderInput.set(!showNewFolderInput())"
        >
          +
        </button>
      </div>

      @if (showNewFolderInput()) {
        <div class="flex gap-2">
          <input
            type="text"
            class="qt-input flex-1"
            placeholder="/path/to/folder/"
            [value]="newFolderInput()"
            [disabled]="disabled()"
            (input)="newFolderInput.set($any($event.target).value)"
            (keydown)="onKeydown($event)"
          />
          <button
            type="button"
            class="qt-button qt-button-primary px-3"
            [disabled]="disabled() || !newFolderInput().trim()"
            (click)="createFolder()"
          >
            Create
          </button>
        </div>
      }
    </div>
  `,
})
export class FolderPicker {
  private readonly core = inject(CoreClient);

  /** Current selected folder path. */
  readonly value = input.required<string>();
  /** Project ID to list folders from (null for general files). */
  readonly projectId = input.required<string | null>();
  /** Whether the picker is disabled. */
  readonly disabled = input(false);

  /** v4's `onChange(path)`. */
  readonly valueChange = output<string>();

  protected readonly newFolderInput = signal('');
  protected readonly showNewFolderInput = signal(false);
  /**
   * Folders created while the create call was unreachable, scoped to the project
   * they were created under (v4 `:44-47`).
   */
  private readonly localFolders = signal<{ projectId: string | null; paths: string[] }>({
    projectId: null,
    paths: [],
  });

  private readonly filesQuery = injectQuery(() => {
    const projectId = this.projectId();
    return {
      queryKey: projectId ? projectKeys.files(projectId) : ['files', 'general'],
      queryFn: async (): Promise<FolderPathOnly[]> => {
        const data = await this.core.dispatchData(
          projectId ? { type: 'filesList', projectId } : { type: 'filesList', filter: 'general' },
        );
        return (data['files'] as FolderPathOnly[] | undefined) ?? [];
      },
    };
  });

  private readonly foldersQuery = injectQuery(() => {
    const projectId = this.projectId();
    return {
      queryKey: ['files', 'folders', projectId ?? 'general'],
      queryFn: async (): Promise<FolderEntry[]> => {
        const data = await this.core.dispatchData(
          projectId ? { type: 'filesFoldersList', projectId } : { type: 'filesFoldersList' },
        );
        return (data['folders'] as FolderEntry[] | undefined) ?? [];
      },
    };
  });

  protected readonly loading = computed(
    () => this.filesQuery.isPending() || this.foldersQuery.isPending(),
  );

  private readonly files = computed(() => this.filesQuery.data() ?? []);
  private readonly dbFolders = computed(() => this.foldersQuery.data() ?? []);
  private readonly localPaths = computed(() => {
    const local = this.localFolders();
    return local.projectId === this.projectId() ? local.paths : [];
  });

  /**
   * v4 `:94-158` — the derivation, in v4's exact order: Root, then the DB
   * folders, then every folder a file's `folderPath` implies AND each of its
   * ancestors, then the local fallbacks; sorted by `path.localeCompare`.
   */
  protected readonly folders = computed<FolderInfo[]>(() => {
    const files = this.files();
    const dbFolders = this.dbFolders();
    const localPaths = this.localPaths();

    const folderMap = new Map<string, FolderInfo>();
    const countFiles = (path: string) => files.filter((f) => (f.folderPath || '/') === path).length;

    // Always include root
    folderMap.set('/', {
      path: '/',
      name: 'Root',
      depth: 0,
      fileCount: countFiles('/'),
      isDbFolder: false,
    });

    // Add DB folders
    for (const dbFolder of dbFolders) {
      const depth = dbFolder.path.split('/').filter(Boolean).length;
      folderMap.set(dbFolder.path, {
        path: dbFolder.path,
        name: dbFolder.name,
        depth,
        fileCount: countFiles(dbFolder.path),
        id: dbFolder.id,
        isDbFolder: true,
      });
    }

    // Extract unique folder paths from files (for backwards compatibility)
    for (const file of files) {
      const path = file.folderPath || '/';
      if (!folderMap.has(path)) {
        const parts = path.split('/').filter(Boolean);
        const name = parts.length === 0 ? 'Root' : parts[parts.length - 1];
        folderMap.set(path, {
          path,
          name,
          depth: parts.length,
          fileCount: countFiles(path),
          isDbFolder: false,
        });
      }
      // Also add parent paths
      const parts = path.split('/').filter(Boolean);
      let current = '/';
      for (const part of parts) {
        current = current === '/' ? `/${part}/` : `${current}${part}/`;
        if (!folderMap.has(current)) {
          const depth = current.split('/').filter(Boolean).length;
          folderMap.set(current, {
            path: current,
            name: part,
            depth,
            fileCount: countFiles(current),
            isDbFolder: false,
          });
        }
      }
    }

    // Folders created locally after the create call failed
    for (const path of localPaths) {
      if (!folderMap.has(path)) {
        const parts = path.split('/').filter(Boolean);
        folderMap.set(path, {
          path,
          name: parts[parts.length - 1] ?? 'Folder',
          depth: parts.length,
          fileCount: 0,
          isDbFolder: false,
        });
      }
    }

    return Array.from(folderMap.values()).sort((a, b) => a.path.localeCompare(b.path));
  });

  private readonly selectRef = viewChild<ElementRef<HTMLSelectElement>>('folderSelect');

  constructor() {
    // v4's `<select value={value}>` is controlled: React assigns `select.value`
    // AFTER the children mount, so a value naming no option leaves the control
    // blank. Angular's `[value]`/`[selected]` run before the `@for` fills the
    // list, so the faithful port is a post-render assignment that re-runs
    // whenever the rows move under a selection that was already set (memory
    // `angular-select-cannot-mirror-react-controlled-value`; the
    // `scenario-select.ts` idiom).
    afterRenderEffect(() => {
      this.value();
      this.folders();
      this.loading();
      const select = this.selectRef()?.nativeElement;
      if (!select) return;
      select.value = select.dataset['qtValue'] ?? '';
    });
  }

  /**
   * v4 `:224-231` — two NON-BREAKING spaces per level below the first, then the
   * elbow, the name (Root spelled `/ (Root)`) and the file count when non-zero.
   */
  protected optionLabel(folder: FolderInfo): string {
    return (
      '\u00a0\u00a0'.repeat(Math.max(0, folder.depth - 1)) +
      (folder.depth > 0 ? '└ ' : '') +
      (folder.name === 'Root' ? '/ (Root)' : folder.name) +
      (folder.fileCount > 0 ? ` (${folder.fileCount} files)` : '')
    );
  }

  /** v4 `:184-193` — Enter submits, Escape closes and clears. */
  protected onKeydown(event: KeyboardEvent): void {
    if (event.key === 'Enter') {
      event.preventDefault();
      void this.createFolder();
    } else if (event.key === 'Escape') {
      this.showNewFolderInput.set(false);
      this.newFolderInput.set('');
    }
  }

  /**
   * v4 `:160-206`. On success the folder list is REFETCHED — copying the previous
   * render's snapshot could not have contained the folder just created. On
   * failure the path joins the project-scoped local fallback list and the
   * selection still moves to it.
   */
  protected async createFolder(): Promise<void> {
    if (!this.newFolderInput().trim()) return;

    // Normalize the new folder path
    let newPath = this.newFolderInput().trim();
    if (!newPath.startsWith('/')) newPath = '/' + newPath;
    if (!newPath.endsWith('/')) newPath = newPath + '/';

    const projectId = this.projectId();
    try {
      const data = await this.core.dispatchData({
        type: 'filesFolderCreate',
        path: newPath,
        projectId,
      });
      const folder = data['folder'] as { path?: string } | undefined;
      const folderPath = folder?.path || newPath;

      // Refresh folder list to include the new folder
      await this.foldersQuery.refetch();

      this.valueChange.emit(folderPath);
      this.newFolderInput.set('');
      this.showNewFolderInput.set(false);
    } catch (error) {
      console.error('[FolderPicker] Failed to create folder', {
        path: newPath,
        error: error instanceof Error ? error.message : String(error),
      });
      // Still add to local list as fallback
      const prev = this.localFolders();
      const paths = prev.projectId === projectId ? prev.paths : [];
      this.localFolders.set(
        paths.includes(newPath) ? { projectId, paths } : { projectId, paths: [...paths, newPath] },
      );
      this.valueChange.emit(newPath);
      this.newFolderInput.set('');
      this.showNewFolderInput.set(false);
    }
  }
}
