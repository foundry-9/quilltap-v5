import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { HousekeepingPreview } from '../core/core-contract';
import { ToastService } from '../ui/toast.service';
import { HousekeepingDialog } from './housekeeping-dialog';

interface DispatchReq {
  type: string;
  options?: Record<string, unknown>;
  [k: string]: unknown;
}

function previewOf(over: Partial<HousekeepingPreview>): HousekeepingPreview {
  return {
    wouldDemote: 0,
    wouldDelete: 0,
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
  it('debounces then loads the preview and shows the Keep/Delete/Archive stats (v4 :208-227)', async () => {
    const dispatch = vi.fn(async (req: DispatchReq) => {
      if (req.type === 'memoryHousekeepPreview') {
        return { preview: previewOf({ wouldKeep: 12, wouldDelete: 3, wouldDemote: 7 }) };
      }
      return {};
    });
    const fixture = render(dispatch);
    await settlePreview(fixture);
    expect(dispatch).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'memoryHousekeepPreview', characterId: 'c1' }),
    );
    // Each tile's number BY TILE: the label paragraph's sibling.
    const tiles = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('.grid.grid-cols-3 > div'),
    ).map((t) => [t.querySelectorAll('p')[1].textContent?.trim(), t.querySelectorAll('p')[0].textContent?.trim()]);
    expect(tiles).toEqual([
      ['Keep', '12'],
      ['Delete', '3'],
      ['Archive', '7'],
    ]);
    expect(fixture.nativeElement.textContent).not.toContain('Merge');
  });

  it('lists changed details, omitting kept rows', async () => {
    const dispatch = async (req: DispatchReq) => {
      if (req.type === 'memoryHousekeepPreview') {
        return {
          preview: previewOf({
            wouldDelete: 1,
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
    ).find((b) => b.textContent?.includes('Archive') && b.textContent?.includes('Memories'))!;
    expect(runBtn.disabled).toBe(true);
    // v4 :259-260.
    expect(fixture.nativeElement.textContent).toContain(
      'Nothing to send to the archive with current settings.',
    );
    expect(fixture.nativeElement.textContent).toContain('All memories are within retention policy.');
  });

  it('runs housekeeping with dryRun:false and emits complete', async () => {
    const seen: DispatchReq[] = [];
    const dispatch = async (req: DispatchReq) => {
      seen.push(req);
      if (req.type === 'memoryHousekeepPreview') {
        return { preview: previewOf({ wouldDelete: 4 }) };
      }
      if (req.type === 'memoryHousekeep') {
        return { result: { demoted: 0, deleted: 4, merged: 0, kept: 0, totalBefore: 4, totalAfter: 0, deletedIds: [], mergedIds: [] } };
      }
      return {};
    };
    const fixture = render(dispatch);
    await settlePreview(fixture);
    let completed = false;
    fixture.componentInstance.complete.subscribe(() => (completed = true));
    const runBtn = Array.from(
      fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
    ).find((b) => b.textContent?.includes('Archive 4 Memories'))!;
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

/**
 * P4.69 — the slider readout beside the importance slider. v4
 * `components/memory/housekeeping-dialog.tsx:173` renders
 * `Min Importance: {(minImportance * 100).toFixed(0)}%` — a WHOLE-number
 * percentage with the `%` sign, off a 0..1 stored value. P4.D142 recorded it as
 * unpinned when `.qt-range` was adopted here.
 */
describe('HousekeepingDialog — the slider readout (P4.69, v4 housekeeping-dialog.tsx:173)', () => {
  it('reads "Min Importance: N%" — whole percent, no decimals (v4 :173 toFixed(0))', () => {
    const fixture = render(async () => ({}));
    const slider = (fixture.nativeElement as HTMLElement).querySelector(
      'input[type="range"]',
    ) as HTMLInputElement;
    expect(slider).not.toBeNull();
    const label = ((fixture.nativeElement as HTMLElement).textContent ?? '').replace(/\s+/g, ' ');
    const expected = (Number(slider.value) * 100).toFixed(0);
    expect(label).toContain(`Min Importance: ${expected}%`);
    // The percent is whole: no "42.5%" can satisfy the line above by accident.
    expect(/Min Importance: \d+%/.test(label)).toBe(true);
  });
});

/**
 * P4.D276 — v4 `d58548051`: housekeeping ARCHIVES. Every string below is read
 * from v4 `components/memory/housekeeping-dialog.tsx` at `01a83539d` (line pins
 * per arm).
 */
describe('HousekeepingDialog — the archive (P4.D276, v4 d58548051)', () => {
  function runButton(fixture: ComponentFixture<HousekeepingDialog>): HTMLButtonElement {
    const buttons = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
    ) as HTMLButtonElement[];
    return buttons.find((b) => /Memories|Running\.\.\./.test(b.textContent ?? ''))!;
  }

  function withPreview(
    preview: HousekeepingPreview,
    seen: DispatchReq[] = [],
    result: Record<string, unknown> = {},
  ): (req: DispatchReq) => Promise<Record<string, unknown>> {
    return async (req: DispatchReq) => {
      seen.push(req);
      if (req.type === 'memoryHousekeepPreview') return { preview };
      if (req.type === 'memoryHousekeep') return { result };
      return {};
    };
  }

  async function runAndToast(result: Record<string, unknown>): Promise<string[]> {
    const fixture = render(withPreview(previewOf({ wouldDemote: 3 }), [], result));
    await settlePreview(fixture);
    runButton(fixture).click();
    for (let i = 0; i < 5; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    return TestBed.inject(ToastService)
      .toasts()
      .map((t) => `${t.type}:${t.message}`);
  }

  it('reads v4\'s description and option copy (v4 :126, :146, :169, :189)', () => {
    const fixture = render(async () => ({}));
    const text = ((fixture.nativeElement as HTMLElement).textContent ?? '').replace(/\s+/g, ' ');
    expect(text).toContain(
      'Old and low-importance memories are retired to the archive, never destroyed. They stay searchable by the gate and can return should the world ask for them again.',
    );
    expect(text).toContain('Never archiving memories that are:');
    expect(text).toContain('Archive old low-importance memories');
    expect(text).toContain('Threshold for archiving');
    expect(text).not.toContain('Not deleting memories that are:');
    expect(text).not.toContain('Threshold for deletion');
  });

  it('has no merge-similar checkbox (v4 hunk -189..-202)', () => {
    const fixture = render(async () => ({}));
    expect((fixture.nativeElement as HTMLElement).querySelector('input[type="checkbox"]')).toBeNull();
    expect(fixture.nativeElement.textContent).not.toContain('Merge similar memories');
  });

  it('warning-classes a demoted detail row (v4 :241-245)', async () => {
    const fixture = render(
      withPreview(
        previewOf({
          wouldDemote: 1,
          details: [{ memoryId: 'z1', action: 'demoted', reason: 'stale', summary: 'Sent cold' }],
        }),
      ),
    );
    await settlePreview(fixture);
    const row = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('.p-3.rounded-lg')).find(
      (el) => el.textContent?.includes('Sent cold'),
    )!;
    expect(row).toBeTruthy();
    expect(row.classList.contains('qt-bg-warning/10')).toBe(true);
    expect(row.classList.contains('qt-bg-destructive/10')).toBe(false);
    expect(fixture.nativeElement.textContent).toContain('Changes Preview');
  });

  it('disables Run at nothing-to-do and enables it on wouldDemote alone (v4 :278)', async () => {
    const idle = render(withPreview(previewOf({ wouldKeep: 5 })));
    await settlePreview(idle);
    expect(runButton(idle).disabled).toBe(true);
    expect(runButton(idle).textContent?.trim()).toBe('Archive 0 Memories');

    const demoteOnly = render(withPreview(previewOf({ wouldDemote: 2 })));
    await settlePreview(demoteOnly);
    expect(runButton(demoteOnly).disabled).toBe(false);
  });

  it('labels Run with the archive + delete sum on a primary button (v4 :279-282)', async () => {
    const fixture = render(withPreview(previewOf({ wouldDemote: 3, wouldDelete: 2 })));
    await settlePreview(fixture);
    const btn = runButton(fixture);
    expect(btn.textContent?.trim()).toBe('Archive 5 Memories');
    expect(btn.classList.contains('qt-button-primary')).toBe(true);
    expect(btn.classList.contains('qt-button-destructive')).toBe(false);
  });

  it('sends no mergeSimilar on the preview or the run (v4 hunk -48..-51, -78..-81)', async () => {
    const seen: DispatchReq[] = [];
    const fixture = render(withPreview(previewOf({ wouldDemote: 1 }), seen, { demoted: 1 }));
    await settlePreview(fixture);
    runButton(fixture).click();
    for (let i = 0; i < 5; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    const previewReq = seen.find((r) => r.type === 'memoryHousekeepPreview')!;
    const runReq = seen.find((r) => r.type === 'memoryHousekeep')!;
    expect(previewReq).toBeTruthy();
    expect(runReq).toBeTruthy();
    expect('mergeSimilar' in previewReq).toBe(false);
    expect('mergeSimilar' in (runReq.options ?? {})).toBe(false);
    expect(runReq.options).toEqual({
      characterId: 'c1',
      maxMemories: 1000,
      maxAgeMonths: 6,
      minImportance: 0.3,
      dryRun: false,
    });
  });

  it('toasts the singular sentence at demoted 1 (v4 :89-90)', async () => {
    expect(await runAndToast({ demoted: 1, deleted: 0 })).toContain(
      'success:One memory has been shown to the archive, where it will keep.',
    );
  });

  it('toasts the plural sentence over demoted ONLY — deleted is not counted (v4 :87-91)', async () => {
    expect(await runAndToast({ demoted: 3, deleted: 9 })).toContain(
      'success:3 memories have been shown to the archive, where they will keep.',
    );
  });

  it('toasts 0 on a pre-round reply that carries no demoted (v4 :87 `?? 0`, R-C)', async () => {
    expect(await runAndToast({ deleted: 4, merged: 0 })).toContain(
      'success:0 memories have been shown to the archive, where they will keep.',
    );
  });

  it('keeps the failure toast unchanged (v4 :95)', async () => {
    const fixture = render(async (req: DispatchReq) => {
      if (req.type === 'memoryHousekeepPreview') return { preview: previewOf({ wouldDemote: 1 }) };
      if (req.type === 'memoryHousekeep') throw new Error('the cellar flooded');
      return {};
    });
    await settlePreview(fixture);
    runButton(fixture).click();
    for (let i = 0; i < 5; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    expect(
      TestBed.inject(ToastService)
        .toasts()
        .map((t) => `${t.type}:${t.message}`),
    ).toContain('error:the cellar flooded');
  });
});
