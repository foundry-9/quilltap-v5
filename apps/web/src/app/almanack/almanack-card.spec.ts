import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { Subject } from 'rxjs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { ScopedEvent } from '../core/core-contract';
import { AlmanackCard } from './almanack-card';

/**
 * The Almanack card (v4 `components/tools/capabilities-report-card.tsx`).
 *
 * The claim worth the most here is the ORDERING one: v4 opens the progress
 * reader BEFORE it posts, and says why ("so the first phase event can't be
 * missed… keeps the bar honest from the very first tick"). It is exactly the
 * kind of sequencing a reimplementation gets backwards without any visible
 * symptom until a fast first phase goes missing, so it is pinned by recording
 * both events on one timeline — the P4.D29 lesson family (subscriptions that
 * seed on open are where SPA bugs hide).
 *
 * The second is that the DISPATCH's resolution drives the UI, not the `done`
 * frame: a generate that never receives a single frame still opens its report.
 */

interface Req {
  type: string;
  [k: string]: unknown;
}

const GENERATED = {
  success: true,
  reportId: 'rep-new',
  filename: 'almanack-2026-08-05.md',
  storageKey: 'reports/almanack.md',
  size: 2048,
  content: '# The Almanack\n\nAll cogs accounted for.',
};

const LISTED = [
  {
    id: 'rep-1',
    filename: 'almanack-2026-08-01.md',
    storageKey: 'reports/one.md',
    createdAt: '2026-08-01T10:00:00.000Z',
    size: 1536,
  },
];

function stubClient(
  hooks: {
    events$?: Subject<ScopedEvent>;
    /** Ordered trace of what happened, for the sequencing pin. */
    trace?: string[];
    reports?: unknown[];
    generate?: () => Promise<unknown>;
    get?: unknown;
    onDispatch?: (req: Req) => void;
  } = {},
): Partial<CoreClient> {
  const events$ = hooks.events$ ?? new Subject<ScopedEvent>();
  return {
    get events$() {
      hooks.trace?.push('subscribe');
      return events$.asObservable();
    },
    dispatchData: (async (req: Req) => {
      hooks.trace?.push(`dispatch:${req.type}`);
      hooks.onDispatch?.(req);
      if (req.type === 'systemAlmanackList') return { reports: hooks.reports ?? [] };
      if (req.type === 'systemAlmanackGenerate') {
        return hooks.generate ? await hooks.generate() : GENERATED;
      }
      if (req.type === 'systemAlmanackGet') return hooks.get ?? { content: '# stored' };
      return {};
    }) as CoreClient['dispatchData'],
  };
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<AlmanackCard>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [AlmanackCard],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(AlmanackCard);
  fixture.detectChanges();
  // Let the list query resolve before any beat looks at the rows.
  await settle(fixture);
  return fixture;
}

function text(fixture: ComponentFixture<unknown>): string {
  return (fixture.nativeElement as HTMLElement).textContent ?? '';
}

/** Let a dispatch / query promise chain settle and re-render. */
async function settle(fixture: ComponentFixture<unknown>, ticks = 6): Promise<void> {
  for (let i = 0; i < ticks; i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 0));
    fixture.detectChanges();
  }
}

