import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { CoreDispatchError } from '../../../core/core-contract';
import { ToastService } from '../../../ui/toast.service';
import { MemoryExtractionGrainCard } from './memory-extraction-grain-card';

/**
 * P4.D276 — the Extraction Grain card, every string and rule from v4
 * `components/tools/memory-extraction-grain-card.tsx` at `01a83539d`.
 */

interface Req {
  type: string;
  config?: Record<string, unknown>;
  [k: string]: unknown;
}
type Handler = (req: Req) => Record<string, unknown> | Promise<Record<string, unknown>>;

const STORED = { otherPass: 'hybrid', perTurnOtherFloor: 0.75, foldCandidatesPerSubject: 3 };

function render(
  overrides: Partial<Record<string, Handler>> = {},
  seen: Req[] = [],
): ComponentFixture<MemoryExtractionGrainCard> {
  const handlers: Record<string, Handler> = {
    memoryExtractionModeConfigGet: () => ({ success: true, settings: { ...STORED } }),
    memoryExtractionModeConfigSet: (req) => ({ success: true, settings: { ...req.config } }),
    ...overrides,
  };
  const client: Partial<CoreClient> = {
    dispatchData: (async (req: Req) => {
      seen.push(req);
      const h = handlers[req.type];
      return h ? h(req) : {};
    }) as CoreClient['dispatchData'],
  };
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [MemoryExtractionGrainCard],
    providers: [
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      { provide: CoreClient, useValue: client },
    ],
  });
  const fixture = TestBed.createComponent(MemoryExtractionGrainCard);
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
): Promise<ComponentFixture<MemoryExtractionGrainCard>> {
  const f = render(overrides, seen);
  await settle(f);
  return f;
}

const host = (f: ComponentFixture<unknown>) => f.nativeElement as HTMLElement;
const select = (f: ComponentFixture<unknown>) =>
  host(f).querySelector('#extraction-other-pass') as HTMLSelectElement;
const floor = (f: ComponentFixture<unknown>) => host(f).querySelector('#extraction-floor') as HTMLInputElement;
const perSubject = (f: ComponentFixture<unknown>) =>
  host(f).querySelector('#extraction-per-subject') as HTMLInputElement;
const description = (f: ComponentFixture<unknown>) =>
  (select(f).nextElementSibling as HTMLElement | null)?.textContent ?? null;
const errorLine = (f: ComponentFixture<unknown>) =>
  host(f).querySelector('p.qt-text-small.qt-text-destructive')?.textContent ?? null;
const sets = (seen: Req[]) => seen.filter((r) => r.type === 'memoryExtractionModeConfigSet');
const toasts = () =>
  TestBed.inject(ToastService)
    .toasts()
    .map((t) => `${t.type}:${t.message}`);

async function commit(f: ComponentFixture<unknown>, input: HTMLInputElement, value: string) {
  input.value = value;
  input.dispatchEvent(new Event('input'));
  await settle(f); // a browser renders between typing and leaving the field
  input.dispatchEvent(new Event('blur'));
  await settle(f);
}

async function choose(f: ComponentFixture<unknown>, value: string) {
  const s = select(f);
  s.value = value;
  expect(s.value).toBe(value);
  s.dispatchEvent(new Event('change'));
  await settle(f);
}

const DESCRIPTIONS = {
  hybrid:
    'Observations of others are gathered when a conversation is folded into its summary, with only the weightiest — commitments, agreements, new standing facts — noted as they happen.',
  fold: 'Observations of others are gathered solely when a conversation is folded into its summary. The thriftiest course.',
  turn: 'Observations of others are noted after each turn, as of old. The most thorough, and the most prone to repeating itself.',
};

