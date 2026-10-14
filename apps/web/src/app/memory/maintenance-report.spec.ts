import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import type {
  ConsolidationClusterReport,
  ConsolidationReport,
  ConsolidationStats,
  MaintenanceReport,
  PhaseSkipReason,
} from '../core/core-contract';
import { MaintenanceReportView, MaintenanceReportsTable, PHASE_LABELS } from './maintenance-report';

/**
 * P4.D300 — the maintenance report, every string from v4
 * `components/memory/maintenance-report.tsx` (`b045dd986`, `48790fb73`) at
 * `83334b9a5`. v4 shipped no test for it; these rows are the lane's own. The
 * cluster-block rows MOVED here from the deleted
 * `memory-consolidation-report-dialog.spec.ts` with the renderer (L-14).
 */

/** The `83334b9a5` stats shape (C3 §1: `demotedCandidates` in, `mirrorFilesWritten` out). */
function stats(over: Record<string, unknown> = {}): ConsolidationStats {
  return {
    rowsLoaded: 0,
    candidates: 0,
    demotedCandidates: 0,
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
    budgetExhausted: false,
    durationMs: 0,
    ...over,
  } as unknown as ConsolidationStats;
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

function consolidation(over: Partial<ConsolidationReport> = {}): ConsolidationReport {
  return {
    characterId: 'c1',
    characterName: 'Pip',
    dryRun: true,
    trigger: 'manual',
    startedAt: '2026-10-13T00:00:00.000Z',
    finishedAt: '2026-10-13T00:00:01.000Z',
    settings: {} as ConsolidationReport['settings'],
    clusters: [],
    stats: stats(),
    ...over,
  };
}

function report(over: Partial<MaintenanceReport> = {}): MaintenanceReport {
  return {
    characterId: 'c1',
    characterName: 'Pip',
    reason: 'manual',
    dryRun: true,
    startedAt: '2026-10-13T00:00:00.000Z',
    finishedAt: '2026-10-13T00:00:01.000Z',
    durationMs: 1000,
    phases: [],
    ...over,
  };
}

function mount(r: MaintenanceReport): ComponentFixture<MaintenanceReportView> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [MaintenanceReportView] });
  const fixture = TestBed.createComponent(MaintenanceReportView);
  fixture.componentRef.setInput('report', r);
  fixture.detectChanges();
  return fixture;
}

function mountTable(reports: MaintenanceReport[]): ComponentFixture<MaintenanceReportsTable> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [MaintenanceReportsTable] });
  const fixture = TestBed.createComponent(MaintenanceReportsTable);
  fixture.componentRef.setInput('reports', reports);
  fixture.detectChanges();
  return fixture;
}

const host = (f: ComponentFixture<unknown>) => f.nativeElement as HTMLElement;
const text = (f: ComponentFixture<unknown>) => (host(f).textContent ?? '').replace(/\s+/g, ' ').trim();
/** The section headed `label`. */
const section = (f: ComponentFixture<unknown>, label: string) =>
  Array.from(host(f).querySelectorAll('section')).find(
    (s) => s.querySelector('h3')?.textContent === label,
  ) as HTMLElement | undefined;
const statPairs = (el: HTMLElement) =>
  Array.from(el.querySelectorAll('.grid .qt-card')).map((c) => [
    c.querySelectorAll('p')[1].textContent,
    c.querySelectorAll('p')[0].textContent,
  ]);

