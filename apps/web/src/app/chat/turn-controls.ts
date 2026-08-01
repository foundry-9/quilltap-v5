import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import { Icon } from '../ui/icon';
import { SpeakerSelector, type ControlledCharacter } from './speaker-selector';

/**
 * The Salon turn-controls bar above the composer (v4 SalonView's speaker
 * selector + user-turn banner). It is purely presentational — the parent
 * (`SalonConversation`) owns the dispatches.
 *
 * The Pause/Resume BUTTON used to live here as a recorded divergence, v5 having
 * no chat sidebar to host v4's `qt-chat-pause-button`. P4.9H1 ported the
 * sidebar, so the button went home (its strip and its Participants drawer, v4's
 * two homes for it) and only the paused NOTICE stays — that notice is v5's own
 * affordance, not something v4 puts in the sidebar.
 *
 *   - The **Speaking-As** selector (v4 `SpeakerSelector`), shown when the user
 *     controls two or more characters.
 *   - The **user-turn banner** (v4 SalonView ~1394–1455): when it is a
 *     user-controlled participant's turn, prompt to type or Skip; when everyone
 *     else has passed, the must-speak copy with no Skip button.
 *   - The **paused-state notice** (the Pause/Resume button itself lives in the
 *     chat sidebar, v4's home for it).
 *   - **Nudge** (v4 `handleNudge`): summon the next LLM speaker out of turn.
 */
@Component({
  selector: 'qt-turn-controls',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, SpeakerSelector],
  template: `
    <div class="qt-chat-turn-controls border-t qt-border-default">
      @if (isPaused()) {
        <div class="qt-chat-paused-banner px-4 py-2 text-sm qt-text-secondary">
          Auto-responses are paused — the next character won't speak until you resume.
        </div>
      }

      @if (userTurnName(); as name) {
        <div class="qt-chat-user-turn-banner flex items-center justify-between px-4 py-2 text-sm">
          <span class="qt-text-secondary">{{ bannerText() }}</span>
          @if (!mustSpeak()) {
            <button
              type="button"
              class="qt-button-secondary px-3 py-1 rounded text-sm"
              [disabled]="disabled()"
              (click)="skipUserTurn.emit()"
            >
              Skip
            </button>
          }
        </div>
      }

      <div class="qt-chat-turn-controls-row flex items-center gap-2 px-4 py-2">
        @if (controlledCharacters().length >= 2) {
          <qt-speaker-selector
            [characters]="controlledCharacters()"
            [activeParticipantId]="activeSpeakerId()"
            [disabled]="disabled()"
            (select)="selectSpeaker.emit($event)"
          />
        }

        <span class="flex-1"></span>

        @if (nudgeTargetName(); as target) {
          <button
            type="button"
            class="qt-button-secondary inline-flex items-center gap-1.5 px-3 py-1 rounded text-sm"
            title="Nudge the next character to respond"
            [disabled]="disabled()"
            (click)="nudge.emit()"
          >
            <qt-icon name="megaphone" class="w-4 h-4" />
            <span>Nudge {{ target }}</span>
          </button>
        }
      </div>
    </div>
  `,
})
export class TurnControls {
  /** User-controlled characters the speaker selector offers (≥2 → shown). */
  readonly controlledCharacters = input.required<ControlledCharacter[]>();
  readonly activeSpeakerId = input<string | null>(null);
  /** Streaming/awaiting in flight — disables the interactive controls. */
  readonly disabled = input(false);
  /** Drives the paused notice (the button moved to the sidebar with P4.9H1). */
  readonly isPaused = input(false);
  /** The name whose (user-controlled) turn it is, or null to hide the banner. */
  readonly userTurnName = input<string | null>(null);
  /** When true, the responder must speak (everyone else passed) — no Skip. */
  readonly mustSpeak = input(false);
  /** A refusal message from a rejected skip (v4's exact copy), or null. */
  /** The next LLM speaker's name, or null to hide the Nudge button. */
  readonly nudgeTargetName = input<string | null>(null);

  readonly selectSpeaker = output<string>();
  readonly skipUserTurn = output<void>();
  readonly nudge = output<void>();

  protected readonly bannerText = computed(() => {
    const name = this.userTurnName() ?? 'this character';
    return this.mustSpeak()
      ? `Everyone else has passed — it falls to ${name} to say something.`
      : `${name}'s turn — type as them, or skip to let someone else respond.`;
  });
}
