import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { CoreDispatchError } from '../../../core/core-contract';
import { ToastService } from '../../../ui/toast.service';
import { MemoryMaintenanceCard } from './memory-maintenance-card';

/**
 * P4.D300 — the Maintenance card, v4 `components/tools/memory-maintenance-card.tsx`
 * (`b045dd986`, `48790fb73`) at `83334b9a5`. The first describe transcribes
 * v4's four jest cases (`__tests__/unit/components/tools/
 * memory-maintenance-card.test.tsx:97`, `:112`, `:123`, `:134`) case for case,
 * with v4's own fixtures; the rest are the lane's own.
 */

const CHAR = '550e8400-e29b-41d4-a716-446655440001';

const MAINTENANCE = {
  enabled: true,
  perCharacterCap: 2000,
  perCharacterCapOverrides: {} as Record<string, number>,
  phases: { foldNearDuplicates: true, demote: true, retire: true },
};
const CONSOLIDATION = {
  enabled: false,
  connectionProfileId: 'profile-1',
  clusterThreshold: 0.72,
  tfidfClusterThreshold: 0.3,
  minClusterSize: 3,
  maxClusterSize: 30,
  matureAfterDays: 7,
  maxClustersPerRun: 40,
  watermark: 150,
  coldRetentionDays: 90 as number | null,
};
const LEDGER = [
  {
    characterId: CHAR,
    lastRun: { demote: { at: '2026-10-13T10:00:00.000Z', reason: 'watermark' } },
    lastSummary: {
      characterId: CHAR,
      reason: 'watermark',
      dryRun: false,
      finishedAt: '2026-10-13T10:00:00.000Z',
      durationMs: 50,
      phases: [],
      demote: { cap: 2000, hotTotal: 2105, demotable: 2100, protected: 5, excess: 100, demoted: 100, highestScoreDemoted: 0.2 },
    },
    lastDemotion: null,
  },
];

interface Req {
  type: string;
  config?: Record<string, unknown>;
  [k: string]: unknown;
}
type Handler = (req: Req) => Record<string, unknown> | Promise<Record<string, unknown>>;

/** v4's `route` (`:51-84`), as dispatch handlers. */
function handlers(over: Partial<Record<string, Handler>> = {}): Record<string, Handler> {
  return {
    memoryMaintenanceConfigGet: () => ({ success: true, settings: MAINTENANCE }),
    memoryMaintenanceConfigSet: (req) => ({ success: true, settings: { ...MAINTENANCE, ...req.config } }),
    memoryConsolidationConfigGet: () => ({ success: true, settings: CONSOLIDATION }),
    memoryConsolidationConfigSet: (req) => ({ success: true, settings: { ...CONSOLIDATION, ...req.config } }),
    memoryCharacterCounts: () => ({ characters: [{ id: CHAR, name: 'Ariadne', demotable: 2100, protected: 5 }] }),
    memoryMaintenanceStatus: () => ({ success: true, ledger: LEDGER }),
    ...over,
  };
}

function render(over: Partial<Record<string, Handler>> = {}, seen: Req[] = []): ComponentFixture<MemoryMaintenanceCard> {
  const h = handlers(over);
  const client: Partial<CoreClient> = {
    dispatchData: (async (req: Req) => {
      seen.push(req);
      const fn = h[req.type];
      return fn ? fn(req) : {};
    }) as CoreClient['dispatchData'],
    dispatchExpect: (async () => ({ type: 'connectionProfiles', data: { profiles: [] } })) as unknown as CoreClient['dispatchExpect'],
  };
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [MemoryMaintenanceCard],
    providers: [
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      { provide: CoreClient, useValue: client },
    ],
  });
  const fixture = TestBed.createComponent(MemoryMaintenanceCard);
  fixture.detectChanges();
  return fixture;
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 10; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function rendered(over: Partial<Record<string, Handler>> = {}, seen: Req[] = []): Promise<ComponentFixture<MemoryMaintenanceCard>> {
  const f = render(over, seen);
  await settle(f);
  return f;
}

