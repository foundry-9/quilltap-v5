/**
 * BrahmaConsolePanel — the Brahma Console itself (a port of v4
 * `components/brahma-console/BrahmaConsolePanel.tsx`, `99463fc56`, at
 * `236c04e35`): launcher (past conversations + opening composer), transcript,
 * composer and the header chrome (model picker, past conversations, new
 * conversation) — independent of where it is shown. The workspace tab
 * ({@link import('./brahma-console-view').BrahmaConsoleView}) and the always-on
 * floating dialog ({@link import('./brahma-console-dialog').BrahmaConsoleDialog})
 * both render it, each with its own {@link BrahmaConsoleSession}.
 *
 * v4's render-prop hands the host two pieces (`headerActions`, `body`). Here
 * the panel renders the body, and {@link BrahmaConsoleHeaderActions} renders
 * the chrome from the panel instance the host passes it — each host places
 * that component in its own frame (the dialog's title bar, the tab's header
 * row).
 *
 * The streaming consumer rides the GLOBAL event stream scope-tagged by
 * `chatId` (v5's D3 boundary), folding the frames through the shared
 * `chat-stream.reducer`. The `send` dispatch resolves at run completion; the
 * panel reconciles by reloading the persisted transcript (v4 `fetchChat()` on
 * done), which carries the settled tool cards so the live overlay clears.
 *
 * @module brahma/brahma-console-panel
 */

import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  input,
  signal,
  viewChild,
} from '@angular/core';
import { filter } from 'rxjs';

import {
  initialChatStreamState,
  reduceChatFrame,
  type ChatStreamState,
} from '../core/chat-stream.reducer';
import { CoreClient } from '../core/core-client';
import { CoreDispatchError } from '../core/core-contract';
import { Icon } from '../ui/icon';
import { BrahmaConsoleMessageList } from './brahma-console-message-list';
import type { BrahmaConsoleSession } from './brahma-console-session';
import { BrahmaConsoleService } from './brahma-console.service';
import { BrahmaConsoleApi, type BrahmaConsoleMessage, type BrahmaPastChat } from './brahma-wire';
import { HelpComposer } from './help-composer';
import { ModelPicker } from './model-picker';

@Component({
  selector: 'qt-brahma-console-panel',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [HelpComposer, BrahmaConsoleMessageList, Icon],
  host: { class: 'flex-1 min-h-0 flex flex-col' },
  template: `
    @if (!currentChatId()) {
      <!-- Launcher: past chats + opening composer -->
      <div class="flex flex-col h-full">
        <div class="flex-1 overflow-y-auto">
          @if (!isEligible()) {
            <div class="p-4 text-sm qt-text-secondary">
              The Console wants for an engine. Establish a connection profile first, then return to
              confer.
            </div>
          } @else if (pastChats().length > 0) {
            <div class="p-3">
              <div class="qt-help-section-label">Recent Console Conversations</div>
              <div class="flex flex-col gap-1">
                @for (chat of pastChats(); track chat.id) {
                  <div class="qt-help-past-chat group">
                    <button
                      type="button"
                      class="flex-1 text-left truncate text-sm"
                      (click)="selectChat(chat.id)"
                    >
                      {{ chat.title || 'Untitled' }}
                    </button>
                    <span class="text-xs qt-text-secondary">{{ chat.messageCount }}</span>
                    <button
                      type="button"
                      class="opacity-0 group-hover:opacity-100 p-0.5 rounded qt-text-secondary hover:qt-text-destructive transition-all"
                      title="Delete"
                      (click)="$event.stopPropagation(); handleDeleteChat(chat.id)"
                    >
                      <qt-icon name="close" class="w-3.5 h-3.5" />
                    </button>
                  </div>
                }
              </div>
            </div>
          } @else {
            <div class="p-4 text-sm qt-text-secondary">
              No prior conversations. Pose a question below to open a fresh line to the engine.
            </div>
          }
        </div>

        <qt-help-composer
          #composer
          [disabled]="!isEligible()"
          placeholder="What shall we put to the engine?"
          (send)="handleSend($event)"
        />
      </div>
    } @else {
      <!-- Conversation view -->
      <div class="flex flex-col h-full">
        @if (streamError()) {
          <div class="qt-help-error">{{ streamError() }}</div>
        }

        <qt-brahma-console-message-list
          [messages]="messages()"
          [streamingContent]="streamingContent()"
          [streamingReasoning]="streamingReasoning()"
          [streamingToolCalls]="streamingToolCalls()"
          [isStreaming]="isStreaming()"
          [isExecutingTools]="isExecutingTools()"
        />

        <qt-help-composer
          #composer
          [disabled]="isStreaming() || loadingMessages()"
          placeholder="Speak to the engine…"
          (send)="handleSend($event)"
        />
      </div>
    }
  `,
})
export class BrahmaConsolePanel {
  /** Which conversation / model this surface is on (v4 `session`). */
  readonly session = input.required<BrahmaConsoleSession>();
  /** Whether the surface is showing — gates the past-chats fetch (v4 `active`). */
  readonly active = input.required<boolean>();

