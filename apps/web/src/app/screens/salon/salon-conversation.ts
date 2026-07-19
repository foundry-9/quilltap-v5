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
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute, RouterLink } from '@angular/router';
import { filter } from 'rxjs';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { ChatComposer } from '../../chat/chat-composer';
import { customToolsKeys } from '../../chat/custom-tools.api';
import { ConversationHeader } from '../../chat/conversation-header';
import { LLMInspectorPanel } from '../../chat/llm-inspector-panel';
import {
  deriveMessagesWithLogs,
  fetchChatLlmLogs,
  llmLogKeys,
  type LlmLogDto,
} from '../../chat/llm-logs.api';
import { MessageList } from '../../chat/message-list';
import type { ImageClickEvent } from '../../chat/message-row';
import { ImageModal } from '../../images/image-modal';
import { SaveImageDialog } from '../../images/save-image-dialog';
import { PhotoGalleryModal } from '../../images/photo-gallery-modal';
import { GenerateImageDialog, type GeneratedImage } from '../../images/generate-image-dialog';
import { StandaloneGenerateImageDialog } from '../../images/standalone-generate-image-dialog';
import { MemoryCascadeDialog, type MemoryCascadeAction } from '../../chat/memory-cascade-dialog';
import { splitSwipeGroups, type SwipeState } from '../../chat/chat-view-model';
import { isMessageVisibleToOperator } from '../../chat/whisper-visibility';
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
import { SplitLayout } from '../../terminal/split-layout';
import { TerminalPane } from '../../terminal/terminal-pane';
import { TerminalSessionPicker } from '../../terminal/terminal-session-picker';
import { TerminalModeController } from '../../terminal/terminal-mode';
import { DocumentApi } from '../../documents/document-api';
import { DocumentModeController, type DocFocusTarget } from '../../documents/document-mode';
import { DocumentPane } from '../../documents/document-pane';
import { DocumentPicker, type DocumentSelection } from '../../documents/document-picker';
import { EditEnclaveModal } from '../../autonomous/edit-enclave-modal';
import {
  PASSIVE_POLL_INTERVAL_MS,
  StoryBackgroundPoller,
  fetchChatBackgroundVar,
  regenerateChatBackground,
  storyBackgroundKeys,
} from './story-background.api';
import { compileRules, type CompiledRules } from '../../editor/text-replacement';
import { listTextReplacements } from '../settings/chat/text-replacements.api';

/**
 * The LLM document tools whose success invalidates an open pane's cached
 * content / mtime / path, so the pane reloads from the server (v4 SalonView
 * `onToolResult`): open/close reconcile the open set; the write/move/delete
 * family can rewrite bytes, mtime, or the path (the move handlers sync
 * `chat_documents.filePath`, so the reload picks up the new path).
 */
