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
import {
  getAnnouncementAccentClasses,
  getAnnouncementImportance,
  getAnnouncementOutcomeState,
  getSystemKindDisplayLabel,
  getSystemSenderDisplayName,
} from './system-message-labels';
import { ThinkingBlock } from './thinking-block';
import { TokenBadge } from './token-badge';
import { ToolMessage } from './tool-message';

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
  imports: [Avatar, Icon, CourierBubble, MessageContent, ThinkingBlock, TokenBadge, ToolMessage],
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
      @if (showAvatar() && !author().isUser && !isPascal()) {
        <div class="qt-chat-desktop-avatar">
          <qt-avatar [name]="author().name" [src]="author().avatarUrl" size="chat" />
        </div>
      }

      <div class="qt-chat-message-body">
        @if (isPascal()) {
          <!-- Pascal's roll outcome: the full-row header bar (v4's expanded
               systemSender header — a static, non-collapsing bar here since the
               row is always shown). -->
          <div
            class="qt-chat-system-bar qt-chat-system-bar-expanded qt-chat-system-bar-static"
            [class]="pascalAccentClasses()"
          >
            <span class="qt-chat-announcement-dot" [class]="pascalDotClass()"></span>
            <span class="qt-chat-system-bar-sender">{{ pascalSender() }}</span>
            <!-- The state is carried by colour alone in the bar; name it for
                 readers who can't see the accent. -->
            @if (pascalOutcomeState(); as state) {
              <span class="sr-only">{{ state }}</span>
            }
            @if (pascalKind()) {
              <span class="qt-chat-system-bar-kind">{{ pascalKind() }}</span>
            }
            <span class="qt-chat-system-bar-time">{{ timestamp() }}</span>
          </div>
        } @else {
          <div class="qt-chat-message-header">
            <span class="qt-chat-message-author">{{ author().name }}</span>
            @if (author().title) {
              <span class="qt-chat-message-time">{{ author().title }}</span>
            }
          </div>
        }

        <div class="qt-chat-message" [class]="bubbleClass()">
          @if (variant() === 'whisper') {
            <div class="qt-chat-whisper-label">whispered to {{ whisperTargets() }}</div>
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

          @if (!editing() && attachedToolMessages().length > 0) {
            <!-- Character-initiated tool results folded into this bubble (v4
                 MessageRow trailing-tools block :428-440) — each renders embedded
                 so the calls read as separate flourishes under the character's
                 prose. -->
            <div class="qt-chat-message-tools">
              @for (tm of attachedToolMessages(); track tm.id) {
                <qt-tool-message [embedded]="true" [message]="tm" [chat]="chat()" />
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
              @if (showLlmLogsAction()) {
                <button
                  type="button"
                  class="qt-chat-message-action-icon"
                  title="View LLM request/response logs"
                  aria-label="View LLM request/response logs"
                  (click)="viewLlmLogs.emit(message().id)"
                >
                  <qt-icon name="cpu" class="w-4 h-4" />
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
            <!-- v4 MessageActionBar.tsx:195 — the timestamp row. The class already
                 carries ml-auto (and the user-bubble color variant this markup
                 finally activates), so it replaces the bare time span v5 had. -->
            <div class="qt-chat-message-action-timestamp flex items-center gap-2">
              <span>{{ timestamp() }}</span>
              @if (showTokenBadge()) {
                <qt-token-badge
                  [promptTokens]="message().promptTokens"
                  [completionTokens]="message().completionTokens"
                  [totalTokens]="message().tokenCount"
                  [showTokens]="tokenDisplay().showPerMessageTokens"
                  [showCost]="tokenDisplay().showPerMessageCost"
                />
              }
            </div>
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
  /**
   * Whether this message has LLM logs (v4 `hasLLMLogs` — the Salon derives it
   * from `messagesWithLogs.has(message.id)`).
   */
  readonly hasLlmLogs = input(false);

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
  /**
   * Open the LLM Inspector scrolled to this message's logs (v4 `onViewLLMLogs`).
   * Emits the message id, as v4 passes it.
   */
  readonly viewLlmLogs = output<string>();

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

  /**
   * A Pascal roll outcome renders as its own full row with a header bar (v4
   * `MessageRow`'s expanded systemSender header — dot · Pascal · toolTitle ·
   * time), NOT a collapsed chip (`isAnnouncementChip` carves it out). The
   * body is the normal markdown pipeline. Pascal has no participant, so the
   * author-header/avatar path (which falls back to a character name) is
   * suppressed in favour of this bar.
   */
  protected readonly isPascal = computed(() => this.message().systemSender === 'pascal');
  /** "Pascal". */
  protected readonly pascalSender = computed(() =>
    getSystemSenderDisplayName(this.message().systemSender),
  );
  /** The tool title (`toolTitle ?? tool`) — v4's header chip subject. */
  protected readonly pascalKind = computed(() => getSystemKindDisplayLabel(this.message()));
  /**
   * The outcome the roll landed on (v4 `getAnnouncementOutcomeState`,
   * `231be14c`), or null for a record that predates the field / carries a state
   * this build doesn't know.
   */
  protected readonly pascalOutcomeState = computed(() =>
    getAnnouncementOutcomeState(this.message()),
  );
  /**
   * The dot class. A roll wears the outcome's OWN state; without a usable one it
   * falls back to the importance tier — which for Pascal is always `high`, i.e.
   * the same red a deleted file gets. That mismatch is what `231be14c` fixed.
   */
  protected readonly pascalDotClass = computed(() => {
    const state = this.pascalOutcomeState();
    return state
      ? `qt-chat-announcement-dot-outcome-${state}`
      : `qt-chat-announcement-dot-${getAnnouncementImportance(this.message())}`;
  });
  /** The `qt-pascal-result` accent on the bar's leading edge, else `''`. */
  protected readonly pascalAccentClasses = computed(() =>
    getAnnouncementAccentClasses(this.message()),
  );

  /**
   * The chat-settings token-display bag (v4 threads this down as the
   * `tokenDisplaySettings` prop). All four flags default false — v4's Zod
   * defaults (`lib/schemas/settings.types.ts:264-273`).
   *
   * Two of the four flags are consumed here; the other two are inert BY DESIGN,
   * and both deserve their why-comment rather than a future reader "fixing" them:
   *
   *  - **`showPerMessageCost` is dead in v4.** It is threaded to `TokenBadge`
   *    below exactly as v4 does, but no cost ever renders: there is no cost
   *    field on the Message type for the badge's `estimatedCostUSD` to come
   *    from (see the TokenBadge class docs). Ported dead, not invented.
   *  - **`showSystemEvents` is inert in v4.** Declared, parsed, defaulted — and
   *    read by NO renderer anywhere in v4. There is deliberately no consumer
   *    here either.
   *
   * `showChatTotals` is the header summary's, not this row's.
   */
  protected readonly tokenDisplay = computed(
    () =>
      this.settings()?.tokenDisplaySettings ?? {
        showPerMessageTokens: false,
        showPerMessageCost: false,
        showChatTotals: false,
        showSystemEvents: false,
      },
  );

  /**
   * v4 `MessageActionBar.tsx:197` —
   * `showPerMessageTokens && (promptTokens || completionTokens)`. Note the gate
   * reads the TOKENS flag only: with `showPerMessageCost` on but
   * `showPerMessageTokens` off, v4 never mounts the badge at all (the second
   * reason per-message cost is unreachable).
   *
   * The counts are JS-truthy-tested, so a message whose usage came back as an
   * explicit zero is treated as having nothing to show.
   *
   * DIVERGENCE (deliberate, cosmetic): v4's gate is a JSX `&&`-chain, so when
   * the flag is on and both counts are numeric `0`, the chain's value is `0` and
   * React renders a literal "0" text node next to the timestamp. That is a
   * React-idiom bug, not a designed behavior; `@if` evaluates the same condition
   * but renders nothing when it is falsy. We match v4's INTENT (and its output
   * for every non-zero case) rather than reproducing a stray glyph.
   */
  protected readonly showTokenBadge = computed(() => {
    if (!this.tokenDisplay().showPerMessageTokens) return false;
    const m = this.message();
    return !!(m.promptTokens || m.completionTokens);
  });

  /**
   * The per-message "View LLM logs" entry (v4 `hasLLMLogs && message.role ===
   * 'ASSISTANT' && onViewLLMLogs`). Only an assistant turn has a request/response
   * pair to inspect; a user message never does.
   *
   * TITLE CHOICE: v4 has TWO action bars with DIFFERENT copy for this one button
   * — `MessageActionBar.tsx:153` ("View LLM request/response logs") and
   * `MessageDesktopActions.tsx:73` ("View LLM logs"). v5 has a single bar, and it
   * is MessageActionBar's: same `qt-chat-message-action-bar` /
   * `qt-chat-message-action-icon` classes, same hover-reveal placement, same
   * sibling buttons. So it carries MessageActionBar's copy.
   */
  protected readonly showLlmLogsAction = computed(
    () => this.hasLlmLogs() && this.message().role === 'ASSISTANT',
  );

  /** The chat's blob mount point, threaded to the markdown img rewrite (dormant in v4). */
  protected readonly blobMountPointId = computed(() => this.chat().blobMountPointId ?? null);

  /** The message's image attachments (v4 `getImageAttachments`: image/* MIME). */
  protected readonly imageAttachments = computed(() =>
    (this.message().attachments || []).filter((a) => a.mimeType.startsWith('image/')),
  );

  /**
   * Character-initiated TOOL rows folded into this assistant by
   * `groupToolMessagesIntoAssistants` — rendered embedded below the prose (v4
   * MessageRow's trailing-tools block). Empty on every non-host message.
   */
  protected readonly attachedToolMessages = computed(() => this.message().attachedToolMessages ?? []);

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

  /**
   * The whisper label's target names (v4 `MessageRow.tsx:321-327` +
   * `participantNames` `SalonView.tsx:181`): each `targetParticipantId` mapped to
   * its participant's character name, or "unknown", comma-joined. Replaces the
   * former hardcoded "Private whisper" so the operator sees who a private line
   * went to.
   */
  protected readonly whisperTargets = computed(() => {
    const ids = this.message().targetParticipantIds ?? [];
    const participants = this.chat().participants;
    return ids
      .map((id) => participants.find((p) => p.id === id)?.character?.name || 'unknown')
      .join(', ');
  });

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
