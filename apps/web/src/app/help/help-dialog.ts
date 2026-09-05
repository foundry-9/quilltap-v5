/**
 * `HelpChatDialog` — port of v4 `components/help-chat/HelpChatDialog.tsx`.
 *
 * Two tabs: **Guide** (browseable documentation) and **Ask** (a conversational
 * help chat). The tab choice persists in `sessionStorage` under
 * `quilltap:help-tab` — session, not local, so it resets with the window.
 *
 * The Ask tab has two views. With no chat in hand: the launcher — the eligible
 * help seats as togglable pills, the operator's recent help chats, and an
 * opening composer. With a chat: the transcript, the composer, and a "new
 * conversation" button in the header.
 *
 * **Recorded divergence (the Brahma precedent):** v4 hosts this in a DRAGGABLE
 * `FloatingDialog`; v5 has no such primitive, so it lands as a centred
 * `qt-dialog-overlay`, exactly as the Brahma console did.
 *
 * The optimistic user bubble is pushed into the SAME message array a reload
 * replaces (the P4.66 shape, dogfood #106) — never a separate signal appended at
 * render, which is what made the user's message show twice mid-turn.
 *
 * @module help/help-dialog
 */

import { NgTemplateOutlet } from '@angular/common';
import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  signal,
  viewChild,
} from '@angular/core';

import { HelpComposer } from '../brahma/help-composer';
import { Icon } from '../ui/icon';
import { HelpEntityPicker, hasParamSegments } from './help-entity-picker';
import { HelpGuideTab } from './help-guide-tab';
import { HelpMessageList, type HelpCharacterInfo } from './help-message-list';
import { HelpNavigate } from './help-navigate';
import { HelpStreamingService } from './help-streaming.service';
import {
  HelpApi,
  type HelpChatMessage,
  type HelpChatParticipant,
  type HelpPastChat,
} from './help-wire';
import { HelpService } from './help.service';

type HelpTab = 'guide' | 'ask';

/** sessionStorage key for the active tab (v4 `quilltap:help-tab`). */
export const STORAGE_KEY_TAB = 'quilltap:help-tab';

function getInitialTab(): HelpTab {
  try {
    const stored = sessionStorage.getItem(STORAGE_KEY_TAB);
    if (stored === 'guide' || stored === 'ask') return stored;
  } catch {
    /* storage unavailable */
  }
  return 'guide';
}

