import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  signal,
  viewChild,
} from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute, RouterLink } from '@angular/router';
import { filter } from 'rxjs';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { ChatComposer } from '../../chat/chat-composer';
import { ConversationHeader } from '../../chat/conversation-header';
import { MessageList } from '../../chat/message-list';
import { MemoryCascadeDialog, type MemoryCascadeAction } from '../../chat/memory-cascade-dialog';
import { splitSwipeGroups, type SwipeState } from '../../chat/chat-view-model';
import { TurnControls } from '../../chat/turn-controls';
import { type ControlledCharacter } from '../../chat/speaker-selector';
import { isParticipantPresent } from '../../chat/skip-signal-helpers';
import {
  computeSkipEligibility,
  type SkipEvent,
  type SkipParticipant,
} from '../../chat/skip-signal';
import {
  initialChatStreamState,
  reduceChatFrame,
  type ChatStreamState,
} from '../../core/chat-stream.reducer';
import { CoreClient } from '../../core/core-client';
import type {
  ChatDetail,
  ChatSettingsDto,
  MessageDto,
  ParticipantDetail,
} from '../../core/core-contract';
import { ErrorAlert } from '../../ui/error-alert';
import { LoadingState } from '../../ui/loading-state';

/** The next-speaker projection off `chatTurnAction { action: 'query' }`. */
interface TurnInfo {
  nextSpeakerId: string | null;
  nextSpeakerControlledBy: string | null;
}

/** A pending delete awaiting the memory-cascade choice. */
interface CascadePrompt {
  messageId: string;
  memoryCount: number;
  isSwipeGroup: boolean;
}

/**
 * One Salon conversation (v4 `SalonView`, slimmed): the read path (chat +
 * settings via `chatGet`/`chatSettings`, swipe-group collapsing), the live send
 * (optimistic user bubble → stream-reducer over `CoreClient.events$` → refetch on
 * done), and tier-1 message actions. The god-component's panes, sidebar, and
 * toolbar plumbing are deferrals.
 */
@Component({
  selector: 'qt-salon-conversation',
  // The host must span the shell's scroller exactly (v4 renders
  // .qt-chat-layout h-full directly; Angular's host element sits in between).
  host: { class: 'block h-full' },
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    RouterLink,
    LoadingState,
    ErrorAlert,
    ConversationHeader,
    MessageList,
    TurnControls,
    ChatComposer,
    MemoryCascadeDialog,
  ],
  template: `
    <div class="qt-chat-layout">
      <div class="qt-chat-main">
        @if (chatQuery.isPending()) {
          <qt-loading-state message="Loading chat..." />
        } @else if (chatQuery.isError() || !chat()) {
          <qt-error-alert
            [message]="'Error: ' + errorMessage()"
            [retryable]="true"
            (retry)="chatQuery.refetch()"
          />
          <div class="p-4">
            <a routerLink="/salon" class="qt-link">← Back to chats</a>
          </div>
        } @else {
          <qt-conversation-header [chat]="chat()!" />

          <div class="qt-chat-messages-viewport">
            <qt-message-list
              [messages]="displayMessages()"
              [chat]="chat()!"
              [swipeStates]="effectiveSwipeStates()"
              [settings]="settings()"
              [stream]="stream()"
              [editingId]="editingId()"
              (copy)="onCopy($event)"
              (edit)="onEdit($event)"
              (delete)="onDelete($event)"
              (regenerate)="onRegenerate($event)"
              (swipePrev)="onSwipe($event, -1)"
              (swipeNext)="onSwipe($event, 1)"
              (saveEdit)="onSaveEdit($event)"
              (cancelEdit)="editingId.set(null)"
            />
          </div>

          <qt-turn-controls
            [controlledCharacters]="controlledCharacters()"
            [activeSpeakerId]="activeSpeakerId()"
            [disabled]="busy()"
            [isPaused]="chat()!.isPaused"
            [userTurnName]="userTurnName()"
            [mustSpeak]="mustSpeak()"
            [skipError]="skipError()"
            [nudgeTargetName]="nudgeTargetName()"
            (selectSpeaker)="onSelectSpeaker($event)"
            (skipUserTurn)="onSkipUserTurn()"
            (togglePause)="onTogglePause()"
            (nudge)="onNudge()"
          />

          <qt-chat-composer
            [busy]="busy()"
            [hasActiveCharacters]="hasActiveCharacters()"
            (send)="send($event)"
            (stop)="stop()"
            (continue)="continueTurn()"
          />
        }
      </div>
    </div>

    @if (cascade(); as c) {
      <qt-memory-cascade-dialog
        [memoryCount]="c.memoryCount"
        [isSwipeGroup]="c.isSwipeGroup"
        (confirm)="onCascadeConfirm($event)"
        (cancel)="cascade.set(null)"
      />
    }
  `,
})
export class SalonConversation {
  private readonly route = inject(ActivatedRoute);
  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();

