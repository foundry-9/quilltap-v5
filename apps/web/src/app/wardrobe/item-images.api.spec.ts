import { afterEach, describe, expect, it, vi } from 'vitest';

import type { CoreClient } from '../core/core-client';
import { CoreDispatchError, type CoreRequest } from '../core/core-contract';
import {
  deleteWardrobeItemImage,
  generateWardrobeItemImage,
  listWardrobeItemImages,
  listWardrobeSaveTargets,
  saveWardrobeImageToStore,
  setCurrentWardrobeItemImage,
  uploadWardrobeItemImage,
  WardrobeImageRequestError,
  wardrobeImageThumbnailUrl,
  wardrobeImageUrl,
  wardrobeItemImagesUrl,
} from './item-images.api';

/**
 * P4.D261 — v4 `7c8572869` `lib/wardrobe/item-images-client.ts:51-160` at the
 * pin `f5e953a3f`: the URL's parameter ORDER (scope, id, action — v4's
 * `URLSearchParams`), the multipart field names, the request bodies; the three
 * dispatch verbs' bodies (C2 §6).
 */
afterEach(() => vi.restoreAllMocks());

function fakeCore(answer: Record<string, unknown> = {}): {
  core: CoreClient;
  seen: Array<Record<string, unknown>>;
} {
  const seen: Array<Record<string, unknown>> = [];
  const core = {
    dispatchData: vi.fn(async (req: CoreRequest) => {
      seen.push(req as unknown as Record<string, unknown>);
      return answer;
    }),
  } as unknown as CoreClient;
  return { core, seen };
}

function stubFetch(status: number, body: unknown): ReturnType<typeof vi.fn> {
  const fn = vi.fn(async () => ({
    ok: status >= 200 && status < 300,
    status,
    json: async () => body,
  }));
  vi.stubGlobal('fetch', fn);
  return fn;
}

describe('wardrobeItemImagesUrl (v4 :51-62)', () => {
  it('orders the query scope, id, action, and encodes the item id', () => {
    expect(wardrobeItemImagesUrl('item 1', { scope: 'character', id: 'c1' }, 'upload')).toBe(
      '/api/v1/wardrobe/item%201/images?scope=character&id=c1&action=upload',
    );
    expect(wardrobeItemImagesUrl('i', { scope: 'project', id: 'p1' })).toBe(
      '/api/v1/wardrobe/i/images?scope=project&id=p1',
    );
  });

  it('General carries no id', () => {
    expect(wardrobeItemImagesUrl('i', { scope: 'general', id: null }, 'generate')).toBe(
      '/api/v1/wardrobe/i/images?scope=general&action=generate',
    );
  });
  // P4.D286 — v4 `9ecebf986` `item-images-client.ts:51-58`: `link-image` joins
  // the union (its helper lands with the import-from-image vertical — R-L).
  it('names the link-image action', () => {
    expect(wardrobeItemImagesUrl('i', { scope: 'character', id: 'c1' }, 'link-image')).toBe(
      '/api/v1/wardrobe/i/images?scope=character&id=c1&action=link-image',
    );
  });
});

describe('the picture byte URLs (v4 :64-72, through the origin-aware apiUrl)', () => {
  it('thumbnail and full size are the files route', () => {
    expect(wardrobeImageThumbnailUrl('file-9')).toBe('/api/v1/files/file-9?action=thumbnail');
    expect(wardrobeImageUrl('file-9')).toBe('/api/v1/files/file-9');
  });
});

describe('the dispatch verbs (C2 §6)', () => {
  it('list / set-current / delete name the container; General carries no containerId', async () => {
    const { core, seen } = fakeCore({ current: null, images: [] });
    await listWardrobeItemImages(core, 'i1', { scope: 'group', id: 'g1' });
    await setCurrentWardrobeItemImage(core, 'i1', { scope: 'general', id: null }, 'f1');
    await deleteWardrobeItemImage(core, 'i1', { scope: 'character', id: 'c1' }, 'f2');
    expect(seen).toEqual([
      { type: 'wardrobeItemImagesList', scope: 'group', containerId: 'g1', itemId: 'i1' },
      { type: 'wardrobeItemImageSetCurrent', scope: 'general', itemId: 'i1', fileId: 'f1' },
      {
        type: 'wardrobeItemImageDelete',
        scope: 'character',
        containerId: 'c1',
        itemId: 'i1',
        fileId: 'f2',
      },
    ]);
  });
});

