import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  afterNextRender,
  computed,
  effect,
  inject,
  input,
  output,
  viewChild,
} from '@angular/core';

import type { ChatStreamState } from '../core/chat-stream.reducer';
import type { ChatDetail, ChatSettingsDto, MessageDto } from '../core/core-contract';
import { AnnouncementGroup } from './announcement-group';
import { buildRenderItems, type SwipeState } from './chat-view-model';
import { MessageRow } from './message-row';
import { StreamingMessage } from './streaming-message';

/**
 * The scrolling message list — the render-item pipeline (message rows +
 * collapsed announcement groups) plus the live streaming bubble. A plain scroll
 * container with bottom-anchoring is the sanctioned MVP substitute for v4's
 * TanStack virtualizer (virtualization is a tracked deferral).
 */
@Component({
  selector: 'qt-message-list',
  // Stretch inside .qt-chat-messages-viewport so .qt-chat-messages (flex-1 +
  // overflow-y-auto) gets a BOUNDED height and actually scrolls — an unstyled
  // host breaks the flex chain (the third Friday dogfood finding).
  host: { class: 'flex flex-col flex-1 min-h-0' },
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [MessageRow, AnnouncementGroup, StreamingMessage],
  template: `
    <div #scroll class="qt-chat-messages">
      <div class="qt-chat-messages-list">
        @for (item of items(); track itemKey(item)) {
          @if (item.type === 'message') {
            <qt-message-row
              [message]="item.message"
              [chat]="chat()"
              [swipeState]="swipeFor(item.message)"
              [settings]="settings()"
              [showAvatar]="showAvatars()"
              [editing]="item.message.id === editingId()"
              (copy)="copy.emit($event)"
              (edit)="edit.emit($event)"
              (delete)="delete.emit($event)"
              (regenerate)="regenerate.emit($event)"
              (swipePrev)="swipePrev.emit($event)"
              (swipeNext)="swipeNext.emit($event)"
              (saveEdit)="saveEdit.emit($event)"
              (cancelEdit)="cancelEdit.emit()"
            />
          } @else {
            <qt-announcement-group [chips]="item.chips" />
          }
        }

        @if (stream(); as s) {
          <qt-streaming-message [state]="s" />
        }
      </div>
    </div>
  `,
})
export class MessageList {
  readonly messages = input.required<MessageDto[]>();
  readonly chat = input.required<ChatDetail>();
  readonly swipeStates = input<Record<string, SwipeState>>({});
  readonly settings = input<ChatSettingsDto | null>(null);
  readonly stream = input<ChatStreamState | null>(null);
  readonly editingId = input<string | null>(null);

  readonly copy = output<MessageDto>();
  readonly edit = output<MessageDto>();
  readonly delete = output<MessageDto>();
  readonly regenerate = output<MessageDto>();
  readonly swipePrev = output<MessageDto>();
  readonly swipeNext = output<MessageDto>();
  readonly saveEdit = output<{ id: string; content: string }>();
  readonly cancelEdit = output<void>();

  private readonly scroll = viewChild<ElementRef<HTMLElement>>('scroll');

  protected readonly items = computed(() => buildRenderItems(this.messages()));

  /** Avatar display: v4 ALWAYS / GROUP_ONLY (≥2 chars) / NEVER. */
  protected readonly showAvatars = computed(() => {
    const mode = this.settings()?.avatarDisplayMode ?? 'ALWAYS';
    if (mode === 'NEVER') return false;
    if (mode === 'GROUP_ONLY') {
      return this.chat().participants.filter((p) => p.type === 'CHARACTER').length >= 2;
    }
    return true;
  });

  constructor() {
    afterNextRender(() => this.scrollToBottom());
    // Re-anchor to the newest content whenever the list or the live stream grows.
    effect(() => {
      this.messages();
      this.stream()?.content;
      this.stream()?.messages.length;
      queueMicrotask(() => this.scrollToBottom());
    });
  }

  protected swipeFor(message: MessageDto): SwipeState | null {
    return message.swipeGroupId ? (this.swipeStates()[message.swipeGroupId] ?? null) : null;
  }

  protected itemKey(item: ReturnType<typeof buildRenderItems>[number]): string {
    return item.type === 'message' ? item.message.id : `grp-${item.chips[0]?.id ?? ''}`;
  }

  private scrollToBottom(): void {
    const el = this.scroll()?.nativeElement;
    if (el) {
      el.scrollTop = el.scrollHeight;
    }
  }
}
