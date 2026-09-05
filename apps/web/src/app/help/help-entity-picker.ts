/**
 * `HelpEntityPicker` — port of v4 `components/help-chat/HelpEntityPicker.tsx`.
 *
 * A help character can offer a navigation link with an unresolved parameter in
 * it (`/aurora/:id/edit` — "go edit a character", without knowing which). This
 * fetches the relevant entity list and lets the operator pick; the `:id` is then
 * substituted and navigation proceeds.
 *
 * The three `PARAM_ROUTES` rows are transcribed from v4 and pinned by
 * `__fixtures__/param-routes-vectors.json`, a probe of v4's real table through
 * its exported `hasParamSegments` / `findParamRoute`. The regexes are
 * deliberately case-sensitive and anchored, so `/AURORA/:id`, a missing leading
 * slash, and `/aurora/:id2` all miss — recorded vectors, not guesses.
 *
 * @module help/help-entity-picker
 */

import { ChangeDetectionStrategy, Component, computed, inject, input, output, signal } from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../core/core-client';
import type { CoreRequest } from '../core/core-contract';
import { Icon } from '../ui/icon';

/** Maps URL param patterns to their entity type and dispatch source. */
export interface ParamRoute {
  /** Regex that matches the full URL and captures the param position */
  pattern: RegExp;
  /** Human-readable entity type for the picker title */
  entityLabel: string;
  /**
   * v4's REST source for the list. Kept as the row's identity — v4 keys its
   * query cache on it and this does too — even though v5 reaches the same rows
   * over dispatch (see {@link request}).
   */
  apiUrl: string;
  /** The dispatch verb v5 reads the same list through. */
  request: CoreRequest;
  /**
   * Pull the array out of the dispatch reply.
   *
   * v4 names a `responseKey` because all three of its REST bodies wrap the list
   * in one (`{characters: [...]}`). v5's three verbs do NOT agree on that:
   * `characterList` and `projectList` answer a keyed object, but `listChats`
   * answers the bare array as its whole `data`. A per-row extractor is the
   * honest shape; the v4 key each one corresponds to is named on the row.
   */
  rows: (data: unknown) => Record<string, unknown>[];
  /** How to extract label + id from each item */
  getLabel: (item: Record<string, unknown>) => string;
  getId: (item: Record<string, unknown>) => string;
  /** Replace param in original URL template with real id */
  buildUrl: (urlTemplate: string, id: string) => string;
}

/** A defensive array read — a missing or wrong-shaped body yields no rows. */
function asRows(value: unknown): Record<string, unknown>[] {
  return Array.isArray(value) ? (value as Record<string, unknown>[]) : [];
}

export const PARAM_ROUTES: ParamRoute[] = [
  {
    pattern: /^\/aurora\/:id(\/|$)/,
    entityLabel: 'Character',
    apiUrl: '/api/v1/characters',
    request: { type: 'characterList' },
    // v4 responseKey: 'characters'
    rows: (data) => asRows((data as Record<string, unknown>)?.['characters']),
    getLabel: (item) => (item['name'] as string) || 'Unnamed',
    getId: (item) => item['id'] as string,
    buildUrl: (tpl, id) => tpl.replace(':id', id),
  },
  {
    pattern: /^\/salon\/:id(\/|$)/,
    entityLabel: 'Chat',
    apiUrl: '/api/v1/chats',
    request: { type: 'listChats' },
    // v4 responseKey: 'chats' — v5's `listChats` answers the bare array.
    rows: (data) => asRows(data),
    getLabel: (item) => (item['title'] as string) || 'Untitled chat',
    getId: (item) => item['id'] as string,
    buildUrl: (tpl, id) => tpl.replace(':id', id),
  },
  {
    pattern: /^\/prospero\/:id(\/|$)/,
    entityLabel: 'Project',
    apiUrl: '/api/v1/projects',
    request: { type: 'projectList' },
    // v4 responseKey: 'projects'
    rows: (data) => asRows((data as Record<string, unknown>)?.['projects']),
    getLabel: (item) => (item['name'] as string) || 'Unnamed project',
    getId: (item) => item['id'] as string,
    buildUrl: (tpl, id) => tpl.replace(':id', id),
  },
];

