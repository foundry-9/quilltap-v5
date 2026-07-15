import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { TokenBadge } from './token-badge';

function render(inputs: Record<string, unknown>): ComponentFixture<TokenBadge> {
  TestBed.configureTestingModule({ imports: [TokenBadge] });
  const fixture = TestBed.createComponent(TokenBadge);
  for (const [key, value] of Object.entries(inputs)) {
    fixture.componentRef.setInput(key, value);
  }
  fixture.detectChanges();
  return fixture;
}

/**
 * The badge's rendered text, whitespace-collapsed.
 *
 * Expect NO spaces between the parts: both Angular and JSX strip the whitespace
 * between sibling elements, so v4's DOM text is likewise "1.5K/320tokens". The
 * visual spacing is the flex `gap-1`/`gap-2`, not text. Asserting the pretty
 * string here would be asserting something neither framework produces.
 */
function text(fixture: ComponentFixture<TokenBadge>): string {
  return (fixture.nativeElement.textContent ?? '').replace(/\s+/g, ' ').trim();
}

describe('TokenBadge (v4 components/chat/TokenBadge.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('renders "<prompt> / <completion> tokens" with the two title spans', () => {
    const fixture = render({ promptTokens: 1500, completionTokens: 320 });
    expect(text(fixture)).toBe('1.5K/320tokens');

    const el = fixture.nativeElement as HTMLElement;
    expect(el.querySelector('[title="Prompt tokens"]')!.textContent).toBe('1.5K');
    expect(el.querySelector('[title="Completion tokens"]')!.textContent).toBe('320');
  });

  it('treats null counts as 0 (v4 `promptTokens ?? 0`)', () => {
    const fixture = render({ promptTokens: null, completionTokens: 42 });
    expect(text(fixture)).toBe('0/42tokens');
  });

  it('renders nothing when the total is 0 (v4 `:32-35` returns null)', () => {
    const fixture = render({ promptTokens: 0, completionTokens: 0 });
    expect(fixture.nativeElement.querySelector('div')).toBeNull();
    expect(text(fixture)).toBe('');
  });

  it('renders nothing when both display flags are off (v4 `:32-35`)', () => {
    const fixture = render({
      promptTokens: 100,
      completionTokens: 50,
      showTokens: false,
      showCost: false,
    });
    expect(fixture.nativeElement.querySelector('div')).toBeNull();
  });

  it('uses totalTokens for the zero-check, falling back to the sum only when null', () => {
    // An explicit total of 0 suppresses the badge even though the parts are non-zero:
    // v4 is `totalTokens ?? (prompt + completion)` — the sum is the FALLBACK, not the value.
    const suppressed = render({ promptTokens: 10, completionTokens: 5, totalTokens: 0 });
    expect(suppressed.nativeElement.querySelector('div')).toBeNull();

    TestBed.resetTestingModule();

    // With no total supplied, the sum decides — and the parts still render as themselves.
    const summed = render({ promptTokens: 10, completionTokens: 5, totalTokens: null });
    expect(text(summed)).toBe('10/5tokens');
  });

  describe('the cost arm — DEAD in v4, ported dead', () => {
    it('renders no cost when showCost is on but no estimate is supplied (v4\'s only call site)', () => {
      // v4 `MessageActionBar.tsx:198-204` passes showCost but never passes
      // estimatedCostUSD — there is no cost field on the Message type. The arm's
      // own `!= null` guard then always fails, so per-message cost is unreachable.
      const fixture = render({ promptTokens: 100, completionTokens: 50, showCost: true });
      expect(text(fixture)).toBe('100/50tokens');
      expect(fixture.nativeElement.querySelector('[title="Estimated cost"]')).toBeNull();
    });

    it('renders the cost when an estimate IS supplied (the arm works — nothing feeds it)', () => {
      const fixture = render({
        promptTokens: 100,
        completionTokens: 50,
        showCost: true,
        estimatedCostUSD: 0.0023,
      });
      expect(fixture.nativeElement.querySelector('[title="Estimated cost"]')!.textContent).toBe(
        '$0.0023',
      );
    });
  });
});
