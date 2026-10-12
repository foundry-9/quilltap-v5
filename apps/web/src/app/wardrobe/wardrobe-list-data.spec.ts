import { signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import type { CoreClient } from '../core/core-client';
import type { CoreRequest, WardrobeItemDto } from '../core/core-contract';
import { GENERAL_CONTAINER, type WardrobeContainer } from './wardrobe-container';
import { createWardrobeListData } from './wardrobe-list-data';

/**
 * P4.D285 — v4 `components/wardrobe/hooks/useWardrobeListData.ts`
 * (`4a38222e6`): the character arm (the merged pool is the list, the
 * resolution pool and the character's items; only own garments manageable)
 * and the shared arm (exactly the container's own items, General archetypes
 * only in the resolution pool, every own row manageable) — over v5's two
 * loaders, as signals (R-J). The loaders' own merge rules are pinned by
 * `wardrobe.api.spec.ts`.
 */

const item = (id: string, extra: Partial<WardrobeItemDto> = {}): WardrobeItemDto => ({
  id,
  title: id,
  types: ['top'],
  createdAt: '2026-01-01T00:00:00.000Z',
  updatedAt: '2026-01-01T00:00:00.000Z',
  ...extra,
});

type Answer = (req: Record<string, unknown>) => Promise<Record<string, unknown>>;

function fakeCore(answer: Answer): { core: CoreClient; seen: Array<Record<string, unknown>> } {
  const seen: Array<Record<string, unknown>> = [];
  const core = {
    dispatchData: vi.fn(async (req: CoreRequest) => {
      const r = req as unknown as Record<string, unknown>;
      seen.push(r);
      return answer(r);
    }),
  } as unknown as CoreClient;
  return { core, seen };
}

const tiers: Answer = async (req) => {
  switch (req['type']) {
    case 'characterWardrobeList':
      return {
        wardrobeItems:
          req['scope'] === 'group'
            ? [item('livery', { origin: { scope: 'group', id: 'g1', name: 'S' } })]
            : [item('greatcoat', { characterId: 'alice' })],
      };
    case 'chatGet':
      return { chat: { projectId: 'p1' } };
    case 'projectWardrobeList':
      return { wardrobeItems: [item('cape', { origin: { scope: 'project', id: 'p1', name: 'B' } })] };
    case 'wardrobeList':
      return { wardrobeItems: [item('watch', { origin: { scope: 'general', id: null, name: 'G' } })] };
    default:
      return {};
  }
};

async function settle(): Promise<void> {
  TestBed.tick();
  for (let i = 0; i < 6; i++) await Promise.resolve();
  await new Promise((r) => setTimeout(r, 0));
}

function mount(core: CoreClient, container: WardrobeContainer | null, chatId: string | null = null) {
  const c = signal<WardrobeContainer | null>(container);
  const archived = signal(false);
  const data = TestBed.runInInjectionContext(() =>
    createWardrobeListData(core, { container: c, chatId: () => chatId, includeArchived: archived }),
  );
  return { data, c, archived };
}

describe('createWardrobeListData (v4 `useWardrobeListData`)', () => {
  it('a character container lists its merged pool everywhere; only own garments are manageable', async () => {
    const { core } = fakeCore(tiers);
    const { data } = mount(core, { scope: 'character', id: 'alice' }, 'chat-1');
    await settle();
    const ids = data.listItems().map((i) => i.id);
    expect(ids).toEqual(expect.arrayContaining(['greatcoat', 'cape', 'watch']));
    expect(data.resolutionPool()).toBe(data.listItems());
    expect(data.characterItems()).toBe(data.listItems());
    expect(data.projectId()).toBe('p1');
    expect(data.canManage(item('greatcoat', { characterId: 'alice' }))).toBe(true);
    expect(data.canManage(item('watch'))).toBe(false);
  });

  it('a shared container lists exactly its own items; General joins only the resolution pool', async () => {
    const { core } = fakeCore(tiers);
    const { data } = mount(core, { scope: 'project', id: 'p1' });
    await settle();
    expect(data.listItems().map((i) => i.id)).toEqual(['cape']);
    expect(data.resolutionPool().map((i) => i.id)).toEqual(['cape', 'watch']);
    expect(data.characterItems()).toEqual([]);
    expect(data.projectId()).toBeNull();
    expect(data.canManage(item('cape'))).toBe(true);
    expect(data.canManage(item('watch'))).toBe(false);
  });

  it('no container reads nothing and lists nothing', async () => {
    const { core, seen } = fakeCore(tiers);
    const { data } = mount(core, null);
    await settle();
    expect(seen).toEqual([]);
    expect(data.listItems()).toEqual([]);
    expect(data.loading()).toBe(false);
  });

  it('flipping Show archived is a NEW read of every tier, with "Loading…" until it lands', async () => {
    let release!: () => void;
    let gate: Promise<void> = Promise.resolve();
    const { core, seen } = fakeCore(async (req) => {
      await gate;
      return tiers(req);
    });
    const { data, archived } = mount(core, GENERAL_CONTAINER);
    await settle();
    expect(seen.map((r) => r['includeArchived'])).toEqual([false]);
    gate = new Promise((r) => (release = r));
    archived.set(true);
    await settle();
    expect(seen.map((r) => r['includeArchived'])).toEqual([false, true]);
    expect(data.loading()).toBe(true);
    release();
    await settle();
    expect(data.loading()).toBe(false);
  });

  it('a reload of the view on display repaints in place — no "Loading…" (v4 isLoading, not isFetching)', async () => {
    let release!: () => void;
    let gate: Promise<void> = Promise.resolve();
    const { core } = fakeCore(async (req) => {
      await gate;
      return tiers(req);
    });
    const { data } = mount(core, GENERAL_CONTAINER);
    await settle();
    gate = new Promise((r) => (release = r));
    const pending = data.reload();
    await settle();
    expect(data.loading()).toBe(false);
    expect(data.listItems().map((i) => i.id)).toEqual(['watch']);
    release();
    await pending;
  });

  it('a late answer for a container no longer browsed is dropped', async () => {
    let releaseFirst!: () => void;
    const first = new Promise<void>((r) => (releaseFirst = r));
    let calls = 0;
    const { core } = fakeCore(async (req) => {
      if (req['type'] === 'projectWardrobeList' && ++calls === 1) await first;
      return tiers(req);
    });
    const { data, c } = mount(core, { scope: 'project', id: 'p1' });
    TestBed.tick();
    c.set(GENERAL_CONTAINER);
    await settle();
    releaseFirst();
    await settle();
    expect(data.listItems().map((i) => i.id)).toEqual(['watch']);
  });
});
