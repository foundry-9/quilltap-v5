import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { HousekeepingPreview } from '../core/core-contract';
import { HousekeepingDialog } from './housekeeping-dialog';

interface DispatchReq {
  type: string;
  options?: Record<string, unknown>;
  [k: string]: unknown;
}

function previewOf(over: Partial<HousekeepingPreview>): HousekeepingPreview {
  return {
    wouldDelete: 0,
    wouldMerge: 0,
    wouldKeep: 0,
    totalBefore: 0,
    totalAfter: 0,
    details: [],
    ...over,
  };
}

function render(
  dispatch: (req: DispatchReq) => Promise<Record<string, unknown>>,
): ComponentFixture<HousekeepingDialog> {
  const client: Partial<CoreClient> = { dispatchData: dispatch as CoreClient['dispatchData'] };
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [HousekeepingDialog],
    providers: [{ provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(HousekeepingDialog);
  fixture.componentRef.setInput('characterId', 'c1');
  fixture.detectChanges();
  return fixture;
}

async function settlePreview(fixture: ComponentFixture<unknown>): Promise<void> {
  // Past the 300ms preview debounce, then let the fetch settle.
  await new Promise((r) => setTimeout(r, 350));
  for (let i = 0; i < 5; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

describe('HousekeepingDialog', () => {
  it('debounces then loads the preview and shows the Keep/Delete/Merge stats', async () => {
    const dispatch = vi.fn(async (req: DispatchReq) => {
      if (req.type === 'memoryHousekeepPreview') {
        return { preview: previewOf({ wouldKeep: 12, wouldDelete: 3, wouldMerge: 1 }) };
      }
      return {};
    });
    const fixture = render(dispatch);
    await settlePreview(fixture);
    expect(dispatch).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'memoryHousekeepPreview', characterId: 'c1' }),
    );
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Keep');
    expect(text).toContain('12');
    expect(text).toContain('3');
    expect(text).toContain('1');
  });

  it('lists changed details, omitting kept rows', async () => {
    const dispatch = async (req: DispatchReq) => {
      if (req.type === 'memoryHousekeepPreview') {
        return {
          preview: previewOf({
            wouldDelete: 1,
            wouldMerge: 0,
            details: [
              { memoryId: 'd1', action: 'deleted', reason: 'stale', summary: 'A stale note' },
              { memoryId: 'k1', action: 'kept', reason: 'protected', summary: 'Kept note' },
            ],
          }),
        };
      }
      return {};
    };
    const fixture = render(dispatch);
    await settlePreview(fixture);
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Changes Preview');
    expect(text).toContain('A stale note');
    expect(text).toContain('stale');
    expect(text).not.toContain('Kept note');
  });

  it('disables Run when nothing would change', async () => {
    const fixture = render(async (req: DispatchReq) =>
      req.type === 'memoryHousekeepPreview' ? { preview: previewOf({ wouldKeep: 5 }) } : {},
    );
    await settlePreview(fixture);
    const runBtn = Array.from(
      fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
    ).find((b) => b.textContent?.includes('Delete') && b.textContent?.includes('Memories'))!;
    expect(runBtn.disabled).toBe(true);
    expect(fixture.nativeElement.textContent).toContain('No memories to clean up with current settings.');
  });

  it('runs housekeeping with dryRun:false and emits complete', async () => {
    const seen: DispatchReq[] = [];
    const dispatch = async (req: DispatchReq) => {
      seen.push(req);
      if (req.type === 'memoryHousekeepPreview') {
        return { preview: previewOf({ wouldDelete: 4 }) };
      }
      if (req.type === 'memoryHousekeep') {
        return { result: { deleted: 4, merged: 0, kept: 0, totalBefore: 4, totalAfter: 0, deletedIds: [], mergedIds: [] } };
      }
      return {};
    };
    const fixture = render(dispatch);
    await settlePreview(fixture);
    let completed = false;
    fixture.componentInstance.complete.subscribe(() => (completed = true));
    const runBtn = Array.from(
      fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
    ).find((b) => b.textContent?.includes('Delete 4 Memories'))!;
    expect(runBtn.disabled).toBe(false);
    runBtn.click();
    for (let i = 0; i < 5; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    const runReq = seen.find((r) => r.type === 'memoryHousekeep');
    expect(runReq?.options).toMatchObject({ characterId: 'c1', dryRun: false });
    expect(completed).toBe(true);
  });

  it('surfaces a preview error', async () => {
    const fixture = render(async (req: DispatchReq) => {
      if (req.type === 'memoryHousekeepPreview') {
        throw new Error('preview boom');
      }
      return {};
    });
    await settlePreview(fixture);
    expect(fixture.nativeElement.textContent).toContain('preview boom');
  });
});
