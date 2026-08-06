import { ChangeDetectionStrategy, Component, computed, inject, input, signal } from '@angular/core';
import { RouterLink } from '@angular/router';

import { CoreClient, coreErrorMessage } from '../../../../core/core-client';
import type { CharacterChatSummary } from '../../../../core/core-contract';
import { notifyQueueChange } from '../../../../layout/queue-status.logic';
import { normalizeAvatarSrc } from '../../../../ui/avatar-stack';
import { Icon } from '../../../../ui/icon';
import { ScriptoriumBadge } from '../../../../ui/scriptorium-badge';
import { ToastService } from '../../../../ui/toast.service';

/**
 * Chat-list date (v4 `lib/format-time.ts` `formatChatListDate`, `useRelative`
 * always true here): today→time, yesterday→'Yesterday', <7d→weekday, else→date
 * (year only when different from now).
 */
export function formatChatListDate(dateString: string): string {
  const date = new Date(dateString);
  if (Number.isNaN(date.getTime())) {
    return String(dateString);
  }
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

  if (diffDays === 0) {
    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  }
  if (diffDays === 1) {
    return 'Yesterday';
  }
  if (diffDays < 7) {
    return date.toLocaleDateString([], { weekday: 'long' });
  }
  return date.toLocaleDateString([], {
    month: 'short',
    day: 'numeric',
    year: date.getFullYear() !== now.getFullYear() ? 'numeric' : undefined,
  });
}

/** The preview text (v4 `lib/chat-utils.ts` `getCharacterChatPreview`): the LAST
 *  element of the recent-first ≤3 array (i.e. the oldest of the recent three),
 *  newlines flattened, truncated to 100 chars. Ported verbatim, quirk and all. */
function previewOf(messages: CharacterChatSummary['messages']): string | null {
  const last = messages[messages.length - 1];
  if (!last) {
    return null;
  }
  const content = last.content.replace(/\n/g, ' ').trim();
  return content.length > 100 ? content.slice(0, 100) + '...' : content;
}

/**
 * One conversation card in the character Conversations tab — a display-only port
 * of v4 `components/chat/ChatCard.tsx` fed by `transformCharacterChatToCardData`
 * with `showAvatars={false}`, `showProject`, `showPreview`, `useRelativeDates`.
 * The whole card links to `/salon/:id`; tags are read-only. The Scriptorium
 * badge (p4.9o) queues an on-demand render on click, as on the Salon card; the
 * v4 delete / re-extract actions remain omitted (routes outside this contract).
 *
 * Divergence noted: v4's ConversationsTab hides the story-background thumbnail
 * (its `ChatCard` gates it behind `showAvatars`, which is false here); the work
 * order enumerates it as a card field, so it renders when present.
 */
@Component({
  selector: 'qt-character-conversation-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, Icon, ScriptoriumBadge],
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

              <qt-scriptorium-badge
                [status]="chat().scriptoriumStatus"
                [busy]="rendering()"
                (render)="renderConversation()"
              />

              @if (chat().isDangerousChat) {
                <span
                  class="qt-text-destructive text-sm flex-shrink-0"
                  title="Flagged as dangerous"
                  aria-label="Flagged as dangerous"
                  >*</span
                >
              }
            </div>

            <p class="qt-text-small qt-text-secondary">{{ dateStr() }}</p>

            @if (previewText()) {
              <p class="qt-text-small qt-text-secondary line-clamp-1 mt-2">{{ previewText() }}</p>
            }

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
export class CharacterConversationCard {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);
  readonly chat = input.required<CharacterChatSummary>();

  protected readonly rendering = signal(false);

  protected readonly displayTitle = computed(() => this.chat().title || 'Untitled Chat');
  protected readonly messageCount = computed(() => this.chat()._count?.messages ?? 0);
  protected readonly memoryCount = computed(() => this.chat()._count?.memories ?? 0);
  protected readonly previewText = computed(() => previewOf(this.chat().messages ?? []));
  protected readonly storyBackgroundUrl = computed(() =>
    normalizeAvatarSrc(this.chat().storyBackground?.filepath),
  );
  protected readonly tagNames = computed(() => (this.chat().tags ?? []).map((t) => t.tag.name));

  protected readonly dateStr = computed(() =>
    formatChatListDate(this.chat().lastMessageAt || this.chat().updatedAt),
  );

  /**
   * Queue an on-demand Scriptorium render (v4 `handleRenderConversation`, the
   * character Conversations tab's copy): POST, toast, and nudge the queue
   * badges. Like the Salon card, v5 skips v4's immediate list refetch — the
   * render is a background job.
   */
  protected async renderConversation(): Promise<void> {
    if (this.rendering()) return;
    this.rendering.set(true);
    try {
      await this.core.renderConversation(this.chat().id);
      this.toasts.showSuccess('Conversation rendering queued');
      notifyQueueChange();
    } catch (err) {
      this.toasts.showError(coreErrorMessage(err, 'Failed to queue conversation rendering'));
    } finally {
      this.rendering.set(false);
    }
  }
}