/** Check whether a URL contains parameterised segments that need resolution. */
export function hasParamSegments(url: string): boolean {
  return PARAM_ROUTES.some((r) => r.pattern.test(url));
}

/** Find the matching route config for a parameterised URL. */
export function findParamRoute(url: string): ParamRoute | null {
  return PARAM_ROUTES.find((r) => r.pattern.test(url)) || null;
}

interface EntityItem {
  id: string;
  label: string;
}

@Component({
  selector: 'qt-help-entity-picker',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  host: { class: 'contents' },
  template: `
    <div class="qt-help-entity-picker-backdrop" (click)="cancel.emit()">
      <div class="qt-help-entity-picker" (click)="$event.stopPropagation()">
        <div class="qt-help-entity-picker-header">
          <span>Select a {{ route()?.entityLabel || 'item' }}</span>
          <button
            type="button"
            class="p-0.5 rounded qt-hover-accent qt-text-secondary transition-colors"
            (click)="cancel.emit()"
          >
            <qt-icon name="close" class="w-3.5 h-3.5" />
          </button>
        </div>

        @if (items().length > 5) {
          <div class="px-2 pb-1">
            <input
              type="text"
              class="qt-help-entity-picker-filter"
              [placeholder]="'Filter ' + (route()?.entityLabel?.toLowerCase() || 'item') + 's...'"
              [value]="filter()"
              (input)="filter.set($any($event.target).value)"
            />
          </div>
        }

        <div class="qt-help-entity-picker-list">
          @if (loading()) {
            <div class="text-xs qt-text-secondary text-center py-3 italic">Loading...</div>
          }
          @if (error(); as err) {
            <div class="text-xs qt-text-destructive text-center py-3">{{ err }}</div>
          }
          @if (!loading() && !error() && filtered().length === 0) {
            <div class="text-xs qt-text-secondary text-center py-3">
              {{
                filter()
                  ? 'No matches'
                  : 'No ' + (route()?.entityLabel?.toLowerCase() || 'item') + 's found'
              }}
            </div>
          }
          @for (item of filtered(); track item.id) {
            <button
              type="button"
              class="qt-help-entity-picker-item"
              (click)="handleSelect(item.id)"
            >
              {{ item.label }}
            </button>
          }
        </div>
      </div>
    </div>
  `,
})
export class HelpEntityPicker {
  /** The parameterised URL template, e.g. /aurora/:id/edit */
  readonly urlTemplate = input.required<string>();
  /** Emits the resolved (real) URL once the operator picks an entity */
  readonly selectEntity = output<string>();
  /** Emits when the operator dismisses the picker */
  readonly cancel = output<void>();

  private readonly core = inject(CoreClient);

  protected readonly filter = signal('');
  protected readonly route = computed(() => findParamRoute(this.urlTemplate()));

  private readonly listQuery = injectQuery(() => {
    const route = this.route();
    return {
      // v4 keys on the api url (`queryKeys.helpChat.entity(apiUrl)`); kept, so
      // the two implementations key the same cache entry per entity kind.
      queryKey: ['help-chat', 'entity', route?.apiUrl ?? ''],
      enabled: !!route,
      queryFn: async (): Promise<Record<string, unknown>[]> => {
        if (!route) return [];
        return route.rows(await this.core.dispatchData(route.request));
      },
    };
  });

  protected readonly items = computed<EntityItem[]>(() => {
    const route = this.route();
    const rows = this.listQuery.data();
    if (!route || !rows) return [];
    return rows.map((item) => ({ id: route.getId(item), label: route.getLabel(item) }));
  });

  protected readonly loading = computed(() => this.listQuery.isLoading());

  protected readonly error = computed(() => {
    if (!this.route()) return 'Unknown entity type';
    const err = this.listQuery.error();
    if (!err) return null;
    return err instanceof Error ? err.message : 'Failed to load';
  });

  protected readonly filtered = computed(() => {
    const f = this.filter();
    if (!f) return this.items();
    const lower = f.toLowerCase();
    return this.items().filter((i) => i.label.toLowerCase().includes(lower));
  });

  protected handleSelect(id: string): void {
    const route = this.route();
    if (!route) return;
    this.selectEntity.emit(route.buildUrl(this.urlTemplate(), id));
  }
}
