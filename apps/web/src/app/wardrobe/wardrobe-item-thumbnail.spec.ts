import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { WardrobeItemThumbnail } from './wardrobe-item-thumbnail';

/**
 * P4.D277 — v4 `components/wardrobe/wardrobe-item-thumbnail.tsx` at
 * `01a83539d`: with `onOpen` the thumbnail is its own button (`:63-74`);
 * without it, the bare image (`:63`); without a `fileId`, nothing (`:50`).
 * v5's gate is the `openable` input + `open` output (R-E).
 */
@Component({
  imports: [WardrobeItemThumbnail],
  template: `
    <qt-wardrobe-item-thumbnail
      [fileId]="fileId()"
      [size]="40"
      [openable]="openable()"
      [openLabel]="openLabel()"
      (open)="opens.set(opens() + 1)"
    />
  `,
})
class Host {
  readonly fileId = signal<string | null>('file-9');
  readonly openable = signal(false);
  readonly openLabel = signal<string>('View picture full size');
  readonly opens = signal(0);
}

function render(setup: (h: Host) => void = () => {}): ComponentFixture<Host> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  setup(fixture.componentInstance);
  fixture.detectChanges();
  return fixture;
}

const el = (f: ComponentFixture<Host>): HTMLElement => f.nativeElement as HTMLElement;

describe('WardrobeItemThumbnail — open (v4 wardrobe-item-thumbnail.tsx at 01a83539d)', () => {
  it('is a plain image without openable (:63)', () => {
    const f = render();
    expect(el(f).querySelector('[data-testid="wardrobe-item-thumbnail"]')).toBeTruthy();
    expect(el(f).querySelector('[data-testid="wardrobe-item-thumbnail-open"]')).toBeNull();
  });

  it("wraps the image in v4's button when openable, and emits open on click (:64-74)", () => {
    const f = render((h) => {
      h.openable.set(true);
      h.openLabel.set('View the picture of Blue Velvet Gown');
    });
    const button = el(f).querySelector<HTMLButtonElement>('[data-testid="wardrobe-item-thumbnail-open"]')!;
    expect(button.tagName).toBe('BUTTON');
    expect(button.getAttribute('type')).toBe('button');
    expect(button.className).toBe('flex-shrink-0 rounded cursor-zoom-in hover:opacity-80 transition-opacity');
    expect(button.title).toBe('View the picture of Blue Velvet Gown');
    expect(button.getAttribute('aria-label')).toBe('View the picture of Blue Velvet Gown');
    expect(button.querySelector('[data-testid="wardrobe-item-thumbnail"]')).toBeTruthy();
    button.click();
    expect(f.componentInstance.opens()).toBe(1);
  });

  it('defaults the label to "View picture full size" (:48)', () => {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({ imports: [WardrobeItemThumbnail] });
    const fixture = TestBed.createComponent(WardrobeItemThumbnail);
    fixture.componentRef.setInput('fileId', 'file-9');
    fixture.componentRef.setInput('size', 28);
    fixture.componentRef.setInput('openable', true);
    fixture.detectChanges();
    const button = (fixture.nativeElement as HTMLElement).querySelector('button')!;
    expect(button.title).toBe('View picture full size');
    expect(button.getAttribute('aria-label')).toBe('View picture full size');
  });

  it('renders nothing without a fileId, openable or not (:50)', () => {
    const f = render((h) => {
      h.fileId.set(null);
      h.openable.set(true);
    });
    expect(el(f).querySelector('img')).toBeNull();
    expect(el(f).querySelector('button')).toBeNull();
  });
});
