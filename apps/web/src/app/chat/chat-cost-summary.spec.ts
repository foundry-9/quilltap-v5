import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { ChatCostDto } from './chat-cost.api';
import { ChatCostSummary } from './chat-cost-summary';

/**
 * The canned breakdown is the Shared-contract §1 body VERBATIM (v4
 * `getChatCostBreakdown`, `lib/services/cost-estimation.service.ts:139-190`), so
 * that when the unifier drops the mock for lane A's live `chatGetCost` verb the
 * shape needs no rewrite.
 */
function breakdown(over: Partial<ChatCostDto> = {}): ChatCostDto {
  return {
    totalTokens: 15400,
    promptTokens: 12000,
    completionTokens: 3400,
    estimatedCostUSD: 0.0234,
    priceSource: 'openrouter',
    ...over,
  };
}

/** Flush the query microtasks (zoneless: `whenStable` does not cover fetches). */
async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

interface Stub {
  calls: { type: string; chatId?: string }[];
  client: Partial<CoreClient>;
}

function stub(data: ChatCostDto | Error): Stub {
  const calls: { type: string; chatId?: string }[] = [];
  const dispatchData = vi.fn(async (req: { type: string; chatId?: string }) => {
    calls.push(req);
    if (data instanceof Error) throw data;
    return data as unknown as Record<string, unknown>;
  });
  return { calls, client: { dispatchData: dispatchData as unknown as CoreClient['dispatchData'] } };
}

async function mount(
  s: Stub,
  inputs: { show?: boolean; refreshKey?: number } = {},
): Promise<ComponentFixture<ChatCostSummary>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [ChatCostSummary],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: s.client }],
  });
  const fixture = TestBed.createComponent(ChatCostSummary);
  fixture.componentRef.setInput('chatId', 'chat-1');
  fixture.componentRef.setInput('show', inputs.show ?? true);
  fixture.componentRef.setInput('refreshKey', inputs.refreshKey ?? 0);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

/**
 * The summary's rendered text, whitespace-collapsed. Expect no spaces BETWEEN
 * the spans: Angular and JSX both strip inter-element whitespace, so v4's DOM
 * text has the same run-together shape. The gaps are flex `gap-2`, not text.
 */
function text(fixture: ComponentFixture<ChatCostSummary>): string {
  return (fixture.nativeElement.textContent ?? '').replace(/\s+/g, ' ').trim();
}

describe('ChatCostSummary — compact (v4 components/chat/ChatCostSummary.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('dispatches chatGetCost and renders "<total> tokens • <cost>"', async () => {
    const s = stub(breakdown());
    const fixture = await mount(s);

    expect(s.calls).toEqual([{ type: 'chatGetCost', chatId: 'chat-1' }]);
    // The total renders TWICE — the md+ span carries the "tokens" word, the
    // mobile span is the bare number. Both are in the DOM; CSS picks one.
    // $0.023, not $0.0234: 0.0234 is above the $0.01 band edge, so it takes the
    // 3-digit arm of formatCostForDisplay.
    expect(text(fixture)).toBe('15.4K tokens15.4K•$0.023');
  });

  it('never sends `detailed` — the detailed arm has no v4 client (a named deferral)', async () => {
    const s = stub(breakdown());
    await mount(s);
    expect(s.calls[0]).not.toHaveProperty('detailed');
  });

  describe('the suppression rules (v4 `:86-103`)', () => {
    it('renders nothing AND does not fetch when `show` is false', async () => {
      const s = stub(breakdown());
      const fixture = await mount(s, { show: false });
      expect(text(fixture)).toBe('');
      // v4 returns before fetching (`:44-48`) — hidden means no request at all.
      expect(s.calls).toEqual([]);
    });

    it('renders nothing when the chat has zero tokens', async () => {
      const fixture = await mount(stub(breakdown({ totalTokens: 0, estimatedCostUSD: 0 })));
      expect(text(fixture)).toBe('');
    });

    it('renders nothing when the fetch fails (v4 warns and leaves costData null)', async () => {
      // A missing breakdown is a UI degradation, not a user-facing alarm — v4
      // deliberately swallows it, so there is no error branch to assert.
      const fixture = await mount(stub(new Error('cost endpoint returned HTTP 500')));
      expect(text(fixture)).toBe('');
    });
  });

  describe('the cost cluster', () => {
    it('omits the cost and the bullet entirely when estimatedCostUSD is null', async () => {
      const fixture = await mount(stub(breakdown({ estimatedCostUSD: null, priceSource: 'unavailable' })));
      expect(text(fixture)).toBe('15.4K tokens15.4K');
      expect(text(fixture)).not.toContain('•');
      // The `unavailable` marker lives INSIDE the cost cluster, so a null cost
      // takes the marker with it — v4 nests them.
      expect(fixture.nativeElement.querySelector('[title="Pricing data unavailable"]')).toBeNull();
    });

    it('shows the info icon + tooltip for an openrouter-estimate price source', async () => {
      const fixture = await mount(stub(breakdown({ priceSource: 'openrouter-estimate' })));
      const marker = fixture.nativeElement.querySelector(
        '[title="Cost estimated using OpenRouter pricing data. Actual cost may vary."]',
      );
      expect(marker).not.toBeNull();
      expect(marker.querySelector('qt-icon')).not.toBeNull();
    });

    it('shows the "*" marker for an unavailable price source with a non-null cost', async () => {
      const fixture = await mount(stub(breakdown({ priceSource: 'unavailable' })));
      const marker = fixture.nativeElement.querySelector('[title="Pricing data unavailable"]');
      expect(marker).not.toBeNull();
      expect(marker.textContent).toBe('*');
    });

    it('shows no marker for an ordinary price source', async () => {
      const fixture = await mount(stub(breakdown({ priceSource: 'registry' })));
      expect(fixture.nativeElement.querySelector('qt-icon')).toBeNull();
      expect(text(fixture)).toBe('15.4K tokens15.4K•$0.023');
    });

    it('renders a zero cost as "Free" (the format-tokens banding)', async () => {
      const fixture = await mount(stub(breakdown({ estimatedCostUSD: 0 })));
      expect(text(fixture)).toBe('15.4K tokens15.4K•Free');
    });
  });

  it('re-fetches when refreshKey changes (v4 keys the effect on it)', async () => {
    const s = stub(breakdown());
    const fixture = await mount(s, { refreshKey: 3 });
    expect(s.calls).toHaveLength(1);

    fixture.componentRef.setInput('refreshKey', 4);
    fixture.detectChanges();
    await settle(fixture);

    expect(s.calls).toHaveLength(2);
  });
});
