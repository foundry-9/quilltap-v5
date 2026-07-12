import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { HousekeepingConfig } from '../../../core/core-contract';
import { MemoryHousekeepingCard } from './memory-housekeeping-card';

interface DispatchReq {
  type: string;
  config?: Partial<HousekeepingConfig>;
  [k: string]: unknown;
}

function render(
  dispatch: (req: DispatchReq) => Promise<Record<string, unknown>>,
): ComponentFixture<MemoryHousekeepingCard> {
  const client: Partial<CoreClient> = { dispatchData: dispatch as CoreClient['dispatchData'] };
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [MemoryHousekeepingCard],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(MemoryHousekeepingCard);
  fixture.detectChanges();
  return fixture;
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

function baseDispatch(seen: DispatchReq[]): (req: DispatchReq) => Promise<Record<string, unknown>> {
  return async (req: DispatchReq) => {
    seen.push(req);
    if (req.type === 'memoryHousekeepingConfigGet') {
      return {
        settings: {
          enabled: false,
          perCharacterCap: 2000,
          perCharacterCapOverrides: {},
          autoMergeSimilarThreshold: 0.9,
          mergeSimilar: false,
        },
      };
    }
    if (req.type === 'memoryCharacterCounts') {
      return {
        characters: [
          { id: 'char-aaaa1111', name: 'Lorian', memoryCount: 2500 },
          { id: 'char-bbbb2222', name: 'Riya', memoryCount: 40 },
        ],
      };
    }
    if (req.type === 'memoryHousekeepingConfigSet') {
      return { settings: { ...req.config } };
    }
    return {};
  };
}

describe('MemoryHousekeepingCard', () => {
  it('loads config + character counts', async () => {
    const fixture = render(baseDispatch([]));
    await settle(fixture);
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Enable automatic housekeeping');
    const cap = fixture.nativeElement.querySelector('#perCharacterCap') as HTMLInputElement;
    expect(cap.value).toBe('2000');
    // Character counts loaded → the (collapsed) overrides toggle appears.
    expect(text).toContain('Per-character overrides (0 active)');
    // Names live inside the collapsed section (v4 parity) — expand to see them.
    Array.from(fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>)
      .find((b) => b.textContent?.includes('Per-character overrides'))!
      .click();
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('Lorian');
    expect(fixture.nativeElement.textContent).toContain('Riya');
  });

  it('toggles enabled with a merge-patch', async () => {
    const seen: DispatchReq[] = [];
    const fixture = render(baseDispatch(seen));
    await settle(fixture);
    const enable = fixture.nativeElement.querySelector('input[type="checkbox"]') as HTMLInputElement;
    enable.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(seen.find((r) => r.type === 'memoryHousekeepingConfigSet')?.config).toEqual({
      enabled: true,
    });
  });

  it('saves a per-character cap on blur, ignoring out-of-range', async () => {
    const seen: DispatchReq[] = [];
    const fixture = render(baseDispatch(seen));
    await settle(fixture);
    const cap = fixture.nativeElement.querySelector('#perCharacterCap') as HTMLInputElement;
    // Out of range → no save.
    cap.value = '5';
    cap.dispatchEvent(new Event('blur'));
    await settle(fixture);
    expect(seen.some((r) => r.type === 'memoryHousekeepingConfigSet')).toBe(false);
    // Valid change → save.
    cap.value = '5000';
    cap.dispatchEvent(new Event('blur'));
    await settle(fixture);
    expect(seen.find((r) => r.type === 'memoryHousekeepingConfigSet')?.config).toEqual({
      perCharacterCap: 5000,
    });
  });

  it('adds a per-character override on blur', async () => {
    const seen: DispatchReq[] = [];
    const fixture = render(baseDispatch(seen));
    await settle(fixture);
    // Reveal overrides.
    Array.from(
      fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
    )
      .find((b) => b.textContent?.includes('Per-character overrides'))!
      .click();
    fixture.detectChanges();
    const overrideInputs = fixture.nativeElement.querySelectorAll(
      'input[type="number"][aria-label^="Cap override"]',
    ) as NodeListOf<HTMLInputElement>;
    expect(overrideInputs.length).toBe(2);
    overrideInputs[0].value = '3000';
    overrideInputs[0].dispatchEvent(new Event('blur'));
    await settle(fixture);
    const setReq = seen.find((r) => r.type === 'memoryHousekeepingConfigSet');
    expect(setReq?.config).toEqual({ perCharacterCapOverrides: { 'char-aaaa1111': 3000 } });
  });

  it('runs a sweep now', async () => {
    const seen: DispatchReq[] = [];
    const fixture = render(baseDispatch(seen));
    await settle(fixture);
    Array.from(fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>)
      .find((b) => b.textContent?.trim() === 'Run housekeeping now')!
      .click();
    await settle(fixture);
    expect(seen.some((r) => r.type === 'memoryHousekeepSweep')).toBe(true);
  });
});
