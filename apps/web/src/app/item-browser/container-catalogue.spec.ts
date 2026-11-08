import { TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { injectContainerCatalogue } from './container-catalogue';

/**
 * v4 `lib/hooks/use-container-catalogue.ts` at the pin `624d0d25c` (NEW at
 * `b2b783b47`) — the characters, projects and groups a tiered browser's
 * container selects offer. v5 reads them through the dispatch LIST verbs
 * (`characterList` / `projectList` / `groupList`), each failing soft (the
 * `inventory-control-dialog.ts` precedent). Transcribed from v4
 * `__tests__/unit/lib/hooks/use-container-catalogue.test.tsx` (2 cases), plus
 * the `Untitled …` rows and a failed read settling the characters.
 */

type Req = { type: string };

function make(route: (req: Req) => unknown) {
  const seen: Req[] = [];
  TestBed.configureTestingModule({
    providers: [
      {
        provide: CoreClient,
        useValue: {
          dispatchData: vi.fn(async (req: Req) => {
            seen.push(req);
            const out = route(req);
            if (out instanceof Error) throw out;
            return out;
          }),
        },
      },
    ],
  });
  const catalogue = TestBed.runInInjectionContext(() => injectContainerCatalogue());
  return { catalogue, seen };
}

const settle = async () => {
  for (let i = 0; i < 5; i++) await Promise.resolve();
};

describe('injectContainerCatalogue (v4 useContainerCatalogue)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('reads all three lists, sorted by name, with placeholders for unnamed rows', async () => {
    const { catalogue, seen } = make((req) => {
      if (req.type === 'characterList')
        return {
          characters: [
            { id: 'c2', name: 'Zelda' },
            { id: 'c1', name: 'Ada', avatarUrl: '/a.webp' },
          ],
        };
      if (req.type === 'projectList')
        return {
          projects: [
            { id: 'p2', name: 'Opus' },
            { id: 'p1', name: '' },
          ],
        };
      return {
        groups: [
          { id: 'g1', name: '' },
          { id: 'g2', name: 'Crew' },
        ],
      };
    });
    expect(catalogue.loading()).toBe(true);
    expect(catalogue.charactersSettled()).toBe(false);
    await settle();
    expect(catalogue.loading()).toBe(false);
    expect(seen.map((r) => r.type).sort()).toEqual(['characterList', 'groupList', 'projectList']);
    expect(catalogue.characters().map((c) => c.id)).toEqual(['c1', 'c2']);
    expect(catalogue.characters()[0].avatarUrl).toBe('/a.webp');
    expect(catalogue.projects()).toEqual([
      { id: 'p2', name: 'Opus' },
      { id: 'p1', name: 'Untitled project' },
    ]);
    expect(catalogue.groups()).toEqual([
      { id: 'g2', name: 'Crew' },
      { id: 'g1', name: 'Untitled group' },
    ]);
    expect(catalogue.charactersSettled()).toBe(true);
  });

  it('settles the characters read even when it fails, leaving the list empty', async () => {
    const { catalogue } = make((req) => {
      if (req.type === 'characterList') return new Error('boom');
      if (req.type === 'projectList') return { projects: [] };
      return {};
    });
    await settle();
    expect(catalogue.charactersSettled()).toBe(true);
    expect(catalogue.characters()).toEqual([]);
    expect(catalogue.groups()).toEqual([]);
    expect(catalogue.loading()).toBe(false);
  });

  it('a failed project or group read leaves that list empty and the others filled', async () => {
    const { catalogue } = make((req) => {
      if (req.type === 'characterList') return { characters: [{ id: 'c1', name: 'Ada' }] };
      return new Error('nope');
    });
    await settle();
    expect(catalogue.characters()).toEqual([{ id: 'c1', name: 'Ada' }]);
    expect(catalogue.projects()).toEqual([]);
    expect(catalogue.groups()).toEqual([]);
  });

  it('sorts with a plain localeCompare (no options) — v4 `byName`', async () => {
    const { catalogue } = make((req) =>
      req.type === 'characterList'
        ? { characters: [{ id: 'b', name: 'beta' }, { id: 'a', name: 'Alpha' }, { id: 'e', name: 'Échelle' }] }
        : {},
    );
    await settle();
    const names = ['beta', 'Alpha', 'Échelle'].sort((x, y) => x.localeCompare(y));
    expect(catalogue.characters().map((c) => c.name)).toEqual(names);
  });
});