const DOC_RELOAD_TOOLS = new Set<string>([
  'doc_open_document',
  'doc_close_document',
  'doc_write_file',
  'doc_move_file',
  'doc_move_folder',
  'doc_delete_file',
  'doc_delete_folder',
]);

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
  providers: [TerminalModeController, DocumentModeController, DocumentApi],
  imports: [
    RouterLink,
    LoadingState,
    ErrorAlert,
    ConversationHeader,
    MessageList,
    TurnControls,
    ChatComposer,
    MemoryCascadeDialog,
    SplitLayout,
    TerminalPane,
    TerminalSessionPicker,
    DocumentPane,
    DocumentPicker,
    ImageModal,
    SaveImageDialog,
    PhotoGalleryModal,
    GenerateImageDialog,
    StandaloneGenerateImageDialog,
    EditEnclaveModal,
    LLMInspectorPanel,
  ],
  template: `
    <div class="qt-chat-layout" [style.--story-background-url]="backgroundVar()">
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
          <qt-split-layout
            [mode]="combinedMode()"
            [dividerPosition]="documentMode.dividerPosition()"
            [rightPaneVerticalSplit]="terminalMode.rightPaneVerticalSplit()"
            [chatContent]="chatContentTpl"
            [documentContent]="documentPaneActive() ? documentPaneTpl : null"
            [terminalContent]="terminalActive() ? terminalPaneTpl : null"
            (dividerPositionChange)="documentMode.setDividerPosition($event)"
            (rightPaneVerticalSplitChange)="terminalMode.setRightPaneVerticalSplit($event)"
          />
        }
      </div>
    </div>

    <ng-template #chatContentTpl>
      <!-- messageCount is v4's refreshKey={messages.length}. It reads the
           CANONICAL server list, not displayMessages(): the latter carries the
           optimistic user bubble, which would re-key the cost query mid-stream
           and fetch totals for a turn the server has not written yet. -->
      <qt-conversation-header
        [chat]="chat()!"
        [settings]="settings()"
        [messageCount]="chat()!.messages.length"
        [storyBackgroundsEnabled]="storyBackgroundsEnabled()"
        [regeneratingBackground]="regeneratingBackground()"
        [inspectorOpen]="inspectorOpen()"
        [showAllWhispers]="showAllWhispers()"
        (toggleAllWhispers)="showAllWhispers.set(!showAllWhispers())"
        (toggleInspector)="toggleInspector()"
        (openGallery)="showGallery.set(true)"
        (editEnclave)="showEditEnclave.set(true)"
        (regenerateBackground)="onRegenerateBackground()"
      />

      @if (backgroundFlash(); as flash) {
        <div
          class="mx-4 mt-2"
          [class.qt-alert-success]="flash.kind === 'success'"
          [class.qt-alert-error]="flash.kind === 'error'"
          role="status"
        >
          <div class="flex items-start justify-between gap-3">
            <span>{{ flash.message }}</span>
            <button
              type="button"
              class="qt-button-ghost qt-button-sm flex-shrink-0"
              (click)="backgroundFlash.set(null)"
            >
              Dismiss
            </button>
          </div>
        </div>
      }

      <div class="qt-chat-messages-viewport">
        <qt-message-list
          [messages]="displayMessages()"
          [chat]="chat()!"
          [swipeStates]="effectiveSwipeStates()"
          [settings]="settings()"
          [stream]="stream()"
          [editingId]="editingId()"
          [messagesWithLogs]="messagesWithLogs()"
          (viewLlmLogs)="onViewLlmLogs($event)"
          (copy)="onCopy($event)"
          (edit)="onEdit($event)"
          (delete)="onDelete($event)"
          (regenerate)="onRegenerate($event)"
          (swipePrev)="onSwipe($event, -1)"
          (swipeNext)="onSwipe($event, 1)"
          (saveEdit)="onSaveEdit($event)"
          (cancelEdit)="editingId.set(null)"
          (imageClick)="modalImage.set($event)"
          (saveImage)="saveImageTarget.set($event)"
          (courierSettled)="onCourierSettled()"
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
        [chatId]="chatId()!"
        [hasActiveCharacters]="hasActiveCharacters()"
        [terminalActive]="terminalActive()"
        [documentActive]="documentPaneActive()"
        [compositionMode]="compositionMode()"
        [textReplacementRules]="textReplacementRules()"
        [textReplacementsEnabled]="textReplacementsEnabled()"
        [composerSpellcheck]="composerSpellcheck()"
        (compositionModeChange)="onCompositionModeChange($event)"
        (send)="send($event)"
        (stop)="stop()"
        (continue)="continueTurn()"
        (openTerminal)="onOpenTerminal()"
        (openDocument)="showDocumentPicker.set(true)"
        (openGenerate)="showStandaloneGenerate.set(true)"
        (customToolRan)="onCustomToolRan()"
      />
    </ng-template>

    <ng-template #documentPaneTpl>
      @if (documentMode.activeEntry(); as entry) {
        <qt-document-pane
          [entry]="entry"
          [mode]="documentMode.documentMode()"
          (contentChange)="documentMode.handleContentChange(entry.document.id, $event)"
          (blur)="documentMode.flushSave(entry.document.id)"
          (rename)="documentMode.renameDocument(entry.document.id, $event)"
          (close)="documentMode.closeDocument(entry.document.id)"
          (delete)="documentMode.deleteDocument(entry.document.id)"
          (toggleFocus)="documentMode.toggleFocusMode()"
        />
      }
    </ng-template>

    <ng-template #terminalPaneTpl>
      <qt-terminal-pane
        [sessionId]="terminalMode.activeTerminalSessionId()!"
        [mode]="terminalMode.terminalMode()"
        (toggleFocusMode)="terminalMode.toggleFocusMode()"
        (hidePane)="terminalMode.hidePane()"
        (kill)="terminalMode.killTerminal()"
      />
    </ng-template>

    @if (terminalMode.showTerminalPicker()) {
      <qt-terminal-session-picker
        [sessions]="terminalMode.pickerSessions()"
        (attach)="terminalMode.attachExistingSession($event)"
        (spawnNew)="terminalMode.spawnNewSession()"
        (close)="terminalMode.closeTerminalPicker()"
      />
    }

    @if (showDocumentPicker() && chatId(); as id) {
      <qt-document-picker
        [chatId]="id"
        (selectDocument)="onSelectDocument($event)"
        (close)="showDocumentPicker.set(false)"
      />
    }

    @if (cascade(); as c) {
      <qt-memory-cascade-dialog
        [memoryCount]="c.memoryCount"
        [isSwipeGroup]="c.isSwipeGroup"
        (confirm)="onCascadeConfirm($event)"
        (cancel)="cascade.set(null)"
      />
    }

    @if (showStandaloneGenerate() && chatId(); as id) {
      <qt-standalone-generate-image-dialog
        [chatId]="id"
        [participants]="chat()!.participants"
        (generated)="onImagesGenerated($event)"
        (close)="showStandaloneGenerate.set(false)"
      />
    }

    @if (showGenerate() && chatId(); as id) {
      <qt-generate-image-dialog
        [chatId]="id"
        [imageProfileId]="chatImageProfileId()"
        [participants]="chat()!.participants"
        (generated)="onImagesGenerated($event)"
        (close)="showGenerate.set(false)"
      />
    }

    @if (saveImageTarget(); as target) {
      <qt-save-image-dialog
        [chatId]="chatId()!"
        [messageId]="target.messageId"
        [attachments]="saveImageAttachments()"
        [initialAttachmentId]="target.attachmentId"
        (saved)="saveImageTarget.set(null)"
        (close)="saveImageTarget.set(null)"
      />
    }

    @if (showGallery() && chatId(); as id) {
      <qt-photo-gallery-modal
        [chatId]="id"
        [characterId]="firstCharacter()?.id"
        [characterName]="firstCharacter()?.name"
        [userCharacterId]="firstUserCharacter()?.id"
        [userCharacterName]="firstUserCharacter()?.name"
        (imageDeleted)="onCourierSettled()"
        (close)="showGallery.set(false)"
      />
    }

    @if (showEditEnclave() && chat(); as c) {
      <qt-edit-enclave-modal
        [chatId]="c.id"
        [currentTitle]="c.title"
        (saved)="onEnclaveSaved()"
        (close)="showEditEnclave.set(false)"
      />
    }

    <!-- The Inspector mounts UNCONDITIONALLY (v4 :1696-1705 renders it outside
         every gate): the slide-over animates on data-open, so it must exist
         while closed. -->
    @if (chat()) {
      <qt-llm-inspector-panel
        [isOpen]="inspectorOpen()"
        [logs]="llmLogs()"
        [loading]="llmLogsQuery.isLoading()"
        [scrollToMessageId]="inspectorScrollToMessageId()"
        [loggingEnabled]="llmLoggingEnabled()"
        (close)="closeInspector()"
        (refresh)="refreshLogs()"
      />
    }

    @if (modalImage(); as img) {
      <qt-image-modal
        [src]="img.src"
        [filename]="img.filename"
        [fileId]="img.fileId"
        [characterId]="firstCharacter()?.id"
        [characterName]="firstCharacter()?.name"
        [userCharacterId]="firstUserCharacter()?.id"
        [userCharacterName]="firstUserCharacter()?.name"
        (close)="modalImage.set(null)"
        (deleted)="onImageDeleted()"
      />
    }
  `,
})
export class SalonConversation {
  private readonly route = inject(ActivatedRoute, { optional: true });
  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();
  private readonly destroyRef = inject(DestroyRef);
  /** Terminal Mode state for this conversation (v4 `useTerminalMode`). */
  protected readonly terminalMode = inject(TerminalModeController);
  /** Document Mode state for this conversation (v4 `useDocumentMode`). */
  protected readonly documentMode = inject(DocumentModeController);