describe('AlmanackCard', () => {
  afterEach(() => {
    document.body.style.overflow = '';
    vi.restoreAllMocks();
  });

  it('carries v4’s header, button and empty state verbatim', async () => {
    const fixture = await render(stubClient());
    const body = text(fixture);
    expect(body).toContain('The Almanack (System Report)');
    expect(body).toContain(
      'A compendium of the establishment — every cog, ledger and shelf, catalogued',
    );
    expect(body).toContain('Compile the Almanack');
    expect(body).toContain('Previous Editions');
    expect(body).toContain('No editions yet. Compile one to get started.');
  });

  it('lists previous editions with their formatted size, and a download link per row', async () => {
    const fixture = await render(stubClient({ reports: LISTED }));
    const host = fixture.nativeElement as HTMLElement;

    expect(text(fixture)).toContain('almanack-2026-08-01.md');
    expect(text(fixture)).toContain('1.5 KB');

    const anchor = host.querySelector('a[download]') as HTMLAnchorElement;
    expect(anchor.getAttribute('href')).toBe(
      '/api/v1/system/tools?action=capabilities-report-get&reportId=rep-1&download=true',
    );
    expect(anchor.getAttribute('download')).toBe('almanack-2026-08-01.md');
  });

  describe('generate', () => {
    let events$: Subject<ScopedEvent>;
    let trace: string[];

    beforeEach(() => {
      events$ = new Subject<ScopedEvent>();
      trace = [];
    });

    it('SUBSCRIBES BEFORE IT DISPATCHES (v4’s stated ordering)', async () => {
      const fixture = await render(stubClient({ events$, trace }));
      trace.length = 0;

      await fixture.componentInstance['generate']();

      expect(trace[0]).toBe('subscribe');
      expect(trace[1]).toBe('dispatch:systemAlmanackGenerate');
    });

    it('mints a progressId and sends it with the generate', async () => {
      const seen: Req[] = [];
      const fixture = await render(stubClient({ events$, onDispatch: (r) => seen.push(r) }));

      await fixture.componentInstance['generate']();

      const req = seen.find((r) => r.type === 'systemAlmanackGenerate');
      expect(typeof req?.['progressId']).toBe('string');
      expect((req?.['progressId'] as string).length).toBeGreaterThan(0);
    });

    it('shows the phase label from a matching frame, and ignores other scopes', async () => {
      let capturedId = '';
      const fixture = await render(
        stubClient({
          events$,
          onDispatch: (r) => {
            if (r.type === 'systemAlmanackGenerate') capturedId = r['progressId'] as string;
          },
          // Hold the dispatch open so the bar is still on screen while frames land.
          generate: () =>
            new Promise((resolve) => {
              setTimeout(() => resolve(GENERATED), 0);
            }),
        }),
      );

      const pending = fixture.componentInstance['generate']();
      await Promise.resolve();

      // A frame for somebody else's run must not move this bar. Note the
      // ellipsis: the bar's own segment CHIPS carry the trimmed labels, so only
      // the ellipsis form is unique to the status line beneath it.
      events$.next({
        progressId: 'a-different-run',
        kind: 'phase',
        key: 'ledgers',
        label: 'Auditing the ledgers…',
      });
      fixture.detectChanges();
      expect(text(fixture)).not.toContain('Auditing the ledgers…');

      events$.next({
        progressId: capturedId,
        kind: 'phase',
        key: 'machinery',
        index: 2,
        total: 7,
        label: 'Cataloguing the machinery…',
      });
      fixture.detectChanges();
      expect(text(fixture)).toContain('Cataloguing the machinery…');
      expect(text(fixture)).toContain('Compiling the Almanack…');

      await pending;
    });

    it('the DISPATCH’s resolution opens the viewer — no frame required', async () => {
      const fixture = await render(stubClient({ events$ }));

      await fixture.componentInstance['generate']();
      await settle(fixture);

      const host = fixture.nativeElement as HTMLElement;
      expect(host.querySelector('qt-almanack-report-dialog')).not.toBeNull();
      expect(host.querySelector('.qt-dialog-title')?.textContent?.trim()).toBe(
        'almanack-2026-08-05.md',
      );
      expect(text(fixture)).toContain('All cogs accounted for.');
    });

    it('clears the progress state and stops reading once settled', async () => {
      const fixture = await render(stubClient({ events$ }));

      await fixture.componentInstance['generate']();
      await settle(fixture);

      expect(events$.observed).toBe(false);
      expect(text(fixture)).toContain('Compile the Almanack');
      expect(text(fixture)).not.toContain('Compiling the Almanack…');
    });

    it('surfaces a failure inline and leaves no reader behind', async () => {
      const fixture = await render(
        stubClient({
          events$,
          generate: () => Promise.reject(new Error('The boiler is cold.')),
        }),
      );

      await fixture.componentInstance['generate']();
      await settle(fixture);

      expect(text(fixture)).toContain('The boiler is cold.');
      expect(events$.observed).toBe(false);
      // No viewer opens on a failure.
      expect(
        (fixture.nativeElement as HTMLElement).querySelector('qt-almanack-report-dialog'),
      ).toBeNull();
    });
  });

  describe('the per-row actions', () => {
    it('View fetches the stored content and opens the viewer on it', async () => {
      const fixture = await render(
        stubClient({ reports: LISTED, get: { content: '# An older edition' } }),
      );

      const host = fixture.nativeElement as HTMLElement;
      host.querySelector<HTMLButtonElement>('button[title="View Report"]')!.click();
      await settle(fixture);

      expect(host.querySelector('.qt-dialog-title')?.textContent?.trim()).toBe(
        'almanack-2026-08-01.md',
      );
      expect(text(fixture)).toContain('An older edition');
    });

    it('Delete asks first, with v4’s sentence, and does nothing when declined', async () => {
      const seen: Req[] = [];
      const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false);
      const fixture = await render(
        stubClient({ reports: LISTED, onDispatch: (r) => seen.push(r) }),
      );

      const host = fixture.nativeElement as HTMLElement;
      host.querySelector<HTMLButtonElement>('button[title="Delete Report"]')!.click();
      await settle(fixture);

      expect(confirm).toHaveBeenCalledWith(
        'Are you sure you want to delete "almanack-2026-08-01.md"?',
      );
      expect(seen.some((r) => r.type === 'systemAlmanackDelete')).toBe(false);
    });

    it('Delete dispatches once confirmed', async () => {
      const seen: Req[] = [];
      vi.spyOn(window, 'confirm').mockReturnValue(true);
      const fixture = await render(
        stubClient({ reports: LISTED, onDispatch: (r) => seen.push(r) }),
      );

      (fixture.nativeElement as HTMLElement)
        .querySelector<HTMLButtonElement>('button[title="Delete Report"]')!
        .click();
      await settle(fixture);

      expect(seen.find((r) => r.type === 'systemAlmanackDelete')?.['reportId']).toBe('rep-1');
    });
  });
});
