import { afterEach, describe, expect, it, vi } from 'vitest';

import type { CoreClient } from '../core/core-client';
import type { CoreRequest } from '../core/core-contract';
import {
  deleteWardrobeItemImage,
  generateWardrobeItemImage,
  listWardrobeItemImages,
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
