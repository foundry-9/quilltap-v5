/**
 * What a wardrobe browser lists for one container, and who may manage what —
 * a port of v4 `components/wardrobe/hooks/useWardrobeListData.ts`
 * (`4a38222e6`):
 *
 *  - A character container lists the character's merged pool
 *    ({@link loadCharacterWardrobeItems}: own vault > groups > project >
 *    General). Only the character's own garments are manageable there; the
 *    rest are borrowed (Move / Copy only, with an origin chip).
 *  - A shared container (General, a project, a group) lists exactly its own
 *    `Wardrobe/` folder, every row manageable; General archetypes join the
 *    resolution pool so a composite row can show its components
 *    ({@link loadWardrobeContainerItems}).
 *
 * **Recorded mechanism divergence (R-J).** v4 reads through TanStack queries
 * keyed by tier and refreshes by invalidating `queryKeys.wardrobe.lists`; the
 * SPA keeps the lists in signals with an explicit {@link WardrobeListData.reload}.
 * `loading` keeps v4's `isLoading` meaning — true only while the FIRST read for
 * the current (container, archived) view is in flight, so a refresh after a
 * row action repaints in place rather than blanking the list to "Loading…".
 * A late answer for a view no longer asked about is dropped, as a keyed query's
 * would be.
 *
 * Construct inside an injection context (a component's constructor): the
 * reload-on-change is an `effect`, v4's hooks refiring on their deps.
 *
 * @module wardrobe/wardrobe-list-data
 */

import { computed, effect, signal, untracked, type Signal } from '@angular/core';

import type { CoreClient } from '../core/core-client';
import type { WardrobeItemDto } from '../core/core-contract';
import { encodeWardrobeContainer, type WardrobeContainer } from './wardrobe-container';
import { loadCharacterWardrobeItems, loadWardrobeContainerItems } from './wardrobe.api';

export interface WardrobeListData {
  /** The rows to list. */
  readonly listItems: Signal<WardrobeItemDto[]>;
  /** Everything a composite row may resolve its components against. */
  readonly resolutionPool: Signal<WardrobeItemDto[]>;
  /** The character's wearable pool (character container only; empty otherwise). */
  readonly characterItems: Signal<WardrobeItemDto[]>;
  readonly loading: Signal<boolean>;
  /** The project tier folded into a character's pool, if any. */
  readonly projectId: Signal<string | null>;
  /**
   * May this view edit / star / duplicate / delete the item? One stable
   * function reading the current signals, so a row's `canManage` input keeps
   * one reference.
   */
  readonly canManage: (item: WardrobeItemDto) => boolean;
  /** Re-read the current view (v4's `wardrobe.lists` invalidation). */
  reload(): Promise<void>;
}

export interface WardrobeListDataOptions {
  container: () => WardrobeContainer | null;
  chatId?: () => string | null;
  includeArchived: () => boolean;
}

const EMPTY: WardrobeItemDto[] = [];

export function createWardrobeListData(
  core: CoreClient,
  opts: WardrobeListDataOptions,
): WardrobeListData {
  const characterItems = signal<WardrobeItemDto[]>(EMPTY);
  const sharedItems = signal<WardrobeItemDto[]>(EMPTY);
  const sharedResolution = signal<WardrobeItemDto[]>(EMPTY);
  const projectId = signal<string | null>(null);
  const inFlight = signal(false);
  /** The view key whose answer is on display; '' before the first answer. */
  const loadedKey = signal('');
  const isCharacterScope = computed(() => opts.container()?.scope === 'character');

  const viewKey = (container: WardrobeContainer | null, includeArchived: boolean): string =>
    container ? `${encodeWardrobeContainer(container)}|${includeArchived}` : '';

  let seq = 0;
  const reload = async (): Promise<void> => {
    const container = opts.container();
    const includeArchived = opts.includeArchived();
    const key = viewKey(container, includeArchived);
    const ticket = ++seq;
    if (!container) {
      characterItems.set(EMPTY);
      sharedItems.set(EMPTY);
      sharedResolution.set(EMPTY);
      projectId.set(null);
      inFlight.set(false);
      loadedKey.set('');
      return;
    }
    inFlight.set(true);
    try {
      if (container.scope === 'character') {
        const result = await loadCharacterWardrobeItems(core, container.id, {
          chatId: opts.chatId?.() ?? null,
          includeArchived,
        });
        if (ticket !== seq) return;
        characterItems.set(result.items);
        sharedItems.set(EMPTY);
        sharedResolution.set(EMPTY);
        projectId.set(result.projectId);
      } else {
        const result = await loadWardrobeContainerItems(core, container, { includeArchived });
        if (ticket !== seq) return;
        characterItems.set(EMPTY);
        sharedItems.set(result.items);
        sharedResolution.set(result.resolutionItems);
        projectId.set(null);
      }
      loadedKey.set(key);
    } finally {
      if (ticket === seq) inFlight.set(false);
    }
  };

  // Reload whenever the browsed container or the archived opt-in changes —
  // flipping "Show archived" is a NEW REQUEST on every tier, never a
  // client-side pass over what is already loaded (v4's hooks refire on their
  // `includeArchived` dep).
  effect(() => {
    opts.container();
    opts.includeArchived();
    untracked(() => void reload());
  });

  const sharedIds = computed(() => new Set(sharedItems().map((i) => i.id)));
  const canManage = (item: WardrobeItemDto): boolean =>
    isCharacterScope() ? Boolean(item.characterId) : sharedIds().has(item.id);

  return {
    listItems: computed(() => (isCharacterScope() ? characterItems() : sharedItems())),
    resolutionPool: computed(() => (isCharacterScope() ? characterItems() : sharedResolution())),
    characterItems: computed(() => (isCharacterScope() ? characterItems() : EMPTY)),
    loading: computed(
      () =>
        inFlight() && loadedKey() !== viewKey(opts.container(), opts.includeArchived()),
    ),
    projectId: computed(() => (isCharacterScope() ? projectId() : null)),
    canManage,
    reload,
  };
}
