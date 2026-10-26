import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import type { ResolvedInventoryItem } from '../core/core-contract';
import { KIT, LAMP, RING, SWORD } from './__fixtures__/inventory-ui.fixture';
import { InventoryGlyph, InventoryPicture, inventorySvgDataUri } from './inventory-glyph';

/** v4 `components/inventory/inventory-glyph.tsx` (`322a5488e`) + `lib/inventory/svg.ts:226-228`. */

@Component({
  imports: [InventoryGlyph, InventoryPicture],
  template: `
    <div class="glyph"><qt-inventory-glyph [item]="item()" [size]="size()" /></div>
    <div class="picture"><qt-inventory-picture [item]="item()" [size]="pictureSize()" /></div>
  `,
})
class Host {
  readonly item = signal<ResolvedInventoryItem>(SWORD);
  readonly size = signal<'sm' | 'md'>('md');
  readonly pictureSize = signal(48);
}

function render(item: ResolvedInventoryItem) {
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  fixture.componentInstance.item.set(item);
  fixture.detectChanges();
  const el = fixture.nativeElement as HTMLElement;
  return { fixture, el, glyph: () => el.querySelector('.glyph')!, picture: () => el.querySelector('.picture')! };
}

describe('inventorySvgDataUri (R-15, spec-pinned)', () => {
  it('URI-encodes the SVG into a utf8 data URI', () => {
    expect(inventorySvgDataUri('<svg viewBox="0 0 1 1"><rect/></svg>')).toBe(
      'data:image/svg+xml;utf8,%3Csvg%20viewBox%3D%220%200%201%201%22%3E%3Crect%2F%3E%3C%2Fsvg%3E',
    );
  });
});

describe('InventoryGlyph', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('an SVG draws through an <img> data URI, never the picture', () => {
    const { glyph } = render(SWORD);
    const img = glyph().querySelector('[data-testid="inventory-glyph-svg"]') as HTMLImageElement;
    expect(img.getAttribute('src')).toBe(inventorySvgDataUri(SWORD.svg!));
    expect(img.getAttribute('class')).toBe('w-8 h-8 text-lg flex-shrink-0 object-contain');
    expect(img.getAttribute('aria-hidden')).toBe('true');
    expect(glyph().querySelector('[data-testid="inventory-picture"]')).toBeNull();
  });

  it('an emoji, titled with the label; small size', () => {
    const { fixture, glyph } = render(RING);
    fixture.componentInstance.size.set('sm');
    fixture.detectChanges();
    const span = glyph().querySelector('[data-testid="inventory-glyph-emoji"]')!;
    expect(span.textContent).toBe('💍');
    expect(span.getAttribute('title')).toBe('signet ring');
    expect(span.getAttribute('class')).toBe('w-5 h-5 text-sm flex-shrink-0 inline-flex items-center justify-center');
  });

  it('the default glyph is a tag, or layers for a kit', () => {
    expect(render(LAMP).glyph().querySelector('[data-testid="inventory-glyph-default"] qt-icon span')!.getAttribute('data-icon')).toBe('tag');
    TestBed.resetTestingModule();
    expect(render(KIT).glyph().querySelector('[data-testid="inventory-glyph-default"] qt-icon span')!.getAttribute('data-icon')).toBe('layers');
  });
});

describe('InventoryPicture', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('shows the thumbnail named "Picture of <label>" at the size given', () => {
    const { fixture, picture } = render(SWORD);
    fixture.componentInstance.pictureSize.set(64);
    fixture.detectChanges();
    const img = picture().querySelector('[data-testid="inventory-picture"]') as HTMLImageElement;
    expect(img.getAttribute('src')).toBe('/api/v1/files/f0000000-0000-4000-8000-000000000001?action=thumbnail');
    expect(img.alt).toBe('Picture of Excalibur');
    expect(img.style.width).toBe('64px');
  });

  it('falls back to the glyph in a 48px muted box without a picture', () => {
    const { picture } = render(RING);
    expect(picture().querySelector('[data-testid="inventory-picture"]')).toBeNull();
    const box = picture().querySelector('span') as HTMLElement;
    expect(box.style.width).toBe('48px');
    expect(box.querySelector('[data-testid="inventory-glyph-emoji"]')).not.toBeNull();
  });
});
