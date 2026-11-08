import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import type { ImpersonationVoiceMode } from '../core/core-contract';
import { Icon } from '../ui/icon';
import { Tooltip } from '../ui/tooltip';
import { injectImagesHidden } from './hidden-image/images-hidden';

/** An armed In Their Own Words mode — what a send from this seat will do. */
export type VoiceRehearsalCue = Exclude<ImpersonationVoiceMode, 'off'>;

/**
 * The one wording of the cue, shared by the portrait and the Send button so
 * the two can never describe different behaviour (v4
 * `SpeakingAsAvatar.tsx:21-39`, `07b8f0209` — this file is its v4 home too).
 */
export function voiceRehearsalTitle(
  cue: VoiceRehearsalCue,
  name: string,
  where: 'portrait' | 'send',
): string {
  if (cue === 'always') {
    return where === 'portrait'
      ? `Speaking as ${name} — your draft goes to ${name} to say in their own words first`
      : `Sends your draft to ${name} to say in their own words first`;
  }
  return where === 'portrait'
    ? `Speaking as ${name} — your draft opens for review; send it as written or have ${name} restate it`
    : `Opens your draft for review — send it as written or have ${name} restate it`;
}

/**
 * The Deus Ex Machina toggle seated on the portrait (v4 `SpeakingAsAvatar.tsx`,
 * `db6190b44` / `df8d4d70a`). The composer owns the armed state; this only
 * shows and flips it. v4's `onToggle` callback is the component's
 * `toggleEstablishedFact` output.
 */
export interface EstablishedFactToggle {
  armed: boolean;
  disabled?: boolean;
}

/** v4's two toggle tooltips, by armed state (`SpeakingAsAvatar.tsx`, `df8d4d70a`). */
export const ESTABLISHED_FACT_TOGGLE_TIP = {
  armed:
    'Deus Ex Machina is armed — your next line descends from the rafters as established fact. Click to stand it down.',
  disarmed: 'Deus Ex Machina — post your next line as established fact (or type /deus)',
} as const;

/**
 * SpeakingAsAvatar — a persistent cue, seated inside the composer directly to
 * the left of the action-button cluster, of the character whose voice a typed
 * message will carry: the human's active "Speaking As" seat, resolved the same
 * way the server attributes the message (`findActiveUserParticipant`,
 * impersonation-overlay aware; see v4 Bug 45 / Bug 46).
 *
 * It stretches to the full height of the composer row (4:5 portrait) and renders
 * at full brightness when the human may type, dimming to near-dark while a reply
 * is in flight — so a glance tells the operator both *who* they are speaking as
 * and *whether* the floor is theirs. Client port of v4
 * `app/salon/[id]/components/SpeakingAsAvatar.tsx` (`1bed814f`); the composer's
 * own wrapper owns the responsive show/hide.
 */
@Component({
  selector: 'qt-speaking-as-avatar',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, Tooltip],
  template: `
    <!-- v4 df8d4d70a: the OUTER box carries the portrait's 4:5 sizing as the
         composer's flex item ("a nested aspect box would measure to nothing");
         the tooltip anchor and the portrait fill it, and the toggle floats over
         its corner. "A stretched aspect box takes its width from its content,
         so a bare initial would shrink it to a sliver — hence the floor when
         there is no picture." -->
    <div
      class="relative flex self-stretch aspect-[4/5] max-h-40 flex-shrink-0"
      [class.min-w-24]="!avatarSrc()"
    >
      <qt-tooltip [content]="titleText()" class="w-full h-full">
        <div
          class="qt-speaking-as-avatar w-full h-full"
          [class.qt-speaking-as-avatar-dim]="!canType()"
          [attr.aria-label]="ariaLabel()"
        >
          @if (avatarSrc(); as src) {
            <img [src]="src" [alt]="name()" class="w-full h-full object-cover" />
          } @else {
            <span class="font-bold qt-text-secondary text-lg">{{ initial() }}</span>
          }
          @if (voiceRehearsal()) {
            <span class="qt-speaking-as-avatar-voice-badge" aria-hidden="true">
              <qt-icon name="thinking" class="w-3 h-3" />
            </span>
          }
        </div>
      </qt-tooltip>
      <!-- Outside the dimmed portrait, "so it stays legible and usable while a
           reply streams". -->
      @if (establishedFact(); as fact) {
        <div class="absolute bottom-1 left-1">
          <qt-tooltip
            [content]="fact.armed ? FACT_TIP.armed : FACT_TIP.disarmed"
          >
            <!-- Keep the caret in the editor: arming is a mode, not a destination. -->
            <button
              type="button"
              class="qt-chat-established-fact-toggle"
              aria-label="Deus Ex Machina"
              [attr.aria-pressed]="fact.armed"
              [disabled]="!!fact.disabled"
              (mousedown)="$event.preventDefault()"
              (click)="toggleEstablishedFact.emit()"
            >
              <qt-icon name="deus-ex-machina" class="w-4 h-4" />
            </button>
          </qt-tooltip>
        </div>
      }
    </div>
  `,
})
export class SpeakingAsAvatar {
  /** The character the human is currently speaking as. */
  readonly name = input.required<string>();
  /** The resolved portrait src (explicit URL → default image filepath), or null. */
  readonly avatarUrl = input<string | null>(null);
  /** Bright when the human may type now; dimmed to near-dark while a reply streams. */
  readonly canType = input(false);
  /**
   * The armed In Their Own Words mode for this seat, or null — a typed line
   * opens the review dialog before it posts. Purely a cue: the badge says what
   * will happen, it does not make it happen (v4 `SpeakingAsAvatar.tsx`'s
   * `voiceRehearsal`, `686954937`; a cue, not a boolean, since `07b8f0209`).
   */
  readonly voiceRehearsal = input<VoiceRehearsalCue | null>(null);
  /** The Deus Ex Machina toggle — omitted (null), no toggle (v4 `establishedFact`). */
  readonly establishedFact = input<EstablishedFactToggle | null>(null);
  /** The toggle was clicked (v4's `establishedFact.onToggle`). */
  readonly toggleEstablishedFact = output<void>();

  protected readonly FACT_TIP = ESTABLISHED_FACT_TOGGLE_TIP;

  private readonly imagesHidden = injectImagesHidden();
  /**
   * v4 `e3937d7aa` `SpeakingAsAvatar.tsx:47-48`: `imagesHidden ? null : …` —
   * the quick-hide "Salon Images" switch falls back to the initial. Read here,
   * not threaded through the composer: the token reaches it from the Salon.
   */
  protected readonly avatarSrc = computed(() => (this.imagesHidden() ? null : this.avatarUrl()));
  protected readonly initial = computed(() => (this.name()[0] ?? '?').toUpperCase());
  /**
   * v4's three-arm ladder: the rehearsal cue outranks BOTH of the others, so an
   * armed seat says what the send will do even while the floor belongs to
   * someone else. The `aria-label` below is deliberately UNCHANGED — v4 leaves
   * it alone, and the badge itself is `aria-hidden`.
   */
  protected readonly titleText = computed(() => {
    const cue = this.voiceRehearsal();
    return cue
      ? voiceRehearsalTitle(cue, this.name(), 'portrait')
      : this.canType()
        ? `Speaking as ${this.name()}`
        : `Speaking as ${this.name()} — waiting for the room`;
  });
  protected readonly ariaLabel = computed(() =>
    this.canType()
      ? `Speaking as ${this.name()}`
      : `Speaking as ${this.name()}, waiting for the room`,
  );
}
