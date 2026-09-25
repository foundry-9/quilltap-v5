import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { QuickHideService } from '../../quick-hide/quick-hide.service';
import { HIDE_SALON_IMAGES_KEY } from '../../quick-hide/quick-hide.storage';
import { Avatar } from '../../ui/avatar';
import { MessageContent } from '../message-content';
import { renderMarkdownToHtml } from '../render/markdown-renderer';
import { clearRenderCache, renderMarkdownCached } from '../render/render-cache';
import { HiddenImageTile } from './hidden-image-tile';
import { hiddenInlineImageHtml, hideInlineImages } from './hidden-inline-image';
import { IMAGES_HIDDEN, provideImagesHidden } from './images-hidden';

/**
 * Quick-hide "Salon Images" (v4 `e3937d7aa`) — the Salon-scoped token and the
 * two stand-ins. v4's own `ImagesHiddenProvider consumers` vectors
 * (`__tests__/unit/components/quick-hide/salon-images.test.tsx`) are
 * transcribed for `Avatar`; v4's three `LazyMessageContent` vectors have no v5
 * subject (v5 dropped the `renderedHtml` fast path — every message takes the
 * full render), so the inline-image behaviour is pinned at the renderer and the
 * render cache instead.
 */

function parse(html: string): HTMLElement {
  const host = document.createElement('div');
  host.innerHTML = html;
  return host;
}

// ---------------------------------------------------------------------------
// The token
// ---------------------------------------------------------------------------

describe('IMAGES_HIDDEN — the Salon-scoped switch (v4 ImagesHiddenContext)', () => {
  afterEach(() => {
    TestBed.resetTestingModule();
    vi.unstubAllGlobals();
  });

  it('defaults to false at the root (v4 `createContext<boolean>(false)`)', () => {
    TestBed.configureTestingModule({});
    expect(TestBed.inject(IMAGES_HIDDEN)()).toBe(false);
  });

  it("the Salon's provider follows the quick-hide service's global flag", async () => {
    const store = new Map<string, string>([[HIDE_SALON_IMAGES_KEY, 'true']]);
    vi.stubGlobal('localStorage', {
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => void store.set(k, v),
      removeItem: (k: string) => void store.delete(k),
    });
    TestBed.configureTestingModule({
      providers: [
        { provide: CoreClient, useValue: { dispatchData: async () => ({ tags: [] }) } },
        provideImagesHidden(),
      ],
    });
    const hidden = TestBed.inject(IMAGES_HIDDEN);
    expect(hidden()).toBe(true);
    TestBed.inject(QuickHideService).toggleHideSalonImages();
    expect(hidden()).toBe(false);
  });

  it('an element-level provider scopes it to the subtree — a sibling outside keeps its images', () => {
    @Component({
      selector: 'qt-veiled',
      imports: [Avatar],
      providers: [{ provide: IMAGES_HIDDEN, useValue: signal(true) }],
      template: `<qt-avatar name="Vera" src="/img/vera.webp" />`,
    })
    class Veiled {}

    @Component({
      imports: [Avatar, Veiled],
      template: `<div class="outside"><qt-avatar name="Otto" src="/img/otto.webp" /></div>
        <div class="inside"><qt-veiled /></div>`,
    })
    class Page {}

    TestBed.configureTestingModule({ imports: [Page] });
    const fixture = TestBed.createComponent(Page);
    fixture.detectChanges();
    const root = fixture.nativeElement as HTMLElement;
    expect(root.querySelector('.outside img')?.getAttribute('src')).toBe('/img/otto.webp');
    expect(root.querySelector('.inside img')).toBeNull();
    expect(root.querySelector('.inside')?.textContent).toContain('V');
  });
});

// ---------------------------------------------------------------------------
// v4's two Avatar vectors, verbatim
// ---------------------------------------------------------------------------

describe('ImagesHiddenProvider consumers — Avatar (v4 salon-images.test.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  function renderAvatar(hidden: boolean | null) {
    TestBed.configureTestingModule({
      imports: [Avatar],
      providers: hidden === null ? [] : [{ provide: IMAGES_HIDDEN, useValue: signal(hidden) }],
    });
    const fixture = TestBed.createComponent(Avatar);
    fixture.componentRef.setInput('name', 'Vera');
    fixture.componentRef.setInput('src', '/img/vera.webp');
    fixture.detectChanges();
    return fixture.nativeElement as HTMLElement;
  }

  it('Avatar paints the image outside the provider', () => {
    const img = renderAvatar(null).querySelector('img[alt="Vera"]');
    expect(img?.getAttribute('src')).toBe('/img/vera.webp');
  });

  it('Avatar falls back to the initial when images are hidden', () => {
    const root = renderAvatar(true);
    expect(root.querySelector('img')).toBeNull();
    expect(root.textContent?.trim()).toBe('V');
  });
});

