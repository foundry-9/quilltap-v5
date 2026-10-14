import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { CoreDispatchError } from '../../../core/core-contract';
import type { MaintenanceLedgerStatus, MaintenanceSummary } from '../../../core/core-contract';
import { MemoryMaintenanceLedger, summaryLine } from './memory-maintenance-ledger';

/**
 * P4.D300 — the maintenance ledger and `summaryLine`, v4
 * `components/tools/memory-maintenance-ledger.tsx` (`b045dd986`, `48790fb73`)
 * at `83334b9a5`. v4 shipped no test for the ledger; these rows are the
 * lane's own (v4's card test `:134` covers one line, transcribed in the card spec).
 */

function summary(over: Partial<MaintenanceSummary> = {}): MaintenanceSummary {
  return {
    characterId: 'c1',
    reason: 'manual',
    dryRun: false,
    finishedAt: '2026-10-13T22:24:00.000Z',
    durationMs: 10,
    phases: [],
    ...over,
  };
}

describe('summaryLine (v4 memory-maintenance-ledger.tsx:38-50)', () => {
  it('both skips', () => {
    expect(summaryLine(summary({ skippedReason: 'archived', fold: { eligible: 1, groups: 1, folded: 1, groupsFailed: 0, groupsDeferred: 0 } }))).toBe(
      'skipped: the character is archived',
    );
    expect(summaryLine(summary({ skippedReason: 'character-not-found' }))).toBe('skipped: character not found');
  });

  it('nothing to do', () => {
    expect(summaryLine(summary())).toBe('nothing to do');
  });

  it('every part, joined with " · ", demote WITHOUT toLocaleString', () => {
    expect(
      summaryLine(
        summary({
          fold: { eligible: 40, groups: 2, folded: 3, groupsFailed: 0, groupsDeferred: 0 },
          consolidation: { digestsCreated: 2, digestsUpdated: 1, membersSuperseded: 9, clustersFailed: 0, clustersDeferred: 0 },
          demote: { cap: 2000, hotTotal: 2500, demotable: 12100, protected: 400, excess: 100, demoted: 1100, highestScoreDemoted: null },
          retire: { retentionDays: 90, retired: 4 },
        }),
      ),
    ).toBe('3 folded · 3 digests, 9 superseded · 1100 archived (12100/2000 counted) · 4 let go');
  });

  it('a skipped consolidation names its reason', () => {
    expect(
      summaryLine(
        summary({
          consolidation: { digestsCreated: 0, digestsUpdated: 0, membersSuperseded: 0, clustersFailed: 0, clustersDeferred: 0, skippedReason: 'no-llm' },
        }),
      ),
    ).toBe('consolidation skipped (no-llm)');
  });
});

interface Req {
  type: string;
}

function render(
  ledger: () => Record<string, unknown>,
  characters: { id: string; name: string }[] = [],
  seen: Req[] = [],
): ComponentFixture<MemoryMaintenanceLedger> {
  const client: Partial<CoreClient> = {
    dispatchData: (async (req: Req) => {
      seen.push(req);
      return ledger();
    }) as CoreClient['dispatchData'],
  };
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [MemoryMaintenanceLedger],
    providers: [
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      { provide: CoreClient, useValue: client },
    ],
  });
  const fixture = TestBed.createComponent(MemoryMaintenanceLedger);
  fixture.componentRef.setInput('characters', characters);
  fixture.detectChanges();
  return fixture;
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 8; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

const host = (f: ComponentFixture<unknown>) => f.nativeElement as HTMLElement;
const text = (f: ComponentFixture<unknown>) => (host(f).textContent ?? '').replace(/\s+/g, ' ').trim();

function entry(over: Partial<MaintenanceLedgerStatus> = {}): MaintenanceLedgerStatus {
  return { characterId: 'abcdef12-0000-0000-0000-000000000001', lastRun: {}, lastSummary: null, lastDemotion: null, ...over };
}

