import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { CoreDispatchError } from '../../../core/core-contract';
import type { ConnectionProfileDto } from '../../../core/core-contract';
import { ToastService } from '../../../ui/toast.service';
import { MemoryConsolidationCard } from './memory-consolidation-card';

/**
 * P4.D276 — the Consolidation card, every string and rule from v4
 * `components/tools/memory-consolidation-card.tsx` at `01a83539d` (line pins
 * per arm). No realtime harness: the card injects no `RealtimeService` (R-J).
 */

interface Req {
  type: string;
  config?: Record<string, unknown>;
  options?: Record<string, unknown>;
  [k: string]: unknown;
}

type Handler = (req: Req) => Record<string, unknown> | Promise<Record<string, unknown>>;

const STORED = {
  enabled: false,
  connectionProfileId: null,
  clusterThreshold: 0.9,
  minClusterSize: 3,
  maxClusterSize: 30,
  matureAfterDays: 7,
  maxClustersPerRun: 40,
  watermark: 150,
  coldRetentionDays: 30,
};

function profile(over: Partial<ConnectionProfileDto>): ConnectionProfileDto {
  return {
    id: 'p1',
    name: 'House',
    provider: 'OPENAI',
    modelName: 'gpt-4o',
    parameters: {},
    isDefault: false,
    ...over,
  };
}

const PROFILES = [profile({}), profile({ id: 'p2', name: 'Bare', provider: 'OLLAMA', modelName: '' })];

function render(
  overrides: Partial<Record<string, Handler>> = {},
  seen: Req[] = [],
): ComponentFixture<MemoryConsolidationCard> {
  const handlers: Record<string, Handler> = {
    memoryConsolidationConfigGet: () => ({ success: true, settings: { ...STORED } }),
    memoryCharacterCounts: () => ({
      characters: [
        { id: 'mnemo', name: 'Mnemo', memoryCount: 1234 },
        { id: 'pip', name: 'Pip', memoryCount: 0 },
      ],
    }),
    memoryConsolidationConfigSet: (req) => ({ success: true, settings: { ...req.config } }),
    memoryConsolidate: () => ({ success: true, dryRun: false, jobId: 'job-1' }),
    ...overrides,
  };
  const client: Partial<CoreClient> = {
    dispatchData: (async (req: Req) => {
      seen.push(req);
      const h = handlers[req.type];
      return h ? h(req) : {};
    }) as CoreClient['dispatchData'],
    dispatchExpect: (async () => ({
      type: 'connectionProfiles',
      data: { profiles: PROFILES },
    })) as unknown as CoreClient['dispatchExpect'],
  };
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [MemoryConsolidationCard],
    providers: [
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      { provide: CoreClient, useValue: client },
    ],
  });
  const fixture = TestBed.createComponent(MemoryConsolidationCard);
  fixture.detectChanges();
  return fixture;
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 8; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function rendered(
  overrides: Partial<Record<string, Handler>> = {},
  seen: Req[] = [],
): Promise<ComponentFixture<MemoryConsolidationCard>> {
  const fixture = render(overrides, seen);
  await settle(fixture);
  return fixture;
}

const host = (f: ComponentFixture<unknown>) => f.nativeElement as HTMLElement;
const field = (f: ComponentFixture<unknown>, key: string) =>
  host(f).querySelector(`#consolidation-${key}`) as HTMLInputElement;
const text = (f: ComponentFixture<unknown>) => (host(f).textContent ?? '').replace(/\s+/g, ' ');
const toasts = () =>
  TestBed.inject(ToastService)
    .toasts()
    .map((t) => `${t.type}:${t.message}`);
const sets = (seen: Req[]) => seen.filter((r) => r.type === 'memoryConsolidationConfigSet');
const errorLine = (f: ComponentFixture<unknown>) =>
  host(f).querySelector('p.qt-text-small.qt-text-destructive')?.textContent ?? null;
const button = (f: ComponentFixture<unknown>, label: string) =>
  Array.from(host(f).querySelectorAll('button')).find(
    (b) => b.textContent?.trim() === label,
  ) as HTMLButtonElement;
const rosterSelect = (f: ComponentFixture<unknown>) =>
  host(f).querySelector('select[aria-label="Character to consolidate"]') as HTMLSelectElement;