  private readonly params = toSignal(this.route.paramMap, { requireSync: true });
  protected readonly chatId = computed(() => this.params().get('id'));

  /** The message list, so a user send can force a scroll-to-bottom (v4 `scrollOnUserMessage`). */
  private readonly messageList = viewChild(MessageList);

  protected readonly chatQuery = injectQuery(() => ({
    queryKey: ['chat', this.chatId()],
    enabled: !!this.chatId(),
    queryFn: async (): Promise<ChatDetail> => {
      const resp = await this.core.dispatchExpect(
        { type: 'chatGet', chatId: this.chatId()! },
        'chat',
      );
      return resp.data.chat;
    },
  }));

  private readonly settingsQuery = injectQuery(() => ({
    queryKey: ['chatSettings'],
    queryFn: async (): Promise<ChatSettingsDto> => {
      const resp = await this.core.dispatchExpect({ type: 'chatSettings' }, 'chatSettings');
      return resp.data;
    },
  }));

  protected readonly chat = computed(() => this.chatQuery.data() ?? null);
  protected readonly settings = computed(() => this.settingsQuery.data() ?? null);

  // --- streaming ---
  protected readonly stream = signal<ChatStreamState | null>(null);
  protected readonly busy = computed(() => this.stream() != null);
  private readonly optimisticUser = signal<MessageDto | null>(null);

  // --- client-side swipe switching (v4 `switchSwipe`) ---
  private readonly swipeOverride = signal<Record<string, number>>({});

  // --- inline edit + delete-cascade ---
  protected readonly editingId = signal<string | null>(null);
  protected readonly cascade = signal<CascadePrompt | null>(null);

  private readonly split = computed(() =>
    this.chat() ? splitSwipeGroups(this.chat()!.messages) : { messages: [], swipeStates: {} },
  );

  /** Swipe states with the client-side override applied to `current`. */
  protected readonly effectiveSwipeStates = computed<Record<string, SwipeState>>(() => {
    const base = this.split().swipeStates;
    const override = this.swipeOverride();
    const out: Record<string, SwipeState> = {};
    for (const [gid, st] of Object.entries(base)) {
      const current = override[gid] ?? st.current;
      out[gid] = { ...st, current };
    }
    return out;
  });

  /** The rendered flow: the collapsed messages (with swipe override) + optimistic user bubble. */
  protected readonly displayMessages = computed<MessageDto[]>(() => {
    const states = this.effectiveSwipeStates();
    const msgs = this.split().messages.map((m) => {
      if (m.swipeGroupId && states[m.swipeGroupId]) {
        const st = states[m.swipeGroupId];
        return st.messages[st.current] ?? m;
      }
      return m;
    });
    const temp = this.optimisticUser();
    return temp ? [...msgs, temp] : msgs;
  });

  protected readonly hasActiveCharacters = computed(() =>
    (this.chat()?.participants ?? []).some(
      (p) => p.type === 'CHARACTER' && p.isActive && p.controlledBy === 'llm',
    ),
  );

  // -------------------------------------------------------------------------
  // Turn management (Speaking-As, the user-turn Skip banner, pause, nudge)
  // -------------------------------------------------------------------------

