import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { MemoryRecallCard } from './memory-recall-card';

interface DispatchReq {
  type: string;
  config?: Record<string, unknown>;
  [k: string]: unknown;
}

function render(
  dispatch: (req: DispatchReq) => Promise<Record<string, unknown>>,
): ComponentFixture<MemoryRecallCard> {
  const client: Partial<CoreClient> = { dispatchData: dispatch as CoreClient['dispatchData'] };
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [MemoryRecallCard],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(MemoryRecallCard);
  fixture.detectChanges();
  return fixture;
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

describe('MemoryRecallCard', () => {
  it('loads the config and shows the active policy description', async () => {
    const fixture = render(async (req) =>
      req.type === 'memoryRecallConfigGet'
        ? { settings: { scopePolicy: 'exclude', expandRelated: true } }
        : {},
    );
    await settle(fixture);
    const select = fixture.nativeElement.querySelector('select') as HTMLSelectElement;
    expect(select.value).toBe('exclude');
    expect(fixture.nativeElement.textContent).toContain('The firmest guard against cross-project leakage.');
    const checkbox = fixture.nativeElement.querySelector('input[type="checkbox"]') as HTMLInputElement;
    expect(checkbox.checked).toBe(true);
  });

  it('saves scopePolicy on change (merge-patch)', async () => {
    const seen: DispatchReq[] = [];
    const fixture = render(async (req) => {
      seen.push(req);
      if (req.type === 'memoryRecallConfigGet') return { settings: { scopePolicy: 'down-weight' } };
      if (req.type === 'memoryRecallConfigSet') return { settings: req.config };
      return {};
    });
    await settle(fixture);
    const select = fixture.nativeElement.querySelector('select') as HTMLSelectElement;
    select.value = 'exclude';
    select.dispatchEvent(new Event('change'));
    await settle(fixture);
    const setReq = seen.find((r) => r.type === 'memoryRecallConfigSet');
    expect(setReq?.config).toEqual({ scopePolicy: 'exclude' });
  });

  it('saves expandRelated on toggle', async () => {
    const seen: DispatchReq[] = [];
    const fixture = render(async (req) => {
      seen.push(req);
      if (req.type === 'memoryRecallConfigGet') return { settings: {} };
      if (req.type === 'memoryRecallConfigSet') return { settings: req.config };
      return {};
    });
    await settle(fixture);
    const checkbox = fixture.nativeElement.querySelector('input[type="checkbox"]') as HTMLInputElement;
    checkbox.checked = true;
    checkbox.dispatchEvent(new Event('change'));
    await settle(fixture);
    const setReq = seen.find((r) => r.type === 'memoryRecallConfigSet');
    expect(setReq?.config).toEqual({ expandRelated: true });
  });

  it('surfaces a save error', async () => {
    const fixture = render(async (req) => {
      if (req.type === 'memoryRecallConfigGet') return { settings: {} };
      if (req.type === 'memoryRecallConfigSet') throw new Error('recall save failed');
      return {};
    });
    await settle(fixture);
    const checkbox = fixture.nativeElement.querySelector('input[type="checkbox"]') as HTMLInputElement;
    checkbox.checked = true;
    checkbox.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(fixture.nativeElement.textContent).toContain('recall save failed');
  });
});
