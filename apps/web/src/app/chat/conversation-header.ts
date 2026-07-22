import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';
import { RouterLink } from '@angular/router';

import type { ChatDetail, ChatSettingsDto } from '../core/core-contract';
import { CopyChatIdButton } from '../ui/copy-chat-id-button';
import { Icon } from '../ui/icon';
import { ChatCostSummary } from './chat-cost-summary';

/**
 * The conversation header (v4 injects this via `usePageToolbar`; v5 renders it
 * inline). Project breadcrumb → title → danger badges → the LLM-Inspector button
 * → the chat-totals summary → the copy-conversation-ID button — v4's toolbar
 * exactly (`SalonView.tsx:960-1040`).
 *
 * Four entries used to live here as recorded PLACEMENT DIVERGENCES, each because
 * "v5 has no chat sidebar": the All-Whispers toggle (P4.6ba), Edit Enclave
 * (P4.6af), Regenerate Background (P4.6ap) and the photo gallery. P4.9H1 ported
 * the sidebar, so all four went home — Visibility, Organize, Chat and Organize
 * respectively — and the header is v4's again.
 *
 * The Inspector button and the cost summary come out of the SAME v4 toolbar
 * effect (`SalonView.tsx:990-1027`), and their ORDER there is load-bearing:
 * inspector first, summary second (v4 :995-1024). They are independent features
 * with independent gates, though — either can render without the other.
 *
 * Character links remain a deferral.
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
      <!-- The Inspector button precedes the cost summary (v4 :995-1024). -->
      @if (showInspectorButton()) {
        <button
          type="button"
          class="p-1.5 rounded transition-colors flex-shrink-0"
          [class]="
            inspectorOpen()
              ? 'qt-bg-primary/15 text-primary'
              : 'qt-text-secondary hover:text-foreground'
          "
          title="LLM Inspector (Cmd+Shift+L)"
          aria-label="Toggle LLM Inspector"
          (click)="toggleInspector.emit()"
        >
          <qt-icon name="code" class="w-4 h-4" />
        </button>
      }
      @if (showChatTotals()) {
        <qt-chat-cost-summary
          [chatId]="chat().id"
          [show]="showChatTotals()"
          [refreshKey]="messageCount()"
          class="flex-shrink-0"
        />
      }
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
  /** Whether the Inspector panel is open — drives the button's active state (v4 :1002-1006). */
  readonly inspectorOpen = input(false);
  /** Toggle the LLM Inspector panel (v4 `toggleInspector`). */
  readonly toggleInspector = output<void>();

  /** v4 `chatSettings?.tokenDisplaySettings?.showChatTotals` — default false. */
  protected readonly showChatTotals = computed(
    () => this.settings()?.tokenDisplaySettings?.showChatTotals ?? false,
  );

  /**
   * v4 `chatSettings?.llmLoggingSettings?.enabled !== false` (`SalonView.tsx:993`).
   *
   * Note the polarity: this defaults TRUE. An absent bag, an absent key, or
   * settings that have not loaded yet all leave the button VISIBLE — only an
   * explicit `false` hides it. (`?? true` would be wrong for an explicit null,
   * which `!== false` admits; the server's own parse defaults it the same way.)
   */
  protected readonly showInspectorButton = computed(
    () =>
      (this.settings()?.['llmLoggingSettings'] as { enabled?: boolean } | undefined)?.enabled !==
      false,
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