describe('MemoryMaintenanceLedger (v4 memory-maintenance-ledger.tsx:58-101)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('reads memoryMaintenanceStatus with no characterId; loading line first', async () => {
    const seen: Req[] = [];
    const f = render(() => ({ success: true, ledger: [] }), [], seen);
    expect(text(f)).toBe('Opening the ledger…');
    await settle(f);
    expect(seen).toEqual([{ type: 'memoryMaintenanceStatus' }]);
  });

  it('the blank ledger — no entries, or only entries with neither a summary nor a run', async () => {
    const f = render(() => ({ success: true, ledger: [entry()] }));
    await settle(f);
    expect(text(f)).toBe('The ledger is blank: no round has been recorded since the house last opened its doors.');
  });

  it('a read failure: readErrorText — the fallback for a server failure, destructive', async () => {
    const f = render(() => {
      throw new CoreDispatchError({ kind: 'internal', message: 'boom' });
    });
    await settle(f);
    const p = host(f).querySelector('p.qt-text-destructive');
    expect(p?.textContent).toBe('Failed to read the maintenance ledger');
  });

  it('an entry: the name from the counts list, the last-run line, the summary, all five phases with never', async () => {
    const f = render(
      () => ({
        success: true,
        ledger: [
          entry({
            characterId: 'mnemo',
            lastRun: { demote: { at: 'not-a-date', reason: 'watermark' } },
            lastSummary: summary({ characterId: 'mnemo', dryRun: true, finishedAt: 'also-not-a-date', reason: 'scheduled', retire: { retentionDays: 9, retired: 2 } }),
          }),
        ],
      }),
      [{ id: 'mnemo', name: 'Mnemo' }],
    );
    await settle(f);
    const card = host(f).querySelector('.qt-card') as HTMLElement;
    expect(card.querySelector('p.qt-text-label')?.textContent).toBe('Mnemo');
    expect(card.querySelector('p.qt-text-xs.qt-text-muted')?.textContent).toBe('last rehearsal also-not-a-date (daily round)');
    expect(card.querySelector('p.qt-text-small')?.textContent).toBe('2 let go');
    const phases = Array.from(card.querySelectorAll('li')).map((li) => Array.from(li.querySelectorAll('span')).map((s) => s.textContent));
    expect(phases).toEqual([
      ['Fold near-duplicates', 'never'],
      ['Consolidate', 'never'],
      ['Demote over the cap', 'not-a-date · nearing the cap'],
      ['Retire old archive notes', 'never'],
      ['Refresh the Commonplace files', 'never'],
    ]);
  });

  it('a parseable date reads toLocaleString; a run reads "last run"; by hand', async () => {
    const at = '2026-10-13T22:24:00.000Z';
    const f = render(() => ({ success: true, ledger: [entry({ lastSummary: summary({ finishedAt: at }), lastRun: { retire: { at, reason: 'manual' } } })] }));
    await settle(f);
    const local = new Date(at).toLocaleString();
    expect(text(f)).toContain(`last run ${local} (by hand)`);
    expect(text(f)).toContain(`${local} · by hand`);
  });

  it('the name falls back to the id\'s first eight characters; a run-only entry has no summary lines', async () => {
    const f = render(() => ({ success: true, ledger: [entry({ lastRun: { mirror: { at: 'x', reason: 'manual' } } })] }));
    await settle(f);
    const card = host(f).querySelector('.qt-card') as HTMLElement;
    expect(card.querySelector('p.qt-text-label')?.textContent).toBe('abcdef12');
    expect(card.querySelector('p.qt-text-small')).toBeNull();
    expect(card.querySelectorAll('.flex-wrap p').length).toBe(1);
  });

  it('lastDemotion: muted "Within the cap…" at excess 0, warning otherwise, silent when effective (48790fb73)', async () => {
    const demotion = (d: MaintenanceLedgerStatus['lastDemotion']) =>
      render(() => ({ success: true, ledger: [entry({ lastSummary: summary(), lastDemotion: d })] }));
    const within = demotion({ at: 'x', demoted: 0, excess: 0, effective: false });
    await settle(within);
    const w = Array.from(host(within).querySelectorAll('p.qt-text-xs')).pop() as HTMLElement;
    expect(w.textContent?.trim()).toBe('Within the cap at the last demotion; automatic demotion looks again in an hour.');
    expect(w.classList.contains('qt-text-muted')).toBe(true);
    expect(w.classList.contains('qt-text-warning')).toBe(false);

    const over = demotion({ at: 'x', demoted: 2, excess: 40, effective: false });
    await settle(over);
    const o = Array.from(host(over).querySelectorAll('p.qt-text-xs')).pop() as HTMLElement;
    expect(o.textContent?.trim()).toBe(
      'The last demotion moved too little of the excess to be worth repeating at once; automatic demotion rests for an hour after such a round.',
    );
    expect(o.classList.contains('qt-text-warning')).toBe(true);
    expect(o.classList.contains('qt-text-muted')).toBe(false);

    const effective = demotion({ at: 'x', demoted: 40, excess: 40, effective: true });
    await settle(effective);
    expect(text(effective)).not.toContain('demotion');
  });
});
