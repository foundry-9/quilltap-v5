import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  output,
  signal,
} from '@angular/core';
import { NgTemplateOutlet } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { RouterLink } from '@angular/router';

import { CoreClient } from '../../core/core-client';
import type {
  CustomToolLibraryEntry,
  CustomToolLibraryError,
  MountAttachment,
} from '../../core/core-contract';
import { MAX_ROSTER_SIZE } from '../../pascal/custom-tool-types';
import { Icon } from '../../ui/icon';
import * as api from './workbench.api';

/**
 * WorkbenchLibrary — the landing surface of Pascal's Workbench (v4
 * `components/custom-tools/WorkbenchLibrary.tsx`, 378).
 *
 * Every definition in every enabled store, valid or not, face up. Deliberately
 * NOT the chat roster: no per-invoker shadowing, no hiding broken files behind
 * badges — this is the authoring surface. Name collisions across stores get an
 * advisory (the library cannot say which wins in general; that depends on the
 * invoker), and invalid files carry the loader's own reason string, VERBATIM —
 * the browser never re-derives it.
 */

/** The advisory a colliding name carries. Carried verbatim from v4 `:36-39`. */
export const TIER_ADVISORY =
  'Nearest tier wins: character → participant → group → project → global. ' +
  'A disabled definition at a nearer tier suppresses the name outward. ' +
  'Which one a given chat deals depends on who is rolling.';

export function attachmentBadgeClass(kind: MountAttachment['kind']): string {
  switch (kind) {
    case 'general':
      return 'qt-badge qt-badge-primary';
    case 'character':
      return 'qt-badge qt-badge-info';
    case 'group':
      return 'qt-badge qt-badge-secondary';
    case 'project':
      return 'qt-badge qt-badge-success';
    case 'unattached':
      return 'qt-badge qt-badge-outline';
  }
}

export function attachmentLabel(attachment: MountAttachment): string {
  switch (attachment.kind) {
    case 'general':
      return 'The General Store';
    case 'character':
      return `${attachment.label}'s vault`;
    case 'group':
      return `Group: ${attachment.label}`;
    case 'project':
      return `Project: ${attachment.label}`;
    case 'unattached':
      return 'Unattached';
  }
}

/** One store holding more tools than a single roster seats. */
interface OverCapStore {
  mountName: string;
  count: number;
}