describe('MemoryExtractionGrainCard (P4.D276)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('shows v4\'s loading line while the settings load (v4 :115-117)', () => {
    const f = render({ memoryExtractionModeConfigGet: () => new Promise(() => undefined) });
    expect(host(f).textContent?.trim()).toBe('Loading extraction settings…');
  });

  it('reads the intro with its two <em> elements (v4 :123-125)', async () => {
    const f = await rendered();
    const p = host(f).querySelector('p.qt-text-small.qt-text-muted') as HTMLElement;
    expect(p.textContent?.replace(/\s+/g, ' ').trim()).toBe(
      'A character’s notes about themselves are always taken as the conversation goes. Notes about other characters are another matter: taken every turn, they pile up into a hundred ways of saying the same thing. Choose the grain.',
    );
    expect(Array.from(p.querySelectorAll('em')).map((e) => e.textContent)).toEqual(['themselves', 'other']);
  });

  it('labels the select and offers the three modes in v4 order (v4 :26-43, :128-143)', async () => {
    const f = await rendered();
    expect(host(f).querySelector('label[for="extraction-other-pass"]')?.textContent?.trim()).toBe(
      'Observations of others',
    );
    expect(Array.from(select(f).options).map((o) => [o.value, o.text])).toEqual([
      ['hybrid', 'Hybrid (recommended)'],
      ['fold', 'At the fold only'],
      ['turn', 'Every turn'],
    ]);
    expect(host(f).querySelector('label[for="extraction-floor"]')?.textContent?.trim()).toBe(
      'Per-turn importance floor',
    );
    expect(host(f).querySelector('label[for="extraction-per-subject"]')?.textContent?.trim()).toBe(
      'Fold candidates per subject',
    );
    // The card's own grid (the Rate limits section below it has a grid of its own — P4.D300).
    const helps = Array.from(host(f).querySelector('.grid')!.querySelectorAll('p.qt-text-xs')).map((p) =>
      p.textContent?.replace(/\s+/g, ' ').trim(),
    );
    expect(helps).toEqual([
      'In hybrid mode, only observations at or above this importance are noted turn by turn.',
      'At a fold, the most notes kept for any one observer and subject.',
    ]);
    expect([floor(f).getAttribute('min'), floor(f).getAttribute('max'), floor(f).getAttribute('step')]).toEqual([
      '0',
      '1',
      '0.05',
    ]);
    expect([perSubject(f).getAttribute('min'), perSubject(f).getAttribute('step')]).toEqual(['1', '1']);
  });

  it('describes the STORED mode beneath the select (v4 :119, :144)', async () => {
    expect(description(await rendered())).toBe(DESCRIPTIONS.hybrid);
    expect(
      description(await rendered({ memoryExtractionModeConfigGet: () => ({ settings: { ...STORED, otherPass: 'turn' } }) })),
    ).toBe(DESCRIPTIONS.turn);
  });

  it('choosing `fold` posts { otherPass: "fold" } only and re-describes (v4 :136)', async () => {
    const seen: Req[] = [];
    const f = await rendered({}, seen);
    await choose(f, 'fold');
    expect(sets(seen)).toEqual([{ type: 'memoryExtractionModeConfigSet', config: { otherPass: 'fold' } }]);
    expect(description(f)).toBe(DESCRIPTIONS.fold);
    expect(toasts()).toContain('success:Extraction settings saved');
  });

  it('a FAILED select save snaps back to the stored mode (R-E, v4 :84-87)', async () => {
    const f = await rendered({
      memoryExtractionModeConfigSet: () => {
        throw new CoreDispatchError({ kind: 'bad-request', message: 'Invalid option' });
      },
    });
    await choose(f, 'turn');
    expect(select(f).value).toBe('hybrid');
    expect(description(f)).toBe(DESCRIPTIONS.hybrid);
    expect(errorLine(f)).toBe('Invalid option');
    expect(toasts()).toContain('error:Invalid option');
  });

  it('disables the floor unless hybrid, and the per-subject under turn (v4 :159, :176)', async () => {
    const hybrid = await rendered();
    expect(floor(hybrid).disabled).toBe(false);
    expect(perSubject(hybrid).disabled).toBe(false);
    const fold = await rendered({ memoryExtractionModeConfigGet: () => ({ settings: { ...STORED, otherPass: 'fold' } }) });
    expect(floor(fold).disabled).toBe(true);
    expect(perSubject(fold).disabled).toBe(false);
    const turn = await rendered({ memoryExtractionModeConfigGet: () => ({ settings: { ...STORED, otherPass: 'turn' } }) });
    expect(floor(turn).disabled).toBe(true);
    expect(perSubject(turn).disabled).toBe(true);
  });

  it('refuses a floor of 1.5 inline and REVERTS the input — the draft is cleared first (v4 :93-99)', async () => {
    const seen: Req[] = [];
    const f = await rendered({}, seen);
    await commit(f, floor(f), '1.5');
    expect(errorLine(f)).toBe('The per-turn floor must be between 0 and 1');
    expect(sets(seen)).toEqual([]);
    expect(floor(f).value).toBe('0.75');
  });

  it('a BLANK floor saves 0 — v4\'s Number(\'\') asymmetry (v4 :95)', async () => {
    const seen: Req[] = [];
    const f = await rendered({}, seen);
    await commit(f, floor(f), '');
    expect(sets(seen)).toEqual([
      { type: 'memoryExtractionModeConfigSet', config: { perTurnOtherFloor: 0 } },
    ]);
  });

  it('a BLANK per-subject is refused, nothing posted (v4 :106-110)', async () => {
    const seen: Req[] = [];
    const f = await rendered({}, seen);
    await commit(f, perSubject(f), '');
    expect(errorLine(f)).toBe('Candidates per subject must be a whole number of at least 1');
    expect(sets(seen)).toEqual([]);
    expect(perSubject(f).value).toBe('3');
  });

  it('a fractional per-subject is refused with the same sentence (v4 :108)', async () => {
    const seen: Req[] = [];
    const f = await rendered({}, seen);
    await commit(f, perSubject(f), '2.5');
    expect(errorLine(f)).toBe('Candidates per subject must be a whole number of at least 1');
    expect(sets(seen)).toEqual([]);
  });

  it('an unchanged value posts nothing; a changed one posts the one key (v4 :101, :112)', async () => {
    const seen: Req[] = [];
    const f = await rendered({}, seen);
    await commit(f, floor(f), '0.75');
    await commit(f, perSubject(f), '3');
    expect(sets(seen)).toEqual([]);
    await commit(f, perSubject(f), '5');
    expect(sets(seen)).toEqual([
      { type: 'memoryExtractionModeConfigSet', config: { foldCandidatesPerSubject: 5 } },
    ]);
  });

  it('shows the server\'s clobbered echo after a one-key save (R-F, v4 :81)', async () => {
    const f = await rendered({
      memoryExtractionModeConfigGet: () => ({ settings: { otherPass: 'hybrid', perTurnOtherFloor: 0.5, foldCandidatesPerSubject: 7 } }),
      // v4's `.partial()` fills every default: a save of otherPass resets the others.
      memoryExtractionModeConfigSet: (req) => ({ settings: { ...STORED, ...req.config } }),
    });
    expect(floor(f).value).toBe('0.5');
    await choose(f, 'fold');
    expect(perSubject(f).value).toBe('3');
    expect(floor(f).value).toBe('0.75');
  });

  it('a READ failure shows the fallback, never the server\'s text (R-G, v4 :69)', async () => {
    const f = await rendered({
      memoryExtractionModeConfigGet: () => {
        throw new CoreDispatchError({ kind: 'internal', message: 'Something private' });
      },
    });
    expect(errorLine(f)).toBe('Failed to load extraction settings');
    expect(select(f).value).toBe('hybrid');
  });

  it('a write failure with no server sentence falls back (v4 :85)', async () => {
    const f = await rendered({
      memoryExtractionModeConfigSet: () => {
        throw new CoreDispatchError({ kind: 'internal', message: '' });
      },
    });
    await commit(f, perSubject(f), '4');
    expect(errorLine(f)).toBe('Failed to save extraction settings');
  });

  it('mounts the Rate limits section below its error line, in a bordered block (P4.D300, v4 :189)', async () => {
    const seen: Req[] = [];
    const f = await rendered({ memoryExtractionLimitsGet: () => ({ success: true, settings: { enabled: true } }) }, seen);
    const section = host(f).querySelector('qt-memory-extraction-limits-section') as HTMLElement;
    expect(section).not.toBeNull();
    expect(section.parentElement!.className).toBe('pt-4 border-t qt-border-default');
    expect(section.querySelector('h3')?.textContent).toBe('Rate limits');
    expect(seen.some((r) => r.type === 'memoryExtractionLimitsGet')).toBe(true);
  });
});
