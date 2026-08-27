import { ChangeDetectionStrategy, Component, computed, inject, input, output } from '@angular/core';
import { Router, RouterLink } from '@angular/router';

import type { CharacterConnectionProfile, CharacterListItem } from '../../../core/core-contract';
import { Icon } from '../../../ui/icon';
import { characterAvatarSrc } from '../characters.api';
import { processTemplate, resolveUserToken } from '../templates';

/**
 * One character card in the roster (v4 `AuroraView.tsx` card block). The WHOLE
 * card is clickable and navigates to `/characters/:id` (v4 `handleCardClick`),
 * except clicks landing on an inner button/link — the three inline toggles
 * (favorite / Carina / controlledBy) and the Chat / Export / Delete actions,
 * which emit up to the list; it owns the mutations (optimistic) and dialogs.
 * The avatar+name block is additionally a real link (middle-click works).
 *
 * `qt-entity-card character-card` + the action classes carry over verbatim.
 */
@Component({
  selector: 'qt-character-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, Icon],
  template: `
    <div
      class="qt-entity-card character-card cursor-pointer hover:qt-border-primary/50 transition-colors"
      (click)="onCardClick($event)"
    >
      <div class="flex items-start justify-between mb-4">
        <a
          class="flex items-center flex-grow gap-4 min-w-0 cursor-pointer"
          [routerLink]="inTab() ? null : ['/characters', character().id]"
          (click)="inTab() && onDrillView($event)"
        >
          @if (avatarSrc()) {
            <img
              [src]="avatarSrc()"
              [alt]="character().name"
              width="48"
              height="48"
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
          <div class="flex-grow min-w-0">
            <!-- v4 makes the name row a flex line to seat the Archived badge
                 (AuroraView.tsx:497). v5's h2 additionally TRUNCATES long
                 names, and "truncate" does nothing on a flex container, so the
                 name keeps its own truncating span inside the row. -->
            <h2 class="qt-heading-3 text-foreground flex min-w-0 items-center gap-2">
              <span class="truncate">{{ character().name }}</span>
              @if (isArchived()) {
                <span
                  class="qt-badge inline-flex flex-shrink-0 items-center rounded-full border qt-border-default qt-bg-muted px-2 py-0.5 text-xs font-medium qt-text-secondary"
                  [title]="'Resting in the archive since ' + archivedSince()"
                >
                  Archived
                </span>
              }
            </h2>
            @if (character().title) {
              <p class="qt-text-small truncate">{{ character().title }}</p>
            }
            <p class="qt-text-small">{{ chatCountLabel() }}</p>
            @if (profileBadge(); as badge) {
              <span
                class="inline-flex items-center gap-1 mt-1 rounded-full qt-bg-muted qt-text-secondary px-2 py-0.5 qt-text-xs"
                [title]="badge.provider + ' · ' + badge.modelName"
              >
                <qt-icon name="cpu" class="w-3 h-3" />{{ badge.modelName }}
              </span>
            }
          </div>
        </a>
        <!-- P4.D64 (v4 AuroraView.tsx:522): the favorite / Carina /
             controlled-by cluster is HIDDEN on an archived card. -->
        @if (!isArchived()) {
        <div class="flex items-center gap-1 ml-2 flex-shrink-0">
          <button
            type="button"
            class="text-2xl qt-text-favorite transition-transform hover:scale-110 leading-none"
            [title]="character().isFavorite ? 'Remove from favorites' : 'Add to favorites'"
            (click)="favorite.emit()"
          >
            {{ character().isFavorite ? '⭐' : '☆' }}
          </button>
          <button
            type="button"
            [class]="
              'transition hover:scale-110 ' +
              (character().canBeCarina ? 'qt-text-favorite' : 'qt-text-secondary')
            "
            [title]="
              character().canBeCarina
                ? 'Disable Carina answers (@-queries)'
                : 'Enable Carina answers (@-queries)'
            "
            (click)="toggleCarina.emit()"
          >
            <qt-icon name="monitor" class="w-6 h-6" />
          </button>
          <button
            type="button"
            [class]="
              'transition hover:scale-110 ' +
              (character().controlledBy === 'user' ? 'qt-text-favorite' : 'qt-text-secondary')
            "
            [title]="
              character().controlledBy === 'user'
                ? 'Switch to LLM control'
                : 'Switch to user control'
            "
            (click)="toggleControlledBy.emit()"
          >
            <qt-icon name="user" class="w-6 h-6" />
          </button>
        </div>
        }
      </div>

      <p class="line-clamp-3 qt-text-small">{{ descriptionPreview() }}</p>

      <div class="qt-entity-card-actions character-card-actions">
        @if (isArchived()) {
          <!-- P4.D64 (v4 AuroraView.tsx:559-565): ONE inert span replaces the
               action affordances. v4's card has Chat + a single JSON export;
               v5's grew a second (PNG) export, and v4's intent is that "an
               archived character neither chats nor exports", so the span covers
               all THREE. Delete stays — an archived character can still be
               thrown away. -->
          <span
            class="inline-flex flex-1 items-center justify-center gap-2 rounded-lg border qt-border-default qt-bg-muted/50 px-4 py-2 text-sm qt-text-secondary"
            title="An archived character neither chats nor exports; open their page to rehydrate them."
          >
            Resting in the archive
          </span>
        } @else {
          <a
            [routerLink]="inTab() ? null : ['/characters', character().id]"
            [queryParams]="inTab() ? null : { action: 'chat' }"
            (click)="inTab() && onDrillChat($event)"
            class="character-card__action character-card__action--chat inline-flex flex-1 items-center justify-center gap-2 rounded-lg qt-bg-success px-4 py-2 text-sm font-semibold qt-text-on-success qt-shadow-sm transition hover:qt-bg-success/90"
            title="Start a chat with this character"
          >
            <qt-icon name="chat" class="w-5 h-5" />
            Chat
          </a>
          <button
            type="button"
            class="character-card__action inline-flex items-center justify-center gap-2 rounded-lg border qt-border-default qt-bg-muted/80 px-3 py-2 text-sm qt-text-primary qt-shadow-sm transition hover:qt-bg-muted"
            title="Export as SillyTavern JSON"
            (click)="exportCharacter.emit()"
          >
            <qt-icon name="download" class="w-5 h-5" />
          </button>
          <button
            type="button"
            class="character-card__action inline-flex items-center justify-center gap-2 rounded-lg border qt-border-default qt-bg-muted/80 px-3 py-2 text-sm qt-text-primary qt-shadow-sm transition hover:qt-bg-muted"
            title="Export as SillyTavern PNG card"
            (click)="exportPng.emit()"
          >
            <qt-icon name="image" class="w-5 h-5" />
          </button>
        }
        <button
          type="button"
          class="character-card__action qt-button-destructive qt-shadow-sm"
          title="Delete this character"
          (click)="deleteCharacter.emit()"
        >
          <qt-icon name="trash" class="w-5 h-5" />
        </button>
      </div>
    </div>
  `,
})
export class CharacterCard {
  private readonly router = inject(Router);

