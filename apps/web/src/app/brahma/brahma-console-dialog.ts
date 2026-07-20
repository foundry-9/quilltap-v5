/**
 * BrahmaConsoleDialog (port of v4
 * `components/brahma-console/BrahmaConsoleDialog.tsx`).
 *
 * The whole console: a single character-less conversation pane — no character
 * tabs, no guide tab. The launcher opens to the operator's past Console chats
 * with a "New conversation" affordance; selecting one resumes it. The header
 * carries a model picker so the engine can be switched at any time (the same
 * chat continues).
 *
 * Two modes (v4 `asTab`):
 *  - **asTab=false (floating):** an overlay dialog on the `qt-dialog-overlay`
 *    pattern, shown while the service's `isOpen` is set. (v4 uses a draggable
 *    FloatingDialog; v5 has no such primitive, so this is the documented
 *    divergence — a centered modal instead.)
 *  - **asTab=true (workspace tab):** renders the body bare with an inline
 *    right-aligned header, live regardless of `isOpen` (the past-chats fetch is
 *    gated `(isOpen || asTab) && !currentChatId`).
 *
 * The streaming consumer rides the GLOBAL event stream scope-tagged by `chatId`
 * (v5's D3 boundary), folding the seven frames through the shared
 * `chat-stream.reducer`. The `send` dispatch resolves at run completion; the
 * console reconciles by reloading the persisted transcript (v4 `fetchChat()` on
 * done), which carries the settled tool cards so the live overlay clears.
 *
 * @module brahma/brahma-console-dialog
 */

import { NgTemplateOutlet } from '@angular/common';
import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
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
import { Icon } from '../ui/icon';
import { BrahmaConsoleMessageList } from './brahma-console-message-list';
import { BrahmaConsoleService } from './brahma-console.service';
import { BrahmaConsoleApi, type BrahmaConsoleMessage, type BrahmaPastChat } from './brahma-wire';
import { HelpComposer } from './help-composer';
import { ModelPicker } from './model-picker';

@Component({
  selector: 'qt-brahma-console-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [NgTemplateOutlet, Icon, HelpComposer, ModelPicker, BrahmaConsoleMessageList],
  template: `
    @if (asTab()) {
      <div class="flex flex-col h-full min-h-0">
        @if (currentChatId()) {
          <div
            class="flex items-center justify-end gap-1 px-3 py-2 border-b qt-border-default/60"
          >
            <ng-container [ngTemplateOutlet]="headerActions" />
          </div>
        }
        <ng-container [ngTemplateOutlet]="body" />
      </div>
    } @else if (isOpen()) {
      <div class="qt-dialog-overlay" (click)="close()">
        <div
          class="qt-dialog flex flex-col"
          role="dialog"
          aria-modal="true"
          aria-label="Brahma Console"
          style="width: 560px; max-width: 90vw; height: 80vh; max-height: 90vh"
          (click)="$event.stopPropagation()"
        >
          <div class="qt-dialog-header flex items-center justify-between gap-4">
            <h2 class="qt-dialog-title">Brahma Console</h2>
            <div class="flex items-center gap-1">
              @if (currentChatId()) {
                <ng-container [ngTemplateOutlet]="headerActions" />
              }
              <button
                type="button"
                class="qt-button-ghost qt-button-sm"
                aria-label="Close"
                (click)="close()"
              >
                <qt-icon name="close" class="w-4 h-4" />
              </button>
            </div>
          </div>
          <div class="qt-dialog-body flex flex-col flex-1 min-h-0 overflow-hidden !p-0">
            <ng-container [ngTemplateOutlet]="body" />
          </div>
        </div>
      </div>
    }

    <!-- The header chrome: model picker + new conversation (shown with a chat open). -->
    <ng-template #headerActions>
      <qt-brahma-model-picker
        [profiles]="profiles()"
        [activeId]="activeId()"
        (select)="setModel($event)"
      />
      <button
        type="button"
        class="p-1 rounded qt-hover-accent qt-text-secondary transition-colors"
        title="New conversation"
        (click)="handleNewChat()"
      >
        <qt-icon name="plus" class="w-4 h-4" />
      </button>
    </ng-template>

    <!-- The body: the launcher (no chat) or the conversation view. -->
    <ng-template #body>
      <div class="flex-1 min-h-0 flex flex-col">
        @if (!currentChatId()) {
          <!-- Launcher: past chats + opening composer -->
          <div class="flex flex-col h-full">
            <div class="flex-1 overflow-y-auto">
              @if (!isEligible()) {
                <div class="p-4 text-sm qt-text-secondary">
                  The Console wants for an engine. Establish a connection profile first, then return
                  to confer.
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
              #conversationComposer
              [disabled]="isStreaming() || loadingMessages()"
              placeholder="Speak to the engine…"
              (send)="handleSend($event)"
            />
          </div>
        }
      </div>
    </ng-template>
  `,
})
export class BrahmaConsoleDialog {
  /**
   * When true, render the body bare (a workspace tab) instead of the floating
   * overlay, and stay live regardless of `isOpen`.
   */
  readonly asTab = input(false);

