import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { RouterLink } from '@angular/router';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../core/core-client';
import type { EnrichedChatSummary } from '../../core/core-contract';
import { ErrorAlert } from '../../ui/error-alert';
import { LoadingState } from '../../ui/loading-state';
import { ChatCard } from './chat-card';

/**
 * The Salon list screen (v4 `app/salon/SalonListView.tsx`) — the enriched
 * `listChats` response rendered as `ChatCard`s. Reads through TanStack Query so
 * a `chatSend` / delete elsewhere invalidates and refetches. Microcopy is
 * v4-verbatim ("Chats", "No chats yet", "Start a new chat").
 *
 * Client-side quick-hide filters (dangerous-chat hide, tag hide) and the
 * autonomous-room toggle are deferred — the server excludes autonomous rooms by
 * default and the read-only list needs no filtering yet.
 */
@Component({
  selector: 'qt-salon-list',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, LoadingState, ErrorAlert, ChatCard],
  template: `
    <div class="chat-page qt-page-container p-6">
      <div class="flex items-center justify-between gap-4 mb-4">
        <h1 class="qt-heading-1 leading-tight">Chats</h1>
      </div>

      @if (chats.isPending()) {
        <qt-loading-state message="Loading chats..." />
      } @else if (chats.isError()) {
        <qt-error-alert
          [message]="'Error: ' + errorMessage()"
          [retryable]="true"
          (retry)="chats.refetch()"
        />
      } @else if (visibleChats().length === 0) {
        <div
          class="chat-empty-state mt-12 rounded-2xl border border-dashed px-8 py-12 text-center"
        >
          <p class="mb-4 text-lg qt-text-small">No chats yet</p>
          <a routerLink="/salon/new" class="font-medium text-primary hover:text-primary/80"
            >Start a new chat</a
          >
        </div>
      } @else {
        <div class="chat-card-stack space-y-4">
          @for (chat of visibleChats(); track chat.id) {
            <qt-chat-card [chat]="chat" />
          }
        </div>
      }
    </div>
  `,
})
export class SalonList {
  private readonly core = inject(CoreClient);

  protected readonly chats = injectQuery(() => ({
    queryKey: ['chats'],
    queryFn: async (): Promise<EnrichedChatSummary[]> => {
      const resp = await this.core.dispatchExpect({ type: 'listChats' }, 'chats');
      return resp.data;
    },
  }));

  protected visibleChats(): EnrichedChatSummary[] {
    return this.chats.data() ?? [];
  }

  protected errorMessage(): string {
    const err = this.chats.error();
    return err instanceof Error ? err.message : 'Failed to load chats.';
  }
}
