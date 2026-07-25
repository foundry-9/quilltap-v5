import { ChangeDetectionStrategy, Component, computed, inject, input, output } from '@angular/core';

import type { ParticipantDetail } from '../../core/core-contract';
import { Avatar } from '../../ui/avatar';
import { normalizeAvatarSrc } from '../../ui/avatar-stack';
import { Icon } from '../../ui/icon';
import { WardrobeDialogService } from '../../wardrobe/wardrobe-dialog.service';
import type { TurnOrderStatus } from '../turn-order';
import { ProviderModelBadge } from './provider-model-badge';

/**
 * One participant in the chat sidebar's cast list (v4
 * `components/chat/ParticipantCard.tsx`): avatar with status overlay + the
 * avatar tool row, name/badges/title, the LLM provider badge, the turn-position
 * badge, and the nudge/queue/dequeue (or Queue+Skip) action row.
 *
 * **The port is v4's card in its no-callback configuration.** Every control v4
 * gates on an optional prop is simply not fed here, because the server verbs
 * behind them are unported (P4.9H1 tier-3 deferrals, enumerated in the lane
 * record): the connection-profile select, the system-prompt select + rebuild
 * button, the talkativeness slider, the four-state status select, and Remove all
 * ride participant-mutation verbs v5's dispatch surface does not carry
 * (`api/salon.rs:1215` names `updateParticipant` / `addParticipant` /
 * `removeParticipantId` as chat-PUT deferrals). v4 renders exactly this shape
 * when those props are absent, so the card is a faithful subset, not a redesign
 * — when the verbs land, the controls slot back in without moving anything.
 *
 * (P4.9H1's note listed Whisper among those deferrals. It does not belong there:
 * v4's whisper posts the ORDINARY chat send with `targetParticipantIds`, which
 * v5 has always had. P4.9E2B wired it.)
 *
 * Wired here: nudge / queue / dequeue (`chatTurnAction`), the user seat's Skip,
 * Stop-generating, impersonate / stop-impersonate (`chatImpersonate` /
 * `chatStopImpersonate`), regenerate avatar (`chatRegenerateAvatar`), Whisper
 * (the chat-send spine, via {@link WhisperDialog}), and the wardrobe dialog (the
 * global `WardrobeDialogService`).
 */