  /** The authoritative next speaker from `chatTurnAction { action: 'query' }`. */
  private readonly turnInfo = signal<TurnInfo | null>(null);
  /** The user's Speaking-As choice (immediate feedback ahead of the refetch). */
  private readonly activeSpeakerOverride = signal<string | null>(null);
  /** A rejected-skip message (v4's all-others-skipped copy). */
  protected readonly skipError = signal<string | null>(null);

  /** Re-query the next speaker whenever the chat settles and no turn is running. */
  private readonly _turnEffect = effect(() => {
    const chat = this.chat();
    const busy = this.busy();
    if (chat && !busy) {
      void this.refreshTurn();
    }
  });

  private async refreshTurn(): Promise<void> {
    const chatId = this.chatId();
    if (!chatId) return;
    const resp = await this.core.dispatch({ type: 'chatTurnAction', chatId, action: 'query' });
    if (resp.type === 'turnAction') {
      const turn = (resp.data as { turn?: Partial<TurnInfo> }).turn;
      this.turnInfo.set({
        nextSpeakerId: turn?.nextSpeakerId ?? null,
        nextSpeakerControlledBy: turn?.nextSpeakerControlledBy ?? null,
      });
    } else {
      this.turnInfo.set(null);
    }
  }

  /** User-controlled, present characters — the Speaking-As selector's options. */
  protected readonly controlledCharacters = computed<ControlledCharacter[]>(() =>
    (this.chat()?.participants ?? [])
      .filter(
        (p) =>
          p.type === 'CHARACTER' &&
          p.controlledBy === 'user' &&
          p.isActive &&
          isParticipantPresent(p.status),
      )
      .map((p) => ({
        participantId: p.id,
        name: p.character?.name ?? 'Character',
        avatarUrl: participantAvatar(p),
      })),
  );

  protected readonly activeSpeakerId = computed(
    () => this.activeSpeakerOverride() ?? this.chat()?.activeTypingParticipantId ?? null,
  );

  private readonly nextSpeaker = computed<ParticipantDetail | null>(() => {
    const id = this.turnInfo()?.nextSpeakerId;
    if (!id) return null;
    return (this.chat()?.participants ?? []).find((p) => p.id === id) ?? null;
  });

  /** The name whose (user-controlled) turn it is, or null when it isn't. */
  protected readonly userTurnName = computed<string | null>(() => {
    if (this.busy()) return null;
    const next = this.nextSpeaker();
    if (!next || next.controlledBy !== 'user') return null;
    return next.character?.name ?? 'this character';
  });

  /** Everyone else has passed → the responder must speak (no Skip button). */
  protected readonly mustSpeak = computed<boolean>(() => {
    const chat = this.chat();
    const next = this.nextSpeaker();
    if (!chat || !next || next.controlledBy !== 'user' || !next.character) return false;
    try {
      const events: SkipEvent[] = chat.messages.map((m) => ({
        type: 'message',
        id: m.id,
        role: m.role,
        content: m.content,
        participantId: m.participantId,
        targetParticipantIds: m.targetParticipantIds,
        systemSender: m.systemSender,
        systemKind: m.systemKind,
        hostEvent: m.hostEvent,
        isSilentMessage: m.isSilentMessage,
      }));
      const participants: SkipParticipant[] = chat.participants.map((p) => ({
        id: p.id,
        type: p.type,
        characterId: p.character?.id ?? null,
        controlledBy: p.controlledBy,
        status: p.status,
      }));
      const eligibility = computeSkipEligibility({
        events,
        participants,
        respondingParticipantId: next.id,
        respondingCharacter: { id: next.character.id, name: next.character.name, aliases: [] },
        summoned: false,
        turnSkippingEnabled: chat.turnSkippingEnabled !== false,
      });
      return eligibility.mustSpeakReason === 'all-others-skipped';
    } catch {
      return false;
    }
  });

  /** The next LLM speaker's name — the Nudge target, or null when it's a user turn. */
  protected readonly nudgeTargetName = computed<string | null>(() => {
    if (this.busy()) return null;
    const info = this.turnInfo();
    if (!info?.nextSpeakerId || info.nextSpeakerControlledBy === 'user') return null;
    const next = this.nextSpeaker();
    return next?.character?.name ?? 'the next character';
  });