@Component({
  selector: 'qt-workbench-library',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [NgTemplateOutlet, FormsModule, RouterLink, Icon],
  template: `
    <div class="h-full overflow-y-auto">
      <div class="max-w-4xl mx-auto p-4 space-y-4">
        <div class="flex items-center justify-between gap-3 flex-wrap">
          <div>
            <h1 class="qt-card-title flex items-center gap-2">
              <qt-icon name="wrench" class="w-5 h-5" />
              Pascal&rsquo;s Workbench
            </h1>
            <p class="text-xs qt-text-secondary mt-1">
              Every contrivance on the premises, face up — whichever store it calls home.
            </p>
          </div>
          <button type="button" class="qt-button qt-button-primary" (click)="create.emit(undefined)">
            <qt-icon name="plus" class="w-4 h-4" />
            New contrivance
          </button>
        </div>

        <div class="flex items-center gap-3 flex-wrap">
          <input
            type="search"
            [ngModel]="search()"
            (ngModelChange)="search.set($event)"
            placeholder="Search by name, title, or description…"
            class="qt-input flex-1 min-w-48"
            aria-label="Search custom tools"
          />
          <label class="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              class="qt-checkbox"
              [ngModel]="groupByStore()"
              (ngModelChange)="groupByStore.set($event)"
            />
            Group by store
          </label>
        </div>

        @if (loading()) {
          <div class="text-sm qt-text-secondary py-8 text-center">
            Consulting Pascal&rsquo;s ledger&hellip;
          </div>
        }

        @if (error(); as err) {
          <div class="text-sm qt-text-destructive py-4">{{ err }}</div>
        }

        @if (!loading() && !error() && tools().length === 0 && errors().length === 0) {
          <div class="qt-card p-6 text-center space-y-2">
            <p class="text-sm">The baize is bare — not a single contrivance on the table.</p>
            <p class="text-xs qt-text-secondary">
              A custom tool is a small JSON file in any document store&rsquo;s
              <code>Tools/</code> folder. Build your first one here, and Pascal will deal it into
              every chat that can see its store.
            </p>
          </div>
        }

        @for (store of overCapStores(); track store.mountName) {
          <div class="text-xs qt-text-secondary">
            {{ store.mountName }} holds {{ store.count }} tools — more than the
            {{ MAX_ROSTER_SIZE }} a single chat&rsquo;s roster seats; the surplus is left off the
            table at deal time.
          </div>
        }

        @if (groups(); as grouped) {
          @for (group of grouped; track group.mountPointId) {
            <div class="space-y-2">
              <h2 class="text-sm font-medium mt-2">{{ group.mountName }}</h2>
              @for (tool of group.tools; track tool.mountPointId + ':' + tool.definitionPath) {
                <ng-container
                  [ngTemplateOutlet]="toolRow"
                  [ngTemplateOutletContext]="{ $implicit: tool }"
                />
              }
            </div>
          }
        } @else {
          @for (tool of filteredTools(); track tool.mountPointId + ':' + tool.definitionPath) {
            <ng-container
              [ngTemplateOutlet]="toolRow"
              [ngTemplateOutletContext]="{ $implicit: tool }"
            />
          }
        }

        @if (filteredErrors().length > 0) {
          <div class="space-y-2">
            <h2 class="text-sm font-medium mt-4">Cards that would not read</h2>
            @for (err of filteredErrors(); track err.mountPointId + ':' + err.definitionPath) {
              <div class="qt-card p-3 border qt-input-error">
                <div class="flex items-start justify-between gap-3">
                  <button
                    type="button"
                    class="min-w-0 text-left flex-1"
                    (click)="open.emit({ mountPointId: err.mountPointId, path: err.definitionPath })"
                    title="Open in repair mode"
                  >
                    <span class="block text-sm font-mono break-all">{{ err.definitionPath }}</span>
                    <span class="block text-xs qt-text-destructive mt-1">{{ err.reason }}</span>
                  </button>
                  <div class="flex items-center gap-1 flex-shrink-0">
                    <button
                      type="button"
                      class="qt-button qt-button-warning qt-button-sm"
                      (click)="
                        open.emit({ mountPointId: err.mountPointId, path: err.definitionPath })
                      "
                    >
                      Repair
                    </button>
                    <button
                      type="button"
                      class="qt-button qt-button-ghost qt-button-sm"
                      (click)="
                        deleteEntry(err.mountPointId, err.definitionPath, err.definitionPath)
                      "
                      title="Delete the definition file"
                    >
                      <qt-icon name="trash" class="w-4 h-4" />
                    </button>
                  </div>
                </div>
                <div class="flex flex-wrap items-center gap-1 mt-2">
                  <span class="qt-badge qt-badge-outline">{{ err.mountName }}</span>
                  @for (a of err.attachments; track a.kind + ':' + (a.id ?? '')) {
                    <span [class]="badgeClass(a.kind)">{{ label(a) }}</span>
                  }
                  <span class="qt-badge qt-badge-destructive">will not read</span>
                </div>
              </div>
            }
          </div>
        }
      </div>
    </div>

    <ng-template #toolRow let-tool>
      <div class="qt-card p-3" [class.opacity-60]="tool.disabled">
        <div class="flex items-start justify-between gap-3">
          <button
            type="button"
            class="min-w-0 text-left flex-1"
            (click)="open.emit({ mountPointId: tool.mountPointId, path: tool.definitionPath })"
            title="Open on the workbench"
          >
            <span class="block text-sm font-medium truncate">{{ tool.title }}</span>
            <span class="block text-xs font-mono qt-text-secondary truncate">{{ tool.name }}</span>
            @if (tool.description) {
              <span class="block text-xs qt-text-secondary mt-1 line-clamp-2">{{
                tool.description
              }}</span>
            }
          </button>
          <div class="flex items-center gap-1 flex-shrink-0">
            <button
              type="button"
              class="qt-button qt-button-ghost qt-button-sm"
              (click)="open.emit({ mountPointId: tool.mountPointId, path: tool.definitionPath })"
              title="Open on the workbench"
            >
              <qt-icon name="pencil" class="w-4 h-4" />
            </button>
            <button
              type="button"
              class="qt-button qt-button-ghost qt-button-sm"
              (click)="duplicate(tool)"
              title="Duplicate as a new contrivance"
            >
              <qt-icon name="copy" class="w-4 h-4" />
            </button>
            <button
              type="button"
              class="qt-button qt-button-ghost qt-button-sm"
              (click)="deleteEntry(tool.mountPointId, tool.definitionPath, tool.title)"
              [disabled]="deleting()"
              title="Delete the definition file"
            >
              <qt-icon name="trash" class="w-4 h-4" />
            </button>
          </div>
        </div>

        <div class="flex flex-wrap items-center gap-1 mt-2">
          <span class="qt-badge qt-badge-outline" [title]="tool.definitionPath">{{
            tool.mountName
          }}</span>
          @for (a of tool.attachments; track a.kind + ':' + (a.id ?? '')) {
            <span [class]="badgeClass(a.kind)">{{ label(a) }}</span>
          }
          @if (tool.disabled) {
            <span
              class="qt-badge qt-badge-disabled"
              title="A tombstone: suppresses this name at this tier and every farther one"
              >disabled</span
            >
          }
          @if (tool.defaultVisibility === 'whisper') {
            <span class="qt-badge qt-badge-secondary" title="Results whisper by default"
              >whisper</span
            >
          }
          <span class="qt-badge qt-badge-outline">{{
            tool.rollForm === 'dice' ? 'dice' : 'range'
          }}</span>
          @if (tool.parameterCount > 0) {
            <span class="qt-badge qt-badge-outline"
              >{{ tool.parameterCount }} param{{ tool.parameterCount === 1 ? '' : 's' }}</span
            >
          }
          <span class="qt-badge qt-badge-outline"
            >{{ tool.outcomeCount }} outcome{{ tool.outcomeCount === 1 ? '' : 's' }}</span
          >
          @if (collisionCount(tool.name); as count) {
            <span class="qt-badge qt-badge-warning" [title]="TIER_ADVISORY"
              >this name is defined in {{ count }} places</span
            >
          }
          <a
            [routerLink]="['/scriptorium', tool.mountPointId]"
            class="text-xs qt-text-secondary underline ml-auto"
            title="Reveal this store in the Scriptorium"
            >reveal in Scriptorium</a
          >
        </div>
      </div>
    </ng-template>
  `,
})
export class WorkbenchLibrary {
  private readonly core = inject(CoreClient);