@Component({
  selector: 'qt-participant-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Avatar, Icon, ProviderModelBadge],
  template: `
    <div [class]="cardClass() + ' participant-card'">
      <!-- Position badge — every active participant with a position (v4 :277). -->
      @if (turnPosition() != null && turnPosition()! > 0) {
        <div [class]="'qt-participant-position-badge ' + positionBadgeClass()" data-testid="position-badge">
          {{ turnPosition() }}
        </div>
      }

      <!-- Queue-position badge — the fallback when no turnPosition (v4 :284). -->
      @if (turnPosition() == null && queuePosition() > 0) {
        <div class="qt-participant-queue-badge absolute -top-2 -right-2 w-6 h-6 qt-shadow-md">
          {{ queuePosition() }}
        </div>
      }

      @if (isCurrentTurn()) {
        <div class="qt-participant-turn-dot"></div>
      }

      <div class="qt-participant-card-header">
        <div class="flex-shrink-0 flex flex-col items-center gap-1">
          <div class="relative">
            <qt-avatar [name]="name()" [src]="avatarSrc()" size="md" [active]="isCurrentTurn()" />
            @if (status() === 'silent') {
              <div
                class="qt-participant-status-overlay qt-participant-status-overlay-silent"
                title="Silent"
              >
                <qt-icon name="ban" class="w-3 h-3" />
              </div>
            }
            @if (status() === 'absent') {
              <div
                class="qt-participant-status-overlay qt-participant-status-overlay-absent"
                title="Absent"
              >
                <qt-icon name="log-out" class="w-3 h-3" />
              </div>
            }
          </div>
          <div class="qt-participant-avatar-tools">
            <button
              type="button"
              class="qt-participant-avatar-tool-button"
              [title]="'Regenerate avatar for ' + name()"
              [attr.aria-label]="'Regenerate avatar for ' + name()"
              (click)="regenerateAvatar.emit(participant().id); $event.stopPropagation()"
            >
              <qt-icon name="camera" class="w-3.5 h-3.5" />
            </button>
            @if (characterId(); as cid) {
              <button
                type="button"
                class="qt-participant-avatar-tool-button"
                [title]="'Open ' + name() + '’s wardrobe'"
                [attr.aria-label]="'Open ' + name() + '’s wardrobe'"
                (click)="openWardrobe(cid)"
              >
                <qt-icon name="wardrobe" class="w-3.5 h-3.5" />
              </button>
            }
          </div>
        </div>

        <div class="qt-participant-card-info">
          <div class="flex items-center gap-2 flex-wrap">
            <span class="qt-participant-card-name">{{ name() }}</span>
            @if (isUserParticipant() || isImpersonating()) {
              <span [class]="'text-xs ' + (isImpersonating() ? 'qt-badge-info' : 'qt-badge-secondary')">
                {{ isImpersonating() ? (isActiveTyping() ? 'Speaking as' : 'You') : 'You' }}
              </span>
            }
            @if (!isUserParticipant() && !isImpersonating() && participant().controlledBy === 'llm') {
              <span class="qt-badge-secondary text-xs opacity-60">AI</span>
              <qt-provider-model-badge
                [provider]="participant().connectionProfile?.provider"
                [modelName]="participant().connectionProfile?.modelName"
                size="sm"
              />
            }
            @if (status() === 'silent') {
              <span class="qt-badge-silent text-xs">Silent</span>
            }
            @if (status() === 'absent') {
              <span class="qt-badge-absent text-xs">Absent</span>
            }
          </div>

          @if (title(); as t) {
            <div class="qt-participant-card-status italic truncate">{{ t }}</div>
          }
        </div>
      </div>

      <div class="qt-participant-card-actions">
        @if (turnStatus() === 'generating') {
          <button
            type="button"
            class="flex-1 qt-button qt-button-sm qt-participant-stop-button"
            title="Stop generating"
            aria-label="Stop generating"
            (click)="stopStreaming.emit()"
          >
            <qt-icon name="stop" class="w-3.5 h-3.5 mr-1" />
            Stop
          </button>
        } @else if (isUserParticipant()) {
          <button
            type="button"
            [class]="
              'flex-1 ' +
              (queuePosition() > 0
                ? 'qt-badge-info hover:qt-bg-info/20'
                : 'qt-button qt-button-secondary qt-button-sm') +
              ' disabled:opacity-50 disabled:cursor-not-allowed'
            "
            [disabled]="actionDisabled()"
            (click)="onActionClick()"
          >
            {{ queuePosition() > 0 ? 'Dequeue' : 'Queue' }}
          </button>
          <button
            type="button"
            class="flex-1 qt-button qt-button-sm qt-chat-continue-button disabled:opacity-50 disabled:cursor-not-allowed"
            [disabled]="isGenerating() || !canSkip()"
            [title]="
              canSkip() ? 'Skip your turn and let a character respond' : 'It’s not your turn to skip'
            "
            (click)="skip.emit()"
          >
            Skip
          </button>
        } @else {
          <button
            type="button"
            [class]="
              'flex-1 ' +
              (queuePosition() > 0
                ? 'qt-badge-info hover:qt-bg-info/20'
                : isCurrentTurn()
                  ? 'qt-participant-turn-indicator cursor-default'
                  : 'qt-button qt-button-secondary qt-button-sm') +
              ' disabled:opacity-50 disabled:cursor-not-allowed'
            "
            [disabled]="actionDisabled()"
            (click)="onActionClick()"
          >
            {{ actionLabel() }}
          </button>
        }

        @if (!isUserParticipant()) {
          <button
            type="button"
            [class]="
              'qt-button qt-button-sm py-1.5 px-2 ' +
              (isImpersonating() ? 'qt-button-secondary' : 'qt-button-primary') +
              ' disabled:opacity-50 disabled:cursor-not-allowed'
            "
            [disabled]="isGenerating()"
            [title]="isImpersonating() ? 'Stop speaking as ' + name() : 'Speak as ' + name()"
            (click)="
              isImpersonating()
                ? stopImpersonate.emit(participant().id)
                : impersonate.emit(participant().id)
            "
          >
            <qt-icon [name]="isImpersonating() ? 'log-out' : 'user'" class="w-3.5 h-3.5" />
          </button>
        }

        <!-- Whisper — non-user participants only, and only when the section
             feeds the callback (v4 gates it on three or more active
             participants: with two in the room, everything is already private). -->
        @if (canWhisper() && !isUserParticipant()) {
          <button
            type="button"
            class="qt-button qt-button-sm py-1.5 px-2 qt-button-secondary"
            [title]="'Whisper to ' + name()"
            [attr.aria-label]="'Whisper to ' + name()"
            (click)="whisper.emit(participant().id)"
          >
            <qt-icon name="chat" class="w-3.5 h-3.5" />
          </button>
        }
      </div>
    </div>
  `,
})
export class ParticipantCard {
  private readonly wardrobeDialog = inject(WardrobeDialogService);

