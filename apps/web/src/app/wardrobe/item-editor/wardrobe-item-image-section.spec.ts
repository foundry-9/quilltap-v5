import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type { CoreRequest, ImageProfileDto, WardrobeItemDto } from '../../core/core-contract';
import { ToastService } from '../../ui/toast.service';
import type { WardrobeContainer } from '../wardrobe-container';
import { WardrobeItemImageSection } from './wardrobe-item-image-section';

/**
 * P4.D261 — v4 `7c8572869` `components/wardrobe/__tests__/wardrobe-item-images-
 * ui.test.tsx` (the section ×2) at the pin `f5e953a3f`, transcribed, plus every
 * string and flow of `WardrobeItemImageSection.tsx` (read whole at the pin):
 * the empty frame, the captions, the client upload checks, the generate
 * toasts and the 422 refusal notice, the picker's option labels, the history
 * strip's Make current / Delete.
 */
const IMAGES_BODY = {
  current: 'file-2',
  images: [
    {
      fileId: 'file-2',
      url: '/api/v1/files/file-2',
      thumbnailUrl: '/api/v1/files/file-2?action=thumbnail',
      source: 'GENERATED',
      createdAt: '2026-10-02T00:00:00.000Z',
      model: 'flux-pro',
      prompt: 'a beaded flapper dress',
    },
    {
      fileId: 'file-1',
      url: '/api/v1/files/file-1',
      thumbnailUrl: '/api/v1/files/file-1?action=thumbnail',
      source: 'UPLOADED',
      createdAt: '2026-10-01T00:00:00.000Z',
    },
  ],
};

function makeItem(over: Partial<WardrobeItemDto> = {}): WardrobeItemDto {
  return {
    id: 'item-1',
    characterId: 'char-1',
    title: 'Midnight Lightning Flapper Dress',
    types: ['top'],
    componentItemIds: [],
    isDefault: false,
    replace: false,
    archivedAt: null,
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    ...over,
  } as WardrobeItemDto;
}

function profile(over: Partial<ImageProfileDto>): ImageProfileDto {
  return {
    id: 'ip-1',
    name: 'House Artist',
    provider: 'OPENAI',
    modelName: 'gpt-image-1',
    isDefault: false,
    isDangerousCompatible: false,
    ...over,
  } as ImageProfileDto;
}

interface Harness {
  fixture: ComponentFixture<WardrobeItemImageSection>;
  el: HTMLElement;
  seen: Array<Record<string, unknown>>;
  changed: number[];
  settle: () => Promise<void>;
}

async function mount(opts: {
  item: WardrobeItemDto | null;
  container: WardrobeContainer | null;
  images?: unknown;
  profiles?: ImageProfileDto[];
  designated?: string | null;
}): Promise<Harness> {
  const seen: Array<Record<string, unknown>> = [];
  let images = opts.images ?? IMAGES_BODY;
  const core = {
    dispatchData: vi.fn(async (req: CoreRequest) => {
      const r = req as unknown as Record<string, unknown>;
      seen.push(r);
      switch (r['type']) {
        case 'wardrobeItemImagesList':
          return images;
        case 'imageProfileList':
          return { profiles: opts.profiles ?? [] };
        case 'wardrobeItemImageSetCurrent':
          images = { ...(images as object), current: r['fileId'] };
          return { current: r['fileId'] };
        case 'wardrobeItemImageDelete':
          return { current: 'file-1' };
        default:
          throw new Error(`unexpected ${String(r['type'])}`);
      }
    }),
    dispatchExpect: vi.fn(async () => ({
      type: 'chatSettings',
      data: {
        avatarDisplayMode: 'ALWAYS',
        avatarDisplayStyle: 'CIRCULAR',
        ...(opts.designated !== undefined
          ? { wardrobeImageSettings: { imageProfileId: opts.designated, generateFromTools: false } }
          : {}),
      },
    })),
  } as unknown as CoreClient;
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [WardrobeItemImageSection],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: core }],
  });
  const fixture = TestBed.createComponent(WardrobeItemImageSection);
  fixture.componentRef.setInput('item', opts.item);
  fixture.componentRef.setInput('container', opts.container);
  const changed: number[] = [];
  fixture.componentInstance.imageChanged.subscribe(() => changed.push(1));
  const settle = async (): Promise<void> => {
    for (let i = 0; i < 8; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
  };
  fixture.detectChanges();
  await settle();
  return { fixture, el: fixture.nativeElement as HTMLElement, seen, changed, settle };
}