  /** Drill into one definition (valid → form mode; broken → repair mode). */
  readonly open = output<{ mountPointId: string; path: string }>();
  /** Start a new definition, optionally with the destination preselected. */
  readonly create = output<string | undefined>();
  /** Start a new definition pre-filled from a serialized one (the duplicate flow). */
  readonly duplicateTemplate = output<string>();

  readonly search = signal('');
  readonly groupByStore = signal(false);

  readonly loading = signal(true);
  readonly error = signal<string | null>(null);
  readonly deleting = signal(false);
  readonly flash = signal<string | null>(null);

  private readonly _tools = signal<CustomToolLibraryEntry[]>([]);
  private readonly _errors = signal<CustomToolLibraryError[]>([]);

  readonly tools = this._tools.asReadonly();
  readonly errors = this._errors.asReadonly();

  protected readonly MAX_ROSTER_SIZE = MAX_ROSTER_SIZE;
  protected readonly TIER_ADVISORY = TIER_ADVISORY;
  protected readonly badgeClass = attachmentBadgeClass;
  protected readonly label = attachmentLabel;

  constructor() {
    void this.reload();
  }

  async reload(): Promise<void> {
    this.loading.set(true);
    this.error.set(null);
    try {
      const library = await api.fetchLibrary(this.core);
      this._tools.set(library.tools ?? []);
      this._errors.set(library.errors ?? []);
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : 'The library could not be read.');
    } finally {
      this.loading.set(false);
    }
  }

  /** Tool names that exist in more than one store — the shadowing advisory. */
  readonly collidingNames = computed(() => {
    const byName = new Map<string, number>();
    for (const tool of this._tools()) byName.set(tool.name, (byName.get(tool.name) ?? 0) + 1);
    return new Map([...byName].filter(([, count]) => count > 1));
  });

  /** Stores holding more tools than one chat's roster can seat. */
  readonly overCapStores = computed<OverCapStore[]>(() => {
    const byMount = new Map<string, OverCapStore>();
    for (const tool of this._tools()) {
      const entry = byMount.get(tool.mountPointId) ?? { mountName: tool.mountName, count: 0 };
      entry.count += 1;
      byMount.set(tool.mountPointId, entry);
    }
    return [...byMount.values()].filter((s) => s.count > MAX_ROSTER_SIZE);
  });

  readonly filteredTools = computed(() => {
    const needle = this.search().trim().toLowerCase();
    const tools = this._tools();
    const matched = needle
      ? tools.filter(
          (t) =>
            t.name.toLowerCase().includes(needle) ||
            t.title.toLowerCase().includes(needle) ||
            t.description.toLowerCase().includes(needle),
        )
      : tools;
    return [...matched].sort(
      (a, b) => a.title.localeCompare(b.title) || a.mountName.localeCompare(b.mountName),
    );
  });

  readonly filteredErrors = computed(() => {
    const needle = this.search().trim().toLowerCase();
    const errors = this._errors();
    return needle ? errors.filter((e) => e.definitionPath.toLowerCase().includes(needle)) : errors;
  });

  readonly groups = computed(() => {
    if (!this.groupByStore()) return null;
    const byStore = new Map<string, { mountName: string; tools: CustomToolLibraryEntry[] }>();
    for (const tool of this.filteredTools()) {
      const entry = byStore.get(tool.mountPointId) ?? { mountName: tool.mountName, tools: [] };
      entry.tools.push(tool);
      byStore.set(tool.mountPointId, entry);
    }
    return [...byStore.entries()]
      .sort((a, b) => a[1].mountName.localeCompare(b[1].mountName))
      .map(([mountPointId, group]) => ({ mountPointId, ...group }));
  });

  /** The collision count for a name, or null when it is unique (so `@if` skips). */
  collisionCount(name: string): number | null {
    return this.collidingNames().get(name) ?? null;
  }

  /**
   * Duplicate re-READS the file rather than reconstructing it from the library
   * entry: the entry is a summary (counts, not the outcome table), so only the
   * bytes on disk can seed a faithful copy. v4 does the same.
   */
  async duplicate(tool: CustomToolLibraryEntry): Promise<void> {
    try {
      const file = await api.readDefinitionFile(this.core, tool.mountPointId, tool.definitionPath);
      this.duplicateTemplate.emit(file.content);
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : 'The definition could not be read.');
    }
  }

  async deleteEntry(mountPointId: string, path: string, title: string): Promise<void> {
    if (
      !window.confirm(`Clear ${title} from the table? The file ${path} will be deleted.`)
    ) {
      return;
    }
    this.deleting.set(true);
    try {
      await api.deleteDefinitionFile(this.core, mountPointId, path);
      this.flash.set(`${title} has been cleared from the table.`);
      await this.reload();
    } catch (err) {
      this.error.set(
        err instanceof Error ? err.message : 'The contrivance would not be removed.',
      );
    } finally {
      this.deleting.set(false);
    }
  }
}
