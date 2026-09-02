import { DestroyRef, Injectable, computed, inject, signal } from '@angular/core';

import { CoreClient } from '../core/core-client';
import type { ConciergeState, TagDto } from '../core/core-contract';
import {
  ACTIVE_TAGS_KEY,
  HIDE_DANGEROUS_KEY,
  INCLUDE_AUTONOMOUS_KEY,
  parseActiveTags,
  readActiveTags,
  readBooleanKey,
  writeActiveTags,
  writeBooleanKey,
} from './quick-hide.storage';
import { quickHideFeaturesVisible, shouldHideByIds, shouldHideChat } from './should-hide';

/**
 * ACTIVATE-AT-UNIFY (shared contract §H): flip to `true` on the branch that
 * carries P4.D143's `chatsHasDangerous` verb, which is the moment the shell
 * footer's quick-hide affordance can start following the uncensored row. Until
 * then the probe is never dispatched — a false constant, not a swallowed
 * error, because the two are indistinguishable after the fact.
 */
export const CHATS_HAS_DANGEROUS_VERB_LANDED = false;

/** A tag flagged for quick-hide (v4 `QuickHideTag`, `quick-hide-provider.tsx:6-9`). */
export interface QuickHideTag {
  id: string;
  name: string;
}

/**
 * The quick-hide service — v5's port of v4's `QuickHideProvider`
 * (`components/providers/quick-hide-provider.tsx`, 239 lines).
 *
 * Holds the three §4 localStorage-backed preferences as signals and the list of
 * tags the user has flagged `quickHide` server-side, and exposes the one
 * predicate every consumer filters through: {@link shouldHideChat}. Since v4
 * `c43d3b1b4` that is genuinely ONE rule — tags, then the Concierge's
 * uncensored row — rather than a rule plus four inlined copies of its second
 * half (see `should-hide.ts` for the retired non-port ruling).
 *
 * Divergences from v4, both deliberate:
 *  - localStorage is read EAGERLY in the initializer. v4 defers to a post-mount
 *    effect (`:89-91`) purely to dodge a Next.js SSR mismatch; v5's SPA is
 *    client-only so there is nothing to desync, and eager reads mean the first
 *    render is already filtered.
 *  - No `status !== 'authenticated'` gate (v4 `:59-65`). v5 has no session
 *    provider; the tag load simply runs, and a pre-unlock failure lands in the
 *    same fail-soft arm v4 uses (`:76-78`).
 */
@Injectable({ providedIn: 'root' })
export class QuickHideService {
  private readonly core = inject(CoreClient);
  private readonly destroyRef = inject(DestroyRef);

  private readonly hiddenTagIdsSignal = signal<ReadonlySet<string>>(readActiveTags());
  private readonly hideDangerousChatsSignal = signal(readBooleanKey(HIDE_DANGEROUS_KEY));
  private readonly includeAutonomousRoomsSignal = signal(readBooleanKey(INCLUDE_AUTONOMOUS_KEY));
  private readonly quickHideTagsSignal = signal<readonly QuickHideTag[]>([]);
  private readonly hasDangerousChatsSignal = signal(false);
  private readonly loadingSignal = signal(true);

  /** Tags the user flagged `quickHide` (v4 `:51`, sourced at `:37-47`). */
  readonly quickHideTags = this.quickHideTagsSignal.asReadonly();
  /** The ids currently hidden (v4 `:52`). */
  readonly hiddenTagIds = this.hiddenTagIdsSignal.asReadonly();
  /** v4 `:53` — the dangerous-chat filter. */
  readonly hideDangerousChats = this.hideDangerousChatsSignal.asReadonly();
  /** v4 `:54` — the autonomous-rooms INCLUDE toggle (adds rows, not hides them). */
  readonly includeAutonomousRooms = this.includeAutonomousRoomsSignal.asReadonly();
  /** v4 `:55` — true until the first tag load settles. */
  readonly loading = this.loadingSignal.asReadonly();