@Component({
  selector: 'qt-help-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    NgTemplateOutlet,
    Icon,
    HelpComposer,
    HelpGuideTab,
    HelpMessageList,
    HelpEntityPicker,
  ],
  template: `
    @if (isOpen()) {
      <div class="qt-dialog-overlay" (click)="close()">
        <div
          class="qt-dialog flex flex-col"
          role="dialog"
          aria-modal="true"
          aria-label="Help"
          style="width: 560px; max-width: 90vw; height: 80vh; max-height: 90vh"
          (click)="$event.stopPropagation()"
        >
          <div class="qt-dialog-header flex items-center justify-between gap-4">
            <h2 class="qt-dialog-title">Help</h2>
            <div class="flex items-center gap-1">
              @if (activeTab() === 'ask' && currentChatId()) {
                <button
                  type="button"
                  class="p-1 rounded qt-hover-accent qt-text-secondary transition-colors"
                  title="New help chat"
                  (click)="handleNewChat()"
                >
                  <qt-icon name="plus" class="w-4 h-4" />
                </button>
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
            <!-- Entity picker overlay for parameterised URLs -->
            @if (pendingParamUrl(); as tpl) {
              <qt-help-entity-picker
                [urlTemplate]="tpl"
                (selectEntity)="handleEntityPicked($event)"
                (cancel)="pendingParamUrl.set(null)"
              />
            }

            <!-- Tab bar -->
            <div class="flex-shrink-0 px-3 pt-2" role="tablist" aria-label="Help tabs">
              <div class="qt-tab-group">
                <button
                  type="button"
                  role="tab"
                  class="qt-tab"
                  [class.qt-tab-active]="activeTab() === 'guide'"
                  [attr.aria-selected]="activeTab() === 'guide'"
                  (click)="handleTabChange('guide')"
                >
                  Guide
                </button>
                <button
                  type="button"
                  role="tab"
                  class="qt-tab"
                  [class.qt-tab-active]="activeTab() === 'ask'"
                  [attr.aria-selected]="activeTab() === 'ask'"
                  (click)="handleTabChange('ask')"
                >
                  Ask
                </button>
              </div>
              <div class="qt-tab-divider"></div>
            </div>

            <!-- Tab content -->
            <div class="flex-1 min-h-0" role="tabpanel">
              @if (activeTab() === 'guide') {
                <qt-help-guide-tab />
              } @else {
                <ng-container [ngTemplateOutlet]="askTab" />
              }
            </div>
          </div>
        </div>
      </div>
    }

    <ng-template #askTab>
      @if (!currentChatId()) {
        <!-- Launcher view -->
        <div class="flex flex-col h-full">
          <!-- Character selection -->
          <div class="p-3 border-b qt-border-default">
            <div class="qt-help-section-label">Help Characters</div>
            <div class="flex flex-wrap gap-2">
              @for (char of toolCapable(); track char.id) {
                <button
                  type="button"
                  class="qt-help-char-pill"
                  [attr.data-selected]="selectedCharacterIds().includes(char.id)"
                  [title]="char.name"
                  (click)="toggleCharacter(char.id)"
                >
                  <div class="w-5 h-5 rounded-full qt-bg-muted overflow-hidden flex-shrink-0">
                    @if (char.avatarUrl) {
                      <img [src]="char.avatarUrl" alt="" class="w-full h-full object-cover" />
                    } @else {
                      <span class="flex items-center justify-center w-full h-full text-[10px]">{{
                        char.name[0]
                      }}</span>
                    }
                  </div>
                  {{ char.name }}
                </button>
              }
              @if (toolCapable().length === 0) {
                <div class="text-xs qt-text-secondary">
                  No eligible help characters. Enable help tools on a character with a tool-capable
                  connection profile.
                </div>
              }
            </div>
          </div>

          <!-- Past chats -->
          <div class="flex-1 overflow-y-auto">
            @if (pastChats().length > 0) {
              <div class="p-3">
                <div class="qt-help-section-label">Recent Help Chats</div>
                <div class="flex flex-col gap-1">
                  @for (chat of pastChats(); track chat.id) {
                    <div class="qt-help-past-chat group">
                      <button
                        type="button"
                        class="flex-1 text-left truncate text-sm"
                        (click)="handleSelectPastChat(chat.id)"
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
            }
          </div>

          <!-- Question input -->
          <qt-help-composer
            [disabled]="toolCapable().length === 0"
            placeholder="What would you like help with?"
            (send)="handleSend($event)"
          />
        </div>
      } @else {
        <!-- Chat view -->
        <div class="flex flex-col h-full">
          @if (streamError()) {
            <div class="qt-help-error">{{ streamError() }}</div>
          }

          <qt-help-message-list
            [messages]="messages()"
            [characterMap]="characterMap()"
            [participantToCharacter]="participantToCharacter()"
            [streamingContent]="streamingContent()"
            [streamingParticipantId]="streamingParticipantId()"
            [isStreaming]="isStreaming()"
            [isExecutingTools]="isExecutingTools()"
            [navigationLinks]="navigationLinks()"
            [suggestedLinks]="suggestedLinks()"
            (navigate)="handleNavigate($event)"
          />

          <qt-help-composer
            #conversationComposer
            [disabled]="isStreaming() || loadingMessages()"
            (send)="handleSend($event)"
          />
        </div>
      }
    </ng-template>
  `,
})
export class HelpDialog {
  private readonly help = inject(HelpService);
  private readonly api = inject(HelpApi);
  private readonly stream = inject(HelpStreamingService);
  private readonly navigator = inject(HelpNavigate);

  // --- service-backed state (shared) ---
  protected readonly isOpen = this.help.isOpen;
  protected readonly currentChatId = this.help.currentChatId;
  protected readonly selectedCharacterIds = this.help.selectedCharacterIds;
  protected readonly toolCapable = this.help.toolCapableCharacters;

  // --- streaming projections ---
  protected readonly isStreaming = this.stream.isStreaming;
  protected readonly isExecutingTools = this.stream.isExecutingTools;
  protected readonly streamingContent = this.stream.streamingContent;
  protected readonly streamingParticipantId = this.stream.streamingParticipantId;
  protected readonly navigationLinks = this.stream.streamingNavigationLinks;
  protected readonly suggestedLinks = this.stream.suggestedLinks;
  protected readonly streamError = this.stream.error;

  // --- local view state ---
  protected readonly activeTab = signal<HelpTab>(getInitialTab());
  protected readonly pastChats = signal<HelpPastChat[]>([]);
  protected readonly messages = signal<HelpChatMessage[]>([]);
  protected readonly characterMap = signal<Map<string, HelpCharacterInfo>>(new Map());
  protected readonly participantToCharacter = signal<Map<string, string>>(new Map());
  protected readonly loadingMessages = signal(false);
  /** URL template pending entity selection, e.g. "/aurora/:id/edit" */
  protected readonly pendingParamUrl = signal<string | null>(null);

  private readonly conversationComposer = viewChild<HelpComposer>('conversationComposer');

  constructor() {
    // v4's past-chats query: enabled only when open, with no chat, on Ask.
    effect(() => {
      const shouldLoad = this.isOpen() && !this.currentChatId() && this.activeTab() === 'ask';
      if (shouldLoad) void this.refetchPastChats();
    });

    // Load messages when the active chat changes (v4's effect).
    effect(() => {
      const chatId = this.currentChatId();
      if (chatId) void this.loadMessages(chatId);
      else this.messages.set([]);
    });

    // Re-focus the composer when streaming completes (v4's effect, 100 ms).
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
    this.help.closeHelpChat();
  }

  protected toggleCharacter(id: string): void {
    this.help.toggleCharacter(id);
  }

