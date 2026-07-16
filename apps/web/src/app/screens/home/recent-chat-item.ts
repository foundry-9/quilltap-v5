import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';
import { RouterLink } from '@angular/router';

import { AvatarStack, type AvatarStackEntity, normalizeAvatarSrc } from '../../ui/avatar-stack';
import { characterAvatarSrc } from '../characters/characters.api';
import { formatMessageTime } from './format-time';
import type { RecentChat } from './home.api';

/**
 * One chat row in the homepage Recent Chats list (v4
 * `components/homepage/RecentChatItem.tsx`): the story-background thumbnail
 * when present (preferred), else the participant avatar stack; the title +
 * joined character names; the relative time + message count with v4's
 * dangerous-chat `*` marker. v4's workspace-tab `openInWorkspace` glue is not
 * ported (no tabbed workspace in v5) — the link navigates.
 *
 * §2: every server-relative path resolves through the shared helpers
 * ({@link normalizeAvatarSrc} / {@link characterAvatarSrc}) — no inline URL
 * building.
 */
@Component({
  selector: 'qt-recent-chat-item',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, AvatarStack],
  template: `
    <a
      [routerLink]="['/salon', chat().id]"
      class="flex items-center gap-3 p-2 rounded-lg hover:qt-bg-muted/50 transition-colors"
    >
      <!-- Story background thumbnail (preferred) or Avatar stack -->
      @if (backgroundUrl(); as bg) {
        <div class="flex-shrink-0 w-16 h-11 rounded-md overflow-hidden qt-bg-muted -my-1">
          <img [src]="bg" alt="" class="w-full h-full object-cover" />
        </div>
      } @else {
        <qt-avatar-stack [entities]="characters()" [diameter]="32" [maxDisplay]="2" />
      }
      <div class="flex-1 min-w-0">
        <p class="qt-card-title truncate">{{ chat().title }}</p>
        <p class="qt-card-subtitle truncate">{{ characterNames() }}</p>
      </div>
      <div class="flex flex-col items-end shrink-0">
        <span class="qt-meta">{{ timeLabel() }}</span>
        <span class="qt-meta text-primary"
          >{{ chat()._count.messages }} msgs@if (chat().isDangerousChat) {<span
            class="qt-text-destructive"
            title="Flagged as dangerous"
            aria-label="Flagged as dangerous"
            >*</span
          >}</span
        >
      </div>
    </a>
  `,
})
export class RecentChatItem {
  readonly chat = input.required<RecentChat>();

  protected readonly backgroundUrl = computed(() =>
    normalizeAvatarSrc(this.chat().storyBackgroundUrl),
  );

  /** Active CHARACTER participants by display order (v4's `characters`). */
  protected readonly characters = computed<AvatarStackEntity[]>(() =>
    this.chat()
      .participants.filter((p) => p.type === 'CHARACTER' && p.isActive && p.character)
      .sort((a, b) => a.displayOrder - b.displayOrder)
      .map((p) => ({
        id: p.character!.id,
        name: p.character!.name,
        avatarUrl: characterAvatarSrc(p.character!.defaultImage, p.character!.defaultImageId),
      })),
  );

  /** v4 `characterNames`: 'Unknown' / one name / 'A + B' / all joined with ' + '. */
  protected readonly characterNames = computed(() => {
    const characters = this.characters();
    return characters.length === 0
      ? 'Unknown'
      : characters.length === 1
        ? characters[0].name
        : characters.length === 2
          ? `${characters[0].name} + ${characters[1].name}`
          : characters.map((c) => c.name).join(' + ');
  });

  protected readonly timeLabel = computed(() =>
    formatMessageTime(this.chat().lastMessageAt ?? this.chat().updatedAt),
  );
}
