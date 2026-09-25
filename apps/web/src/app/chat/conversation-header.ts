import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';
import { RouterLink } from '@angular/router';

import type { ChatDetail, ChatSettingsDto } from '../core/core-contract';
import { getConciergeProvenance, getConciergeReason, getConciergeState } from './concierge-state';
import { ConciergeTooltipBody } from './concierge-mark';
import {
  CONCIERGE_STATE_PRESENTATION,
  conciergeToneSuffix,
  describeConciergeState,
} from './concierge-state-presentation';
import { Tooltip } from '../ui/tooltip';
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
  imports: [RouterLink, Icon, CopyChatIdButton, ChatCostSummary, Tooltip, ConciergeTooltipBody],
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

      <!-- The Concierge badge (v4 SalonView.tsx:1190-1222 at ce2f1dabf). ONE
           pill, derived from the three-state; Moderated is the default and
           renders no badge at all — "the pill means something other than the
           default is set".

           Every word, icon and tone comes from the ONE presentation table, so
           the pill speaks the same sentences as the list marks and the
           sidebar's helper text; the drawn bubble is the only place the
           classifier's categories are named. The pill is THE one place that
           passes the refusal count ("after two refusals") — list payloads
           carry none. -->
      @if (conciergeState() !== 'moderated') {
        <qt-tooltip [contentTemplate]="conciergeTip" placement="bottom">
          <span
            [class]="'qt-danger-badge' + badgeSuffixClass() + ' flex-shrink-0'"
            role="img"
            [attr.aria-label]="'Concierge: ' + conciergePresentation().label"
          >
            <qt-icon [name]="conciergePresentation().icon" class="w-3 h-3" />{{
              conciergePresentation().label
            }}
          </span>
        </qt-tooltip>
        <ng-template #conciergeTip>
          <qt-concierge-tooltip-body [description]="conciergeDescription()" />
        </ng-template>
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

  /**
   * The three-state, derived through the shared predicate module so the badge
   * can never disagree with the sidebar control or the message-list danger
   * styling (v4 `getConciergeState`).
   */
  protected readonly conciergeState = computed(() => getConciergeState(this.chat()));

  /** The state's row of the ONE presentation table (v4 `c43d3b1b4`). */
  protected readonly conciergePresentation = computed(
    () => CONCIERGE_STATE_PRESENTATION[this.conciergeState()],
  );

  /**
   * The bubble's contents (v4 `SalonView.tsx:1197-1201`): the provenance picks
   * Unmoderated's sentence, the refusal count names "after N refusals", and the
   * categories reach it only when the classifier moved the chat.
   */
  protected readonly conciergeDescription = computed(() => {
    const chat = this.chat();
    return describeConciergeState(
      this.conciergeState(),
      {
        setBy: getConciergeProvenance(chat),
        reason: getConciergeReason(chat),
        refusalCount: chat.conciergeRefusalCount,
      },
      chat.dangerCategories ?? undefined,
    );
  });

  /**
   * `danger` is the base rule, so it adds nothing; only Locked's muted tone
   * carries a modifier. This IS an interpolation — the same one v4's
   * `SalonView.tsx` builds — so `check-qt-classes` cannot see
   * `qt-danger-badge-muted` from here; the rule lives in `_chat.css` and is
   * referenced by name only in specs. The guard flags references without rules,
   * not rules without references, so this is safe as long as that rule stays.
   */
  protected readonly badgeSuffixClass = computed(() => {
    const suffix = conciergeToneSuffix(this.conciergePresentation().tone);
    return suffix ? ` qt-danger-badge${suffix}` : '';
  });
}
