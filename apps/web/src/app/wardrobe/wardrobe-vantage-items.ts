/**
 * The wardrobe a Workbench vantage can reach — a port of v4
 * `lib/hooks/use-wardrobe-vantage-items.ts` (NEW at `5cb0e0190`).
 *
 * v4's hook composes two TanStack hooks: a CHARACTER vantage reads the
 * character's own + group + General tiers (`useCharacterWardrobeItems` with no
 * project — "own + groups + General, NO project tier": a tool has no chat, so
 * there is no project to fold in), a CONTAINER vantage reads that one shared
 * wardrobe plus the General archetypes its composites may name
 * (`useWardrobeContainerItems`). Both opt in to archived items, so a reference
 * to an archived garment still resolves (and can be flagged). The `none`
 * vantage, and `null` — "read nothing until a wardrobe row exists" — read
 * nothing at all.
 *
 * v5 has no hook layer here: this is a small signal loader over the EXISTING
 * `loadCharacterWardrobeItems` / `loadWardrobeContainerItems`, the
 * `wardrobe-item-editor.ts` `loadCandidates` precedent. It mints no query key
 * and no route (neither does v4's hook). TanStack's `retry` is therefore moot.
 *
 * **Recorded divergence class (P4.D320 R-2):** v4's hook shares TanStack's
 * cache with the wardrobe dialog's reads; this loader reads fresh on every
 * vantage change. Invisible except in a network trace.
 *
 * @module wardrobe/wardrobe-vantage-items
 */

import { signal } from '@angular/core';

import type { CoreClient } from '../core/core-client';
import type { WardrobeItemDto } from '../core/core-contract';
import type { WardrobeVantage } from '../pascal/tool-draft';
import { encodeWardrobeContainer } from './wardrobe-container';
import { loadCharacterWardrobeItems, loadWardrobeContainerItems } from './wardrobe.api';

/** The vantage as a stable string, so a stale answer can recognise itself. */
function vantageKey(vantage: WardrobeVantage | null): string {
  if (!vantage) return 'null';
  switch (vantage.kind) {
    case 'character':
      return `char:${vantage.characterId}`;
    case 'container':
      return encodeWardrobeContainer(vantage.container);
    case 'none':
      return 'none';
  }
}

export class WardrobeVantageItemsLoader {
  /** What the vantage can reach, nearest tier first — the pool a reference resolves against. */
  readonly items = signal<WardrobeItemDto[]>([]);
  /** `items` plus anything a composite's components may name (General archetypes for a shared container). */
  readonly resolutionItems = signal<WardrobeItemDto[]>([]);
  readonly loading = signal(false);
  /** True once the reads have settled; false for the `none` vantage. */
  readonly fetched = signal(false);

  /** The vantage the most recent `load` asked for — any other answer is stale. */
  private current = 'null';

  constructor(private readonly core: CoreClient) {}

  /** Load the vantage's wardrobe. `null` (or the `none` vantage) reads nothing. */
  async load(vantage: WardrobeVantage | null): Promise<void> {
    const key = vantageKey(vantage);
    this.current = key;
    if (!vantage || vantage.kind === 'none') {
      this.items.set([]);
      this.resolutionItems.set([]);
      this.loading.set(false);
      this.fetched.set(false);
      return;
    }

    this.loading.set(true);
    this.fetched.set(false);
    let items: WardrobeItemDto[];
    let resolutionItems: WardrobeItemDto[];
    if (vantage.kind === 'character') {
      // No `projectId` — v4's "own + groups + General, NO project tier".
      const loaded = await loadCharacterWardrobeItems(this.core, vantage.characterId, {
        includeArchived: true,
      });
      items = loaded.items;
      resolutionItems = loaded.items;
    } else {
      const loaded = await loadWardrobeContainerItems(this.core, vantage.container, {
        includeArchived: true,
      });
      items = loaded.items;
      resolutionItems = loaded.resolutionItems;
    }
    // A slow read for an abandoned vantage never lands.
    if (this.current !== key) return;
    this.items.set(items);
    this.resolutionItems.set(resolutionItems);
    this.loading.set(false);
    this.fetched.set(true);
  }
}
