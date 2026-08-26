import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { coreStreamStub } from '../../../core/core-client.testing';
import { MemoryRegenerateCard } from './memory-regenerate-card';

interface DispatchReq {
  type: string;
  [k: string]: unknown;
}

function render(
  dispatch: (req: DispatchReq) => Promise<Record<string, unknown>>,
): ComponentFixture<MemoryRegenerateCard> {
  const client = {
    ...coreStreamStub(),
    dispatchData: dispatch as CoreClient['dispatchData'],
  } as unknown as Partial<CoreClient>;
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [MemoryRegenerateCard],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(MemoryRegenerateCard);
  fixture.detectChanges();
  return fixture;
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

function buttonByText(fixture: ComponentFixture<unknown>, text: string): HTMLButtonElement {
  return Array.from(
    fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
  ).find((b) => b.textContent?.trim().includes(text))!;
}

describe('MemoryRegenerateCard', () => {
  it('shows the in-flight status line while a sweep is draining', async () => {
    const fixture = render(async (req) =>
      req.type === 'memoryRegenerateAllStatus'
        ? { inFlightFanOut: 1, inFlightWipes: 2, inFlightExtractions: 3, inFlight: 6 }
        : {},
    );
    await settle(fixture);
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('In flight:');
    expect(text).toContain('1 fan-out');
    expect(text).toContain('2 chat wipes');
    expect(text).toContain('3 extractions');
  });

  it('requires an inline confirm before regenerating', async () => {
    const seen: DispatchReq[] = [];
    const fixture = render(async (req) => {
      seen.push(req);
      if (req.type === 'memoryRegenerateAllStatus') return { inFlight: 0 };
      if (req.type === 'memoryRegenerateAll') return { success: true, jobId: 'j1', message: 'ok' };
      return {};
    });
    await settle(fixture);
    // First click reveals the confirm, does not dispatch.
    buttonByText(fixture, 'Delete and regenerate all memories').click();
    fixture.detectChanges();
    expect(seen.some((r) => r.type === 'memoryRegenerateAll')).toBe(false);
    expect(fixture.nativeElement.textContent).toContain(
      'This will delete and rebuild every chat-linked memory. Continue?',
    );
    // Confirm dispatches.
    buttonByText(fixture, 'Yes, regenerate').click();
    await settle(fixture);
    expect(seen.some((r) => r.type === 'memoryRegenerateAll')).toBe(true);
  });

  it('surfaces a regenerate error', async () => {
    const fixture = render(async (req) => {
      if (req.type === 'memoryRegenerateAllStatus') return { inFlight: 0 };
      if (req.type === 'memoryRegenerateAll') throw new Error('regen boom');
      return {};
    });
    await settle(fixture);
    buttonByText(fixture, 'Delete and regenerate all memories').click();
    fixture.detectChanges();
    buttonByText(fixture, 'Yes, regenerate').click();
    await settle(fixture);
    expect(fixture.nativeElement.textContent).toContain('regen boom');
  });
});
