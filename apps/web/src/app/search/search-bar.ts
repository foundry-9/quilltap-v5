import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  ElementRef,
  inject,
  signal,
  viewChild,
} from '@angular/core';

import { Icon } from '../ui/icon';
import { SearchDialog } from './search-dialog';
import { SearchResults } from './search-results';
import { searchInline } from './search.api';
import type { SearchResult, SearchType } from './search.types';

/** v4 300ms debounce (`search-bar.tsx:105-107`). */
const DEBOUNCE_MS = 300;
/** v4's inline limit (`search-bar.tsx:75`). */
const INLINE_LIMIT = 20;
/** v4's desktop/mobile split for the ⌘K shortcut (`search-bar.tsx:46`). */
const DESKTOP_MIN_WIDTH = 768;

/**
 * The global search bar (v4 `components/search/search-bar.tsx`) — the page
 * toolbar's center occupant. Desktop (≥768px, CSS `hidden md:block`): an
 * inline input with a dropdown of the first 20 results, per-type
 * `(shown/total)` filter buttons opening the dialog, and "See all results →".
 * Mobile: a search icon button opening the dialog. Cmd+K (macOS) / Ctrl+K
 * focuses the inline input on desktop and opens the dialog under 768px —
 * platform-split exactly as v4 (on macOS Ctrl+K is "delete to end of line"
 * and must not be intercepted).
 */
@Component({
  selector: 'qt-search-bar',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: {
    '(document:keydown)': 'onGlobalKeydown($event)',
    '(document:mousedown)': 'onDocumentMouseDown($event)',
  },
  imports: [Icon, SearchDialog, SearchResults],
  template: `
    <!-- Desktop inline search - hidden on mobile -->
    <div #container class="relative hidden md:block">
      <div class="relative">
        <qt-icon
          name="search"
          class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 qt-text-secondary"
        />
        <input
          #searchInput
          type="text"
          [value]="query()"
          (input)="onInput($event)"
          (keydown)="onInputKeydown($event)"
          (focus)="onFocus()"
          placeholder="Search... (⌘K)"
          class="qt-input w-48 lg:w-64 pl-9 pr-3 py-1.5 focus:border-transparent transition-all"
        />
      </div>

      @if (showDropdown() && (query().length >= 2 || hasSearched())) {
        <div
          class="absolute top-full left-0 mt-2 w-96 bg-background rounded-lg qt-shadow-lg border qt-border-default overflow-hidden z-50"
        >
          <div class="max-h-96 overflow-y-auto">
            @if (hasSearched()) {
              <qt-search-results
                [results]="results()"
                [query]="query()"
                [isLoading]="isLoading()"
                [countsByType]="countsByType()"
                [hasTypeCountClick]="true"
                (resultClick)="onResultClick()"
                (typeCountClick)="onTypeCountClick($event)"
              />
            } @else {
              <div class="p-4 text-center qt-text-small">Type at least 2 characters to search</div>
            }
          </div>
          @if (results().length > 0) {
            <div class="px-3 py-2 border-t qt-border-default qt-bg-muted">
              <button type="button" (click)="seeAllResults()" class="text-xs text-primary hover:underline">
                See all results →
              </button>
            </div>
          }
        </div>
      }
    </div>

    <!-- Mobile search button - hidden on desktop -->
    <button
      type="button"
      (click)="openMobileDialog()"
      class="md:hidden p-2 qt-text-secondary qt-hover-accent rounded-md transition-colors"
      aria-label="Search"
    >
      <qt-icon name="search" class="w-5 h-5" />
    </button>

    <!-- Search dialog for mobile and "see all results" -->
    <qt-search-dialog
      [isOpen]="isDialogOpen()"
      [initialQuery]="dialogInitialQuery()"
      [initialTypes]="dialogInitialTypes()"
      (close)="onDialogClose()"
    />
  `,
})
export class SearchBar {
  private readonly destroyRef = inject(DestroyRef);

  protected readonly query = signal('');
  protected readonly results = signal<SearchResult[]>([]);
  protected readonly countsByType = signal<Partial<Record<SearchType, number>>>({});
  protected readonly isLoading = signal(false);
  protected readonly showDropdown = signal(false);
  protected readonly hasSearched = signal(false);

  protected readonly isDialogOpen = signal(false);
  protected readonly dialogInitialQuery = signal('');
  protected readonly dialogInitialTypes = signal<SearchType[] | undefined>(undefined);

