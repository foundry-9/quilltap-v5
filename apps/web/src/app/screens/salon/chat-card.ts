import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { RouterLink } from '@angular/router';

import { CoreClient, coreErrorMessage } from '../../core/core-client';
import type { EnrichedChatSummary } from '../../core/core-contract';
import { notifyQueueChange } from '../../layout/queue-status.logic';
import { AvatarStack, type AvatarStackEntity, normalizeAvatarSrc } from '../../ui/avatar-stack';
import { Icon } from '../../ui/icon';
import { ScriptoriumBadge } from '../../ui/scriptorium-badge';
import { ToastService } from '../../ui/toast.service';

/**
 * One conversation card in the Salon list — a slim port of v4
 * `components/chat/ChatCard.tsx` fed by `transformSalonChatToCardData`. The whole
 * card is a link to `/salon/:id`; badges/tags/participants are read-only.
 *
 * The Scriptorium badge (p4.9o) is a three-state pill that queues an on-demand
 * conversation render on click. Remaining deferral: the memory badge is a static
 * count (no click-to-reextract) and tag colours use the default style (v4
 * resolves them client-side by id).
 *
 * The optional `removable` mode (v4 `actionType="remove"`, used by the project
 * chats section) overlays an X that emits `remove` — it DISASSOCIATES the chat
 * from its project, never deletes it.
 */
@Component({
  selector: 'qt-chat-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, Icon, AvatarStack, ScriptoriumBadge],
  template: `
    <a
      class="qt-entity-card chat-card relative block cursor-pointer transition-colors"
      [routerLink]="['/salon', chat().id]"
    >
      @if (removable()) {
        <button
          type="button"
          class="absolute top-2 right-2 z-10 p-1.5 rounded-full qt-text-secondary hover:qt-text-destructive hover:qt-bg-destructive/10 transition-colors"
          title="Remove from project"
          aria-label="Remove from project"
          (click)="onRemove($event)"
        >
          <qt-icon name="close" class="w-4 h-4" />
        </button>
      }
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
  private readonly toasts = inject(ToastService);
  private readonly core = inject(CoreClient);
  readonly chat = input.required<EnrichedChatSummary>();
  /** v4 `actionType="remove"` — overlay an X that disassociates from the project. */
  readonly removable = input(false);
  readonly remove = output<string>();

  protected readonly copied = signal(false);
  protected readonly rendering = signal(false);

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

  protected readonly tagNames = computed(() => (this.chat().tags ?? []).map((t) => t.tag.name));

  protected readonly dateStr = computed(() => {
    // The Salon transform deliberately omits lastMessageAt, so cards show updatedAt.
    const d = new Date(this.chat().updatedAt);
    return Number.isNaN(d.getTime()) ? '' : d.toLocaleDateString();
  });

  protected onRemove(event: Event): void {
    event.preventDefault();
    event.stopPropagation();
    this.remove.emit(this.chat().id);
  }

  /**
   * Queue an on-demand Scriptorium render (v4 `handleRenderConversation`): POST
   * the render-conversation action, toast, and wake the toolbar queue badges.
   * v5 skips v4's immediate list refetch — the render is a background job, so
   * the badge only changes once it completes (the next natural list load).
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

  protected copyLink(event: Event): void {
    event.preventDefault();
    event.stopPropagation();
    const origin = typeof window !== 'undefined' && window.location ? window.location.origin : '';
    const url = `${origin}/salon/${this.chat().id}`;
    void navigator.clipboard
      ?.writeText(url)
      .then(() => {
        // v4 keeps BOTH: the 1.5s check icon AND the toast (`ChatCard.tsx:160-164`).
        this.copied.set(true);
        this.toasts.showSuccess('Link copied to clipboard');
        setTimeout(() => this.copied.set(false), 1500);
      })
      .catch(() => this.toasts.showError('Failed to copy link'));
  }
}