  /** The Open-Document picker's visibility (local UX). */
  protected readonly showDocumentPicker = signal(false);

  /**
   * Tab-mode identity (v4 `SalonTabPayload.chatId`); when set, wins over the
   * route `:id`. Null ⇒ routed mode, byte-identical.
   */
  readonly chatIdInput = input<string | null>(null, { alias: 'chatId' });
  private readonly routeParams = this.route
    ? toSignal(this.route.paramMap, { requireSync: true })
    : undefined;
  protected readonly chatId = computed(
    () => this.chatIdInput() ?? this.routeParams?.().get('id') ?? null,
  );

  /** The terminal pane is showing when a session is bound and mode isn't normal (v4). */
  protected readonly terminalActive = computed(
    () =>
      !!this.terminalMode.activeTerminalSessionId() &&
      this.terminalMode.terminalMode() !== 'normal',
  );

  /** The document pane is showing when a document is open (v4 `documentActive`). */
  protected readonly documentPaneActive = computed(
    () => this.documentMode.activeDocument() !== null,
  );

  /**
   * Combine the two panes' modes for the split layout (v4 `combinedMode`): focus
   * on either side wins, else split if either is split, else normal.
   */
  protected readonly combinedMode = computed<'normal' | 'split' | 'focus'>(() => {
    const d = this.documentMode.documentMode();
    const t = this.terminalMode.terminalMode();
    if (d === 'focus' || t === 'focus') return 'focus';
    if (d === 'split' || t === 'split') return 'split';
    return 'normal';
  });