const host = (f: ComponentFixture<unknown>) => f.nativeElement as HTMLElement;
const text = (f: ComponentFixture<unknown>) => (host(f).textContent ?? '').replace(/\s+/g, ' ');
/** The checkbox whose label text starts with `label`. */
const box = (f: ComponentFixture<unknown>, label: string) =>
  Array.from(host(f).querySelectorAll('label')).find((l) =>
    (l.querySelector('span > span')?.textContent ?? '').startsWith(label),
  )!.querySelector('input') as HTMLInputElement;
const posts = (seen: Req[], type: string) => seen.filter((r) => r.type === type).map((r) => r.config);
const errorLine = (f: ComponentFixture<unknown>) =>
  Array.from(host(f).querySelectorAll(':scope > div > p.qt-text-small.qt-text-destructive')).map((p) => p.textContent);
const toasts = () =>
  TestBed.inject(ToastService)
    .toasts()
    .map((t) => `${t.type}:${t.message}`);

async function blurWith(f: ComponentFixture<unknown>, el: HTMLInputElement, value: string): Promise<void> {
  el.value = value;
  el.dispatchEvent(new Event('input'));
  await settle(f); // a browser renders between typing and leaving the field
  el.dispatchEvent(new Event('blur'));
  await settle(f);
}

async function click(f: ComponentFixture<unknown>, el: HTMLElement): Promise<void> {
  el.click();
  await settle(f);
}

describe('MemoryMaintenanceCard — v4 memory-maintenance-card.test.tsx, transcribed', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('saves one phase toggle as a phases patch, and consolidation through its own document (:97)', async () => {
    const seen: Req[] = [];
    const f = await rendered({}, seen);
    await click(f, box(f, 'Demote over the cap'));
    expect(posts(seen, 'memoryMaintenanceConfigSet')).toEqual([{ phases: { foldNearDuplicates: true, demote: false, retire: true } }]);
    await click(f, box(f, 'Consolidate'));
    expect(posts(seen, 'memoryConsolidationConfigSet')).toEqual([{ enabled: true }]);
  });

  it('clears the archive retention with a blank, as null (:112)', async () => {
    const seen: Req[] = [];
    const f = await rendered({}, seen);
    const input = host(f).querySelector('#maintenance-retention') as HTMLInputElement;
    expect(input.value).toBe('90');
    await blurWith(f, input, '');
    expect(posts(seen, 'memoryConsolidationConfigSet')).toEqual([{ coldRetentionDays: null }]);
  });

  it('saves a typed per-character cap on blur (:123, review of PR 98)', async () => {
    const seen: Req[] = [];
    const f = await rendered({}, seen);
    const input = host(f).querySelector('#perCharacterCap') as HTMLInputElement;
    await blurWith(f, input, '3000');
    expect(posts(seen, 'memoryMaintenanceConfigSet')).toEqual([{ perCharacterCap: 3000 }]);
    expect(input.value).toBe('3000');
  });

  it('shows the ledger by character name with the last run’s outcome (:134)', async () => {
    const f = await rendered();
    expect(text(f)).toMatch(/100 archived \(2100\/2000 counted\)/);
    const ledger = host(f).querySelector('qt-memory-maintenance-ledger') as HTMLElement;
    expect(ledger.querySelector('p.qt-text-label')?.textContent).toBe('Ariadne');
  });
});

