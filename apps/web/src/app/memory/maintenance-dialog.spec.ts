import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../core/core-client';
import { CoreDispatchError } from '../core/core-contract';
import { ToastService } from '../ui/toast.service';
import { MaintenanceDialog } from './maintenance-dialog';

/**
 * P4.D300 — the Maintenance dialog, v4 `components/memory/maintenance-dialog.tsx`
 * (`b045dd986`) at `83334b9a5`. The first describe transcribes v4's three jest
 * cases (`__tests__/unit/components/memory/maintenance-dialog.test.tsx:52`,
 * `:67`, `:77`) case for case; the rest are the lane's own. The request body is
 * compared through `JSON.stringify` where key order is the contract.
 */

const CHAR = '550e8400-e29b-41d4-a716-446655440001';

/** v4's `REPORT` fixture (`maintenance-dialog.test.tsx:28-45`), verbatim. */
const REPORT = {
  characterId: CHAR,
  characterName: 'Ariadne',
  reason: 'manual',
  dryRun: true,
  startedAt: '2026-10-13T00:00:00.000Z',
  finishedAt: '2026-10-13T00:00:01.000Z',
  durationMs: 1000,
  phases: [
    { phase: 'fold-near-duplicates', status: 'ran', durationMs: 10 },
    { phase: 'consolidate', status: 'skipped', skippedBecause: 'not-requested', durationMs: 0 },
    { phase: 'demote', status: 'ran', durationMs: 10 },
    { phase: 'retire', status: 'skipped', skippedBecause: 'no-retention-window', durationMs: 0 },
    { phase: 'mirror', status: 'skipped', skippedBecause: 'not-requested', durationMs: 0 },
  ],
  fold: { eligible: 40, deferred: 0, groups: [{ survivorId: 'a', absorbedIds: ['b'], minCosine: 0.93 }], folded: 1, groupsFailed: 0, groupsDeferred: 0 },
  demote: { cap: 100, hotTotal: 130, demotable: 120, protected: 10, excess: 20, demoted: 20, demotedIds: [], highestScoreDemoted: 0.1 },
};

interface Req {
  type: string;
  options?: Record<string, unknown>;
  [k: string]: unknown;
}
type Handler = (req: Req) => Record<string, unknown> | Promise<Record<string, unknown>>;

function render(
  inputs: { characterId?: string; characterName?: string },
  handlers: Partial<Record<string, Handler>>,
  seen: Req[] = [],
): ComponentFixture<MaintenanceDialog> {
  const client: Partial<CoreClient> = {
    dispatchData: (async (req: Req) => {
      seen.push(req);
      const h = handlers[req.type];
      return h ? h(req) : {};
    }) as CoreClient['dispatchData'],
  };
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [MaintenanceDialog],
    providers: [
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      { provide: CoreClient, useValue: client },
    ],
  });
  const fixture = TestBed.createComponent(MaintenanceDialog);
  if (inputs.characterId !== undefined) fixture.componentRef.setInput('characterId', inputs.characterId);
  if (inputs.characterName !== undefined) fixture.componentRef.setInput('characterName', inputs.characterName);
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
const text = (f: ComponentFixture<unknown>) => (host(f).textContent ?? '').replace(/\s+/g, ' ');
const button = (f: ComponentFixture<unknown>, label: string) =>
  Array.from(host(f).querySelectorAll('button')).find((b) => b.textContent?.trim() === label) as HTMLButtonElement;
const phaseBox = (f: ComponentFixture<unknown>, label: string) =>
  Array.from(host(f).querySelectorAll('fieldset label')).find(
    (l) => l.querySelector('span > span')?.textContent === label,
  )!.querySelector('input') as HTMLInputElement;
const picker = (f: ComponentFixture<unknown>) => host(f).querySelector('#maintenance-target') as HTMLSelectElement | null;
const capInput = (f: ComponentFixture<unknown>) => host(f).querySelector('#maintenance-cap') as HTMLInputElement | null;
const maintains = (seen: Req[]) => seen.filter((r) => r.type === 'memoryMaintain');
const toasts = () =>
  TestBed.inject(ToastService)
    .toasts()
    .map((t) => `${t.type}:${t.message}`);

