import { describe, expect, it, vi } from 'vitest';

import type { CoreClient } from '../core/core-client';
import type { CoreRequest, WardrobeItemDto } from '../core/core-contract';
import {
  containerCreateRequest,
  containerDeleteRequest,
  containerListRequest,
  containerUpdateRequest,
  deleteWardrobeItem,
  duplicateWardrobeItem,
  loadCharacterWardrobeItems,
  loadWardrobeContainerItems,
  toggleItemDefault,
} from './wardrobe.api';
import { GENERAL_CONTAINER, type WardrobeContainer } from './wardrobe-container';

const CHAR_CONTAINER: WardrobeContainer = { scope: 'character', id: 'c1' };
const PROJECT_CONTAINER: WardrobeContainer = { scope: 'project', id: 'p1' };
const GROUP_CONTAINER: WardrobeContainer = { scope: 'group', id: 'g1' };

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

describe('the container -> verb router (v4 wardrobeCollectionUrl / wardrobeItemUrl)', () => {
  it('lists each container over its own verb (v4 :57-67)', () => {
    expect(containerListRequest(CHAR_CONTAINER)).toEqual({
      type: 'characterWardrobeList',
      characterId: 'c1',
    });
    expect(containerListRequest(PROJECT_CONTAINER)).toEqual({
      type: 'projectWardrobeList',
      projectId: 'p1',
    });
    expect(containerListRequest(GROUP_CONTAINER)).toEqual({
      type: 'groupWardrobeList',
      groupId: 'g1',
    });
    expect(containerListRequest(GENERAL_CONTAINER)).toEqual({ type: 'wardrobeList' });
  });

  it('creates into each container (the POST arm of v4 wardrobeCollectionUrl)', () => {
    const item = { title: 'x' };
    expect(containerCreateRequest(CHAR_CONTAINER, item)).toEqual({
      type: 'characterWardrobeCreate',
      characterId: 'c1',
      item,
    });
    expect(containerCreateRequest(PROJECT_CONTAINER, item)).toEqual({
      type: 'projectWardrobeCreate',
      projectId: 'p1',
      item,
    });
    expect(containerCreateRequest(GROUP_CONTAINER, item)).toEqual({
      type: 'groupWardrobeCreate',
      groupId: 'g1',
      item,
    });
    expect(containerCreateRequest(GENERAL_CONTAINER, item)).toEqual({
      type: 'wardrobeCreate',
      item,
    });
  });

  it('updates and deletes one item in each container (v4 wardrobeItemUrl, :70-72)', () => {
    const item = { isDefault: true };
    expect(containerUpdateRequest(CHAR_CONTAINER, 'i1', item)).toEqual({
      type: 'characterWardrobeUpdate',
      characterId: 'c1',
      itemId: 'i1',
      item,
    });
    expect(containerUpdateRequest(PROJECT_CONTAINER, 'i1', item)).toEqual({
      type: 'projectWardrobeUpdate',
      projectId: 'p1',
      itemId: 'i1',
      item,
    });
    expect(containerUpdateRequest(GROUP_CONTAINER, 'i1', item)).toEqual({
      type: 'groupWardrobeUpdate',
      groupId: 'g1',
      itemId: 'i1',
      item,
    });
    expect(containerUpdateRequest(GENERAL_CONTAINER, 'i1', item)).toEqual({
      type: 'wardrobeUpdate',
      itemId: 'i1',
      item,
    });
    expect(containerDeleteRequest(GROUP_CONTAINER, 'i1')).toEqual({
      type: 'groupWardrobeDelete',
      groupId: 'g1',
      itemId: 'i1',
    });
    expect(containerDeleteRequest(GENERAL_CONTAINER, 'i1')).toEqual({
      type: 'wardrobeDelete',
      itemId: 'i1',
    });
  });

  it('refuses loudly rather than addressing the wrong tier when an id is missing', () => {
    expect(() => containerListRequest({ scope: 'group', id: null })).toThrow(
      'Wardrobe container of scope "group" has no id',
    );
  });
});