describe('MemoryMaintenanceCard — the lane\'s own rows (P4.D300)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('the loading line, then the sections in v4\'s order', async () => {
    const f = render();
    expect(text(f).trim()).toBe('Loading maintenance settings…');
    await settle(f);
    expect(Array.from(host(f).querySelectorAll('h3')).map((h) => h.textContent)).toEqual([
      'Automatic rounds',
      'The cap',
      'Run now',
      'The ledger',
    ]);
    expect(text(f)).toContain(
      'One round of the household staff tends each character’s memories, in a fixed order: repetitions are folded together, related notes are consolidated into digests, anything over the cap goes to the archive, and superseded archive notes past their retention are let go. Archived notes are not lost: they stay on the shelf, still guard against repeats, and return to active duty if the world asks for them again.',
    );
    expect(text(f)).toContain('Only extracted notes in active service count. Maintenance calls when a character reaches nine-tenths of it.');
    expect(text(f)).toContain('One character or every one, any phases, with a dry run first.');
    expect(text(f)).toContain(
      'The Commonplace files in each vault are refreshed after every consolidation. Hand-written notes and digests are never folded, demoted or let go by any of this.',
    );
    expect(text(f)).toContain('▸ Consolidation particulars');
  });

  it('the five switches in v4\'s order, with v4\'s hints (Consolidate\'s curly ’)', async () => {
    const f = await rendered();
    const rows = Array.from(host(f).querySelectorAll('section label.qt-body')).map((l) => [
      l.querySelector('span > span')?.textContent,
      l.querySelector('span > span.qt-text-xs')?.textContent,
    ]);
    expect(rows).toEqual([
      ['Enable automatic maintenance', 'A daily round, and a visit whenever a character nears the cap. Off by default — read the limits below first.'],
      ['Fold near-duplicates', 'Notes that say the same thing about the same occasion are folded into the sturdiest of them, stray details kept as footnotes. No model is consulted.'],
      ['Consolidate', 'Once a day, and whenever a character’s unconsidered notes pass the watermark, a model folds clusters of related notes into digests and the originals go to the archive. Has its own switch, as it costs model calls.'],
      ['Demote over the cap', 'Past the cap, the least protected notes — old, unreinforced, unlinked, unconsulted — go to the archive first. Nothing is destroyed.'],
      ['Retire old archive notes', 'Superseded archive notes older than the retention window below are let go for good.'],
    ]);
  });

  it('disable rules: automatic maintenance OFF disables fold / demote / retire, NOT Consolidate', async () => {
    const f = await rendered({ memoryMaintenanceConfigGet: () => ({ success: true, settings: { ...MAINTENANCE, enabled: false } }) });
    expect(box(f, 'Enable automatic maintenance').disabled).toBe(false);
    expect(box(f, 'Fold near-duplicates').disabled).toBe(true);
    expect(box(f, 'Demote over the cap').disabled).toBe(true);
    expect(box(f, 'Retire old archive notes').disabled).toBe(true);
    expect(box(f, 'Consolidate').disabled).toBe(false);
    expect((host(f).querySelector('#maintenance-retention') as HTMLInputElement).disabled).toBe(false);
  });

  it('disable rules: EITHER document saving disables every switch and the retention', async () => {
    let release!: (v: Record<string, unknown>) => void;
    const f = await rendered({ memoryConsolidationConfigSet: () => new Promise((r) => (release = r)) });
    box(f, 'Consolidate').click();
    await settle(f);
    for (const label of ['Enable automatic maintenance', 'Fold near-duplicates', 'Consolidate', 'Demote over the cap', 'Retire old archive notes']) {
      expect(box(f, label).disabled).toBe(true);
    }
    expect((host(f).querySelector('#maintenance-retention') as HTMLInputElement).disabled).toBe(true);
    release({ success: true, settings: { ...CONSOLIDATION, enabled: true } });
    await settle(f);
    expect(box(f, 'Fold near-duplicates').disabled).toBe(false);
  });

  it('Enable automatic maintenance posts { enabled } alone; the toast is the document\'s', async () => {
    const seen: Req[] = [];
    const f = await rendered({}, seen);
    await click(f, box(f, 'Enable automatic maintenance'));
    expect(posts(seen, 'memoryMaintenanceConfigSet')).toEqual([{ enabled: false }]);
    expect(toasts()).toEqual(['success:Maintenance settings saved']);
  });

  it('a FAILED save snaps each switch back (L-3)', async () => {
    const refuse = () => {
      throw new CoreDispatchError({ kind: 'bad-request', message: 'Refused' });
    };
    const f = await rendered({ memoryMaintenanceConfigSet: refuse, memoryConsolidationConfigSet: refuse });
    for (const [label, stored] of [
      ['Enable automatic maintenance', true],
      ['Fold near-duplicates', true],
      ['Demote over the cap', true],
      ['Retire old archive notes', true],
      ['Consolidate', false],
    ] as const) {
      await click(f, box(f, label));
      expect(box(f, label).checked).toBe(stored);
    }
    expect(errorLine(f)).toEqual(['Refused']);
  });

  it('retention: unchanged → no save; 0 / 1.5 → the sentence; a new whole number saves', async () => {
    const seen: Req[] = [];
    const f = await rendered({}, seen);
    const input = () => host(f).querySelector('#maintenance-retention') as HTMLInputElement;
    expect(input().getAttribute('placeholder')).toBe('never');
    await blurWith(f, input(), '90');
    expect(posts(seen, 'memoryConsolidationConfigSet')).toEqual([]);
    for (const bad of ['0', '1.5']) {
      await blurWith(f, input(), bad);
      expect(errorLine(f)).toEqual(['Archive retention must be a whole number of days, or blank for never']);
      expect(input().value).toBe('90');
    }
    await blurWith(f, input(), '30');
    expect(posts(seen, 'memoryConsolidationConfigSet')).toEqual([{ coldRetentionDays: 30 }]);
  });

  it('retention blank when already never → no save', async () => {
    const seen: Req[] = [];
    const f = await rendered({ memoryConsolidationConfigGet: () => ({ success: true, settings: { ...CONSOLIDATION, coldRetentionDays: null } }) }, seen);
    const input = host(f).querySelector('#maintenance-retention') as HTMLInputElement;
    expect(input.value).toBe('');
    await blurWith(f, input, '');
    expect(posts(seen, 'memoryConsolidationConfigSet')).toEqual([]);
  });

  it('the cap: out of range → the toLocaleString sentence, the input snaps to the stored value; unchanged → no save', async () => {
    const seen: Req[] = [];
    const f = await rendered({}, seen);
    const input = host(f).querySelector('#perCharacterCap') as HTMLInputElement;
    expect([input.getAttribute('min'), input.getAttribute('max'), input.getAttribute('step')]).toEqual(['100', '100000', '100']);
    for (const bad of ['99', '100001', '']) {
      await blurWith(f, input, bad);
      expect(errorLine(f)).toEqual([`The per-character cap must be between ${(100).toLocaleString()} and ${(100000).toLocaleString()}`]);
      expect(input.value).toBe('2000');
    }
    await blurWith(f, input, '2000.7');
    expect(posts(seen, 'memoryMaintenanceConfigSet')).toEqual([]);
  });

  it('overrides: the toggle with the active count, the help, `N counted · M exempt`, the destructive class', async () => {
    const f = await rendered({
      memoryMaintenanceConfigGet: () => ({ success: true, settings: { ...MAINTENANCE, perCharacterCapOverrides: { b: 5000 } } }),
      memoryCharacterCounts: () => ({
        characters: [
          { id: 'a', name: 'Mnemo', memoryCount: 2600, demotable: 2500, protected: 100 },
          { id: 'b', name: 'Orla', memoryCount: 2600, demotable: 2600 },
          { id: 'c', name: 'Pip', memoryCount: 12 },
        ],
      }),
    });
    const toggle = Array.from(host(f).querySelectorAll('button')).find((b) => b.textContent?.includes('Per-character overrides'))!;
    expect(toggle.textContent?.trim()).toBe('▸ Per-character overrides (1 active)');
    await click(f, toggle);
    expect(toggle.textContent?.trim()).toBe('▾ Per-character overrides (1 active)');
    expect(text(f)).toContain(`Set a different cap for individual characters. Leave blank to fall back to the global cap (${(2000).toLocaleString()}).`);
    const rows = Array.from(host(f).querySelectorAll('.mt-3 .space-y-1 > div')).map((r) => [
      r.querySelector('.qt-body')!.textContent,
      r.querySelector('.qt-body span:last-child')!.getAttribute('class'),
      (r.querySelector('input') as HTMLInputElement).value,
      r.querySelector('input')!.getAttribute('aria-label'),
      r.querySelector('input')!.getAttribute('placeholder'),
    ]);
    expect(rows).toEqual([
      [`Mnemo (${(2500).toLocaleString()} counted · 100 exempt)`, 'qt-text-destructive qt-text-small', '', 'Cap override for Mnemo', '2000'],
      [`Orla (${(2600).toLocaleString()} counted)`, 'qt-text-muted qt-text-small', '5000', 'Cap override for Orla', '2000'],
      ['Pip (12 counted)', 'qt-text-muted qt-text-small', '', 'Cap override for Pip', '2000'],
    ]);
  });

  it('overrides: the WHOLE map on a save; blank removes; blank on an absent one is a no-op; the slice(0, 8) refusal keeps the draft', async () => {
    const seen: Req[] = [];
    const f = await rendered(
      {
        memoryMaintenanceConfigGet: () => ({ success: true, settings: { ...MAINTENANCE, perCharacterCapOverrides: { [CHAR]: 3000, other: 400 } } }),
        memoryMaintenanceConfigSet: (req) => ({ success: true, settings: { ...MAINTENANCE, ...req.config } }),
        memoryCharacterCounts: () => ({ characters: [{ id: CHAR, name: 'Ariadne', demotable: 2100 }, { id: 'pip', name: 'Pip', memoryCount: 3 }] }),
      },
      seen,
    );
    await click(f, Array.from(host(f).querySelectorAll('button')).find((b) => b.textContent?.includes('Per-character overrides'))!);
    const input = (name: string) => host(f).querySelector(`input[aria-label="Cap override for ${name}"]`) as HTMLInputElement;

    await blurWith(f, input('Pip'), '');
    expect(posts(seen, 'memoryMaintenanceConfigSet')).toEqual([]);

    await blurWith(f, input('Ariadne'), '50');
    expect(errorLine(f)).toEqual([`Override for character 550e8400 must be between ${(100).toLocaleString()} and ${(100000).toLocaleString()}`]);
    expect(input('Ariadne').value).toBe('50');
    expect(posts(seen, 'memoryMaintenanceConfigSet')).toEqual([]);

    await blurWith(f, input('Ariadne'), '3000');
    expect(posts(seen, 'memoryMaintenanceConfigSet')).toEqual([]);

    await blurWith(f, input('Pip'), '700');
    expect(posts(seen, 'memoryMaintenanceConfigSet')).toEqual([{ perCharacterCapOverrides: { [CHAR]: 3000, other: 400, pip: 700 } }]);

    await blurWith(f, input('Ariadne'), '');
    expect(posts(seen, 'memoryMaintenanceConfigSet')[1]).toEqual({ perCharacterCapOverrides: { other: 400, pip: 700 } });
  });

  it('the counts read failing: no overrides toggle, no error, the card still renders', async () => {
    const f = await rendered({
      memoryCharacterCounts: () => {
        throw new CoreDispatchError({ kind: 'internal', message: 'boom' });
      },
    });
    expect(text(f)).not.toContain('Per-character overrides');
    expect(errorLine(f)).toEqual([]);
    expect(text(f)).toContain('The cap');
  });

  it('ONE error line from either document', async () => {
    const f = await rendered({
      memoryMaintenanceConfigGet: () => {
        throw new CoreDispatchError({ kind: 'internal', message: 'x' });
      },
      memoryConsolidationConfigGet: () => {
        throw new CoreDispatchError({ kind: 'internal', message: 'y' });
      },
    });
    expect(errorLine(f)).toEqual(['Failed to load maintenance settings']);
    const g = await rendered({
      memoryConsolidationConfigGet: () => {
        throw new CoreDispatchError({ kind: 'internal', message: 'y' });
      },
    });
    expect(errorLine(g)).toEqual(['Failed to load consolidation settings']);
  });

  it('Run maintenance… opens the picker variant of the dialog; its Close closes it', async () => {
    const f = await rendered();
    const run = Array.from(host(f).querySelectorAll('button')).find((b) => b.textContent?.trim() === 'Run maintenance…')!;
    expect(run.className).toBe('qt-button qt-button-secondary');
    await click(f, run);
    const dialog = document.body.querySelector('qt-maintenance-dialog') as HTMLElement;
    expect(dialog).not.toBeNull();
    expect(dialog.querySelector('h2')?.textContent).toBe('Memory Maintenance');
    expect(dialog.querySelector('#maintenance-target')).not.toBeNull();
    (Array.from(dialog.querySelectorAll('button')).find((b) => b.textContent?.trim() === 'Close') as HTMLButtonElement).click();
    await settle(f);
    expect(document.body.querySelector('qt-maintenance-dialog')).toBeNull();
  });
});