describe('MaintenanceReportView (P4.D300, v4 maintenance-report.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('PHASE_LABELS reads v4 :12-18', () => {
    expect(PHASE_LABELS).toEqual({
      'fold-near-duplicates': 'Fold near-duplicates',
      consolidate: 'Consolidate',
      demote: 'Demote over the cap',
      retire: 'Retire old archive notes',
      mirror: 'Refresh the Commonplace files',
    });
  });

  it('a skipped report shows only its sentence on a warning line (REPORT_SKIP_TEXT)', () => {
    for (const [reason, sentence] of [
      ['character-not-found', 'The character could not be found.'],
      ['archived', 'The character rests in the archive; nothing was done.'],
    ] as const) {
      const f = mount(report({ skippedReason: reason, phases: [{ phase: 'demote', status: 'ran', durationMs: 5 }] }));
      expect(host(f).querySelector('p.qt-text-small.qt-text-warning')?.textContent).toBe(sentence);
      expect(host(f).querySelector('ul')).toBeNull();
    }
  });

  it('the phase list: ran → rehearsed|ran · {s}s; skipped → every SKIP_TEXT sentence, muted', () => {
    const reasons: [PhaseSkipReason, string][] = [
      ['not-requested', 'not asked for'],
      ['maintenance-disabled', 'automatic maintenance is off'],
      ['consolidation-disabled', 'consolidation is off'],
      ['phase-disabled', 'switched off above'],
      ['no-retention-window', 'no archive retention window is set'],
      ['nothing-touched', 'nothing changed that it mirrors'],
    ];
    for (const [reason, sentence] of reasons) {
      const f = mount(report({ phases: [{ phase: 'retire', status: 'skipped', skippedBecause: reason, durationMs: 0 }] }));
      const li = host(f).querySelector('ul li') as HTMLElement;
      expect(li.querySelectorAll('span')[0].textContent).toBe('Retire old archive notes');
      expect(li.querySelectorAll('span')[0].classList.contains('qt-text-muted')).toBe(true);
      expect(li.querySelectorAll('span')[1].textContent).toBe(`skipped — ${sentence}`);
    }
    // An absent skippedBecause reads as not asked for.
    const bare = mount(report({ phases: [{ phase: 'mirror', status: 'skipped', durationMs: 0 }] }));
    expect(host(bare).querySelector('ul li span:last-child')?.textContent).toBe('skipped — not asked for');

    const ran = mount(report({ phases: [{ phase: 'demote', status: 'ran', durationMs: 1234 }] }));
    const li = host(ran).querySelector('ul li') as HTMLElement;
    expect(li.querySelectorAll('span')[0].classList.contains('qt-text-muted')).toBe(false);
    expect(li.querySelectorAll('span')[1].textContent).toBe('rehearsed · 1.2s');
    const real = mount(report({ dryRun: false, phases: [{ phase: 'demote', status: 'ran', durationMs: 50 }] }));
    expect(host(real).querySelector('ul li span:last-child')?.textContent).toBe('ran · 0.1s');
  });

  it('fold: the three stats (Would fold | Folded) and the tail pieces', () => {
    const fold = { groupsDeferred: 0, eligible: 40, deferred: 0, groups: [{ survivorId: 's', absorbedIds: ['a'], minCosine: 0.9 }], folded: 1, groupsFailed: 0 };
    const f = mount(report({ fold }));
    const s = section(f, 'Fold near-duplicates')!;
    expect(statPairs(s)).toEqual([['Notes compared', '40'], ['Sets found', '1'], ['Would fold', '1']]);
    expect(s.querySelector(':scope > p.qt-text-xs')).toBeNull();
    const real = mount(report({ dryRun: false, fold: { ...fold, deferred: 3, groupsDeferred: 2, groupsFailed: 1 } }));
    const rs = section(real, 'Fold near-duplicates')!;
    expect(statPairs(rs)[2]).toEqual(['Folded', '1']);
    expect(rs.querySelector(':scope > p.qt-text-xs')?.textContent).toBe(
      '3 notes wait for a later comparison. 2 sets wait for the next run. 1 sets could not be folded and were left as they were.',
    );
  });

  it('demote: Cap / Counted / Exempt (48790fb73) / Would archive, toLocaleString, both tails', () => {
    const demote = { cap: 2000, hotTotal: 2400, demotable: 2100, protected: 1300, excess: 100, demoted: 100, demotedIds: [], highestScoreDemoted: 0.2 };
    const f = mount(report({ demote }));
    const s = section(f, 'Demote over the cap')!;
    expect(statPairs(s)).toEqual([
      ['Cap', (2000).toLocaleString()],
      ['Counted', (2100).toLocaleString()],
      ['Exempt', (1300).toLocaleString()],
      ['Would archive', '100'],
    ]);
    expect(s.querySelector(':scope > p.qt-text-xs')?.textContent).toBe(
      '100 counted notes over the cap; the least protected go to the archive first. Hand-written notes and digests are never counted and never moved.',
    );
    const within = mount(report({ dryRun: false, demote: { ...demote, excess: 0, demoted: 0 } }));
    const ws = section(within, 'Demote over the cap')!;
    expect(statPairs(ws)[3]).toEqual(['Archived', '0']);
    expect(ws.querySelector(':scope > p.qt-text-xs')?.textContent).toBe(
      'Within the cap. Nothing need go to the archive. Hand-written notes and digests are never counted and never moved.',
    );
  });

  it('retire: zero vs {n} note{s} {would be|were} let go', () => {
    const line = (r: MaintenanceReport) => section(mount(r), 'Retire old archive notes')?.querySelector('p')?.textContent;
    expect(line(report({ retire: { retentionDays: 90, retired: 0, retiredIds: [] } }))).toBe(
      'No superseded archive note is older than 90 days.',
    );
    expect(line(report({ retire: { retentionDays: 90, retired: 1, retiredIds: [] } }))).toBe(
      '1 superseded archive note older than 90 days would be let go.',
    );
    expect(line(report({ dryRun: false, retire: { retentionDays: 30, retired: 2, retiredIds: [] } }))).toBe(
      '2 superseded archive notes older than 30 days were let go.',
    );
  });

  it('mirror: only with subjects; the three verbs incl. the deferred-to-commit one (R-M)', () => {
    const line = (r: MaintenanceReport) => section(mount(r), 'Refresh the Commonplace files')?.querySelector('p')?.textContent;
    expect(line(report({ mirror: { subjects: [], filesWritten: 0, deferredToCommit: false } }))).toBeUndefined();
    expect(line(report({ mirror: { subjects: ['a'], filesWritten: 0, deferredToCommit: true } }))).toBe(
      '1 subject file would be refreshed.',
    );
    expect(line(report({ dryRun: false, mirror: { subjects: ['a', 'b'], filesWritten: 0, deferredToCommit: true } }))).toBe(
      '2 subject files refreshed once the run is committed.',
    );
    expect(line(report({ dryRun: false, mirror: { subjects: ['a', 'b'], filesWritten: 2, deferredToCommit: false } }))).toBe(
      '2 subject files refreshed.',
    );
  });

  it('consolidation: Clusters worth folding reads clustersQualified, not clustersFound (48790fb73)', () => {
    const f = mount(
      report({
        consolidation: consolidation({
          stats: stats({ candidates: 41, clustersFound: 9, clustersQualified: 6, clustersAttempted: 4, digestsCreated: 1, digestsUpdated: 1 }),
        }),
      }),
    );
    expect(statPairs(section(f, 'Consolidate')!)).toEqual([
      ['Notes considered', '41'],
      ['Clusters worth folding', '6'],
      ['Clusters attempted', '4'],
      ['Digests proposed', '2'],
    ]);
  });

  it('consolidation: the no-llm sentence on a warning line, and no empty sentence with it', () => {
    const f = mount(report({ consolidation: consolidation({ skippedReason: 'no-llm' }) }));
    const s = section(f, 'Consolidate')!;
    expect(s.querySelector('p.qt-text-small.qt-text-warning')?.textContent).toBe(
      'No model is available to do the consolidating. Name a connection profile, or configure the cheap LLM.',
    );
    expect(s.textContent).not.toContain('Not a single cluster worth folding.');
    const empty = mount(report({ consolidation: consolidation() }));
    expect(section(empty, 'Consolidate')!.textContent).toContain(
      'Not a single cluster worth folding. The shelves are in good order.',
    );
  });

  it('consolidation: the note line with every clause, and alone as the duration', () => {
    const note = (r: MaintenanceReport) =>
      (section(mount(r), 'Consolidate')!.querySelector('.grid')!.nextElementSibling as HTMLElement).textContent;
    expect(
      note(report({ consolidation: consolidation({ stats: stats({ clustersDeferred: 4, immatureSkipped: 2, noEmbeddingSkipped: 1, budgetExhausted: true, durationMs: 12_345 }) }) })),
    ).toBe(
      '4 further clusters wait for another run. 2 notes are too young to fold. 1 notes lack embeddings. The time budget ran out; the rest will keep. 12.3s.',
    );
    expect(note(report({ consolidation: consolidation() }))).toBe('0.0s.');
  });

  it('a phase block renders only when the report carries it', () => {
    const f = mount(report({ phases: [{ phase: 'demote', status: 'ran', durationMs: 1 }] }));
    expect(host(f).querySelectorAll('section').length).toBe(0);
  });
});

