import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';

import type { ParticipantDetail, ParticipantStatusWire } from '../../core/core-contract';
import { Avatar } from '../../ui/avatar';
import { normalizeAvatarSrc } from '../../ui/avatar-stack';
import { Icon } from '../../ui/icon';
import { WardrobeDialogService } from '../../wardrobe/wardrobe-dialog.service';
import type { TurnOrderStatus } from '../turn-order';
import { ProviderModelBadge } from './provider-model-badge';

/** v4's sentinel for "the human types for this character" (`ParticipantCard.tsx:25`). */
export const USER_IMPERSONATION_VALUE = '__user__';

/** One selectable backend in the Controlled-By select (v4 `ConnectionProfileOption`). */
export interface ConnectionProfileOption {
  id: string;
  name: string;
  provider?: string;
  modelName?: string;
}

/**
 * One participant in the chat sidebar's cast list (v4
 * `components/chat/ParticipantCard.tsx`): avatar with status overlay + the
 * avatar tool row, name/badges/title, the LLM provider badge, the turn-position
 * badge, and the nudge/queue/dequeue (or Queue+Skip) action row.
 *
 * **The card is now v4's full configuration.** P4.9H1 shipped it in v4's
 * no-callback shape, because every remaining control rode a participant-mutation
 * verb v5's dispatch surface did not carry; P4.9E1A lands
 * `chatUpdateParticipant` / `chatRemoveParticipant` / `chatRebuildSystemPrompt`,
 * so the connection-profile select, the system-prompt select and its rebuild
 * button, the talkativeness slider, the four-state status select and Remove are
 * all here (P4.9E1B). They slotted into v4's own positions; nothing moved.
 *
 * Two of v4's gates are worth naming, because they are what decides whether a
 * control renders at all:
 *
 *  - the system-prompt row is for LLM-controlled, non-user characters only, and
 *    its SELECT appears only when the character actually has named prompts
 *    (`ParticipantCard.tsx:439-441`) — the rebuild button appears either way;
 *  - Remove is offered for non-user characters only, and only when
 *    {@link canRemove} (v4: more than one character present).
 *
 * The talkativeness slider keeps v4's local-then-report shape: the thumb tracks
 * the drag immediately and the WRITE is debounced by the Salon (v4
 * `useChatControls:613-635` — 400 ms per participant), so a drag makes one
 * request on release, not one per step.
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
            <!-- P4.D64 (v4 ParticipantCard.tsx:386-393): AFTER Absent, and both
                 can show at once — an archived seat is normally absent too.
                 REUSES qt-badge-absent deliberately, which is v4's choice. -->
            @if (participant().character?.archivedAt) {
              <span
                class="qt-badge-absent text-xs"
                title="Resting in the archive — rehydrate them from their character page to let them speak again"
              >
                Archived
              </span>
            }
          </div>

          @if (title(); as t) {
            <div class="qt-participant-card-status italic truncate">{{ t }}</div>
          }

          <!-- Connection profile — every character, including the "You" seat, so
               one control flips a participant between the human and an LLM
               (v4 :392-431). -->
          <div class="mt-1">
            <select
              class="qt-select qt-select-sm w-full"
              title="Connection profile"
              [attr.aria-label]="'Connection profile for ' + name()"
              [value]="connectionProfileValue()"
              (change)="onProfileChange($any($event.target).value)"
            >
              <option value="">Select a provider...</option>
              <option [value]="USER_IMPERSONATION_VALUE">User (you type)</option>
              @for (profile of connectionProfiles(); track profile.id) {
                <option [value]="profile.id" [selected]="profile.id === connectionProfileValue()">
                  {{ profileLabel(profile) }}
                </option>
              }
            </select>
          </div>

          <!-- System prompt: the select only when the character has named
               prompts; the rebuild button always (v4 :433-470). -->
          @if (showSystemPromptRow()) {
            <div class="mt-1 flex items-center gap-1">
              @if (systemPrompts().length > 0) {
                <select
                  class="qt-select qt-select-sm flex-1 min-w-0"
                  title="System prompt"
                  [attr.aria-label]="'System prompt for ' + name()"
                  [value]="participant().selectedSystemPromptId || ''"
                  (change)="onSystemPromptChange($any($event.target).value)"
                >
                  <option value="">Use default prompt</option>
                  @for (prompt of systemPrompts(); track prompt.id) {
                    <option
                      [value]="prompt.id"
                      [selected]="prompt.id === participant().selectedSystemPromptId"
                    >
                      {{ prompt.name }}{{ prompt.isDefault ? ' (Default)' : '' }}
                    </option>
                  }
                </select>
              }
              <button
                type="button"
                class="qt-button qt-button-secondary qt-button-sm py-1 px-1.5 flex-shrink-0"
                title="Rebuild system prompt from latest character data"
                [attr.aria-label]="'Rebuild system prompt for ' + name()"
                (click)="rebuildSystemPrompt.emit(participant().id)"
              >
                <qt-icon name="refresh" class="w-3.5 h-3.5" />
              </button>
            </div>
          }

          <!-- Talkativeness: live for a character, greyed out for the persona
               (v4 :472-506). -->
          @if (!isUserParticipant()) {
            <div class="mt-2">
              <div class="flex items-center justify-between qt-text-xs mb-1">
                <span>Talkativeness</span>
                <span>{{ talkativenessPercent() }}%</span>
              </div>
              <input
                type="range"
                min="0.1"
                max="1"
                step="0.1"
                class="qt-input w-full h-1 rounded-lg appearance-none cursor-pointer accent-primary"
                [attr.aria-label]="'Talkativeness for ' + name()"
                [value]="localTalkativeness()"
                (input)="onTalkativenessInput($any($event.target).value)"
              />
            </div>
          } @else {
            <div class="mt-2 opacity-50">
              <div class="flex items-center justify-between qt-text-xs mb-1">
                <span>Talkativeness</span>
                <span>N/A</span>
              </div>
              <input
                type="range"
                min="0.1"
                max="1"
                step="0.1"
                value="0.5"
                disabled
                class="qt-input w-full h-1 rounded-lg appearance-none cursor-not-allowed"
              />
            </div>
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

        <!-- The four-state status select that replaced v4's eye toggle
             (:568-582). v4 renders only three of the four: "removed" is a
             server-side state, not something the operator picks here. -->
        <select
          class="qt-select qt-select-sm qt-participant-status-select py-1 px-1.5 text-xs"
          [title]="'Status for ' + name() + ': ' + status()"
          [attr.aria-label]="'Participation status for ' + name()"
          [value]="status()"
          (change)="onStatusChange($any($event.target).value)"
        >
          <option value="active" [selected]="status() === 'active'">Active</option>
          <option value="silent" [selected]="status() === 'silent'">Silent</option>
          <option value="absent" [selected]="status() === 'absent'">Absent</option>
        </select>

        <!-- Remove — characters only, never the operator's own seat, and only
             while another character remains (v4 :584-597). -->
        @if (!isUserParticipant() && canRemove()) {
          <button
            type="button"
            class="qt-button qt-button-destructive qt-button-sm py-1.5 px-2 disabled:opacity-50 disabled:cursor-not-allowed"
            [disabled]="isGenerating()"
            [title]="'Remove ' + name() + ' from chat'"
            [attr.aria-label]="'Remove ' + name() + ' from chat'"
            (click)="remove.emit(participant().id)"
          >
            <qt-icon name="close" class="w-3.5 h-3.5" />
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
  /** The user's connection profiles, for the Controlled-By select (v4 prop). */
  readonly connectionProfiles = input<ConnectionProfileOption[]>([]);
  /**
   * Whether Remove is offered at all. v4 computes it in the section as "more
   * than one character present" (`ChatSidebar.tsx:786`) and gates the button on
   * it — with one character left, removing is not an option to be disabled, it
   * is an option that does not exist.
   */
  readonly canRemove = input(true);

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
  /**
   * The Controlled-By select (v4 `onConnectionProfileChange`): choosing the
   * human sends `{profileId: null, controlledBy: 'user'}`, an LLM profile sends
   * `{profileId, controlledBy: 'llm'}`. The Salon turns that into the wire's
   * absent-vs-present `connectionProfileId`.
   */
  readonly connectionProfileChange = output<{
    participantId: string;
    profileId: string | null;
    controlledBy: 'llm' | 'user';
  }>();
  /** The named-prompt select — `null` means "use the default prompt". */
  readonly systemPromptChange = output<{ participantId: string; promptId: string | null }>();
  /** Force-recompile this participant's cached identity stack. */
  readonly rebuildSystemPrompt = output<string>();
  /** Slider moves — the Salon debounces the write (v4 400 ms per participant). */
  readonly talkativenessChange = output<{ participantId: string; value: number }>();
  /** The four-state status select. */
  readonly statusChange = output<{ participantId: string; status: ParticipantStatusWire }>();
  /** Remove this character from the chat (v4 `onRemove`). */
  readonly remove = output<string>();

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

  protected readonly USER_IMPERSONATION_VALUE = USER_IMPERSONATION_VALUE;

  /** The character's named prompts, when the projection carries any (v4 `:441`). */
  protected readonly systemPrompts = computed(
    () => this.participant().character?.systemPrompts ?? [],
  );

  /**
   * v4 `:438` — the system-prompt row is for LLM-driven characters only. The
   * operator's own seat and any user-controlled character have no identity stack
   * to pick or rebuild.
   */
  protected readonly showSystemPromptRow = computed(
    () => !this.isUserParticipant() && !this.isUserControlledCharacter(),
  );

  /** v4 `connectionProfileValue` (`:376-378`). */
  protected readonly connectionProfileValue = computed(() =>
    this.participant().controlledBy === 'user'
      ? USER_IMPERSONATION_VALUE
      : (this.participant().connectionProfile?.id ?? ''),
  );

  /**
   * The slider's live position (v4 `localTalkativeness`, seeded from the
   * character and updated on every drag step so the thumb never lags the write).
   * v4 seeds from `character.talkativeness`; the per-chat override, when the
   * participant carries one, wins — that is what the slider is editing.
   */
  private readonly localOverride = signal<number | null>(null);
  protected readonly localTalkativeness = computed(
    () =>
      this.localOverride() ??
      this.participant().talkativeness ??
      this.participant().character?.talkativeness ??
      0.5,
  );
  protected readonly talkativenessPercent = computed(() =>
    (this.localTalkativeness() * 100).toFixed(0),
  );

  protected profileLabel(profile: ConnectionProfileOption): string {
    // v4 `:415-420`: "name — model", collapsing to one when they are the same.
    const model = profile.modelName?.trim();
    const label = model && model !== profile.name ? `${profile.name} — ${model}` : profile.name;
    return label || (model ?? '');
  }

  /** v4 `handleProfileChange` (`:365-373`). */
  protected onProfileChange(value: string): void {
    const participantId = this.participant().id;
    if (value === USER_IMPERSONATION_VALUE) {
      this.connectionProfileChange.emit({ participantId, profileId: null, controlledBy: 'user' });
    } else {
      this.connectionProfileChange.emit({
        participantId,
        profileId: value || null,
        controlledBy: 'llm',
      });
    }
  }

  /** v4 `handleSystemPromptChangeEvent` — an empty option means "default". */
  protected onSystemPromptChange(value: string): void {
    this.systemPromptChange.emit({
      participantId: this.participant().id,
      promptId: value || null,
    });
  }

  /** v4 `handleTalkativenessChange` — move the thumb, then report. */
  protected onTalkativenessInput(value: string): void {
    const parsed = Number.parseFloat(value);
    if (Number.isNaN(parsed)) return;
    this.localOverride.set(parsed);
    this.talkativenessChange.emit({ participantId: this.participant().id, value: parsed });
  }

  /** v4 `handleStatusChange`. */
  protected onStatusChange(value: string): void {
    this.statusChange.emit({
      participantId: this.participant().id,
      status: value as ParticipantStatusWire,
    });
  }

  /** v4 `wardrobeDialog.open({characterId, chatId?})` (`ParticipantCard.tsx:337`). */
  protected openWardrobe(characterId: string): void {
    const chatId = this.chatId();
    this.wardrobeDialog.open(chatId ? { characterId, chatId } : { characterId });
  }
}