  /** Persist the tab choice for the session (v4 `handleTabChange`). */
  protected handleTabChange(tab: HelpTab): void {
    this.activeTab.set(tab);
    try {
      sessionStorage.setItem(STORAGE_KEY_TAB, tab);
    } catch {
      /* ignore */
    }
  }

  /**
   * A link with an unresolved `:id` opens the entity picker; anything else
   * navigates (keep-alive-safe inside the workspace).
   */
  protected handleNavigate(url: string): void {
    if (hasParamSegments(url)) {
      this.pendingParamUrl.set(url);
    } else {
      this.navigator.go(url);
    }
  }

  protected handleEntityPicked(resolvedUrl: string): void {
    this.pendingParamUrl.set(null);
    this.navigator.go(resolvedUrl);
  }

  // -------------------------------------------------------------------------
  // Loads
  // -------------------------------------------------------------------------

  private async refetchPastChats(): Promise<void> {
    try {
      this.pastChats.set(await this.api.chatList());
    } catch (error) {
      console.error('Failed to load help chats:', error);
    }
  }

  /**
   * v4 loads the transcript, then the chat record for its participant maps —
   * two calls, because the messages route carries only `participantId`s.
   */
  private async loadMessages(chatId: string): Promise<void> {
    this.loadingMessages.set(true);
    try {
      this.messages.set(await this.api.chatMessages(chatId));
      const chat = await this.api.chatGet(chatId);
      if (chat?.participants) this.buildParticipantMaps(chat.participants);
    } catch (error) {
      console.error('Failed to load help chat messages:', error);
    } finally {
      this.loadingMessages.set(false);
    }
  }

  /**
   * v4 `buildParticipantMaps` — reads BOTH the nested and the flattened
   * participant shape (`p.character?.id || p.characterId`), because the create
   * reply and the get projection do not agree on which one they send. A
   * participant missing either an id or a name is skipped entirely.
   */
  private buildParticipantMaps(participants: HelpChatParticipant[]): void {
    const charMap = new Map<string, HelpCharacterInfo>();
    const ptcMap = new Map<string, string>();
    for (const p of participants) {
      const charId = p.character?.id || p.characterId;
      const charName = p.character?.name || p.name;
      const charAvatar = p.character?.avatarUrl ?? p.avatarUrl ?? null;
      if (charId && charName) {
        charMap.set(charId, { id: charId, name: charName, avatarUrl: charAvatar });
        ptcMap.set(p.id, charId);
      }
    }
    this.characterMap.set(charMap);
    this.participantToCharacter.set(ptcMap);
  }

  // -------------------------------------------------------------------------
  // Send / create / new / delete
  // -------------------------------------------------------------------------

  /**
   * v4 pushes the optimistic bubble into the message ARRAY, so the reload that
   * follows the turn replaces it. Keeping it in a separate signal is what caused
   * dogfood #106 in the Salon — don't repeat it here.
   */
  protected handleSend(content: string): void {
    const optimistic: HelpChatMessage = {
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

  /**
   * v4 `handleCreateChat`: the picked seats, narrowed to the ones that are
   * actually tool-capable, falling back to the FIRST eligible when the
   * selection is empty or stale. With nothing eligible it does nothing at all.
   */
  private async handleCreateChat(question: string): Promise<void> {
    const eligible = this.toolCapable();
    const charIds = this.selectedCharacterIds().filter((id) => eligible.some((c) => c.id === id));
    if (charIds.length === 0 && eligible.length > 0) {
      charIds.push(eligible[0].id);
    }
    if (charIds.length === 0) return;

    try {
      const chat = await this.api.chatCreate(charIds, this.help.currentPageUrl());
      const chatId = chat?.id;
      if (chatId) {
        this.help.setCurrentChatId(chatId);
        if (chat?.participants) this.buildParticipantMaps(chat.participants);
        await this.runSend(question, chatId);
      }
    } catch (error) {
      console.error('Failed to create help chat:', error);
    }
  }

  private async runSend(content: string, overrideChatId?: string): Promise<void> {
    const chatId = overrideChatId ?? this.currentChatId();
    if (!chatId) return;
    await this.stream.sendMessage(chatId, content, () => void this.loadMessages(chatId));
    // Reconcile against the persisted transcript, then drop the live overlay.
    await this.loadMessages(chatId);
    void this.refetchPastChats();
    this.stream.reset();
  }

  protected handleSelectPastChat(chatId: string): void {
    this.help.setCurrentChatId(chatId);
  }

  protected handleNewChat(): void {
    this.help.setCurrentChatId(null);
    this.messages.set([]);
  }

  protected async handleDeleteChat(chatId: string): Promise<void> {
    try {
      await this.api.chatDelete(chatId);
      this.pastChats.update((cs) => cs.filter((c) => c.id !== chatId));
      await this.refetchPastChats();
      if (this.currentChatId() === chatId) {
        this.help.setCurrentChatId(null);
        this.messages.set([]);
      }
    } catch (error) {
      console.error('Failed to delete help chat:', error);
    }
  }
}
