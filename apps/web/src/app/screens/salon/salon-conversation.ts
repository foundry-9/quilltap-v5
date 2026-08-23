import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  computed,
  effect,
  inject,
  input,
  signal,
  untracked,
  viewChild,
} from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute, RouterLink } from '@angular/router';
import { filter } from 'rxjs';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import {
  ChatComposer,
  type PendingToolResultChip,
  type ToolExecutionStatus,
  type SpeakingAsSeat,
} from '../../chat/chat-composer';
import {
  LibraryFilePickerModal,
  type LinkedLibraryFile,
} from '../../chat/library-picker/library-file-picker-modal';
import type { RngPendingResult } from '../../chat/rng-dropdown';
import { customToolsKeys } from '../../chat/custom-tools.api';
import { ConversationHeader } from '../../chat/conversation-header';
import { resolveToolResultErrorText } from '../../chat/tool-result-error';
import { LLMInspectorPanel } from '../../chat/llm-inspector-panel';
import {
  deriveMessagesWithLogs,
  fetchChatLlmLogs,
  llmLogKeys,
  type LlmLogDto,
} from '../../chat/llm-logs.api';
import { MessageList } from '../../chat/message-list';
import { ChatSidebar } from '../../chat/sidebar/chat-sidebar';
import type { ChatSectionState } from '../../chat/sidebar/chat-section';
import type { VisibilityState } from '../../chat/sidebar/visibility-section';
import type { ImageClickEvent } from '../../chat/message-row';
import { notifyQueueChange } from '../../layout/queue-status.logic';
import { ImageModal } from '../../images/image-modal';
import { SaveImageDialog } from '../../images/save-image-dialog';
import { PhotoGalleryModal } from '../../images/photo-gallery-modal';
import { GenerateImageDialog, type GeneratedImage } from '../../images/generate-image-dialog';
import { StandaloneGenerateImageDialog } from '../../images/standalone-generate-image-dialog';
import { MemoryCascadeDialog, type MemoryCascadeAction } from '../../chat/memory-cascade-dialog';
import { ComposeMailDialog, type ComposeMailParticipant } from '../../chat/post-office/compose-mail-dialog';
import { InsertAnnouncementDialog } from '../../chat/post-office/insert-announcement-dialog';
import type { AudienceCandidate } from '../../chat/post-office/post-office.api';
import { WhisperDialog } from '../../chat/post-office/whisper-dialog';
import { AddCharacterDialog } from '../../chat/cast/add-character-dialog';
import { BulkCharacterReplaceModal } from '../../chat/bulk-character-replace-modal';
import { ChatProjectModal } from '../../chat/chat-project-modal';
import { MergeConversationModal } from '../../chat/merge-conversation-modal';
import { ChatToolSettingsModal } from '../../chat/tools/chat-tool-settings-modal';
import { AllLLMPauseModal, type AllLLMPauseParticipant } from '../../chat/all-llm-pause-modal';
import { getNextPauseThreshold } from '../../chat/all-llm-pause';
import { RunToolModal } from '../../chat/tools/run-tool-modal';
import { SearchReplaceModal } from '../../chat/tools/search-replace-modal';
import { ChatRenameModal } from '../../chat/chat-rename-modal';
import { ReattributeMessageDialog } from '../../chat/reattribute-message-dialog';
import { SelectLlmProfileDialog } from '../../chat/select-llm-profile-dialog';
import {
  removeParticipant,
  rebuildSystemPrompt,
  updateParticipant,
  type UpdateParticipantPatch,
} from '../../chat/chat-cast.api';
import type { ConnectionProfileOption } from '../../chat/sidebar/participant-card';
import { Modal } from '../../ui/modal';
import { splitSwipeGroups, type SwipeState } from '../../chat/chat-view-model';
import { isMessageVisibleToOperator } from '../../chat/whisper-visibility';
import { TurnControls } from '../../chat/turn-controls';
import { type ControlledCharacter } from '../../chat/speaker-selector';
import { isParticipantPresent } from '../../chat/skip-signal-helpers';
import {
  computeSkipEligibility,
  qualifiesForTurnSkipping,
  type SkipEvent,
  type SkipParticipant,
} from '../../chat/skip-signal';
import {
  narrowRenderingPatterns,
  type DialogueDetection,
  type RenderingPattern,
} from '../../chat/render/roleplay-rendering';
import { fetchRoleplayTemplate } from '../settings/templates/templates.api';
import type { NarrationDelimiters, TemplateDelimiter } from '../../core/core-contract';
import {
  addToQueue,
  createInitialTurnState,
  findActiveUserParticipant,
  isUserDrivenSeat,
  nudgeParticipant,
  removeFromQueue,
  type TurnSelectionResult,
  type TurnState,
} from '../../chat/turn-order';
import {
  initialChatStreamState,
  reduceChatFrame,
  type ChatStreamState,
} from '../../core/chat-stream.reducer';
import { CoreClient } from '../../core/core-client';
import type {
  ChatDetail,
  ChatSettingsDto,
  ConnectionProfileDto,
  MessageDto,
  ParticipantDetail,
  ParticipantStatusWire,
} from '../../core/core-contract';
import { ErrorAlert } from '../../ui/error-alert';
import { LoadingState } from '../../ui/loading-state';
import { ToastService } from '../../ui/toast.service';
import { SalonModePanes } from './salon-mode-panes';
import { TerminalPane } from '../../terminal/terminal-pane';
import { TerminalSessionPicker } from '../../terminal/terminal-session-picker';
import { TerminalModeController } from '../../terminal/terminal-mode';
import { DocumentApi } from '../../documents/document-api';
import { DocumentModeController, type DocFocusTarget } from '../../documents/document-mode';
import { DocumentPane } from '../../documents/document-pane';
import { DocumentPicker, type DocumentSelection } from '../../documents/document-picker';
import { EditEnclaveModal } from '../../autonomous/edit-enclave-modal';
import { StateEditorModal } from '../../shared/state/state-editor-modal';
import {
  PASSIVE_POLL_INTERVAL_MS,
  StoryBackgroundPoller,
  fetchChatBackgroundVar,
  regenerateChatBackground,
  storyBackgroundKeys,
} from './story-background.api';
import { compileRules, type CompiledRules } from '../../editor/text-replacement';
import { listTextReplacements } from '../settings/chat/text-replacements.api';
import {
  WORKSPACE_BACKDROP_REGISTRY,
  WORKSPACE_TAB_ID,
} from '../../workspace/workspace-contract';

/**
 * v4 `setImpersonatingParticipantIds(data.impersonatingParticipantIds || [])`
 * (`useImpersonation.ts:62,105`) — read the list off an impersonate reply, with
 * a caller-supplied fallback for a body that does not carry it.
 */
export function readImpersonatingIds(
  data: Record<string, unknown>,
  fallback: string[],
): string[] {
  const ids = data['impersonatingParticipantIds'];
  return Array.isArray(ids) ? ids.filter((id): id is string => typeof id === 'string') : fallback;
}

/**
 * v4 `data.activeTypingParticipantId` off an impersonate / stop-impersonate reply
 * (`useImpersonation.ts:63,107,134`). Like {@link readImpersonatingIds}, the reply
 * is the AUTHORITATIVE source: the chat GET projects neither key, so the client
 * must hold both locally. Returns the id, or `null` when the body omits it (the
 * caller supplies v4's `|| participantId` / `|| null` fallback).
 */
export function readActiveTyping(data: Record<string, unknown>): string | null {
  const id = data['activeTypingParticipantId'];
  return typeof id === 'string' ? id : null;
}

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

/**
 * v4 debounces the per-chat talkativeness write by 400 ms PER PARTICIPANT
 * (`useChatControls:613-635`), so dragging the slider fires one request on
 * release rather than one per step.
 */
