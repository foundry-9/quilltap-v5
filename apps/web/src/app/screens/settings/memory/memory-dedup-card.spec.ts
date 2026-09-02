import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { DedupCharacterResult, DedupPreviewDto } from '../../../core/core-contract';
import { ToastService } from '../../../ui/toast.service';
import { MemoryDedupCard } from './memory-dedup-card';

/**
 * Ported from v4's `memory-dedup-card.tsx`: the Analyze→preview→Run state
 * machine, the zero-removable Run disable, the exact success toast, and the
 * inline-only (NOT toasted) preview failure.
 */

function char(over: Partial<DedupCharacterResult>): DedupCharacterResult {
  return {
    characterId: 'c1',
    characterName: 'Aria',
    originalCount: 10,
    withEmbeddings: 10,
    withoutEmbeddings: 0,
    clustersFound: 0,
    memoriesInClusters: 0,
    removedCount: 0,
    mergedDetailCount: 0,
    finalCount: 10,
    ...over,
  };
}

function preview(over: Partial<DedupPreviewDto>): DedupPreviewDto {
  return {
    threshold: 0.8,
    dryRun: true,
    characters: [char({})],
    totalOriginal: 10,
    totalRemoved: 0,
    totalMergedDetails: 0,
    totalFinal: 10,
    processedAt: '2026-01-01T00:00:00Z',
    ...over,
  };
}

const toasts = { showSuccess: vi.fn(), showError: vi.fn() } as unknown as ToastService;

interface Stub {
  client: Partial<CoreClient>;
  run: ReturnType<typeof vi.fn>;
}

function stubClient(previewImpl: () => Promise<DedupPreviewDto>, runResult?: DedupPreviewDto): Stub {
  const run = vi.fn(async () => runResult ?? preview({ totalRemoved: 3, totalMergedDetails: 2 }));
  return {
    run,
    client: {
      memoryDedupPreview: previewImpl as unknown as CoreClient['memoryDedupPreview'],
      memoryDedupRun: run as unknown as CoreClient['memoryDedupRun'],
    },
  };
}

async function mount(stub: Stub): Promise<ComponentFixture<MemoryDedupCard>> {
  vi.clearAllMocks();
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [MemoryDedupCard],
    providers: [
      { provide: CoreClient, useValue: stub.client },
      { provide: ToastService, useValue: toasts },
    ],
  });
  const fixture = TestBed.createComponent(MemoryDedupCard);
  fixture.detectChanges();
  return fixture;
}

async function settle(fixture: ComponentFixture<MemoryDedupCard>): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
  fixture.detectChanges();
}

function text(fixture: ComponentFixture<MemoryDedupCard>): string {
  return (fixture.nativeElement as HTMLElement).textContent ?? '';
}

function byText(fixture: ComponentFixture<MemoryDedupCard>, label: string): HTMLButtonElement {
  const btn = [...(fixture.nativeElement as HTMLElement).querySelectorAll('button')].find(
    (b) => (b as HTMLButtonElement).textContent?.trim() === label,
  );
  if (!btn) throw new Error(`button "${label}" not found`);
  return btn as HTMLButtonElement;
}

describe('MemoryDedupCard', () => {
  it('disables Run when the preview has nothing removable', async () => {
    const stub = stubClient(async () => preview({ totalRemoved: 0 }));
    const fixture = await mount(stub);
    byText(fixture, 'Analyze Memories').click();
    await settle(fixture);

    expect(text(fixture)).toContain('No duplicate memories found at this threshold');
    expect(byText(fixture, 'Run Deduplication').disabled).toBe(true);
  });

  it('enables Run when removable > 0, then runs and toasts the exact sentence', async () => {
    const stub = stubClient(
      async () => preview({ totalRemoved: 3, characters: [char({ clustersFound: 2, removedCount: 3, finalCount: 7 })] }),
      preview({ totalRemoved: 3, totalMergedDetails: 2, totalFinal: 7, characters: [char({ removedCount: 3, finalCount: 7 })] }),
    );
    const fixture = await mount(stub);
    byText(fixture, 'Analyze Memories').click();
    await settle(fixture);

    const runBtn = byText(fixture, 'Run Deduplication');
    expect(runBtn.disabled).toBe(false);
    runBtn.click();
    await settle(fixture);

    expect(stub.run).toHaveBeenCalledWith(0.8);
    expect(text(fixture)).toContain('Deduplication Complete');
    expect(toasts.showSuccess).toHaveBeenCalledWith(
      'Removed 3 duplicate memories, merged 2 details',
    );
  });

  it('shows a preview failure inline and does NOT toast it', async () => {
    const stub = stubClient(async () => {
      throw new Error('Failed to analyze memories');
    });
    const fixture = await mount(stub);
    byText(fixture, 'Analyze Memories').click();
    await settle(fixture);

    expect(text(fixture)).toContain('Failed to analyze memories');
    expect(toasts.showError).not.toHaveBeenCalled();
    expect(toasts.showSuccess).not.toHaveBeenCalled();
  });
});

/**
 * P4.69 — the slider readout beside the similarity slider. v4
 * `components/tools/memory-dedup-card.tsx:133` renders
 * `Similarity Threshold: {threshold.toFixed(2)}` — the colon and the TWO
 * decimals are the string; a bare `0.9` would read wrong. P4.D142 recorded it
 * as unpinned when `.qt-range` was adopted here.
 */
describe('MemoryDedupCard — the slider readout (P4.69, v4 memory-dedup-card.tsx:133)', () => {
  it('reads "Similarity Threshold: N.NN", two decimals (v4 :133 toFixed(2))', async () => {
    const stub = stubClient(async () => preview({ totalRemoved: 1 }));
    const fixture = await mount(stub);
    const slider = (fixture.nativeElement as HTMLElement).querySelector(
      'input[type="range"]',
    ) as HTMLInputElement;
    expect(slider).not.toBeNull();
    // Whatever the default is, it is rendered to exactly two decimals.
    expect(text(fixture).replace(/\s+/g, ' ')).toContain(
      `Similarity Threshold: ${Number(slider.value).toFixed(2)}`,
    );
  });
});
