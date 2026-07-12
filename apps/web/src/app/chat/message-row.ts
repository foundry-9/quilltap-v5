import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  input,
  output,
  signal,
} from '@angular/core';

import type { ChatDetail, ChatSettingsDto, MessageDto } from '../core/core-contract';
import { Avatar } from '../ui/avatar';
import { Icon } from '../ui/icon';
import { resolveMessageAuthor, type SwipeState } from './chat-view-model';
import { MessageContent } from './message-content';
import { ThinkingBlock } from './thinking-block';

/** The bubble variant for a message (drives the qt-chat-message-* class). */
type Variant = 'user' | 'assistant' | 'whisper' | 'silent';

/**
 * One conversation message row — a slim port of v4 `MessageRow`. Renders the
 * author avatar + name, whisper/silent labels, reasoning blocks, the markdown
 * body, a timestamp/token line, swipe arrows, and a hover action bar. Mutations
 * are emitted to the conversation screen, which owns the dispatches.
 */
@Component({
  selector: 'qt-message-row',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Avatar, Icon, MessageContent, ThinkingBlock],
  template: `
    <div
      class="qt-chat-message-row"
      [class.qt-chat-message-row-user]="author().isUser"
      [class.qt-chat-message-row-assistant]="!author().isUser"
      [class.group]="true"
    >
      @if (showAvatar() && !author().isUser) {
        <div class="qt-chat-desktop-avatar">
          <qt-avatar [name]="author().name" [src]="author().avatarUrl" size="chat" />
        </div>
      }

      <div class="qt-chat-message-body">
        <div class="qt-chat-message-header">
          <span class="qt-chat-message-author">{{ author().name }}</span>
          @if (author().title) {
            <span class="qt-chat-message-time">{{ author().title }}</span>
          }
        </div>

        <div class="qt-chat-message" [class]="bubbleClass()">
          @if (variant() === 'whisper') {
            <div class="qt-chat-whisper-label">Private whisper</div>
          } @else if (variant() === 'silent') {
            <div class="qt-chat-silent-label">Silent — inner thoughts</div>
          }

          @for (block of reasoningBlocks(); track block.seq) {
            <qt-thinking-block [content]="block.content" [collapsed]="thinkingCollapsed()" />
          }

          @if (editing()) {
            <textarea
              class="qt-chat-composer-input w-full"
              rows="3"
              [value]="editDraft()"
              aria-label="Edit message"
              (input)="editDraft.set($any($event.target).value)"
            ></textarea>
            <div class="flex justify-end gap-2 mt-2">
              <button
                type="button"
                class="qt-button-secondary qt-button-sm"
                (click)="cancelEdit.emit()"
              >
                Cancel
              </button>
              <button
                type="button"
                class="qt-button-primary qt-button-sm"
                (click)="saveEdit.emit({ id: message().id, content: editDraft() })"
              >
                Save
              </button>
            </div>
          } @else {
            <qt-message-content [content]="message().content" />
          }

          <div class="qt-chat-message-action-bar">
            <div class="qt-chat-message-action-bar-icons">
              <button
                type="button"
                class="qt-chat-message-action-icon"
                title="Copy message"
                aria-label="Copy message"
                (click)="copy.emit(message())"
              >
                <qt-icon name="copy" class="w-4 h-4" />
              </button>
              @if (message().role === 'USER') {
                <button
                  type="button"
                  class="qt-chat-message-action-icon"
                  title="Edit message"
                  aria-label="Edit message"
                  (click)="edit.emit(message())"
                >
                  <qt-icon name="pencil" class="w-4 h-4" />
                </button>
              }
              @if (message().role === 'ASSISTANT') {
                <button
                  type="button"
                  class="qt-chat-message-action-icon"
                  title="Regenerate response"
                  aria-label="Regenerate response"
                  (click)="regenerate.emit(message())"
                >
                  <qt-icon name="refresh" class="w-4 h-4" />
                </button>
              }
              <button
                type="button"
                class="qt-chat-message-action-icon"
                title="Delete message"
                aria-label="Delete message"
                (click)="delete.emit(message())"
              >
                <qt-icon name="trash" class="w-4 h-4" />
              </button>

              @if (swipeState() && swipeState()!.total > 1) {
                <button
                  type="button"
                  class="qt-chat-message-action-icon"
                  title="Previous response"
                  aria-label="Previous response"
                  [disabled]="swipeState()!.current === 0"
                  (click)="swipePrev.emit(message())"
                >
                  <qt-icon name="chevron-left" class="w-4 h-4" />
                </button>
                <span class="qt-text-xs px-1"
                  >{{ swipeState()!.current + 1 }} / {{ swipeState()!.total }}</span
                >
                <button
                  type="button"
                  class="qt-chat-message-action-icon"
                  title="Next response"
                  aria-label="Next response"
                  [disabled]="swipeState()!.current === swipeState()!.total - 1"
                  (click)="swipeNext.emit(message())"
                >
                  <qt-icon name="chevron-right" class="w-4 h-4" />
                </button>
              }
            </div>
            <span class="qt-chat-message-time ml-auto">{{ timestamp() }}</span>
          </div>
        </div>
      </div>

      @if (showAvatar() && author().isUser) {
        <div class="qt-chat-desktop-avatar">
          <qt-avatar [name]="author().name" [src]="author().avatarUrl" size="chat" />
        </div>
      }
    </div>
  `,
})
export class MessageRow {
  readonly message = input.required<MessageDto>();
  readonly chat = input.required<ChatDetail>();
  readonly swipeState = input<SwipeState | null>(null);
  readonly settings = input<ChatSettingsDto | null>(null);
  readonly showAvatar = input(true);
  readonly editing = input(false);

  readonly copy = output<MessageDto>();
  readonly edit = output<MessageDto>();
  readonly delete = output<MessageDto>();
  readonly regenerate = output<MessageDto>();
  readonly swipePrev = output<MessageDto>();
  readonly swipeNext = output<MessageDto>();
  readonly saveEdit = output<{ id: string; content: string }>();
  readonly cancelEdit = output<void>();

  protected readonly editDraft = signal('');

  constructor() {
    // Seed the draft from the current content each time editing begins.
    effect(() => {
      if (this.editing()) {
        this.editDraft.set(this.message().content);
      }
    });
  }

  protected readonly author = computed(() => resolveMessageAuthor(this.message(), this.chat()));

  protected readonly variant = computed<Variant>(() => {
    const m = this.message();
    if (m.isSilentMessage) return 'silent';
    if (m.targetParticipantIds && m.targetParticipantIds.length > 0) return 'whisper';
    return m.role === 'USER' ? 'user' : 'assistant';
  });

  protected readonly bubbleClass = computed(() => `qt-chat-message-${this.variant()}`);

  protected readonly reasoningBlocks = computed(() => {
    const m = this.message();
    if (m.reasoningSegments && m.reasoningSegments.length > 0) {
      return m.reasoningSegments;
    }
    if (m.reasoningContent) {
      return [{ anchorOffset: 0, content: m.reasoningContent, seq: 0 }];
    }
    return [];
  });

  protected readonly thinkingCollapsed = computed(
    () => this.settings()?.thinkingDisplay?.defaultCollapsed ?? true,
  );

  protected readonly timestamp = computed(() => {
    const d = new Date(this.message().createdAt);
    return Number.isNaN(d.getTime())
      ? ''
      : d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
  });
}