  constructor() {
    // Bind the id-dependent wiring once the chat id is known. In routed mode the
    // route param is synchronous, so this runs on the first change detection with
    // the id already resolved (ordering preserved); in workspace-tab mode the
    // `chatId` input arrives after construction, so the one-shot effect defers the
    // wiring until it does. The `wiredChatId` guard makes it fire exactly once.
    let wiredChatId: string | null = null;
    effect(() => {
      const chatId = this.chatId();
      if (!chatId || wiredChatId === chatId) return;
      wiredChatId = chatId;
      // Bind the terminal controller (refetch after spawn so the Ariel
      // session-opened announcement appears).
      this.terminalMode.configure(chatId, () => {
        void this.chatQuery.refetch();
      });
      // The Librarian open/save/rename/delete announcements are persisted
      // server-side; refetch so the collapsed Librarian chip appears (the
      // announcement-asymmetry lesson — no bespoke append path).
      this.documentMode.configure(chatId, () => {
        void this.queryClient.invalidateQueries({ queryKey: ['chat', chatId] });
      });

      // React to the LLM's document tools on the live stream (v4 SalonView
      // `onToolResult`): open/close/write/move/delete → reload from server;
      // doc_focus → route to the pane that owns the target document.
      const sub = this.core.events$
        .pipe(filter((frame) => frame.chatId === chatId))
        .subscribe((frame) => {
          const result = frame.toolResult;
          if (!result) return;
          if (result.success && DOC_RELOAD_TOOLS.has(result.name)) {
            void this.documentMode.reloadFromServer();
          }
          if (result.name === 'doc_focus' && result.success && result.result) {
            this.documentMode.handleDocFocus(result.result as DocFocusTarget);
          }
        });
      this.destroyRef.onDestroy(() => sub.unsubscribe());
    });

    effect(() => this.terminalMode.hydrate(this.chat()));
    effect(() => this.documentMode.hydrate(this.chat()));

    // A terminal announcement (open/close/periodic summary) landed → refetch the
    // chat so the new Ariel message appears (v4's salon-page listeners).
    const onTerminalChatUpdate = (event: Event) => {
      const detail = (event as CustomEvent<{ chatId?: string }>).detail;
      if (detail?.chatId && detail.chatId === this.chatId()) {
        void this.queryClient.invalidateQueries({ queryKey: ['chat', this.chatId()] });
      }
    };
    // Cmd/Ctrl+Shift+T toggles the terminal pane; Escape exits focus back to split (v4).
    const onKeydown = (event: KeyboardEvent) => {
      if (
        (event.metaKey || event.ctrlKey) &&
        event.shiftKey &&
        (event.key === 'T' || event.key === 't')
      ) {
        event.preventDefault();
        if (this.terminalMode.terminalMode() === 'normal') void this.terminalMode.requestOpen();
        else void this.terminalMode.hidePane();
        return;
      }
      if (event.key === 'Escape' && this.terminalMode.terminalMode() === 'focus') {
        event.preventDefault();
        this.terminalMode.toggleFocusMode();
      }
    };
    if (typeof window !== 'undefined') {
      window.addEventListener('quilltap:chat-update', onTerminalChatUpdate);
      window.addEventListener('quilltap:terminal-exited', onTerminalChatUpdate);
      window.addEventListener('keydown', onKeydown);
      this.destroyRef.onDestroy(() => {
        window.removeEventListener('quilltap:chat-update', onTerminalChatUpdate);
        window.removeEventListener('quilltap:terminal-exited', onTerminalChatUpdate);
        window.removeEventListener('keydown', onKeydown);
      });
    }

    /**
     * Cmd/Ctrl+Shift+L toggles the Inspector (v4 `SalonView.tsx:796-811`).
     *
     * Two v4 details are load-bearing. The listener is attached ONLY while
     * logging is enabled (v4's effect returns early otherwise), so the shortcut
     * is dead exactly where the toolbar button is hidden — which is why this is a
     * gated effect rather than a branch inside the terminal listener above. And
     * the test is `e.key === 'L'` UPPERCASE: Shift is held, so the browser
     * reports the capital.
     */
    const onInspectorKeydown = (event: KeyboardEvent): void => {
      if ((event.metaKey || event.ctrlKey) && event.shiftKey && event.key === 'L') {
        event.preventDefault();
        this.toggleInspector();
      }
    };
    effect((onCleanup) => {
      if (!this.llmLoggingEnabled()) return;
      document.addEventListener('keydown', onInspectorKeydown);
      onCleanup(() => document.removeEventListener('keydown', onInspectorKeydown));
    });
  }

  /** Terminal-open entry: the controller re-attaches, shows the picker, or spawns. */
  protected onOpenTerminal(): void {
    void this.terminalMode.requestOpen();
  }

