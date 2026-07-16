import { ChangeDetectionStrategy, Component, computed, inject, input } from '@angular/core';
import { Router, RouterLink } from '@angular/router';

import type { CharacterConnectionProfile } from '../../core/core-contract';
import { Icon } from '../../ui/icon';
import { characterAvatarSrc } from '../characters/characters.api';
import { profileDisplayName, type HomepageCharacter } from './home.api';

/**
 * One character card in the homepage grid (v4
 * `components/homepage/CharacterCard.tsx`): 4:5 avatar, name, italic title
 * (an NBSP placeholder keeps the two-line box), the compact provider badge,
 * and the Chat action.
 *
 * Two documented divergences from v4, both order-mandated (P4.6av tier 1):
 *  - The WHOLE card is clickable and navigates to `/characters/:id` (the
 *    finding-#4 pattern from the roster card), except clicks landing on an
 *    inner button/link; v4 only links the avatar+name block. The inner link
 *    stays real so middle-click works.
 *  - The Chat button navigates to `/salon/new?characterId=` (the established
 *    New-Chat-page precedent, P4.6q) instead of opening v4's `NewChatModal`
 *    (banked for the M6 screen-parity review).
 */
@Component({
  selector: 'qt-home-character-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, Icon],
  template: `
    <div class="qt-character-card cursor-pointer" (click)="onCardClick($event)">
      <a
        [routerLink]="['/characters', character().id]"
        class="flex flex-col items-center gap-2 flex-grow hover:opacity-80 transition-opacity"
      >
        @if (avatarSrc(); as src) {
          <img
            [src]="src"
            [alt]="character().name"
            width="48"
            height="60"
            class="w-12 h-15 rounded-lg object-cover qt-bg-muted flex-shrink-0"
            style="aspect-ratio: 4/5"
          />
        } @else {
          <div
            class="w-12 h-15 rounded-lg qt-bg-muted flex items-center justify-center flex-shrink-0"
            style="aspect-ratio: 4/5"
          >
            <span class="text-lg font-bold qt-text-secondary">{{ initial() }}</span>
          </div>
        }
        <div class="text-center min-w-0 w-full">
          <p class="qt-card-title truncate">{{ character().name }}</p>
          <p class="qt-card-subtitle line-clamp-2 italic min-h-[2.5rem]">{{ titleText() }}</p>
          @if (badgeName() !== null) {
            <span
              class="inline-flex items-center gap-1 mt-1 rounded-full qt-bg-muted qt-text-secondary px-2 py-0.5 qt-text-xs"
              [title]="badgeTitle()"
            >
              <qt-icon name="cpu" class="w-3 h-3" />{{ badgeName() }}
            </span>
          }
        </div>
      </a>
      <button
        type="button"
        class="mt-auto w-full qt-button-success qt-button-sm"
        [title]="'Start a chat with ' + character().name"
        (click)="onChat()"
      >
        <qt-icon name="chat" class="w-3.5 h-3.5" />
        Chat
      </button>
    </div>
  `,
})
export class HomeCharacterCard {
  private readonly router = inject(Router);

  readonly character = input.required<HomepageCharacter>();
  /** Resolved by the section from `connectionProfileList` (v4 `getProfileProvider`). */
  readonly profile = input<CharacterConnectionProfile | null>(null);

  /**
   * The finding-#4 guard (v4 `AuroraView.tsx` `handleCardClick`, the P4.6f
   * pattern): don't navigate when the click landed on a button, link, or other
   * interactive element — those own their own behavior.
   */
  protected onCardClick(e: MouseEvent): void {
    const target = e.target as HTMLElement;
    if (target.closest('button') || target.closest('a')) {
      return;
    }
    void this.router.navigate(['/characters', this.character().id]);
  }

  protected onChat(): void {
    void this.router.navigate(['/salon/new'], {
      queryParams: { characterId: this.character().id },
    });
  }

  protected readonly avatarSrc = computed(() =>
    characterAvatarSrc(this.character().defaultImage, this.character().defaultImageId),
  );
  protected readonly initial = computed(() => (this.character().name[0] ?? '?').toUpperCase());
  /** v4: `character.title || ' '` — the NBSP keeps the two-line box. */
  protected readonly titleText = computed(() => this.character().title || '\u00A0');

  /** v4: the badge renders whenever a profile resolved for the character. */
  protected readonly badgeName = computed<string | null>(() => {
    if (!this.character().defaultConnectionProfileId || !this.profile()) {
      return null;
    }
    return profileDisplayName(this.profile());
  });

  /** v4 badge tooltip: `${name || provider}: ${modelName}`. */
  protected readonly badgeTitle = computed(() => {
    const p = this.profile();
    if (!p) {
      return '';
    }
    return `${p.name || p.provider}: ${p.modelName}`;
  });
}
