import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { Modal } from './modal';

/**
 * P4.D277 — v4 `01a83539d` `components/ui/BaseModal.tsx`: the new
 * `overlayClassName` prop (`:31-35`, default `''` `:90`) appended to the
 * overlay's `qt-dialog-overlay` (`:107`). v5's `overlayClass` input; no other
 * `Modal` behaviour changes (R-C).
 */
@Component({
  imports: [Modal],
  template: `<qt-modal title="A dialog" [overlayClass]="overlay()"><p>body</p></qt-modal>`,
})
class Host {
  readonly overlay = signal('');
}

function render(overlay: string): HTMLElement {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  fixture.componentInstance.overlay.set(overlay);
  fixture.detectChanges();
  return fixture.nativeElement as HTMLElement;
}

describe('Modal overlayClass (v4 BaseModal.tsx:31-35, :90, :107 at 01a83539d)', () => {
  it('carries exactly qt-dialog-overlay by default — no stray token', () => {
    const overlay = render('').querySelector('qt-modal > div')!;
    expect(overlay.className).toBe('qt-dialog-overlay');
  });

  it('appends a raised z-index class to the overlay', () => {
    const overlay = render('z-[100]').querySelector('qt-modal > div')!;
    expect(overlay.classList.contains('qt-dialog-overlay')).toBe(true);
    expect(overlay.classList.contains('z-[100]')).toBe(true);
    expect(overlay.classList).toHaveLength(2);
  });
});