const profileSelect = (f: ComponentFixture<unknown>) =>
  host(f).querySelector('#consolidation-profile') as HTMLSelectElement;

async function commit(
  fixture: ComponentFixture<unknown>,
  key: string,
  value: string,
): Promise<void> {
  const input = field(fixture, key);
  input.value = value;
  input.dispatchEvent(new Event('input'));
  // A browser renders between typing and leaving the field.
  await settle(fixture);
  input.dispatchEvent(new Event('blur'));
  await settle(fixture);
}

async function choose(fixture: ComponentFixture<unknown>, id: string): Promise<void> {
  const select = rosterSelect(fixture);
  select.value = id;
  expect(select.value).toBe(id);
  select.dispatchEvent(new Event('change'));
  await settle(fixture);
}

const serverError = (message: string) => new CoreDispatchError({ kind: 'bad-request', message });

describe('MemoryConsolidationCard (P4.D276)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('shows v4\'s loading line while the settings load (v4 :181-183)', () => {
    const fixture = render({ memoryConsolidationConfigGet: () => new Promise(() => undefined) });
    expect(host(fixture).textContent?.trim()).toBe('Loading consolidation settings…');
  });

  it('reads every v4 sentence (v4 :191-193, :203, :207-226, :258-261, :270, :283, :291)', async () => {
    const f = await rendered();
    const t = text(f);
    expect(t).toContain(
      'Consolidation folds clusters of near-duplicate memories into a single, denser digest, and sends the originals to the archive, where they keep for reference and are labelled as superseded. Recall then reads the digests rather than the pile. It runs once a day and whenever a character’s unconsidered notes pass the watermark — once switched on. A capable model does this better than the cheap one. Off by default.',
    );
    expect(host(f).querySelector('input[type="checkbox"]')!.closest('label')!.textContent?.trim()).toBe(
      'Enable automatic consolidation',
    );
    expect(host(f).querySelector('label[for="consolidation-profile"]')?.textContent?.trim()).toBe(
      'Connection profile',
    );
    expect(t).toContain('A capable model is recommended for the first, large backfill.');
    expect(t).toContain('Consolidate now');
    expect(t).toContain(
      'Choose a character. A dry run rehearses a handful of clusters and shows the proposed digests without writing a thing; the real run is queued in the background and ignores the master switch above.',
    );
    expect(rosterSelect(f).options[0].text).toBe('Choose a character…');
    expect(button(f, 'Dry run').classList.contains('qt-button-secondary')).toBe(true);
    expect(button(f, 'Consolidate for real').classList.contains('qt-button-primary')).toBe(true);
  });

  it('transcribes the seven number fields in v4 order (v4 :60-68, :230-254)', async () => {
    const f = await rendered();
    const rows = Array.from(host(f).querySelectorAll('.grid > div')).map((row) => {
      const input = row.querySelector('input') as HTMLInputElement;
      return [
        row.querySelector('label')?.textContent?.trim(),
        input.id,
        input.getAttribute('min'),
        input.getAttribute('max'),
        input.getAttribute('step'),
        input.getAttribute('placeholder'),
        row.querySelector('p')?.textContent?.trim(),
      ];
    });
    expect(rows).toEqual([
      ['Cluster threshold', 'consolidation-clusterThreshold', '0', '1', '0.01', null, 'How alike two notes must be (0 to 1, by cosine) to be folded together.'],
      ['Smallest cluster', 'consolidation-minClusterSize', '2', null, '1', null, 'Fewest notes worth a digest. Falls to two when one of them is quite old.'],
      ['Largest cluster', 'consolidation-maxClusterSize', '2', null, '1', null, 'Most notes put before the model in one sitting.'],
      ['Mature after (days)', 'consolidation-matureAfterDays', '0', null, '1', null, 'Notes younger than this are left to settle.'],
      ['Clusters per run', 'consolidation-maxClustersPerRun', '1', null, '1', null, 'Bounds the cost of a run; a backlog drains over several.'],
      ['Watermark', 'consolidation-watermark', '1', null, '1', null, 'Unconsidered active notes at which a character is queued automatically.'],
      ['Archive retention (days)', 'consolidation-coldRetentionDays', '1', null, '1', 'never', 'Delete superseded archived notes older than this. Leave blank to keep them forever.'],
    ]);
    expect(field(f, 'clusterThreshold').value).toBe('0.9');
    expect(field(f, 'coldRetentionDays').value).toBe('30');
  });

  it('refuses an out-of-range threshold inline: no dispatch, no toast, the draft kept (v4 :146-150)', async () => {
    const seen: Req[] = [];
    const f = await rendered({}, seen);
    await commit(f, 'clusterThreshold', '2');
    expect(errorLine(f)).toBe('Cluster threshold must be a number between 0 and 1');
    expect(sets(seen)).toEqual([]);
    expect(toasts()).toEqual([]);
    expect(field(f, 'clusterThreshold').value).toBe('2');
  });

  it('names a whole number with its floor for integer fields (v4 :149)', async () => {
    const f = await rendered();
    await commit(f, 'minClusterSize', '2.5');
    expect(errorLine(f)).toBe('Smallest cluster must be a whole number of at least 2');
  });

  it('names "a number of at least" for a non-integer field without a max (v4 :149)', async () => {
    const f = await rendered();
    await commit(f, 'matureAfterDays', '-1');
    expect(errorLine(f)).toBe('Mature after (days) must be a number of at least 0');
  });

  it('clears the nullable retention with a null save, and only when it was set (v4 :141-144)', async () => {
    const seen: Req[] = [];
    const f = await rendered({}, seen);
    await commit(f, 'coldRetentionDays', '');
    expect(sets(seen).map((r) => r.config)).toEqual([{ coldRetentionDays: null }]);

    const seen2: Req[] = [];
    const g = await rendered(
      { memoryConsolidationConfigGet: () => ({ settings: { ...STORED, coldRetentionDays: null } }) },
      seen2,
    );
    await commit(g, 'coldRetentionDays', '');
    expect(sets(seen2)).toEqual([]);
  });

  it('a blank NON-nullable field saves nothing and says nothing (v4 :141-144)', async () => {
    const seen: Req[] = [];
    const f = await rendered({}, seen);
    await commit(f, 'watermark', '');
    expect(sets(seen)).toEqual([]);
    expect(errorLine(f)).toBeNull();
    expect(field(f, 'watermark').value).toBe('150');
  });

  it('trims the draft and posts exactly the one key (v4 :140, :154; R-F)', async () => {
    const seen: Req[] = [];
    const f = await rendered({}, seen);
    // jsdom sanitizes a number input's value, so the padded draft goes in directly.
    (f.componentInstance as unknown as { setDraft(k: string, v: string): void }).setDraft('watermark', ' 200 ');
    field(f, 'watermark').dispatchEvent(new Event('blur'));
    await settle(f);
    expect(sets(seen)).toEqual([{ type: 'memoryConsolidationConfigSet', config: { watermark: 200 } }]);
    expect(toasts()).toContain('success:Consolidation settings saved');
  });

  it('an unchanged value posts nothing (v4 :153)', async () => {
    const seen: Req[] = [];
    const f = await rendered({}, seen);
    await commit(f, 'watermark', '150');
    expect(sets(seen)).toEqual([]);
  });

  it('the checkbox posts { enabled: true } only (v4 :200)', async () => {
    const seen: Req[] = [];
    const f = await rendered({}, seen);
    const box = host(f).querySelector('input[type="checkbox"]') as HTMLInputElement;
    box.click();
    await settle(f);
    expect(sets(seen)).toEqual([{ type: 'memoryConsolidationConfigSet', config: { enabled: true } }]);
  });

  it('a FAILED checkbox save leaves the box unchecked, the server sentence inline and toasted (R-E, v4 :122-125)', async () => {
    const f = await rendered({
      memoryConsolidationConfigSet: () => {
        throw serverError('The settings ledger is locked');
      },
    });
    const box = host(f).querySelector('input[type="checkbox"]') as HTMLInputElement;
    box.click();
    expect(box.checked).toBe(true); // the click, before the save settles
    await settle(f);
    expect(box.checked).toBe(false);
    expect(errorLine(f)).toBe('The settings ledger is locked');
    expect(toasts()).toContain('error:The settings ledger is locked');
  });

  it('shows the server\'s clobbered echo, not a client merge (R-F, v4 :119)', async () => {
    const f = await rendered({
      // v4's `.partial()` fills every default: a save of `enabled` resets the threshold.
      memoryConsolidationConfigSet: (req) => ({
        settings: { ...STORED, clusterThreshold: 0.72, coldRetentionDays: null, ...req.config },
      }),
    });
    expect(field(f, 'clusterThreshold').value).toBe('0.9');
    (host(f).querySelector('input[type="checkbox"]') as HTMLInputElement).click();
    await settle(f);
    expect(field(f, 'clusterThreshold').value).toBe('0.72');
    expect(field(f, 'coldRetentionDays').value).toBe('');
  });

  it('labels the profiles as v4 does (v4 :219-224)', async () => {
    const f = await rendered();
    expect(Array.from(profileSelect(f).options).map((o) => [o.value, o.text])).toEqual([
      ['', 'The cheap LLM (default)'],
      ['p1', 'House (OPENAI • gpt-4o)'],
      ['p2', 'Bare (OLLAMA)'],
    ]);
  });

  it('keeps a stored profile that has left the building as its own option (v4 :218)', async () => {
    const f = await rendered({
      memoryConsolidationConfigGet: () => ({ settings: { ...STORED, connectionProfileId: 'gone' } }),
    });
    const select = profileSelect(f);
    expect(Array.from(select.options).map((o) => [o.value, o.text])[1]).toEqual([
      'gone',
      '(a profile that has since left the building)',
    ]);
    expect(select.value).toBe('gone');
  });

  it('posts connectionProfileId: null for the cheap LLM, and the id otherwise (v4 :215)', async () => {
    const seen: Req[] = [];
    const f = await rendered(
      { memoryConsolidationConfigGet: () => ({ settings: { ...STORED, connectionProfileId: 'p1' } }) },
      seen,
    );
    const select = profileSelect(f);
    expect(select.value).toBe('p1');
    select.value = '';
    select.dispatchEvent(new Event('change'));
    await settle(f);
    expect(sets(seen).map((r) => r.config)).toEqual([{ connectionProfileId: null }]);
  });

  it('a FAILED profile save snaps the select back to the stored profile (R-E)', async () => {
    const f = await rendered({
      memoryConsolidationConfigGet: () => ({ settings: { ...STORED, connectionProfileId: 'p1' } }),
      memoryConsolidationConfigSet: () => {
        throw serverError('No such profile');
      },
    });
    const select = profileSelect(f);
    select.value = 'p2';
    select.dispatchEvent(new Event('change'));
    await settle(f);
    expect(select.value).toBe('p1');
    expect(errorLine(f)).toBe('No such profile');
  });

  it('lists the roster with grouped counts (v4 :271-275)', async () => {
    const f = await rendered();
    expect(Array.from(rosterSelect(f).options).map((o) => o.text)).toEqual([
      'Choose a character…',
      'Mnemo (1,234 memories)',
      'Pip (0 memories)',
    ]);
  });

  it('disables both run buttons with no character and while a run is in flight (v4 :280, :288)', async () => {
    let release: (v: Record<string, unknown>) => void = () => undefined;
    const f = await rendered({
      memoryConsolidate: () => new Promise((r) => (release = r)),
    });
    expect(button(f, 'Dry run').disabled).toBe(true);
    expect(button(f, 'Consolidate for real').disabled).toBe(true);
    await choose(f, 'pip');
    expect(button(f, 'Dry run').disabled).toBe(false);
    button(f, 'Consolidate for real').click();
    await settle(f);
    expect(button(f, 'Dry run').disabled).toBe(true);
    expect(button(f, 'Queuing…').disabled).toBe(true);
    expect(rosterSelect(f).disabled).toBe(true);
    expect(field(f, 'watermark').disabled).toBe(true);
    release({ jobId: 'j' });
    await settle(f);
    expect(button(f, 'Consolidate for real').disabled).toBe(false);
  });

  it('labels a rehearsal in flight `Rehearsing…` (v4 :283)', async () => {
    const f = await rendered({ memoryConsolidate: () => new Promise(() => undefined) });
    await choose(f, 'pip');
    button(f, 'Dry run').click();
    await settle(f);
    expect(button(f, 'Rehearsing…')).toBeTruthy();
  });

  it('a dry run sends exactly { characterId, dryRun: true } and opens the report (v4 :157-168)', async () => {
    const seen: Req[] = [];
    const f = await rendered(
      {
        memoryConsolidate: () => ({
          success: true,
          dryRun: true,
          report: {
            characterId: 'pip',
            characterName: 'Pip',
            dryRun: true,
            trigger: 'manual',
            startedAt: '',
            finishedAt: '',
            settings: STORED,
            clusters: [],
            stats: { candidates: 0, clustersFound: 0, clustersAttempted: 0, digestsCreated: 0, digestsUpdated: 0, clustersDeferred: 0, immatureSkipped: 0, noEmbeddingSkipped: 0, budgetExhausted: false, durationMs: 40 },
          },
        }),
      },
      seen,
    );
    await choose(f, 'pip');
    button(f, 'Dry run').click();
    await settle(f);
    TestBed.tick();
    expect(seen.filter((r) => r.type === 'memoryConsolidate')).toEqual([
      { type: 'memoryConsolidate', options: { characterId: 'pip', dryRun: true } },
    ]);
    const dialog = document.body.querySelector('qt-memory-consolidation-report-dialog') as HTMLElement;
    expect(dialog).toBeTruthy();
    expect(dialog.querySelector('h2')?.textContent).toBe('Consolidation, Rehearsed — Pip');
    expect(toasts()).toEqual([]);
  });

  it('a dry reply WITHOUT a report toasts the queued sentence — v4\'s quirk (v4 :167-171)', async () => {
    const f = await rendered({ memoryConsolidate: () => ({ success: true, dryRun: true }) });
    await choose(f, 'pip');
    button(f, 'Dry run').click();
    await settle(f);
    expect(toasts()).toContain(
      'success:The consolidation has been sent below stairs — it will run in the background',
    );
    expect(document.body.querySelector('qt-memory-consolidation-report-dialog')).toBeNull();
  });

  it('a real run sends dryRun: false and toasts the queued sentence (v4 :170)', async () => {
    const seen: Req[] = [];
    const f = await rendered({}, seen);
    await choose(f, 'mnemo');
    button(f, 'Consolidate for real').click();
    await settle(f);
    expect(seen.filter((r) => r.type === 'memoryConsolidate')).toEqual([
      { type: 'memoryConsolidate', options: { characterId: 'mnemo', dryRun: false } },
    ]);
    expect(toasts()).toContain(
      'success:The consolidation has been sent below stairs — it will run in the background',
    );
  });

  it('a failed rehearsal shows the server sentence, the fallback when it has none (v4 :173)', async () => {
    const f = await rendered({
      memoryConsolidate: () => {
        throw serverError('Character not found');
      },
    });
    await choose(f, 'pip');
    button(f, 'Dry run').click();
    await settle(f);
    expect(errorLine(f)).toBe('Character not found');

    const g = await rendered({
      memoryConsolidate: () => {
        throw serverError('');
      },
    });
    await choose(g, 'pip');
    button(g, 'Dry run').click();
    await settle(g);
    expect(errorLine(g)).toBe('The rehearsal failed');
    expect(toasts()).toContain('error:The rehearsal failed');
  });

  it('a failed real run falls back to `Failed to queue consolidation` (v4 :173)', async () => {
    const f = await rendered({
      memoryConsolidate: () => {
        throw serverError('');
      },
    });
    await choose(f, 'pip');
    button(f, 'Consolidate for real').click();
    await settle(f);
    expect(errorLine(f)).toBe('Failed to queue consolidation');
  });

  it('a READ failure shows the fallback, never the server\'s text, and the card still renders (R-G, v4 :105-108)', async () => {
    const f = await rendered({
      memoryConsolidationConfigGet: () => {
        throw serverError('Something private went wrong');
      },
    });
    expect(errorLine(f)).toBe('Failed to load consolidation settings');
    expect(text(f)).not.toContain('Something private went wrong');
    expect(field(f, 'watermark').value).toBe('150');
  });
});
