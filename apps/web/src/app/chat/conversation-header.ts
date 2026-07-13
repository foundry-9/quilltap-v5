import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';
import { RouterLink } from '@angular/router';

import type { ChatDetail } from '../core/core-contract';
import { CopyChatIdButton } from '../ui/copy-chat-id-button';
import { Icon } from '../ui/icon';

/**
 * The conversation header (v4 injects this via `usePageToolbar`; v5 renders it
 * inline). Project breadcrumb → title → danger badges → the copy-conversation-ID
 * button. Character links and the cost/inspector cluster are deferrals.
 */
@Component({
  selector: 'qt-conversation-header',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, Icon, CopyChatIdButton],
  template: `
    <header class="flex items-center gap-2 text-sm min-w-0 px-4 py-3 border-b qt-border-default">
      @if (chat().projectId && chat().projectName) {
        <a
          class="inline-flex items-center gap-1.5 qt-text-secondary hover:text-foreground transition-colors flex-shrink-0"
          [routerLink]="['/prospero', chat().projectId]"
        >
          <qt-icon name="folder" class="w-4 h-4" />
          {{ chat().projectName }}
        </a>
        <span class="qt-text-muted">/</span>
      }

      <a
        class="qt-text-primary truncate hover:text-foreground transition-colors"
        [routerLink]="['/salon', chat().id]"
        [title]="chat().title"
        >{{ chat().title || 'Untitled chat' }}</a
      >

      @if (isOffDuty()) {
        <span
          class="qt-danger-badge flex-shrink-0"
          title="The Concierge is off-duty for this chat."
        >
          <qt-icon name="check-circle" class="w-3 h-3" />Off-duty
        </span>
      }
      @if (chat().isDangerousChat) {
        <span class="qt-danger-badge flex-shrink-0" [title]="dangerTitle()">
          <qt-icon name="alert-triangle" class="w-3 h-3" />Flagged
        </span>
      }

      <span class="flex-1"></span>
      <button
        type="button"
        class="qt-button-ghost qt-button-sm flex-shrink-0"
        title="View chat photos"
        aria-label="View chat photos"
        (click)="openGallery.emit()"
      >
        <qt-icon name="image" class="w-4 h-4" />
      </button>
      <qt-copy-chat-id-button [chatId]="chat().id" />
    </header>
  `,
})
export class ConversationHeader {
  readonly chat = input.required<ChatDetail>();
  /** Open the in-chat photo gallery (v4 SalonView sidebar gallery entry). */
  readonly openGallery = output<void>();

  protected readonly isOffDuty = computed(() => this.chat().conciergeOverride === 'OFF');

  protected readonly dangerTitle = computed(() => {
    const cats = this.chat().dangerCategories ?? [];
    return cats.length > 0
      ? `The Concierge has flagged this chat: ${cats.join(', ')}`
      : 'The Concierge has flagged this chat.';
  });
}
