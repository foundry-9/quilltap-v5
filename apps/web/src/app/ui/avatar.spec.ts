import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { IMAGES_HIDDEN } from '../chat/hidden-image/images-hidden';
import { Avatar } from './avatar';

/**
 * v4 `components/ui/Avatar.tsx:59-64, :175-184` (`27fb3ec77`): with
 * `onImageClick` the PICTURE alone becomes a button that opens it full size —
 * "ignored when there is no picture to show". v5 spells v4's "is the callback
 * passed" as the explicit `zoomable` input (ruling PL-6).
 */
@Component({
  imports: [Avatar],
  template: `<qt-avatar name="Abigail" [src]="src()" size="chat" [zoomable]="zoomable()" (imageClick)="clicked.set($event)" />`,
})
class Host {
  readonly src = signal<string | null>('/files/abigail.webp');
  readonly zoomable = signal(true);
  readonly clicked = signal<string | null>(null);
}

function render(over: { src?: string | null; zoomable?: boolean; hidden?: boolean } = {}) {
  TestBed.configureTestingModule({
    imports: [Host],
    providers: over.hidden ? [{ provide: IMAGES_HIDDEN, useValue: signal(true) }] : [],
  });
  const f = TestBed.createComponent(Host);
  if (over.src !== undefined) f.componentInstance.src.set(over.src);
  if (over.zoomable !== undefined) f.componentInstance.zoomable.set(over.zoomable);
  f.detectChanges();
  return { f, el: f.nativeElement as HTMLElement };
}

describe('Avatar — the picture opens full size (v4 27fb3ec77)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('wraps the picture in a named zoom button that emits its src', () => {
    const { f, el } = render();
    const button = el.querySelector<HTMLButtonElement>('button')!;
    expect(button.getAttribute('type')).toBe('button');
    expect(button.getAttribute('aria-label')).toBe("View Abigail's picture full size");
    expect(button.classList.contains('cursor-zoom-in')).toBe(true);
    expect(button.querySelector('img')!.getAttribute('src')).toBe('/files/abigail.webp');
    button.click();
    expect(f.componentInstance.clicked()).toBe('/files/abigail.webp');
  });

  it('draws no button when not zoomable', () => {
    const { el } = render({ zoomable: false });
    expect(el.querySelector('button')).toBeNull();
    expect(el.querySelector('img')).not.toBeNull();
  });

  it('draws no button for an initial (no picture)', () => {
    const { el } = render({ src: null });
    expect(el.querySelector('button')).toBeNull();
    expect(el.textContent!.trim()).toBe('A');
  });

  it('draws no button while the Salon hides its images', () => {
    const { el } = render({ hidden: true });
    expect(el.querySelector('button')).toBeNull();
    expect(el.querySelector('img')).toBeNull();
  });
});