async function choose(f: ComponentFixture<unknown>, value: string): Promise<void> {
  const sel = picker(f)!;
  sel.value = value;
  sel.dispatchEvent(new Event('change'));
  await settle(f);
}

async function typeCap(f: ComponentFixture<unknown>, value: string): Promise<void> {
  const el = capInput(f)!;
  el.value = value;
  el.dispatchEvent(new Event('input'));
  await settle(f);
}

/** For the jsdom number sanitizer: set the draft the way an `input` event would. */
async function seedCap(f: ComponentFixture<MaintenanceDialog>, value: string): Promise<void> {
  (f.componentInstance as unknown as { setCapDraft(v: string): void }).setCapDraft(value);
  await settle(f);
}

describe('MaintenanceDialog — v4 maintenance-dialog.test.tsx, transcribed', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('rehearses the chosen phases for a fixed character and shows the report (:52)', async () => {
    const seen: Req[] = [];
    const f = render({ characterId: CHAR, characterName: 'Ariadne' }, { memoryMaintain: () => ({ success: true, dryRun: true, report: REPORT }) }, seen);
    await settle(f);
    // Leave consolidation out of the rehearsal.
    phaseBox(f, 'Consolidate').click();
    await settle(f);
    button(f, 'Dry run').click();
    await settle(f);
    expect(text(f)).toContain('Would archive');
    expect(maintains(seen).map((r) => r.options)).toEqual([
      { characterId: CHAR, phases: ['fold-near-duplicates', 'demote', 'retire'], dryRun: true },
    ]);
    expect(text(f)).toMatch(/no archive retention window is set/i);
  });

  it('sends a cap for this run only when demotion is asked for (:67)', async () => {
    const seen: Req[] = [];
    const f = render({ characterId: CHAR }, { memoryMaintain: () => ({ success: true, dryRun: true, report: REPORT }) }, seen);
    await settle(f);
    await typeCap(f, '500');
    button(f, 'Dry run').click();
    await settle(f);
    expect(maintains(seen)[0].options).toMatchObject({ settings: { cap: 500 } });
  });

  it('will not rehearse consolidation for every character, but will queue it (:77)', async () => {
    const seen: Req[] = [];
    const f = render(
      {},
      {
        memoryCharacterCounts: () => ({ characters: [{ id: CHAR, name: 'Ariadne', demotable: 12 }] }),
        memoryMaintain: () => ({ success: true, dryRun: false, jobIds: ['j1', 'j2'] }),
      },
      seen,
    );
    const events: string[] = [];
    f.componentInstance.queued.subscribe(() => events.push('queued'));
    f.componentInstance.close.subscribe(() => events.push('close'));
    await settle(f);
    expect(Array.from(picker(f)!.options).some((o) => /ariadne/i.test(o.textContent ?? ''))).toBe(true);

    await choose(f, '__all__');
    expect(button(f, 'Dry run').disabled).toBe(true);

    button(f, 'Run for real').click();
    await settle(f);
    expect(maintains(seen).map((r) => r.options)).toEqual([
      { phases: ['fold-near-duplicates', 'consolidate', 'demote', 'retire'], dryRun: false },
    ]);
    expect(toasts().some((t) => t.startsWith('success:') && t.includes('2 characters'))).toBe(true);
    // v4: `onQueued?.()` THEN `onClose()`.
    expect(events).toEqual(['queued', 'close']);
  });
});

