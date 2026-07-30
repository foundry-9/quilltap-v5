import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  ElementRef,
  effect,
  inject,
  input,
  output,
  signal,
  viewChild,
} from '@angular/core';

import { Icon } from '../ui/icon';
import { SearchResults } from './search-results';
import { searchPaged } from './search.api';
import { appendDeduped } from './search.logic';
import type { SearchResult, SearchType } from './search.types';

const ALL_TYPES: SearchType[] = ['chats', 'characters', 'messages', 'tags', 'memories'];
/** v4 `PAGE_SIZE`. */
const PAGE_SIZE = 20;
/** v4 300ms debounce. */
const DEBOUNCE_MS = 300;

/**
 * The full search dialog (v4 `components/search/search-dialog.tsx`): the
 * mobile entry point and the bar's "See all results →" target. Type-filter
 * chips (at least one always selected), 300ms-debounced paged search,
 * infinite-scroll load-more deduped on `type-id`, Enter = immediate search,
 * Escape/backdrop = close, and the ↵/Esc footer.
 *
 * v4's open/close lifecycle is reproduced with the host owning `isOpen`:
 * opening seeds `initialQuery`/`initialTypes` (and fires the initial search
 * when the seed is ≥2 chars); closing resets everything.
 */
@Component({
  selector: 'qt-search-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { '(document:keydown.escape)': 'onEscape()' },
  imports: [Icon, SearchResults],
  template: `
    @if (isOpen()) {
      <!-- Backdrop (v4: a full-viewport button on qt-dialog-overlay). -->
      <button
        class="qt-dialog-overlay !p-0 cursor-default z-[100]"
        (click)="close.emit()"
        aria-label="Close search"
        type="button"
      ></button>

      <div
        class="fixed inset-x-4 top-16 md:inset-x-auto md:left-1/2 md:-translate-x-1/2 md:w-full md:max-w-2xl z-[101]"
      >
        <div class="bg-background rounded-lg shadow-2xl overflow-hidden">
          <!-- Search input + type chips -->
          <div class="p-4 border-b qt-border-default">
            <div class="relative">
              <qt-icon
                name="search"
                class="absolute left-3 top-1/2 -translate-y-1/2 w-5 h-5 qt-text-secondary"
              />
              <input
                #searchInput
                type="text"
                [value]="query()"
                (input)="onInput($event)"
                (keydown.enter)="onEnter()"
                placeholder="Search chats, characters, messages, tags, memories..."
                class="w-full pl-10 pr-4 py-3 text-lg border-0 bg-transparent text-foreground placeholder-muted-foreground focus:outline-none focus:ring-0"
              />
              @if (query()) {
                <button
                  type="button"
                  (click)="clearQuery()"
                  class="absolute right-3 top-1/2 -translate-y-1/2 p-1 qt-text-secondary hover:text-foreground"
                  aria-label="Clear search"
                >
                  <qt-icon name="close" class="w-5 h-5" />
                </button>
              }
            </div>

            <div class="flex flex-wrap gap-2 mt-3">
              @for (type of allTypes; track type) {
                <button
                  type="button"
                  (click)="toggleType(type)"
                  [class]="selectedTypes().includes(type) ? 'qt-filter-chip-active' : 'qt-filter-chip'"
                >
                  {{ chipLabel(type) }}
                </button>
              }
            </div>
          </div>

          <!-- Results -->
          <div #scrollContainer class="max-h-[60vh] overflow-y-auto" (scroll)="onScroll()">
            @if (hasSearched()) {
              <qt-search-results
                [results]="results()"
                [query]="query()"
                [isLoading]="isLoading()"
                [countsByType]="countsByType()"
                (resultClick)="close.emit()"
              />
              @if (isLoadingMore()) {
                <div class="p-4 text-center">
                  <div
                    class="inline-block w-5 h-5 border-2 qt-border-primary border-t-transparent rounded-full animate-spin"
                  ></div>
                </div>
              }
              @if (!isLoading() && !isLoadingMore() && results().length > 0 && !hasMore()) {
                <div class="p-3 text-center qt-text-xs qt-text-secondary border-t qt-border-default">
                  Showing all {{ totalCount() }} results
                </div>
              }
            } @else {
              <div class="p-6 text-center qt-text-small">
                <p>Type at least 2 characters to search</p>
                <p class="qt-text-xs mt-1">
                  Search across your chats, characters, messages, tags, and memories
                </p>
              </div>
            }
          </div>

          <!-- Footer -->
          <div class="px-4 py-2 border-t qt-border-default qt-bg-muted qt-text-xs flex justify-between">
            <span>
              <kbd class="px-1.5 py-0.5 qt-bg-accent qt-text-on-accent rounded">↵</kbd> to search
            </span>
            <span>
              <kbd class="px-1.5 py-0.5 qt-bg-accent qt-text-on-accent rounded">Esc</kbd> to close
            </span>
          </div>
        </div>
      </div>
    }
  `,
})
export class SearchDialog {
  private readonly destroyRef = inject(DestroyRef);

  readonly isOpen = input.required<boolean>();
  readonly initialQuery = input<string>('');
  /** Pre-select specific types when opening the dialog. */
  readonly initialTypes = input<SearchType[] | undefined>(undefined);
  readonly close = output<void>();

