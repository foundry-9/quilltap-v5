import { ChangeDetectionStrategy, Component, computed, inject, input, signal } from '@angular/core';
import { injectQuery, QueryClient } from '@tanstack/angular-query-experimental';

import { chatKeys } from '../chat/chat-keys';
import { CoreClient, coreErrorMessage } from '../core/core-client';
import type {
  OutfitHistoryEntry,
  OutfitHistorySlotItem,
  WardrobeSlotType,
} from '../core/core-contract';
import { formatRelativeDate } from '../shared/format-date';
import { NowService } from '../shared/now.service';
import { ToastService } from '../ui/toast.service';
import { WARDROBE_SLOT_META, WARDROBE_SLOT_TYPES } from './slot-meta';
import { fetchWardrobeHistory, postWardrobeRollback } from './wardrobe.api';

/** One slot's items in canonical order, an absent slot key read as `[]`. */
interface EntrySlotRow {
  slot: WardrobeSlotType;
  item: OutfitHistorySlotItem;
}

/**
 * How many distinct items in an entry are no longer available (archived or
 * gone) — v4 `outfit-history-tab.tsx:77-86`. An id stale in two slots counts
 * once; an absent slot key reads as empty.
 */
export function staleCount(entry: OutfitHistoryEntry): number {
  const stale = new Set<string>();
  for (const slot of WARDROBE_SLOT_TYPES) {
    for (const item of entry.slots[slot] ?? []) {
      if (item.archived || item.missing) stale.add(item.itemId);
    }
  }
  return stale.size;
}

/**
 * The Wardrobe dialog's History tab — a port of v4
 * `components/wardrobe/outfit-history-tab.tsx` (`8d8618819`, + `4ae20977f`'s
 * `actorToolName` type field).
 *
 * One character's numbered outfit timeline in this chat, newest first. Each
 * entry shows who changed it (the server's `changedBy` label, VERBATIM — the
 * tab never reads `actorName` / `actorToolName` / `source`, R-8), when, and
 * what was worn, with archived or vanished garments marked. *Wear again* posts
 * the rollback: the server commits the restored outfit once, moves the
 * timeline's cursor (nothing is appended), and Aurora announces the change.
 *
 * If the Live tab holds staged edits for this character, *Wear again* asks
 * first. Either way it throws the staging away BEFORE rolling back — otherwise
 * Done would flush the stale staging straight over the restored outfit — and
 * asks the dialog to re-seed the Live tab once the rollback's re-reads have
 * settled. (v4 discards only after the confirm, i.e. only when dirty — the
 * ruled R-L divergence, see `wearAgain`.)
 *
 * v5 notes:
 *  - **The keys (R-H).** The read lives at `chatKeys.wardrobeHistory(chatId,
 *    characterId)` under `chatKeys.detail(chatId)`, so the `chats` realtime
 *    topic (a tool call, Aurora's announcement) refetches it with NO topic-map
 *    row — v4 spells it `['wardrobe', 'history', …]` and spends a row. The tab
 *    opens no realtime subscription of its own (v4 has none either).
 *  - **The settle (R-5).** v4's `onSettled` awaits `invalidateQueries(
 *    wardrobe.all)` — the snapshot AND every history — then re-seeds. v5's
 *    snapshot and lists are signal-held by the dialog, so the tab invalidates
 *    its own histories prefix and then awaits `discardStagedEdits({ reseed:
 *    true })`, whose dialog binding re-reads the lists and the outfit BEFORE
 *    the reseed.
 *  - **The rollback (R-9)** is an awaited call guarded by `rollbackPending`
 *    rather than `injectMutation`; the flag spans the call AND the settled
 *    re-reads (v4's `isPending` ends with `onSettled`'s promise as well, since
 *    TanStack awaits it) and disables EVERY *Wear again* (v4
 *    `disabled={rollback.isPending}`).
 *  - **The confirm (R-2).** `requestConfirmation` is v4's async prop type; the
 *    dialog binds it over its synchronous `window.confirm` (its recorded
 *    divergence), and the tab awaits it as v4 does.
 *
 * @module wardrobe/outfit-history-tab
 */
