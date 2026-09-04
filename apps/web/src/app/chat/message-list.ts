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
import type { ChatDetail, ChatSettingsDto, DetailCharacter, MessageDto } from '../core/core-contract';
import { AnnouncementGroup } from './announcement-group';
import { AutoScrollController } from './auto-scroll';
import { buildRenderItems, type RenderItem, type SwipeState } from './chat-view-model';
import { MessageRow, type ImageClickEvent } from './message-row';
import type { DialogueDetection, RenderingPattern } from './render/roleplay-rendering';
import { StreamingMessage } from './streaming-message';
import { ToolMessage } from './tool-message';
import { isOverheardWhisper } from './whisper-visibility';
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
  imports: [MessageRow, AnnouncementGroup, StreamingMessage, ToolMessage, VirtualRow, Icon],
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
                    [isOverheardWhisper]="overheard(item.message)"
                    [isDangerousChat]="isDangerousChat()"
                    [renderingPatterns]="renderingPatterns()"
                    [dialogueDetection]="dialogueDetection()"
                    (viewLlmLogs)="viewLlmLogs.emit($event)"
                    (copyMessage)="copyMessage.emit($event)"
                    (edit)="edit.emit($event)"
                    (delete)="delete.emit($event)"
                    (regenerate)="regenerate.emit($event)"
                    (swipePrev)="swipePrev.emit($event)"
                    (swipeNext)="swipeNext.emit($event)"
                    (saveEdit)="saveEdit.emit($event)"
                    (cancelEdit)="cancelEdit.emit()"
                    (imageClick)="imageClick.emit($event)"
                    (saveImage)="saveImage.emit($event)"
                    (reattribute)="reattribute.emit($event)"
                    (courierSettled)="courierSettled.emit($event)"
                  />
                } @else if (item.type === 'tool') {
                  <qt-tool-message [message]="item.message" [chat]="chat()" />
                } @else {
                  <qt-announcement-group
                    [chips]="item.chips"
                    [chatId]="chat().id"
                    [chat]="chat()"
                    [renderingPatterns]="renderingPatterns()"
                    [dialogueDetection]="dialogueDetection()"
                  />
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
              [isDangerousChat]="isDangerousChat()"
              [renderingPatterns]="renderingPatterns()"
              [dialogueDetection]="dialogueDetection()"
              (copyMessage)="copyMessage.emit($event)"
              (imageClick)="imageClick.emit($event)"
              (saveImage)="saveImage.emit($event)"
              (courierSettled)="courierSettled.emit($event)"
            />
          } @else if (item.type === 'tool') {
            <qt-tool-message [message]="item.message" [chat]="chat()" />
          } @else {
            <qt-announcement-group
              [chips]="item.chips"
              [chatId]="chat().id"
              [chat]="chat()"
              [renderingPatterns]="renderingPatterns()"
              [dialogueDetection]="dialogueDetection()"
            />
          }
        }

        @if (stream(); as s) {
          <qt-streaming-message
            [state]="s"
            [renderingPatterns]="renderingPatterns()"
            [dialogueDetection]="dialogueDetection()"
            [showAvatar]="showAvatars()"
            [respondingCharacter]="respondingCharacter()"
            [isDangerousChat]="isDangerousChat()"
          />
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
   * The chat's roleplay-template rendering patterns and dialogue detection,
   * fetched by the conversation screen (v4 `SalonView.tsx:745-776`) and handed
   * to every rendered row — v4 passes the identical pair at its two call sites,
   * `VirtualizedMessageList.tsx:314-315` (each MessageRow) and `:387-388` (the
   * streaming bubble). Undefined means the chat has no template, or its template
   * could not be read; the renderer then falls back to the built-in defaults.
   */
  readonly renderingPatterns = input<RenderingPattern[] | undefined>(undefined);
  readonly dialogueDetection = input<DialogueDetection | null | undefined>(undefined);
  /**
   * The ids of messages that have LLM logs (v4 `messagesWithLogs`, threaded from
   * SalonView through to each row — `SalonView.tsx:1355`).
   *
   * The stream-accumulated rows below deliberately do NOT receive it: a bubble
   * that is still transient has no fetched logs to point at, and it hands off to
   * its canonical row (which does) on the reconcile refetch.
   */
  readonly messagesWithLogs = input<ReadonlySet<string>>(new Set<string>());
  /**
   * The participant ids the human controls (v4 SalonView `userParticipantIdSet`),
   * for the overheard-whisper dim (v4 `VirtualizedMessageList.tsx:358-373`).
   */
  readonly userParticipantIds = input<ReadonlySet<string>>(new Set<string>());
  /**
   * The Salon's `shouldShowDangerStyling(chat)` verdict, threaded straight
   * through to every row (v4 `VirtualizedMessageList.tsx:106` prop, `:165`
   * default false, `:368` → `MessageRow`). The list only forwards it — the
   * predicate lives in `chat/concierge-state.ts` and is applied at the wiring
   * site, exactly as v4 applies it in `SalonView.tsx:1489`.
   */
  readonly isDangerousChat = input(false);

  readonly copyMessage = output<MessageDto>();
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
  /** A row's swap icon — open `ReattributeMessageDialog` for that message. */
  readonly reattribute = output<MessageDto>();
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

  /**
   * v4 `SalonView.tsx:1171-1174 shouldShowAvatars` — the ONE avatar gate, used
   * for the settled rows (`VirtualizedMessageList.tsx:274`, `:305`) AND the
   * streaming bubble (`:383`):
   *
   * ```ts
   * if (!chatSettings) return true
   * return chatSettings.avatarDisplayMode === 'ALWAYS'
   * ```
   *
   * ⚠ **GROUP_ONLY means NO avatars, not "avatars in a group".** v5 used to
   * read it as "≥2 characters"; measured against the pin, v4 consumes
   * `avatarDisplayMode` at exactly one site — this one — and tests only for
   * `'ALWAYS'`, and v4's own settings copy calls the mode "(will be implemented
   * in the future)" (`components/settings/chat-settings/types.ts:266`). The
   * ≥2 rule was a v5 invention implementing a feature v4 has not built; it is
   * gone (P4.75, the order's own "transcribe both, do not merge them" premise
   * refuted by measurement — v4 has ONE rule at BOTH sites).
   */
  protected readonly showAvatars = computed(() => {
    const settings = this.settings();
    if (!settings) return true;
    return settings.avatarDisplayMode === 'ALWAYS';
  });

  /**
   * v4 `SalonView.tsx:1176-1184 getRespondingCharacter` — who the LIVE bubble's
   * avatar column names.
   *
   * The id arm looks `respondingParticipantId` up in the cast and takes that
   * participant's character whatever its type or active state; the fallback is
   * v4's `getFirstCharacter()` (`useParticipants.ts:227,235`) — the first
   * participant that is BOTH `type === 'CHARACTER'` and `isActive`, whose
   * character may still be absent, in which case v4's Avatar renders the 'AI'
   * fallback name.
   */
  protected readonly respondingCharacter = computed<DetailCharacter | null>(() => {
    const participants = this.chat().participants;
    const id = this.stream()?.respondingParticipantId ?? null;
    if (id) {
      const named = participants.find((p) => p.id === id);
      if (named?.character) return named.character;
    }
    return participants.find((p) => p.type === 'CHARACTER' && p.isActive)?.character ?? null;
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

  protected overheard(message: MessageDto): boolean {
    return isOverheardWhisper(message, this.userParticipantIds());
  }

  protected itemKey(item: ReturnType<typeof buildRenderItems>[number]): string {
    return item.type === 'announcement-group' ? `grp-${item.chips[0]?.id ?? ''}` : item.message.id;
  }

  protected streamItemKey(item: RenderItem): string {
    return `stream-${this.itemKey(item)}`;
  }
}

/**
 * Turn one streamed {@link StreamMessage} into a {@link MessageDto} so it renders
 * through the exact MessageRow / AnnouncementGroup path the settled flow uses. A
 * carina/host/pascal entry already carries the full posted message object (v4
 * encodes `{ carinaAnswer|hostAnnouncement|pascalResult: message }` — the
 * serialized message, `pascalMeta` and all), so it casts straight across; an
 * assistant bubble is assembled from the reducer's fields (createdAt is left
 * blank — the transient row carries no timestamp; the canonical refetch supplies
 * the real one on handoff).
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
    // Absent on every live frame (v4's confirmationResult never carries the
    // pre-revision text — it arrives with the canonical refetch); mapped so the
    // badge's five-field family survives the stream→bubble hop whole (P4.D132).
    confirmationOriginalContent: sm.confirmationOriginalContent,
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
