/**
 * The pronoun-aware filter (v4 `lib/hooks/use-pronoun-guidance-filter.ts` at
 * `236c04e35`): applies iff the character's pronouns are a standard set;
 * `aware` starts on; `offer` / `countHeldBack` over a four-item fixture.
 */
import { signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { Pronouns } from '../core/core-contract';
import { createPronounGuidanceFilter, type PronounGuidanceFilter } from './pronoun-guidance-filter';

const ITEMS = [
  { id: 'gown', pronounGuidance: ['she'] },
  { id: 'waistcoat', pronounGuidance: ['he'] },
  { id: 'cloak' },
  { id: 'kilt', pronounGuidance: ['he', 'they'] },
];

async function filterFor(
  pronouns: Pronouns | null,
  characterId: string | null = 'c1',
): Promise<{ filter: PronounGuidanceFilter; reads: string[] }> {
  const reads: string[] = [];
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    providers: [
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      {
        provide: CoreClient,
        useValue: {
          dispatchData: async (req: { type: string }) => {
            reads.push(req.type);
            return { character: { id: 'c1', name: 'Vivienne', pronouns } };
          },
        },
      },
    ],
  });
  const id = signal(characterId);
  const filter = TestBed.runInInjectionContext(() =>
    createPronounGuidanceFilter(id, TestBed.inject(CoreClient)),
  );
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    TestBed.tick();
  }
  return { filter, reads };
}

describe('createPronounGuidanceFilter (v4 usePronounGuidanceFilter)', () => {
  it('applies to a standard set; aware starts on and offers only what fits', async () => {
    const { filter } = await filterFor({ subject: 'he', object: 'him', possessive: 'his' });
    expect(filter.applies()).toBe(true);
    expect(filter.aware()).toBe(true);
    expect(filter.offer(ITEMS).map((i) => i.id)).toEqual(['waistcoat', 'cloak', 'kilt']);
    expect(filter.countHeldBack(ITEMS)).toBe(1);
  });

  it('with the switch off offers everything — and still counts what guidance would hide', async () => {
    const { filter } = await filterFor({ subject: 'she', object: 'her', possessive: 'her' });
    filter.setAware(false);
    expect(filter.aware()).toBe(false);
    expect(filter.offer(ITEMS)).toHaveLength(4);
    expect(filter.countHeldBack(ITEMS)).toBe(2);
  });

  it('does not apply to custom or unset pronouns, and hides nothing', async () => {
    const custom = await filterFor({ subject: 'xe', object: 'xem', possessive: 'xyr' });
    expect(custom.filter.applies()).toBe(false);
    expect(custom.filter.offer(ITEMS)).toHaveLength(4);
    const unset = await filterFor(null);
    expect(unset.filter.applies()).toBe(false);
  });

  it('with no character reads nothing and does not apply', async () => {
    const { filter, reads } = await filterFor(
      { subject: 'he', object: 'him', possessive: 'his' },
      null,
    );
    expect(reads).toEqual([]);
    expect(filter.applies()).toBe(false);
  });
});
