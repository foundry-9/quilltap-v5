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

import { ChatComposer, type PendingToolResultChip } from '../../chat/chat-composer';
import {
  LibraryFilePickerModal,
  type LinkedLibraryFile,
} from '../../chat/library-picker/library-file-picker-modal';
import type { RngPendingResult } from '../../chat/rng-dropdown';
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
import { ChatSidebar } from '../../chat/sidebar/chat-sidebar';
import type { ChatSectionState } from '../../chat/sidebar/chat-section';
import type { VisibilityState } from '../../chat/sidebar/visibility-section';
import type { ImageClickEvent } from '../../chat/message-row';
import { ImageModal } from '../../images/image-modal';
import { SaveImageDialog } from '../../images/save-image-dialog';
import { PhotoGalleryModal } from '../../images/photo-gallery-modal';
import { GenerateImageDialog, type GeneratedImage } from '../../images/generate-image-dialog';
import { StandaloneGenerateImageDialog } from '../../images/standalone-generate-image-dialog';
import { MemoryCascadeDialog, type MemoryCascadeAction } from '../../chat/memory-cascade-dialog';
import { ComposeMailDialog, type ComposeMailParticipant } from '../../chat/post-office/compose-mail-dialog';
import { InsertAnnouncementDialog } from '../../chat/post-office/insert-announcement-dialog';
import { WhisperDialog } from '../../chat/post-office/whisper-dialog';
import { AddCharacterDialog } from '../../chat/cast/add-character-dialog';
import { BulkCharacterReplaceModal } from '../../chat/bulk-character-replace-modal';
import { ChatProjectModal } from '../../chat/chat-project-modal';
import { MergeConversationModal } from '../../chat/merge-conversation-modal';
import { ChatToolSettingsModal } from '../../chat/tools/chat-tool-settings-modal';
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
  addToQueue,
  createInitialTurnState,
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
          [turnSelectionResult]="turnSelection()"
          [isGenerating]="busy()"
          [isPaused]="c.isPaused"
          [userParticipantId]="userParticipantId()"
          [respondingParticipantId]="stream()?.respondingParticipantId ?? null"
          [impersonatingParticipantIds]="impersonatingIds()"
          [activeTypingParticipantId]="c.activeTypingParticipantId ?? null"
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

      @if (chatFlash(); as flash) {
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
              (click)="chatFlash.set(null)"
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
        [skipError]="skipError()"
        [nudgeTargetName]="nudgeTargetName()"
        (selectSpeaker)="onSelectSpeaker($event)"
        (skipUserTurn)="onSkipUserTurn()"
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
        [pendingToolResults]="pendingToolResults()"
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
        (mountFileAttached)="onLibraryMountFileAttached($event)"
        (close)="showLibraryPicker.set(false)"
      />
    }

    @if (showAnnouncement() && chatId(); as id) {
      <qt-insert-announcement-dialog
        [chatId]="id"
        [participantCharacterIds]="participantCharacterIds()"
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
        (added)="onCharacterAdded($event)"
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
        (saved)="onToolSettingsSaved()"
        (close)="showToolSettings.set(false)"
      />
    }

    @if (showMerge() && chat(); as c) {
      <qt-merge-conversation-modal
        [targetChatId]="c.id"
        [existingCharacterIds]="castCharacterIds()"
        (merged)="onConversationMerged($event)"
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
        (reattributed)="onMessageReattributed($event)"
        (close)="reattributeTarget.set(null)"
      />
    }

    @if (showProject() && chat(); as c) {
      <qt-chat-project-modal
        [chatId]="c.id"
        [projectId]="c.projectId"
        [projectName]="c.projectName"
        (assigned)="onProjectAssigned($event)"
        (close)="showProject.set(false)"
      />
    }

    @if (showBulkReplace() && chat(); as c) {
      <qt-bulk-character-replace-modal
        [chatId]="c.id"
        [participants]="c.participants"
        [messages]="c.messages"
        (reattributed)="onBulkReattributed($event)"
        (close)="showBulkReplace.set(false)"
      />
    }

    @if (showRename() && chat(); as c) {
      <qt-chat-rename-modal
        [chatId]="c.id"
        [currentTitle]="c.title || ''"
        [isManuallyRenamed]="c.isManuallyRenamed"
        (renamed)="onChatRenamed($event)"
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
  /**
   * v4's toasts have no v5 bus yet — the scriptorium `flash` idiom stands in.
   * Shared by every in-chat action that v4 answers with a toast (the background
   * regeneration below, and the Post Office's delivery notice).
   */
  protected readonly chatFlash = signal<{ kind: 'success' | 'error'; message: string } | null>(
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
    this.chatFlash.set(null);
    try {
      const result = await regenerateChatBackground(this.core, chatId);
      // Both §2 success arms are shown verbatim: "…queued" and "…already in
      // progress" are distinct states the user should be able to tell apart.
      this.chatFlash.set({ kind: 'success', message: result.message });
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
      this.chatFlash.set({
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

  // -------------------------------------------------------------------------
  // The in-chat dialog family (P4.9E3C) — v4 mounts these from `ChatModals.tsx`
  // off `useModalState`; v5 keeps its signal-per-dialog pattern (see the Post
  // Office note below, which weighed the same choice).
  //
  // `LibraryFilePickerModal` — deferred by name here through P4.9E3C — LANDED in
  // P4.9E4B: `qt-library-file-picker-modal`, opened from the composer gutter's
  // file-plus button, which had been ABSENT rather than refusing.
  //
  // ## Tier-3 deferral (LOUD — rendered nowhere, nothing stubbed)
  //
  // **`AllLLMPauseModal`** (v4 `components/chat/AllLLMPauseModal.tsx`, 148 LOC)
  // is NOT ported, because it is unreachable in v4 itself. `ChatModals.tsx:423`
  // mounts it and `SalonView` wires all three of its handlers, but
  // `setAllLLMPauseModalOpen(true)` appears NOWHERE in v4 at `e8a49597` — every
  // occurrence passes `false`. The pause it describes is real and already
  // ported: the chain driver stops the chat itself when the turn count hits a
  // threshold (v4 `turn-orchestrator.service.ts:126` →
  // `services/turn_orchestrator.rs:455`, over the differential-verified
  // `quilltap-core::all_llm_pause`), writing `isPaused` and never telling the
  // client. And `allLLMPauseTurnCount`, which the modal's copy is built from, is
  // in neither app's chat-GET projection.
  //
  // So porting it would mean shipping a dialog with no opener, and adding an
  // opener would be v5 inventing a control v4 does not have. The pure helpers
  // are likewise NOT copied into TypeScript: their only v4 client consumer is
  // `ChatModals.tsx:427`, computing this dead modal's `nextPauseAt`.
  // -------------------------------------------------------------------------

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
    this.chatFlash.set({ kind: 'success', message: 'Tool settings saved' });
    await this.onChatUpdated();
  }

  /** v4 `onMerged` → `fetchChat` (`SalonView.tsx:1594`). */
  protected async onConversationMerged(message: string): Promise<void> {
    this.chatFlash.set({ kind: 'success', message });
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
  protected async onMessageReattributed(message: string): Promise<void> {
    const target = this.reattributeTarget();
    this.reattributeTarget.set(null);
    this.chatFlash.set({ kind: 'success', message });
    await this.onChatUpdated();
    if (!target) return;
    setTimeout(() => {
      document
        .getElementById(`message-${target.id}`)
        ?.scrollIntoView({ behavior: 'smooth', block: 'center' });
    }, 100);
  }

  /** v4 `onSuccess` → `fetchChat` (`ChatModals.tsx:193`); the sentence is v4's. */
  protected async onProjectAssigned(message: string): Promise<void> {
    this.chatFlash.set({ kind: 'success', message });
    await this.onChatUpdated();
  }

  /** v4 `onSuccess` → `fetchChat` (`ChatModals.tsx:265`); the counts are v4's copy. */
  protected async onBulkReattributed(message: string): Promise<void> {
    this.chatFlash.set({ kind: 'success', message });
    await this.onChatUpdated();
  }

  /**
   * v4 `onSuccess` patches its local chat object in place
   * (`ChatModals.tsx:202-206`); v5 holds the chat in a query, so the equivalent
   * is a refetch. The sentence is the dialog's own — v4's toast copy, raised
   * here because the dialog closes before it could be read.
   */
  protected async onChatRenamed(result: { message: string }): Promise<void> {
    this.chatFlash.set({ kind: 'success', message: result.message });
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

  /** A posted announcement is a real message — refetch (v4 `onPosted` → `fetchChat`). */
  /**
   * The picker linked a legacy library file (v4 `ChatModals.tsx:250-261`): push it
   * into the composer's pending-attachment tray so the next send carries it. The
   * message is v4's own toast sentence — v5 has no toasts, so it lands as the
   * chat flash.
   */
  protected onLibraryFileLinked(linked: LinkedLibraryFile): void {
    this.composer()?.addAttachedFile(linked.file);
    this.chatFlash.set({ kind: 'success', message: linked.message });
  }

  /**
   * The picker pinned a document-store file (v4 `ChatModals.tsx:262-266`). There
   * is NO tray hand-off — the Librarian announcement is already in the transcript
   * — so this only refetches the chat, exactly as v4's `fetchChat()` does.
   */
  protected async onLibraryMountFileAttached(message: string): Promise<void> {
    this.chatFlash.set({ kind: 'success', message });
    await this.queryClient.invalidateQueries({ queryKey: ['chat', this.chatId()] });
  }

  protected async onAnnouncementPosted(): Promise<void> {
    this.chatFlash.set({ kind: 'success', message: 'Announcement posted' });
    await this.queryClient.invalidateQueries({ queryKey: ['chat', this.chatId()] });
  }

  /** v4's own success copy, verbatim (`ComposeMailDialog.tsx:143`). */
  protected async onMailPosted(): Promise<void> {
    this.chatFlash.set({
      kind: 'success',
      message: 'Suparṇā has the letter and is already aloft.',
    });
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
    if (!chatId) {
      this.activeSpeakerOverride.set(null);
      return;
    }
    void this.core
      .dispatch({ type: 'chatSetActiveSpeaker', chatId, participantId })
      .then(() => this.queryClient.invalidateQueries({ queryKey: ['chat', chatId] }))
      // Hand authority back to the refetched chat, whether the server took the
      // choice or refused it (it rejects a participant who is not being
      // impersonated). Either way the override must not survive the round trip.
      .finally(() => this.activeSpeakerOverride.set(null));
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
      // v4's chat GET never returns `timelineMode` (see `chat-section.ts`), so
      // this seeds the control once and the section keeps the operator's choice.
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

  /** The Visibility section's slice. The two write-only columns stay absent. */
  protected readonly visibilityState = computed<VisibilityState>(() => {
    const c = this.chat();
    return {
      allowCrossCharacterVaultReads: c?.allowCrossCharacterVaultReads ?? false,
      coreWhisperEnabled: c?.coreWhisperEnabled ?? null,
      coreWhisperInterval: c?.coreWhisperInterval ?? null,
      turnSkippingEnabled: c?.turnSkippingEnabled ?? null,
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

  /** v4 `onCharacterAdded` — refetch the chat, and say who joined. */
  protected async onCharacterAdded(joined: { characterId: string; name: string }): Promise<void> {
    this.chatFlash.set({ kind: 'success', message: `${joined.name} has joined the chat` });
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
      this.chatFlash.set({ kind: 'success', message: 'System prompt rebuilt' });
      await this.onChatUpdated();
    } catch (err) {
      this.chatFlash.set({
        kind: 'error',
        message: err instanceof Error ? err.message : 'Failed to rebuild system prompt',
      });
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
      this.chatFlash.set({
        kind: 'error',
        message: `Cannot remove ${name} while they are generating a response. Please wait for them to finish.`,
      });
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
      this.chatFlash.set({
        kind: 'success',
        message: `${target.name} has been removed from the chat`,
      });
      this.turnState.update((prev) => removeFromQueue(prev, target.participantId));
      this.removeTarget.set(null);
      await this.onChatUpdated();
      const remaining = (this.chat()?.participants ?? []).filter(
        (p) =>
          p.type === 'CHARACTER' && p.isActive && p.id !== target.participantId,
      );
      if (remaining.length === 0) {
        this.chatFlash.set({
          kind: 'error',
          message: 'All characters have been removed. Add a character to continue the conversation.',
        });
      }
    } catch (err) {
      this.chatFlash.set({
        kind: 'error',
        message: err instanceof Error ? err.message : 'Failed to remove character',
      });
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
      if (success) this.chatFlash.set({ kind: 'success', message: success });
      await this.onChatUpdated();
    } catch (err) {
      this.chatFlash.set({ kind: 'error', message: err instanceof Error ? err.message : failure });
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
    if (!participant || participant.controlledBy === 'user') return;
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
    if (!chatId || !this.hasActiveCharacters()) return;
    const resp = await this.core.dispatch({ type: 'chatTurnAction', chatId, action: 'query' });
    if (resp.type !== 'turnAction') return;
    this.applyTurnResponse(resp.data);
    const turn = (resp.data as { turn?: TurnInfo }).turn;
    if (
      turn?.nextSpeakerId &&
      turn.nextSpeakerId !== this.userParticipantId() &&
      turn.nextSpeakerControlledBy !== 'user'
    ) {
      await this.runTurn({ continueMode: true, respondingParticipantId: turn.nextSpeakerId });
    }
  }

  /** v4 `useImpersonation.handleStartImpersonation`. */
  protected async onImpersonate(participantId: string): Promise<void> {
    const chatId = this.chatId();
    if (!chatId) return;
    const data = await this.core.dispatchData({ type: 'chatImpersonate', chatId, participantId });
    this.impersonatingLocal.set(readImpersonatingIds(data, [participantId]));
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
   * v4's sync guard is kept: the chat record overrides only when it carries a
   * NON-EMPTY list (`:39-42`), so a record that omits the key leaves this alone.
   */
  private readonly impersonatingLocal = signal<string[]>([]);
  protected readonly impersonatingIds = computed(() => {
    const fromChat = this.chat()?.impersonatingParticipantIds ?? [];
    return fromChat.length > 0 ? fromChat : this.impersonatingLocal();
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
    const data = await this.core.dispatchData({
      type: 'chatStopImpersonate',
      chatId,
      participantId,
    });
    this.impersonatingLocal.set(readImpersonatingIds(data, []));
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
    } catch (err) {
      this.chatFlash.set({
        kind: 'error',
        message: err instanceof Error ? err.message : 'Failed to assign LLM profile',
      });
      return;
    }
    this.chatFlash.set({ kind: 'success', message: `${name} is now controlled by AI` });
    await this.queryClient.invalidateQueries({ queryKey: ['chat', chatId] });
  }

  /** v4 `handleRegenerateAvatar` — the card's camera button. */
  protected async onRegenerateAvatar(participantId: string): Promise<void> {
    const chatId = this.chatId();
    const characterId = (this.chat()?.participants ?? []).find((p) => p.id === participantId)
      ?.character?.id;
    if (!chatId || !characterId) return;
    await this.core.dispatch({ type: 'chatRegenerateAvatar', chatId, characterId });
    await this.queryClient.invalidateQueries({ queryKey: ['chat', chatId] });
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

/**
 * Unwrap the CSS `url('…')` story-background value into the raw URL the backdrop
 * registry reports (v4 `BackdropEntry.url` is a plain URL). Null when unset.
 */
function rawBackdropUrl(cssVar: string | null): string | null {
  if (!cssVar) return null;
  const m = /^url\((['"]?)(.*)\1\)$/.exec(cssVar);
  return m ? m[2] : cssVar;
}