// ---------------------------------------------------------------------------
// The two stand-ins
// ---------------------------------------------------------------------------

describe('HiddenImageTile (v4 images-hidden-context.tsx:33-46)', () => {
  afterEach(() => TestBed.resetTestingModule());

  function renderTile(label?: string): HTMLElement {
    TestBed.configureTestingModule({ imports: [HiddenImageTile] });
    const fixture = TestBed.createComponent(HiddenImageTile);
    if (label !== undefined) fixture.componentRef.setInput('label', label);
    fixture.detectChanges();
    return (fixture.nativeElement as HTMLElement).querySelector('span[role="img"]') as HTMLElement;
  }

  it("carries v4's classes, role, title and aria-label for a labelled image", () => {
    const tile = renderTile('sketch.png');
    expect(tile.className.split(/\s+/).filter(Boolean)).toEqual([
      'w-full',
      'h-full',
      'flex',
      'items-center',
      'justify-center',
      'qt-bg-muted',
      'qt-text-secondary',
    ]);
    expect(tile.getAttribute('title')).toBe('Image hidden: sketch.png');
    expect(tile.getAttribute('aria-label')).toBe('Image hidden: sketch.png');
    const icon = tile.querySelector('[data-icon]');
    expect(icon?.getAttribute('data-icon')).toBe('eye-off');
    expect(icon?.classList.contains('w-6')).toBe(true);
    expect(icon?.classList.contains('h-6')).toBe(true);
  });

  it("says plain 'Image hidden' with no label (and for an empty one)", () => {
    expect(renderTile().getAttribute('aria-label')).toBe('Image hidden');
    TestBed.resetTestingModule();
    const empty = renderTile('');
    expect(empty.getAttribute('title')).toBe('Image hidden');
    expect(empty.getAttribute('aria-label')).toBe('Image hidden');
  });
});

