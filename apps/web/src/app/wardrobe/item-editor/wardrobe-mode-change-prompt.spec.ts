import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { WardrobeModeChangePrompt } from './wardrobe-mode-change-prompt';

/**
 * The bundle → single-garment confirmation (v4 `WardrobeModeChangePrompt.tsx`),
 * at its WCAG semantics: v4 `dff76ebef` gave the card `role="alertdialog"` named
 * by its title, `useDialogFocus`, and `useEscapeKey(onCancel, true)` (P4.D358).
 */

function mount(count = 2): ComponentFixture<WardrobeModeChangePrompt> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [WardrobeModeChangePrompt] });
  const fixture = TestBed.createComponent(WardrobeModeChangePrompt);
  fixture.componentRef.setInput('componentCount', count);
  fixture.detectChanges();
  return fixture;
}

describe('WardrobeModeChangePrompt — WCAG semantics (P4.D358)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('announces as an alertdialog named by its title', () => {
    const fixture = mount();
    const host = fixture.nativeElement as HTMLElement;
    const dlg = host.querySelector('[role="alertdialog"]') as HTMLElement;
    expect(dlg.classList.contains('qt-dialog')).toBe(true);
    expect(dlg.getAttribute('aria-modal')).toBe('true');
    expect(dlg.getAttribute('tabindex')).toBe('-1');
    const id = dlg.getAttribute('aria-labelledby')!;
    expect(id).toMatch(/^qt-dialog-\d+$/);
    expect((host.querySelector('#' + id) as HTMLElement).textContent?.trim()).toBe(
      'Switch to a single garment?',
    );
  });

  it('a document Escape cancels exactly once, and neither resets nor keeps', () => {
    const fixture = mount();
    const seen: string[] = [];
    fixture.componentInstance.cancel.subscribe(() => seen.push('cancel'));
    fixture.componentInstance.reset.subscribe(() => seen.push('reset'));
    fixture.componentInstance.keepTypes.subscribe(() => seen.push('keep'));
    document.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }),
    );
    expect(seen).toEqual(['cancel']);
  });
});
