import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { MemoryBackfillCard } from './memory-backfill-card';

interface DispatchReq {
  type: string;
  batchSize?: number;
  [k: string]: unknown;
}

function render(
  dispatch: (req: DispatchReq) => Promise<Record<string, unknown>>,
): ComponentFixture<MemoryBackfillCard> {
  const client: Partial<CoreClient> = { dispatchData: dispatch as CoreClient['dispatchData'] };
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [MemoryBackfillCard],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(MemoryBackfillCard);
  fixture.detectChanges();
  return fixture;
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

describe('MemoryBackfillCard', () => {
  it('renders the missing/in-flight counts', async () => {
    const fixture = render(async (req) =>
      req.type === 'memoryBackfillProgress' ? { progress: { remaining: 42, inFlight: 3 } } : {},
    );
    await settle(fixture);
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('42');
    expect(text).toContain('3');
    expect(text).toContain('Backfill up to 500 memories');
  });

  it('disables the button and shows Nothing to backfill when remaining is 0', async () => {
    const fixture = render(async (req) =>
      req.type === 'memoryBackfillProgress' ? { progress: { remaining: 0, inFlight: 0 } } : {},
    );
    await settle(fixture);
    const btn = fixture.nativeElement.querySelector('button') as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
    expect(btn.textContent).toContain('Nothing to backfill');
  });

  it('starts a backfill of 500', async () => {
    const seen: DispatchReq[] = [];
    const fixture = render(async (req) => {
      seen.push(req);
      if (req.type === 'memoryBackfillProgress') return { progress: { remaining: 10, inFlight: 0 } };
      if (req.type === 'memoryBackfillStart') return { enqueued: 10, remaining: 0 };
      return {};
    });
    await settle(fixture);
    (fixture.nativeElement.querySelector('button') as HTMLButtonElement).click();
    await settle(fixture);
    expect(seen.some((r) => r.type === 'memoryBackfillStart' && r.batchSize === 500)).toBe(true);
  });

  it('surfaces a start error', async () => {
    const fixture = render(async (req) => {
      if (req.type === 'memoryBackfillProgress') return { progress: { remaining: 10, inFlight: 0 } };
      if (req.type === 'memoryBackfillStart') throw new Error('queue is full');
      return {};
    });
    await settle(fixture);
    (fixture.nativeElement.querySelector('button') as HTMLButtonElement).click();
    await settle(fixture);
    expect(fixture.nativeElement.textContent).toContain('queue is full');
  });
});