  protected readonly allTypes = ALL_TYPES;
  protected readonly query = signal('');
  protected readonly results = signal<SearchResult[]>([]);
  protected readonly isLoading = signal(false);
  protected readonly isLoadingMore = signal(false);
  protected readonly selectedTypes = signal<SearchType[]>(ALL_TYPES);
  protected readonly hasSearched = signal(false);
  protected readonly hasMore = signal(false);
  protected readonly totalCount = signal(0);
  protected readonly countsByType = signal<Partial<Record<SearchType, number>>>({});

  private readonly searchInput = viewChild<ElementRef<HTMLInputElement>>('searchInput');
  private readonly scrollContainer = viewChild<ElementRef<HTMLDivElement>>('scrollContainer');

  private debounceHandle: ReturnType<typeof setTimeout> | null = null;
  /** v4 `currentQueryRef` — a stale response must not clobber a newer query's. */
  private currentQuery = '';

  constructor() {
    // v4's open/close effect (:110-140): opening seeds the initial query/types
    // and focuses; closing resets everything.
    effect(() => {
      if (this.isOpen()) {
        const initial = this.initialQuery();
        const initialTypes = this.initialTypes();
        if (initial) this.query.set(initial);
        this.selectedTypes.set(initialTypes && initialTypes.length > 0 ? initialTypes : ALL_TYPES);
        setTimeout(() => this.searchInput()?.nativeElement.focus(), 100);
        if (initial && initial.length >= 2) {
          void this.performSearch(initial, this.selectedTypes(), 0, true);
        }
      } else {
        this.query.set('');
        this.results.set([]);
        this.hasSearched.set(false);
        this.hasMore.set(false);
        this.totalCount.set(0);
        this.countsByType.set({});
        this.selectedTypes.set(ALL_TYPES);
        this.currentQuery = '';
      }
    });
    this.destroyRef.onDestroy(() => {
      if (this.debounceHandle) clearTimeout(this.debounceHandle);
    });
  }

  /** v4 chip text: `type.charAt(0).toUpperCase() + type.slice(1)`. */
  protected chipLabel(type: SearchType): string {
    return type.charAt(0).toUpperCase() + type.slice(1);
  }

  protected onEscape(): void {
    if (this.isOpen()) this.close.emit();
  }

  protected onInput(event: Event): void {
    const value = (event.target as HTMLInputElement).value;
    this.query.set(value);
    if (this.debounceHandle) clearTimeout(this.debounceHandle);
    this.debounceHandle = setTimeout(() => {
      void this.performSearch(value, this.selectedTypes(), 0, true);
    }, DEBOUNCE_MS);
  }

  protected onEnter(): void {
    if (this.query().length < 2) return;
    if (this.debounceHandle) clearTimeout(this.debounceHandle);
    void this.performSearch(this.query(), this.selectedTypes(), 0, true);
  }

  protected clearQuery(): void {
    this.query.set('');
    this.results.set([]);
    this.hasSearched.set(false);
    this.hasMore.set(false);
    this.totalCount.set(0);
    this.countsByType.set({});
    this.searchInput()?.nativeElement.focus();
  }

  /** v4 `toggleType` — the last selected chip cannot be deselected. */
  protected toggleType(type: SearchType): void {
    const current = this.selectedTypes();
    const next = current.includes(type)
      ? current.filter((t) => t !== type)
      : [...current, type];
    if (next.length === 0) return;
    this.selectedTypes.set(next);
    if (this.query().length >= 2) {
      void this.performSearch(this.query(), next, 0, true);
    }
  }

  /** v4's infinite scroll: load more within 100px of the bottom. */
  protected onScroll(): void {
    const el = this.scrollContainer()?.nativeElement;
    if (!el) return;
    if (el.scrollHeight - el.scrollTop - el.clientHeight < 100) {
      this.loadMore();
    }
  }

  private loadMore(): void {
    if (!this.isLoadingMore() && this.hasMore() && this.query().length >= 2) {
      void this.performSearch(this.query(), this.selectedTypes(), this.results().length, false);
    }
  }

  /** v4 `performSearch` (:42-108) — new-search vs append-page arms. */
  private async performSearch(
    searchQuery: string,
    types: SearchType[],
    offset: number,
    isNewSearch: boolean,
  ): Promise<void> {
    if (searchQuery.length < 2) {
      this.results.set([]);
      this.hasSearched.set(false);
      this.hasMore.set(false);
      this.totalCount.set(0);
      this.countsByType.set({});
      return;
    }

    if (isNewSearch) {
      this.isLoading.set(true);
      this.currentQuery = searchQuery;
    } else {
      this.isLoadingMore.set(true);
    }
    this.hasSearched.set(true);

    try {
      const data = await searchPaged(searchQuery, types, PAGE_SIZE, offset);
      if (this.currentQuery === searchQuery) {
        if (isNewSearch) {
          this.results.set(data.results ?? []);
          // v4: countsByType updates only on a NEW search, not load-more.
          this.countsByType.set(data.countsByType ?? {});
        } else {
          this.results.update((prev) => appendDeduped(prev, data.results ?? []));
        }
        this.hasMore.set(data.hasMore ?? false);
        this.totalCount.set(data.totalCount ?? 0);
      }
    } catch {
      if (isNewSearch) this.results.set([]);
      this.hasMore.set(false);
    } finally {
      if (isNewSearch) {
        this.isLoading.set(false);
      } else {
        this.isLoadingMore.set(false);
      }
    }
  }
}