describe('loadWardrobeContainerItems (v4 use-wardrobe-container-items.ts:46-97)', () => {
  it('reads the container plus General as a resolution pool, de-duped (v4 :66-79)', async () => {
    const { core, seen } = stubCore((req) => {
      switch (req.type as string) {
        case 'projectWardrobeList':
          return { wardrobeItems: [dto({ id: 'a', title: 'project-a' }), dto({ id: 'b' })] };
        case 'wardrobeList':
          return { wardrobeItems: [dto({ id: 'a', title: 'general-a' }), dto({ id: 'z' })] };
        default:
          return new Error(`unexpected ${req.type}`);
      }
    });
    const result = await loadWardrobeContainerItems(core, PROJECT_CONTAINER);
    // `items` is EXACTLY the container's own contents — no tier merging.
    expect(result.items.map((i) => i.id)).toEqual(['a', 'b']);
    // The pool appends General archetypes; the container's own copy of `a`
    // wins because it was pushed first (v4 :75-79).
    expect(result.resolutionItems.map((i) => i.id)).toEqual(['a', 'b', 'z']);
    expect(result.resolutionItems[0].title).toBe('project-a');
    expect(seen.map((r) => r.type).sort()).toEqual(['projectWardrobeList', 'wardrobeList']);
  });

  it('General is its own resolution pool — no second read fires (v4 :69)', async () => {
    const { core, seen } = stubCore(() => ({ wardrobeItems: [dto({ id: 'g1' })] }));
    const result = await loadWardrobeContainerItems(core, GENERAL_CONTAINER);
    expect(seen).toEqual([{ type: 'wardrobeList' }]);
    expect(result.items.map((i) => i.id)).toEqual(['g1']);
    expect(result.resolutionItems.map((i) => i.id)).toEqual(['g1']);
  });

  it('a group container reads the group verb (§Shared contract item 1)', async () => {
    const { core, seen } = stubCore((req) =>
      (req.type as string) === 'groupWardrobeList' ? { wardrobeItems: [dto({ id: 'liv' })] } : { wardrobeItems: [] },
    );
    const result = await loadWardrobeContainerItems(core, GROUP_CONTAINER);
    expect(seen[0]).toEqual({ type: 'groupWardrobeList', groupId: 'g1' });
    expect(result.items.map((i) => i.id)).toEqual(['liv']);
  });

  it('a failed container read empties BOTH lists (v4 :81-85) — never a half-loaded editable set', async () => {
    const { core } = stubCore((req) =>
      (req.type as string) === 'groupWardrobeList' ? new Error('HTTP 500') : { wardrobeItems: [dto({ id: 'z' })] },
    );
    const result = await loadWardrobeContainerItems(core, GROUP_CONTAINER);
    expect(result).toEqual({ items: [], resolutionItems: [] });
  });

  it('a failed GENERAL read still yields the container itself (v4 :76 `.ok` check)', async () => {
    const { core } = stubCore((req) =>
      (req.type as string) === 'wardrobeList' ? new Error('HTTP 500') : { wardrobeItems: [dto({ id: 'p' })] },
    );
    const result = await loadWardrobeContainerItems(core, PROJECT_CONTAINER);
    expect(result.items.map((i) => i.id)).toEqual(['p']);
    expect(result.resolutionItems.map((i) => i.id)).toEqual(['p']);
  });

  it('a character container (or null) is a no-op — that view merges tiers instead (v4 :50)', async () => {
    const { core, seen } = stubCore(() => ({}));
    expect(await loadWardrobeContainerItems(core, CHAR_CONTAINER)).toEqual({
      items: [],
      resolutionItems: [],
    });
    expect(await loadWardrobeContainerItems(core, null)).toEqual({
      items: [],
      resolutionItems: [],
    });
    expect(seen).toEqual([]);
  });
});

