import { TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { CopyChatIdButton } from './copy-chat-id-button';

/**
 * P4.D358 — WCAG 2.5.8 target size: the icon-only `inline` livery is at least
 * 24 × 24 CSS px (`min-w-6 min-h-6`); the labelled `palette` livery is sized by
 * its own label and carries neither.
 */
describe('CopyChatIdButton — target size', () => {
  afterEach(() => TestBed.resetTestingModule());

  function button(variant?: 'inline' | 'palette'): HTMLButtonElement {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({ imports: [CopyChatIdButton] });
    const fixture = TestBed.createComponent(CopyChatIdButton);
    fixture.componentRef.setInput('chatId', 'chat-1');
    if (variant) fixture.componentRef.setInput('variant', variant);
    fixture.detectChanges();
    return fixture.nativeElement.querySelector('button') as HTMLButtonElement;
  }

  it('the inline (icon-only) button carries min-w-6 and min-h-6; the labelled palette one does not', () => {
    const inline = button();
    expect(inline.getAttribute('aria-label')).toBe('Copy conversation ID');
    expect(inline.classList.contains('min-w-6')).toBe(true);
    expect(inline.classList.contains('min-h-6')).toBe(true);

    const palette = button('palette');
    expect(palette.textContent).toContain('Copy ID');
    expect(palette.classList.contains('min-w-6')).toBe(false);
    expect(palette.classList.contains('min-h-6')).toBe(false);
  });
});
