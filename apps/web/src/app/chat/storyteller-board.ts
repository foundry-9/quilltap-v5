import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
  untracked,
} from '@angular/core';
import { injectQuery, QueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient, coreErrorMessage } from '../core/core-client';
import {
  TIER_LABELS,
  type ParticipantDetail,
  type StorytellerBid,
  type StorytellerOrder,
} from '../core/core-contract';
import { RealtimeService } from '../core/realtime.service';
import { Avatar } from '../ui/avatar';
import { normalizeAvatarSrc } from '../ui/avatar-stack';
import { Icon } from '../ui/icon';
import { ToastService } from '../ui/toast.service';
import { chatKeys } from './chat-keys';
import {
  arrangeBids,
  cardClass,
  moveId,
  chosenInBoardOrder,
  ordinal,
  selectAllIds,
  statusLine,
  summaryText,
  TIER_BADGE,
  toggleSelection,
} from './storyteller-board.pure';

/** Cadence of the offline fallback poll, in ms (v4 `FALLBACK_POLL_MS`). */
const FALLBACK_POLL_MS = 60_000;

/** What the board asks the Salon to run (v4 `onRun`'s four arguments). */
export interface StorytellerRunRequest {
  roundId: string;
  participantIds: string[];
  order: StorytellerOrder;
  keepOrder: boolean;
}

/**
 * StorytellerBoard — the round's bids, laid out for the operator's choosing
 * (v4 `components/chat/StorytellerBoard.tsx` at the pin `236c04e35`).
 *
 * Shown above the composer while a storyteller round is open: one card per
 * invited seat, filled in as the bids arrive (live off the send's stream — the
 * Salon writes those frames into this board's query cache — and through the
 * `storytellerRounds` realtime topic for anything that lands after the stream
 * has closed). The operator ticks the bids that go ahead and runs them in order
 * or all at once, or lets the round lie. Once the round has run the board folds
 * into one line naming who was chosen, until the next round.
 *
 * A Destroy bid wears the strongest warning and must be chosen on its own;
 * "Select all" never includes it (D7).
 *
 * v5 differences, each recorded in the P4.D344 lane record:
 *  - R-L / R-1: no drag. v4 sets the running order by dragging the cards
 *    (`@dnd-kit`); v5 reaches the same `arrangement` by each live card's
 *    arrows (buttons, and ArrowUp/Left / ArrowDown/Right on the focused card).
 *    Untouched, the room's rotation sets the order (`keepOrder: false`); once
 *    arranged, the order badges, `Reset order` and `keepOrder: true` follow as
 *    in v4. The unarranged hint is v5's own sentence — v4's promises dragging
 *    (`BOARD_ARRANGE_HINT_WITHOUT_DRAG`).
 *  - R-5: v4 disables the buttons for `busy || running`, `running` a local flag
 *    around its awaited `onRun`. Here the Salon owns the run and its `busy()`
 *    is true for the whole of it, so `busy` alone is the running state.
 *  - v4's `ask` / `lie` mutations are dispatches by variant (every SPA lane's
 *    rule); `askPending` / `liePending` stand in for `isPending`.
 */
