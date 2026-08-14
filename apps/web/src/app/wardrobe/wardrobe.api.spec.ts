import { describe, expect, it, vi } from 'vitest';

import type { CoreClient } from '../core/core-client';
import type { CoreRequest, WardrobeItemDto } from '../core/core-contract';
import {
  deleteWardrobeItem,
  duplicateWardrobeItem,
  loadCharacterWardrobeItems,
  toggleItemDefault,
} from './wardrobe.api';

type AnyRequest = CoreRequest & Record<string, unknown>;

/** A CoreClient stub that records every dispatch and answers from a router. */
function stubCore(
  route: (req: AnyRequest) => Record<string, unknown> | Error,
): { core: CoreClient; seen: AnyRequest[] } {
  const seen: AnyRequest[] = [];
  const dispatchData = vi.fn(async (req: CoreRequest) => {
    const r = req as AnyRequest;
    seen.push(r);
    const out = route(r);
    if (out instanceof Error) throw out;
    return out;
  });
  return { core: { dispatchData } as unknown as CoreClient, seen };
}

function dto(partial: Partial<WardrobeItemDto> & { id: string }): WardrobeItemDto {
  return {
    title: partial.id,
    types: ['top'],
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    ...partial,
  } as WardrobeItemDto;
}

describe('loadCharacterWardrobeItems (v4 use-character-wardrobe-items.ts:57-118)', () => {
  it('reads the four tiers and merges de-duped, personal tier winning (v4 :93-124)', async () => {
    const { core, seen } = stubCore((req) => {
      switch (req.type as string) {
        case 'chatGet':
          return { chat: { projectId: 'p1' } };
        case 'characterWardrobeList':
          return (req as { scope?: string }).scope === 'group'
            ? { wardrobeItems: [dto({ id: 'g', characterId: null })] }
            : { wardrobeItems: [dto({ id: 'a', title: 'personal-a' })] };
        case 'projectWardrobeList':
          return { wardrobeItems: [dto({ id: 'a', title: 'project-a' }), dto({ id: 'b' })] };
        case 'wardrobeList':
          return { wardrobeItems: [dto({ id: 'c' })] };
        default:
          return new Error(`unexpected ${req.type}`);
      }
    });

    const result = await loadCharacterWardrobeItems(core, 'char-1', { chatId: 'chat-1' });
    // The chat is fetched SOLELY to resolve the project tier (v4 :66-77), then
    // the four tier reads fire in parallel — personal, group, project, general.
    expect(seen.map((r) => r.type)).toEqual([
      'chatGet',
      'characterWardrobeList',
      'characterWardrobeList',
      'projectWardrobeList',
      'wardrobeList',
    ]);
    expect(seen.filter((r) => (r.type as string) === 'characterWardrobeList').map((r) => r['scope']))
      .toEqual([undefined, 'group']);
    expect(result.projectId).toBe('p1');
    expect(result.items.map((i) => i.id)).toEqual(['a', 'g', 'b', 'c']);
    // De-dup keeps the nearer tier's copy (v4 push() skips existing ids).
    expect(result.items[0].title).toBe('personal-a');
  });

  /**
   * v4 4.8.2 `8600c83f`: the group tier sits BETWEEN personal and project, so
   * a group's livery shadows a project's copy of the same item while a
   * character's personal copy shadows both — including the `isDefault: false`
   * personal copy that is how a character opts out of a shared default.
   */
  it('merges with precedence personal > group > project > general (v4 :102)', async () => {
    const { core } = stubCore((req) => {
      switch (req.type as string) {
        case 'characterWardrobeList':
          return (req as { scope?: string }).scope === 'group'
            ? {
                wardrobeItems: [
                  dto({ id: 'shared', title: 'group', characterId: null }),
                  dto({ id: 'livery', title: 'group-livery', characterId: null }),
                ],
              }
            : { wardrobeItems: [dto({ id: 'shared', title: 'personal' })] };
        case 'projectWardrobeList':
          return {
            wardrobeItems: [
              dto({ id: 'shared', title: 'project' }),
              dto({ id: 'livery', title: 'project-livery' }),
            ],
          };
        case 'wardrobeList':
          return { wardrobeItems: [dto({ id: 'shared', title: 'general' })] };
        default:
          return new Error(`unexpected ${req.type}`);
      }
    });

    const result = await loadCharacterWardrobeItems(core, 'char-1', { projectId: 'p1' });
    const byId = new Map(result.items.map((i) => [i.id, i.title]));
    expect(byId.get('shared')).toBe('personal');
    expect(byId.get('livery')).toBe('group-livery');
  });

  /**
   * Group items come back from `findArchetypesInMounts` with a null
   * `characterId`, exactly as project and General items do — so the row's
   * `isShared` computed lights on its own and they are wear-only without any
   * extra labeling here (v4 `wardrobe-item-row.tsx:84`).
   */
  it('group items arrive shared-shaped (characterId null), so they are wear-only', async () => {
    const { core } = stubCore((req) => {
      switch (req.type as string) {
        case 'characterWardrobeList':
          return (req as { scope?: string }).scope === 'group'
            ? { wardrobeItems: [dto({ id: 'g', characterId: null })] }
            : { wardrobeItems: [] };
        case 'wardrobeList':
          return { wardrobeItems: [] };
        default:
          return new Error(`unexpected ${req.type}`);
      }
    });
    const result = await loadCharacterWardrobeItems(core, 'char-1');
    expect(result.items).toHaveLength(1);
    expect(result.items[0].characterId ?? null).toBeNull();
  });

  it('the group tier fails soft — a server without the scope arm still yields the rest', async () => {
    const { core } = stubCore((req) => {
      switch (req.type as string) {
        case 'characterWardrobeList':
          if ((req as { scope?: string }).scope === 'group') {
            // A pre-4.8.2 server (or one whose group arm has not landed) —
            // the loader folds in nothing for it, as v4 skips a non-ok tier.
            return new Error('unknown field: scope');
          }
          return { wardrobeItems: [dto({ id: 'a' })] };
        case 'wardrobeList':
          return { wardrobeItems: [] };
        default:
          return new Error(`unexpected ${req.type}`);
      }
    });
    const result = await loadCharacterWardrobeItems(core, 'char-1');
    expect(result.items.map((i) => i.id)).toEqual(['a']);
  });

  it('skips the project read when no project tier resolves (v4 :82-84)', async () => {
    const { core, seen } = stubCore((req) => {
      switch (req.type as string) {
        case 'chatGet':
          return { chat: { projectId: null } };
        case 'characterWardrobeList':
        case 'wardrobeList':
          return { wardrobeItems: [] };
        default:
          return new Error(`unexpected ${req.type}`);
      }
    });
    const result = await loadCharacterWardrobeItems(core, 'char-1', { chatId: 'chat-1' });
    expect(seen.some((r) => (r.type as string) === 'projectWardrobeList')).toBe(false);
    expect(result.projectId).toBeNull();
  });

  it('an explicit projectId wins over chat derivation and no chatGet fires (v4 :64-66)', async () => {
    const { core, seen } = stubCore((req) => {
      switch (req.type as string) {
        case 'characterWardrobeList':
        case 'projectWardrobeList':
        case 'wardrobeList':
          return { wardrobeItems: [] };
        default:
          return new Error(`unexpected ${req.type}`);
      }
    });
    await loadCharacterWardrobeItems(core, 'char-1', { projectId: 'p9', chatId: 'chat-1' });
    expect(seen.some((r) => (r.type as string) === 'chatGet')).toBe(false);
    expect(seen.find((r) => (r.type as string) === 'projectWardrobeList')?.['projectId']).toBe('p9');
  });

  it('each tier read fails soft — a rejected global tier still yields the others (v4 :109-124 .ok checks)', async () => {
    const { core } = stubCore((req) => {
      switch (req.type as string) {
        case 'characterWardrobeList':
          return { wardrobeItems: [dto({ id: 'a' })] };
        case 'wardrobeList':
          // Lane P4.9f1's verb — unknown until unification; the loader folds
          // in nothing for it, exactly as v4 skips a non-ok tier response.
          return new Error('unknown request type');
        default:
          return new Error(`unexpected ${req.type}`);
      }
    });
    const result = await loadCharacterWardrobeItems(core, 'char-1');
    expect(result.items.map((i) => i.id)).toEqual(['a']);
  });

  it('a null characterId short-circuits to empty (v4 :58-61)', async () => {
    const { core, seen } = stubCore(() => new Error('no dispatch expected'));
    const result = await loadCharacterWardrobeItems(core, null, { chatId: 'chat-1' });
    expect(result.items).toEqual([]);
    expect(seen).toEqual([]);
  });
});

