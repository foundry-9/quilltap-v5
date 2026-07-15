import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';
import { RouterLink } from '@angular/router';

import type { ChatDetail, ChatSettingsDto } from '../core/core-contract';
import { CopyChatIdButton } from '../ui/copy-chat-id-button';
import { Icon } from '../ui/icon';
import { ChatCostSummary } from './chat-cost-summary';

/**
 * The conversation header (v4 injects this via `usePageToolbar`; v5 renders it
 * inline). Project breadcrumb → title → danger badges → the chat-totals summary
 * → the entry cluster (edit-enclave / gallery / copy-conversation-ID).
 *
 * Character links remain a deferral. So does the **LLM-Inspector button**, which
 * v4 renders from the same toolbar effect as the cost summary
 * (`SalonView.tsx:990-1027`, gated by `llmLoggingSettings.enabled`) — sharing an
 * effect makes them look like one feature, but the Inspector is a separate
 * subsystem with its own surface.
 */
@Component({
  selector: 'qt-conversation-header',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, Icon, CopyChatIdButton, ChatCostSummary],
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
      @if (showChatTotals()) {
        <qt-chat-cost-summary
          [chatId]="chat().id"
          [show]="showChatTotals()"
          [refreshKey]="messageCount()"
          class="flex-shrink-0"
        />
      }
      @if (isAutonomous()) {
        <button
          type="button"
          class="qt-button-ghost qt-button-sm flex-shrink-0"
          title="Edit this enclave’s schedule, budget, and visibility"
          aria-label="Edit Enclave"
          (click)="editEnclave.emit()"
        >
          <qt-icon name="settings" class="w-4 h-4" />
        </button>
      }
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
  /** The shared chat-settings row — the totals summary reads `tokenDisplaySettings`. */
  readonly settings = input<ChatSettingsDto | null>(null);
  /**
   * v4's `refreshKey={messages.length}` (`SalonView.tsx:1017`) — the summary
   * re-fetches whenever the message count moves, since a new turn is exactly
   * what changes the totals.
   */
  readonly messageCount = input(0);
  /** Open the in-chat photo gallery (v4 SalonView sidebar gallery entry). */
  readonly openGallery = output<void>();
  /**
   * Open the Edit-Enclave modal (v4 ChatSidebar's "Organize" palette entry,
   * shown only for autonomous rooms). PLACEMENT DIVERGENCE: v5 has no chat
   * sidebar/Organize palette, so this rides the conversation header's right
   * cluster next to the gallery/copy-id buttons. Visibility is the only guard —
   * no confirmation dialog; the button opens the modal directly.
   */
  readonly editEnclave = output<void>();

  /** v4 `chatSettings?.tokenDisplaySettings?.showChatTotals` — default false. */
  protected readonly showChatTotals = computed(
    () => this.settings()?.tokenDisplaySettings?.showChatTotals ?? false,
  );

  protected readonly isAutonomous = computed(() => this.chat().chatType === 'autonomous');

  protected readonly isOffDuty = computed(() => this.chat().conciergeOverride === 'OFF');

  protected readonly dangerTitle = computed(() => {
    const cats = this.chat().dangerCategories ?? [];
    return cats.length > 0
      ? `The Concierge has flagged this chat: ${cats.join(', ')}`
      : 'The Concierge has flagged this chat.';
  });
}