/** The cluster block — moved with the renderer (L-14), rows transcribed from P4.D276's spec. */
describe('MaintenanceReportView — the cluster block (moved from the consolidation report dialog)', () => {
  afterEach(() => TestBed.resetTestingModule());
  const withClusters = (clusters: ConsolidationClusterReport[], over: Partial<ConsolidationReport> = {}) =>
    mount(report({ consolidation: consolidation({ clusters, ...over }) }));

  it('falls Digests proposed back to the clusters\' digests when the sum is 0', () => {
    const digest = { id: 'd', action: 'create' as const, content: 'x', summary: 'x', keywords: [], importance: 0.5, reinforcedImportance: 0.5, reinforcementCount: 0, kind: 'semantic' as const, occurredAt: null, memberIds: ['m1'] };
    const f = withClusters([
      cluster({ digests: [digest, { ...digest, id: 'e' }] }),
      cluster({ memberIds: ['m3'], memberContents: ['y'], digests: [{ ...digest, id: 'f' }] }),
    ]);
    expect(statPairs(section(f, 'Consolidate')!)[3]).toEqual(['Digests proposed', '3']);
  });

  it('heads each cluster with its bucket and kind, and shows the status', () => {
    const f = withClusters([
      cluster(),
      cluster({ bucket: { kind: 'other', subjectCharacterId: 'c2', subjectName: 'Orla' }, clusterKind: 'episodic', status: 'written' }),
      cluster({ bucket: { kind: 'none', subjectCharacterId: null, subjectName: '' } }),
    ]);
    const headers = Array.from(host(f).querySelectorAll('.qt-card.space-y-3 p.qt-text-label')).map((p) => p.textContent);
    expect(headers).toEqual([
      'Cluster 1 · About themselves · semantic',
      'Cluster 2 · About Orla · episodic',
      'Cluster 3 · About no one in particular · semantic',
    ]);
    const statuses = Array.from(host(f).querySelectorAll('.qt-card.space-y-3 > div:first-child > span')).map((s) => s.textContent);
    expect(statuses).toEqual(['planned', 'written', 'planned']);
  });

  it('lists the members, the standalone in warning with its note', () => {
    const f = withClusters([cluster({ keepStandalone: ['m2'] })]);
    const card = host(f).querySelector('.qt-card.space-y-3') as HTMLElement;
    expect(card.querySelector('p.qt-text-label-xs')?.textContent).toBe('Notes to be folded (2)');
    const items = Array.from(card.querySelectorAll('ul.space-y-1 > li')) as HTMLElement[];
    expect(items.map((li) => li.textContent)).toEqual([
      '• Pip likes tea.',
      '• Pip drinks tea daily. (kept on the shelf as it is)',
    ]);
    expect(items[0].querySelector('span')!.classList.contains('qt-text-warning')).toBe(false);
    expect(items[1].querySelector('span')!.classList.contains('qt-text-warning')).toBe(true);
  });

  it('labels and describes the proposed digests', () => {
    const base = { content: 'Pip is fond of tea.', summary: 's', keywords: [], reinforcedImportance: 0.5, reinforcementCount: 0, occurredAt: null };
    const one = withClusters([cluster({ digests: [{ ...base, id: 'd1', action: 'create', kind: 'semantic', importance: 0.8, memberIds: ['a', 'b', 'c'] }] })]);
    expect(text(one)).toContain('Proposed digest');
    expect(text(one)).not.toContain('Proposed digests');
    expect(host(one).querySelector('li.border-l-2 p.qt-text-xs')?.textContent).toBe('new digest · semantic · importance 80% · from 3 notes');
    const two = withClusters([
      cluster({
        digests: [
          { ...base, id: 'd1', action: 'update', kind: 'episodic', importance: 0.05, memberIds: ['a'] },
          { ...base, id: 'd2', action: 'create', kind: 'semantic', importance: 0.5, memberIds: ['b', 'c'] },
        ],
      }),
    ]);
    expect(text(two)).toContain('Proposed digests');
    expect(host(two).querySelector('li.border-l-2 p.qt-text-xs')?.textContent).toBe('revises an existing digest · episodic · importance 5% · from 1 note');
  });

  it('lists contradictions with the note or the fallback, and the shortened ids', () => {
    const f = withClusters([
      cluster({
        contradictions: [
          { olderId: 'abcdefgh-1111', newerId: '12345678-2222', note: 'Pip gave up tea.' },
          { olderId: 'abcdefgh-1111', newerId: '12345678-2222', note: '' },
        ],
      }),
    ]);
    const label = host(f).querySelector('p.qt-text-label-xs.qt-text-warning');
    expect(label?.textContent).toBe('Contradictions noticed (2)');
    expect(Array.from(label!.nextElementSibling!.querySelectorAll('li')).map((li) => li.textContent)).toEqual([
      '• Pip gave up tea. (abcdefgh superseded by 12345678)',
      '• A newer note displaces an older one. (abcdefgh superseded by 12345678)',
    ]);
  });

  it('shows a cluster error on a destructive line', () => {
    const f = withClusters([cluster({ status: 'failed', error: 'The model sulked.' })]);
    expect(host(f).querySelector('.qt-card.space-y-3 p.qt-text-destructive')?.textContent).toBe('The model sulked.');
  });
});

