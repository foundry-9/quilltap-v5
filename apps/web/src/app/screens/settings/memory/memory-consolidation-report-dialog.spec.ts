import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import type {
  ConsolidationClusterReport,
  ConsolidationReport,
  ConsolidationStats,
} from '../../../core/core-contract';
import { MemoryConsolidationReportDialog } from './memory-consolidation-report-dialog';

/**
 * P4.D276 — the consolidation report dialog, every string from v4
 * `components/tools/memory-consolidation-report-dialog.tsx` at `01a83539d`
 * (line pins per arm). R-H: hand markup, portalled to `document.body`.
 */

function stats(over: Partial<ConsolidationStats> = {}): ConsolidationStats {
  return {
    rowsLoaded: 0,
    candidates: 0,
    digestsLoaded: 0,
    immatureSkipped: 0,
    noEmbeddingSkipped: 0,
    bucketsScanned: 0,
    bucketRowsCapped: 0,
    clustersFound: 0,
    clustersQualified: 0,
    clustersSelected: 0,
    clustersDeferred: 0,
    clustersBelowMin: 0,
    clustersStale: 0,
    clustersAttempted: 0,
    clustersSucceeded: 0,
    clustersFailed: 0,
    clustersLostToTimeout: 0,
    digestsCreated: 0,
    digestsUpdated: 0,
    membersSuperseded: 0,
    contradictionsApplied: 0,
    rowsMarkedConsidered: 0,
    linksRewired: 0,
    mirrorFilesWritten: 0,
    budgetExhausted: false,
    durationMs: 0,
    ...over,
  };
}

function cluster(over: Partial<ConsolidationClusterReport> = {}): ConsolidationClusterReport {
  return {
    bucket: { kind: 'self', subjectCharacterId: 'c1', subjectName: 'Pip' },
    clusterKind: 'semantic',
    memberIds: ['m1', 'm2'],
    memberContents: ['Pip likes tea.', 'Pip drinks tea daily.'],
    existingDigestId: null,
    existingDigestContent: null,
    digests: [],
    keepStandalone: [],
    contradictions: [],
    status: 'planned',
    ...over,
  };
}

function report(over: Partial<ConsolidationReport> = {}): ConsolidationReport {
  return {
    characterId: 'c1',
    characterName: 'Pip',
    dryRun: true,
    trigger: 'manual',
    startedAt: '2026-10-09T00:00:00.000Z',
    finishedAt: '2026-10-09T00:00:01.000Z',
    settings: {
      enabled: false,
      connectionProfileId: null,
      clusterThreshold: 0.72,
      minClusterSize: 3,
      maxClusterSize: 30,
      matureAfterDays: 7,
      maxClustersPerRun: 40,
      watermark: 150,
      coldRetentionDays: null,
    },
    clusters: [],
    stats: stats(),
    ...over,
  };
}

function mount(r: ConsolidationReport): ComponentFixture<MemoryConsolidationReportDialog> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [MemoryConsolidationReportDialog] });
  const fixture = TestBed.createComponent(MemoryConsolidationReportDialog);
  fixture.componentRef.setInput('report', r);
  fixture.detectChanges();
  TestBed.tick();
  return fixture;
}

const host = (f: ComponentFixture<unknown>) => f.nativeElement as HTMLElement;
const statPairs = (f: ComponentFixture<unknown>) =>
  Array.from(host(f).querySelectorAll('.grid .qt-card')).map((c) => [
    c.querySelectorAll('p')[1].textContent,
    c.querySelectorAll('p')[0].textContent,
  ]);
const noteLine = (f: ComponentFixture<unknown>) =>
  (host(f).querySelector('.grid')!.nextElementSibling as HTMLElement).textContent;