describe('MaintenanceDialog — the lane\'s own rows (P4.D300)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('the body\'s KEY ORDER is v4\'s: characterId, phases, dryRun, settings', async () => {
    const seen: Req[] = [];
    const f = render({ characterId: CHAR }, { memoryMaintain: () => ({ success: true, dryRun: true, report: REPORT }) }, seen);
    await settle(f);
    await seedCap(f, '300');
    button(f, 'Dry run').click();
    await settle(f);
    expect(JSON.stringify(maintains(seen)[0].options)).toBe(
      `{"characterId":"${CHAR}","phases":["fold-near-duplicates","consolidate","demote","retire"],"dryRun":true,"settings":{"cap":300}}`,
    );
  });

  it('a typed cap is NOT sent once demotion is unticked; the cap input hides', async () => {
    const seen: Req[] = [];
    const f = render({ characterId: CHAR }, { memoryMaintain: () => ({ success: true, dryRun: true, report: REPORT }) }, seen);
    await settle(f);
    await seedCap(f, '300');
    phaseBox(f, 'Demote over the cap').click();
    await settle(f);
    expect(capInput(f)).toBeNull();
    button(f, 'Dry run').click();
    await settle(f);
    expect(JSON.stringify(maintains(seen)[0].options)).toBe(
      `{"characterId":"${CHAR}","phases":["fold-near-duplicates","consolidate","retire"],"dryRun":true}`,
    );
  });

  it('capInvalid at 99 / 100,001 / abc: the hint and both buttons disabled; blank and 100 are fine', async () => {
    const f = render({ characterId: CHAR }, {});
    await settle(f);
    const hint = () => capInput(f)!.parentElement!.querySelector('p')!.textContent;
    expect(hint()).toBe('Leave blank to use each character’s own cap (its override, else the global one).');
    expect(capInput(f)!.getAttribute('placeholder')).toBe('the configured cap');
    for (const bad of ['99', '100001', 'abc']) {
      await seedCap(f, bad);
      expect(hint()).toBe(`Between ${(100).toLocaleString()} and ${(100000).toLocaleString()}, if you please.`);
      expect(button(f, 'Dry run').disabled).toBe(true);
      expect(button(f, 'Run for real').disabled).toBe(true);
    }
    for (const good of ['100', '100000', '']) {
      await seedCap(f, good);
      expect(button(f, 'Run for real').disabled).toBe(false);
    }
  });

  it('nothing ticked → both buttons disabled; the picker variant needs a target first', async () => {
    const f = render({ characterId: CHAR }, {});
    await settle(f);
    for (const label of ['Fold near-duplicates', 'Consolidate', 'Demote over the cap', 'Retire old archive notes']) {
      phaseBox(f, label).click();
      await settle(f);
    }
    expect(button(f, 'Dry run').disabled).toBe(true);
    expect(button(f, 'Run for real').disabled).toBe(true);

    const p = render({}, { memoryCharacterCounts: () => ({ characters: [] }) });
    await settle(p);
    expect(picker(p)!.value).toBe('');
    expect(button(p, 'Run for real').disabled).toBe(true);
  });

  it('the result clears on each of the three edits: a phase, the target, the cap', async () => {
    const f = render(
      {},
      {
        memoryCharacterCounts: () => ({ characters: [{ id: CHAR, name: 'Ariadne', demotable: 12 }, { id: 'c2', name: 'Orla', memoryCount: 3 }] }),
        memoryMaintain: () => ({ success: true, dryRun: true, report: REPORT }),
      },
    );
    await settle(f);
    await choose(f, CHAR);
    const rehearse = async () => {
      button(f, 'Dry run').click();
      await settle(f);
      expect(text(f)).toContain('Would archive');
    };
    await rehearse();
    phaseBox(f, 'Retire old archive notes').click();
    await settle(f);
    expect(text(f)).not.toContain('Would archive');
    await rehearse();
    await choose(f, 'c2');
    expect(text(f)).not.toContain('Would archive');
    await rehearse();
    await typeCap(f, '200');
    expect(text(f)).not.toContain('Would archive');
    expect(text(f)).toContain('Choose the phases, then rehearse or send for the staff.');
  });

  it('every control is disabled while a run is in flight; Rehearsing… shows', async () => {
    let release!: (v: Record<string, unknown>) => void;
    const f = render({}, {
      memoryCharacterCounts: () => ({ characters: [{ id: CHAR, name: 'Ariadne', demotable: 12 }] }),
      memoryMaintain: () => new Promise((r) => (release = r)),
    });
    await settle(f);
    await choose(f, CHAR);
    button(f, 'Dry run').click();
    await settle(f);
    expect(picker(f)!.disabled).toBe(true);
    expect(Array.from(host(f).querySelectorAll('fieldset input')).every((i) => (i as HTMLInputElement).disabled)).toBe(true);
    expect(capInput(f)!.disabled).toBe(true);
    expect(button(f, 'Rehearsing…').disabled).toBe(true);
    expect(button(f, 'Run for real').disabled).toBe(true);
    expect(host(f).querySelector('.flex-1 p.qt-text-muted')?.textContent).toBe('Rehearsing…');
    release({ success: true, dryRun: true, report: REPORT });
    await settle(f);
    expect(picker(f)!.disabled).toBe(false);
  });

  it('the fixed variant: a bare title without a name, no picker, the counts never read', async () => {
    const seen: Req[] = [];
    const f = render({ characterId: CHAR }, {}, seen);
    await settle(f);
    expect(host(f).querySelector('h2')?.textContent).toBe('Memory Maintenance');
    expect(picker(f)).toBeNull();
    expect(seen.some((r) => r.type === 'memoryCharacterCounts')).toBe(false);
    const named = render({ characterId: CHAR, characterName: 'Ariadne' }, {});
    expect(host(named).querySelector('h2')?.textContent).toBe('Memory Maintenance — Ariadne');
  });

  it('the picker lists Choose…, Every character, then each `{name} ({n} counted)` in reply order', async () => {
    const f = render({}, {
      memoryCharacterCounts: () => ({
        characters: [
          { id: 'b', name: 'Mnemo', memoryCount: 45, demotable: 1200 },
          { id: 'a', name: 'Pip', memoryCount: 0 },
          { id: 'c', name: 'Bare' },
        ],
      }),
    });
    await settle(f);
    expect(Array.from(picker(f)!.options).map((o) => [o.value, o.textContent])).toEqual([
      ['', 'Choose a character…'],
      ['__all__', 'Every character'],
      ['b', `Mnemo (${(1200).toLocaleString()} counted)`],
      ['a', 'Pip (0 counted)'],
      ['c', 'Bare (0 counted)'],
    ]);
    expect(host(f).querySelector('label[for="maintenance-target"]')?.textContent).toBe('Character');
  });

  it('Every character with Consolidate ticked: the warning; unticked, the dry run asks for the table', async () => {
    const seen: Req[] = [];
    const f = render({}, {
      memoryCharacterCounts: () => ({ characters: [] }),
      memoryMaintain: () => ({ success: true, dryRun: true, reports: [] }),
    }, seen);
    await settle(f);
    await choose(f, '__all__');
    expect(text(f)).toContain('Consolidation calls a model, so it can be rehearsed for one character at a time.');
    phaseBox(f, 'Consolidate').click();
    await settle(f);
    expect(text(f)).not.toContain('Consolidation calls a model');
    button(f, 'Dry run').click();
    await settle(f);
    expect(JSON.stringify(maintains(seen)[0].options)).toBe(
      '{"phases":["fold-near-duplicates","demote","retire"],"dryRun":true}',
    );
    expect(text(f)).toContain('No character has any memories to tend.');
  });

  it('a dry reply without `report` or `reports` renders the empty table (v4 `reports ?? []`)', async () => {
    const f = render({ characterId: CHAR }, { memoryMaintain: () => ({ success: true, dryRun: true }) });
    await settle(f);
    button(f, 'Dry run').click();
    await settle(f);
    expect(text(f)).toContain('No character has any memories to tend.');
  });

  it('a real run for one character: the plain toast, the status key invalidated, (queued) then (close)', async () => {
    const f = render({ characterId: CHAR }, { memoryMaintain: () => ({ success: true, dryRun: false, jobId: 'j1', jobIds: ['j1'] }) });
    await settle(f);
    const qc = TestBed.inject(QueryClient);
    const invalidated: unknown[] = [];
    const original = qc.invalidateQueries.bind(qc);
    qc.invalidateQueries = ((filters: { queryKey?: unknown }) => {
      invalidated.push(filters?.queryKey);
      return original(filters as never);
    }) as typeof qc.invalidateQueries;
    const events: string[] = [];
    f.componentInstance.queued.subscribe(() => events.push('queued'));
    f.componentInstance.close.subscribe(() => events.push('close'));
    button(f, 'Run for real').click();
    await settle(f);
    expect(toasts()).toEqual(['success:The staff have been sent round; the work proceeds below stairs.']);
    expect(invalidated).toEqual([['memories', 'maintenance-status']]);
    expect(events).toEqual(['queued', 'close']);
  });

  it('every character, one job / no jobIds: `1 character` and `0 characters`', async () => {
    for (const [reply, n] of [
      [{ success: true, dryRun: false, jobIds: ['j1'] }, '1 character;'],
      [{ success: true, dryRun: false }, '0 characters;'],
    ] as const) {
      const f = render({}, { memoryCharacterCounts: () => ({ characters: [] }), memoryMaintain: () => reply });
      await settle(f);
      await choose(f, '__all__');
      button(f, 'Run for real').click();
      await settle(f);
      expect(toasts()).toEqual([`success:The staff have been sent round to ${n} the work proceeds below stairs.`]);
    }
  });

  it('a failed rehearsal / queue: the server sentence (or v4\'s fallback) on the line + an error toast', async () => {
    const cases: [boolean, string, string][] = [
      [true, 'A dry run that includes consolidation names one character (characterId)', 'A dry run that includes consolidation names one character (characterId)'],
      [true, '', 'The rehearsal failed'],
      [false, '', 'Failed to queue maintenance'],
    ];
    for (const [dry, message, shown] of cases) {
      const events: string[] = [];
      const f = render({ characterId: CHAR }, {
        memoryMaintain: () => {
          throw new CoreDispatchError({ kind: 'bad-request', message });
        },
      });
      f.componentInstance.close.subscribe(() => events.push('close'));
      await settle(f);
      button(f, dry ? 'Dry run' : 'Run for real').click();
      await settle(f);
      expect(host(f).querySelector('p.qt-text-destructive')?.textContent).toBe(shown);
      expect(toasts()).toEqual([`error:${shown}`]);
      expect(text(f)).not.toContain('Choose the phases, then rehearse');
      expect(events).toEqual([]);
    }
  });

  it('sits under document.body, z-[100], with no role="dialog"; both Close controls emit close (L-4)', async () => {
    const f = render({ characterId: CHAR }, {});
    await settle(f);
    expect(host(f).parentElement).toBe(document.body);
    expect((host(f).querySelector('.qt-dialog-overlay') as HTMLElement).classList.contains('z-[100]')).toBe(true);
    expect(host(f).querySelector('[role="dialog"]')).toBeNull();
    let closed = 0;
    f.componentInstance.close.subscribe(() => (closed += 1));
    (host(f).querySelector('button[aria-label="Close"]') as HTMLButtonElement).click();
    button(f, 'Close').click();
    expect(closed).toBe(2);
  });

  it('the description, the phase hints and the idle line read v4\'s words', async () => {
    const f = render({ characterId: CHAR }, {});
    await settle(f);
    expect(host(f).querySelector('.qt-dialog-description')?.textContent?.replace(/\s+/g, ' ').trim()).toBe(
      'Rehearse first, if you please: a dry run shows what each phase would do and writes not a word. The real run is queued in the background and goes ahead whatever the switches on the settings card say.',
    );
    const hints = Array.from(host(f).querySelectorAll('fieldset label')).map((l) => [
      l.querySelector('span > span')?.textContent,
      l.querySelector('span > span.qt-text-xs')?.textContent,
      (l.querySelector('input') as HTMLInputElement).checked,
    ]);
    expect(hints).toEqual([
      ['Fold near-duplicates', 'Fold notes that say the same thing about the same occasion into the sturdiest of them. No model is consulted.', true],
      ['Consolidate', 'Ask a model to fold clusters of related notes into digests; the originals go to the archive. Costs model calls.', true],
      ['Demote over the cap', 'Send the least protected notes to the archive while a character holds more than the cap.', true],
      ['Retire old archive notes', 'Let go of superseded archive notes older than the retention window (when one is set).', true],
    ]);
    expect(host(f).querySelector('legend')?.textContent).toBe('Phases');
    expect(text(f)).toContain('Choose the phases, then rehearse or send for the staff.');
  });
});
