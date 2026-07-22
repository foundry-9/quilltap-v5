import { TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { QuillAnimation } from './quill-animation';

/**
 * v4 shipped no unit tests with `deab0e5d`, so these are the coverage of record
 * for the documented semantics of `QuillAnimation`: the size classes ride BOTH
 * the wrapper and the glyph, the glyph always carries the motion class, and the
 * label defaults to "Writing…" but is suppressed entirely when `null` is passed
 * (the indicator then sits silent inside an already-labelled live region).
 */
describe('QuillAnimation', () => {
  function render(inputs: Record<string, unknown> = {}) {
    TestBed.configureTestingModule({ imports: [QuillAnimation] });
    const fixture = TestBed.createComponent(QuillAnimation);
    for (const [key, value] of Object.entries(inputs)) fixture.componentRef.setInput(key, value);
    fixture.detectChanges();
    const host = fixture.nativeElement as HTMLElement;
    return {
      wrapper: host.querySelector('span') as HTMLElement,
      glyph: host.querySelector('[data-icon]') as HTMLElement,
    };
  }

  /** Angular re-orders the tokens of a bound `class`, so assert per token. */
  function hasClasses(el: HTMLElement, ...names: string[]): boolean {
    return names.every((n) => el.classList.contains(n));
  }

  it('defaults to the large size and the "Writing…" label', () => {
    const { wrapper, glyph } = render();
    expect(hasClasses(wrapper, 'inline-flex', 'items-center', 'justify-center')).toBe(true);
    expect(hasClasses(wrapper, 'w-12', 'h-12')).toBe(true);
    expect(glyph.getAttribute('data-icon')).toBe('thinking');
    expect(hasClasses(glyph, 'qt-thinking-indicator', 'w-12', 'h-12')).toBe(true);
    expect(glyph.getAttribute('aria-label')).toBe('Writing…');
    expect(glyph.getAttribute('role')).toBe('img');
  });

  it('renders the small variant at w-4 h-4 on both the wrapper and the glyph', () => {
    const { wrapper, glyph } = render({ size: 'sm' });
    expect(hasClasses(wrapper, 'w-4', 'h-4')).toBe(true);
    expect(hasClasses(glyph, 'w-4', 'h-4')).toBe(true);
    expect(glyph.classList.contains('w-12')).toBe(false);
  });

  it('announces nothing when the label is null', () => {
    const { glyph } = render({ size: 'sm', label: null });
    expect(glyph.getAttribute('aria-label')).toBeNull();
    expect(glyph.getAttribute('role')).toBeNull();
    expect(glyph.getAttribute('aria-hidden')).toBe('true');
    // The motion still runs — only the announcement is suppressed.
    expect(glyph.classList.contains('qt-thinking-indicator')).toBe(true);
  });

  it('appends call-site classes to the wrapper', () => {
    const { wrapper } = render({ size: 'sm', class: 'ml-auto qt-text-secondary' });
    expect(hasClasses(wrapper, 'ml-auto', 'qt-text-secondary')).toBe(true);
  });
});
