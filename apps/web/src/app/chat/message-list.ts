import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  ElementRef,
  afterNextRender,
  computed,
  effect,
  inject,
  input,
  output,
  untracked,
  viewChild,
} from '@angular/core';
import { injectVirtualizer } from '@tanstack/angular-virtual';

import type { ChatStreamState, StreamMessage } from '../core/chat-stream.reducer';
import type { ChatDetail, ChatSettingsDto, MessageDto } from '../core/core-contract';
import { AnnouncementGroup } from './announcement-group';
import { AutoScrollController } from './auto-scroll';
import { buildRenderItems, type RenderItem, type SwipeState } from './chat-view-model';
import { MessageRow, type ImageClickEvent } from './message-row';
import { StreamingMessage } from './streaming-message';
import { Icon } from '../ui/icon';
import { VirtualRow } from './virtual-row';

/**
 * The scrolling message list — a port of v4's `VirtualizedMessageList` +
 * `useAutoScroll` (`app/salon/[id]/…`). The render-item array (message rows +
 * collapsed announcement groups) is windowed with `@tanstack/angular-virtual`
 * (the official Angular adapter over the exact virtual-core v4 uses): only the
 * viewport + overscan rows mount, so a 300-message chat pays a bounded render
 * cost instead of rendering every row through the markdown pipeline at once
 * (dogfood finding #3b). Row markdown is memoized ({@link
 * import('./render/render-cache')}) so re-entering rows re-mount as a cache hit.
 *
 * The scroll behavior — initial settle + one-time instant scroll-to-bottom,
 * stick-to-bottom intent, completion-gated auto-scroll, the jump-to-bottom
 * button — lives in {@link AutoScrollController}.
 */
@Component({
  selector: 'qt-message-list',
  // Stretch inside .qt-chat-messages-viewport so .qt-chat-messages (flex-1 +
  // overflow-y-auto) gets a BOUNDED height and actually scrolls — an unstyled
  // host breaks the flex chain (dogfood finding #3a). The virtualizer measures
  // its offsets against .qt-chat-messages, so that element must stay the bounded
  // scroll container.
  host: { class: 'flex flex-col flex-1 min-h-0' },
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [MessageRow, AnnouncementGroup, StreamingMessage, VirtualRow, Icon],
  template: `
    <div #scroll class="qt-chat-messages">
      <div class="qt-chat-messages-list">
        <!-- Total-size spacer: reserves the full scroll height; rows are absolutely
             positioned within it and translated to their measured offsets (v4's
             inline-styled rows, VirtualizedMessageList.tsx:188-293). The 1rem
             padding-bottom restores the inter-row gap the old space-y-4 gave, so
             each measured row height includes it. -->
        <div style="position:relative;width:100%" [style.height.px]="virtualizer.getTotalSize()">
          @for (row of virtualizer.getVirtualItems(); track row.key) {
            @let item = items()[row.index];
            @if (item) {
              <div
                style="position:absolute;top:0;left:0;width:100%;padding-bottom:1rem"
                [qtVirtualRow]="virtualizer"
                [attr.data-index]="row.index"
                [style.transform]="'translateY(' + row.start + 'px)'"
              >
                @if (item.type === 'message') {
                  <qt-message-row
                    [message]="item.message"
                    [chat]="chat()"
                    [swipeState]="swipeFor(item.message)"
                    [settings]="settings()"
                    [showAvatar]="showAvatars()"
                    [editing]="item.message.id === editingId()"
                    [hasLlmLogs]="messagesWithLogs().has(item.message.id)"
                    (viewLlmLogs)="viewLlmLogs.emit($event)"
                    (copy)="copy.emit($event)"
                    (edit)="edit.emit($event)"
                    (delete)="delete.emit($event)"
                    (regenerate)="regenerate.emit($event)"
                    (swipePrev)="swipePrev.emit($event)"
                    (swipeNext)="swipeNext.emit($event)"
                    (saveEdit)="saveEdit.emit($event)"
                    (cancelEdit)="cancelEdit.emit()"
                    (imageClick)="imageClick.emit($event)"
                    (saveImage)="saveImage.emit($event)"
                    (courierSettled)="courierSettled.emit($event)"
                  />
                } @else {
                  <qt-announcement-group [chips]="item.chips" [chatId]="chat().id" />
                }
              </div>
            }
          }
        </div>

        <!-- Stream-accumulated FINISHED bubbles (dogfood finding #7): a chained
             character's completed reply must be visible the instant its turn ends,
             not held back until the whole chain finishes and the canonical refetch
             lands. The reducer already folds each intermediate done / carina answer /
             host announcement into state.messages (v4 useSSEStreaming.ts
             onIntermediateDone/onCarinaAnswer/onHostAnnouncement :759-788,:684-691);
             we render them here, deduped by id against the canonical list, through
             the SAME MessageRow / AnnouncementGroup path so the reconcile handoff is
             pixel-stable. Not virtualized — a handful of transient rows. -->
        @for (item of streamItems(); track streamItemKey(item)) {
          @if (item.type === 'message') {
            <qt-message-row
              [message]="item.message"
              [chat]="chat()"
              [settings]="settings()"
              [showAvatar]="showAvatars()"
              (copy)="copy.emit($event)"
              (imageClick)="imageClick.emit($event)"
              (saveImage)="saveImage.emit($event)"
              (courierSettled)="courierSettled.emit($event)"
            />
          } @else {
            <qt-announcement-group [chips]="item.chips" [chatId]="chat().id" />
          }
        }

        @if (stream(); as s) {
          <qt-streaming-message [state]="s" />
        }

        <div #endAnchor></div>
      </div>
    </div>

    @if (autoScroll.showScrollToBottom()) {
      <button
        type="button"
        class="qt-chat-scroll-to-bottom"
        aria-label="Jump to latest message"
        title="Jump to latest message"
        (click)="autoScroll.scrollToBottom()"
      >
        <qt-icon name="chevron-down" class="w-5 h-5" />
      </button>
    }
  `,
})
export class MessageList {
  readonly messages = input.required<MessageDto[]>();
  readonly chat = input.required<ChatDetail>();
  readonly swipeStates = input<Record<string, SwipeState>>({});
  readonly settings = input<ChatSettingsDto | null>(null);
  readonly stream = input<ChatStreamState | null>(null);
  readonly editingId = input<string | null>(null);
  /**
   * The ids of messages that have LLM logs (v4 `messagesWithLogs`, threaded from
   * SalonView through to each row — `SalonView.tsx:1355`).
   *
   * The stream-accumulated rows below deliberately do NOT receive it: a bubble
   * that is still transient has no fetched logs to point at, and it hands off to
   * its canonical row (which does) on the reconcile refetch.
   */
  readonly messagesWithLogs = input<ReadonlySet<string>>(new Set<string>());