  /**
   * v4 `sidebar-footer.tsx:145` `hasAnyHidden` — drives the menu-entry badge
   * (an open eye when nothing is hidden, a struck-through eye when something is).
   */
  readonly hasAnyHidden = computed(
    () => this.hiddenTagIdsSignal().size > 0 || this.hideDangerousChatsSignal(),
  );

  /**
   * v4 `sidebar-footer.tsx:144` `hasQuickHideFeatures` — whether the quick-hide
   * affordance is worth showing at all. The third arm is v4's
   * `useHasDangerousChats` probe, which `c43d3b1b4` re-based onto the uncensored
   * ROW: the affordance appears when any chat is routed uncensored, not when
   * any chat merely carries the label (shared contract §H).
   *
   * ACTIVATE-AT-UNIFY behind {@link CHATS_HAS_DANGEROUS_VERB_LANDED}. Until
   * P4.D143's `chatsHasDangerous` verb is on the branch the probe never fires
   * and this reads exactly as it did before — a named constant rather than a
   * try/catch, because a swallowed dispatch failure looks identical to an
   * honest `false` and would hide the arm forever after the flip.
   */
  readonly hasQuickHideFeatures = computed(() =>
    quickHideFeaturesVisible(
      this.quickHideTagsSignal().length > 0,
      this.hideDangerousChatsSignal(),
      this.hasDangerousChatsSignal(),
    ),
  );

  /** v4 `useHasDangerousChats` — false until the §H probe answers. */
  readonly hasDangerousChats = this.hasDangerousChatsSignal.asReadonly();

  constructor() {
    this.listenForCrossTabChanges();
    void this.refresh();
    void this.refreshHasDangerousChats();
  }

  /**
   * The §H probe (v4 `useHasDangerousChats`). Fail-soft like the tag load: a
   * refusal leaves the answer `false`, which is the same thing v5 did before
   * the arm existed.
   */
  private async refreshHasDangerousChats(): Promise<void> {
    if (!CHATS_HAS_DANGEROUS_VERB_LANDED) return;
    try {
      const data = await this.core.dispatchData({ type: 'chatsHasDangerous' });
      this.hasDangerousChatsSignal.set(data['hasDangerous'] === true);
    } catch (error) {
      console.warn('Unable to probe for uncensored-route chats', {
        error: error instanceof Error ? error.message : String(error),
      });
      this.hasDangerousChatsSignal.set(false);
    }
  }

  /**
   * v4 `shouldHideByIds` (`:183-196`), bound to the live hidden set. Reading the
   * signal here is what makes every consumer's `computed` re-run on a toggle.
   */
  shouldHideByIds(tagIds?: ReadonlyArray<string | null | undefined>): boolean {
    return shouldHideByIds(this.hiddenTagIdsSignal(), tagIds);
  }

  /**
   * v4 `shouldHideChat` (`:203-215`) — THE rule every chat list filters
   * through. Reading both signals here is what makes a consumer's `computed`
   * re-run on either toggle.
   */
  shouldHideChat(chat: {
    characterTags?: ReadonlyArray<string | null | undefined>;
    conciergeState?: ConciergeState;
  }): boolean {
    return shouldHideChat(this.hiddenTagIdsSignal(), this.hideDangerousChatsSignal(), chat);
  }

  /** v4 `toggleTag` (`:155-166`). */
  toggleTag(tagId: string): void {
    const next = new Set(this.hiddenTagIdsSignal());
    if (next.has(tagId)) {
      next.delete(tagId);
    } else {
      next.add(tagId);
    }
    this.setHiddenTagIds(next);
  }

  /** v4 `toggleHideDangerousChats` (`:168-170`). */
  toggleHideDangerousChats(): void {
    const next = !this.hideDangerousChatsSignal();
    this.hideDangerousChatsSignal.set(next);
    writeBooleanKey(HIDE_DANGEROUS_KEY, next);
  }

