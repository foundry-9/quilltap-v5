import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { CoreDispatchError } from '../../../core/core-contract';
import { ToastService } from '../../../ui/toast.service';
import { MemoryExtractionLimitsSection } from './memory-extraction-limits-section';

/**
 * P4.D300 — the extraction Rate limits section, v4 `components/tools/
 * memory-extraction-limits-section.tsx` (`b045dd986`) at `83334b9a5`. v4
 * shipped no test for it; these rows are the lane's own.
 */

interface Req {
  type: string;
  config?: Record<string, unknown>;
}
type Handler = (req: Req) => Record<string, unknown> | Promise<Record<string, unknown>>;

const STORED = { enabled: true, maxPerHour: 20, softStartFraction: 0.7, softFloor: 0.7 };

function render(over: Partial<Record<string, Handler>> = {}, seen: Req[] = []): ComponentFixture<MemoryExtractionLimitsSection> {
  const handlers: Record<string, Handler> = {
    memoryExtractionLimitsGet: () => ({ success: true, settings: { ...STORED } }),
    memoryExtractionLimitsSet: (req) => ({ success: true, settings: { ...STORED, ...req.config } }),
    ...over,
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
    imports: [MemoryExtractionLimitsSection],
    providers: [
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      { provide: CoreClient, useValue: client },
    ],
  });
  const fixture = TestBed.createComponent(MemoryExtractionLimitsSection);
  fixture.detectChanges();
  return fixture;
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 8; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function rendered(over: Partial<Record<string, Handler>> = {}, seen: Req[] = []) {
  const f = render(over, seen);
  await settle(f);
  return f;
}

const host = (f: ComponentFixture<unknown>) => f.nativeElement as HTMLElement;
const field = (f: ComponentFixture<unknown>, key: string) => host(f).querySelector(`#extraction-limits-${key}`) as HTMLInputElement;
const checkbox = (f: ComponentFixture<unknown>) => host(f).querySelector('input[type="checkbox"]') as HTMLInputElement;
const sets = (seen: Req[]) => seen.filter((r) => r.type === 'memoryExtractionLimitsSet').map((r) => r.config);
const errorLine = (f: ComponentFixture<unknown>) => host(f).querySelector('p.qt-text-destructive')?.textContent ?? null;
const toasts = () =>
  TestBed.inject(ToastService)
    .toasts()
    .map((t) => `${t.type}:${t.message}`);

async function commit(f: ComponentFixture<unknown>, key: string, value: string): Promise<void> {
  const input = field(f, key);
  input.value = value;
  input.dispatchEvent(new Event('input'));
  await settle(f);
  input.dispatchEvent(new Event('blur'));
  await settle(f);
}

describe('MemoryExtractionLimitsSection (P4.D300)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('the loading line, then the heading, the checkbox with its hint, three fields in order', async () => {
    const f = render();
    expect(host(f).textContent?.trim()).toBe('Loading extraction limits…');
    await settle(f);
    expect(host(f).querySelector('h3')?.textContent).toBe('Rate limits');
    const label = checkbox(f).closest('label')!;
    expect(label.querySelector('span > span')?.textContent).toBe('Limit how quickly a character takes notes');
    expect(label.querySelector('span > span.qt-text-xs')?.textContent).toBe('A guard against a long, busy evening filling the shelves. Off by default.');
    const rows = Array.from(host(f).querySelectorAll('.grid > div')).map((d) => [
      d.querySelector('input')!.id,
      d.querySelector('label')!.textContent,
      d.querySelector('p')!.textContent,
      d.querySelector('input')!.getAttribute('min'),
      d.querySelector('input')!.getAttribute('max'),
      d.querySelector('input')!.getAttribute('step'),
      (d.querySelector('input') as HTMLInputElement).value,
    ]);
    expect(rows).toEqual([
      ['extraction-limits-maxPerHour', 'Notes per hour', 'The most new notes one character may take in an hour.', '1', '10000', '1', '20'],
      ['extraction-limits-softStartFraction', 'Begin choosing at', 'The share of the hourly limit (0 to 1) past which only weightier notes are kept.', '0', '1', '0.05', '0.7'],
      ['extraction-limits-softFloor', 'Importance floor', 'Past that point, the least importance a new note must carry (0 to 1).', '0', '1', '0.05', '0.7'],
    ]);
  });

  it('the fields are disabled while the limiter is off (v4 defaults when the read carries none)', async () => {
    const f = await rendered({ memoryExtractionLimitsGet: () => ({ success: true }) });
    expect(checkbox(f).checked).toBe(false);
    for (const key of ['maxPerHour', 'softStartFraction', 'softFloor']) expect(field(f, key).disabled).toBe(true);
  });

  it('the checkbox posts { enabled } alone and toasts `Extraction limits saved`', async () => {
    const seen: Req[] = [];
    const f = await rendered({ memoryExtractionLimitsGet: () => ({ success: true, settings: { ...STORED, enabled: false } }) }, seen);
    checkbox(f).click();
    await settle(f);
    expect(sets(seen)).toEqual([{ enabled: true }]);
    expect(toasts()).toEqual(['success:Extraction limits saved']);
    expect(field(f, 'maxPerHour').disabled).toBe(false);
  });

  it('a FAILED checkbox save snaps it back; the section\'s own error line (L-3)', async () => {
    const f = await rendered({
      memoryExtractionLimitsSet: () => {
        throw new CoreDispatchError({ kind: 'bad-request', message: 'Refused' });
      },
    });
    checkbox(f).click();
    await settle(f);
    expect(checkbox(f).checked).toBe(true);
    expect(errorLine(f)).toBe('Refused');
  });

  it('a valid value saves ONE key; unchanged saves nothing', async () => {
    const seen: Req[] = [];
    const f = await rendered({}, seen);
    await commit(f, 'maxPerHour', '30');
    await commit(f, 'softFloor', '0.7');
    expect(sets(seen)).toEqual([{ maxPerHour: 30 }]);
  });

  it('out of bounds / not whole / blank: v4\'s sentence, the stored value back, no save', async () => {
    const seen: Req[] = [];
    const f = await rendered({}, seen);
    for (const [key, value, sentence] of [
      ['maxPerHour', '0', 'Notes per hour must be a whole number between 1 and 10000'],
      ['maxPerHour', '10001', 'Notes per hour must be a whole number between 1 and 10000'],
      ['maxPerHour', '2.5', 'Notes per hour must be a whole number between 1 and 10000'],
      ['softStartFraction', '1.5', 'Begin choosing at must be a number between 0 and 1'],
      ['softFloor', '', 'Importance floor must be a number between 0 and 1'],
    ] as const) {
      await commit(f, key, value);
      expect(errorLine(f)).toBe(sentence);
      expect(field(f, key).value).toBe(String(STORED[key]));
    }
    expect(sets(seen)).toEqual([]);
  });

  it('a read failure shows readErrorText on its own line', async () => {
    const f = await rendered({
      memoryExtractionLimitsGet: () => {
        throw new CoreDispatchError({ kind: 'internal', message: 'boom' });
      },
    });
    expect(errorLine(f)).toBe('Failed to load extraction limits');
  });
});
