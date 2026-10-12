import { describe, expect, it, vi } from 'vitest';

import type { CoreClient } from '../core/core-client';
import type { CoreRequest } from '../core/core-contract';
import { EMPTY_EQUIPPED_SLOTS, freshSlots, type EquippedSlots } from './equipped-slots';
import { createOutfitStore } from './outfit-store';

/**
 * P4.D285 — v4 `lib/hooks/use-outfit.ts` `useChatOutfit` at `4a38222e6`
 * (`e23fb095b`, bug 198). The first case is v4 `__tests__/unit/hooks/
 * useChatOutfit.test.tsx` transcribed: v4 seeds a TanStack cache with Laura
 * dressed; v5 has no cross-visit cache (the store is per dialog mount —
 * survey §2, measured), so the transcription is "a fresh store answers
 * undefined until ITS read lands, then the fresh snapshot". The rest pin
 * `slotsFor`'s arms and the read's edges.
 */

const CHAT = 'chat-1';
const LAURA = 'laura';

function outfitWith(top: string[]): Record<string, EquippedSlots> {
  return { [LAURA]: { ...freshSlots(), top } };
}

function fakeCore(answer: () => Promise<Record<string, unknown>>) {
  const seen: Array<Record<string, unknown>> = [];
  const core = {
    dispatchData: vi.fn(async (req: CoreRequest) => {
      seen.push(req as unknown as Record<string, unknown>);
      return answer();
    }),
  } as unknown as CoreClient;
  return { core, seen };
}

describe('createOutfitStore (v4 `useChatOutfit` — bug 198)', () => {
  it('withholds every snapshot until this store’s own fetch lands', async () => {
    let release!: () => void;
    const { core, seen } = fakeCore(
      () =>
        new Promise((resolve) => {
          release = () => resolve({ equippedOutfit: outfitWith([]) });
        }),
    );
    const store = createOutfitStore(core, CHAT);
    const pending = store.refreshOutfit();
    // Nothing answers while the read is in flight.
    expect(store.slotsFor(LAURA)).toBeUndefined();
    expect(store.loading()).toBe(true);
    expect(seen).toEqual([{ type: 'chatOutfitGet', chatId: CHAT }]);
    release();
    await pending;
    expect(store.slotsFor(LAURA)?.top).toEqual([]);
    expect(store.loading()).toBe(false);
  });

  it('a second open builds a second store, which reads again (no snapshot crosses mounts)', async () => {
    const answers = [outfitWith(['blouse']), outfitWith([])];
    const { core, seen } = fakeCore(async () => ({ equippedOutfit: answers.shift() }));
    const first = createOutfitStore(core, CHAT);
    await first.refreshOutfit();
    expect(first.slotsFor(LAURA)?.top).toEqual(['blouse']);
    const second = createOutfitStore(core, CHAT);
    expect(second.slotsFor(LAURA)).toBeUndefined();
    await second.refreshOutfit();
    expect(second.slotsFor(LAURA)?.top).toEqual([]);
    expect(seen).toHaveLength(2);
  });

  it('a character the chat never dressed reads empty slots once fetched; a null id reads undefined', async () => {
    const { core } = fakeCore(async () => ({ equippedOutfit: outfitWith(['blouse']) }));
    const store = createOutfitStore(core, CHAT);
    await store.refreshOutfit();
    expect(store.slotsFor('nora')).toEqual(EMPTY_EQUIPPED_SLOTS);
    expect(store.slotsFor(null)).toBeUndefined();
    expect(store.equippedOutfit()).toEqual(outfitWith(['blouse']));
  });

  it('an answer without equippedOutfit reads as {} (v4 `data?.equippedOutfit ?? {}`)', async () => {
    const { core } = fakeCore(async () => ({}));
    const store = createOutfitStore(core, CHAT);
    expect(await store.refreshOutfit()).toEqual({});
    expect(store.slotsFor(LAURA)).toEqual(EMPTY_EQUIPPED_SLOTS);
  });

  it('a chat-less store never reads', async () => {
    const { core, seen } = fakeCore(async () => ({}));
    const store = createOutfitStore(core, null);
    expect(await store.refreshOutfit()).toBeNull();
    expect(seen).toEqual([]);
    expect(store.slotsFor(LAURA)).toBeUndefined();
    expect(store.loading()).toBe(false);
  });

  it('a failed read resolves null and keeps the snapshot it had', async () => {
    let fail = false;
    const { core } = fakeCore(async () => {
      if (fail) throw new Error('down');
      return { equippedOutfit: outfitWith(['blouse']) };
    });
    const store = createOutfitStore(core, CHAT);
    await store.refreshOutfit();
    fail = true;
    expect(await store.refreshOutfit()).toBeNull();
    expect(store.slotsFor(LAURA)?.top).toEqual(['blouse']);
    expect(store.loading()).toBe(false);
  });

  it('a late answer from an earlier read never overwrites a newer one', async () => {
    const releases: Array<() => void> = [];
    const answers = [outfitWith(['stale']), outfitWith(['fresh'])];
    const { core } = fakeCore(() => {
      const answer = answers.shift();
      return new Promise((resolve) => releases.push(() => resolve({ equippedOutfit: answer })));
    });
    const store = createOutfitStore(core, CHAT);
    const a = store.refreshOutfit();
    const b = store.refreshOutfit();
    releases[1]();
    await b;
    releases[0]();
    await a;
    expect(store.slotsFor(LAURA)?.top).toEqual(['fresh']);
  });
});