  protected onSelectSpeaker(participantId: string): void {
    this.activeSpeakerOverride.set(participantId);
    const chatId = this.chatId();
    if (!chatId) return;
    void this.core
      .dispatch({ type: 'chatSetActiveSpeaker', chatId, participantId })
      .then(() => this.queryClient.invalidateQueries({ queryKey: ['chat', chatId] }));
  }

  protected async onSkipUserTurn(): Promise<void> {
    const chatId = this.chatId();
    const target = this.nextSpeaker();
    if (!chatId || !target) return;
    this.skipError.set(null);
    const resp = await this.core.dispatch({
      type: 'chatTurnAction',
      chatId,
      action: 'skipUserTurn',
      participantId: target.id,
    });
    if (resp.type === 'error') {
      this.skipError.set(resp.data.message);
      return;
    }
    await this.queryClient.invalidateQueries({ queryKey: ['chat', chatId] });
    // v4 `handleSkipUserTurn`: if the skip hands the turn to an LLM, generate.
    const turn = resp.type === 'turnAction' ? (resp.data as { turn?: TurnInfo }).turn : undefined;
    if (turn?.nextSpeakerId && turn.nextSpeakerControlledBy !== 'user') {
      await this.runTurn({ continueMode: true, respondingParticipantId: turn.nextSpeakerId });
    } else {
      await this.refreshTurn();
    }
  }

  protected async onTogglePause(): Promise<void> {
    const chatId = this.chatId();
    const chat = this.chat();
    if (!chatId || !chat) return;
    await this.core.dispatch({ type: 'chatUpdate', chatId, chat: { isPaused: !chat.isPaused } });
    await this.queryClient.invalidateQueries({ queryKey: ['chat', chatId] });
  }

  protected onNudge(): void {
    const info = this.turnInfo();
    if (!info?.nextSpeakerId || info.nextSpeakerControlledBy === 'user') return;
    void this.runTurn({
      continueMode: true,
      respondingParticipantId: info.nextSpeakerId,
      nudge: true,
    });
  }

  // -------------------------------------------------------------------------
  // Send + streaming
  // -------------------------------------------------------------------------

  protected send(content: string): void {
    void this.runTurn({ content });
  }

  protected continueTurn(): void {
    void this.runTurn({ continueMode: true });
  }

  private async runTurn(opts: {
    content?: string;
    continueMode?: boolean;
    respondingParticipantId?: string;
    nudge?: boolean;
  }): Promise<void> {
    const chatId = this.chatId();
    if (!chatId || this.busy()) {
      return;
    }

    if (opts.content) {
      this.optimisticUser.set(this.makeTempUserMessage(opts.content));
      // A user send always chases the bottom and re-enables auto-scroll (v4).
      this.messageList()?.scrollOnUserMessage();
    }

    let state: ChatStreamState = { ...initialChatStreamState(), waitingForResponse: true };
    this.stream.set(state);

    const sub = this.core.events$
      .pipe(filter((frame) => frame.chatId === chatId))
      .subscribe((frame) => {
        state = reduceChatFrame(state, frame);
        this.stream.set(state);
      });

    try {
      await this.core.dispatchExpect(
        {
          type: 'chatSend',
          chatId,
          content: opts.content,
          continueMode: opts.continueMode,
          respondingParticipantId: opts.respondingParticipantId,
          nudge: opts.nudge,
          // Thread the Speaking-As choice onto a user-authored send (v4 does the
          // same); irrelevant to a continue/nudge, so only sent with content.
          speakingAsParticipantId: opts.content ? (this.activeSpeakerId() ?? undefined) : undefined,
        },
        'chatSend',
      );
    } catch (err) {
      state = { ...state, error: err instanceof Error ? err.message : 'Send failed.' };
      this.stream.set(state);
    } finally {
      sub.unsubscribe();
    }

    // Reconcile: refetch the canonical chat (v4 `fetchChat()` on done), then clear
    // the optimistic overlays so the streamed bubbles hand off without duplication.
    await this.queryClient.invalidateQueries({ queryKey: ['chat', chatId] });
    this.stream.set(null);
    this.optimisticUser.set(null);
  }