describe('hiddenInlineImageHtml (v4 HiddenInlineImage, images-hidden-context.tsx:48-58)', () => {
  it("renders v4's inline span: classes, role, aria-label, the eye-off icon and the VISIBLE text", () => {
    const span = parse(hiddenInlineImageHtml('a map')).firstElementChild as HTMLElement;
    expect(span.tagName).toBe('SPAN');
    expect(span.className).toBe(
      'inline-flex items-center gap-1 px-2 py-0.5 rounded qt-bg-muted qt-text-secondary text-xs align-middle',
    );
    expect(span.getAttribute('role')).toBe('img');
    expect(span.getAttribute('aria-label')).toBe('Image hidden: a map');
    expect(span.textContent).toBe('Image hidden: a map');
    const icon = span.querySelector('.qt-icon') as HTMLElement;
    expect(icon.getAttribute('data-icon')).toBe('eye-off');
    expect(icon.className).toBe('qt-icon w-3 h-3');
    expect(icon.getAttribute('aria-hidden')).toBe('true');
  });

  it("says plain 'Image hidden' without an alt", () => {
    const span = parse(hiddenInlineImageHtml()).firstElementChild as HTMLElement;
    expect(span.getAttribute('aria-label')).toBe('Image hidden');
    expect(span.textContent).toBe('Image hidden');
  });

  it('escapes the alt in both the attribute and the text', () => {
    const span = parse(hiddenInlineImageHtml('<b>"x" & y</b>')).firstElementChild as HTMLElement;
    expect(span.getAttribute('aria-label')).toBe('Image hidden: <b>"x" & y</b>');
    expect(span.textContent).toBe('Image hidden: <b>"x" & y</b>');
    expect(span.querySelector('b')).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// The renderer + the render cache
// ---------------------------------------------------------------------------

describe('inline markdown images under the switch (v4 MessageContent.tsx:520-521)', () => {
  beforeEach(() => clearRenderCache());

  it('keeps the <img> when images are shown', () => {
    const root = parse(renderMarkdownToHtml('![a map](/img/a.webp)'));
    expect(root.querySelector('img')?.getAttribute('src')).toBe('/img/a.webp');
  });

  it('swaps every <img> for the inline stand-in, speaking the alt', () => {
    const root = parse(
      renderMarkdownToHtml('Look: ![a map](/img/a.webp) and ![](/img/b.webp)', {
        imagesHidden: true,
      }),
    );
    expect(root.querySelector('img')).toBeNull();
    const stands = Array.from(root.querySelectorAll('span[role="img"]'));
    // v4 `alt || undefined`: an empty alt says plain "Image hidden".
    expect(stands.map((s) => s.getAttribute('aria-label'))).toEqual([
      'Image hidden: a map',
      'Image hidden',
    ]);
    expect(root.textContent).toContain('Look:');
  });

  // Fixed at the b0b6656b5 unification (§3 review): hast escapes only `"`
  // and `&` inside a double-quoted attribute, so a raw `>` in the alt
  // survives into `alt="…"`; a matcher that stopped at the first `>` cut the
  // tag short and its CLOSED replacement span ended the attribute context,
  // turning the rest of the alt into live markup under `[innerHTML]`. v4 never
  // string-splices (its `img` renderer returns an element), so this is v5's
  // own hazard — pinned here through the REAL renderer, not the helper.
  it('never lets a `>` inside the alt escape the tag (a closed stand-in must not end the attribute)', () => {
    const root = parse(
      renderMarkdownToHtml('![<b>x <svg onload=alert(1)>](/img/y.webp)', { imagesHidden: true }),
    );
    expect(root.querySelector('img')).toBeNull();
    expect(root.querySelector('svg, b')).toBeNull();
    const stand = root.querySelector('span[role="img"]');
    expect(stand?.getAttribute('aria-label')).toBe('Image hidden: <b>x <svg onload=alert(1)>');
    expect(root.textContent).toBe('Image hidden: <b>x <svg onload=alert(1)>');
  });

  it('speaks an alt that contains `>` whole, with no stray text after the stand-in', () => {
    const root = parse(renderMarkdownToHtml('![a > b](/img/y.webp) tail', { imagesHidden: true }));
    const stand = root.querySelector('span[role="img"]');
    expect(stand?.getAttribute('aria-label')).toBe('Image hidden: a > b');
    expect(root.textContent).toBe('Image hidden: a > b tail');
  });

  it('runs after the blob rewrite, and decodes the emitted alt back to the author’s words', () => {
    const html = renderMarkdownToHtml('![Tom & "Jerry"](images/x.webp)', {
      blobMountPointId: 'mp-1',
      imagesHidden: true,
    });
    const root = parse(html);
    expect(root.querySelector('img')).toBeNull();
    expect(html).not.toContain('/api/v1/mount-points/');
    expect(root.querySelector('span[role="img"]')?.getAttribute('aria-label')).toBe(
      'Image hidden: Tom & "Jerry"',
    );
  });

  it('leaves image-free HTML byte-identical', () => {
    const content = 'Plain *words* and a `code` span.';
    expect(renderMarkdownToHtml(content, { imagesHidden: true })).toBe(
      renderMarkdownToHtml(content),
    );
  });

  it('hideInlineImages treats a missing alt attribute as no alt', () => {
    const root = parse(hideInlineImages('<p><img src="/x.webp"></p>'));
    expect(root.querySelector('span[role="img"]')?.getAttribute('aria-label')).toBe('Image hidden');
  });

  it('the render cache keys the switch: a warm entry never leaks across a flip', () => {
    const content = '![a map](/img/a.webp)';
    const shown = parse(renderMarkdownCached(content));
    expect(shown.querySelector('img')).not.toBeNull();

    // Same content, warm cache — the flag alone must change the answer.
    const hidden = parse(renderMarkdownCached(content, { imagesHidden: true }));
    expect(hidden.querySelector('img')).toBeNull();
    expect(hidden.querySelector('span[role="img"]')).not.toBeNull();

    // And back again, from the other warm entry.
    expect(parse(renderMarkdownCached(content)).querySelector('img')).not.toBeNull();
  });
});

describe('MessageContent reads the switch (v4 MessageContent.tsx:344-346)', () => {
  beforeEach(() => clearRenderCache());
  afterEach(() => TestBed.resetTestingModule());

  function renderContent(hidden: boolean | null): HTMLElement {
    TestBed.configureTestingModule({
      imports: [MessageContent],
      providers: hidden === null ? [] : [{ provide: IMAGES_HIDDEN, useValue: signal(hidden) }],
    });
    const fixture = TestBed.createComponent(MessageContent);
    fixture.componentRef.setInput('content', '![a map](/img/a.webp)');
    fixture.detectChanges();
    return fixture.nativeElement as HTMLElement;
  }

  it('paints embedded images on every surface that does not provide the switch', () => {
    expect(renderContent(null).querySelector('img')).not.toBeNull();
  });

  it('swaps them for the stand-in inside the Salon', () => {
    const root = renderContent(true);
    expect(root.querySelector('img')).toBeNull();
    expect(root.querySelector('span[role="img"]')?.getAttribute('aria-label')).toBe(
      'Image hidden: a map',
    );
  });

  it('repaints live when the switch flips', () => {
    const flag = signal(false);
    TestBed.configureTestingModule({
      imports: [MessageContent],
      providers: [{ provide: IMAGES_HIDDEN, useValue: flag }],
    });
    const fixture = TestBed.createComponent(MessageContent);
    fixture.componentRef.setInput('content', '![a map](/img/a.webp)');
    fixture.detectChanges();
    const root = fixture.nativeElement as HTMLElement;
    expect(root.querySelector('img')).not.toBeNull();
    flag.set(true);
    fixture.detectChanges();
    expect(root.querySelector('img')).toBeNull();
  });
});
