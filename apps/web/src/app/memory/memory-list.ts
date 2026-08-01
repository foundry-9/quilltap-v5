import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  ElementRef,
  computed,
  effect,
  inject,
  input,
  signal,
  viewChild,
} from '@angular/core';
import { Router } from '@angular/router';
import { injectInfiniteQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../core/core-client';
import type { MemoryDto, MemorySortBy, MemorySortOrder } from '../core/core-contract';
import { SectionHeader } from '../ui/section-header';
import { fetchMemories, memoryKeys, type MemoryListPage } from './memory.api';
import { HousekeepingDialog } from './housekeeping-dialog';
import { MemoryCard } from './memory-card';
import { MemoryEditor } from './memory-editor';
import { appendUniqueMemories } from './memory-format';
import { deleteMemory } from './memory.api';
import { ToastService } from '../ui/toast.service';

const MEMORIES_PER_PAGE = 30;
const SEARCH_DEBOUNCE_MS = 300;

type SourceFilter = 'ALL' | 'AUTO' | 'MANUAL';

/**
 * The per-character Commonplace Book list (v4 `components/memory/memory-list.tsx`):
 * a search box (debounced), sort-by + sort-order + source filters, a header with
 * the total count and an "Add Memory" action, an infinite-scroll grid of memory
 * cards (`MEMORIES_PER_PAGE = 30`, NOT virtualized — v4 parity), and the
 * create/edit editor. Appended pages are deduped by id (offset instability during
 * a regenerate sweep — v4 parity). Delete is confirmed inline.
 *
 * The "Cleanup" housekeeping dialog lands in the P4.6t housekeeping unit; the
 * Source link on a card navigates to the source chat (deep message anchoring is
 * a P4.6t deferral).
 */
@Component({
  selector: 'qt-memory-list',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [SectionHeader, MemoryCard, MemoryEditor, HousekeepingDialog],
  template: `
    <div class="space-y-4">
      @if (banner(); as msg) {
        <div class="qt-alert-error">{{ msg }}</div>
      }

      <!-- Header -->
      <div class="flex items-center justify-between gap-4">
        <qt-section-header title="Memories" [count]="totalCount()" class="flex-1">
          <button type="button" class="qt-button-primary qt-button-sm" (click)="openCreate()">
            Add Memory
          </button>
        </qt-section-header>
        @if (memories().length > 0) {
          <button
            type="button"
            class="px-3 py-1.5 qt-bg-muted qt-text-body text-sm rounded-lg qt-hover-accent"
            title="Clean up old and low-importance memories"
            (click)="showHousekeeping.set(true)"
          >
            Cleanup
          </button>
        }
      </div>

      <!-- Filters -->
      <div class="flex flex-wrap gap-3 items-center">
        <div class="flex-1 min-w-[200px]">
          <input
            type="text"
            placeholder="Search memories..."
            class="qt-input text-sm"
            [value]="searchInput()"
            (input)="onSearch($any($event.target).value)"
          />
        </div>
        <select
          class="qt-select w-auto"
          (change)="sortBy.set($any($event.target).value)"
        >
          <option value="createdAt" [selected]="sortBy() === 'createdAt'">Created Date</option>
          <option value="updatedAt" [selected]="sortBy() === 'updatedAt'">Updated Date</option>
          <option value="importance" [selected]="sortBy() === 'importance'">Importance</option>
        </select>
        <button
          type="button"
          class="px-3 py-2 text-sm border qt-border-default qt-bg-surface qt-text-body rounded-lg qt-hover-accent"
          [title]="sortOrder() === 'asc' ? 'Ascending' : 'Descending'"
          (click)="toggleOrder()"
        >
          {{ sortOrder() === 'asc' ? '↑' : '↓' }}
        </button>
        <select
          class="qt-select w-auto"
          (change)="sourceFilter.set($any($event.target).value)"
        >
          <option value="ALL" [selected]="sourceFilter() === 'ALL'">All Sources</option>
          <option value="AUTO" [selected]="sourceFilter() === 'AUTO'">Auto-generated</option>
          <option value="MANUAL" [selected]="sourceFilter() === 'MANUAL'">Manual</option>
        </select>
      </div>

      @if (memoriesQuery.isPending()) {
        <div class="flex items-center justify-center py-12">
          <div class="flex items-center gap-3 qt-text-secondary">
            <div
              class="h-5 w-5 animate-spin rounded-full border-2 qt-border-primary border-r-transparent"
            ></div>
            Loading memories...
          </div>
        </div>
      } @else {
        @if (memoriesQuery.isError()) {
          <div class="text-center py-6">
            <p class="qt-text-destructive">{{ errorMessage() }}</p>
            <button type="button" class="mt-2 text-primary hover:underline" (click)="memoriesQuery.refetch()">
              Try again
            </button>
          </div>
        }

        @if (memories().length === 0) {
          <div class="text-center py-12 border border-dashed qt-border-default rounded-lg">
            @if (search()) {
              <p class="qt-text-small">No memories match your search</p>
            } @else {
              <p class="qt-text-label">No memories yet</p>
              <p class="mt-1 qt-text-small">
                Memories will be created automatically during conversations, or you can add them
                manually.
              </p>
            }
          </div>
        } @else {
          <div class="grid gap-4 sm:grid-cols-1 lg:grid-cols-2">
            @for (memory of memories(); track memory.id) {
              <qt-memory-card
                [memory]="memory"
                [isDeleting]="deletingId() === memory.id"
                (edit)="openEdit($event)"
                (delete)="confirmDelete($event)"
                (navigateToSource)="navigateToSource($event)"
              />
            }
          </div>
        }

        <!-- Infinite-scroll sentinel -->
        <div #loadMore class="py-2">
          @if (memoriesQuery.isFetchingNextPage()) {
            <div class="flex items-center justify-center gap-2 qt-text-secondary">
              <div
                class="h-4 w-4 animate-spin rounded-full border-2 qt-border-primary border-r-transparent"
              ></div>
              Loading more memories...
            </div>
          } @else if (!memoriesQuery.hasNextPage() && memories().length > 0 && memories().length < totalCount()) {
            <p class="text-center qt-text-small">All memories loaded</p>
          }
        </div>
      }
    </div>

    @if (showEditor()) {
      <qt-memory-editor
        [characterId]="characterId()"
        [memory]="editingMemory()"
        (close)="closeEditor()"
        (saved)="onEditorSaved()"
      />
    }

    @if (showHousekeeping()) {
      <qt-housekeeping-dialog
        [characterId]="characterId()"
        (close)="showHousekeeping.set(false)"
        (complete)="onHousekeepingComplete()"
      />
    }

    @if (pendingDeleteId(); as id) {
      <div class="qt-dialog-overlay" (click)="pendingDeleteId.set(null)">
        <div class="qt-dialog max-w-md" role="dialog" aria-modal="true" (click)="$event.stopPropagation()">
          <div class="qt-dialog-header">
            <h2 class="qt-dialog-title">Delete memory</h2>
          </div>
          <div class="qt-dialog-body">
            <p class="qt-text-body">Are you sure you want to delete this memory?</p>
          </div>
          <div class="qt-dialog-footer">
            <button type="button" class="qt-button qt-button-secondary" (click)="pendingDeleteId.set(null)">
              Cancel
            </button>
            <button
              type="button"
              class="qt-button qt-button-destructive"
              [disabled]="deletingId() === id"
              (click)="doDelete(id)"
            >
              {{ deletingId() === id ? 'Deleting...' : 'Delete' }}
            </button>
          </div>
        </div>
      </div>
    }
  `,
})
export class MemoryList {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);
  private readonly router = inject(Router);
  private readonly queryClient = injectQueryClient();
  private readonly destroyRef = inject(DestroyRef);

  readonly characterId = input.required<string>();

  protected readonly searchInput = signal('');
  protected readonly search = signal('');
  protected readonly sortBy = signal<MemorySortBy>('createdAt');
  protected readonly sortOrder = signal<MemorySortOrder>('desc');
  protected readonly sourceFilter = signal<SourceFilter>('ALL');

  protected readonly showEditor = signal(false);
  protected readonly showHousekeeping = signal(false);
  protected readonly editingMemory = signal<MemoryDto | null>(null);
  protected readonly pendingDeleteId = signal<string | null>(null);
  protected readonly deletingId = signal<string | null>(null);
  protected readonly banner = signal<string | null>(null);

  private searchTimer: ReturnType<typeof setTimeout> | null = null;
  private readonly loadMoreEl = viewChild<ElementRef<HTMLElement>>('loadMore');

  /** The source param sent to the server: `undefined` when the filter is ALL. */
  private readonly sourceParam = computed<'AUTO' | 'MANUAL' | undefined>(() => {
    const s = this.sourceFilter();
    return s === 'ALL' ? undefined : s;
  });

  private readonly filters = computed(() => ({
    search: this.search() || undefined,
    sortBy: this.sortBy(),
    sortOrder: this.sortOrder(),
    source: this.sourceParam(),
  }));

  protected readonly memoriesQuery = injectInfiniteQuery(() => ({
    queryKey: memoryKeys.list(this.characterId(), this.filters()),
    initialPageParam: 0,
    queryFn: ({ pageParam }): Promise<MemoryListPage> =>
      fetchMemories(this.core, {
        characterId: this.characterId(),
        limit: MEMORIES_PER_PAGE,
        offset: pageParam as number,
        sortBy: this.sortBy(),
        sortOrder: this.sortOrder(),
        search: this.search() || undefined,
        source: this.sourceParam(),
      }),
    // v4 `hasMore = newMemories.length === MEMORIES_PER_PAGE`.
    getNextPageParam: (lastPage: MemoryListPage, _all: unknown, lastParam: number) =>
      lastPage.memories.length === MEMORIES_PER_PAGE ? lastParam + MEMORIES_PER_PAGE : undefined,
  }));

  /** Flatten pages, deduping ids across page boundaries (v4 append dedupe). */
  protected readonly memories = computed(() =>
    (this.memoriesQuery.data()?.pages ?? []).reduce<MemoryDto[]>(
      (acc, page) => appendUniqueMemories(acc, page.memories),
      [],
    ),
  );

  /** v4 shows the server `totalCount` from the latest page. */
  protected readonly totalCount = computed(() => {
    const pages = this.memoriesQuery.data()?.pages ?? [];
    return pages.length > 0 ? pages[pages.length - 1].totalCount : 0;
  });

  constructor() {
    // Infinite scroll: pull the next page as the sentinel nears view.
    effect((onCleanup) => {
      const el = this.loadMoreEl()?.nativeElement;
      if (!el || typeof IntersectionObserver === 'undefined') {
        return;
      }
      const observer = new IntersectionObserver(
        (entries) => {
          if (entries[0]?.isIntersecting) {
            this.loadNext();
          }
        },
        { threshold: 0.1 },
      );
      observer.observe(el);
      onCleanup(() => observer.disconnect());
    });

    this.destroyRef.onDestroy(() => {
      if (this.searchTimer) {
        clearTimeout(this.searchTimer);
      }
    });
  }

  protected onSearch(value: string): void {
    this.searchInput.set(value);
    if (this.searchTimer) {
      clearTimeout(this.searchTimer);
    }
    this.searchTimer = setTimeout(() => this.search.set(value), SEARCH_DEBOUNCE_MS);
  }

  protected toggleOrder(): void {
    this.sortOrder.update((o) => (o === 'asc' ? 'desc' : 'asc'));
  }

  protected loadNext(): void {
    if (this.memoriesQuery.hasNextPage() && !this.memoriesQuery.isFetchingNextPage()) {
      void this.memoriesQuery.fetchNextPage();
    }
  }

  protected openCreate(): void {
    this.editingMemory.set(null);
    this.showEditor.set(true);
  }

  protected openEdit(memory: MemoryDto): void {
    this.editingMemory.set(memory);
    this.showEditor.set(true);
  }

  protected closeEditor(): void {
    this.showEditor.set(false);
    this.editingMemory.set(null);
  }

  protected onEditorSaved(): void {
    this.closeEditor();
    void this.queryClient.invalidateQueries({ queryKey: memoryKeys.list(this.characterId()) });
  }

  protected confirmDelete(id: string): void {
    this.pendingDeleteId.set(id);
  }

  protected async doDelete(id: string): Promise<void> {
    this.deletingId.set(id);
    this.banner.set(null);
    try {
      await deleteMemory(this.core, id);
      this.toasts.showSuccess('Memory deleted');
      this.pendingDeleteId.set(null);
      await this.queryClient.invalidateQueries({ queryKey: memoryKeys.list(this.characterId()) });
    } catch (err) {
      this.toasts.showError(
        err instanceof Error && err.message ? err.message : 'Failed to delete memory',
      );
    } finally {
      this.deletingId.set(null);
    }
  }

  protected onHousekeepingComplete(): void {
    this.showHousekeeping.set(false);
    void this.queryClient.invalidateQueries({ queryKey: memoryKeys.list(this.characterId()) });
  }

  protected navigateToSource(e: { chatId: string; messageId: string }): void {
    // Deep message anchoring is a P4.6t deferral; navigate to the chat.
    void this.router.navigate(['/salon', e.chatId]);
  }

  protected errorMessage(): string {
    const err = this.memoriesQuery.error();
    return err instanceof Error ? err.message : 'Failed to fetch memories';
  }
}