describe('the tier-routed row mutations (v4 wardrobe-control-dialog.tsx)', () => {
  it('toggleItemDefault PUTs the character route for owned items (v4 :361) with the flipped flag', async () => {
    const { core, seen } = stubCore(() => ({}));
    await toggleItemDefault(core, dto({ id: 'i1', characterId: 'c1', isDefault: false }));
    expect(seen).toEqual([
      {
        type: 'characterWardrobeUpdate',
        characterId: 'c1',
        itemId: 'i1',
        item: { isDefault: true },
      },
    ]);
  });

  it('toggleItemDefault PUTs the global route for shared archetypes (v4 :362)', async () => {
    const { core, seen } = stubCore(() => ({}));
    await toggleItemDefault(core, dto({ id: 'i1', characterId: null, isDefault: true }));
    expect(seen).toEqual([{ type: 'wardrobeUpdate', itemId: 'i1', item: { isDefault: false } }]);
  });

  it('deleteWardrobeItem picks the same tier pair (v4 :386-388)', async () => {
    const { core, seen } = stubCore(() => ({}));
    await deleteWardrobeItem(core, dto({ id: 'i1', characterId: 'c1' }));
    await deleteWardrobeItem(core, dto({ id: 'i2', characterId: null }));
    expect(seen).toEqual([
      { type: 'characterWardrobeDelete', characterId: 'c1', itemId: 'i1' },
      { type: 'wardrobeDelete', itemId: 'i2' },
    ]);
  });

  it('duplicateWardrobeItem always POSTs the character route with the v4 payload (:421-433, no imagePrompt)', async () => {
    const { core, seen } = stubCore(() => ({}));
    await duplicateWardrobeItem(
      core,
      'c1',
      dto({
        id: 'i1',
        description: 'desc',
        imagePrompt: 'cue',
        types: ['top', 'bottom'],
        appropriateness: 'formal',
        isDefault: true,
        componentItemIds: ['x'],
        replace: true,
      }),
      'Suit (copy)',
    );
    expect(seen).toEqual([
      {
        type: 'characterWardrobeCreate',
        characterId: 'c1',
        item: {
          title: 'Suit (copy)',
          description: 'desc',
          types: ['top', 'bottom'],
          appropriateness: 'formal',
          isDefault: true,
          componentItemIds: ['x'],
          replace: true,
        },
      },
    ]);
  });
});