@Component({
  selector: 'qt-storyteller-board',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Avatar, Icon],
  // R-12: a Tailwind utility, never an unruled `qt-*` host class.
  host: { class: 'block' },
  template: `
    @if (round(); as r) {
      @if (r.status !== 'open') {
        <div class="qt-storyteller-summary" role="status">
          <qt-icon name="users" class="w-3.5 h-3.5" />
          <span>{{ summary() }}</span>
        </div>
      } @else {
        <section class="qt-storyteller-board" aria-label="The storyteller's board">
          <div class="qt-storyteller-board-header">
            <span
              >The company bids@if (pending() > 0) {<span class="qt-storyteller-meta">
                  — {{ pending() }} still considering…</span
                >}</span
            >
            <span class="qt-storyteller-board-tools">
              @if (arranged()) {
                <button
                  type="button"
                  class="qt-button-ghost qt-button-sm"
                  [disabled]="disabled()"
                  title="Put the cards back and let the room's rotation set the order"
                  (click)="arrangement.set(null)"
                >
                  Reset order
                </button>
              }
              <button
                type="button"
                class="qt-button-ghost qt-button-sm"
                [disabled]="disabled() || biddable().length === 0"
                (click)="selectAll()"
              >
                Select all
              </button>
            </span>
          </div>

          <div class="qt-storyteller-cards">
            @for (bid of cards(); track bid.participantId) {
              @let isSelected = selected().has(bid.participantId);
              @let line = statusLineOf(bid);
              @let runPosition = runPositionOf(bid, isSelected);
              @let movable = bid.status === 'bid' && !disabled();
              <div
                [attr.class]="cardClassOf(bid, isSelected)"
                [attr.tabindex]="movable ? 0 : null"
                (keydown)="onCardKey($event, bid)"
              >
                <div class="qt-storyteller-card-head">
                  @if (runPosition > 0) {
                    <span class="qt-storyteller-card-order" [title]="'Speaks ' + ordinalOf(runPosition)">{{
                      runPosition
                    }}</span>
                  }
                  @if (bid.status === 'bid') {
                    <input
                      type="checkbox"
                      [checked]="isSelected"
                      [disabled]="disabled()"
                      [attr.aria-label]="'Let ' + bid.characterName + ' go ahead'"
                      (change)="toggle(bid)"
                    />
                  }
                  <qt-avatar [name]="bid.characterName" [src]="avatarFor(bid.participantId)" size="xs" />
                  <span class="qt-storyteller-card-name" [title]="bid.characterName">{{
                    bid.characterName
                  }}</span>
                  @if (bid.status === 'bid' && bid.tier) {
                    <span [attr.class]="'qt-badge ' + tierBadge[bid.tier]">{{ tierLabels[bid.tier] }}</span>
                  }
                </div>

                @if (bid.status === 'bid' && bid.plan) {
                  <div class="qt-storyteller-plan">{{ bid.plan }}</div>
                }
                @if (line) {
                  <div class="qt-storyteller-meta">{{ line }}</div>
                }

                @if (bid.status === 'bid' && bid.tier === 'destroy') {
                  <div class="qt-storyteller-warning">
                    This bid means to delete something, and cannot be undone. It goes ahead alone.
                  </div>
                }
                @if (bid.status === 'bid' && bid.tierRaised && bid.statedTier && bid.tier) {
                  <div class="qt-storyteller-meta">
                    Asked for {{ tierLabels[bid.statedTier] }}, named {{ bid.tier === 'act' ? 'an' : 'a' }}
                    {{ tierLabels[bid.tier] }} tool.
                  </div>
                }
                @if (bid.status === 'bid' && bid.tools.length > 0) {
                  <div class="qt-storyteller-meta">Tools: {{ bid.tools.join(', ') }}</div>
                }
                @if (bid.status === 'bid' && bid.reason) {
                  <div class="qt-storyteller-meta">{{ bid.reason }}</div>
                }

                <!-- The arrows sit on the foot line, not the head: in the head
                     they squeezed the name to one letter at a phone width. -->
                <div class="flex items-center justify-between gap-2">
                  <div class="qt-storyteller-meta">
                    Talkativeness {{ bid.talkativeness.toFixed(2) }}{{ bid.silent ? ' · silent' : '' }}
                  </div>
                  @if (movable) {
                    <span class="flex items-center gap-1">
                      <!-- R-1: v4 drags the card (dnd-kit); v5 moves it by button
                           or arrow key — the same arrangement state, no dependency. -->
                      <button
                        type="button"
                        class="qt-button-ghost qt-button-sm"
                        [attr.aria-label]="'Move ' + bid.characterName + ' earlier'"
                        [title]="'Move ' + bid.characterName + ' earlier'"
                        (click)="move(bid, -1)"
                      >
                        <qt-icon name="arrow-left" class="w-3.5 h-3.5" />
                      </button>
                      <button
                        type="button"
                        class="qt-button-ghost qt-button-sm"
                        [attr.aria-label]="'Move ' + bid.characterName + ' later'"
                        [title]="'Move ' + bid.characterName + ' later'"
                        (click)="move(bid, 1)"
                      >
                        <qt-icon name="arrow-right" class="w-3.5 h-3.5" />
                      </button>
                    </span>
                  }
                </div>

                @if (bid.status === 'kept-quiet' || bid.status === 'error') {
                  <button
                    type="button"
                    class="qt-button-secondary qt-button-sm"
                    [disabled]="disabled() || askPending()"
                    (click)="ask(bid.participantId)"
                  >
                    Ask anyway
                  </button>
                }
              </div>
            }
          </div>

          @if (biddable().length > 1) {
            <div class="qt-storyteller-meta">
              {{
                arranged()
                  ? 'Chosen bids run in the order shown.'
                  : 'Use a card’s arrows to set the running order; left alone, the room’s rotation decides.'
              }}
            </div>
          }

          @if (destroyMixed()) {
            <div class="qt-storyteller-warning">A Destroy bid must be chosen on its own.</div>
          }

          <div class="qt-storyteller-actions">
            <button
              type="button"
              class="qt-button-primary qt-button-sm"
              [disabled]="disabled() || chosenBids().length === 0 || destroyMixed()"
              (click)="onRun('sequential')"
            >
              Run in order
            </button>
            <button
              type="button"
              class="qt-button-secondary qt-button-sm"
              [disabled]="disabled() || chosenBids().length < 2 || destroyMixed()"
              title="Simple and Lookup bids speak at once; Act and Destroy bids follow one at a time"
              (click)="onRun('simultaneous')"
            >
              All at once
            </button>
            <button
              type="button"
              class="qt-button-ghost qt-button-sm"
              [disabled]="disabled() || liePending()"
              (click)="lie()"
            >
              Let it lie
            </button>
          </div>
        </section>
      }
    }
  `,
})
export class StorytellerBoard {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);
  private readonly queryClient = inject(QueryClient);
  private readonly realtime = inject(RealtimeService);

  readonly chatId = input.required<string>();
  /** The room is speaking — the board's buttons wait (and, R-5, the run is in flight). */
  readonly busy = input(false);
  /** The chat's seats, for each card's avatar (v4 `avatarFor`). */
  readonly participants = input<ParticipantDetail[]>([]);

  /** Run the chosen bids. The Salon streams them through its own transport. */
  readonly run = output<StorytellerRunRequest>();

  protected readonly tierLabels = TIER_LABELS;
  protected readonly tierBadge = TIER_BADGE;

  private readonly roundQuery = injectQuery(() => ({
    queryKey: chatKeys.storytellerRound(this.chatId()),
    queryFn: async () =>
      (
        await this.core.dispatchExpect(
          { type: 'chatStorytellerRound', chatId: this.chatId() },
          'storytellerRound',
        )
      ).data,
    enabled: !!this.chatId(),
    refetchInterval: this.realtime.refetchInterval(FALLBACK_POLL_MS),
  }));

  protected readonly round = computed(() => this.roundQuery.data()?.round ?? null);

  /** The ticked seats (participant ids). Per round — reset when the round changes. */
  protected readonly selected = signal<ReadonlySet<string>>(new Set());
  /** The operator's hand-set card order (participant ids); null until they arrange it. */
  protected readonly arrangement = signal<string[] | null>(null);
  protected readonly askPending = signal(false);
  protected readonly liePending = signal(false);

  protected readonly cards = computed(() => arrangeBids(this.round()?.bids ?? [], this.arrangement()));
  protected readonly biddable = computed(() => this.cards().filter((b) => b.status === 'bid'));
  /** In board order, which is the run order once the operator has arranged it. */
  protected readonly chosenBids = computed(() => chosenInBoardOrder(this.cards(), this.selected()));
  protected readonly arranged = computed(() => this.arrangement() !== null);
  private readonly destroyChosen = computed(() => this.chosenBids().some((b) => b.tier === 'destroy'));
  protected readonly destroyMixed = computed(() => this.destroyChosen() && this.chosenBids().length > 1);
  protected readonly pending = computed(
    () => (this.round()?.bids ?? []).filter((b) => b.status === 'pending').length,
  );
  protected readonly disabled = computed(() => this.busy());
  protected readonly summary = computed(() => {
    const r = this.round();
    return r ? summaryText(r) : '';
  });

  constructor() {
    // A new round starts with nothing ticked and the cards in their own order
    // (v4 `:191-195`). Keyed on the round's ID, so a bid patch into the same
    // round keeps the ticks.
    let lastRoundId: string | null | undefined;
    effect(() => {
      const id = this.round()?.id ?? null;
      if (id === lastRoundId) return;
      lastRoundId = id;
      untracked(() => {
        this.selected.set(new Set());
        this.arrangement.set(null);
      });
    });
  }

  protected statusLineOf(bid: StorytellerBid): string | null {
    return statusLine(bid);
  }

  protected cardClassOf(bid: StorytellerBid, isSelected: boolean): string {
    // `draggable` / `dragging` are never emitted in v5 (R-L).
    return cardClass(bid, isSelected, false, false);
  }

  protected ordinalOf(n: number): string {
    return ordinal(n);
  }

  /** The order badge's number — the card's place among the chosen bids, once arranged. */
  protected runPositionOf(bid: StorytellerBid, isSelected: boolean): number {
    if (!this.arranged() || !isSelected) return 0;
    return this.chosenBids().findIndex((b) => b.participantId === bid.participantId) + 1;
  }

  protected avatarFor(participantId: string): string | null {
    const seat = this.participants().find((p) => p.id === participantId);
    return normalizeAvatarSrc(seat?.character?.avatarUrl) ?? null;
  }

  /**
   * Move a card one place (v4 `handleDragEnd`'s `arrayMove` over the card ids,
   * `:229-237`) — the keyboard / button twin of v4's drag (R-1).
   */
  protected move(bid: StorytellerBid, delta: -1 | 1): void {
    const ids = this.cards().map((b) => b.participantId);
    const from = ids.indexOf(bid.participantId);
    const to = from + delta;
    if (from === -1 || to < 0 || to >= ids.length) return;
    this.arrangement.set(moveId(ids, from, to));
  }

  /** v4's `KeyboardSensor` twin: ArrowUp / ArrowLeft earlier, ArrowDown / ArrowRight later. */
  protected onCardKey(event: KeyboardEvent, bid: StorytellerBid): void {
    if (bid.status !== 'bid' || this.disabled()) return;
    if (event.target !== event.currentTarget) return; // the checkbox / buttons keep their keys
    const delta = event.key === 'ArrowUp' || event.key === 'ArrowLeft' ? -1
      : event.key === 'ArrowDown' || event.key === 'ArrowRight' ? 1 : 0;
    if (delta === 0) return;
    event.preventDefault();
    this.move(bid, delta);
  }

  protected toggle(bid: StorytellerBid): void {
    this.selected.set(toggleSelection(this.selected(), bid, this.biddable()));
  }

  protected selectAll(): void {
    this.selected.set(selectAllIds(this.biddable()));
  }

  protected onRun(order: StorytellerOrder): void {
    const r = this.round();
    const chosen = this.chosenBids();
    if (!r || chosen.length === 0) return;
    this.run.emit({
      roundId: r.id,
      participantIds: chosen.map((b) => b.participantId),
      order,
      keepOrder: this.arranged(),
    });
  }

  protected async ask(participantId: string): Promise<void> {
    const chatId = this.chatId();
    const roundId = this.round()?.id;
    if (!roundId) return;
    this.askPending.set(true);
    try {
      await this.core.dispatchExpect(
        { type: 'chatStorytellerAsk', chatId, roundId, participantId },
        'storytellerAsk',
      );
    } catch (error) {
      // v4 `apiErrorMessage(error, 'Could not ask for that bid')`.
      this.toasts.showError(coreErrorMessage(error, 'Could not ask for that bid') || 'Could not ask for that bid');
    } finally {
      this.askPending.set(false);
      void this.queryClient.invalidateQueries({ queryKey: chatKeys.storytellerRound(chatId) });
    }
  }

  protected async lie(): Promise<void> {
    const chatId = this.chatId();
    const roundId = this.round()?.id;
    if (!roundId) return;
    this.liePending.set(true);
    try {
      await this.core.dispatchExpect({ type: 'chatStorytellerLie', chatId, roundId }, 'storytellerLie');
    } catch (error) {
      this.toasts.showError(coreErrorMessage(error, 'Could not close the round') || 'Could not close the round');
    } finally {
      this.liePending.set(false);
      void this.queryClient.invalidateQueries({ queryKey: chatKeys.storytellerRound(chatId) });
    }
  }
}