const TALKATIVENESS_DEBOUNCE_MS = 400;

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
    SalonModePanes,
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
    StateEditorModal,
    LLMInspectorPanel,
    ChatSidebar,
    InsertAnnouncementDialog,
    ComposeMailDialog,
    WhisperDialog,
    AddCharacterDialog,
    ChatRenameModal,
    BulkCharacterReplaceModal,
    ChatProjectModal,
    MergeConversationModal,
    ChatToolSettingsModal,
    AllLLMPauseModal,
    RunToolModal,
    SearchReplaceModal,
    ReattributeMessageDialog,
    SelectLlmProfileDialog,
    LibraryFilePickerModal,
    Modal,
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
          <qt-salon-mode-panes
            [parentChatId]="chatId()!"
            [chatTitle]="chat()?.title ?? null"
            [mode]="combinedMode()"
            [dividerPosition]="documentMode.dividerPosition()"
            [rightPaneVerticalSplit]="terminalMode.rightPaneVerticalSplit()"
            [chatContent]="chatContentTpl"
            [documentPaneTemplate]="documentPaneTpl"
            [documentEntries]="documentMode.openDocs()"
            [focusedDocId]="documentMode.focusedDocId()"
            [terminalContent]="terminalActive() ? terminalPaneTpl : null"
            [terminalActive]="terminalActive()"
            (closeDocument)="documentMode.closeDocument($event)"
            (closeTerminal)="terminalMode.hidePane()"
            (dividerPositionChange)="documentMode.setDividerPosition($event)"
            (rightPaneVerticalSplitChange)="terminalMode.setRightPaneVerticalSplit($event)"
          />
        }
      </div>

      <!-- The chat sidebar is a sibling of .qt-chat-main inside the flex layout
           (v4 SalonView.tsx:1695). It carries its own classes on the host, so
           the DOM the ported CSS sees matches v4's. -->
      @if (chat(); as c) {
        <qt-chat-sidebar
          [participants]="c.participants"
          [turnState]="turnState()"
          [turnSelectionResult]="effectiveTurnSelection()"
          [isGenerating]="busy()"
          [isPaused]="c.isPaused"
          [userParticipantId]="userParticipantId()"
          [respondingParticipantId]="stream()?.respondingParticipantId ?? null"
          [impersonatingParticipantIds]="impersonatingIds()"
          [activeTypingParticipantId]="activeSpeakerId()"
          [isDangerousChat]="c.isDangerousChat === true"
          [chatId]="c.id"
          [chatSectionState]="chatSectionState()"
          [storyBackgroundsEnabled]="storyBackgroundsEnabled()"
          [regeneratingBackground]="regeneratingBackground()"
          [visibilityState]="visibilityState()"
          [turnSkippingApplies]="turnSkippingApplies()"
          [showAllWhispers]="showAllWhispers()"
          [isAutonomousRoom]="c.chatType === 'autonomous'"
          (togglePause)="onTogglePause()"
          (nudge)="onSidebarNudge($event)"
          (queue)="onSidebarQueue($event)"
          (dequeue)="onSidebarDequeue($event)"
          (skip)="onSidebarSkip()"
          (stopStreaming)="stop()"
          (impersonate)="onImpersonate($event)"
          (stopImpersonate)="onStopImpersonate($event)"
          (regenerateAvatar)="onRegenerateAvatar($event)"
          (chatUpdated)="onChatUpdated()"
          (regenerateBackground)="onRegenerateBackground()"
          (toggleAllWhispers)="showAllWhispers.set(!showAllWhispers())"
          (editEnclave)="showEditEnclave.set(true)"
          (rename)="showRename.set(true)"
          (mergeIn)="showMerge.set(true)"
          (bulkReplace)="showBulkReplace.set(true)"
          (searchReplace)="showSearchReplace.set(true)"
          (openProject)="showProject.set(true)"
          (openToolSettings)="showToolSettings.set(true)"
          (openRunTool)="showRunTool.set(true)"
          (openState)="showStateEditor.set(true)"
          (openGallery)="showGallery.set(true)"
          (whisper)="onWhisper($event)"
          (addCharacter)="showAddCharacter.set(true)"
          [connectionProfiles]="connectionProfiles()"
          (connectionProfileChange)="onParticipantProfileChange($event)"
          (systemPromptChange)="onParticipantSystemPromptChange($event)"
          (rebuildSystemPrompt)="onParticipantRebuildSystemPrompt($event)"
          (talkativenessChange)="onParticipantTalkativenessChange($event)"
          (statusChange)="onParticipantStatusChange($event)"
          (removeParticipant)="onParticipantRemoveRequested($event)"
        />
      }
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
        [inspectorOpen]="inspectorOpen()"
        (toggleInspector)="toggleInspector()"
      />

      <div class="qt-chat-messages-viewport">
        <qt-message-list
          [messages]="displayMessages()"
          [chat]="chat()!"
          [renderingPatterns]="renderingPatterns()"
          [dialogueDetection]="dialogueDetection()"
          [swipeStates]="effectiveSwipeStates()"
          [settings]="settings()"
          [stream]="stream()"
          [editingId]="editingId()"
          [messagesWithLogs]="messagesWithLogs()"
          [userParticipantIds]="userParticipantIdSet()"
          (viewLlmLogs)="onViewLlmLogs($event)"
          (copyMessage)="onCopy($event)"
          (edit)="onEdit($event)"
          (delete)="onDelete($event)"
          (regenerate)="onRegenerate($event)"
          (swipePrev)="onSwipe($event, -1)"
          (swipeNext)="onSwipe($event, 1)"
          (saveEdit)="onSaveEdit($event)"
          (cancelEdit)="editingId.set(null)"
          (imageClick)="modalImage.set($event)"
          (saveImage)="saveImageTarget.set($event)"
          (reattribute)="reattributeTarget.set($event)"
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
        [nudgeTargetName]="nudgeTargetName()"
        (selectSpeaker)="onSelectSpeaker($event)"
        (skipUserTurn)="onSkipUserTurn()"
        (nudge)="onNudge()"
      />

      <qt-chat-composer
        [busy]="busy()"
        [chatId]="chatId()!"
        [speakingAs]="speakingAsSeat()"
        [hasActiveCharacters]="hasActiveCharacters()"
        [terminalActive]="terminalActive()"
        [documentActive]="documentPaneActive()"
        [compositionMode]="compositionMode()"
        [templateDelimiters]="templateDelimiters()"
        [narrationDelimiters]="narrationDelimiters()"
        [textReplacementRules]="textReplacementRules()"
        [textReplacementsEnabled]="textReplacementsEnabled()"
        [composerSpellcheck]="composerSpellcheck()"
        [pendingToolResults]="pendingToolResults()"
        [toolExecutionStatus]="toolExecutionStatus()"
        (dismissToolExecutionStatus)="dismissToolExecutionStatus()"
        (compositionModeChange)="onCompositionModeChange($event)"
        (pendingToolResult)="onPendingToolResult($event)"
        (removePendingToolResult)="onRemovePendingToolResult($event)"
        (send)="send($event)"
        (stop)="stop()"
        (continue)="continueTurn()"
        (openTerminal)="onOpenTerminal()"
        (openDocument)="showDocumentPicker.set(true)"
        (openGenerate)="showStandaloneGenerate.set(true)"
        (openLibrary)="showLibraryPicker.set(true)"
        (openAnnouncement)="showAnnouncement.set(true)"
        (openMail)="showComposeMail.set(true)"
        (customToolRan)="onCustomToolRan()"
      />
    </ng-template>

    <ng-template #documentPaneTpl let-entry>
      <qt-document-pane
        [entry]="entry"
        [mode]="documentMode.documentMode()"
        [delimiters]="templateDelimiters()"
        (contentChange)="documentMode.handleContentChange(entry.document.id, $event)"
        (blur)="documentMode.flushSave(entry.document.id)"
        (rename)="documentMode.renameDocument(entry.document.id, $event)"
        (close)="documentMode.closeDocument(entry.document.id)"
        (delete)="documentMode.deleteDocument(entry.document.id)"
        (toggleFocus)="documentMode.toggleFocusMode()"
      />
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

    @if (showLibraryPicker() && chatId(); as id) {
      <qt-library-file-picker-modal
        [chatId]="id"
        (fileLinked)="onLibraryFileLinked($event)"
        (mountFileAttached)="onLibraryMountFileAttached()"
        (close)="showLibraryPicker.set(false)"
      />
    }

    @if (showAnnouncement() && chatId(); as id) {
      <qt-insert-announcement-dialog
        [chatId]="id"
        [participantCharacterIds]="participantCharacterIds()"
        [audienceCandidates]="audienceCandidates()"
        (posted)="onAnnouncementPosted()"
        (close)="showAnnouncement.set(false)"
      />
    }

    @if (showComposeMail() && chatId(); as id) {
      <qt-compose-mail-dialog
        [chatId]="id"
        [participants]="mailParticipants()"
        (posted)="onMailPosted()"
        (close)="showComposeMail.set(false)"
      />
    }

    @if (showAddCharacter() && chatId(); as id) {
      <qt-add-character-dialog
        [chatId]="id"
        [existingCharacterIds]="castCharacterIds()"
        (added)="onCharacterAdded()"
        (close)="showAddCharacter.set(false)"
      />
    }

    <!-- v4 answers Remove with showConfirmation() (useChatControls:458-464);
         v5 has no confirm service, so the sentence lives in a qt-modal — the
         characters-vertical idiom. The copy is v4's, word for word. -->
    @if (removeTarget(); as target) {
      <qt-modal title="Remove Character" maxWidth="md" (close)="removeTarget.set(null)">
        <p class="qt-text-small">
          Remove <strong class="text-foreground">{{ target.name }}</strong> from this chat? Their
          past messages will remain visible, but they will no longer participate in the
          conversation.
        </p>
        <div qt-modal-footer class="flex justify-end gap-3">
          <button
            type="button"
            class="qt-button qt-button-secondary"
            [disabled]="removing()"
            (click)="removeTarget.set(null)"
          >
            Cancel
          </button>
          <button
            type="button"
            class="qt-button qt-button-destructive"
            [disabled]="removing()"
            (click)="confirmRemoveParticipant()"
          >
            {{ removing() ? 'Removing...' : 'Remove' }}
          </button>
        </div>
      </qt-modal>
    }

    @if (whisperTarget(); as target) {
      <qt-whisper-dialog
        [targetName]="target.name"
        [targetParticipantId]="target.participantId"
        (send)="onWhisperSend($event)"
        (close)="whisperTarget.set(null)"
      />
    }

    @if (showStateEditor() && chatId(); as id) {
      <qt-state-editor-modal
        entityType="chat"
        [entityId]="id"
        (close)="showStateEditor.set(false)"
      />
    }

    @if (showSearchReplace() && chat(); as c) {
      <qt-search-replace-modal
        [chatId]="c.id"
        [chatTitle]="c.title"
        (completed)="onSearchReplaced()"
        (close)="showSearchReplace.set(false)"
      />
    }

    @if (showRunTool() && chat(); as c) {
      <qt-run-tool-modal
        [chatId]="c.id"
        [participants]="c.participants"
        (executed)="onToolRun()"
        (close)="showRunTool.set(false)"
      />
    }

    @if (showToolSettings() && chat(); as c) {
      <qt-chat-tool-settings-modal
        [chatId]="c.id"
        [disabledTools]="c.disabledTools ?? []"
        [disabledToolGroups]="c.disabledToolGroups ?? []"
        [profileToolsDisabled]="profileToolsDisabled()"
        (saved)="onToolSettingsSaved()"
        (close)="showToolSettings.set(false)"
      />
    }

    @if (showAllLLMPause() && chat()) {
      <qt-all-llm-pause-modal
        [turnCount]="allLLMPauseTurnCount()"
        [nextPauseAt]="allLLMNextPauseAt()"
        [participants]="allLLMPauseParticipants()"
        (continueRun)="onAllLLMContinue()"
        (stopRun)="onAllLLMStop()"
        (takeOver)="onAllLLMTakeOver($event)"
        (close)="showAllLLMPause.set(false)"
      />
    }

    @if (showMerge() && chat(); as c) {
      <qt-merge-conversation-modal
        [targetChatId]="c.id"
        [existingCharacterIds]="castCharacterIds()"
        (merged)="onConversationMerged()"
        (close)="showMerge.set(false)"
      />
    }

    @if (handOffTarget(); as target) {
      <qt-select-llm-profile-dialog
        [characterName]="target.character?.name || 'Character'"
        [characterAvatarUrl]="target.character?.avatarUrl ?? null"
        [defaultConnectionProfileId]="null"
        (confirm)="onConfirmHandOff($event)"
        (cancel)="handOffTarget.set(null)"
      />
    }

    @if (reattributeTarget(); as target) {
      <qt-reattribute-message-dialog
        [messageId]="target.id"
        [currentParticipantId]="target.participantId"
        [participants]="chat()!.participants"
        (reattributed)="onMessageReattributed()"
        (close)="reattributeTarget.set(null)"
      />
    }

    @if (showProject() && chat(); as c) {
      <qt-chat-project-modal
        [chatId]="c.id"
        [projectId]="c.projectId"
        [projectName]="c.projectName"
        (assigned)="onProjectAssigned()"
        (close)="showProject.set(false)"
      />
    }

    @if (showBulkReplace() && chat(); as c) {
      <qt-bulk-character-replace-modal
        [chatId]="c.id"
        [participants]="c.participants"
        [messages]="c.messages"
        (reattributed)="onBulkReattributed()"
        (close)="showBulkReplace.set(false)"
      />
    }

    @if (showRename() && chat(); as c) {
      <qt-chat-rename-modal
        [chatId]="c.id"
        [currentTitle]="c.title || ''"
        [isManuallyRenamed]="c.isManuallyRenamed"
        (renamed)="onChatRenamed()"
        (close)="showRename.set(false)"
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
  private readonly toasts = inject(ToastService);
  private readonly queryClient = injectQueryClient();
  private readonly destroyRef = inject(DestroyRef);
  /** Terminal Mode state for this conversation (v4 `useTerminalMode`). */
  protected readonly terminalMode = inject(TerminalModeController);
  /** Document Mode state for this conversation (v4 `useDocumentMode`). */
  protected readonly documentMode = inject(DocumentModeController);
  /** Workspace backdrop seams (p4.9j2, v4 `useReportWorkspaceBackdrop`); null ⇒ routed. */
  private readonly backdropRegistry = inject(WORKSPACE_BACKDROP_REGISTRY, { optional: true });
  private readonly workspaceTabId = inject(WORKSPACE_TAB_ID, { optional: true });

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

    // The roleplay-template fetch — v4 `SalonView.tsx:745-776`, ported arm for
    // arm. The three reset arms (no id / non-ok / throw) are deliberately the
    // same reset, which is what lets a chat whose template was deleted keep
    // rendering with the defaults rather than showing an error.
    effect(() => {
      const templateId = this.roleplayTemplateId();
      const seq = ++this.templateFetchSeq;
      const reset = () => {
        if (seq !== this.templateFetchSeq) return;
        this.roleplayRenderingPatterns.set(undefined);
        this.roleplayDialogueDetection.set(undefined);
        this.roleplayDelimiters.set([]);
        this.roleplayNarrationDelimiters.set(null);
      };
      if (!templateId) {
        reset();
        return;
      }
      untracked(() => {
        void (async () => {
          try {
            const template = await fetchRoleplayTemplate(this.core, templateId);
            if (seq !== this.templateFetchSeq) return;
            this.roleplayRenderingPatterns.set(
              template.renderingPatterns
                ? narrowRenderingPatterns(template.renderingPatterns)
                : undefined,
            );
            this.roleplayDialogueDetection.set(template.dialogueDetection);
            this.roleplayDelimiters.set(template.delimiters ?? []);
            this.roleplayNarrationDelimiters.set(template.narrationDelimiters ?? null);
          } catch {
            reset();
          }
        })();
      });
    });

    // Report this Salon's story background to the workspace backdrop registry
    // (v4 `useReportWorkspaceBackdrop(url, isSalon: true)`). The host arbitrates
    // (a Salon with a background wins full-screen). Report the raw file URL
    // (`isSalon: true`); clear on a background-less chat and on destroy. Inert in
    // routed mode (the registry token resolves null).
    const registry = this.backdropRegistry;
    const tabId = this.workspaceTabId;
    if (registry && tabId != null) {
      effect(() => {
        const raw = rawBackdropUrl(this.backgroundVar());
        if (raw) registry.report(tabId, { url: raw, isSalon: true });
        else registry.clear(tabId);
      });
      this.destroyRef.onDestroy(() => registry.clear(tabId));
    }

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

  /**
   * The composer, so a linked library file can reach its tray. v4 keeps
   * `attachedFiles` in SalonView and passes it down; v5 keeps the tray inside the
   * composer, beside the upload machinery that fills it, so the hand-off is a
   * method call rather than a prop. The composer lives in `#chatContentTpl`,
   * which the panes render through an outlet — a view query still matches it,
   * since queries follow the DECLARATION view, not the insertion point.
   */
  private readonly composer = viewChild(ChatComposer);

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

  // -------------------------------------------------------------------------
  // The chat's roleplay template (v4 `SalonView.tsx:745-776`)
  // -------------------------------------------------------------------------

  /**
   * The template's rendering patterns / dialogue detection, handed to every
   * rendered row. Before P4.30 nothing fetched them, so EVERY message in v5
   * rendered with the built-in defaults no matter what template the chat wore.
   *
   * `undefined` is the "no template" state, and it is also where all three of
   * v4's reset arms land — no template id, a non-ok response, or a throw. That
   * is what makes a chat pointing at a DELETED template fall back to the
   * defaults instead of failing.
   *
   * `narrationDelimiters` JOINED this fetch in P4.9L, when the composer got its
   * formatting toolbar (v4 `ChatComposer.tsx:327` → `FormattingToolbar.tsx
   * :382-407`). The template's own `delimiters` ride with it — v4's toolbar
   * fetches THAT array itself, from the very same route this effect is already
   * calling, so v5 reads it here instead of opening a second source of truth
   * for one row (the toolbar's class doc records the divergence).
   *
   * One of v4's fetched values is still DELIBERATELY not kept:
   * `roleplayTemplateName` is set by v4 (`SalonView.tsx:140`) and READ NOWHERE
   * — its declaration and its four setters are its only occurrences in the
   * whole v4 checkout. Mirroring dead state would be a stub.
   */
  private readonly roleplayRenderingPatterns = signal<RenderingPattern[] | undefined>(undefined);
  private readonly roleplayDialogueDetection = signal<DialogueDetection | null | undefined>(
    undefined,
  );
  private readonly roleplayDelimiters = signal<TemplateDelimiter[]>([]);
  private readonly roleplayNarrationDelimiters = signal<NarrationDelimiters | null>(null);
  protected readonly renderingPatterns = this.roleplayRenderingPatterns.asReadonly();
  protected readonly dialogueDetection = this.roleplayDialogueDetection.asReadonly();
  protected readonly templateDelimiters = this.roleplayDelimiters.asReadonly();
  protected readonly narrationDelimiters = this.roleplayNarrationDelimiters.asReadonly();

  /**
   * v4's effect keys on `chat?.roleplayTemplateId` — a PRIMITIVE, so it re-runs
   * only when the id itself moves, not on every chat refetch. This computed is
   * what gives the effect below the same grain (a signal comparing by `Object.is`
   * notifies only on a real change), and it is also the reconcile point: change
   * the template from the sidebar, the chat refetches, this id moves, and the
   * new template is fetched mid-session.
   */
  private readonly roleplayTemplateId = computed(() => this.chat()?.roleplayTemplateId ?? null);

  /**
   * Guards against an out-of-order response: switch templates twice quickly and
   * the first fetch may land last. v4 has no such guard (its effect has no
   * cleanup), but the two only differ under an interleaving no test can pin, and
   * losing the race writes the WRONG template's patterns into the room.
   */
  private templateFetchSeq = 0;

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
    try {
      const result = await regenerateChatBackground(this.core, chatId);
      // Both §2 success arms are shown verbatim: "…queued" and "…already in
      // progress" are distinct states the user should be able to tell apart.
      this.toasts.showSuccess(result.message);
      // v4 `useChatControls` (:410) wakes the queue badges — the regeneration
      // rides the STORY_BACKGROUND_GENERATION queue.
      notifyQueueChange();
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
      this.toasts.showError(
        message || 'Failed to regenerate background',
        );
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

  // -------------------------------------------------------------------------
  // The in-chat dialog family (P4.9E3C) — v4 mounts these from `ChatModals.tsx`
  // off `useModalState`; v5 keeps its signal-per-dialog pattern (see the Post
  // Office note below, which weighed the same choice).
  //
  // `LibraryFilePickerModal` — deferred by name here through P4.9E3C — LANDED in
  // P4.9E4B: `qt-library-file-picker-modal`, opened from the composer gutter's
  // file-plus button, which had been ABSENT rather than refusing.
  //
  // ## The all-LLM pause modal (v4 `AllLLMPauseModal.tsx`, bug 37)
  //
  // Long deferred because it was unreachable in v4 itself:
  // `setAllLLMPauseModalOpen(true)` appeared nowhere, and neither app projected
  // `allLLMPauseTurnCount`. v4 `bd419ae9` gave it an opener — `chat.isPaused &&
  // isAllLLM`, keyed off the projected `isPaused`/`allLLMPauseTurnCount` and
  // riding the existing chain-complete refetch — so the deferral is void. The
  // pause it describes is the same one the chain driver already sets server-side
  // (v4 `turn-orchestrator.service.ts:126` → `services/turn_orchestrator.rs:455`,
  // over the differential-verified `quilltap-core::all_llm_pause`); the modal
  // just makes it visible instead of silent. See `all-llm-pause-modal.ts` +
  // `all-llm-pause.ts` (the `nextPauseAt` helpers' TS twin).
  // -------------------------------------------------------------------------

  /** v4 `allLLMPauseModalOpen` (`useModalState.ts:37`), opened by the effect below. */
  protected readonly showAllLLMPause = signal(false);
  /**
   * Whether every present participant is LLM-controlled and no USER message has
   * been sent (v4 `useParticipants.isAllLLM` = `isAllLLMChat(participantsAsBase)
   * && !messages.some(role === 'USER')`).
   */
  protected readonly isAllLLM = computed(() => {
    const c = this.chat();
    if (!c) return false;
    const anyUserControlled = c.participants.some(
      (p) => isParticipantPresent(p.status) && p.controlledBy === 'user',
    );
    if (anyUserControlled) return false;
    return !c.messages.some((m) => m.role === 'USER');
  });
  /** v4's opener predicate — `chat.isPaused && isAllLLM` (`SalonView.tsx:1228`). */
  private readonly allLLMPauseActive = computed(() => !!this.chat()?.isPaused && this.isAllLLM());
  private prevAllLLMPauseActive = false;
  /** v4 `chat.allLLMPauseTurnCount ?? 0` — the count the modal's copy reads. */
  protected readonly allLLMPauseTurnCount = computed(() => this.chat()?.allLLMPauseTurnCount ?? 0);
  /** v4 `getNextPauseThreshold(allLLMPauseTurnCount)` (`ChatModals.tsx:436`). */
  protected readonly allLLMNextPauseAt = computed(() =>
    getNextPauseThreshold(this.allLLMPauseTurnCount()),
  );
  /** v4 `useParticipants.llmParticipants` — the take-over roster. */
  protected readonly allLLMPauseParticipants = computed<AllLLMPauseParticipant[]>(() => {
    const impersonating = this.impersonatingIds();
    return (this.chat()?.participants ?? [])
      .filter(
        (p) =>
          p.type === 'CHARACTER' &&
          p.isActive &&
          p.controlledBy !== 'user' &&
          !impersonating.includes(p.id),
      )
      .map((p) => ({
        id: p.id,
        characterName: p.character?.name || 'Unknown',
        // v4 hands the whole character to <Avatar>, which falls back to
        // defaultImage when avatarUrl is absent — the house idiom for that
        // resolution is participantAvatar() (same as the sibling roster).
        avatarUrl: participantAvatar(p),
      }));
  });

  /**
   * Surface the all-LLM pause so it is no longer silent (v4 `SalonView.tsx:1220-
   * 1231`). The pause fires server-side and the chain-complete refetch flips the
   * projected `isPaused`, so this opens on both a live pause and loading an
   * already-paused all-LLM room. Only the false→true edge opens it, so closing
   * the modal (Continue/Stop/Take Over) never immediately reopens it while the
   * chat stays paused.
   */
  private readonly allLLMPauseOpener = effect(() => {
    const active = this.allLLMPauseActive();
    if (active && !this.prevAllLLMPauseActive) {
      this.showAllLLMPause.set(true);
    }
    this.prevAllLLMPauseActive = active;
  });

  /** v4 `handleAllLLMContinue` — dismiss; the chain resumes on the next nudge. */
  protected onAllLLMContinue(): void {
    this.showAllLLMPause.set(false);
  }

  /** v4 `handleAllLLMStop` — `chatControls.setPauseState(true)`. */
  protected async onAllLLMStop(): Promise<void> {
    this.showAllLLMPause.set(false);
    const chatId = this.chatId();
    if (!chatId) return;
    await this.core.dispatch({ type: 'chatUpdate', chatId, chat: { isPaused: true } });
    await this.queryClient.invalidateQueries({ queryKey: ['chat', chatId] });
  }

  /** v4 `handleAllLLMTakeOver` — start impersonating the chosen character. */
  protected async onAllLLMTakeOver(participantId: string): Promise<void> {
    this.showAllLLMPause.set(false);
    await this.onImpersonate(participantId);
  }

  /** v4 `libraryFilePickerOpen` (`useModalState.ts:38`), opened from the gutter. */
  protected readonly showLibraryPicker = signal(false);
  /** v4 `renameModalOpen` (`useModalState.ts`), opened from Organize. */
  protected readonly showRename = signal(false);
  /** v4 `bulkCharacterReplaceOpen`, opened from the Edit Content section. */
  protected readonly showBulkReplace = signal(false);
  /** v4 `projectModalOpen`, opened from the Chat section's Project entry. */
  protected readonly showProject = signal(false);
  /**
   * v4 `mergeConversationOpen`. Like v4 (`SalonView.tsx:1586-1597`) the dialog is
   * MOUNTED only while open, so its two-step state resets on every open without
   * a reset effect.
   */
  protected readonly showMerge = signal(false);
  /** v4 `toolSettingsModalOpen`, opened from the Chat section's Tools… entry. */
  protected readonly showToolSettings = signal(false);
  /**
   * Whether any LLM participant's connection profile forbids tool use, in which
   * case the tool-settings dialog shows a warning that its toggles are moot (v4
   * `ChatModals.tsx:401`, bug 36). Projected since v4 `bd419ae9`.
   */
  protected readonly profileToolsDisabled = computed(() =>
    (this.chat()?.participants ?? []).some(
      (p) => p.controlledBy === 'llm' && p.connectionProfile?.allowToolUse === false,
    ),
  );
  /** v4 `runToolModalOpen`, opened from the Chat section's Run Tool… entry. */
  protected readonly showRunTool = signal(false);
  /** v4 `searchReplaceOpen`, opened from the Edit Content section. */
  protected readonly showSearchReplace = signal(false);

  /** v4 `onComplete` — the transcript may have changed under the operator. */
  protected async onSearchReplaced(): Promise<void> {
    await this.onChatUpdated();
  }

  /** v4 `onToolExecuted` → `fetchChat` (`ChatModals.tsx:377`). */
  protected async onToolRun(): Promise<void> {
    await this.onChatUpdated();
  }

  /** v4 patches its local chat (`ChatModals.tsx:394-400`); v5 refetches. */
  protected async onToolSettingsSaved(): Promise<void> {
    await this.onChatUpdated();
  }

  /** v4 `onMerged` → `fetchChat` (`SalonView.tsx:1594`). */
  protected async onConversationMerged(): Promise<void> {
    await this.onChatUpdated();
  }
  /** v4 `reattributeDialogState`, opened from a message's action bar. */
  protected readonly reattributeTarget = signal<MessageDto | null>(null);
  /**
   * v4 `selectLLMProfileDialogState` — the participant whose hand-off back to the
   * AI is waiting on a profile choice. Set only by {@link onStopImpersonate}.
   */
  protected readonly handOffTarget = signal<ParticipantDetail | null>(null);

  /**
   * v4 `handleReattributed` (`SalonView.tsx:1206-1218`): close, refetch, then
   * scroll the message back into view after a beat — the list re-renders around
   * the moved row, so v4 waits 100ms before looking for it.
   */
  protected async onMessageReattributed(): Promise<void> {
    const target = this.reattributeTarget();
    this.reattributeTarget.set(null);
    await this.onChatUpdated();
    if (!target) return;
    setTimeout(() => {
      document
        .getElementById(`message-${target.id}`)
        ?.scrollIntoView({ behavior: 'smooth', block: 'center' });
    }, 100);
  }

  /** v4 `onSuccess` → `fetchChat` (`ChatModals.tsx:193`). */
  protected async onProjectAssigned(): Promise<void> {
    await this.onChatUpdated();
  }

  /** v4 `onSuccess` → `fetchChat` (`ChatModals.tsx:265`). */
  protected async onBulkReattributed(): Promise<void> {
    await this.onChatUpdated();
  }

  /**
   * v4 `onSuccess` patches its local chat object in place
   * (`ChatModals.tsx:202-206`); v5 holds the chat in a query, so the equivalent
   * is a refetch. The dialog raises its own toast, as v4's does.
   */
  protected async onChatRenamed(): Promise<void> {
    await this.onChatUpdated();
  }

  // -------------------------------------------------------------------------
  // The Post Office (P4.9E2B) — v4 mounts these from `ChatModals` /`SalonView`
  // off `useModalState`. v5 keeps its established signal-per-modal pattern
  // rather than introducing a second one (the P4.9E2B tier-2 item 9 check:
  // v5 already has an answer, so a centralized hook would be the second
  // pattern, not the first).
  // -------------------------------------------------------------------------

  /** The Insert Announcement dialog (v4 `ChatModals.tsx:317`). */
  protected readonly showAnnouncement = signal(false);
  /** The Compose Mail dialog (v4 `ChatModals.tsx:332`). */
  protected readonly showComposeMail = signal(false);
  /** The whisper target (v4 `SalonView.tsx:150` `whisperTarget`). */
  protected readonly whisperTarget = signal<{ participantId: string; name: string } | null>(null);

  /**
   * The character ids already in the scene — the announcement picker excludes
   * them (v4 `ChatModals.tsx:321-323`).
   *
   * Both filters are v4's and both matter. `type === 'CHARACTER'` skips the
   * non-character rows, and **`!removedAt`** keeps a SOFT-REMOVED participant
   * out of the exclusion set, so a character who has left the scene becomes
   * available to announce from off-scene again. v5 had neither filter; the gap
   * was invisible until P4.9E1B's cast walk performed the first soft remove any
   * e2e beat had ever produced, and the sibling Post Office beat caught it.
   */
  protected readonly participantCharacterIds = computed<string[]>(() =>
    (this.chat()?.participants ?? [])
      .filter((p) => p.type === 'CHARACTER' && !p.removedAt)
      .map((p) => p.character?.id)
      .filter((id): id is string => !!id),
  );

  /**
   * The chat's current participants, offered to Insert Announcement as
   * optional whisper targets (v4 `ChatModals.tsx:325-332`, `a163862c`). Unlike
   * `participantCharacterIds` above, this keeps a SOFT-REMOVED participant
   * OUT via `status !== 'removed'` rather than `!removedAt` — v4 filters both,
   * and a character can be soft-removed (`removedAt` unset, `status:
   * 'removed'`) without having formally left the scene.
   */
  protected readonly audienceCandidates = computed<AudienceCandidate[]>(() =>
    (this.chat()?.participants ?? [])
      .filter((p) => p.type === 'CHARACTER' && !p.removedAt && p.status !== 'removed' && p.character)
      .map((p) => ({
        participantId: p.id,
        name: p.character!.name,
        controlledBy: p.controlledBy === 'user' ? ('user' as const) : ('llm' as const),
        avatarUrl: p.character!.avatarUrl ?? null,
        status: p.status,
      })),
  );

  /**
   * The chat's CHARACTER participants as Compose Mail wants them: the workspace
   * character id (not the participant id — the mail action addresses characters),
   * the name, and who controls them.
   */
  protected readonly mailParticipants = computed<ComposeMailParticipant[]>(() =>
    (this.chat()?.participants ?? [])
      .filter((p) => p.type === 'CHARACTER' && p.character)
      .map((p) => ({
        id: p.character!.id,
        name: p.character!.name,
        controlledBy: p.controlledBy === 'user' ? ('user' as const) : ('llm' as const),
      })),
  );

  /** v4 `handleWhisper` (`SalonView.tsx:195-199`) — resolve the name, open the dialog. */
  protected onWhisper(participantId: string): void {
    const participant = (this.chat()?.participants ?? []).find((p) => p.id === participantId);
    this.whisperTarget.set({
      participantId,
      name: participant?.character?.name ?? 'Unknown',
    });
  }

  /**
   * Run the whispered turn (v4 `WhisperDialog.handleSend` + `SalonView`'s
   * `onSent`, `:1802-1805`). The dialog has already closed itself, so this runs
   * behind it: a bare `chatSend` — deliberately NOT `runTurn`, which would raise
   * the optimistic bubble and the streaming overlay a whisper does not get in v4
   * — awaited to completion so the server-side turn is not abandoned, then the
   * chat is refetched. A failure is logged, never surfaced (v4 `:71-73`).
   */
  protected async onWhisperSend(event: {
    targetParticipantId: string;
    content: string;
  }): Promise<void> {
    const chatId = this.chatId();
    if (!chatId) return;
    try {
      await this.core.dispatchExpect(
        {
          type: 'chatSend',
          chatId,
          content: event.content,
          targetParticipantIds: [event.targetParticipantId],
          speakingAsParticipantId: this.activeSpeakerId() ?? undefined,
        },
        'chatSend',
      );
    } catch (error) {
      console.error('Failed to send whisper:', error);
      return;
    }
    await this.queryClient.invalidateQueries({ queryKey: ['chat', chatId] });
  }

  /**
   * The picker linked a legacy library file (v4 `ChatModals.tsx:250-261`): push it
   * into the composer's pending-attachment tray so the next send carries it. The
   * picker raises v4's sentence itself.
   */
  protected onLibraryFileLinked(linked: LinkedLibraryFile): void {
    this.composer()?.addAttachedFile(linked.file);
  }

  /**
   * The picker pinned a document-store file (v4 `ChatModals.tsx:262-266`). There
   * is NO tray hand-off — the Librarian announcement is already in the transcript
   * — so this only refetches the chat, exactly as v4's `fetchChat()` does.
   */
  protected async onLibraryMountFileAttached(): Promise<void> {
    await this.queryClient.invalidateQueries({ queryKey: ['chat', this.chatId()] });
  }

  /** A posted announcement is a real message — refetch (v4 `onPosted` → `fetchChat`). */
  protected async onAnnouncementPosted(): Promise<void> {
    await this.queryClient.invalidateQueries({ queryKey: ['chat', this.chatId()] });
  }

  /** The dialog raises v4's delivery notice itself (`ComposeMailDialog.tsx:143`). */
  protected async onMailPosted(): Promise<void> {
    await this.queryClient.invalidateQueries({ queryKey: ['chat', this.chatId()] });
  }

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
  protected readonly userParticipantIdSet = computed<ReadonlySet<string>>(
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
  /**
   * The sidebar's display-only turn state (v4 `SalonView.tsx:144` — the client
   * copy starts empty and only ever takes `queue` back from the server, plus the
   * optimistic queue edits the sidebar's own buttons make).
   */
  protected readonly turnState = signal<TurnState>(createInitialTurnState());
  /** The selection envelope the sidebar numbers its badges from (v4 same name). */
  protected readonly turnSelection = signal<TurnSelectionResult | null>(null);
  /**
   * The user's Speaking-As choice, held ONLY for the moment between the click
   * and the refetch that confirms it. It is cleared the instant the chat comes
   * back, so `chat.activeTypingParticipantId` — the server's answer — governs
   * from then on.
   *
   * v4 never holds an unconfirmed value at all: every one of its impersonation
   * handlers assigns `data.activeTypingParticipantId` from the RESPONSE
   * (`useImpersonation.ts:63,107,134`), and its sync effect re-reads the chat.
   * A latch that outranks the server here is not a cosmetic difference — the
   * server drops the active speaker when that participant stops being
   * user-controlled, and a stale override then misattributes every optimistic
   * bubble to a character the user is no longer playing.
   */
  private readonly activeSpeakerOverride = signal<string | null>(null);

  /**
   * v4 Bug 48's client turn presentation (`SalonView.tsx`
   * `handleImpersonateAndTakeTurn`). v4 computes the turn CLIENT-side from history
   * and just overwrites `turnSelectionResult`; v5's turn is SERVER-authoritative
   * and auto-refreshed ({@link _turnEffect} → {@link refreshTurn} on every chat
   * settle), so a direct write would be clobbered by the next query — especially
   * once P4.D60's GET projection makes the post-impersonate refetch change
   * `chat()`. This override LAYERS above the server turn (the same way
   * {@link activeSpeakerOverride} layers above the persisted speaking-as) and
   * survives until a message is sent ({@link runTurn} clears it), matching v4's
   * "recomputed from history once a message is sent". Set only for a user-driven
   * (impersonated) seat, so its presence means "an impersonated user turn".
   */
  private readonly turnOverride = signal<TurnSelectionResult | null>(null);

  /** The turn seat the UI presents: the client override (Bug 48) wins over the server query. */
  private readonly effectiveNextSpeakerId = computed<string | null>(
    () => this.turnOverride()?.nextSpeakerId ?? this.turnInfo()?.nextSpeakerId ?? null,
  );

  /** The selection envelope the sidebar reads: the override wins, else the server's. */
  protected readonly effectiveTurnSelection = computed<TurnSelectionResult | null>(
    () => this.turnOverride() ?? this.turnSelection(),
  );

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
      this.applyTurnResponse(resp.data);
    } else {
      this.turnInfo.set(null);
    }
  }

  /**
   * v4 `useTurnManagement.applyServerResponse`: take the authoritative queue back
   * from `state.queue` and rebuild the selection result from the `turn` envelope.
   * `spokenSinceUserTurn` / `lastSpeakerId` are never refreshed client-side — v4
   * leaves them at their initial values too.
   */
  private applyTurnResponse(data: unknown): void {
    const body = data as {
      turn?: { nextSpeakerId?: string | null; reason?: string; cycleComplete?: boolean };
      state?: { queue?: string[] };
    };
    this.turnState.update((prev) => ({ ...prev, queue: body.state?.queue ?? prev.queue }));
    if (body.turn) {
      this.turnSelection.set({
        nextSpeakerId: body.turn.nextSpeakerId ?? null,
        reason: (body.turn.reason ?? 'weighted_selection') as TurnSelectionResult['reason'],
        cycleComplete: body.turn.cycleComplete ?? false,
      });
    }
  }

  /**
   * The Speaking-As selector's options: the seats the human may speak as — every
   * genuine user-controlled character AND every seat impersonated this session
   * (v4 `useParticipants.ts` `controlledCharacters`:
   * `(isUserControlled || isImpersonating) && isActive`). Omitting the overlay
   * arm left the selector hidden while impersonating, so the human was locked to
   * the impersonated seat and could never switch back to their own character
   * without stopping (dogfood #77).
   */
  protected readonly controlledCharacters = computed<ControlledCharacter[]>(() => {
    const impersonating = this.impersonatingIds();
    return (this.chat()?.participants ?? [])
      .filter(
        (p) =>
          p.type === 'CHARACTER' &&
          (p.controlledBy === 'user' || impersonating.includes(p.id)) &&
          p.isActive &&
          isParticipantPresent(p.status),
      )
      .map((p) => ({
        participantId: p.id,
        name: p.character?.name ?? 'Character',
        avatarUrl: participantAvatar(p),
      }));
  });

  /**
   * v4 `activeTypingParticipantId` useState (`useImpersonation.ts`): the seat the
   * human is currently speaking as, applied from the impersonate / stop replies
   * (`|| participantId` on start, `|| null` on stop) and — v4 Bug 49 — from the
   * turn-follow. LOCAL for the same reason as {@link impersonatingLocal}: the
   * persisted `activeTypingParticipantId` is only the INITIAL default, so the
   * {@link impersonationSync} effect SEEDS it once (v4 Bug 51 client,
   * `useImpersonation.ts:43` `prev => prev ?? activeTypingId ?? null`) and the
   * local governs thereafter. Reading the chat record LIVE would snap the
   * composer back to the stale persisted seat after each turn once P4.D60
   * projects it — the clobber Bug 51 fixes.
   */
  private readonly activeTypingLocal = signal<string | null>(null);

  protected readonly activeSpeakerId = computed(
    () => this.activeSpeakerOverride() ?? this.activeTypingLocal() ?? null,
  );

  private readonly nextSpeaker = computed<ParticipantDetail | null>(() => {
    const id = this.effectiveNextSpeakerId();
    if (!id) return null;
    return (this.chat()?.participants ?? []).find((p) => p.id === id) ?? null;
  });

  /**
   * The character the human is currently speaking as — resolved exactly the way
   * the server attributes a typed message (`findActiveUserParticipant`, honouring
   * the impersonation overlay), then hydrated with its avatar for the
   * composer-side cue (v4 Bug 46(b), `SalonView.tsx` `speakingAsSeat`). Null when
   * the human plays no character (e.g. an all-LLM room).
   */
  protected readonly speakingAsSeat = computed<SpeakingAsSeat | null>(() => {
    const participants = this.chat()?.participants ?? [];
    const resolved = findActiveUserParticipant(
      participants,
      this.activeSpeakerId(),
      this.impersonatingIds(),
    );
    const seatId = resolved?.id ?? this.activeSpeakerId();
    if (!seatId) return null;
    const p = participants.find((pp) => pp.id === seatId);
    if (!p?.character) return null;
    return { name: p.character.name, avatarUrl: participantAvatar(p) };
  });

  /**
   * The name whose (user-driven) turn it is, or null when it isn't.
   *
   * Gated on `isUserDrivenSeat` over the impersonation overlay, matching v4's
   * composer turn banner since `1bed814f` (`SalonView.tsx:~1428` —
   * `isUserDrivenSeat({ id, controlledBy }, impersonatingParticipantIds)`). An
   * impersonated seat keeps `controlledBy: 'llm'` (v4 Bug 44 overlay), so its
   * own turn is announced via the overlay — matching what the server returns
   * (`reason: 'user_turn'`) and `help/chat-turn-manager.md` (v4 Bug 46(a)).
   * Keying on the bare column would leave the impersonated seat's paused turn
   * with no "type as them" prompt and no Skip button.
   */
  protected readonly userTurnName = computed<string | null>(() => {
    if (this.busy()) return null;
    const next = this.nextSpeaker();
    if (
      !next ||
      !isUserDrivenSeat({ id: next.id, controlledBy: next.controlledBy ?? 'llm' }, this.impersonatingIds())
    )
      return null;
    return next.character?.name ?? 'this character';
  });

  /** Everyone else has passed → the responder must speak (no Skip button). */
  protected readonly mustSpeak = computed<boolean>(() => {
    const chat = this.chat();
    const next = this.nextSpeaker();
    if (
      !chat ||
      !next ||
      !isUserDrivenSeat({ id: next.id, controlledBy: next.controlledBy ?? 'llm' }, this.impersonatingIds()) ||
      !next.character
    )
      return false;
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
    // Bug 48: an impersonate-takes-turn override is always a user-driven (paused)
    // turn — show the banner, never a Nudge.
    if (this.turnOverride()) return null;
    const info = this.turnInfo();
    if (!info?.nextSpeakerId || info.nextSpeakerControlledBy === 'user') return null;
    const next = this.nextSpeaker();
    return next?.character?.name ?? 'the next character';
  });

  protected async onSelectSpeaker(participantId: string): Promise<void> {
    const chatId = this.chatId();
    if (!chatId) return;
    // Optimistic: reflect the pick immediately, before the reply lands.
    this.activeSpeakerOverride.set(participantId);
    try {
      const data = await this.core.dispatchData({
        type: 'chatSetActiveSpeaker',
        chatId,
        participantId,
      });
      // v4 `handleSetActiveSpeaker` (`useImpersonation.ts:157-163`): apply the
      // reply to the LOCAL mirrors — the chat GET projects neither field, so the
      // reply is the source (same as onImpersonate). The server adds a genuine
      // user seat to `impersonatingParticipantIds` and returns the updated list;
      // an LLM seat that is NOT impersonated is rejected (a thrown error here).
      this.activeTypingLocal.set(participantId);
      const ids = data['impersonatingParticipantIds'];
      if (Array.isArray(ids)) {
        this.impersonatingLocal.set(ids.filter((id): id is string => typeof id === 'string'));
      }
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to set active speaker');
    } finally {
      // The persistent mirror now governs; the transient optimistic latch retires.
      this.activeSpeakerOverride.set(null);
    }
  }

  protected async onSkipUserTurn(): Promise<void> {
    const chatId = this.chatId();
    const target = this.nextSpeaker();
    if (!chatId || !target) return;
    const resp = await this.core.dispatch({
      type: 'chatTurnAction',
      chatId,
      action: 'skipUserTurn',
      participantId: target.id,
    });
    if (resp.type === 'error') {
      // v4 `handleSkipUserTurn` (:212-215): `callTurnAction` swallows the
      // server's sentence into a console.error and the operator is told this
      // fixed line. v5's inline skip banner — which showed the server message —
      // was an invention of the no-toast era and goes with it.
      this.toasts.showError('Failed to skip turn. Please try again.');
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

  // -------------------------------------------------------------------------
  // The chat sidebar (P4.9H1) — v4's `useTurnManagement` / `useImpersonation`
  // handlers, plus the derived bags the sidebar's sections read.
  // -------------------------------------------------------------------------

  /** The state editor's visibility (v4 `modals.openStateEditor`, chat tier). */
  protected readonly showStateEditor = signal(false);

  /** v4 `participantsWithImpersonation.userParticipantId` — the user's seat. */
  protected readonly userParticipantId = computed(
    () =>
      (this.chat()?.participants ?? []).find(
        (p) => p.type === 'CHARACTER' && p.controlledBy === 'user' && isParticipantPresent(p.status),
      )?.id ?? null,
  );

  /** The Chat section's slice of the chat record. */
  protected readonly chatSectionState = computed<ChatSectionState>(() => {
    const c = this.chat();
    return {
      roleplayTemplateId: c?.roleplayTemplateId ?? null,
      // Projected by the route (v4 `get.ts:558`), so this one IS authoritative.
      avatarGenerationEnabled: c?.avatarGenerationEnabled ?? null,
      // Projected since v4 `bd419ae9` (bug 22) — a reload now shows the saved
      // clock instead of snapping to 'realtime' (see `chat-section.ts`).
      timelineMode: c?.timelineMode ?? null,
      imageProfileId: c?.imageProfileId ?? null,
      alertCharactersOfLanternImages: c?.alertCharactersOfLanternImages ?? null,
      projectId: c?.projectId ?? null,
      projectName: c?.projectName ?? null,
      // v4 seeds its badge from the RESOLVED cascade value, not the stored
      // column (`useChatControls.ts:76-82`).
      agentModeEnabled: c?.resolvedAgentModeEnabled ?? null,
    };
  });

  /**
   * The Visibility section's slice. `showThinking` and `answerConfirmationOverride`
   * are projected since v4 `bd419ae9` (bug 22), so they now survive a reload
   * instead of snapping to their defaults (see `visibility-section.ts`).
   */
  protected readonly visibilityState = computed<VisibilityState>(() => {
    const c = this.chat();
    return {
      allowCrossCharacterVaultReads: c?.allowCrossCharacterVaultReads ?? false,
      coreWhisperEnabled: c?.coreWhisperEnabled ?? null,
      coreWhisperInterval: c?.coreWhisperInterval ?? null,
      turnSkippingEnabled: c?.turnSkippingEnabled ?? null,
      showThinking: c?.showThinking ?? null,
      // The projected column is a free string; narrow it to the control's enum
      // exactly as v4's onAnswerConfirmationChange coerces (`'ON'|'OFF'|null`).
      answerConfirmationOverride:
        c?.answerConfirmationOverride === 'ON'
          ? 'ON'
          : c?.answerConfirmationOverride === 'OFF'
            ? 'OFF'
            : null,
    };
  });

  /** v4 gates the Turn Skipping row on `qualifiesForTurnSkipping(participants)`. */
  protected readonly turnSkippingApplies = computed(() =>
    qualifiesForTurnSkipping(
      (this.chat()?.participants ?? []).map((p) => ({
        id: p.id,
        type: p.type,
        characterId: p.character?.id ?? null,
        controlledBy: p.controlledBy,
        status: p.status,
      })),
    ),
  );

  /** Any sidebar write landed → refetch the chat record (v4 `onChatUpdated`). */
  protected async onChatUpdated(): Promise<void> {
    await this.queryClient.invalidateQueries({ queryKey: ['chat', this.chatId()] });
  }

  // -------------------------------------------------------------------------
  // The cast (P4.9E1B) — v4's `AddCharacterDialog`, opened from the sidebar's
  // participants footer exactly as `SalonView` opens it (`ChatModals.tsx:305`).
  // -------------------------------------------------------------------------

  protected readonly showAddCharacter = signal(false);

  /** The character ids already in the cast — v4's `existingCharacterIds`. */
  protected readonly castCharacterIds = computed(() =>
    (this.chat()?.participants ?? [])
      .map((p) => p.character?.id)
      .filter((id): id is string => !!id),
  );

  /** v4 `onCharacterAdded` — refetch the chat (the dialog says who joined). */
  protected async onCharacterAdded(): Promise<void> {
    await this.onChatUpdated();
  }

  /**
   * The connection profiles the cast cards' Controlled-By selects offer (v4
   * threads `connectionProfiles` from `SalonView` into `ChatSidebar`).
   */
  protected readonly profilesQuery = injectQuery(() => ({
    queryKey: ['connection-profiles'] as const,
    queryFn: async () => {
      const data = await this.core.dispatchData({ type: 'connectionProfileList' });
      return (data['profiles'] as ConnectionProfileDto[]) ?? [];
    },
  }));

  protected readonly connectionProfiles = computed<ConnectionProfileOption[]>(() =>
    (this.profilesQuery.data() ?? []).map((p) => ({
      id: p.id,
      name: p.name,
      provider: p.provider,
      modelName: p.modelName,
    })),
  );

  /**
   * v4 `handleConnectionProfileChange` (`useChatControls:500-526`). The subtle
   * part is on the wire, not here: user control sends `connectionProfileId:
   * undefined`, which `JSON.stringify` DROPS — so the key is absent, never null.
   * {@link updateParticipant} reproduces that by not passing the key at all.
   */
  protected async onParticipantProfileChange(change: {
    participantId: string;
    profileId: string | null;
    controlledBy: 'llm' | 'user';
  }): Promise<void> {
    await this.writeParticipant(
      change.participantId,
      change.controlledBy === 'user'
        ? { controlledBy: 'user' }
        : { controlledBy: 'llm', ...(change.profileId ? { connectionProfileId: change.profileId } : {}) },
      'Connection profile updated',
      'Failed to update connection profile',
    );
  }

  /**
   * v4 `handleSystemPromptChange` (`:531-556`) — and here the explicit `null`
   * matters: "Use default prompt" CLEARS the override, which the server only
   * hears if the key is present and null.
   */
  protected async onParticipantSystemPromptChange(change: {
    participantId: string;
    promptId: string | null;
  }): Promise<void> {
    await this.writeParticipant(
      change.participantId,
      { selectedSystemPromptId: change.promptId },
      'System prompt updated',
      'Failed to update system prompt',
    );
  }

  /** v4 `handleRebuildSystemPrompt` (`:562-580`). */
  protected async onParticipantRebuildSystemPrompt(participantId: string): Promise<void> {
    const chatId = this.chatId();
    if (!chatId) return;
    try {
      await rebuildSystemPrompt(this.core, chatId, participantId);
      this.toasts.showSuccess('System prompt rebuilt');
      await this.onChatUpdated();
    } catch (err) {
      this.toasts.showError(
        err instanceof Error ? err.message : 'Failed to rebuild system prompt',
        );
    }
  }

  /**
   * v4 `handleParticipantSettingsChange` (`:583-611`) — the status select sends
   * `status` AND v4's derived `isActive` (`ChatSidebar.tsx:818`: active or silent
   * counts as active), so a client that sent only `status` would leave the legacy
   * column stale.
   */
  protected async onParticipantStatusChange(change: {
    participantId: string;
    status: ParticipantStatusWire;
  }): Promise<void> {
    await this.writeParticipant(
      change.participantId,
      {
        status: change.status,
        isActive: change.status === 'active' || change.status === 'silent',
      },
      null,
      'Failed to update participant settings',
    );
  }

  /**
   * v4 `handleTalkativenessChange` (`:613-635`): DEBOUNCED per participant, so a
   * slider drag fires one request when the user lets go rather than one per
   * step. The timers are keyed by participant id, exactly as v4's ref map is.
   */
  private readonly talkativenessTimers = new Map<string, ReturnType<typeof setTimeout>>();

  protected onParticipantTalkativenessChange(change: {
    participantId: string;
    value: number;
  }): void {
    const pending = this.talkativenessTimers.get(change.participantId);
    if (pending) clearTimeout(pending);
    this.talkativenessTimers.set(
      change.participantId,
      setTimeout(() => {
        this.talkativenessTimers.delete(change.participantId);
        void this.writeParticipant(
          change.participantId,
          { talkativeness: change.value },
          null,
          'Failed to update talkativeness',
        );
      }, TALKATIVENESS_DEBOUNCE_MS),
    );
  }

  /**
   * v4 `handleRemoveCharacter` (`:449-497`): refuse while that character is
   * mid-generation, confirm, remove, drop them from the local queue, refetch —
   * and warn if the cast is now empty of characters.
   */
  protected readonly removeTarget = signal<{ participantId: string; name: string } | null>(null);
  protected readonly removing = signal(false);

  protected onParticipantRemoveRequested(participantId: string): void {
    const participant = (this.chat()?.participants ?? []).find((p) => p.id === participantId);
    const name = participant?.character?.name ?? 'This character';
    if (this.busy() && this.turnState().lastSpeakerId === participantId) {
      this.toasts.showError(
        `Cannot remove ${name} while they are generating a response. Please wait for them to finish.`,
        );
      return;
    }
    this.removeTarget.set({ participantId, name });
  }

  protected async confirmRemoveParticipant(): Promise<void> {
    const target = this.removeTarget();
    const chatId = this.chatId();
    if (!target || !chatId) return;
    this.removing.set(true);
    try {
      await removeParticipant(this.core, chatId, target.participantId);
      this.toasts.showSuccess(
        `${target.name} has been removed from the chat`,
        );
      this.turnState.update((prev) => removeFromQueue(prev, target.participantId));
      this.removeTarget.set(null);
      await this.onChatUpdated();
      const remaining = (this.chat()?.participants ?? []).filter(
        (p) =>
          p.type === 'CHARACTER' && p.isActive && p.id !== target.participantId,
      );
      if (remaining.length === 0) {
        this.toasts.showError(
          'All characters have been removed. Add a character to continue the conversation.',
          );
      }
    } catch (err) {
      this.toasts.showError(
        err instanceof Error ? err.message : 'Failed to remove character',
        );
    } finally {
      this.removing.set(false);
    }
  }

  // -------------------------------------------------------------------------
  // Pending tool results (P4.9E1B) — the composer gutter's RNG rolls in preview
  // mode, so a roll waits here as a chip until the next send carries it. v4 owns
  // the list at `SalonView.tsx:148` for the same reason: it must outlive the
  // composer's own post-send reset.
  // -------------------------------------------------------------------------

  protected readonly pendingToolResults = signal<PendingToolResultChip[]>([]);

  /** v4 `handleAddPendingToolResult` (`SalonView.tsx:605-612`). */
  protected onPendingToolResult(result: RngPendingResult): void {
    this.pendingToolResults.update((prev) => [
      ...prev,
      { ...result, id: crypto.randomUUID(), createdAt: new Date().toISOString() },
    ]);
  }

  /** v4 `handleRemovePendingToolResult` (`:614-616`). */
  protected onRemovePendingToolResult(id: string): void {
    this.pendingToolResults.update((prev) => prev.filter((r) => r.id !== id));
  }

  /** The shared write: patch, report, refetch (v4's four handlers all do this). */
  private async writeParticipant(
    participantId: string,
    patch: UpdateParticipantPatch,
    success: string | null,
    failure: string,
  ): Promise<void> {
    const chatId = this.chatId();
    if (!chatId) return;
    try {
      await updateParticipant(this.core, chatId, participantId, patch);
      if (success) this.toasts.showSuccess(success);
      await this.onChatUpdated();
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : failure);
    }
  }

  /**
   * v4 `handleNudge`: unpause if paused, move the participant to the front of
   * the local queue, then generate directly — deliberately WITHOUT the `nudge`
   * turn action, which would queue them a second time and make the server chain
   * produce a duplicate response. The summon withholds the skip option.
   */
  protected async onSidebarNudge(participantId: string): Promise<void> {
    const chat = this.chat();
    const participant = (chat?.participants ?? []).find((p) => p.id === participantId);
    if (participant?.controlledBy === 'user') {
      this.toasts.showError(
        'User-controlled characters cannot be nudged for AI response. Use Queue instead.',
      );
      return;
    }
    if (!participant) return;
    if (chat?.isPaused) {
      await this.onTogglePause();
    }
    this.turnState.update((prev) => nudgeParticipant(prev, participantId));
    await this.runTurn({ continueMode: true, respondingParticipantId: participantId, nudge: true });
  }

  /** v4 `handleQueue`: optimistic add, then the authoritative server state. */
  protected async onSidebarQueue(participantId: string): Promise<void> {
    const chatId = this.chatId();
    if (!chatId) return;
    this.turnState.update((prev) => addToQueue(prev, participantId));
    const resp = await this.core.dispatch({
      type: 'chatTurnAction',
      chatId,
      action: 'queue',
      participantId,
    });
    if (resp.type === 'turnAction') this.applyTurnResponse(resp.data);
  }

  /** v4 `handleDequeue`. */
  protected async onSidebarDequeue(participantId: string): Promise<void> {
    const chatId = this.chatId();
    if (!chatId) return;
    this.turnState.update((prev) => removeFromQueue(prev, participantId));
    const resp = await this.core.dispatch({
      type: 'chatTurnAction',
      chatId,
      action: 'dequeue',
      participantId,
    });
    if (resp.type === 'turnAction') this.applyTurnResponse(resp.data);
  }

  /**
   * The user card's Skip (v4 wires it to `handleContinue`, not to
   * `skipUserTurn`): ask the server who is up, and if it is an LLM, let them
   * speak.
   */
  protected async onSidebarSkip(): Promise<void> {
    const chatId = this.chatId();
    if (!chatId) return;
    // v4 `handleContinue`'s three arms, each with its own toast (:179-202).
    if (!this.hasActiveCharacters()) {
      this.toasts.showError('No characters available. Add a character to continue.');
      return;
    }
    const resp = await this.core.dispatch({ type: 'chatTurnAction', chatId, action: 'query' });
    if (resp.type !== 'turnAction') {
      this.toasts.showError('Failed to determine next speaker. Please try again.');
      return;
    }
    this.applyTurnResponse(resp.data);
    const turn = (resp.data as { turn?: TurnInfo }).turn;
    if (turn?.nextSpeakerId && turn.nextSpeakerId !== this.userParticipantId()) {
      // v4 returns silently when the next speaker is the operator's own
      // character — there is nothing to generate and nothing to report.
      if (turn.nextSpeakerControlledBy === 'user') return;
      await this.runTurn({ continueMode: true, respondingParticipantId: turn.nextSpeakerId });
    } else {
      this.toasts.showInfo(
        'No characters available to speak. Try adding or activating a character.',
      );
    }
  }

  /**
   * v4 `useImpersonation.handleStartImpersonation` wrapped by
   * `handleImpersonateAndTakeTurn` (`SalonView.tsx`, Bug 48). Both of v4's
   * impersonate entry points (the sidebar's `onImpersonate` and the AllLLMPause
   * take-over) route through the wrapper; in v5 they both route through this
   * method, so the take-the-turn logic lives here once and covers both
   * ({@link onAllLLMTakeOver} delegates here).
   */
  protected async onImpersonate(participantId: string): Promise<void> {
    const chatId = this.chatId();
    if (!chatId) return;
    const name =
      this.chat()?.participants.find((p) => p.id === participantId)?.character?.name ?? 'Character';
    try {
      const data = await this.core.dispatchData({ type: 'chatImpersonate', chatId, participantId });
      this.impersonatingLocal.set(readImpersonatingIds(data, [participantId]));
      // v4 `setActiveTypingParticipantId(data.activeTypingParticipantId || participantId)`.
      this.activeTypingLocal.set(readActiveTyping(data) ?? participantId);
      this.toasts.showSuccess(`Now speaking as ${name}`);
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to start impersonation');
      return;
    }
    // Bug 48: impersonating is an explicit "I'll take this character now", so —
    // unless an LLM is mid-generation — hand the current turn to that seat. The
    // banner then reads its turn and, via the Bug 49 follow, the composer speaks
    // as it, so a typed message lands in turn. `!busy()` is v5's
    // `!streamingRef.current` (the {@link _turnEffect} gate's source of truth):
    // an LLM mid-stream is left undisturbed. The override survives the refetch
    // below because it layers above the server-queried turn.
    if (!this.busy()) {
      this.turnOverride.set({ nextSpeakerId: participantId, reason: 'queue', cycleComplete: false });
    }
    await this.queryClient.invalidateQueries({ queryKey: ['chat', chatId] });
  }

  /**
   * Who the operator is currently speaking for.
   *
   * ⚠ **This is LOCAL state on purpose, and fixing that is what made the
   * hand-off dialog reachable at all.** Neither app's chat GET projects
   * `impersonatingParticipantIds` — v4's `handlers/get.ts` has no such key, so
   * v4's own `useEffect` on `chat?.impersonatingParticipantIds` never fires and
   * the hook lives entirely off what the impersonate / stop-impersonate replies
   * return (`useImpersonation.ts:26,44-47,62-63`). v5 had been binding the
   * sidebar straight to the chat record, so every refetch — including the one
   * the impersonate dispatch itself triggers — erased the state and the card
   * snapped back to "Speak as". Impersonation could not be entered at all.
   *
   * v4's sync guard is kept, but it now lives in {@link impersonationSync}: the
   * local is authoritative and the chat record only RE-SEEDS it (never overrides
   * it live), so a stale non-empty record cannot resurrect a just-stopped
   * impersonation between the reply and the refetch (v4 Bug 51 client — see the
   * clobber-guard parity spec).
   */
  private readonly impersonatingLocal = signal<string[]>([]);
  protected readonly impersonatingIds = computed(() => this.impersonatingLocal());

  /**
   * v4 `useImpersonation` chat-sync effect (`useImpersonation.ts:29-48`), Bug 51's
   * client half. The overlay is HELD LOCAL — the chat GET projected neither field
   * before P4.D60, and even after it lands the mutation replies own every
   * transition — so the persisted values are only a SEED:
   *  - the impersonating LIST is re-applied from the record whenever it is
   *    NON-EMPTY (`:34-36`); a record that omits it (or carries `[]`) leaves the
   *    local alone, so the reply handlers own every transition, including → empty
   *    (the refetch after a stop arrives already-consistent);
   *  - the speaking-as is seeded ONCE, only while still unset
   *    (`prev => prev ?? activeTypingId ?? null`, `:43`); re-applying it on every
   *    refetch would clobber the turn-follow (Bug 49) and any manual pick — the
   *    composer would snap back to the stale persisted seat after each turn.
   * (`allLLMPauseTurnCount` is read straight off the chat by a computed, so the
   * effect need not mirror v4's third `setAllLLMPauseTurnCount` arm.)
   */
  private readonly impersonationSync = effect(() => {
    const chat = this.chat();
    const ids = chat?.impersonatingParticipantIds;
    if (ids && ids.length > 0) {
      this.impersonatingLocal.set([...ids]);
      untracked(() => {
        if (this.activeTypingLocal() == null) {
          this.activeTypingLocal.set(chat.activeTypingParticipantId ?? null);
        }
      });
    }
  });

  /**
   * v4 Bug 49 (`SalonView.tsx` turn-follow effect, `f6eac168`): the composer's
   * speaking-as follows the current user-driven turn. When the turn lands on a
   * seat the human drives — their own character OR one they are impersonating
   * (Bug 44 overlay) — and that seat CHANGES, default the speaking-as to it, so
   * on the impersonated character's own turn you speak as them without a manual
   * switch.
   *
   * Keyed on the turn SEAT (a latch), not on the speaking-as value: a deliberate
   * SpeakerSelector choice made on the SAME turn still sticks (it moves
   * `activeTypingLocal` without moving the turn seat, and the latch leaves it
   * alone until the turn seat itself changes). A non-user seat or no next speaker
   * clears the latch. It sets only the client speaking-as, which the send path
   * forwards as `speakingAsParticipantId` — no per-turn persistence. The
   * `activeTypingLocal` read is `untracked` so the effect reacts to turn-seat
   * changes only, never to its own write.
   */
  private lastFollowedTurnSeat: string | null = null;
  private readonly turnFollow = effect(() => {
    const nextId = this.effectiveNextSpeakerId();
    if (!nextId) {
      this.lastFollowedTurnSeat = null;
      return;
    }
    const next = (this.chat()?.participants ?? []).find((p) => p.id === nextId);
    if (
      !next ||
      !isUserDrivenSeat({ id: next.id, controlledBy: next.controlledBy ?? 'llm' }, this.impersonatingIds())
    ) {
      this.lastFollowedTurnSeat = null;
      return;
    }
    // Only react when the user-driven turn seat itself changes — not when the
    // human re-picks the speaking-as on the same turn.
    if (this.lastFollowedTurnSeat === nextId) return;
    this.lastFollowedTurnSeat = nextId;
    untracked(() => {
      if (this.activeTypingLocal() !== nextId) {
        this.activeTypingLocal.set(nextId);
      }
    });
  });

  /**
   * v4 `useImpersonation.handleStopImpersonation` (`:71-113`).
   *
   * **The early return is the whole point.** If the participant is a character
   * with no connection profile of their own, v4 does NOT call the server: it
   * opens `SelectLLMProfileDialog` and returns, because handing the character
   * back to the AI needs somebody to drive them and nothing on the record says
   * who. The dialog's confirm resumes the flow with `newConnectionProfileId`.
   */
  protected async onStopImpersonate(participantId: string): Promise<void> {
    const chatId = this.chatId();
    if (!chatId) return;
    const participant = this.chat()?.participants.find((p) => p.id === participantId);
    if (participant?.character && !participant.connectionProfile) {
      this.handOffTarget.set(participant);
      return;
    }
    const name = participant?.character?.name ?? 'Character';
    try {
      const data = await this.core.dispatchData({
        type: 'chatStopImpersonate',
        chatId,
        participantId,
      });
      this.impersonatingLocal.set(readImpersonatingIds(data, []));
      // v4 `setActiveTypingParticipantId(data.activeTypingParticipantId || null)`.
      this.activeTypingLocal.set(readActiveTyping(data));
      this.toasts.showSuccess(`Stopped speaking as ${name}`);
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to stop impersonation');
      return;
    }
    await this.queryClient.invalidateQueries({ queryKey: ['chat', chatId] });
  }

  /** v4 `handleConfirmStopImpersonation` (`useImpersonation.ts:115-142`). */
  protected async onConfirmHandOff(connectionProfileId: string): Promise<void> {
    const chatId = this.chatId();
    const participant = this.handOffTarget();
    this.handOffTarget.set(null);
    if (!chatId || !participant) return;
    const name = participant.character?.name || 'Character';
    try {
      const data = await this.core.dispatchData({
        type: 'chatStopImpersonate',
        chatId,
        participantId: participant.id,
        newConnectionProfileId: connectionProfileId,
      });
      this.impersonatingLocal.set(readImpersonatingIds(data, []));
      // v4 `setActiveTypingParticipantId(data.activeTypingParticipantId || null)`.
      this.activeTypingLocal.set(readActiveTyping(data));
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to assign LLM profile');
      return;
    }
    this.toasts.showSuccess(`${name} is now controlled by AI`);
    await this.queryClient.invalidateQueries({ queryKey: ['chat', chatId] });
  }

  /** v4 `handleRegenerateAvatar` (`SalonView.tsx:256-276`) — the card's camera button. */
  protected async onRegenerateAvatar(participantId: string): Promise<void> {
    const chatId = this.chatId();
    const participant = (this.chat()?.participants ?? []).find((p) => p.id === participantId);
    const characterId = participant?.character?.id;
    if (!chatId || !characterId) return;
    const name = participant?.character?.name || 'Unknown';
    const resp = await this.core.dispatch({ type: 'chatRegenerateAvatar', chatId, characterId });
    if (resp.type === 'error') {
      this.toasts.showError(resp.data.message || 'Failed to regenerate avatar');
      return;
    }
    this.toasts.showInfo(`Avatar regeneration queued for ${name}`);
    await this.queryClient.invalidateQueries({ queryKey: ['chat', chatId] });
  }

  /** v4 `togglePause` (`useChatControls.ts:194-201`). */
  protected async onTogglePause(): Promise<void> {
    const chatId = this.chatId();
    const chat = this.chat();
    if (!chatId || !chat) return;
    const paused = !chat.isPaused;
    await this.core.dispatch({ type: 'chatUpdate', chatId, chat: { isPaused: paused } });
    this.toasts.showInfo(paused ? 'Auto-responses paused' : 'Auto-responses resumed');
    await this.queryClient.invalidateQueries({ queryKey: ['chat', chatId] });
  }

  protected onNudge(): void {
    // Bug 48: an impersonate-takes-turn override is a user-driven turn — no nudge.
    if (this.turnOverride()) return;
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
    // v4 `useSSEStreaming:606-612` snapshots the pending results and clears them
    // BEFORE the request, so a second send cannot carry the same roll twice.
    const pending = this.pendingToolResults();
    this.pendingToolResults.set([]);
    void this.runTurn({ content: payload.content, fileIds: payload.fileIds, pending });
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
    /** Rolled-but-unsent tool results riding this send (v4 `pendingToolResults`). */
    pending?: PendingToolResultChip[];
  }): Promise<void> {
    const chatId = this.chatId();
    if (!chatId || this.busy()) {
      return;
    }
    // Bug 48: a turn action supersedes the optimistic impersonate-takes-turn
    // override — v4 recomputes `turnSelectionResult` from history once a message
    // is sent, so the server's post-send turn governs from here on.
    this.turnOverride.set(null);

    const hasAttachments = (opts.fileIds?.length ?? 0) > 0;
    const pending = opts.pending ?? [];
    if (opts.content || hasAttachments || pending.length > 0) {
      this.optimisticUser.set(this.makeTempUserMessage(opts.content ?? ''));
      // A user send always chases the bottom and re-enables auto-scroll (v4).
      this.messageList()?.scrollOnUserMessage();
    }

    let state: ChatStreamState = { ...initialChatStreamState(), waitingForResponse: true };
    this.stream.set(state);

    const sub = this.core.events$
      .pipe(filter((frame) => frame.chatId === chatId))
      .subscribe((frame) => {
        const before = state;
        state = reduceChatFrame(state, frame);
        this.stream.set(state);
        this.reportStreamTransitions(before, state);
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
          // v4 `:658-666` maps each chip to the SIX fields the server's schema
          // takes — the chip's own id, displayName and icon are client-side
          // presentation and never travel.
          pendingToolResults: pending.length
            ? pending.map((r) => ({
                tool: r.tool,
                success: r.success,
                result: r.formattedResult,
                prompt: r.requestPrompt,
                arguments: r.arguments,
                createdAt: r.createdAt,
              }))
            : undefined,
          // Thread the Speaking-As choice onto a user-authored send (v4 does the
          // same); irrelevant to a continue/nudge, so only sent with content or
          // an attachment-only message.
          speakingAsParticipantId:
            opts.content || hasAttachments || pending.length > 0
              ? (this.activeSpeakerId() ?? undefined)
              : undefined,
        },
        'chatSend',
      );
    } catch (err) {
      // v4 `useSSEStreaming` (:841-845): an unknown/TypeError failure reads as a
      // lost connection; anything else keeps the server's own sentence.
      const raw = err instanceof Error ? err.message : String(err);
      const display =
        raw === 'Unknown error' || raw === 'TypeError' ? 'Connection lost. Please try again.' : raw;
      this.toasts.showError(display || 'Failed to send message');
      state = { ...state, error: display || 'Failed to send message' };
      this.stream.set(state);
    } finally {
      sub.unsubscribe();
    }

    // Reconcile: refetch the canonical chat (v4 `fetchChat()` on done), then clear
    // the optimistic overlays so the streamed bubbles hand off without duplication.
    await this.queryClient.invalidateQueries({ queryKey: ['chat', chatId] });
    // Wake the queue badges — the turn just enqueued post-turn jobs (v4 fires
    // notifyQueueChange at all four useSSEStreaming completion callbacks
    // (:771/:827/:1018/:1038); v5's single reconcile point covers them).
    notifyQueueChange();
    // Drop a tool-execution notice still stuck at 'pending' — its result never
    // arrived (v4 Bug 77 calls `clearPendingToolExecutionStatus` at BOTH onDone
    // boundaries, `:848`/`:1016`; this single reconcile point stands in for all
    // of them, which is the whole point of the bug's fix: no route out of a turn
    // may strand the notice). A SETTLED notice is deliberately left alone — its
    // own 6 s countdown is running and cutting it short would rob the user of
    // the outcome.
    this.clearPendingToolExecutionStatus();
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

  /**
   * The tool-execution notice above the composer (v4 `useSSEStreaming.ts`'s
   * `toolExecutionStatus`, Bug 77). v5's SSE fold is the deliberately pure
   * `chat-stream.reducer`, so the notice — state with a lifetime — lives here in
   * the vertical, beside the other side effects the transitions raise.
   */
  protected readonly toolExecutionStatus = signal<ToolExecutionStatus | null>(null);

  /**
   * Auto-dismiss timer for a SETTLED notice. v4 held it in a ref; a plain field
   * is its analogue here — nothing renders off it, so it must not be a signal.
   */
  private toolStatusTimer: ReturnType<typeof setTimeout> | null = null;

  /** How long a settled tool-execution notice lingers before it dismisses itself. */
  private static readonly TOOL_STATUS_DISMISS_MS = 6000;

  /**
   * The single door for raising the notice (v4 `publishToolExecutionStatus`,
   * `:342-357`). A `'pending'` notice stays up until it settles or the turn
   * ends; a settled one schedules its own dismissal, so no caller has to
   * remember to tear it down. Each publish supersedes the timer before it.
   */
  private publishToolExecutionStatus(status: ToolExecutionStatus): void {
    this.clearToolStatusTimer();
    this.toolExecutionStatus.set(status);
    if (status.status !== 'pending') {
      this.toolStatusTimer = setTimeout(() => {
        this.toolStatusTimer = null;
        this.toolExecutionStatus.set(null);
      }, SalonConversation.TOOL_STATUS_DISMISS_MS);
    }
  }

  /**
   * Turn-boundary cleanup (v4 `clearPendingToolExecutionStatus`, `:333-340`):
   * drop a notice that is still `'pending'` — its tool result never arrived. A
   * settled notice is left alone; its own countdown is already running and
   * cutting it short would rob the user of the outcome.
   */
  private clearPendingToolExecutionStatus(): void {
    if (this.toolExecutionStatus()?.status === 'pending') {
      this.toolExecutionStatus.set(null);
    }
  }

  /**
   * Clear the notice and any pending auto-dismiss timer (v4
   * `dismissToolExecutionStatus`, `:324-331`) — the close button's handler, and
   * what `stop()` calls so aborting a turn clears it at once.
   */
  protected dismissToolExecutionStatus(): void {
    this.clearToolStatusTimer();
    this.toolExecutionStatus.set(null);
  }

  private clearToolStatusTimer(): void {
    if (this.toolStatusTimer !== null) {
      clearTimeout(this.toolStatusTimer);
      this.toolStatusTimer = null;
    }
  }

  /**
   * Cancel a live auto-dismiss timer on teardown, so nothing writes the signal
   * after the component is gone (v4 clears the same ref in its unmount effect,
   * `:308-322`).
   */
  private readonly _toolStatusTeardown = this.destroyRef.onDestroy(() =>
    this.clearToolStatusTimer(),
  );

  /**
   * v4 raises these from inside its SSE reader; v5's reader is the pure
   * reducer, so the reporting rides its state transitions instead.
   *
   *  - a `retrying` status stage → v4's warning toast, the server's own
   *    sentence (`useSSEStreaming.ts:430-434`). Two edge divergences from
   *    riding transitions rather than events: v4 toasts on EVERY retrying
   *    status event (repeats included) where this fires once per entry into
   *    the stage, and an empty message is skipped where v4 would toast a
   *    blank — both deliberate (the reducer coalesces repeats; a blank toast
   *    informs no one);
   *  - a done frame carrying a FAILED attachment ledger → v4's warning toast
   *    (`:601-616`, bug 94), raised before the chain branch so an intermediate
   *    done warns too;
   *  - a terminal `emptyResponse` → v4's error toast with the server's reason
   *    or its fallback (`:720-722`);
   *  - a recorded transport/stream error → v4's `:1024-1027` toast;
   *  - a `generate_image` tool result → v4's success/failure pair (`:350-367`),
 *    the failure carrying the tool's own sentence (Bug 84).
   */
  private reportStreamTransitions(before: ChatStreamState, after: ChatStreamState): void {
    if (after.status?.stage === 'retrying' && before.status?.stage !== 'retrying') {
      const message = after.status.message;
      if (message) this.toasts.showWarning(message);
    }
    // Attachments the provider plugin could not put on the wire. The ledger has
    // always been emitted and, until v4 `a14a1811`, was never displayed (bug
    // 94) — so an image that silently failed to reach a vision model looked
    // exactly like a model that had seen it and ignored it. v4 reads it inside
    // `if (data.done)`, immediately after clearing the status and BEFORE the
    // chain branch, so it warns once per done EVENT, intermediate dones
    // included. Riding transitions, the ledger object's identity is that key:
    // every done frame brings its own (the reducer carries the frame's object
    // through), while the Courier's `pendingExternalTurn` patch spreads the
    // previous one forward unchanged and must not warn twice.
    const ledger = after.finalDone?.attachmentResults ?? null;
    if (ledger !== (before.finalDone?.attachmentResults ?? null)) {
      const failedAttachments = ledger?.failed;
      if (Array.isArray(failedAttachments) && failedAttachments.length > 0) {
        const first = failedAttachments[0]?.error ?? 'unknown reason';
        const more =
          failedAttachments.length > 1 ? ` (and ${failedAttachments.length - 1} more)` : '';
        this.toasts.showWarning(
          `${
            failedAttachments.length === 1
              ? 'An attachment was'
              : `${failedAttachments.length} attachments were`
          } not sent to the model${more}: ${first}`,
        );
      }
    }
    if (after.finalDone?.emptyResponse && !before.finalDone?.emptyResponse) {
      this.toasts.showError(
        after.finalDone.emptyResponseReason ||
          'The AI returned an empty response. Use the Resend button to try again.',
      );
    }
    if (after.error && after.error !== before.error) {
      this.toasts.showError(after.error);
    }
    // Every generate_image call already seen, and those of them already settled.
    // v4 raises the notice from its two SSE callbacks (`trackToolsDetected` /
    // `trackToolResult`); riding transitions, a call is "newly detected" when its
    // id was absent before, and "newly settled" when it was pending before.
    const seen = new Set(before.toolBatches.flatMap((b) => b.calls.map((c) => c.id)));
    const settled = new Set(
      before.toolBatches.flatMap((b) =>
        b.calls.filter((c) => c.status !== 'pending').map((c) => c.id),
      ),
    );
    for (const batch of after.toolBatches) {
      for (const call of batch.calls) {
        if (call.name !== 'generate_image') continue;
        if (call.status === 'pending') {
          // v4 `trackToolsDetected:381` — raised the moment the batch is
          // detected, and it stays up until the result lands or the turn ends.
          if (!seen.has(call.id)) {
            this.publishToolExecutionStatus({
              tool: 'generate_image',
              status: 'pending',
              message: `Generating image...`,
            });
          }
          continue;
        }
        if (settled.has(call.id)) continue;
        const result = (call.result ?? {}) as { images?: unknown[] };
        if (call.status === 'success') {
          const count = result.images?.length || 1;
          // v4 raises the settled NOTICE and the toast both (`:417-421`) — the
          // two carry different sentences and neither stands in for the other.
          this.publishToolExecutionStatus({
            tool: call.name,
            status: 'success',
            message: `Successfully generated ${count} image${count > 1 ? 's' : ''}!`,
          });
          this.toasts.showSuccess(
            `Image generation complete! ${count} image${count > 1 ? 's' : ''} generated.`,
          );
        } else {
          // v4 `:444-453`. The failing tool's own sentence rides the frame's
          // SIBLING `error` (carried onto the call by the reducer as
          // `errorText`); the generic strings below survive only as the
          // fallback for a frame that carries nothing worth showing. Reading
          // `result.error` — one level too deep — is what Bug 84 was.
          const detail = resolveToolResultErrorText({ result: call.result, error: call.errorText });
          this.publishToolExecutionStatus({
            tool: call.name,
            status: 'error',
            message: detail || 'Failed to generate image',
          });
          this.toasts.showError(`Image generation failed: ${detail || 'Unknown error'}`);
        }
      }
    }
  }

  protected stop(): void {
    // The server turn rides the shared SSE and can't be aborted from here yet;
    // clear the local streaming overlay (tracked deferral: a real stop dispatch).
    // v4 `stopStreaming` (:1055-1057) reports only when prose had begun.
    // v4 `stopStreaming:1128` dismisses outright — aborting a turn clears the
    // notice at once, settled or not.
    this.dismissToolExecutionStatus();
    if (this.stream()?.content) this.toasts.showInfo('Response stopped - chat paused');
    this.stream.set(null);
    this.optimisticUser.set(null);
  }

  private makeTempUserMessage(content: string): MessageDto {
    const participants = this.chat()?.participants ?? [];
    const speakingAsId = this.activeSpeakerId();
    // Attribute the optimistic bubble to the seat the SERVER will resolve this
    // message onto — `findActiveUserParticipant`, which honours the impersonation
    // overlay and falls back to the owner user seat when the active-typing id is
    // not itself a user-driven seat (v4 Bug 45, `useSSEStreaming.ts`). Using the
    // bare `activeSpeakerId` here diverged from the persisted row and made the
    // bubble flicker to the wrong author on refetch.
    const optimisticAuthor = findActiveUserParticipant(
      participants,
      speakingAsId,
      this.impersonatingIds(),
    );
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
      participantId: optimisticAuthor?.id ?? speakingAsId ?? null,
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

  /** v4 `copyMessageContent` (`useMessageActions.ts:358-361`). */
  protected onCopy(message: MessageDto): void {
    void navigator.clipboard?.writeText(message.content);
    this.toasts.showSuccess('Message copied to clipboard!');
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

  /** v4 `saveEdit` (`useMessageActions.ts:44-62`) — its only report is the failure. */
  protected async onSaveEdit(event: { id: string; content: string }): Promise<void> {
    this.editingId.set(null);
    const resp = await this.core.dispatch({
      type: 'messageEdit',
      messageId: event.id,
      content: event.content,
    });
    if (resp.type === 'error') {
      this.toasts.showError(resp.data.message || 'Failed to update message');
      return;
    }
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

  /** v4 `generateSwipe` (`useMessageActions.ts:318-332`). */
  protected async onRegenerate(message: MessageDto): Promise<void> {
    const resp = await this.core.dispatch({ type: 'messageSwipe', messageId: message.id });
    if (resp.type === 'error') {
      this.toasts.showError(resp.data.message || 'Failed to generate alternative response');
      return;
    }
    await this.queryClient.invalidateQueries({ queryKey: ['chat', this.chatId()] });
    // v4 `useMessageActions.generateSwipe` (:327) wakes the queue badges after
    // the swipe lands (the regeneration enqueues post-turn jobs).
    notifyQueueChange();
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
    if (resp.type === 'error') {
      this.toasts.showError(resp.data.message || 'Failed to delete message');
      return;
    }
    await this.queryClient.invalidateQueries({ queryKey: ['chat', this.chatId()] });
  }

  protected async onCascadeConfirm(action: MemoryCascadeAction): Promise<void> {
    const pending = this.cascade();
    this.cascade.set(null);
    if (!pending) return;
    const resp = await this.core.dispatch({
      type: 'messageDelete',
      messageId: pending.messageId,
      memoryAction: action,
      skipConfirmation: true,
    });
    if (resp.type === 'error') {
      this.toasts.showError(resp.data.message || 'Failed to delete message');
      return;
    }
    // v4 `completeDeleteWithMemoryAction` (:95-97) reports ONLY when memories
    // went with the message.
    const deleted =
      resp.type === 'messageDelete' && 'memoriesDeleted' in resp.data
        ? (resp.data.memoriesDeleted as number)
        : 0;
    if (deleted > 0) {
      this.toasts.showSuccess(
        `Deleted message and ${deleted} ${deleted === 1 ? 'memory' : 'memories'}`,
      );
    }
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

/**
 * Unwrap the CSS `url('…')` story-background value into the raw URL the backdrop
 * registry reports (v4 `BackdropEntry.url` is a plain URL). Null when unset.
 */
function rawBackdropUrl(cssVar: string | null): string | null {
  if (!cssVar) return null;
  const m = /^url\((['"]?)(.*)\1\)$/.exec(cssVar);
  return m ? m[2] : cssVar;
}