  /** A picker selection opens (or creates) the document, then closes the picker. */
  protected onSelectDocument(params: DocumentSelection): void {
    this.showDocumentPicker.set(false);
    void this.documentMode.openDocument(params);
  }

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

  /**
   * The chat's story background (dogfood finding #9): applied as
   * `--story-background-url` on the layout root, where the ported
   * `.qt-chat-layout::before` layer (`_chat.css`) draws it at 0.45 opacity,
   * fixed/cover. Null when the chat has no background → the `:not([style*=…])`
   * rule hides the layer.
   *
   * DISPLAY IS UNCONDITIONAL; the settings flag gates only the PASSIVE POLL (v4
   * `useStoryBackground.ts:68` — `enablePassivePolling` is a separate argument
   * from the query's `enabled`). A chat keeps showing the backdrop it has even
   * with generation switched off; the 30s poll only exists to notice a backdrop
   * a background JOB wrote.
   */
  private readonly backgroundQuery = injectQuery(() => ({
    queryKey: storyBackgroundKeys.background(this.chatId() ?? ''),
    enabled: !!this.chatId(),
    queryFn: () => fetchChatBackgroundVar(this.core, this.chatId()!),
    refetchInterval: this.storyBackgroundsEnabled() ? PASSIVE_POLL_INTERVAL_MS : false,
    // v4 `refetchOnReconnect: false`.
    refetchOnReconnect: false,
  }));
  protected readonly backgroundVar = computed<string | null>(
    () => this.backgroundQuery.data() ?? null,
  );

  /** v4 `chatSettings?.storyBackgroundsSettings?.enabled ?? false` (SalonView.tsx:107). */
  protected readonly storyBackgroundsEnabled = computed<boolean>(
    () =>
      (this.settings()?.['storyBackgroundsSettings'] as { enabled?: boolean } | undefined)
        ?.enabled ?? false,
  );

  // --- story-background regeneration (v4 useChatControls.ts:397-416) ---

  private readonly poller = new StoryBackgroundPoller();
  /** v4 clears the interval on unmount (`:144-150`) — a live 3-minute timer must not outlive the view. */
  private readonly _pollerTeardown = this.destroyRef.onDestroy(() => this.poller.stop());
  /** v4's toasts have no v5 bus yet — the scriptorium `flash` idiom stands in. */
  protected readonly backgroundFlash = signal<{ kind: 'success' | 'error'; message: string } | null>(
    null,
  );
  protected readonly regeneratingBackground = signal(false);

  /**
   * v4 fires `onBackgroundChanged` when a poll sees the backdrop move, and the
   * Salon's callback is `() => { void fetchChat() }`: a Lantern announcement is
   * posted ALONGSIDE the new backdrop, so the chat must be refetched or the
   * announcement only appears if the user leaves and returns.
   */
  private onBackgroundChanged(): void {
    const chatId = this.chatId();
    if (!chatId) return;
    void this.queryClient.invalidateQueries({ queryKey: ['chat', chatId] });
  }

  /**
   * The passive-poll change hook (v4 `:131-141`): the 30s revalidation can land
   * a new URL on its own, so a transition fires the same callback the active
   * poll does. v4 skips the INITIAL load — only transitions from a known value
   * count, or every chat open would refetch itself.
   */
  private previousBackgroundVar: string | null | undefined = undefined;
  private readonly _backgroundChangeEffect = effect(() => {
    const next = this.backgroundVar();
    const previous = this.previousBackgroundVar;
    this.previousBackgroundVar = next;
    if (previous === undefined) return;
    if (previous !== next) this.onBackgroundChanged();
  });

  protected async onRegenerateBackground(): Promise<void> {
    const chatId = this.chatId();
    if (!chatId) return;
    this.regeneratingBackground.set(true);
    this.backgroundFlash.set(null);
    try {
      const result = await regenerateChatBackground(this.core, chatId);
      // Both §2 success arms are shown verbatim: "…queued" and "…already in
      // progress" are distinct states the user should be able to tell apart.
      this.backgroundFlash.set({ kind: 'success', message: result.message });
      this.poller.start(
        this.backgroundVar(),
        async () => (await this.backgroundQuery.refetch()).data ?? null,
        () => this.onBackgroundChanged(),
      );
    } catch (error) {
      // v4 surfaces the server's own message (`errorData.error`) — that is how
      // the §2 badRequest strings ("Story backgrounds are not enabled. …") reach
      // the user — falling back to its generic copy.
      const message = error instanceof Error ? error.message : String(error);
      this.backgroundFlash.set({
        kind: 'error',
        message: message || 'Failed to regenerate background',
      });
    } finally {
      this.regeneratingBackground.set(false);
    }
  }