  private readonly service = inject(BrahmaConsoleService);
  private readonly api = inject(BrahmaConsoleApi);
  private readonly core = inject(CoreClient);
  private readonly destroyRef = inject(DestroyRef);

  // --- service-backed state (shared) ---
  protected readonly isOpen = this.service.isOpen;
  protected readonly currentChatId = this.service.currentChatId;
  protected readonly isEligible = this.service.isEligible;
  protected readonly profiles = this.service.profiles;
  protected readonly activeId = this.service.activeConnectionProfileId;

  // --- local view state ---
  protected readonly pastChats = signal<BrahmaPastChat[]>([]);
  protected readonly messages = signal<BrahmaConsoleMessage[]>([]);
  protected readonly loadingMessages = signal(false);
  /** The live folded stream state; null when no turn is in flight. */
  private readonly stream = signal<ChatStreamState | null>(null);
  protected readonly streamError = signal<string | null>(null);

  private readonly conversationComposer = viewChild<HelpComposer>('conversationComposer');

  // --- streaming projections for the message list ---
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

  constructor() {
    // Load the past-chats launcher when open (or as a tab) and no chat is
    // selected (v4 query `enabled: (isOpen || asTab) && !currentChatId`).
    effect(() => {
      const shouldLoad = (this.isOpen() || this.asTab()) && !this.currentChatId();
      if (shouldLoad) void this.refetchPastChats();
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
        setTimeout(() => this.conversationComposer()?.focus(), 100);
      }
      wasStreaming = streaming;
    });
  }

  protected close(): void {
    this.service.closeConsole();
  }

  protected setModel(id: string): void {
    void this.service.setModel(id);
  }

  protected selectChat(id: string): void {
    this.service.setCurrentChatId(id);
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
    this.loadingMessages.set(true);
    try {
      this.messages.set(await this.api.messages(chatId));
      // Sync the active model from the chat record.
      const chat = await this.api.get(chatId);
      this.service.setActiveConnectionProfileId(chat?.consoleConnectionProfileId ?? null);
    } catch (error) {
      console.error('Failed to load Brahma Console messages:', error);
    } finally {
      this.loadingMessages.set(false);
    }
  }

  // -------------------------------------------------------------------------
  // Send / create / new / delete
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
    try {
      const chat = await this.api.create(this.service.activeConnectionProfileId() ?? undefined);
      const chatId = chat?.id;
      if (chatId) {
        this.service.setActiveConnectionProfileId(chat?.consoleConnectionProfileId ?? null);
        this.service.setCurrentChatId(chatId);
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

  protected handleNewChat(): void {
    this.service.setCurrentChatId(null);
    this.messages.set([]);
    void this.refetchPastChats();
  }

  protected async handleDeleteChat(chatId: string): Promise<void> {
    try {
      await this.api.delete(chatId);
      this.pastChats.update((cs) => cs.filter((c) => c.id !== chatId));
      await this.refetchPastChats();
      if (this.currentChatId() === chatId) {
        this.service.setCurrentChatId(null);
        this.messages.set([]);
      }
    } catch (error) {
      console.error('Failed to delete Brahma Console chat:', error);
    }
  }
}