describe('MemoryConsolidationReportDialog (P4.D276)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('titles a rehearsal with the name and a real report without one (v4 :115-118)', () => {
    expect(host(mount(report())).querySelector('h2')?.textContent).toBe(
      'Consolidation, Rehearsed — Pip',
    );
    expect(
      host(mount(report({ dryRun: false, characterName: null }))).querySelector('h2')?.textContent,
    ).toBe('Consolidation Report');
  });

  it('reads both descriptions (v4 :123-127)', () => {
    expect(host(mount(report())).querySelector('.qt-dialog-description')?.textContent).toBe(
      'Nothing has been written. This is what the Commonplace Book would do, were it let off the leash.',
    );
    expect(
      host(mount(report({ dryRun: false }))).querySelector('.qt-dialog-description')?.textContent,
    ).toBe('The Commonplace Book has been at work.');
  });

  it('writes the three skipped sentences on a warning line (v4 :12-16, :131-133)', () => {
    const cases: [NonNullable<ConsolidationReport['skippedReason']>, string][] = [
      ['character-not-found', 'The house has no record of that character.'],
      ['archived', 'That character has been archived, and the archive is not to be disturbed.'],
      [
        'no-llm',
        'No model is available to do the consolidating. Name a connection profile above, or configure the cheap LLM.',
      ],
    ];
    for (const [reason, text] of cases) {
      const f = mount(report({ skippedReason: reason }));
      const warn = host(f).querySelector('p.qt-text-small.qt-text-warning');
      expect(warn?.textContent).toBe(text);
      // A skip is not "the shelves are in good order".
      expect(host(f).textContent).not.toContain('Not a single cluster worth folding.');
    }
  });

  it('labels the four stats and reads their values (v4 :135-140)', () => {
    const f = mount(
      report({
        stats: stats({ candidates: 41, clustersFound: 6, clustersAttempted: 4, digestsCreated: 1, digestsUpdated: 1 }),
      }),
    );
    expect(statPairs(f)).toEqual([
      ['Notes considered', '41'],
      ['Clusters found', '6'],
      ['Clusters attempted', '4'],
      ['Digests proposed', '2'],
    ]);
  });

  it('falls Digests proposed back to the clusters\' digests when the sum is 0 or NaN (v4 :139)', () => {
    const digest = {
      id: 'd',
      action: 'create' as const,
      content: 'x',
      summary: 'x',
      keywords: [],
      importance: 0.5,
      reinforcedImportance: 0.5,
      reinforcementCount: 0,
      kind: 'semantic' as const,
      occurredAt: null,
      memberIds: ['m1'],
    };
    const clusters = [
      cluster({ digests: [digest, { ...digest, id: 'e' }] }),
      cluster({ memberIds: ['m3'], memberContents: ['y'], digests: [{ ...digest, id: 'f' }] }),
    ];
    expect(statPairs(mount(report({ clusters })))[3]).toEqual(['Digests proposed', '3']);
    const noUpdated = stats();
    delete (noUpdated as Partial<ConsolidationStats>).digestsUpdated;
    expect(statPairs(mount(report({ clusters, stats: noUpdated })))[3]).toEqual(['Digests proposed', '3']);
  });

  it('composes the note line with every clause, and alone as the duration (v4 :141-147)', () => {
    expect(
      noteLine(
        mount(
          report({
            stats: stats({
              clustersDeferred: 4,
              immatureSkipped: 2,
              noEmbeddingSkipped: 1,
              budgetExhausted: true,
              durationMs: 12_345,
            }),
          }),
        ),
      ),
    ).toBe(
      '4 further clusters wait for another run. 2 notes are too young to fold. 1 notes lack embeddings. The time budget ran out; the rest will keep. 12.3s.',
    );
    expect(noteLine(mount(report()))).toBe('0.0s.');
  });

  it('shows the empty sentence only with no clusters and no skip (v4 :149-153)', () => {
    expect(host(mount(report())).textContent).toContain(
      'Not a single cluster worth folding. The shelves are in good order.',
    );
    expect(host(mount(report({ clusters: [cluster()] }))).textContent).not.toContain(
      'Not a single cluster worth folding.',
    );
  });

  it('heads each cluster with its bucket and kind, and shows the status (v4 :18-37)', () => {
    const f = mount(
      report({
        clusters: [
          cluster(),
          cluster({
            bucket: { kind: 'other', subjectCharacterId: 'c2', subjectName: 'Orla' },
            clusterKind: 'episodic',
            status: 'written',
          }),
          cluster({ bucket: { kind: 'none', subjectCharacterId: null, subjectName: '' } }),
        ],
      }),
    );
    const headers = Array.from(host(f).querySelectorAll('.qt-card.space-y-3 p.qt-text-label')).map(
      (p) => p.textContent,
    );
    expect(headers).toEqual([
      'Cluster 1 · About themselves · semantic',
      'Cluster 2 · About Orla · episodic',
      'Cluster 3 · About no one in particular · semantic',
    ]);
    const statuses = Array.from(
      host(f).querySelectorAll('.qt-card.space-y-3 > div:first-child > span'),
    ).map((s) => s.textContent);
    expect(statuses).toEqual(['planned', 'written', 'planned']);
  });

  it('lists the members, the standalone in warning with its note (v4 :43-56)', () => {
    const f = mount(report({ clusters: [cluster({ keepStandalone: ['m2'] })] }));
    expect(host(f).querySelector('p.qt-text-label-xs')?.textContent).toBe('Notes to be folded (2)');
    const items = Array.from(host(f).querySelectorAll('ul.space-y-1 > li')) as HTMLElement[];
    expect(items.map((li) => li.textContent)).toEqual([
      '• Pip likes tea.',
      '• Pip drinks tea daily. (kept on the shelf as it is)',
    ]);
    expect(items[0].querySelector('span')!.classList.contains('qt-text-warning')).toBe(false);
    expect(items[1].querySelector('span')!.classList.contains('qt-text-warning')).toBe(true);
  });

  it('labels and describes the proposed digests (v4 :60-78)', () => {
    const base = {
      content: 'Pip is fond of tea.',
      summary: 's',
      keywords: [],
      reinforcedImportance: 0.5,
      reinforcementCount: 0,
      occurredAt: null,
    };
    const one = mount(
      report({
        clusters: [
          cluster({
            digests: [{ ...base, id: 'd1', action: 'create', kind: 'semantic', importance: 0.8, memberIds: ['a', 'b', 'c'] }],
          }),
        ],
      }),
    );
    expect(host(one).textContent).toContain('Proposed digest');
    expect(host(one).textContent).not.toContain('Proposed digests');
    expect(host(one).querySelector('li.border-l-2 p.qt-text-xs')?.textContent).toBe(
      'new digest · semantic · importance 80% · from 3 notes',
    );

    const two = mount(
      report({
        clusters: [
          cluster({
            digests: [
              { ...base, id: 'd1', action: 'update', kind: 'episodic', importance: 0.05, memberIds: ['a'] },
              { ...base, id: 'd2', action: 'create', kind: 'semantic', importance: 0.5, memberIds: ['b', 'c'] },
            ],
          }),
        ],
      }),
    );
    expect(host(two).textContent).toContain('Proposed digests');
    expect(host(two).querySelector('li.border-l-2 p.qt-text-xs')?.textContent).toBe(
      'revises an existing digest · episodic · importance 5% · from 1 note',
    );
  });

  it('lists contradictions with the note or the fallback, and the shortened ids (v4 :80-96)', () => {
    const f = mount(
      report({
        clusters: [
          cluster({
            contradictions: [
              { olderId: 'abcdefgh-1111', newerId: '12345678-2222', note: 'Pip gave up tea.' },
              { olderId: 'abcdefgh-1111', newerId: '12345678-2222', note: '' },
            ],
          }),
        ],
      }),
    );
    const label = host(f).querySelector('p.qt-text-label-xs.qt-text-warning');
    expect(label?.textContent).toBe('Contradictions noticed (2)');
    const items = Array.from(label!.nextElementSibling!.querySelectorAll('li')).map((li) => li.textContent);
    expect(items).toEqual([
      '• Pip gave up tea. (abcdefgh superseded by 12345678)',
      '• A newer note displaces an older one. (abcdefgh superseded by 12345678)',
    ]);
  });

  it('shows a cluster error on a destructive line (v4 :40)', () => {
    const f = mount(report({ clusters: [cluster({ status: 'failed', error: 'The model sulked.' })] }));
    expect(host(f).querySelector('.qt-card.space-y-3 p.qt-text-destructive')?.textContent).toBe(
      'The model sulked.',
    );
  });

  it('emits close from both the header icon and the footer button (v4 :119, :161)', () => {
    const f = mount(report());
    let closed = 0;
    f.componentInstance.close.subscribe(() => (closed += 1));
    (host(f).querySelector('button[aria-label="Close"]') as HTMLButtonElement).click();
    (
      Array.from(host(f).querySelectorAll('button')).find(
        (b) => b.textContent?.trim() === 'Close',
      ) as HTMLButtonElement
    ).click();
    expect(closed).toBe(2);
  });

  it('sits under document.body, z-[100], with no role="dialog" (R-H)', () => {
    const f = mount(report());
    expect(host(f).parentElement).toBe(document.body);
    const overlay = host(f).querySelector('.qt-dialog-overlay') as HTMLElement;
    expect(overlay.classList.contains('z-[100]')).toBe(true);
    expect(host(f).querySelector('[role="dialog"]')).toBeNull();
  });
});