const byTestId = (el: HTMLElement, id: string): HTMLElement | null =>
  el.querySelector(`[data-testid="${id}"]`);
const buttonByText = (el: HTMLElement, text: string): HTMLButtonElement | undefined =>
  [...el.querySelectorAll<HTMLButtonElement>('button')].find((b) => b.textContent!.trim() === text);
const toasts = (): Array<{ type: string; message: string }> =>
  TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
const stubFetch = (status: number, body: unknown): ReturnType<typeof vi.fn> => {
  const fn = vi.fn(async () => ({ ok: status < 300, status, json: async () => body }));
  vi.stubGlobal('fetch', fn);
  return fn;
};
const choose = async (h: Harness, file: File): Promise<void> => {
  const input = byTestId(h.el, 'wardrobe-item-image-upload-input') as HTMLInputElement;
  Object.defineProperty(input, 'files', { value: [file], configurable: true });
  input.dispatchEvent(new Event('change'));
  await h.settle();
};

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('WardrobeItemImageSection (v4 wardrobe-item-images-ui.test.tsx)', () => {
  it('is present but inert in create mode', async () => {
    const h = await mount({ item: null, container: null });
    const section = byTestId(h.el, 'wardrobe-item-image-section')!;
    expect(section.getAttribute('aria-label')).toBe('Picture');
    expect(section.querySelector('span.qt-label')!.textContent!.trim()).toBe('Picture');
    expect(byTestId(h.el, 'wardrobe-item-image-inert')!.textContent!.trim()).toBe(
      'Save the item first; then it may sit for its portrait.',
    );
    expect(h.el.querySelector('button')).toBeNull();
    expect(h.seen).toEqual([]);
  });

  it('shows the current picture, its history, and a catalogue caption for a shared item', async () => {
    const h = await mount({
      item: makeItem({ characterId: null, imageFileId: 'file-2' }),
      container: { scope: 'general', id: null },
    });
    expect(byTestId(h.el, 'wardrobe-item-image-current')!.getAttribute('src')).toBe(
      '/api/v1/files/file-2',
    );
    expect(byTestId(h.el, 'wardrobe-item-image-caption')!.textContent!.trim()).toBe(
      'Drawn by flux-pro · catalogue shot',
    );
    expect(h.el.querySelectorAll('[data-testid="wardrobe-item-image-history-entry"]')).toHaveLength(2);
    expect([...h.el.querySelectorAll('button')].filter((b) => b.textContent!.trim() === 'Make current')).toHaveLength(1);
    expect(h.seen.filter((r) => r['type'] === 'wardrobeItemImagesList')).toEqual([
      { type: 'wardrobeItemImagesList', scope: 'general', itemId: 'item-1' },
    ]);
  });
});

