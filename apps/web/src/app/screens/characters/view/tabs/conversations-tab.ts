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
import { injectInfiniteQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../../core/core-client';
import { QuickHideService } from '../../../../quick-hide/quick-hide.service';
import type { CharacterChatsResult } from '../../../../core/core-contract';
import { Icon } from '../../../../ui/icon';
import { characterKeys, fetchCharacterChats } from '../../characters.api';
import { CharacterConversationCard } from './character-conversation-card';

const CHATS_PER_PAGE = 10;
const SEARCH_DEBOUNCE_MS = 300;

/**
 * The Conversations tab (v4 `components/character/character-conversations-tab.tsx`):
 * the per-character chat list with a debounced search box and offset pagination
 * (v4 `CHATS_PER_PAGE = 10`, infinite scroll). Each card links into the Salon
 * conversation. The v4 "Refresh Conversation Archive" / "New Chat" actions and
 * per-card delete / re-extract hit routes outside this vertical's contract and
 * are omitted (a follow-up vertical, per the P4.6g deferral list); the per-card
 * re-render went LIVE with the P4.9H2B scriptorium badge.
 */
@Component({
  selector: 'qt-character-conversations-tab',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, CharacterConversationCard],
  template: `
    <div class="space-y-4">
      <div class="flex items-center gap-4">
        <div class="relative flex-1">
          <input
            type="text"
            placeholder="Search conversations..."
            class="qt-input pl-10"
            [value]="searchInput()"
            (input)="onSearch($any($event.target).value)"
          />
          <qt-icon
            name="search"
            class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 qt-text-secondary"
          />
        </div>
      </div>

      @if (chatsQuery.isPending()) {
        <div class="flex items-center justify-center py-12">
          <div class="flex items-center gap-3 qt-text-secondary">
            <div
              class="h-5 w-5 animate-spin rounded-full border-2 qt-border-primary border-r-transparent"
            ></div>
            Loading conversations...
          </div>
        </div>
      } @else if (chatsQuery.isError()) {
        <div class="text-center py-12">
          <p class="qt-text-destructive">{{ errorMessage() }}</p>
          <button
            type="button"
            class="mt-4 text-primary hover:underline"
            (click)="chatsQuery.refetch()"
          >
            Try again
          </button>
        </div>
      } @else if (chats().length === 0) {
        <div class="text-center py-12 border border-dashed qt-border-default rounded-lg">
          <qt-icon name="chat" class="mx-auto h-12 w-12 qt-text-secondary" />
          <p class="mt-2 qt-text-small">
            @if (search()) {
              No conversations found matching "{{ search() }}"
            } @else {
              No conversations with {{ characterName() }} yet
            }
          </p>
        </div>
      } @else {
        <div class="space-y-4">
          @for (chat of chats(); track chat.id) {
            <qt-character-conversation-card [chat]="chat" />
          }

          <div #loadMore class="py-4">
            @if (chatsQuery.hasNextPage()) {
              <button
                type="button"
                class="mx-auto block text-primary hover:underline"
                [disabled]="chatsQuery.isFetchingNextPage()"
                (click)="loadNext()"
              >
                @if (chatsQuery.isFetchingNextPage()) {
                  Loading more...
                } @else {
                  Load more conversations
                }
              </button>
            } @else {
              <p class="text-center qt-text-small">No more conversations</p>
            }
          </div>
        </div>
      }
    </div>
  `,
})
export class CharacterConversationsTab {
  private readonly core = inject(CoreClient);
  private readonly destroyRef = inject(DestroyRef);
  private readonly quickHide = inject(QuickHideService);

  readonly characterId = input.required<string>();
  readonly characterName = input<string>('Character');

  protected readonly searchInput = signal('');
  protected readonly search = signal('');
  private searchTimer: ReturnType<typeof setTimeout> | null = null;

  private readonly loadMoreEl = viewChild<ElementRef<HTMLElement>>('loadMore');

  protected readonly chatsQuery = injectInfiniteQuery(() => ({
    queryKey: characterKeys.chats(this.characterId(), this.search() || undefined),
    initialPageParam: 0,
    queryFn: ({ pageParam }): Promise<CharacterChatsResult> =>
      fetchCharacterChats(this.core, this.characterId(), {
        search: this.search() || undefined,
        limit: CHATS_PER_PAGE,
        offset: pageParam as number,
      }),
    // v4 `hasMore = newChats.length === CHATS_PER_PAGE`.
    getNextPageParam: (lastPage: CharacterChatsResult, _all: unknown, lastParam: number) =>
      lastPage.chats.length === CHATS_PER_PAGE ? lastParam + CHATS_PER_PAGE : undefined,
  }));

  /**
   * v4 `character-conversations-tab.tsx:38-44`: the dangerous arm first, then
   * CHAT-level tags only — this consumer does not consult participants (the
   * page is already scoped to one character).
   */
  protected readonly chats = computed(() =>
    (this.chatsQuery.data()?.pages ?? [])
      .flatMap((p) => p.chats)
      .filter((chat) => {
        if (this.quickHide.hideDangerousChats() && chat.isDangerousChat) return false;
        return !this.quickHide.shouldHideByIds((chat.tags ?? []).map((ct) => ct.tag.id));
      }),
  );

  constructor() {
    // Infinite scroll: observe the sentinel and pull the next page as it nears
    // view (v4's IntersectionObserver, guarded for non-DOM test/SSR contexts).
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

  protected loadNext(): void {
    if (this.chatsQuery.hasNextPage() && !this.chatsQuery.isFetchingNextPage()) {
      void this.chatsQuery.fetchNextPage();
    }
  }

  protected errorMessage(): string {
    const err = this.chatsQuery.error();
    return err instanceof Error ? err.message : 'Failed to load conversations';
  }
}
