import { DestroyRef, Injectable, computed, inject, signal } from '@angular/core';

import { CoreClient } from '../core/core-client';
import type { ConciergeState, TagDto } from '../core/core-contract';
import {
  ACTIVE_TAGS_KEY,
  HIDE_DANGEROUS_KEY,
  HIDE_SALON_IMAGES_KEY,
  INCLUDE_AUTONOMOUS_KEY,
  parseActiveTags,
  readActiveTags,
  readBooleanKey,
  writeActiveTags,
  writeBooleanKey,
} from './quick-hide.storage';
import { shouldHideByIds, shouldHideChat } from './should-hide';

/** A tag flagged for quick-hide (v4 `QuickHideTag`, `quick-hide-provider.tsx:6-9`). */
export interface QuickHideTag {
  id: string;
  name: string;
}

/**
 * The quick-hide service — v5's port of v4's `QuickHideProvider`
 * (`components/providers/quick-hide-provider.tsx`, 239 lines).
 *
 * Holds the four localStorage-backed preferences as signals and the list of
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
  private readonly hideSalonImagesSignal = signal(readBooleanKey(HIDE_SALON_IMAGES_KEY));
  private readonly quickHideTagsSignal = signal<readonly QuickHideTag[]>([]);
  private readonly loadingSignal = signal(true);

  /** Tags the user flagged `quickHide` (v4 `:51`, sourced at `:37-47`). */
  readonly quickHideTags = this.quickHideTagsSignal.asReadonly();
  /** The ids currently hidden (v4 `:52`). */
  readonly hiddenTagIds = this.hiddenTagIdsSignal.asReadonly();
  /** v4 `:53` — the dangerous-chat filter. */
  readonly hideDangerousChats = this.hideDangerousChatsSignal.asReadonly();
  /** v4 `:54` — the autonomous-rooms INCLUDE toggle (adds rows, not hides them). */
  readonly includeAutonomousRooms = this.includeAutonomousRoomsSignal.asReadonly();
  /**
   * v4 `hideSalonImages` (`e3937d7aa`, `quick-hide-provider.tsx:22-27`): when
   * true, the Salon withholds every image it would paint — the story
   * background, avatars in the transcript and the participant sidebar, and
   * attached or embedded images. Off by default. The Salon hands it down
   * through `IMAGES_HIDDEN` (`chat/hidden-image/images-hidden.ts`); nothing
   * else reads it, so every other page keeps its images.
   */
  readonly hideSalonImages = this.hideSalonImagesSignal.asReadonly();
  /** v4 `:55` — true until the first tag load settles. */
  readonly loading = this.loadingSignal.asReadonly();

  /**
   * v4 `sidebar-footer.tsx:144` `hasAnyHidden` — hidden tags, the danger
   * filter, or (since `e3937d7aa`) the Salon Images switch. v4 feeds it to the
   * footer quick-hide button's icon and title; v5 has no footer button (the
   * quick-hide section lives inside the user menu, `shell/user-menu.ts`), so it
   * is carried as v4's predicate, pinned by spec, for the surface that grows
   * one.
   *
   * v4's sibling `hasQuickHideFeatures` is gone: since `e3937d7aa` it is
   * `mounted` alone ("The Salon Images switch is always meaningful, so the
   * quick-hide button is always offered"), and v5's client-only SPA has no
   * pre-mount render — so the section is simply unconditional. The
   * `chatsHasDangerous` probe that fed its third arm was retired with it (v4
   * deleted `useHasDangerousChats` in `e3937d7aa` and the endpoint in
   * `944127d9a`; the client half is P4.D223's, the server half P4.D220's).
   */
  readonly hasAnyHidden = computed(
    () =>
      this.hiddenTagIdsSignal().size > 0 ||
      this.hideDangerousChatsSignal() ||
      this.hideSalonImagesSignal(),
  );

  constructor() {
    this.listenForCrossTabChanges();
    void this.refresh();
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

  /** v4 `toggleHideSalonImages` (`e3937d7aa`, `:199-201`). */
  toggleHideSalonImages(): void {
    const next = !this.hideSalonImagesSignal();
    this.hideSalonImagesSignal.set(next);
    writeBooleanKey(HIDE_SALON_IMAGES_KEY, next);
  }

  /**
   * v4 `clearAllHidden` (`:176-181`; `e3937d7aa` adds the Salon Images reset,
   * `:206`). Note what it does NOT reset:
   * `includeAutonomousRooms` is an "include" toggle (it ADDS items), not a
   * "hide" toggle, so Clear All Hidden deliberately spares it (v4 `:179-180`).
   */
  clearAllHidden(): void {
    this.setHiddenTagIds(new Set());
    this.hideDangerousChatsSignal.set(false);
    writeBooleanKey(HIDE_DANGEROUS_KEY, false);
    this.hideSalonImagesSignal.set(false);
    writeBooleanKey(HIDE_SALON_IMAGES_KEY, false);
  }

  /**
   * v4 `loadTags` (`:58-82`), exposed as `refresh` (`:222`) — the tags tab calls
   * it after authoring so the menu list updates live.
   *
   * Fail-soft exactly as v4: a failed load logs and empties the list rather than
   * surfacing an error (`quick-hide-provider.tsx:79-84` — the `console.warn`
   * below is v4's own line at `:82`, message and payload shape included, so it
   * STAYS; P4.69 measured it before retiring the probe's invented twin).
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
   * a signals port, so it is wired deliberately over all four keys.
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
      // v4 `e3937d7aa` `:170-172`.
      if (event.key === HIDE_SALON_IMAGES_KEY && event.newValue) {
        this.hideSalonImagesSignal.set(event.newValue === 'true');
      }
    };
    window.addEventListener('storage', handler);
    this.destroyRef.onDestroy(() => window.removeEventListener('storage', handler));
  }
}