  readonly character = input.required<CharacterListItem>();
  /** Resolved by the list from `connectionProfileList` (v4 `getProfileProvider`). */
  readonly profile = input<CharacterConnectionProfile | null>(null);
  /** Hosted as a workspace tab ⇒ open drills in place (emits `view`). */
  readonly inTab = input<boolean>(false);

  readonly favorite = output<void>();
  readonly toggleCarina = output<void>();
  readonly toggleControlledBy = output<void>();
  readonly exportCharacter = output<void>();
  readonly exportPng = output<void>();
  readonly deleteCharacter = output<void>();
  /** Drill target (v4 `AuroraView` `setSelectedCharacterId`). */
  readonly view = output<void>();
  /**
   * In-tab Chat drill (v4 `AuroraView` `:509-521`): drill into the detail AND
   * flag its start-chat auto-open (`openChatOnMount`). The routed arm keeps the
   * `?action=chat` link above.
   */
  readonly openChat = output<void>();

  /**
   * v4 `handleCardClick`: don't navigate if the click landed on a button, link,
   * or other interactive element — those own their own behavior. Hosted ⇒ drill
   * in place (emit) rather than route.
   */
  protected onCardClick(e: MouseEvent): void {
    const target = e.target as HTMLElement;
    if (target.closest('button') || target.closest('a')) {
      return;
    }
    if (this.inTab()) {
      this.view.emit();
      return;
    }
    void this.router.navigate(['/characters', this.character().id]);
  }

  /**
   * The avatar/name affordance in tab mode: suppress navigation (the routerLink
   * is nulled) and drill into the detail in place.
   */
  protected onDrillView(e: Event): void {
    e.preventDefault();
    this.view.emit();
  }

  /**
   * The Chat action in tab mode (v4 `AuroraView` `:509-521`): suppress
   * navigation and drill into the detail with its start-chat auto-open flagged
   * (`openChatOnMount`).
   */
  protected onDrillChat(e: Event): void {
    e.preventDefault();
    this.openChat.emit();
  }

  /** P4.D64: `archivedAt` non-null ⇒ the tombstone card (v4 `Boolean(archivedAt)`). */
  protected readonly isArchived = computed(() => Boolean(this.character().archivedAt));

  /**
   * The badge tooltip's date. v4 interpolates
   * `new Date(archivedAt).toLocaleDateString()` with no guard, so a malformed
   * stamp reads "Invalid Date" in the tooltip — carried verbatim rather than
   * "improved" into hiding the badge, which would lose the fact that the
   * character IS archived. Badge VISIBILITY keys off `isArchived()`.
   */
  protected readonly archivedSince = computed(() =>
    new Date(this.character().archivedAt ?? '').toLocaleDateString(),
  );

  protected readonly avatarSrc = computed(() =>
    characterAvatarSrc(this.character().defaultImage, this.character().defaultImageId),
  );
  protected readonly initial = computed(() => (this.character().name[0] ?? '?').toUpperCase());
  protected readonly chatCountLabel = computed(() => {
    const n = this.character()._count?.chats ?? 0;
    return `${n} chat${n !== 1 ? 's' : ''}`;
  });

  protected readonly profileBadge = computed(() => {
    const p = this.profile();
    if (!this.character().defaultConnectionProfileId || !p?.provider || !p?.modelName) {
      return null;
    }
    return { provider: p.provider, modelName: p.modelName };
  });

  /** v4 card preview: the description with `{{char}}`/`{{user}}` substituted. */
  protected readonly descriptionPreview = computed(() => {
    const c = this.character();
    return processTemplate(c.description || '', {
      char: c.name,
      user: resolveUserToken(c.controlledBy, c.name, c.defaultPartnerName),
    });
  });
}
