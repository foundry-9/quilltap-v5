import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import type { ParticipantDetail, ParticipantStatusWire } from '../../core/core-contract';
import { Icon } from '../../ui/icon';
import {
  getQueuePosition,
  type TurnOrderEntry,
  type TurnSelectionResult,
  type TurnState,
} from '../turn-order';
import { ParticipantCard, type ConnectionProfileOption } from './participant-card';

/**
 * The chat sidebar's default-open section (v4 `ChatSidebar.tsx:721-845`
 * `ParticipantsSection`): the turn meta line, the queue depth, the Pause /
 * Resume button, the cast of {@link ParticipantCard}s in predicted turn order,
 * and the "Add Character" footer.
 *
 * The footer is no longer withheld. It was a tier-3 deferral for as long as v5
 * carried no participant-mutation verb at all; P4.9E1A lands
 * `chatAddParticipant`, so the button is here and raises {@link addCharacter},
 * which the Salon answers with the ported `AddCharacterDialog` — the same shape
 * as v4's `onAddCharacter` prop (`ChatSidebar.tsx:823`).
 */
@Component({
  selector: 'qt-participants-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, ParticipantCard],
  template: `
    <div class="qt-chat-sidebar-section qt-chat-sidebar-section-participants">
      @if (turnSelectionResult(); as turn) {
        <div class="qt-chat-sidebar-meta">
          @if (activeCharacterCount() === 0) {
            <span style="color: var(--qt-status-warning-fg)">No characters available</span>
          } @else if (turn.nextSpeakerId === null) {
            @if (turn.cycleComplete) {
              <span style="color: var(--qt-status-success-fg)"
                >All characters have spoken - your turn</span
              >
            } @else {
              <span style="color: var(--qt-status-success-fg)">Your turn to speak</span>
            }
          } @else if (isGenerating()) {
            <span style="color: var(--qt-status-info-fg)">Generating response...</span>
          } @else {
            <span>Waiting for next turn...</span>
          }
        </div>
      }

      @if (turnState().queue.length > 0) {
        <div class="mt-1 qt-chat-sidebar-meta qt-chat-sidebar-queue">
          {{ turnState().queue.length }} in queue
        </div>
      }

      <button
        type="button"
        [class]="'qt-chat-pause-button mt-3 ' + (isPaused() ? 'qt-chat-pause-button-paused' : '')"
        [title]="isPaused() ? 'Resume auto-responses' : 'Pause auto-responses'"
        (click)="togglePause.emit()"
      >
        <qt-icon [name]="isPaused() ? 'play' : 'pause'" class="w-4 h-4" />
        <span>{{ isPaused() ? 'Resume' : 'Pause' }}</span>
      </button>

      <div class="qt-chat-sidebar-cards mt-3">
        @if (sortedParticipants().length === 0) {
          <div class="qt-empty-state py-8">
            <qt-icon name="users" class="qt-empty-state-icon" />
            <p class="qt-empty-state-title">No participants</p>
            <p class="qt-empty-state-description">Add a character to get started</p>
          </div>
        }
        @for (participant of sortedParticipants(); track participant.id) {
          <qt-participant-card
            [participant]="participant"
            [isCurrentTurn]="currentSpeakerId() === participant.id"
            [queuePosition]="queuePosition(participant.id)"
            [isGenerating]="isGenerating() && currentSpeakerId() === participant.id"
            [isUserParticipant]="participant.id === userParticipantId()"
            [canSkip]="canSkip()"
            [turnPosition]="turnEntry(participant.id)?.position ?? null"
            [turnStatus]="turnEntry(participant.id)?.status"
            [isImpersonating]="impersonatingParticipantIds().includes(participant.id)"
            [isActiveTyping]="activeTypingParticipantId() === participant.id"
            [isDangerousChat]="isDangerousChat()"
            [chatId]="chatId()"
            [canWhisper]="canWhisper()"
            [connectionProfiles]="connectionProfiles()"
            [canRemove]="canRemove()"
            (nudge)="nudge.emit($event)"
            (queue)="queue.emit($event)"
            (dequeue)="dequeue.emit($event)"
            (skip)="skip.emit()"
            (stopStreaming)="stopStreaming.emit()"
            (impersonate)="impersonate.emit($event)"
            (stopImpersonate)="stopImpersonate.emit($event)"
            (regenerateAvatar)="regenerateAvatar.emit($event)"
            (whisper)="whisper.emit($event)"
            (connectionProfileChange)="connectionProfileChange.emit($event)"
            (systemPromptChange)="systemPromptChange.emit($event)"
            (rebuildSystemPrompt)="rebuildSystemPrompt.emit($event)"
            (talkativenessChange)="talkativenessChange.emit($event)"
            (statusChange)="statusChange.emit($event)"
            (remove)="remove.emit($event)"
          />
        }
      </div>

      <!-- v4 ChatSidebar.tsx:823-836 — the dashed Add Character footer. -->
      <div class="qt-chat-sidebar-add mt-3">
        <button
          type="button"
          class="w-full py-2 px-4 text-sm font-medium rounded-lg border border-dashed qt-border-default qt-text-secondary hover:qt-bg-surface-alt hover:qt-text transition-colors flex items-center justify-center gap-2"
          (click)="addCharacter.emit()"
        >
          <qt-icon name="plus" class="w-4 h-4" />
          Add Character
        </button>
      </div>
    </div>
  `,
})
export class ParticipantsSection {
  readonly sortedParticipants = input.required<ParticipantDetail[]>();
  readonly turnOrder = input.required<TurnOrderEntry[]>();
  readonly turnState = input.required<TurnState>();
  readonly turnSelectionResult = input<TurnSelectionResult | null>(null);
  readonly isGenerating = input(false);
  readonly isPaused = input(false);
  readonly currentSpeakerId = input<string | null>(null);
  readonly userParticipantId = input<string | null>(null);
  readonly activeCharacterCount = input(0);
  readonly impersonatingParticipantIds = input<string[]>([]);
  readonly activeTypingParticipantId = input<string | null>(null);
  readonly isDangerousChat = input(false);
  readonly chatId = input<string | null>(null);
  /** The user's connection profiles, for each card's Controlled-By select. */
  readonly connectionProfiles = input<ConnectionProfileOption[]>([]);