@Component({
  selector: 'qt-outfit-history-tab',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="space-y-2 mb-3">
      <p class="qt-text-xs qt-text-secondary px-1">{{ description }}</p>
      @if (history.isLoading()) {
        <p class="qt-text-xs qt-text-muted px-1">Consulting the record…</p>
      }
      @if (history.isError()) {
        <p class="qt-text-xs qt-text-danger px-1">{{ readError() }}</p>
      }
      @if (history.isSuccess() && entries().length === 0) {
        <p class="qt-text-xs qt-text-muted px-1">{{ emptyLine }}</p>
      }
      <ul class="space-y-2">
        @for (entry of entries(); track entry.index) {
          <li class="qt-card p-2" [attr.data-testid]="'outfit-history-entry-' + entry.index">
            <div class="flex items-center gap-2 mb-1">
              <span class="qt-badge text-xs">#{{ entry.index }}</span>
              <span class="qt-text-xs qt-text-secondary" [attr.title]="fullDate(entry.at)">{{
                relative(entry.at)
              }}</span>
              <span class="qt-text-xs qt-text-secondary">{{ '· ' + entry.changedBy }}</span>
              <span class="flex-1"></span>
              @if (entry.isCurrent) {
                <span class="qt-badge text-xs">Wearing now</span>
              } @else {
                <button
                  type="button"
                  class="qt-button-secondary qt-button-sm"
                  [disabled]="rollbackPending()"
                  (click)="wearAgain(entry.index)"
                  [attr.title]="wearAgainTitle(entry)"
                >Wear again</button>
              }
            </div>
            <p class="qt-text-xs mb-1">{{ entry.summary }}</p>
            @if (slotRows(entry); as rows) {
              @if (rows.length === 0) {
                <p class="qt-text-xs qt-text-muted italic">Nothing at all.</p>
              } @else {
                <div class="flex flex-wrap gap-1">
                  @for (row of rows; track row.slot + ':' + row.item.itemId) {
                    <span [attr.class]="badgeClass(row)" [attr.title]="badgeTitle(row)">{{
                      badgeText(row.item)
                    }}</span>
                  }
                </div>
              }
            }
          </li>
        }
      </ul>
    </div>
  `,
})
export class OutfitHistoryTab {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);
  private readonly queryClient = inject(QueryClient);

  readonly chatId = input.required<string>();
  readonly characterId = input.required<string>();
  readonly characterName = input.required<string>();
  /** True when the Live tab holds unsaved staged edits for this character. */
  readonly hasStagedEdits = input.required<boolean>();
  /**
   * Throw away this character's staged Live-tab edits (after the operator
   * agrees); with `reseed`, re-seed the Live tab from the refreshed snapshot.
   * Widened from v4's `=> void` so the dialog's binding can await its re-reads
   * before the reseed (R-5); the tab awaits it.
   */
  readonly discardStagedEdits =
    input.required<(options?: { reseed?: boolean }) => void | Promise<void>>();
  readonly requestConfirmation = input.required<(message: string) => Promise<boolean>>();

  /** v4 `useNow(60_000)` (`:123`). */
  protected readonly nowMs = inject(NowService).now(60_000);

  protected readonly description =
    'Every change of clothes in this chat, numbered from the outfit they arrived in. Wear again returns to one — Aurora will say so.';
  protected readonly emptyLine =
    'No outfits recorded yet. The first change of clothes starts the record.';

  /** v4 `:124-131`. */
  protected readonly history = injectQuery(() => ({
    queryKey: chatKeys.wardrobeHistory(this.chatId(), this.characterId()),
    queryFn: () => fetchWardrobeHistory(this.core, this.chatId(), this.characterId()),
  }));

  /** v4 `rollback.isPending`. */
  protected readonly rollbackPending = signal(false);

  /** v4 `:171` — newest first (the wire is ascending). */
  protected readonly entries = computed<OutfitHistoryEntry[]>(() =>
    [...(this.history.data()?.entries ?? [])].sort((a, b) => b.index - a.index),
  );

  protected readonly readError = computed(() =>
    coreErrorMessage(this.history.error(), 'The outfit record could not be read.'),
  );

  protected relative(at: string): string {
    return formatRelativeDate(at, this.nowMs());
  }

  /** v4's exact call — `new Date(entry.at).toLocaleString()`. */
  protected fullDate(at: string): string {
    return new Date(at).toLocaleString();
  }

  protected wearAgainTitle(entry: OutfitHistoryEntry): string {
    const stale = staleCount(entry);
    return stale > 0
      ? `${stale} item${stale === 1 ? ' in this outfit is' : 's in this outfit are'} no longer available and will be left off.`
      : `Return ${this.characterName()} to outfit ${entry.index}`;
  }

  /** v4 `EntrySlots` (`:88-112`) — canonical slot order, absent slot = `[]`. */
  protected slotRows(entry: OutfitHistoryEntry): EntrySlotRow[] {
    return WARDROBE_SLOT_TYPES.flatMap((slot) =>
      (entry.slots[slot] ?? []).map((item) => ({ slot, item })),
    );
  }

  protected badgeClass(row: EntrySlotRow): string {
    const stale = row.item.archived || row.item.missing;
    return `qt-badge ${stale ? 'qt-badge-secondary' : WARDROBE_SLOT_META[row.slot].badgeClass} text-xs`;
  }

  protected badgeTitle(row: EntrySlotRow): string {
    const stale = row.item.archived || row.item.missing;
    return `${WARDROBE_SLOT_META[row.slot].label}${stale ? (row.item.missing ? ' — no longer exists' : ' — archived') : ''}`;
  }

  protected badgeText(item: OutfitHistorySlotItem): string {
    return `${item.title ?? item.itemId}${item.missing ? ' · gone' : item.archived ? ' · archived' : ''}`;
  }

  /**
   * v4 `wearAgain` (`:160-169`). Every input is read into a local BEFORE the
   * first await (R-10): v4's React closure captured the render's values, so a
   * character switch mid-rollback still settles the character clicked.
   */
  protected async wearAgain(index: number): Promise<void> {
    const chatId = this.chatId();
    const characterId = this.characterId();
    const characterName = this.characterName();
    const hasStagedEdits = this.hasStagedEdits();
    const discardStagedEdits = this.discardStagedEdits();
    const requestConfirmation = this.requestConfirmation();
    if (hasStagedEdits) {
      const ok = await requestConfirmation(
        `Discard the staged changes to ${characterName}'s outfit and return to outfit ${index}?`,
      );
      if (!ok) return;
    }
    // RULED DIVERGENCE (R-L, the human, 2026-10-19): v4 discards only inside
    // the `hasStagedEdits` arm, so on the clean path the Live tab keeps its
    // seeded (pre-rollback) staging — the settle's `{ reseed: true }` is a
    // no-op for an already-seeded character — and a later gesture + Done
    // `set_all`s over the rollback (measured in v4 at `e8e17b2d9`). v5
    // discards unconditionally; the confirm still asks only when dirty.
    await discardStagedEdits();
    void this.rollback(chatId, characterId, index, discardStagedEdits);
  }

  /** v4's `rollback` mutation (`:133-158`): success OR error, then the settle. */
  private async rollback(
    chatId: string,
    characterId: string,
    index: number,
    discardStagedEdits: (options?: { reseed?: boolean }) => void | Promise<void>,
  ): Promise<void> {
    this.rollbackPending.set(true);
    try {
      try {
        const result = await postWardrobeRollback(this.core, chatId, characterId, index);
        const skipped = result.skipped ?? [];
        if (skipped.length === 0) {
          this.toasts.showSuccess(`Back in outfit ${result.restoredIndex}`);
        } else {
          const names = skipped.map((s) => s.title ?? s.itemId).join(', ');
          this.toasts.showSuccess(
            `Back in outfit ${result.restoredIndex}. ${skipped.length} item${skipped.length === 1 ? '' : 's'} could not be put back on: ${names}`,
          );
        }
      } catch (error) {
        this.toasts.showError(coreErrorMessage(error, 'Could not return to that outfit'));
      }
      // onSettled (R-5): the histories first, then the dialog's re-reads and
      // the reseed — so Done never flushes over the restored outfit.
      await this.queryClient.invalidateQueries({ queryKey: chatKeys.wardrobeHistories(chatId) });
      await discardStagedEdits({ reseed: true });
    } finally {
      this.rollbackPending.set(false);
    }
  }
}
