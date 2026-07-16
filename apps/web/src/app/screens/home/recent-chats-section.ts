import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { RouterLink } from '@angular/router';

import { RecentChatItem } from './recent-chat-item';
import type { RecentChat } from './home.api';

/**
 * The homepage Recent Chats section (v4
 * `components/homepage/RecentChatsSection.tsx`): header + "View all" into the
 * Salon, the empty arm, and the chat rows. The server caps the list (§1: max
 * 12); CSS `overflow: hidden` on the section content hides rows that don't fit.
 *
 * v4's quick-hide filtering (`useQuickHide` — tag hide + hide-dangerous-chats)
 * is DEFERRED: v5 has no quick-hide provider yet (the same standing deferral
 * the Salon list records), so every server-sent chat renders. The
 * dangerous-chat `*` marker on the rows is pure payload data and IS ported.
 */
@Component({
  selector: 'qt-recent-chats-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, RecentChatItem],
  template: `
    <div class="qt-homepage-section">
      <div class="qt-homepage-section-header">
        <h2 class="qt-homepage-section-title">Recent Chats</h2>
        <a routerLink="/salon" class="qt-homepage-section-link">View all &rarr;</a>
      </div>
      <div class="qt-homepage-section-content">
        @if (chats().length === 0) {
          <div class="text-center py-6 qt-text-secondary">
            <p class="text-sm">No chats yet</p>
            <a routerLink="/salon/new" class="text-xs text-primary hover:underline">
              Start your first chat
            </a>
          </div>
        } @else {
          @for (chat of chats(); track chat.id) {
            <qt-recent-chat-item [chat]="chat" />
          }
        }
      </div>
    </div>
  `,
})
export class RecentChatsSection {
  readonly chats = input.required<RecentChat[]>();
}
