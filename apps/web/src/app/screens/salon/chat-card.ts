import { ChangeDetectionStrategy, Component, computed, input, signal } from '@angular/core';
import { RouterLink } from '@angular/router';

import type { EnrichedChatSummary } from '../../core/core-contract';
import { AvatarStack, type AvatarStackEntity, normalizeAvatarSrc } from '../../ui/avatar-stack';
import { Icon } from '../../ui/icon';

/**
 * One conversation card in the Salon list — a slim port of v4
 * `components/chat/ChatCard.tsx` fed by `transformSalonChatToCardData`. The whole
 * card is a link to `/salon/:id`; badges/tags/participants are read-only.
 *
 * Deferrals (v4 features not in the M4 slice): the memory / scriptorium
 * badges are shown as static counts (no click-to-reextract / re-render), tag
 * colours use the default style (v4 resolves them client-side by id), and the
 * delete / remove action button is omitted.
 */
@Component({
  selector: 'qt-chat-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, Icon, AvatarStack],
  template: `
    <a
      class="qt-entity-card chat-card relative block cursor-pointer transition-colors"
      [routerLink]="['/salon', chat().id]"
    >
      <div class="flex items-stretch justify-between gap-4">
        <div class="flex items-stretch flex-1 gap-4 min-w-0">
          @if (storyBackgroundUrl()) {
            <div
              class="flex-shrink-0 self-stretch w-24 min-h-16 max-h-32 rounded-lg overflow-hidden qt-bg-muted"
            >
              <img class="w-full h-full object-cover" [src]="storyBackgroundUrl()" alt="" />
            </div>
          } @else if (participants().length > 0) {
            <qt-avatar-stack [entities]="participants()" />
          }

          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2 mb-1 flex-wrap">
              <h3 class="qt-card-title truncate">{{ displayTitle() }}</h3>

              <span
                class="chat-card__badge inline-flex items-center gap-1 rounded-full qt-bg-primary/10 px-2.5 py-0.5 qt-body-sm font-semibold flex-shrink-0"
                title="Messages"
              >
                <qt-icon name="chat" class="w-3 h-3" />{{ messageCount() }}
              </span>

              @if (memoryCount() > 0) {
                <span
                  class="chat-card__badge inline-flex items-center gap-1 rounded-full qt-bg-primary/10 px-2.5 py-0.5 qt-body-sm font-semibold flex-shrink-0"
                  title="Memories"
                >
                  <qt-icon name="book" class="w-3 h-3" />{{ memoryCount() }}
                </span>
              }

              @if (chat().isDangerousChat) {
                <span
                  class="qt-text-destructive text-sm flex-shrink-0"
                  title="Flagged as dangerous"
                  aria-label="Flagged as dangerous"
                  >*</span
                >
              }

              @if (isAutonomous()) {
                <span
                  class="chat-card__badge inline-flex items-center gap-1 rounded-full qt-bg-muted qt-text-secondary px-2 py-0.5 qt-body-sm font-semibold flex-shrink-0"
                  title="Autonomous character-to-character room"
                >
                  <qt-icon name="clock" class="w-3 h-3" />Autonomous
                </span>
              }

              <button
                type="button"
                class="chat-card__badge inline-flex items-center justify-center rounded-full qt-bg-muted qt-text-secondary w-6 h-6 flex-shrink-0 transition-colors cursor-pointer"
                title="Copy link to this chat"
                aria-label="Copy link to this chat"
                (click)="copyLink($event)"
              >
                <qt-icon [name]="copied() ? 'check' : 'link'" class="w-3 h-3" />
              </button>
            </div>

            <p class="qt-text-small qt-text-secondary">
              @if (participantNames()) {
                {{ participantNames() }} <span aria-hidden="true">•</span>
              }
              {{ dateStr() }}
            </p>

            @if (chat().project || tagNames().length > 0) {
              <div class="mt-2 flex items-center gap-2 flex-wrap">
                @if (chat().project; as project) {
                  <span
                    class="inline-flex items-center gap-1 qt-text-xs px-2 py-1 rounded-full qt-bg-muted"
                  >
                    <qt-icon name="folder" class="w-3 h-3" />
                    <span>{{ project.name }}</span>
                  </span>
                }
                @for (tag of tagNames(); track tag) {
                  <span class="qt-tag-badge qt-tag-badge-md">{{ tag }}</span>
                }
              </div>
            }
          </div>
        </div>
      </div>
    </a>
  `,
})
export class ChatCard {
  readonly chat = input.required<EnrichedChatSummary>();

  protected readonly copied = signal(false);

  protected readonly displayTitle = computed(() => this.chat().title || 'Untitled Chat');
  protected readonly messageCount = computed(() => this.chat()._count?.messages ?? 0);
  protected readonly memoryCount = computed(() => this.chat()._count?.memories ?? 0);
  protected readonly isAutonomous = computed(() => this.chat().chatType === 'autonomous');
  protected readonly storyBackgroundUrl = computed(() =>
    normalizeAvatarSrc(this.chat().storyBackground?.filepath),
  );

  /** Active CHARACTER participants, sorted by display order (v4 transform). */
  protected readonly participants = computed<AvatarStackEntity[]>(() =>
    (this.chat().participants ?? [])
      .filter((p) => p.type === 'CHARACTER' && p.isActive && p.character)
      .sort((a, b) => a.displayOrder - b.displayOrder)
      .map((p) => ({ id: p.id, name: p.character!.name, avatarUrl: p.character!.avatarUrl })),
  );

  protected readonly participantNames = computed(() =>
    this.participants()
      .map((p) => p.name)
      .join(', '),
  );

  protected readonly tagNames = computed(() =>
    (this.chat().tags ?? []).map((t) => t.tag.name),
  );

  protected readonly dateStr = computed(() => {
    // The Salon transform deliberately omits lastMessageAt, so cards show updatedAt.
    const d = new Date(this.chat().updatedAt);
    return Number.isNaN(d.getTime()) ? '' : d.toLocaleDateString();
  });

  protected copyLink(event: Event): void {
    event.preventDefault();
    event.stopPropagation();
    const origin =
      typeof window !== 'undefined' && window.location ? window.location.origin : '';
    const url = `${origin}/salon/${this.chat().id}`;
    void navigator.clipboard?.writeText(url).then(() => {
      this.copied.set(true);
      setTimeout(() => this.copied.set(false), 1500);
    });
  }
}