  /** v4 `toggleIncludeAutonomousRooms` (`:172-174`). */
  toggleIncludeAutonomousRooms(): void {
    const next = !this.includeAutonomousRoomsSignal();
    this.includeAutonomousRoomsSignal.set(next);
    writeBooleanKey(INCLUDE_AUTONOMOUS_KEY, next);
  }

  /**
   * v4 `clearAllHidden` (`:176-181`). Note what it does NOT reset:
   * `includeAutonomousRooms` is an "include" toggle (it ADDS items), not a
   * "hide" toggle, so Clear All Hidden deliberately spares it (v4 `:179-180`).
   */
  clearAllHidden(): void {
    this.setHiddenTagIds(new Set());
    this.hideDangerousChatsSignal.set(false);
    writeBooleanKey(HIDE_DANGEROUS_KEY, false);
  }

  /**
   * v4 `loadTags` (`:58-82`), exposed as `refresh` (`:222`) — the tags tab calls
   * it after authoring so the menu list updates live.
   *
   * Fail-soft exactly as v4: a failed load logs and empties the list rather than
   * surfacing an error (`:76-78`).
   */
  async refresh(): Promise<void> {
    this.loadingSignal.set(true);
    try {
      const data = await this.core.dispatchData({ type: 'tagList' });
      const allTags = (data['tags'] as TagDto[] | undefined) ?? [];
      // v4 `:45`: `Boolean(tag.quickHide)` — only flagged tags reach the menu.
      const tags = allTags
        .filter((tag) => Boolean(tag.quickHide))
        .map((tag) => ({ id: tag.id, name: tag.name }));
      this.quickHideTagsSignal.set(tags);
      this.pruneHiddenToValid(tags);
    } catch (error) {
      console.warn('Unable to load quick-hide tags', {
        error: error instanceof Error ? error.message : String(error),
      });
      this.quickHideTagsSignal.set([]);
    } finally {
      this.loadingSignal.set(false);
    }
  }

  /**
   * v4 `:71-75`: drop hidden ids whose tag no longer carries the quickHide flag
   * (or no longer exists), so a tag un-flagged in the tags tab stops hiding
   * rows the user can no longer un-hide. The size check preserves the previous
   * set identity when nothing changed — v4 does this to avoid a re-render, and
   * v5 keeps it so dependent `computed`s don't recompute either.
   */
  private pruneHiddenToValid(tags: readonly QuickHideTag[]): void {
    const prev = this.hiddenTagIdsSignal();
    const allowed = new Set(tags.map((tag) => tag.id));
    const next = new Set([...prev].filter((id) => allowed.has(id)));
    if (next.size !== prev.size) {
      this.setHiddenTagIds(next);
    }
  }

  private setHiddenTagIds(next: ReadonlySet<string>): void {
    this.hiddenTagIdsSignal.set(next);
    writeActiveTags(next);
  }

  /**
   * v4 `:131-153` — the cross-tab `storage` listener. It does NOT come free with
   * a signals port, so it is wired deliberately over all three keys.
   *
   * Faithful quirk: v4 guards each arm with `&& event.newValue`, so a key being
   * CLEARED (`newValue === null`) is ignored rather than reset to defaults.
   */
  private listenForCrossTabChanges(): void {
    if (typeof window === 'undefined') return;
    const handler = (event: StorageEvent): void => {
      if (event.key === ACTIVE_TAGS_KEY && event.newValue) {
        this.hiddenTagIdsSignal.set(parseActiveTags(event.newValue));
      }
      if (event.key === HIDE_DANGEROUS_KEY && event.newValue) {
        this.hideDangerousChatsSignal.set(event.newValue === 'true');
      }
      if (event.key === INCLUDE_AUTONOMOUS_KEY && event.newValue) {
        this.includeAutonomousRoomsSignal.set(event.newValue === 'true');
      }
    };
    window.addEventListener('storage', handler);
    this.destroyRef.onDestroy(() => window.removeEventListener('storage', handler));
  }
}