  private readonly searchInput = viewChild<ElementRef<HTMLInputElement>>('searchInput');
  private readonly container = viewChild<ElementRef<HTMLDivElement>>('container');

  private debounceHandle: ReturnType<typeof setTimeout> | null = null;

  constructor() {
    this.destroyRef.onDestroy(() => {
      if (this.debounceHandle) clearTimeout(this.debounceHandle);
    });
  }

  /** v4 :33-58 — Cmd+K on macOS, Ctrl+K elsewhere; never both. */
  protected onGlobalKeydown(event: KeyboardEvent): void {
    const isMac =
      typeof navigator !== 'undefined' && navigator.platform.toUpperCase().indexOf('MAC') >= 0;
    const isShortcutPressed = isMac
      ? event.metaKey && !event.ctrlKey
      : event.ctrlKey && !event.metaKey;
    if (isShortcutPressed && event.key === 'k') {
      event.preventDefault();
      if (window.innerWidth >= DESKTOP_MIN_WIDTH) {
        this.searchInput()?.nativeElement.focus();
        this.showDropdown.set(true);
      } else {
        this.isDialogOpen.set(true);
      }
    }
  }

  /** v4 `useClickOutside` on the container (mousedown, per the house idiom). */
  protected onDocumentMouseDown(event: Event): void {
    if (!this.showDropdown()) return;
    const container = this.container()?.nativeElement;
    if (container && container.contains(event.target as Node)) return;
    this.showDropdown.set(false);
  }

  protected onInput(event: Event): void {
    const value = (event.target as HTMLInputElement).value;
    this.query.set(value);
    this.showDropdown.set(true);
    if (this.debounceHandle) clearTimeout(this.debounceHandle);
    this.debounceHandle = setTimeout(() => void this.performSearch(value), DEBOUNCE_MS);
  }

  /** v4 `handleKeyDown`: Enter = immediate search; Escape = close + blur. */
  protected onInputKeydown(event: KeyboardEvent): void {
    if (event.key === 'Enter' && this.query().length >= 2) {
      if (this.debounceHandle) clearTimeout(this.debounceHandle);
      void this.performSearch(this.query());
    } else if (event.key === 'Escape') {
      this.showDropdown.set(false);
      this.searchInput()?.nativeElement.blur();
    }
  }

  /** v4 `handleFocus` — reopen the dropdown, re-running a live query. */
  protected onFocus(): void {
    this.showDropdown.set(true);
    if (this.query().length >= 2) {
      void this.performSearch(this.query());
    }
  }

  /** v4 `handleResultClick` — close and clear. */
  protected onResultClick(): void {
    this.showDropdown.set(false);
    this.query.set('');
    this.results.set([]);
    this.countsByType.set({});
    this.hasSearched.set(false);
  }

  /** v4 `handleTypeCountClick` — dialog filtered to one type. */
  protected onTypeCountClick(type: SearchType): void {
    this.dialogInitialQuery.set(this.query());
    this.dialogInitialTypes.set([type]);
    this.isDialogOpen.set(true);
    this.showDropdown.set(false);
  }

  protected seeAllResults(): void {
    this.dialogInitialQuery.set(this.query());
    this.dialogInitialTypes.set(undefined);
    this.isDialogOpen.set(true);
    this.showDropdown.set(false);
  }

  protected openMobileDialog(): void {
    this.dialogInitialQuery.set('');
    this.isDialogOpen.set(true);
  }

  protected onDialogClose(): void {
    this.isDialogOpen.set(false);
    this.dialogInitialQuery.set('');
    this.dialogInitialTypes.set(undefined);
  }

  /** v4 `performSearch` (:64-92): min-2 clears; failures clear. */
  private async performSearch(searchQuery: string): Promise<void> {
    if (searchQuery.length < 2) {
      this.results.set([]);
      this.countsByType.set({});
      this.hasSearched.set(false);
      return;
    }
    this.isLoading.set(true);
    this.hasSearched.set(true);
    try {
      const data = await searchInline(searchQuery, INLINE_LIMIT);
      this.results.set(data.results ?? []);
      this.countsByType.set(data.countsByType ?? {});
    } catch {
      this.results.set([]);
      this.countsByType.set({});
    } finally {
      this.isLoading.set(false);
    }
  }
}
