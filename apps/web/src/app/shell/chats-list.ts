import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../core/core-client';
import type { ChatSummaryDto } from '../core/core-contract';
import { EmptyState } from '../ui/empty-state';
import { ErrorAlert } from '../ui/error-alert';
import { LoadingState } from '../ui/loading-state';

/**
 * The chats list — the first real server data on screen (v4 `/salon`
 * `SalonListView`, reduced to a foundation list; the rich ChatCard / autonomous
 * rooms / quick-hide filters are P4.6 Salon concerns). Reads `listChats` through
 * TanStack Query. Copy verbatim from v4.
 */
@Component({
  selector: 'qt-chats-list',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [LoadingState, ErrorAlert, EmptyState],
  template: `
    <div class="chat-page qt-page-container p-6">
      <h1 class="qt-heading-1 mb-4">Chats</h1>

      @if (chats.isPending()) {
        <qt-loading-state message="Loading chats..." />
      } @else if (chats.isError()) {
        <qt-error-alert [message]="'Error: ' + errorMessage()" [retryable]="true" (retry)="chats.refetch()" />
      } @else if ((chats.data() ?? []).length === 0) {
        <qt-empty-state title="No chats yet" description="Your conversations will appear here." />
      } @else {
        <ul class="space-y-2">
          @for (chat of chats.data(); track chat.id) {
            <li class="qt-card p-4 flex items-center justify-between gap-4">
              <div class="min-w-0">
                <p class="qt-text-primary font-semibold truncate">{{ chat.title || 'Untitled chat' }}</p>
                <p class="qt-text-secondary qt-body-sm">
                  {{ chat.chatType }} · {{ chat.messageCount }} message{{ chat.messageCount === 1 ? '' : 's' }}
                </p>
              </div>
              <span class="qt-text-small qt-text-muted flex-shrink-0">{{ formatDate(chat.updatedAt) }}</span>
            </li>
          }
        </ul>
      }
    </div>
  `,
})
export class ChatsList {
  private readonly core = inject(CoreClient);

  protected readonly chats = injectQuery(() => ({
    queryKey: ['chats'],
    queryFn: async (): Promise<ChatSummaryDto[]> => {
      const resp = await this.core.dispatchExpect({ type: 'listChats' }, 'chats');
      return resp.data;
    },
  }));

  protected errorMessage(): string {
    const err = this.chats.error();
    return err instanceof Error ? err.message : 'Failed to load chats.';
  }

  protected formatDate(iso: string): string {
    const d = new Date(iso);
    return Number.isNaN(d.getTime()) ? '' : d.toLocaleDateString();
  }
}