  private readonly service = inject(BrahmaConsoleService);
  private readonly api = inject(BrahmaConsoleApi);
  private readonly core = inject(CoreClient);

  readonly isEligible = this.service.isEligible;
  readonly profiles = this.service.profiles;
  readonly currentChatId = computed(() => this.session().currentChatId());

  protected readonly pastChats = signal<BrahmaPastChat[]>([]);
  protected readonly messages = signal<BrahmaConsoleMessage[]>([]);
  protected readonly loadingMessages = signal(false);
  /** The live folded stream state; null when no turn is in flight. */
  private readonly stream = signal<ChatStreamState | null>(null);
  protected readonly streamError = signal<string | null>(null);

  /** v4's one `composerInputRef` — whichever composer is showing. */
  private readonly composer = viewChild<HelpComposer>('composer');

  protected readonly isStreaming = computed(() => this.stream() != null);
  protected readonly streamingContent = computed(() => this.stream()?.content ?? '');
  protected readonly streamingReasoning = computed(() => this.stream()?.reasoning ?? '');
  protected readonly streamingToolCalls = computed(() =>
    (this.stream()?.toolBatches ?? []).flatMap((b) => b.calls),
  );
  protected readonly isExecutingTools = computed(() => {
    const s = this.stream();
    return !!s?.status && !s.content;
  });

  /**
   * v4 `:220-223` — on the launcher the picker chooses the engine the next
   * conversation opens on; until the operator picks one, that is the default
   * profile.
   */
  readonly pickerActiveId = computed(
    () =>
      this.session().activeConnectionProfileId() ??
      (this.currentChatId() ? null : (this.profiles().find((p) => p.isDefault)?.id ?? null)),
  );

  constructor() {
    // v4 `:125-129` — the past-chats query is `enabled: active && !currentChatId`.
    effect(() => {
      if (this.active() && !this.currentChatId()) void this.refetchPastChats();
    });

    // Load messages when the active chat changes (v4 effect).
    effect(() => {
      const chatId = this.currentChatId();
      if (chatId) void this.loadMessages(chatId);
      else this.messages.set([]);
    });

    // Re-focus the composer when streaming finishes (v4 effect).
    let wasStreaming = false;
    effect(() => {
      const streaming = this.isStreaming();
      if (wasStreaming && !streaming) {
        setTimeout(() => this.composer()?.focus(), 100);
      }
      wasStreaming = streaming;
    });
  }

  setModel(id: string): void {
    void this.session().setModel(id);
  }

  protected selectChat(id: string): void {
    this.session().setCurrentChatId(id);
  }

  /**
   * Back to the launcher: the past-conversations roster plus a fresh composer
   * (v4 `:197-204`). Both header buttons — `Past conversations` and `New
   * conversation` — land here.
   */
  handleShowLauncher(): void {
    const session = this.session();
    session.setCurrentChatId(null);
    this.messages.set([]);
    session.setActiveConnectionProfileId(null);
    void this.refetchPastChats();
    setTimeout(() => this.composer()?.focus(), 50);
  }

  // -------------------------------------------------------------------------
  // Loads
  // -------------------------------------------------------------------------

  private async refetchPastChats(): Promise<void> {
    try {
      this.pastChats.set(await this.api.list());
    } catch (error) {
      console.error('Failed to load Brahma Console chats:', error);
    }
  }

  private async loadMessages(chatId: string): Promise<void> {
    const session = this.session();
    this.loadingMessages.set(true);
    try {
      try {
        this.messages.set(await this.api.messages(chatId));
      } catch (error) {
        // v4 `:87-90`: "The remembered conversation was deleted elsewhere — fall
        // back to the launcher." v5's dispatch carries the 404 as the error's
        // `kind` (`api/brahma.rs` `verify_brahma_chat` → `not_found()`).
        if (error instanceof CoreDispatchError && error.kind === 'not-found') {
          session.setCurrentChatId(null);
          return;
        }
        throw error;
      }
      // Sync the active model from the chat record.
      const chat = await this.api.get(chatId);
      session.setActiveConnectionProfileId(chat?.consoleConnectionProfileId ?? null);
    } catch (error) {
      console.error('Failed to load Brahma Console messages:', error);
    } finally {
      this.loadingMessages.set(false);
    }
  }

