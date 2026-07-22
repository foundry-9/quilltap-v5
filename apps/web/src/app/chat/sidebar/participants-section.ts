import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import type { ParticipantDetail } from '../../core/core-contract';
import { Icon } from '../../ui/icon';
import {
  getQueuePosition,
  type TurnOrderEntry,
  type TurnSelectionResult,
  type TurnState,
} from '../turn-order';
import { ParticipantCard } from './participant-card';

/**
 * The chat sidebar's default-open section (v4 `ChatSidebar.tsx:721-845`
 * `ParticipantsSection`): the turn meta line, the queue depth, the Pause /
 * Resume button, the cast of {@link ParticipantCard}s in predicted turn order,
 * and — in v4 — the "Add Character" footer.
 *
 * **Add Character is a tier-3 deferral (loud):** v4 gates it on `onAddCharacter`
 * and opens `AddCharacterDialog`, which posts the chat-PUT `addParticipant` bag
 * — a participant-family verb v5's dispatch surface does not carry
 * (`api/salon.rs:1215`). With the prop absent v4 renders exactly what this does:
 * no footer. Nothing is stubbed.
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
            (nudge)="nudge.emit($event)"
            (queue)="queue.emit($event)"
            (dequeue)="dequeue.emit($event)"
            (skip)="skip.emit()"
            (stopStreaming)="stopStreaming.emit()"
            (impersonate)="impersonate.emit($event)"
            (stopImpersonate)="stopImpersonate.emit($event)"
            (regenerateAvatar)="regenerateAvatar.emit($event)"
          />
        }
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

  readonly togglePause = output<void>();
  readonly nudge = output<string>();
  readonly queue = output<string>();
  readonly dequeue = output<string>();
  readonly skip = output<void>();
  readonly stopStreaming = output<void>();
  readonly impersonate = output<string>();
  readonly stopImpersonate = output<string>();
  readonly regenerateAvatar = output<string>();

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