  readonly participant = input.required<ParticipantDetail>();
  readonly isCurrentTurn = input(false);
  /** 0 = not queued, 1+ = position in the manual queue. */
  readonly queuePosition = input(0);
  readonly isGenerating = input(false);
  readonly isUserParticipant = input(false);
  /** The user may skip (the next speaker is nobody and nothing is generating). */
  readonly canSkip = input(false);
  readonly turnPosition = input<number | null>(null);
  readonly turnStatus = input<TurnOrderStatus | undefined>(undefined);
  readonly isImpersonating = input(false);
  readonly isActiveTyping = input(false);
  /** The chat is Concierge-flagged — tints the card (v4 `isDangerousChat`). */
  readonly isDangerousChat = input(false);
  /** The chat id, passed to the wardrobe dialog so it can show "wearing now". */
  readonly chatId = input<string | null>(null);
  /**
   * Whether the Whisper affordance is offered at all. v4 threads `onWhisper`
   * only when the chat has three or more ACTIVE participants
   * (`ChatSidebar.tsx:823`) and renders the button only when the callback is
   * present — a signal input is v5's way of saying the same thing.
   */
  readonly canWhisper = input(false);

  readonly nudge = output<string>();
  readonly queue = output<string>();
  readonly dequeue = output<string>();
  readonly skip = output<void>();
  readonly stopStreaming = output<void>();
  readonly impersonate = output<string>();
  readonly stopImpersonate = output<string>();
  readonly regenerateAvatar = output<string>();
  /** Open the Whisper dialog against this participant (v4 `onWhisper`). */
  readonly whisper = output<string>();

  protected readonly name = computed(() => this.participant().character?.name ?? 'Unknown');
  protected readonly title = computed(() => this.participant().character?.title ?? null);
  protected readonly characterId = computed(() => this.participant().character?.id ?? null);
  protected readonly avatarSrc = computed(() =>
    normalizeAvatarSrc(this.participant().character?.avatarUrl),
  );
  protected readonly status = computed(() => this.participant().status || 'active');
  /** v4 `isInactive` — drives the muted card class. */
  protected readonly isInactive = computed(
    () => this.turnStatus() === 'inactive' || this.turnStatus() === 'absent',
  );
  /** Only an LLM-controlled character can be NUDGED; a user seat is queued (v4). */
  private readonly isUserControlledCharacter = computed(
    () => this.participant().controlledBy === 'user',
  );

  /** v4 `getCardClass()`. */
  protected readonly cardClass = computed(() => {
    const danger = this.isDangerousChat() ? ' qt-participant-card-dangerous' : '';
    if (this.isInactive()) return 'qt-participant-card-inactive' + danger;
    if (this.status() === 'silent') {
      return (
        (this.isCurrentTurn()
          ? 'qt-participant-card-active qt-participant-card-silent'
          : 'qt-participant-card qt-participant-card-silent') + danger
      );
    }
    if (this.isCurrentTurn()) return 'qt-participant-card-active' + danger;
    return 'qt-participant-card' + danger;
  });

  /** v4 `getPositionBadgeClass()`. */
  protected readonly positionBadgeClass = computed(() => {
    switch (this.turnStatus()) {
      case 'generating':
        return 'qt-participant-position-generating';
      case 'next':
        return 'qt-participant-position-next';
      case 'queued':
        return 'qt-participant-position-queued';
      case 'eligible':
        return 'qt-participant-position-eligible';
      case 'user-turn':
        return 'qt-participant-position-user-turn';
      case 'spoken':
        return 'qt-participant-position-spoken';
      default:
        return '';
    }
  });

  /** v4 `getActionButtonLabel()`. */
  protected readonly actionLabel = computed(() => {
    if (this.queuePosition() > 0) return 'Dequeue';
    if (this.isGenerating() && this.isCurrentTurn()) return 'Speaking...';
    if (this.isGenerating()) return 'Queue';
    if (this.isCurrentTurn()) return this.isUserControlledCharacter() ? 'Queue' : 'Nudge';
    return this.isUserControlledCharacter() ? 'Queue' : 'Nudge';
  });

  /** v4 `isActionDisabled` — only while THIS participant is generating. */
  protected readonly actionDisabled = computed(() => this.isGenerating() && this.isCurrentTurn());

  /** v4 `handleActionClick()`. */
  protected onActionClick(): void {
    const id = this.participant().id;
    if (this.queuePosition() > 0) {
      this.dequeue.emit(id);
    } else if (this.isGenerating()) {
      this.queue.emit(id);
    } else if (!this.isUserControlledCharacter()) {
      this.nudge.emit(id);
    } else {
      this.queue.emit(id);
    }
  }

  /** v4 `wardrobeDialog.open({characterId, chatId?})` (`ParticipantCard.tsx:337`). */
  protected openWardrobe(characterId: string): void {
    const chatId = this.chatId();
    this.wardrobeDialog.open(chatId ? { characterId, chatId } : { characterId });
  }
}