  protected stop(): void {
    // The server turn rides the shared SSE and can't be aborted from here yet;
    // clear the local streaming overlay (tracked deferral: a real stop dispatch).
    this.stream.set(null);
    this.optimisticUser.set(null);
  }

  private makeTempUserMessage(content: string): MessageDto {
    const participants = this.chat()?.participants ?? [];
    const speakingAsId = this.activeSpeakerId();
    const activeUser =
      (speakingAsId && participants.find((p) => p.id === speakingAsId)) ||
      participants.find((p) => p.type === 'CHARACTER' && p.controlledBy === 'user');
    return {
      id: `temp-user-${Date.now()}`,
      role: 'USER',
      content,
      tokenCount: null,
      promptTokens: null,
      completionTokens: null,
      createdAt: new Date().toISOString(),
      swipeGroupId: null,
      swipeIndex: null,
      participantId: activeUser?.id ?? null,
      attachments: [],
      provider: null,
      modelName: null,
      targetParticipantIds: null,
      isSilentMessage: null,
      systemSender: null,
      systemKind: null,
      hostEvent: null,
      customAnnouncer: null,
      carinaMeta: null,
      pendingExternalPrompt: null,
      pendingExternalPromptFull: null,
      pendingExternalAttachments: null,
      reasoningContent: null,
      reasoningSegments: null,
    };
  }

  // -------------------------------------------------------------------------
  // Message actions (tier 1)
  // -------------------------------------------------------------------------

  protected onCopy(message: MessageDto): void {
    void navigator.clipboard?.writeText(message.content);
  }

  protected onEdit(message: MessageDto): void {
    this.editingId.set(message.id);
  }

  protected async onSaveEdit(event: { id: string; content: string }): Promise<void> {
    this.editingId.set(null);
    await this.core.dispatch({ type: 'messageEdit', messageId: event.id, content: event.content });
    await this.queryClient.invalidateQueries({ queryKey: ['chat', this.chatId()] });
  }

  protected onSwipe(message: MessageDto, direction: -1 | 1): void {
    const gid = message.swipeGroupId;
    if (!gid) return;
    const st = this.effectiveSwipeStates()[gid];
    if (!st) return;
    const next = Math.max(0, Math.min(st.total - 1, st.current + direction));
    this.swipeOverride.update((o) => ({ ...o, [gid]: next }));
  }

  protected async onRegenerate(message: MessageDto): Promise<void> {
    await this.core.dispatch({ type: 'messageSwipe', messageId: message.id });
    await this.queryClient.invalidateQueries({ queryKey: ['chat', this.chatId()] });
  }

  protected async onDelete(message: MessageDto): Promise<void> {
    if (
      typeof window !== 'undefined' &&
      !window.confirm('Are you sure you want to delete this message?')
    ) {
      return;
    }
    const resp = await this.core.dispatch({ type: 'messageDelete', messageId: message.id });
    if (
      resp.type === 'messageDelete' &&
      'requiresConfirmation' in resp.data &&
      resp.data.requiresConfirmation
    ) {
      this.cascade.set({
        messageId: message.id,
        memoryCount: resp.data.memoryCount,
        isSwipeGroup: resp.data.isSwipeGroup,
      });
      return;
    }
    await this.queryClient.invalidateQueries({ queryKey: ['chat', this.chatId()] });
  }

  protected async onCascadeConfirm(action: MemoryCascadeAction): Promise<void> {
    const pending = this.cascade();
    this.cascade.set(null);
    if (!pending) return;
    await this.core.dispatch({
      type: 'messageDelete',
      messageId: pending.messageId,
      memoryAction: action,
      skipConfirmation: true,
    });
    await this.queryClient.invalidateQueries({ queryKey: ['chat', this.chatId()] });
  }

  protected errorMessage(): string {
    const err = this.chatQuery.error();
    return err instanceof Error ? err.message : 'Failed to load the conversation.';
  }
}

/** Resolve a participant's avatar src (explicit URL → default image filepath). */
function participantAvatar(p: ParticipantDetail): string | null {
  return p.character?.avatarUrl ?? p.character?.defaultImage?.filepath ?? null;
}
