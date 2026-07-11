import { ChangeDetectionStrategy, Component, computed, inject, input, signal } from '@angular/core';
import { RouterLink } from '@angular/router';

import { CoreClient } from '../../../core/core-client';
import type { EnrichedChatSummary } from '../../../core/core-contract';
import { ErrorAlert } from '../../../ui/error-alert';
import { ChatCard } from '../../salon/chat-card';
import { fetchProjectChats, removeProjectChat } from '../projects.api';

const PAGE_SIZE = 20;

/**
 * The project chats section (v4 `ChatsSection.tsx`): a paginated list of the
 * project's chats using the shared {@link ChatCard} in its removable mode
 * (remove DISASSOCIATES, doesn't delete). "Load more" appends the next page
 * (page size 20); a New Chat link starts one under the project.
 *
 * The v4 IntersectionObserver auto-load is simplified to an explicit "Load more"
 * button (v4's visible fallback) — a recorded, minor divergence.
 */
@Component({
  selector: 'qt-project-chats-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, ChatCard, ErrorAlert],
  template: `
    <div class="mt-8">
      <div class="flex items-center justify-between mb-4">
        <h2 class="qt-heading-3 text-foreground">
          Chats
          @if (total() > 0) {
            <span class="ml-2 text-base font-normal qt-text-secondary">({{ chats().length }})</span>
          }
        </h2>
        <a
          [routerLink]="['/salon/new']"
          [queryParams]="{ projectId: projectId() }"
          class="qt-button qt-button-primary"
        >
          New Chat
        </a>
      </div>

      @if (error(); as msg) {
        <qt-error-alert [message]="msg" class="mb-4" />
      }

      @if (loading()) {
        <div class="flex items-center justify-center py-12 qt-text-secondary">
          <p>Loading chats...</p>
        </div>
      } @else if (chats().length === 0) {
        <div
          class="rounded-2xl qt-border border-dashed qt-bg-card px-8 py-12 text-center qt-shadow-sm"
        >
          <p class="mb-4 text-lg qt-text-secondary">No chats in this project yet.</p>
          <a
            [routerLink]="['/salon/new']"
            [queryParams]="{ projectId: projectId() }"
            class="font-medium qt-text-primary hover:opacity-80"
          >
            Start a new chat
          </a>
        </div>
      } @else {
        <div class="grid gap-4 grid-cols-1 xl:grid-cols-2">
          @for (chat of chats(); track chat.id) {
            <qt-chat-card [chat]="chat" [removable]="true" (remove)="onRemove($event)" />
          }
        </div>
        <div class="py-8 flex items-center justify-center">
          @if (loadingMore()) {
            <p class="qt-text-secondary">Loading more chats...</p>
          } @else if (hasMore()) {
            <button type="button" class="qt-button qt-button-ghost" (click)="loadMore()">
              Load more
            </button>
          } @else {
            <p class="qt-text-xs qt-text-secondary">All chats loaded</p>
          }
        </div>
      }
    </div>
  `,
})
export class ProjectChatsSection {
  readonly projectId = input.required<string>();

  private readonly core = inject(CoreClient);

  protected readonly chats = signal<EnrichedChatSummary[]>([]);
  protected readonly total = signal(0);
  protected readonly offset = signal(0);
  protected readonly loading = signal(true);
  protected readonly loadingMore = signal(false);
  protected readonly error = signal<string | null>(null);

  protected readonly hasMore = computed(() => this.chats().length < this.total());

  private loadedFor: string | null = null;

  constructor() {
    // Kick the first page once the projectId input resolves.
    queueMicrotask(() => void this.ensureLoaded());
  }

  private async ensureLoaded(): Promise<void> {
    const id = this.projectId();
    if (!id || this.loadedFor === id) {
      return;
    }
    this.loadedFor = id;
    this.loading.set(true);
    this.error.set(null);
    try {
      const page = await fetchProjectChats(this.core, id, { limit: PAGE_SIZE, offset: 0 });
      this.chats.set(page.chats);
      this.total.set(page.total);
      this.offset.set(0);
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : 'Failed to load chats');
    } finally {
      this.loading.set(false);
    }
  }

  protected async loadMore(): Promise<void> {
    if (this.loadingMore() || !this.hasMore()) {
      return;
    }
    this.loadingMore.set(true);
    const nextOffset = this.offset() + PAGE_SIZE;
    try {
      const page = await fetchProjectChats(this.core, this.projectId(), {
        limit: PAGE_SIZE,
        offset: nextOffset,
      });
      this.chats.update((prev) => [...prev, ...page.chats]);
      this.total.set(page.total);
      this.offset.set(nextOffset);
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : 'Failed to load more chats');
    } finally {
      this.loadingMore.set(false);
    }
  }

  protected async onRemove(chatId: string): Promise<void> {
    this.error.set(null);
    try {
      await removeProjectChat(this.core, this.projectId(), chatId);
      this.chats.update((prev) => prev.filter((c) => c.id !== chatId));
      this.total.update((t) => Math.max(0, t - 1));
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : 'Failed to remove chat');
    }
  }
}