  readonly togglePause = output<void>();
  readonly nudge = output<string>();
  readonly queue = output<string>();
  readonly dequeue = output<string>();
  readonly skip = output<void>();
  readonly stopStreaming = output<void>();
  readonly impersonate = output<string>();
  readonly stopImpersonate = output<string>();
  readonly regenerateAvatar = output<string>();
  readonly whisper = output<string>();
  /** The footer button — v4 `onAddCharacter` (`ChatSidebar.tsx:823`). */
  readonly addCharacter = output<void>();
  readonly connectionProfileChange = output<{
    participantId: string;
    profileId: string | null;
    controlledBy: 'llm' | 'user';
  }>();
  readonly systemPromptChange = output<{ participantId: string; promptId: string | null }>();
  readonly rebuildSystemPrompt = output<string>();
  readonly talkativenessChange = output<{ participantId: string; value: number }>();
  readonly statusChange = output<{ participantId: string; status: ParticipantStatusWire }>();
  readonly remove = output<string>();

  /**
   * v4 `ChatSidebar.tsx:786` — `canRemove = activeCharacterCount > 1`. The last
   * character standing cannot be removed, and the server enforces the same rule
   * ("Cannot remove the last character from the chat", `participants.ts:500`);
   * withholding the button keeps the operator from meeting that refusal.
   */
  protected readonly canRemove = computed(() => this.activeCharacterCount() > 1);

  /**
   * v4 `ChatSidebar.tsx:722-724,823` — the Whisper button is threaded to the
   * cards only when THREE OR MORE participants are active. Below that there is
   * no one to keep a message from, so v4 withholds the affordance entirely.
   */
  protected readonly canWhisper = computed(
    () => this.sortedParticipants().filter((p) => p.isActive).length >= 3,
  );

  /** v4 `canSkip`: nobody is up next and nothing is generating. */
  protected readonly canSkip = computed(
    () => this.turnSelectionResult()?.nextSpeakerId === null && !this.isGenerating(),
  );

  private readonly orderMap = computed(() => {
    const map = new Map<string, TurnOrderEntry>();
    for (const entry of this.turnOrder()) {
      map.set(entry.participantId, entry);
    }
    return map;
  });

  protected turnEntry(participantId: string): TurnOrderEntry | undefined {
    return this.orderMap().get(participantId);
  }

  protected queuePosition(participantId: string): number {
    return getQueuePosition(this.turnState(), participantId);
  }
}