describe('WardrobeItemImageSection — v4 WardrobeItemImageSection.tsx strings and flows', () => {
  it('an empty frame says "No picture yet" and offers Generate + Upload; no profiles disables Generate', async () => {
    const h = await mount({
      item: makeItem(),
      container: { scope: 'character', id: 'char-1' },
      images: { current: null, images: [] },
    });
    expect(h.el.textContent).toContain('No picture yet');
    const gen = buttonByText(h.el, 'Generate')!;
    expect(gen.disabled).toBe(true);
    expect(gen.getAttribute('title')).toBe('No image profiles are configured');
    const chevron = h.el.querySelector<HTMLButtonElement>('button[aria-label="Choose an image profile"]')!;
    expect(chevron.getAttribute('title')).toBe('Choose another image profile, just this once');
    const upload = buttonByText(h.el, 'Upload')!;
    expect(upload.getAttribute('title')).toBe(
      'Hang a picture of your own (JPEG, PNG, WebP or GIF; 10 MB at most)',
    );
    const input = byTestId(h.el, 'wardrobe-item-image-upload-input') as HTMLInputElement;
    expect(input.getAttribute('aria-label')).toBe('Upload a picture');
    expect(input.getAttribute('accept')).toBe('image/jpeg,image/png,image/webp,image/gif');
    expect(h.el.textContent).not.toContain('History');
  });

  it('captions a hand-hung picture in a character vault without "catalogue shot"', async () => {
    const h = await mount({
      item: makeItem(),
      container: { scope: 'character', id: 'char-1' },
      images: { current: 'file-1', images: IMAGES_BODY.images },
    });
    expect(byTestId(h.el, 'wardrobe-item-image-caption')!.textContent!.trim()).toBe('Hung by hand');
    expect(h.seen.filter((r) => r['type'] === 'wardrobeItemImagesList')).toEqual([
      { type: 'wardrobeItemImagesList', scope: 'character', containerId: 'char-1', itemId: 'item-1' },
    ]);
  });

  it('the client refuses a wrong type or an over-weight file without a request', async () => {
    const fetchFn = stubFetch(201, {});
    const h = await mount({ item: makeItem(), container: { scope: 'character', id: 'char-1' } });
    await choose(h, new File(['x'], 'a.bmp', { type: 'image/bmp' }));
    const big = new File(['x'], 'b.png', { type: 'image/png' });
    Object.defineProperty(big, 'size', { value: 10 * 1024 * 1024 + 1 });
    await choose(h, big);
    expect(fetchFn).not.toHaveBeenCalled();
    expect(toasts()).toEqual([
      { type: 'error', message: 'Only JPEG, PNG, WebP or GIF pictures may be hung' },
      { type: 'error', message: 'That picture weighs more than 10 MB; the easel will not bear it' },
    ]);
  });

  it('an upload hangs the picture, refetches the history and tells the editor', async () => {
    const fetchFn = stubFetch(201, { current: 'file-3', image: {} });
    const h = await mount({ item: makeItem(), container: { scope: 'character', id: 'char-1' } });
    await choose(h, new File(['x'], 'p.png', { type: 'image/png' }));
    expect(fetchFn.mock.calls[0][0]).toBe(
      '/api/v1/wardrobe/item-1/images?scope=character&id=char-1&action=upload',
    );
    expect(toasts()).toContainEqual({ type: 'success', message: 'Picture hung' });
    expect(h.seen.filter((r) => r['type'] === 'wardrobeItemImagesList')).toHaveLength(2);
    expect(h.changed).toHaveLength(1);
  });

  it('Generate draws with the designated profile and captions who drew it', async () => {
    const fetchFn = stubFetch(201, {
      image: {},
      current: 'file-2',
      prompt: 'p',
      subject: 'worn',
      profile: { id: 'ip-1', name: 'House Artist' },
      rerouted: true,
      trail: null,
    });
    const h = await mount({
      item: makeItem(),
      container: { scope: 'character', id: 'char-1' },
      profiles: [profile({})],
    });
    const gen = buttonByText(h.el, 'Generate')!;
    expect(gen.getAttribute('title')).toBe('Paint it with the designated wardrobe profile');
    gen.click();
    await h.settle();
    expect((fetchFn.mock.calls[0][1] as RequestInit).body).toBe('{}');
    expect(toasts()).toContainEqual({
      type: 'success',
      message: 'The portrait is hung — drawn at the uncensored desk',
    });
    expect(byTestId(h.el, 'wardrobe-item-image-caption')!.textContent!.trim()).toBe(
      'Drawn by House Artist · rerouted to the uncensored desk',
    );
    expect(h.changed).toHaveLength(1);
  });

  it('a 422 refusal names the artist, opens the picker, and toasts nothing', async () => {
    stubFetch(422, {
      error: 'The image provider declined to draw this garment',
      details: { trail: [{ profileName: 'Back Room', detail: 'content policy' }], refused: true },
    });
    const h = await mount({
      item: makeItem(),
      container: { scope: 'character', id: 'char-1' },
      profiles: [profile({})],
    });
    buttonByText(h.el, 'Generate')!.click();
    await h.settle();
    expect(h.el.querySelector('[role="alert"]')!.textContent!.trim()).toBe(
      'Back Room declined to paint it (content policy). Try another profile.',
    );
    expect(h.el.textContent).toContain('Image model');
    expect(toasts()).toEqual([]);
  });

  it('a non-refusal failure toasts the route’s sentence', async () => {
    stubFetch(502, { error: 'Image generation failed: timeout', details: { trail: null, refused: false } });
    const h = await mount({
      item: makeItem(),
      container: { scope: 'character', id: 'char-1' },
      profiles: [profile({})],
    });
    buttonByText(h.el, 'Generate')!.click();
    await h.settle();
    expect(toasts()).toEqual([{ type: 'error', message: 'Image generation failed: timeout' }]);
  });

  it('the picker labels each profile v4’s way, preselects the designated one, and generates with it once', async () => {
    const fetchFn = stubFetch(201, {
      image: {},
      current: 'file-2',
      prompt: 'p',
      subject: 'catalogue',
      profile: { id: 'ip-2', name: 'Back Room' },
      rerouted: false,
      trail: null,
    });
    const h = await mount({
      item: makeItem(),
      container: { scope: 'group', id: 'g1' },
      profiles: [
        profile({ id: 'ip-1', isDefault: true }),
        profile({ id: 'ip-2', name: 'Back Room', provider: 'GROK', modelName: 'grok-image', isDangerousCompatible: true }),
      ],
      designated: 'ip-2',
    });
    h.el.querySelector<HTMLButtonElement>('button[aria-label="Choose an image profile"]')!.click();
    await h.settle();
    const select = h.el.querySelector<HTMLSelectElement>('select')!;
    expect([...select.options].map((o) => o.textContent!.trim())).toEqual([
      'House Artist (default) — OPENAI/gpt-image-1',
      'Back Room — GROK/grok-image · uncensored · designated',
    ]);
    expect(select.value).toBe('ip-2');
    expect(buttonByText(h.el, 'Cancel')).toBeDefined();
    buttonByText(h.el, 'Generate with this')!.click();
    await h.settle();
    expect(fetchFn.mock.calls[0][0]).toBe('/api/v1/wardrobe/item-1/images?scope=group&id=g1&action=generate');
    expect((fetchFn.mock.calls[0][1] as RequestInit).body).toBe('{"imageProfileId":"ip-2"}');
    expect(toasts()).toContainEqual({ type: 'success', message: 'The portrait is hung' });
    expect(byTestId(h.el, 'wardrobe-item-image-caption')!.textContent!.trim()).toBe(
      'Drawn by Back Room · catalogue shot',
    );
  });

  it('Make current and Delete ride the verbs; Delete asks first', async () => {
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValueOnce(false).mockReturnValueOnce(true);
    const h = await mount({ item: makeItem(), container: { scope: 'character', id: 'char-1' } });
    const entries = [...h.el.querySelectorAll<HTMLElement>('[data-testid="wardrobe-item-image-history-entry"]')];
    expect(entries[0].className).toContain('qt-border-primary');
    expect(entries[0].querySelector('img')!.getAttribute('alt')).toBe('Current picture');
    expect(entries[0].querySelector('img')!.getAttribute('title')).toBe('a beaded flapper dress');
    expect(entries[1].querySelector('img')!.getAttribute('alt')).toBe('Earlier picture');
    expect(entries[1].querySelector('img')!.getAttribute('src')).toBe('/api/v1/files/file-1?action=thumbnail');

    buttonByText(entries[1], 'Make current')!.click();
    await h.settle();
    expect(h.seen).toContainEqual({
      type: 'wardrobeItemImageSetCurrent',
      scope: 'character',
      containerId: 'char-1',
      itemId: 'item-1',
      fileId: 'file-1',
    });
    expect(h.changed).toHaveLength(1);

    const del = (): HTMLButtonElement =>
      [...h.el.querySelectorAll<HTMLButtonElement>('button[title="Delete this picture"]')][0];
    del().click();
    await h.settle();
    expect(confirmSpy).toHaveBeenCalledWith('Take this picture down for good? It cannot be rehung.');
    expect(h.seen.some((r) => r['type'] === 'wardrobeItemImageDelete')).toBe(false);
    del().click();
    await h.settle();
    expect(h.seen.some((r) => r['type'] === 'wardrobeItemImageDelete')).toBe(true);
    expect(toasts()).toContainEqual({ type: 'success', message: 'Picture taken down' });
  });
});