describe('MaintenanceReportsTable (P4.D300, v4 maintenance-report.tsx:238-271)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('the empty sentence when every report was skipped (or none came)', () => {
    expect(text(mountTable([]))).toBe('No character has any memories to tend.');
    expect(text(mountTable([report({ skippedReason: 'archived' })]))).toBe('No character has any memories to tend.');
  });

  it('the five columns, the title, one row per unskipped report, — for a missing block, Unnamed, red on excess', () => {
    const f = mountTable([
      report({
        characterId: 'a',
        characterName: 'Mnemo',
        fold: { groupsDeferred: 0, eligible: 40, deferred: 0, groups: [], folded: 3, groupsFailed: 0 },
        demote: { cap: 2000, hotTotal: 2400, demotable: 2100, protected: 300, excess: 100, demoted: 100, demotedIds: [], highestScoreDemoted: null },
        retire: { retentionDays: 90, retired: 4, retiredIds: [] },
      }),
      report({ characterId: 'b', characterName: null }),
      report({ characterId: 'c', characterName: 'Gone', skippedReason: 'character-not-found' }),
      report({
        characterId: 'd',
        characterName: 'Orla',
        demote: { cap: 2000, hotTotal: 10, demotable: 10, protected: 0, excess: 0, demoted: 0, demotedIds: [], highestScoreDemoted: null },
      }),
    ]);
    const heads = Array.from(host(f).querySelectorAll('th')).map((th) => th.textContent);
    expect(heads).toEqual(['Character', 'Would fold', 'Counted / cap', 'Would archive', 'Would let go']);
    expect(host(f).querySelectorAll('th')[2].getAttribute('title')).toBe('Counted notes against the cap');
    const rows = Array.from(host(f).querySelectorAll('tbody tr')).map((tr) =>
      Array.from(tr.querySelectorAll('td')).map((td) => td.textContent),
    );
    expect(rows).toEqual([
      ['Mnemo', '3', `${(2100).toLocaleString()} / ${(2000).toLocaleString()}`, '100', '4'],
      ['Unnamed', '—', '—', '—', '—'],
      ['Orla', '—', `10 / ${(2000).toLocaleString()}`, '0', '—'],
    ]);
    const counted = Array.from(host(f).querySelectorAll('tbody tr')).map((tr) =>
      tr.querySelectorAll('td')[2].classList.contains('qt-text-destructive'),
    );
    expect(counted).toEqual([true, false, false]);
  });
});