  readonly copy = output<MessageDto>();
  readonly edit = output<MessageDto>();
  readonly delete = output<MessageDto>();
  readonly regenerate = output<MessageDto>();
  readonly swipePrev = output<MessageDto>();
  readonly swipeNext = output<MessageDto>();
  readonly saveEdit = output<{ id: string; content: string }>();
  readonly cancelEdit = output<void>();
  readonly imageClick = output<ImageClickEvent>();
  readonly saveImage = output<{ messageId: string; attachmentId: string }>();
  readonly courierSettled = output<string>();
  /** A row's cpu icon — open the Inspector scrolled to that message (v4 `onViewLLMLogs`). */
  readonly viewLlmLogs = output<string>();

  private readonly scroll = viewChild<ElementRef<HTMLElement>>('scroll');
  private readonly endAnchor = viewChild<ElementRef<HTMLElement>>('endAnchor');

  protected readonly items = computed(() => buildRenderItems(this.messages()));

  /**
   * The stream-accumulated finished bubbles (assistant intermediate + carina +
   * host), deduped by id against the canonical flow so the post-reconcile rows
   * never double up (dogfood finding #7). Rendered through the normal render-item
   * path (announcement chips for staff senders, rows otherwise).
   */
  protected readonly streamItems = computed(() =>
    buildStreamRenderItems(this.stream(), this.messages()),
  );

  /** The virtualizer over the render-item array (v4 `useVirtualizer`). */
  protected readonly virtualizer = injectVirtualizer<HTMLElement, Element>(() => ({
    scrollElement: this.scroll()?.nativeElement,
    count: this.items().length,
    estimateSize: () => 150,
    overscan: 5,
    getItemKey: (index) => this.itemKey(this.items()[index]),
  }));

  protected readonly autoScroll = new AutoScrollController();

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
    // Attach the scroll controller once the container exists (post-render).
    afterNextRender(() => {
      const container = this.scroll()?.nativeElement;
      if (!container) return;
      this.autoScroll.bind(
        {
          container,
          end: this.endAnchor()?.nativeElement ?? null,
          virtualizer: this.virtualizer,
          itemCount: () => this.items().length,
          autoScrollOnComplete: () => this.settings()?.autoScrollOnResponseComplete ?? false,
        },
        this.messages().length,
      );
    });

