import { ChangeDetectionStrategy, Component, computed, inject, input } from '@angular/core';
import { RouterLink } from '@angular/router';

import { QuickHideService } from '../../quick-hide/quick-hide.service';
import { RecentChatItem } from './recent-chat-item';
import type { RecentChat } from './home.api';

/**
 * The homepage Recent Chats section (v4
 * `components/homepage/RecentChatsSection.tsx`): header + "View all" into the
 * Salon, the empty arm, and the chat rows. The server caps the list (§1: max
 * 12); CSS `overflow: hidden` on the section content hides rows that don't fit.
 *
 * Quick-hide filtering is v4 `RecentChatsSection.tsx:20-37` transcribed, and its
 * tag collection is deliberately NARROWER than the Salon list's: this section
 * gathers ONLY participant character tags (`:30-36`) — chat-level tags are not
 * consulted, because the homepage `RecentChat` projection does not carry them.
 * The dangerous arm is checked FIRST (`:22-25`), reading the real payload field
 * `isDangerousChat`.
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
        @if (visibleChats().length === 0) {
          <div class="text-center py-6 qt-text-secondary">
            <p class="text-sm">No chats yet</p>
            <a routerLink="/salon/new" class="text-xs text-primary hover:underline">
              Start your first chat
            </a>
          </div>
        } @else {
          @for (chat of visibleChats(); track chat.id) {
            <qt-recent-chat-item [chat]="chat" />
          }
        }
      </div>
    </div>
  `,
})
export class RecentChatsSection {
  readonly chats = input.required<RecentChat[]>();

  private readonly quickHide = inject(QuickHideService);

  /** v4 `:20-37` — the dangerous arm first, then participant character tags only. */
  protected readonly visibleChats = computed(() =>
    this.chats().filter((chat) => {
      if (this.quickHide.hideDangerousChats() && chat.isDangerousChat) {
        return false;
      }
      const allTagIds: string[] = [];
      for (const participant of chat.participants) {
        if (participant.character?.tags) {
          allTagIds.push(...participant.character.tags);
        }
      }
      return !this.quickHide.shouldHideByIds(allTagIds);
    }),
  );
}