  /**
   * Composition mode (dogfood finding #8): the per-chat flag rides the chat's
   * `documentEditingMode` column (v4 `useChatControls.ts:348-365` — local state
   * seeded from the chat, toggle persisted via the chat PUT). The optimistic
   * override is keyed to the chat id so switching chats falls back to the
   * canonical value.
   */
  private readonly compositionOverride = signal<{ chatId: string; value: boolean } | null>(null);
  protected readonly compositionMode = computed<boolean>(() => {
    const override = this.compositionOverride();
    if (override && override.chatId === this.chatId()) return override.value;
    return this.chat()?.documentEditingMode ?? false;
  });

  protected async onCompositionModeChange(value: boolean): Promise<void> {
    const chatId = this.chatId();
    if (!chatId) return;
    this.compositionOverride.set({ chatId, value });
    await this.core.dispatch({ type: 'chatUpdate', chatId, chat: { documentEditingMode: value } });
    await this.queryClient.invalidateQueries({ queryKey: ['chat', chatId] });
  }

  /**
   * The composer text-replacement rules (v4 `useTextReplacementRules` — the
   * global rule list compiled once, gated by
   * `chat_settings.textReplacementsEnabled`). Composer-only; form fields never
   * receive rules.
   */
  private readonly textReplacementsQuery = injectQuery(() => ({
    queryKey: ['textReplacements'],
    queryFn: async () => (await listTextReplacements(this.core)).rules,
  }));
  protected readonly textReplacementRules = computed<CompiledRules | null>(() => {
    const rules = this.textReplacementsQuery.data();
    return rules && rules.length > 0 ? compileRules(rules) : null;
  });
  protected readonly textReplacementsEnabled = computed<boolean>(
    () => this.settings()?.textReplacementsEnabled ?? true,
  );

  /** v4 `chatSettings?.composerSpellcheck ?? true` (`LexicalComposerWrapper.tsx:107`). */
  protected readonly composerSpellcheck = computed<boolean>(
    () => (this.settings()?.['composerSpellcheck'] as boolean | undefined) ?? true,
  );

  // --- the LLM Inspector (v4 `useLLMLogs` + SalonView's toolbar/shortcut/panel) ---

  /**
   * v4 `chatSettings?.llmLoggingSettings?.enabled !== false` (`SalonView.tsx:797`).
   * DEFAULTS TRUE — only an explicit `false` closes the gate. Drives the toolbar
   * button, the keyboard shortcut, and the panel's disabled empty state.
   */
  protected readonly llmLoggingEnabled = computed<boolean>(
    () =>
      (this.settings()?.['llmLoggingSettings'] as { enabled?: boolean } | undefined)?.enabled !==
      false,
  );

  /**
   * Every log for this chat (v4 `useLLMLogs.ts:25-30`).
   *
   * `enabled: messages.length > 0` is v4's (`:29`): a chat with no messages has
   * no logs, so the fetch is skipped rather than round-tripped for an empty list.
   * It reads the CANONICAL list — the optimistic user bubble must not trigger a
   * fetch for a turn the server has not written yet (the same reasoning the cost
   * summary's `messageCount` follows).
   */
  protected readonly llmLogsQuery = injectQuery(() => ({
    queryKey: llmLogKeys.byChat(this.chatId() ?? ''),
    enabled: !!this.chatId() && (this.chat()?.messages.length ?? 0) > 0,
    queryFn: () => fetchChatLlmLogs(this.core, this.chatId()!),
  }));
  protected readonly llmLogs = computed<LlmLogDto[]>(() => this.llmLogsQuery.data() ?? []);

  /** v4 `useLLMLogs.ts:35-41` — which messages get the per-row cpu icon. */
  protected readonly messagesWithLogs = computed(() => deriveMessagesWithLogs(this.llmLogs()));

  protected readonly inspectorOpen = signal(false);
  protected readonly inspectorScrollToMessageId = signal<string | null>(null);

  /** v4 `handleViewLLMLogs` (`:44-47`) — open scrolled to this message's logs. */
  protected onViewLlmLogs(messageId: string): void {
    this.inspectorScrollToMessageId.set(messageId);
    this.inspectorOpen.set(true);
  }

  /**
   * v4 `toggleInspector` (`:50-58`) — the toolbar button and the shortcut.
   *
   * The scroll target is cleared ONLY when OPENING. Clearing it on close as well
   * would look tidier and be wrong: the panel is still mounted and animating out,
   * and dropping the target mid-transition would strip the highlight from the
   * entry the user is watching leave.
   */
  protected toggleInspector(): void {
    const opening = !this.inspectorOpen();
    if (opening) {
      this.inspectorScrollToMessageId.set(null);
    }
    this.inspectorOpen.set(opening);
  }