describe('generate + upload — the REST route (v4 :102-130)', () => {
  it('generate POSTs JSON: the override profile, or {} for the designated one', async () => {
    const fetchFn = stubFetch(201, { current: 'f', image: {}, rerouted: false });
    await generateWardrobeItemImage('i1', { scope: 'character', id: 'c1' });
    await generateWardrobeItemImage('i1', { scope: 'character', id: 'c1' }, 'ip-2');
    expect(fetchFn.mock.calls[0][0]).toBe(
      '/api/v1/wardrobe/i1/images?scope=character&id=c1&action=generate',
    );
    const init0 = fetchFn.mock.calls[0][1] as RequestInit;
    expect(init0.method).toBe('POST');
    expect((init0.headers as Record<string, string>)['Content-Type']).toBe('application/json');
    expect(init0.body).toBe('{}');
    expect((fetchFn.mock.calls[1][1] as RequestInit).body).toBe('{"imageProfileId":"ip-2"}');
  });

  it('upload POSTs multipart `file` + `kind` (default uploaded) with no content-type of its own', async () => {
    const fetchFn = stubFetch(201, { current: 'f', image: {} });
    const file = new File(['x'], 'p.png', { type: 'image/png' });
    await uploadWardrobeItemImage('i1', { scope: 'general', id: null }, file);
    await uploadWardrobeItemImage('i1', { scope: 'general', id: null }, file, 'imported');
    expect(fetchFn.mock.calls[0][0]).toBe('/api/v1/wardrobe/i1/images?scope=general&action=upload');
    const init = fetchFn.mock.calls[0][1] as RequestInit;
    expect(init.method).toBe('POST');
    expect(init.headers).toBeUndefined();
    const form = init.body as FormData;
    expect(form.get('file')).toBeInstanceOf(File);
    expect(form.get('kind')).toBe('uploaded');
    expect(((fetchFn.mock.calls[1][1] as RequestInit).body as FormData).get('kind')).toBe('imported');
  });

  it('a failure throws with the body’s error, the status and the refusal details (v4 :74-100)', async () => {
    const trail = [{ profileName: 'Desk', detail: 'nope' }];
    stubFetch(422, { error: 'The image provider declined to draw this garment', details: { trail, refused: true } });
    const err = await generateWardrobeItemImage('i1', { scope: 'general', id: null }).catch((e) => e);
    expect(err).toBeInstanceOf(WardrobeImageRequestError);
    expect(err.message).toBe('The image provider declined to draw this garment');
    expect(err.status).toBe(422);
    expect(err.refusal).toEqual({ trail, refused: true });

    stubFetch(500, null);
    const bare = await generateWardrobeItemImage('i1', { scope: 'general', id: null }).catch((e) => e);
    expect(bare.message).toBe('Request failed (500)');
    expect(bare.refusal).toBeNull();
  });
});

/**
 * P4.D277 — `01a83539d`'s two wardrobe save actions (v4
 * `item-images-client.ts:51-57`, `SaveImageDialog.tsx:56-72, 130-201` at the
 * pin), ridden as C2's dispatch verbs `wardrobeItemImageSaveTargets` /
 * `wardrobeItemImageSaveToStore` (landed server-side by P4.D275).
 */
