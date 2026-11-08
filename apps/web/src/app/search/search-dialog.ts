import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  ElementRef,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
  untracked,
  viewChild,
} from '@angular/core';

import {
  CONVERSATION_SEARCH_MIN_QUERY,
  type ConversationSearchSource,
} from '../chat/conversation-search';

import { Icon } from '../ui/icon';
import { ConversationMatches } from './conversation-matches';
import { SearchResults } from './search-results';
import { searchPaged } from './search.api';
import { appendDeduped } from './search.logic';
import { ALL_SEARCH_TYPES } from './search.types';
import type { SearchResult, SearchType } from './search.types';
import { isInFloatingLayer } from '../ui/floating-layer';
import { DialogFocus } from '../ui/dialog-focus';

/**
 * v4's dialog reads the shared constant rather than keeping its own copy
 * (`b220999d`) — the chip order and the default `types=` CSV both come from it,
 * so a new type can't be half-added.
 */
const ALL_TYPES = ALL_SEARCH_TYPES;
/** v4 `PAGE_SIZE`. */
const PAGE_SIZE = 20;
/** v4 `CONVERSATION_LIMIT` — in-chat matches the dialog lists (`4f5047241`). */
const CONVERSATION_LIMIT = 50;
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
 *
 * In a Salon chat (P4.D384, v4 `4f5047241`) the host passes the focused
 * conversation's source: a `This conversation` chip (on by default, off when
 * the dialog was opened on one type) lists up to 50 in-chat matches above an
 * `Everywhere` divider; picking one hands the query back to the bar
 * (`conversationReveal`), reveals the message, and closes.
 */
