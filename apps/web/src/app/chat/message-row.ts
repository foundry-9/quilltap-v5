import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  input,
  output,
  signal,
} from '@angular/core';

import type { ChatDetail, ChatSettingsDto, MessageAttachment, MessageDto } from '../core/core-contract';
import { Avatar } from '../ui/avatar';
import { Icon } from '../ui/icon';
import { thumbnailUrl, fileUrl } from '../images/image-urls';
import { resolveMessageAuthor, type SwipeState } from './chat-view-model';
import { CourierBubble } from './courier-bubble';
import { MessageContent } from './message-content';
import { ThinkingBlock } from './thinking-block';

/** The bubble variant for a message (drives the qt-chat-message-* class). */
type Variant = 'user' | 'assistant' | 'whisper' | 'silent';

/** An in-chat image click → opens the lightbox (v4 `onImageClick`). */
export interface ImageClickEvent {
  /** Full-resolution image URL (the modal `src`). */
  src: string;
  filename: string;
  fileId?: string;
}

/**
 * One conversation message row — a slim port of v4 `MessageRow`. Renders the
 * author avatar + name, whisper/silent labels, reasoning blocks, the markdown
 * body, a timestamp/token line, swipe arrows, and a hover action bar. Mutations
 * are emitted to the conversation screen, which owns the dispatches.
 */
@Component({
  selector: 'qt-message-row',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Avatar, Icon, CourierBubble, MessageContent, ThinkingBlock],
  template: `
    <div
      class="qt-chat-message-row"
      [class.qt-chat-message-row-user]="author().isUser"
      [class.qt-chat-message-row-assistant]="!author().isUser"
      [class.qt-chat-message-row-courier]="isCourier()"
      [class.group]="true"
    >
      @if (isCourier()) {
        <!-- The Courier: a pending manual/clipboard turn renders a bubble in place
             of the normal message, skipping the action bar and danger chrome (v4
             MessageRow's early-return courier branch). -->
        @if (showAvatar() && !author().isUser) {
          <div class="qt-chat-desktop-avatar">
            <qt-avatar [name]="author().name" [src]="author().avatarUrl" size="chat" />
          </div>
        }
        <div class="qt-chat-message-body group">
          <div class="qt-chat-message qt-chat-message-assistant">
            <qt-courier-bubble
              [chatId]="chat().id"
              [message]="message()"
              [characterName]="author().name"
              (settled)="courierSettled.emit($event)"
            />
          </div>
        </div>
      } @else {
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
            <qt-message-content [content]="message().content" [blobMountPointId]="blobMountPointId()" />
          }

          @if (imageAttachments().length > 0) {
            <div class="qt-chat-attachment-list">
              @for (att of imageAttachments(); track att.id) {
                <button
                  type="button"
                  class="qt-button qt-chat-attachment-button"
                  [title]="att.filename"
                  [attr.aria-label]="'View ' + att.filename"
                  (click)="onThumbnailClick(att)"
                >
                  <img
                    [src]="thumbFor(att)"
                    [alt]="att.filename"
                    width="80"
                    height="80"
                    class="qt-chat-attachment-image"
                  />
                  <div class="qt-chat-attachment-overlay">
                    <qt-icon name="zoom-in" class="w-4 h-4" />
                  </div>
                </button>
              }
            </div>
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
              @if (imageAttachments().length > 0) {
                <button
                  type="button"
                  class="qt-chat-message-action-icon"
                  [title]="
                    imageAttachments().length > 1
                      ? 'Save an image to a photo album'
                      : 'Save image to a photo album'
                  "
                  aria-label="Save image to a photo album"
                  (click)="onSaveImage()"
                >
                  <qt-icon name="bookmark" class="w-4 h-4" />
                </button>
              }
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
  /** An in-chat image thumbnail was clicked — open the lightbox (v4 `onImageClick`). */
  readonly imageClick = output<ImageClickEvent>();
  /** The action-bar Save button — open the save-to-album dialog (v4 `onSaveImage`). */
  readonly saveImage = output<{ messageId: string; attachmentId: string }>();
  /** The courier turn settled (resolved/cancelled) — trigger a chat refetch (v4). */
  readonly courierSettled = output<string>();

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

  /** A pending manual/clipboard turn renders the Courier bubble (v4). */
  protected readonly isCourier = computed(() => this.message().pendingExternalPrompt != null);

  /** The chat's blob mount point, threaded to the markdown img rewrite (dormant in v4). */
  protected readonly blobMountPointId = computed(() => this.chat().blobMountPointId ?? null);

  /** The message's image attachments (v4 `getImageAttachments`: image/* MIME). */
  protected readonly imageAttachments = computed(() =>
    (this.message().attachments || []).filter((a) => a.mimeType.startsWith('image/')),
  );

  protected thumbFor(att: MessageAttachment): string {
    return thumbnailUrl(att.id);
  }

  protected onThumbnailClick(att: MessageAttachment): void {
    this.imageClick.emit({ src: fileUrl(att.id), filename: att.filename, fileId: att.id });
  }

  /** Save the first image attachment (v4 MessageActionBar passes `images[0].id`). */
  protected onSaveImage(): void {
    const first = this.imageAttachments()[0];
    if (first) {
      this.saveImage.emit({ messageId: this.message().id, attachmentId: first.id });
    }
  }

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