describe('the save-to-store helpers (C2 wardrobe images, 01a83539d)', () => {
  it('keeps the two new actions in the URL union, for wardrobeItemImagesUrl parity (v4 :51-57)', () => {
    expect(wardrobeItemImagesUrl('item-1', { scope: 'character', id: 'char-1' }, 'save-targets')).toBe(
      '/api/v1/wardrobe/item-1/images?scope=character&id=char-1&action=save-targets',
    );
    expect(wardrobeItemImagesUrl('item-1', { scope: 'general', id: null }, 'save-to-store')).toBe(
      '/api/v1/wardrobe/item-1/images?scope=general&action=save-to-store',
    );
  });

  it('reads the save targets for a character container and for General (no containerId)', async () => {
    const albums = [
      { mountPointId: 'mp-1', name: 'Vivienne', kind: 'character', characterId: 'char-1' },
      { mountPointId: 'mp-g', name: 'Quilltap General', kind: 'general', isDefault: true },
    ];
    const { core, seen } = fakeCore({ albums });
    expect(await listWardrobeSaveTargets(core, 'item-1', { scope: 'character', id: 'char-1' })).toEqual(albums);
    await listWardrobeSaveTargets(core, 'item-2', { scope: 'general', id: null });
    expect(seen).toEqual([
      { type: 'wardrobeItemImageSaveTargets', scope: 'character', containerId: 'char-1', itemId: 'item-1' },
      { type: 'wardrobeItemImageSaveTargets', scope: 'general', itemId: 'item-2' },
    ]);
  });

  it('reads an absent albums key as an empty list (v4 SaveImageDialog.tsx:135-136)', async () => {
    const { core } = fakeCore({});
    expect(await listWardrobeSaveTargets(core, 'item-1', { scope: 'project', id: 'p-1' })).toEqual([]);
  });

  it('saves with {fileId, mountPointId, caption} — caption OMITTED when undefined, never null; no tags', async () => {
    const saved = {
      saved: true,
      mountPoint: 'Quilltap General',
      relativePath: 'photos/gown.webp',
      linkId: 'l-1',
      keptAt: '2026-10-09T12:00:00.000Z',
      fileId: 'f-1',
      sha256: 'abc',
    };
    const { core, seen } = fakeCore(saved);
    expect(
      await saveWardrobeImageToStore(core, 'item-1', { scope: 'character', id: 'char-1' }, {
        fileId: 'f-1',
        mountPointId: 'mp-g',
        caption: 'For the ball',
      }),
    ).toEqual(saved);
    await saveWardrobeImageToStore(core, 'item-2', { scope: 'general', id: null }, {
      fileId: 'f-2',
      mountPointId: 'mp-g',
    });
    expect(seen[0]).toEqual({
      type: 'wardrobeItemImageSaveToStore',
      scope: 'character',
      containerId: 'char-1',
      itemId: 'item-1',
      fileId: 'f-1',
      mountPointId: 'mp-g',
      caption: 'For the ball',
    });
    expect(seen[1]).toEqual({
      type: 'wardrobeItemImageSaveToStore',
      scope: 'general',
      itemId: 'item-2',
      fileId: 'f-2',
      mountPointId: 'mp-g',
    });
    expect('caption' in seen[1]).toBe(false);
    expect('tags' in seen[0]).toBe(false);
  });

  it('propagates a CoreDispatchError with kind / code / keptAt intact', async () => {
    const core = {
      dispatchData: vi.fn(async () => {
        throw new CoreDispatchError({
          kind: 'conflict',
          message: 'That picture is already in this album',
          code: 'ALREADY_SAVED',
          alreadySaved: { relativePath: 'photos/gown.webp', keptAt: '2026-10-01T00:00:00.000Z' },
        } as never);
      }),
    } as unknown as CoreClient;
    const err = await saveWardrobeImageToStore(core, 'item-1', { scope: 'general', id: null }, {
      fileId: 'f-1',
      mountPointId: 'mp-g',
    }).catch((e) => e);
    expect(err).toBeInstanceOf(CoreDispatchError);
    expect(err.kind).toBe('conflict');
    expect(err.code).toBe('ALREADY_SAVED');
    expect(err.keptAt).toBe('2026-10-01T00:00:00.000Z');
  });
});