describe('the tier-routed row mutations (v4 wardrobe-control-dialog.tsx at d7263f39)', () => {
  it('toggleItemDefault PUTs the character route for owned items (v4 :499) with the flipped flag', async () => {
    const { core, seen } = stubCore(() => ({}));
    await toggleItemDefault(
      core,
      dto({ id: 'i1', characterId: 'c1', isDefault: false }),
      CHAR_CONTAINER,
    );
    expect(seen).toEqual([
      {
        type: 'characterWardrobeUpdate',
        characterId: 'c1',
        itemId: 'i1',
        item: { isDefault: true },
      },
    ]);
  });

  it('toggleItemDefault PUTs the global route for shared archetypes (v4 :500)', async () => {
    const { core, seen } = stubCore(() => ({}));
    await toggleItemDefault(
      core,
      dto({ id: 'i1', characterId: null, isDefault: true }),
      CHAR_CONTAINER,
    );
    expect(seen).toEqual([{ type: 'wardrobeUpdate', itemId: 'i1', item: { isDefault: false } }]);
  });

  it('browsing a shared container, the star targets THAT container (v4 :501)', async () => {
    const { core, seen } = stubCore(() => ({}));
    // The item's own `characterId` is null, which in the character view would
    // route to Quilltap General — here it must go to the group being browsed.
    await toggleItemDefault(
      core,
      dto({ id: 'i1', characterId: null, isDefault: false }),
      GROUP_CONTAINER,
    );
    expect(seen).toEqual([
      { type: 'groupWardrobeUpdate', groupId: 'g1', itemId: 'i1', item: { isDefault: true } },
    ]);
  });

  it('deleteWardrobeItem picks the same two arms (v4 :514-518)', async () => {
    const { core, seen } = stubCore(() => ({}));
    await deleteWardrobeItem(core, dto({ id: 'i1', characterId: 'c1' }), CHAR_CONTAINER);
    await deleteWardrobeItem(core, dto({ id: 'i2', characterId: null }), CHAR_CONTAINER);
    await deleteWardrobeItem(core, dto({ id: 'i3', characterId: null }), PROJECT_CONTAINER);
    expect(seen).toEqual([
      { type: 'characterWardrobeDelete', characterId: 'c1', itemId: 'i1' },
      { type: 'wardrobeDelete', itemId: 'i2' },
      { type: 'projectWardrobeDelete', projectId: 'p1', itemId: 'i3' },
    ]);
  });

  it('duplicateWardrobeItem POSTs into the target container and PRESERVES the Portrait Cue (v4 :565-580)', async () => {
    const { core, seen } = stubCore(() => ({}));
    const item = dto({
      id: 'i1',
      description: 'desc',
      imagePrompt: 'cue',
      types: ['top', 'bottom'],
      appropriateness: 'formal',
      isDefault: true,
      componentItemIds: ['x'],
      replace: true,
    });
    await duplicateWardrobeItem(core, CHAR_CONTAINER, item, 'Suit (copy)');
    await duplicateWardrobeItem(core, GROUP_CONTAINER, item, 'Suit (copy 2)');
    expect(seen).toEqual([
      {
        type: 'characterWardrobeCreate',
        characterId: 'c1',
        item: {
          title: 'Suit (copy)',
          description: 'desc',
          imagePrompt: 'cue',
          types: ['top', 'bottom'],
          appropriateness: 'formal',
          isDefault: true,
          componentItemIds: ['x'],
          replace: true,
        },
      },
      {
        type: 'groupWardrobeCreate',
        groupId: 'g1',
        item: {
          title: 'Suit (copy 2)',
          description: 'desc',
          imagePrompt: 'cue',
          types: ['top', 'bottom'],
          appropriateness: 'formal',
          isDefault: true,
          componentItemIds: ['x'],
          replace: true,
        },
      },
    ]);
  });

  it('a missing Portrait Cue duplicates as an explicit null, never an absent key', async () => {
    const { core, seen } = stubCore(() => ({}));
    await duplicateWardrobeItem(core, GENERAL_CONTAINER, dto({ id: 'i1' }), 'x (copy)');
    expect(seen[0]).toMatchObject({ type: 'wardrobeCreate' });
    expect((seen[0] as { item: Record<string, unknown> }).item).toHaveProperty(
      'imagePrompt',
      null,
    );
  });
});
