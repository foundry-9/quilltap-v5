import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';
import { RouterLink } from '@angular/router';

import type { ChatDetail, ChatSettingsDto } from '../core/core-contract';
import { CopyChatIdButton } from '../ui/copy-chat-id-button';
import { Icon } from '../ui/icon';
import { ChatCostSummary } from './chat-cost-summary';

/**
 * The conversation header (v4 injects this via `usePageToolbar`; v5 renders it
 * inline). Project breadcrumb → title → danger badges → the LLM-Inspector button
 * → the chat-totals summary → the entry cluster (edit-enclave / gallery /
 * copy-conversation-ID).
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
      @if (storyBackgroundsEnabled()) {
        <button
          type="button"
          class="qt-button-ghost qt-button-sm flex-shrink-0"
          title="Regenerate story background image"
          aria-label="Regenerate Background"
          [disabled]="regeneratingBackground()"
          (click)="regenerateBackground.emit()"
        >
          <qt-icon name="sparkles" class="w-4 h-4" />
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
  /** Whether the Inspector panel is open — drives the button's active state (v4 :1002-1006). */
  readonly inspectorOpen = input(false);
  /** Toggle the LLM Inspector panel (v4 `toggleInspector`). */
  readonly toggleInspector = output<void>();
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
  /**
   * Queue a story-background regeneration (v4's ChatSidebar tool-palette entry,
   * `ChatSidebar.tsx:1204-1214`).
   *
   * PLACEMENT DIVERGENCE: v5 has no chat sidebar / tool palette
   * (`salon-conversation.ts:95`), so this relocates to the header entry cluster —
   * the same idiom the Edit-Enclave entry already established (P4.6af). ONE
   * button moves; the palette stays deferred.
   *
   * GLYPH DIVERGENCE: v4 uses the `image` icon, which works beside a text label
   * in the palette. The header cluster is icon-only and `image` ALREADY means
   * "View chat photos" there, so this uses `sparkles` — v5's established
   * "generate an image" glyph (the composer's generate button). v4's title copy
   * is unchanged and carries the meaning.
   */
  readonly regenerateBackground = output<void>();

  /** v4 `chatControls.storyBackgroundsEnabled` — the tool-palette gate. */
  readonly storyBackgroundsEnabled = input(false);
  /** Disables the entry while a regeneration poll is in flight (v4 has no such guard). */
  readonly regeneratingBackground = input(false);

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