  /** v4 `closeInspector` (`:61-64`) — clears both. */
  protected closeInspector(): void {
    this.inspectorOpen.set(false);
    this.inspectorScrollToMessageId.set(null);
  }

  /** v4 `refreshLogs` (`:67-69`) — the panel's refresh button and the post-turn hook. */
  protected refreshLogs(): void {
    void this.llmLogsQuery.refetch();
  }

  // --- streaming ---
  protected readonly stream = signal<ChatStreamState | null>(null);
  protected readonly busy = computed(() => this.stream() != null);
  private readonly optimisticUser = signal<MessageDto | null>(null);

  // --- client-side swipe switching (v4 `switchSwipe`) ---
  private readonly swipeOverride = signal<Record<string, number>>({});

  // --- inline edit + delete-cascade ---
  protected readonly editingId = signal<string | null>(null);
  protected readonly cascade = signal<CascadePrompt | null>(null);

  // --- the in-chat image lightbox (v4 SalonView `modalImage`) ---
  protected readonly modalImage = signal<ImageClickEvent | null>(null);

  // --- the save-to-album dialog (v4 SalonView `saveImageTarget`) ---
  protected readonly saveImageTarget = signal<{ messageId: string; attachmentId: string } | null>(
    null,
  );

  // --- the in-chat photo gallery (v4 SalonView sidebar gallery entry) ---
  protected readonly showGallery = signal(false);

  // --- the Edit-Enclave modal (v4 SalonView, autonomous rooms only) ---
  protected readonly showEditEnclave = signal(false);

  /**
   * The STANDALONE generate-image dialog (v4 `StandaloneGenerateImageDialog`).
   * The composer's camera gutter button opens this one — in v4 that button is
   * the single opener in the whole app (`ComposerGutterTools:16,:52` ←
   * `ChatComposer:75,:129,:349` ← `SalonView:1530` ←
   * `useModalState:39,:79`).
   */
  protected readonly showStandaloneGenerate = signal(false);

  /**
   * The chat-profile-fixed generate-image dialog (v4 `GenerateImageDialog`).
   *
   * **Nothing sets this true, and that is faithful.** v4 mounts this dialog in
   * `ChatModals.tsx:209` and exports `openGenerateImage` from `useModalState`
   * (`:63`), but no v4 component ever calls it — the dialog is unreachable in
   * v4 too. v5 previously pointed the composer's camera button here; P4.9b
   * re-pointed that button at the standalone dialog to match v4's real opener
   * chain, which loses nothing, since the standalone dialog is a strict
   * superset (explicit profile picker rather than the chat's fixed profile,
   * participant quick-inserts, and 1–4 images rather than a hardcoded 1).
   *
   * Kept mounted rather than deleted, per the standing rule to port v4's
   * vestigial code faithfully and sweep it deliberately after the port.
   */
  protected readonly showGenerate = signal(false);

  /** The chat's image profile — the generate target (first participant with one). */
  protected readonly chatImageProfileId = computed<string | null>(
    () =>
      (this.chat()?.participants ?? []).find((p) => p.imageProfile)?.imageProfile?.id ?? null,
  );

  /** The target message's attachments, for the SaveImageDialog picker. */
  protected readonly saveImageAttachments = computed(() => {
    const target = this.saveImageTarget();
    if (!target) return [];
    return this.chat()?.messages.find((m) => m.id === target.messageId)?.attachments ?? [];
  });

  /** The first non-user character — the save-to-gallery target (v4 `getFirstCharacter`). */
  protected readonly firstCharacter = computed<{ id: string; name: string } | null>(() => {
    const p = (this.chat()?.participants ?? []).find(
      (x) => x.type === 'CHARACTER' && x.controlledBy === 'llm' && x.character,
    );
    return p?.character ? { id: p.character.id, name: p.character.name } : null;
  });

  /** The first user-controlled character (v4 `getFirstUserCharacter`). */
  protected readonly firstUserCharacter = computed<{ id: string; name: string } | null>(() => {
    const p = (this.chat()?.participants ?? []).find(
      (x) => x.type === 'CHARACTER' && x.controlledBy === 'user' && x.character,
    );
    return p?.character ? { id: p.character.id, name: p.character.name } : null;
  });

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

  /** The "All Whispers" toggle (v4 SalonView `showAllWhispers`, default off). */
  protected readonly showAllWhispers = signal(false);

  /**
   * The participant ids the human controls (v4 SalonView `userParticipantIdSet`
   * — `controlledBy: 'user'`), so the whisper filter shows the operator their
   * own whispers whatever the toggle says.
   */
  private readonly userParticipantIdSet = computed<ReadonlySet<string>>(
    () =>
      new Set(
        (this.chat()?.participants ?? [])
          .filter((p) => p.controlledBy === 'user')
          .map((p) => p.id),
      ),
  );