    // Feed the controller its two v4 signals: the streaming flags and the flat
    // message count. Streaming NEVER scrolls per chunk — only completion (gated).
    effect(() => {
      const s = this.stream();
      this.autoScroll.notifyStreaming(s != null, s?.waitingForResponse ?? false);
    });
    effect(() => {
      const count = this.messages().length;
      const s = this.stream();
      this.autoScroll.notifyMessageCount(count, s != null, s?.waitingForResponse ?? false);
    });

    inject(DestroyRef).onDestroy(() => this.autoScroll.dispose());

    // Drive the virtualizer's window computation from a plain effect as well as
    // the adapter's own `afterRenderEffect`. `_willUpdate()` wires the scroll-rect
    // / scroll-offset observers the first time the container resolves (which is
    // what recomputes `range`); it is a guarded no-op once the scroll element is
    // unchanged, so the redundant call in the browser is free. This also makes
    // the list render its window in environments where afterRender hooks don't
    // fire (the jsdom unit-test harness).
    effect(() => {
      const el = this.scroll()?.nativeElement;
      this.items().length; // re-run when the render-item count changes
      if (el) untracked(() => this.virtualizer._willUpdate());
    });
  }

  /** Called by the conversation screen when the user sends — always scrolls + re-enables. */
  scrollOnUserMessage(): void {
    this.autoScroll.scrollOnUserMessage();
  }

  protected swipeFor(message: MessageDto): SwipeState | null {
    return message.swipeGroupId ? (this.swipeStates()[message.swipeGroupId] ?? null) : null;
  }

  protected itemKey(item: ReturnType<typeof buildRenderItems>[number]): string {
    return item.type === 'message' ? item.message.id : `grp-${item.chips[0]?.id ?? ''}`;
  }

  protected streamItemKey(item: RenderItem): string {
    return `stream-${this.itemKey(item)}`;
  }
}

/**
 * Turn one streamed {@link StreamMessage} into a {@link MessageDto} so it renders
 * through the exact MessageRow / AnnouncementGroup path the settled flow uses. A
 * carina/host entry already carries the full posted message object (v4 encodes
 * `{ carinaAnswer|hostAnnouncement: message }` — the serialized message), so it
 * casts straight across; an assistant bubble is assembled from the reducer's
 * fields (createdAt is left blank — the transient row carries no timestamp; the
 * canonical refetch supplies the real one on handoff).
 */
function streamMessageToMessageDto(sm: StreamMessage): MessageDto {
  if (sm.kind !== 'assistant') {
    return sm.message as unknown as MessageDto;
  }
  return {
    id: sm.id,
    role: 'ASSISTANT',
    content: sm.content,
    tokenCount: null,
    promptTokens: null,
    completionTokens: null,
    createdAt: '',
    swipeGroupId: null,
    swipeIndex: null,
    participantId: sm.participantId,
    attachments: [],
    provider: sm.provider,
    modelName: sm.modelName,
    targetParticipantIds: null,
    isSilentMessage: sm.isSilentMessage ?? null,
    systemSender: null,
    systemKind: null,
    hostEvent: null,
    customAnnouncer: null,
    carinaMeta: null,
    pendingExternalPrompt: null,
    pendingExternalPromptFull: null,
    pendingExternalAttachments: null,
    reasoningContent: sm.reasoningContent,
    reasoningSegments: sm.reasoningSegments,
    confirmed: sm.confirmed ?? undefined,
    confirmationChecked: sm.confirmationChecked,
    confirmationRevised: sm.confirmationRevised,
    confirmationNotes: sm.confirmationNotes,
  };
}

/**
 * Build the render items for the stream-accumulated finished bubbles, deduped by
 * id against `existing` (the canonical + optimistic flow). Skipped turns append
 * nothing to `stream.messages` (the reducer resets the buffer without a bubble —
 * `reduceDone` :297-299), so they naturally render nothing; their Host note
 * arrives as a separate `host` entry and DOES render.
 */
export function buildStreamRenderItems(
  stream: ChatStreamState | null,
  existing: MessageDto[],
): RenderItem[] {
  const streamMessages = stream?.messages ?? [];
  if (streamMessages.length === 0) {
    return [];
  }
  const existingIds = new Set(existing.map((m) => m.id));
  const dtos: MessageDto[] = [];
  for (const sm of streamMessages) {
    if (existingIds.has(sm.id)) {
      continue;
    }
    dtos.push(streamMessageToMessageDto(sm));
  }
  return buildRenderItems(dtos);
}