  // -------------------------------------------------------------------------
  // Send / create / delete
  // -------------------------------------------------------------------------

  protected handleSend(content: string): void {
    const optimistic: BrahmaConsoleMessage = {
      id: `optimistic-${Date.now()}`,
      role: 'USER',
      content,
      createdAt: new Date().toISOString(),
    };
    this.messages.update((m) => [...m, optimistic]);

    if (!this.currentChatId()) {
      void this.handleCreateChat(content);
    } else {
      void this.runSend(content);
    }
  }

  private async handleCreateChat(question: string): Promise<void> {
    const session = this.session();
    try {
      const chat = await this.api.create(session.activeConnectionProfileId() ?? undefined);
      const chatId = chat?.id;
      if (chatId) {
        session.setActiveConnectionProfileId(chat?.consoleConnectionProfileId ?? null);
        session.setCurrentChatId(chatId);
        await this.runSend(question, chatId);
      }
    } catch (error) {
      console.error('Failed to create Brahma Console chat:', error);
    }
  }

  /**
   * The streaming consumer: subscribe the event stream (scope-tagged by chatId)
   * BEFORE dispatching, fold each frame through the shared reducer, and reconcile
   * against the persisted transcript when the run completes.
   */
  private async runSend(content: string, overrideChatId?: string): Promise<void> {
    const chatId = overrideChatId ?? this.currentChatId();
    if (!chatId) return;

    this.streamError.set(null);
    let state: ChatStreamState = { ...initialChatStreamState(), waitingForResponse: true };
    this.stream.set(state);

    const sub = this.core.events$
      .pipe(filter((frame) => frame.chatId === chatId))
      .subscribe((frame) => {
        state = reduceChatFrame(state, frame);
        this.stream.set(state);
        if (state.error) this.streamError.set(state.error);
      });

    try {
      await this.api.send(chatId, content);
    } catch (err) {
      this.streamError.set(err instanceof Error ? err.message : 'Failed to send message');
    } finally {
      sub.unsubscribe();
    }

    // Reconcile: reload the transcript (it carries the settled tool cards) and
    // refresh the launcher counts, then clear the live overlay.
    await this.loadMessages(chatId);
    void this.refetchPastChats();
    this.stream.set(null);
  }

  protected async handleDeleteChat(chatId: string): Promise<void> {
    const session = this.session();
    try {
      await this.api.delete(chatId);
      this.pastChats.update((cs) => cs.filter((c) => c.id !== chatId));
      await this.refetchPastChats();
      if (this.currentChatId() === chatId) {
        session.setCurrentChatId(null);
        this.messages.set([]);
      }
    } catch (error) {
      console.error('Failed to delete Brahma Console chat:', error);
    }
  }
}

/**
 * The panel's header chrome (v4 `BrahmaConsolePanel.tsx:225-253`'s
 * `headerActions`): the model picker ALWAYS while eligible, and — with a
 * conversation open — `Past conversations` then `New conversation`, both back
 * to the launcher. Renders nothing while ineligible (v4's `null`). The two
 * buttons carry v4's `title`s and no `aria-label` (C9 R6: none at the pin;
 * a `title` names a button — `check-a11y-names.mjs` counts it).
 */
@Component({
  selector: 'qt-brahma-console-header-actions',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, ModelPicker],
  template: `
    @if (panel().isEligible()) {
      <div class="flex items-center gap-1">
        <qt-brahma-model-picker
          [profiles]="panel().profiles()"
          [activeId]="panel().pickerActiveId()"
          (select)="panel().setModel($event)"
        />
        @if (panel().currentChatId()) {
          <button
            type="button"
            class="p-1 rounded qt-hover-accent qt-text-secondary transition-colors"
            title="Past conversations"
            (click)="panel().handleShowLauncher()"
          >
            <qt-icon name="clock" class="w-4 h-4" />
          </button>
          <button
            type="button"
            class="p-1 rounded qt-hover-accent qt-text-secondary transition-colors"
            title="New conversation"
            (click)="panel().handleShowLauncher()"
          >
            <qt-icon name="plus" class="w-4 h-4" />
          </button>
        }
      </div>
    }
  `,
})
export class BrahmaConsoleHeaderActions {
  readonly panel = input.required<BrahmaConsolePanel>();
}