  /**
   * The rendered flow: the collapsed messages (with swipe override), whisper-
   * filtered for the operator (v4 SalonView `visibleMessages`), + the optimistic
   * user bubble (always the human's own, so it never filters out).
   */
  protected readonly displayMessages = computed<MessageDto[]>(() => {
    const states = this.effectiveSwipeStates();
    const showAll = this.showAllWhispers();
    const userIds = this.userParticipantIdSet();
    const msgs = this.split()
      .messages.map((m) => {
        if (m.swipeGroupId && states[m.swipeGroupId]) {
          const st = states[m.swipeGroupId];
          return st.messages[st.current] ?? m;
        }
        return m;
      })
      .filter((m) =>
        isMessageVisibleToOperator(m, { showAllWhispers: showAll, userParticipantIds: userIds }),
      );
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

  protected send(payload: { content: string; fileIds: string[] }): void {
    void this.runTurn({ content: payload.content, fileIds: payload.fileIds });
  }

  protected continueTurn(): void {
    void this.runTurn({ continueMode: true });
  }

  private async runTurn(opts: {
    content?: string;
    fileIds?: string[];
    continueMode?: boolean;
    respondingParticipantId?: string;
    nudge?: boolean;
  }): Promise<void> {
    const chatId = this.chatId();
    if (!chatId || this.busy()) {
      return;
    }

    const hasAttachments = (opts.fileIds?.length ?? 0) > 0;
    if (opts.content || hasAttachments) {
      this.optimisticUser.set(this.makeTempUserMessage(opts.content ?? ''));
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
          fileIds: opts.fileIds?.length ? opts.fileIds : undefined,
          continueMode: opts.continueMode,
          respondingParticipantId: opts.respondingParticipantId,
          nudge: opts.nudge,
          // Thread the Speaking-As choice onto a user-authored send (v4 does the
          // same); irrelevant to a continue/nudge, so only sent with content or
          // an attachment-only message.
          speakingAsParticipantId:
            opts.content || hasAttachments ? (this.activeSpeakerId() ?? undefined) : undefined,
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

    // Refresh the LLM logs now the turn is done (v4 `SalonView.tsx:769-781` —
    // the effect that fires when generation stops calls `llmLogs.refreshLogs()`).
    // The turn is exactly what WROTE the new log rows, so without this the
    // Inspector — and every row's cpu icon — stays a turn behind until something
    // else refetches.
    this.refreshLogs();

    // On turn end, re-read every open document (the LLM may have edited one
    // without a surfaced doc_* tool result); dirty panes are skipped (v4
    // `handleLLMEditEnd`).
    if (this.documentMode.documentActive()) {
      await this.documentMode.handleLLMEditEnd();
    }
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

  /** A courier turn settled (resolved/cancelled) → refetch (v4 `onCourierTurnSettled`). */
  protected async onCourierSettled(): Promise<void> {
    await this.queryClient.invalidateQueries({ queryKey: ['chat', this.chatId()] });
  }

  /**
   * A manual custom-tool run landed → refetch the chat so Pascal's outcome
   * appears, and invalidate the roster so the popup re-resolves (v4 `onRan`
   * invalidates both `chats.detail` and `customTools.byChat`).
   */
  protected async onCustomToolRan(): Promise<void> {
    const chatId = this.chatId();
    if (!chatId) return;
    await Promise.all([
      this.queryClient.invalidateQueries({ queryKey: ['chat', chatId] }),
      this.queryClient.invalidateQueries({ queryKey: customToolsKeys.byChat(chatId) }),
    ]);
  }

  /** The Edit-Enclave modal saved → refetch the chat (v4 SalonView `onSaved`). */
  protected async onEnclaveSaved(): Promise<void> {
    this.showEditEnclave.set(false);
    await this.queryClient.invalidateQueries({ queryKey: ['chat', this.chatId()] });
  }

  /** The lightbox deleted an image → refetch so the thumbnail disappears (v4 `onDelete`). */
  protected async onImageDeleted(): Promise<void> {
    this.modalImage.set(null);
    await this.queryClient.invalidateQueries({ queryKey: ['chat', this.chatId()] });
  }

  /** Generated images → record the tool result and refetch (v4 `onImagesGenerated`). */
  protected async onImagesGenerated(event: {
    images: GeneratedImage[];
    prompt: string;
  }): Promise<void> {
    this.showGenerate.set(false);
    const chatId = this.chatId();
    if (!chatId) return;
    await this.core.dispatch({
      type: 'chatAddToolResult',
      chatId,
      tool: 'generate_image',
      initiatedBy: 'user',
      prompt: event.prompt,
      images: event.images.map((img) => ({ id: img.id, filename: img.filename })),
    });
    await this.queryClient.invalidateQueries({ queryKey: ['chat', chatId] });
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
