import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

import { Icon } from '../ui/icon';

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
  imports: [Icon],
  template: `
    <div
      class="qt-speaking-as-avatar"
      [class.qt-speaking-as-avatar-dim]="!canType()"
      [title]="titleText()"
      [attr.aria-label]="ariaLabel()"
    >
      @if (avatarUrl()) {
        <img [src]="avatarUrl()" [alt]="name()" class="w-full h-full object-cover" />
      } @else {
        <span class="font-bold qt-text-secondary text-lg">{{ initial() }}</span>
      }
      @if (voiceRehearsal()) {
        <span class="qt-speaking-as-avatar-voice-badge" aria-hidden="true">
          <qt-icon name="thinking" class="w-3 h-3" />
        </span>
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
   * True when In Their Own Words is armed for this seat — a typed line goes to
   * the character for a restatement you review before it posts. Purely a cue:
   * the badge says what will happen, it does not make it happen (v4
   * `SpeakingAsAvatar.tsx`'s `voiceRehearsal`, `686954937`).
   */
  readonly voiceRehearsal = input(false);

  protected readonly initial = computed(() => (this.name()[0] ?? '?').toUpperCase());
  /**
   * v4's three-arm ladder: the rehearsal cue outranks BOTH of the others, so an
   * armed seat says what the send will do even while the floor belongs to
   * someone else. The `aria-label` below is deliberately UNCHANGED — v4 leaves
   * it alone, and the badge itself is `aria-hidden`.
   */
  protected readonly titleText = computed(() =>
    this.voiceRehearsal()
      ? `Speaking as ${this.name()} — your draft goes to ${this.name()} to say in their own words first`
      : this.canType()
        ? `Speaking as ${this.name()}`
        : `Speaking as ${this.name()} — waiting for the room`,
  );
  protected readonly ariaLabel = computed(() =>
    this.canType()
      ? `Speaking as ${this.name()}`
      : `Speaking as ${this.name()}, waiting for the room`,
  );
}
