import { describe, expect, it, vi } from 'vitest';

import type { CoreClient } from '../core/core-client';
import type { CoreRequest, WardrobeItemDto } from '../core/core-contract';
import { WardrobeVantageItemsLoader } from './wardrobe-vantage-items';

/**
 * The Workbench's vantage loader (v4 `use-wardrobe-vantage-items.ts`,
 * `5cb0e0190`; P4.D320 R-2). No TanStack here — a plain loader over the
 * existing wardrobe reads — so `retry: false` is moot; the dispatch is stubbed
 * on the `CoreClient` and asserted request by request.
 */

const item = (id: string, title = id): WardrobeItemDto =>
  ({ id, title, types: ['top'], createdAt: '', updatedAt: '' }) as WardrobeItemDto;

function stubCore(answer: (req: CoreRequest) => Record<string, unknown>) {
  const calls: CoreRequest[] = [];
  const core = {
    dispatchData: vi.fn(async (req: CoreRequest) => {
      calls.push(req);
      return answer(req);
    }),
  } as unknown as CoreClient;
  return { core, calls };
}

describe('WardrobeVantageItemsLoader', () => {
  it("reads a character's own, group and General tiers — never a project — archived included", async () => {
    const { core, calls } = stubCore((req) =>
      req.type === 'characterWardrobeList' && !('scope' in req)
        ? { wardrobeItems: [item('own')] }
        : req.type === 'wardrobeList'
          ? { wardrobeItems: [item('general')] }
          : { wardrobeItems: [] },
    );
    const loader = new WardrobeVantageItemsLoader(core);
    await loader.load({ kind: 'character', characterId: 'char-1' });

    expect(calls).toContainEqual({
      type: 'characterWardrobeList',
      characterId: 'char-1',
      includeArchived: true,
    });
    expect(calls.some((c) => c.type === 'projectWardrobeList' || c.type === 'chatGet')).toBe(false);
    expect(calls.every((c) => (c as { includeArchived?: boolean }).includeArchived === true)).toBe(
      true,
    );
    expect(loader.items().map((i) => i.id)).toEqual(['own', 'general']);
    expect(loader.resolutionItems().map((i) => i.id)).toEqual(['own', 'general']);
    expect(loader.fetched()).toBe(true);
    expect(loader.loading()).toBe(false);
  });

  it('reads one shared container, keeping its General archetypes for resolution only', async () => {
    const { core, calls } = stubCore((req) =>
      req.type === 'groupWardrobeList'
        ? { wardrobeItems: [item('group-coat')] }
        : req.type === 'wardrobeList'
          ? { wardrobeItems: [item('general-boots')] }
          : {},
    );
    const loader = new WardrobeVantageItemsLoader(core);
    await loader.load({ kind: 'container', container: { scope: 'group', id: 'g-1' } });

    expect(calls).toContainEqual({ type: 'groupWardrobeList', groupId: 'g-1', includeArchived: true });
    expect(loader.items().map((i) => i.id)).toEqual(['group-coat']);
    expect(loader.resolutionItems().map((i) => i.id)).toEqual(['group-coat', 'general-boots']);
    expect(loader.fetched()).toBe(true);
  });

  it('reads nothing for the none vantage or for null', async () => {
    const { core, calls } = stubCore(() => ({}));
    const loader = new WardrobeVantageItemsLoader(core);
    await loader.load({ kind: 'none' });
    await loader.load(null);
    expect(calls).toEqual([]);
    expect(loader.items()).toEqual([]);
    expect(loader.resolutionItems()).toEqual([]);
    expect(loader.fetched()).toBe(false);
    expect(loader.loading()).toBe(false);
  });

  it('is loading until the first answer, then fetched', async () => {
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    const core = {
      dispatchData: vi.fn(async () => {
        await gate;
        return { wardrobeItems: [] };
      }),
    } as unknown as CoreClient;
    const loader = new WardrobeVantageItemsLoader(core);
    const done = loader.load({ kind: 'character', characterId: 'c' });
    expect(loader.loading()).toBe(true);
    expect(loader.fetched()).toBe(false);
    release();
    await done;
    expect(loader.loading()).toBe(false);
    expect(loader.fetched()).toBe(true);
  });

  it('discards the answer for a vantage abandoned mid-flight', async () => {
    const pending: Record<string, Array<() => void>> = { slow: [], fast: [] };
    const core = {
      dispatchData: vi.fn(async (req: CoreRequest) => {
        const id = (req as { characterId?: string }).characterId;
        // The General read carries no character; it answers at once.
        if (id) await new Promise<void>((r) => pending[id].push(r));
        return {
          wardrobeItems: req.type === 'characterWardrobeList' && !('scope' in req) ? [item(id!)] : [],
        };
      }),
    } as unknown as CoreClient;
    const loader = new WardrobeVantageItemsLoader(core);
    const first = loader.load({ kind: 'character', characterId: 'slow' });
    const second = loader.load({ kind: 'character', characterId: 'fast' });
    // The second vantage's reads answer first; the first's answer lands after.
    pending['fast'].forEach((r) => r());
    await second;
    pending['slow'].forEach((r) => r());
    await first;
    expect(loader.items().map((i) => i.id)).toEqual(['fast']);
    expect(loader.fetched()).toBe(true);
  });
});
