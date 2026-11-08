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
  type Signal,
} from '@angular/core';
import { injectVirtualizer } from '@tanstack/angular-virtual';

import {
  initialChatStreamState,
  type ChatStreamState,
  type ConcurrentStream,
  type StreamMessage,
} from '../core/chat-stream.reducer';
import type {
  ChatDetail,
  ChatSettingsDto,
  DetailCharacter,
  MessageDto,
  ResponseStatus,
} from '../core/core-contract';
import { AnnouncementGroup } from './announcement-group';
import { AutoScrollController } from './auto-scroll';
import { buildRenderItems, type RenderItem, type SwipeState } from './chat-view-model';
import { MessageRow, type ImageClickEvent } from './message-row';
import type { RegenerationState } from './regeneration.state';
import { ResponseStatusStrip } from './response-status-strip';
import type { DialogueDetection, RenderingPattern } from './render/roleplay-rendering';
import { StreamingMessage } from './streaming-message';
import { ToolMessage } from './tool-message';
import type { ConciergeRetryHandlers } from './concierge-retry';
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
  imports: [
    MessageRow,
    AnnouncementGroup,
    ResponseStatusStrip,
    StreamingMessage,
    ToolMessage,
    VirtualRow,
    Icon,
  ],
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
                    [conciergeRetry]="conciergeRetry()"
                    [regeneration]="regenerationFor(item.message)"
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
                  <qt-tool-message
                    [message]="item.message"
                    [chat]="chat()"
                    [onTryUncensored]="conciergeRetry()?.onRetryPicture"
                  />
                } @else {
                  <qt-announcement-group
                    [chips]="item.chips"
                    [chatId]="chat().id"
                    [chat]="chat()"
                    [conciergeRetry]="conciergeRetry()"
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
              [conciergeRetry]="conciergeRetry()"
              [renderingPatterns]="renderingPatterns()"
              [dialogueDetection]="dialogueDetection()"
              (copyMessage)="copyMessage.emit($event)"
              (imageClick)="imageClick.emit($event)"
              (saveImage)="saveImage.emit($event)"
              (courierSettled)="courierSettled.emit($event)"
            />
          } @else if (item.type === 'tool') {
            <qt-tool-message
                    [message]="item.message"
                    [chat]="chat()"
                    [onTryUncensored]="conciergeRetry()?.onRetryPicture"
                  />
          } @else {
            <qt-announcement-group
              [chips]="item.chips"
              [chatId]="chat().id"
              [chat]="chat()"
              [conciergeRetry]="conciergeRetry()"
              [renderingPatterns]="renderingPatterns()"
              [dialogueDetection]="dialogueDetection()"
            />
          }
        }

        <!-- The regeneration's own strip. Mounted only when there is no stream
             to carry it, so the two sites are exclusive (see the input's own
             note); the bubble owns the strip during an ordinary turn. -->
        @if (!stream() && regenerationStatus()) {
          <qt-response-status-strip [status]="regenerationStatus()" />
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

        <!-- === P4.D344: one bubble per concurrent ("all at once") storyteller
             turn, after the ordinary slot (v4 VirtualizedMessageList.tsx:421-437) === -->
        @for (entry of concurrentSlots(); track entry.participantId) {
          <qt-streaming-message
            [state]="entry.state"
            [renderingPatterns]="renderingPatterns()"
            [dialogueDetection]="dialogueDetection()"
            [showAvatar]="showAvatars()"
            [respondingCharacter]="entry.character"
            [isDangerousChat]="isDangerousChat()"
          />
        }
        <!-- === end P4.D344 === -->

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
  // === P4.D344: the concurrent storyteller turns' live text ===
  readonly concurrentStreams = input<ConcurrentStream[]>([]);
  // === end P4.D344 ===
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
  /**
   * "Try uncensored" (v4 `VirtualizedMessageList.conciergeRetry`, #77) — handed
   * to every row, every standalone TOOL row and every announcement group.
   * Absent on a Locked chat.
   */
  readonly conciergeRetry = input<ConciergeRetryHandlers | undefined>(undefined);
  /**
   * The in-flight regeneration, if any (v4 `VirtualizedMessageList`'s
   * `regeneration` prop, `f564b0de3`). It reaches EXACTLY the row whose id it
   * names; every other row is handed null and is untouched.
   *
   * v4 also threads an `onRegenerate` prop here, because its list wires the
   * action bar's refresh icon straight to it. v5's rows already emit a
   * `regenerate` output the list re-emits to the salon, so the door is the one
   * that was always there and only the state is new.
   */
  readonly regeneration = input<RegenerationState | null>(null);
  /**
   * The regeneration's stage line for the status strip (v4 feeds its ONE strip
   * `regenerationStatus ?? sseStreaming.responseStatus`, `SalonView.tsx:1648`).
   *
   * v5's strip lives inside the streaming bubble, which mounts only while a
   * turn is streaming — so the list renders the strip itself when there is a
   * regeneration and no stream. The two can never both be live: a send during
   * a regeneration is refused by the composer's lock, which is why v4's `??`
   * has no contested case and v5's two mount sites cannot both appear.
   */
  readonly regenerationStatus = input<ResponseStatus | null>(null);

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

  // === P4.D384 ===
  /**
   * Whether the initial scroll to the newest message has finished (v4
   * `useAutoScroll`'s `isInitialScrollDone`, delegated to the controller). The
   * Salon takes a `?msg=` reveal only once this is true — the initial scroll
   * would otherwise carry the view straight back down.
   */
  readonly isInitialScrollDone: Signal<boolean> = this.autoScroll.isInitialScrollDone;

  /** The in-flight reveal's animation frame (0 = none). */
  private revealFrame = 0;
  /** The pending highlight-removal timers, cleared on destroy. */
  private readonly revealHighlightTimers = new Set<ReturnType<typeof setTimeout>>();
  // === end P4.D384 ===

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
  // === P4.D344 ===
  /**
   * Each concurrent entry as a bubble state (v4: `streaming = content.length >
   * 0`, `waitingForResponse` its inverse, no tool batches, no reasoning) and its
   * character. The synthesized state is memoised per participant and reused
   * while its text is unchanged, so the input object stays stable across change
   * detection (NG0100).
   */
  private concurrentCache = new Map<string, { content: string; state: ChatStreamState }>();
  protected readonly concurrentSlots = computed(() => {
    const participants = this.chat().participants;
    const next = new Map<string, { content: string; state: ChatStreamState }>();
    const slots = this.concurrentStreams().map((c) => {
      let cached = this.concurrentCache.get(c.participantId);
      if (!cached || cached.content !== c.content) {
        cached = {
          content: c.content,
          state: {
            ...initialChatStreamState(),
            content: c.content,
            streaming: c.content.length > 0,
            waitingForResponse: c.content.length === 0,
            respondingParticipantId: c.participantId,
          },
        };
      }
      next.set(c.participantId, cached);
      return {
        participantId: c.participantId,
        state: cached.state,
        character: participants.find((p) => p.id === c.participantId)?.character ?? null,
      };
    });
    this.concurrentCache = next;
    return slots;
  });
  // === end P4.D344 ===

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

    inject(DestroyRef).onDestroy(() => {
      this.autoScroll.dispose();
      // === P4.D384: a reveal never outlives the list ===
      this.cancelReveal();
      for (const t of this.revealHighlightTimers) clearTimeout(t);
      this.revealHighlightTimers.clear();
      // === end P4.D384 ===
    });

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

  /**
   * Scroll a message into view by id (P4.D176 — the chat gallery's
   * Jump-to-message, v4 `lib/chat/message-navigation.ts::scrollToMessage`).
   *
   * v4's DOM is unvirtualized, so it can `querySelector` any message row
   * directly; v5's transcript is windowed (`@tanstack/angular-virtual` —
   * only the viewport + overscan rows are mounted), so the target row may not
   * exist in the DOM yet. This asks the virtualizer to scroll to the item's
   * INDEX first (mounting it), then centers it — the same two-step v4's own
   * `VirtualizedMessageList` uses internally for its own scroll-to-index
   * paths. A message folded into a collapsed announcement group scrolls to
   * that group's row instead (the group carries the chip's own id).
   *
   * A miss (the message is not in the current render-item array — deleted,
   * or belongs to a different chat) is a silent no-op, matching v4's
   * `querySelector` returning `null`.
   */
  scrollToMessage(messageId: string): void {
    // === P4.D384: the one index lookup, shared with revealMessage ===
    const index = this.renderIndexOf(messageId);
    // === end P4.D384 ===
    if (index === -1) return;
    this.virtualizer.scrollToIndex(index, { align: 'center', behavior: 'smooth' });
  }

  // === P4.D384 ===
  /**
   * Scroll a message to the top of the transcript and flash it — v4's `?msg=`
   * / in-conversation-search reveal (`SalonView.tsx` settle loop, `4f5047241`,
   * pinned at `624d0d25c`). The list is virtualized, so the row may not exist
   * yet: ask the virtualizer for the index (`align: 'start'`), then nudge until
   * the row's top sits 12px below the container's top, and flash it with
   * `qt-memory-source-highlight` for 3000ms.
   *
   * The loop is v4's, rule for rule:
   * - at most 40 attempts (frames), each re-asking the virtualizer while the row
   *   is not mounted;
   * - a kept-alive tab is `display:none` until it is activated, and a request can
   *   arrive in the same tick as the activation — while the container's
   *   `clientHeight` is 0 it waits (≤ 600 frames) WITHOUT spending an attempt;
   * - keep nudging while rows above are still being measured
   *   (`|offset| > 2`), until the row is at the top or the container can scroll
   *   no further (`scrollTop` stopped moving).
   *
   * Two v5 shapes, both recorded:
   * - v4 finds the row by `[data-message-id]`; v5's message rows carry
   *   `id="message-<id>"` (v4 stamps both; `message-row.ts` keeps the id only),
   *   which the chips carry too — so the lookup is by that id.
   * - A message inside a collapsed announcement group: v4 expands the chip first
   *   and reveals the message's own row; v5's expansion state is private to
   *   `AnnouncementGroup` (handoff H3), so this scrolls to and flashes the GROUP
   *   row (the chip's `.qt-chat-announcement-group`).
   *
   * A target not in the render-item array is a silent no-op (the caller — the
   * Salon — says why with a toast first). A new reveal cancels the one in
   * flight (v4's effect cleanup); destroy cancels both the loop and the flash.
   */
  revealMessage(messageId: string): void {
    this.cancelReveal();
    const index = this.renderIndexOf(messageId);
    if (index === -1) return;

    this.virtualizer.scrollToIndex(index, { align: 'start' });
    let attempts = 0;
    let lastScrollTop = -1;
    let hiddenFrames = 0;
    const settle = (): void => {
      this.revealFrame = 0;
      const container = this.scroll()?.nativeElement;
      if (!container || ++attempts > 40) return;
      // A kept-alive tab is display:none until it is activated, and a request
      // can arrive in the same tick as the activation: wait until it shows.
      if (container.clientHeight === 0) {
        if (++hiddenFrames > 600) return;
        attempts--;
        this.revealFrame = requestAnimationFrame(settle);
        return;
      }
      const row = this.revealRowOf(container, messageId);
      if (!row) {
        // v4 re-asks the same index, and its effect restarts the whole loop
        // whenever the render items change (a new line arrived); v5 has no
        // such effect, so it re-asks for the target's CURRENT index instead,
        // and stops if the target is gone.
        const current = this.renderIndexOf(messageId);
        if (current === -1) return;
        this.virtualizer.scrollToIndex(current, { align: 'start' });
        this.revealFrame = requestAnimationFrame(settle);
        return;
      }
      const offset = row.getBoundingClientRect().top - container.getBoundingClientRect().top - 12;
      // Keep nudging while rows above are still being measured, until the row
      // is at the top or the container can scroll no further.
      if (Math.abs(offset) > 2 && container.scrollTop !== lastScrollTop) {
        lastScrollTop = container.scrollTop;
        container.scrollTop += offset;
        this.revealFrame = requestAnimationFrame(settle);
        return;
      }
      row.classList.add('qt-memory-source-highlight');
      const timer = setTimeout(() => {
        this.revealHighlightTimers.delete(timer);
        row.classList.remove('qt-memory-source-highlight');
      }, 3000);
      this.revealHighlightTimers.add(timer);
    };
    this.revealFrame = requestAnimationFrame(settle);
  }

  /** Cancel the in-flight reveal loop, if any (the flash already shown stays). */
  private cancelReveal(): void {
    if (this.revealFrame) cancelAnimationFrame(this.revealFrame);
    this.revealFrame = 0;
  }

  /** The render-item index holding `messageId` (a message row, or the group whose chip it is). */
  private renderIndexOf(messageId: string): number {
    return this.items().findIndex((item) =>
      item.type === 'announcement-group'
        ? item.chips.some((chip) => chip.id === messageId)
        : item.message.id === messageId,
    );
  }

  /**
   * The mounted element to align and flash: the message's own row, or — for a
   * chip in a collapsed group — the group row the chip sits in.
   */
  private revealRowOf(container: HTMLElement, messageId: string): HTMLElement | null {
    // v4 builds a `CSS.escape`d selector; matching the id directly over the
    // mounted rows (the window is a handful) is the same lookup unescaped.
    const id = `message-${messageId}`;
    const anchor = [...container.querySelectorAll<HTMLElement>('[id^="message-"]')].find(
      (el) => el.id === id,
    );
    if (!anchor) return null;
    if (anchor.hasAttribute('data-message-id')) {
      return anchor.closest<HTMLElement>('.qt-chat-announcement-group') ?? anchor;
    }
    return anchor;
  }
  // === end P4.D384 ===

  protected swipeFor(message: MessageDto): SwipeState | null {
    return message.swipeGroupId ? (this.swipeStates()[message.swipeGroupId] ?? null) : null;
  }

  /**
   * The regeneration state for one row: the in-flight one when it names THIS
   * message, null otherwise (v4 `VirtualizedMessageList.tsx:334` —
   * `regeneration?.messageId === message.id ? regeneration : null`).
   */
  protected regenerationFor(message: MessageDto): RegenerationState | null {
    const regen = this.regeneration();
    return regen && regen.messageId === message.id ? regen : null;
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
    routeTrail: sm.routeTrail,
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