@Component({
  selector: 'qt-search-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { '(document:keydown.escape)': 'onEscape($event)' },
  imports: [ConversationMatches, DialogFocus, Icon, SearchResults],
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
        <div
          class="qt-bg-popover rounded-lg shadow-2xl overflow-hidden"
          role="dialog"
          aria-modal="true"
          aria-label="Search"
          tabindex="-1"
          qtDialogFocus
        >
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
                aria-label="Search"
                placeholder="Search chats, characters, messages, documents, tags, memories..."
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
              @if (conversation()) {
                <button
                  type="button"
                  (click)="includeConversation.set(!includeConversation())"
                  [attr.aria-pressed]="includeConversation()"
                  [class]="includeConversation() ? 'qt-filter-chip-active' : 'qt-filter-chip'"
                >
                  This conversation
                </button>
              }
              @for (type of allTypes; track type) {
                <button
                  type="button"
                  (click)="toggleType(type)"
                  [attr.aria-pressed]="selectedTypes().includes(type)"
                  [class]="selectedTypes().includes(type) ? 'qt-filter-chip-active' : 'qt-filter-chip'"
                >
                  {{ chipLabel(type) }}
                </button>
              }
            </div>
          </div>

          <!-- Results -->
          <div #scrollContainer class="max-h-[60vh] overflow-y-auto" (scroll)="onScroll()">
            @if (conversationResults(); as inChat) {
              <qt-conversation-matches
                [matches]="inChat.matches"
                [total]="inChat.total"
                [query]="query()"
                (select)="onConversationSelect($event)"
              />
              <div
                class="px-3 pt-2 pb-1 border-t qt-border-default qt-text-xs font-semibold uppercase tracking-wider"
              >
                Everywhere
              </div>
            }
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
                  Search across your chats, characters, messages, documents, tags, and memories
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
  private readonly hostEl = inject<ElementRef<HTMLElement>>(ElementRef);

  readonly isOpen = input.required<boolean>();
  readonly initialQuery = input<string>('');
  /** Pre-select specific types when opening the dialog. */
  readonly initialTypes = input<SearchType[] | undefined>(undefined);
  /** The open Salon chat, searched first behind a "This conversation" chip. */
  readonly conversation = input<ConversationSearchSource | null>(null);
  readonly close = output<void>();
  /** Emitted with the trimmed query when an in-chat match is picked, just before closing. */
  readonly conversationReveal = output<string>();

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
  /**
   * "This conversation" is a filter of its own beside the global types; it is
   * on by default, and off when the dialog was opened to look at one type.
   */
  protected readonly includeConversation = signal(true);
  private readonly trimmedQuery = computed(() => this.query().trim());
  protected readonly conversationResults = computed(() => {
    const conversation = this.conversation();
    const trimmed = this.trimmedQuery();
    // `untracked`: v4's `useMemo` keys on [conversation, includeConversation,
    // trimmedQuery] only.
    return conversation && this.includeConversation() && trimmed.length >= CONVERSATION_SEARCH_MIN_QUERY
      ? untracked(() => conversation.search(trimmed, CONVERSATION_LIMIT))
      : null;
  });

  private readonly searchInput = viewChild<ElementRef<HTMLInputElement>>('searchInput');
  private readonly scrollContainer = viewChild<ElementRef<HTMLDivElement>>('scrollContainer');

  private debounceHandle: ReturnType<typeof setTimeout> | null = null;
  /** v4 `currentQueryRef` — a stale response must not clobber a newer query's. */
  private currentQuery = '';

  constructor() {
    // PORTAL to document.body (v5 fixed first — dogfood #45; v4 `ea4dc011`
    // converged with `createPortal`, bug 40).
    //
    // `SearchBar` renders this dialog inline, and the bar lives inside
    // `.qt-page-toolbar`, which carries `backdrop-filter` (_layout.css:709).
    // A `backdrop-filter` makes an element a CONTAINING BLOCK for
    // `position: fixed` descendants, so the backdrop's `fixed inset-0`
    // resolved against the toolbar instead of the viewport — measured at
    // 1224x64 in a 1280x720 window. Clicking anywhere outside the toolbar hit
    // the page, not the backdrop, so only Escape closed the dialog. Reparenting
    // to the body gives the fixed backdrop the viewport as its containing block;
    // the backdrop element and its `qt-dialog-overlay` class are untouched.
    //
    // NB `slide-over-panel.ts` records the standing "v5 renders inline"
    // decision, whose stated premise was that no ancestor carries a
    // transform/filter. The P4.9P toolbar invalidated that premise for
    // anything rendered inside it; this is the first such surface.
    if (typeof document !== 'undefined') {
      const el = this.hostEl.nativeElement;
      document.body.appendChild(el);
      this.destroyRef.onDestroy(() => el.remove());
    }

    // v4's open/close effect (:110-140): opening seeds the initial query/types
    // and focuses; closing resets everything.
    effect(() => {
      if (this.isOpen()) {
        const initial = this.initialQuery();
        const initialTypes = this.initialTypes();
        if (initial) this.query.set(initial);
        // Compute the seed types locally rather than reading `selectedTypes()`
        // back — a signal read here would REGISTER the effect on it, so every
        // later chip toggle would re-run this seeding (snapping the chips back
        // to the initial types, clobbering the typed query, and re-firing the
        // initial search). v4's effect deps are [isOpen, initialQuery,
        // initialTypes] only (search-dialog.tsx:110-140).
        const seedTypes = initialTypes && initialTypes.length > 0 ? initialTypes : ALL_TYPES;
        this.selectedTypes.set(seedTypes);
        // v4: opened on one type → the in-chat filter starts OFF; otherwise ON.
        this.includeConversation.set(!(initialTypes && initialTypes.length > 0));
        setTimeout(() => this.searchInput()?.nativeElement.focus(), 100);
        if (initial && initial.length >= 2) {
          void this.performSearch(initial, seedTypes, 0, true);
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

  /** v4 `handleConversationSelect`: hand the query back, reveal, close. */
  protected onConversationSelect(messageId: string): void {
    this.conversationReveal.emit(this.trimmedQuery());
    this.conversation()?.reveal(messageId);
    this.close.emit();
  }

  protected onEscape(event?: Event): void {
    if (isInFloatingLayer(event?.target ?? null)) return;
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

  /**
   * DIVERGENCE from v4 (recorded): in v4, clearing the input of a dialog
   * opened with an initial query immediately re-runs the INITIAL search and
   * repopulates results under the now-empty input — its second open effect
   * (search-dialog.tsx:143-151) depends on `hasSearched`, which this reset
   * flips. v5's seeding effect deliberately does not track `hasSearched`, so
   * a cleared dialog stays cleared.
   */
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
