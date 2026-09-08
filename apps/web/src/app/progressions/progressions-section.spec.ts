import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { coreStreamStub } from '../core/core-client.testing';
import { ToastService } from '../ui/toast.service';
import { idFromName } from './character-progressions.api';
import { ProgressionsSection } from './progressions-section';

/**
 * Parity specs for the Aurora progressions editor — v4's
 * `__tests__/unit/components/characters/progressions.test.tsx` at `25f534c0b`,
 * transcribed case for case, whose header rides across:
 *
 * > Two things matter here beyond "it renders". The first is the SAVE PAYLOAD:
 * > `PUT /api/v1/characters/[id]` replaces the whole `metadata` object, so a
 * > card that forgot to spread the user's other keys back in would quietly eat
 * > their fact sheet — the single most expensive bug this feature could ship.
 * > The second is the tombstone rule: an archived character's PUT is refused
 * > server-side, so the card must not offer edits it cannot make.
 *
 * v5 drives `characterGet` / `characterUpdate` through {@link CoreClient} where
 * v4 stubs `fetch`; the recorded PUTs below are the `characterUpdate` requests.
 *
 * @module progressions/progressions-section.spec
 */

const CHARACTER_ID = '11111111-1111-1111-1111-111111111111';

/**
 * A span that finished long ago, so the live line reads `complete` on any clock
 * a test run happens to have — v4's own note. A fixture straddling "now" would
 * flip between pending, active and complete depending on the hour the suite ran.
 */
const CANNON = {
  name: 'Cannon recharge',
  startTime: '2020-01-01T14:00:00Z',
  endTime: '2020-01-01T14:10:00Z',
  timeIncrement: 'minute',
  percentageReport: true,
  reportFrequency: 'turn',
  onComplete: 'keep',
};

type Req = { type: string; [k: string]: unknown };

class Toasts {
  readonly success: string[] = [];
  readonly errors: string[] = [];
  showSuccess(m: string): string {
    this.success.push(m);
    return 't';
  }
  showError(m: string): string {
    this.errors.push(m);
    return 't';
  }
}

/**
 * Stub the character read and record whatever `characterUpdate` the card sends.
 *
 * The read answers the DETAIL projection the real verb returns, because a stub
 * shaped otherwise would let a reader forgetting `metadata`'s home pass here and
 * fail in the app.
 */
function stub(
  metadata: unknown,
  archivedAt: string | null = null,
  puts: Array<Record<string, unknown>> = [],
  fail?: Error,
): CoreClient {
  return {
    ...coreStreamStub(),
    dispatchData: vi.fn(async (req: Req) => {
      if (req.type === 'characterUpdate') {
        if (fail) throw fail;
        puts.push(req['character'] as Record<string, unknown>);
        return {};
      }
      return { character: { id: CHARACTER_ID, name: 'Iris Volney', archivedAt, metadata } };
    }),
  } as unknown as CoreClient;
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function render(
  core: CoreClient,
  toasts: Toasts = new Toasts(),
): Promise<ComponentFixture<ProgressionsSection>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [ProgressionsSection],
    providers: [
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: core },
      { provide: ToastService, useValue: toasts },
    ],
  });
  const fixture = TestBed.createComponent(ProgressionsSection);
  fixture.componentRef.setInput('characterId', CHARACTER_ID);
  fixture.componentRef.setInput('characterName', 'Iris Volney');
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

const el = (f: ComponentFixture<unknown>) => f.nativeElement as HTMLElement;
const text = (f: ComponentFixture<unknown>) => el(f).textContent ?? '';
const buttons = (f: ComponentFixture<unknown>) =>
  Array.from(el(f).querySelectorAll('button') as NodeListOf<HTMLButtonElement>);
const byLabel = (f: ComponentFixture<unknown>, label: string) =>
  buttons(f).find((b) => b.textContent?.trim() === label)!;
const byAria = (f: ComponentFixture<unknown>, prefix: string) =>
  buttons(f).filter((b) => (b.getAttribute('aria-label') ?? '').startsWith(prefix));
const input = (f: ComponentFixture<unknown>, placeholder: string) =>
  el(f).querySelector(`[placeholder="${placeholder}"]`) as HTMLInputElement;

/** Fire the event Angular's `(input)` / `(change)` bindings listen for. */
function type(node: HTMLInputElement | HTMLSelectElement, value: string, event = 'input'): void {
  node.value = value;
  node.dispatchEvent(new Event(event, { bubbles: true }));
}

async function click(f: ComponentFixture<unknown>, node: HTMLElement): Promise<void> {
  node.click();
  await settle(f);
}

describe('idFromName', () => {
  it('coerces a display name into a legal identifier', () => {
    expect(idFromName('Cannon recharge')).toBe('cannon-recharge');
    expect(idFromName('  Pregnancy!  ')).toBe('pregnancy');
  });

  it('guarantees a leading letter, which the pattern demands', () => {
    expect(idFromName('9 lives')).toBe('p-9-lives');
    expect(idFromName('!!!')).toBe('progression');
    expect(idFromName('')).toBe('progression');
  });

  it('stays within the 64-character ceiling', () => {
    expect(idFromName('x'.repeat(200)).length).toBeLessThanOrEqual(64);
  });
});

describe('ProgressionsSection — the header', () => {
  it('carries v4’s heading and its whole explanatory paragraph, with the character named', async () => {
    const fixture = await render(stub({}));
    expect(text(fixture)).toContain('Progressions');
    const paragraph = el(fixture)
      .querySelector('p.qt-text-small')!
      .textContent!.replace(/\s+/g, ' ')
      .trim();
    // v4's bytes, extracted from `ProgressionsSection.tsx` at `25f534c0b` and
    // resolved the way React resolves them (`&rsquo;` → ’, the code element's
    // tags dropped, the character's name interpolated).
    expect(paragraph).toBe(
      'Spans of time Iris Volney is carrying — a gestation, a recharging weapon, a fermentation. ' +
        'Each turn, Quilltap works out how far along it is and tells them so, without the model ' +
        'having to do arithmetic or remember that time has passed. They live in the vault’s ' +
        'metadata.json, where a custom tool can read them and adjust them; the model itself never ' +
        'sets one.',
    );
  });

  it('uses the typographic apostrophe in “the vault’s”, never an ASCII one', async () => {
    const paragraph = el(await render(stub({}))).querySelector('p.qt-text-small')!.textContent!;
    expect(paragraph).toContain('the vault’s');
    expect(paragraph).not.toContain("vault's");
  });
});

describe('ProgressionsSection — the list', () => {
  it('lists an entry with its id, its state and the line the character reads', async () => {
    const fixture = await render(stub({ faction: 'Ordo Aurum', progressions: { cannon: CANNON } }));
    expect(text(fixture)).toContain('Cannon recharge');
    // The id's own `code` element — the header paragraph carries one too.
    expect(el(fixture).querySelector('code.qt-text-secondary')!.textContent).toBe('cannon');
    // The cannon finished in 2020; on any clock a test run has, it is complete.
    expect(text(fixture)).toContain('complete');
    expect(text(fixture)).toMatch(/Cannon recharge: complete;/);
  });

  it('offers the empty state to a character carrying nothing', async () => {
    const fixture = await render(stub({ faction: 'Ordo Aurum' }));
    expect(text(fixture)).toMatch(/Nothing in progress/);
  });

  /**
   * A RECORDED DIVERGENCE, filed upstream. v4 writes
   * `<code>{invalidIds.join('</code>, <code>')}</code>` — a JSX EXPRESSION, so
   * React escapes it: measured at `25f534c0b` with `react-dom/server`, two bad
   * ids render the visible text `broken</code>, <code>other`. v5 renders them
   * as separate `<code>` elements joined by a comma, which is plainly what the
   * line means. Identical for ONE id, which is the common case and the only one
   * v4's own suite covers.
   */
  it('lists several unparseable ids as separate code elements, not as escaped markup', async () => {
    const fixture = await render(
      stub({
        progressions: {
          'Cannon Recharge': CANNON,
          'Fuse Timer': CANNON,
        },
      }),
    );
    expect(text(fixture)).toMatch(/2 entries in/);
    expect(text(fixture)).toMatch(/are being skipped/);
    expect(text(fixture)).toMatch(/mend them/);
    // The tell: v4's rendering would put this literal text on the screen.
    expect(text(fixture)).not.toContain('</code>');
    const codes = Array.from(el(fixture).querySelectorAll('code')).map((c) => c.textContent);
    expect(codes).toContain('Cannon Recharge');
    expect(codes).toContain('Fuse Timer');
  });

  it('uses v4’s singular wording for exactly one', async () => {
    const fixture = await render(stub({ progressions: { 'Cannon Recharge': CANNON } }));
    expect(text(fixture)).toMatch(/One entry in/);
    expect(text(fixture)).toMatch(/is being skipped/);
    expect(text(fixture)).toMatch(/mend it/);
  });

  it('says so when an entry in the vault could not be read', async () => {
    const fixture = await render(
      stub({
        progressions: { cannon: CANNON, broken: { ...CANNON, endTime: '2019-01-01T00:00:00Z' } },
      }),
    );
    expect(text(fixture)).toMatch(/could not be read/);
    expect(text(fixture)).toContain('Cannon recharge');
  });
});

describe('ProgressionsSection — archived characters are tombstones', () => {
  it('refuses to add, edit or delete', async () => {
    const fixture = await render(
      stub({ progressions: { cannon: CANNON } }, '2026-09-01T00:00:00Z'),
    );
    expect(text(fixture)).toMatch(/is archived, so this card is read-only/);
    expect(byLabel(fixture, '+ Add Progression').disabled).toBe(true);
    expect(byAria(fixture, 'Edit progression')[0].disabled).toBe(true);
    expect(byAria(fixture, 'Delete progression')[0].disabled).toBe(true);
  });
});

describe('ProgressionsSection — the save payload', () => {
  it('spreads every other metadata key back in, untouched', async () => {
    const puts: Array<Record<string, unknown>> = [];
    const fixture = await render(
      stub(
        {
          faction: 'Ordo Aurum',
          hasAnsibleAccess: true,
          clearanceLevel: 3,
          progressions: { cannon: CANNON },
        },
        null,
        puts,
      ),
    );

    await click(fixture, byAria(fixture, 'Delete progression')[0]);
    await click(fixture, byLabel(fixture, 'Delete'));

    expect(puts).toHaveLength(1);
    expect(puts[0]['metadata']).toEqual({
      faction: 'Ordo Aurum',
      hasAnsibleAccess: true,
      clearanceLevel: 3,
    });
  });

  it('drops the reserved key entirely when the last progression goes', async () => {
    const puts: Array<Record<string, unknown>> = [];
    const fixture = await render(
      stub({ faction: 'Ordo Aurum', progressions: { cannon: CANNON } }, null, puts),
    );

    await click(fixture, byAria(fixture, 'Delete progression')[0]);
    await click(fixture, byLabel(fixture, 'Delete'));

    expect(puts).toHaveLength(1);
    expect(puts[0]['metadata']).not.toHaveProperty('progressions');
  });

  it('keeps the character’s other progressions when one is removed', async () => {
    const puts: Array<Record<string, unknown>> = [];
    const fixture = await render(
      stub({ progressions: { cannon: CANNON, fuse: { ...CANNON, name: 'Fuse' } } }, null, puts),
    );

    await click(fixture, byAria(fixture, 'Delete progression')[0]);
    await click(fixture, byLabel(fixture, 'Delete'));

    expect(puts).toHaveLength(1);
    const written = (puts[0]['metadata'] as Record<string, unknown>)['progressions'] as Record<
      string,
      unknown
    >;
    expect(Object.keys(written)).toEqual(['fuse']);
  });

  it('names the failure through the toast rather than swallowing it', async () => {
    const toasts = new Toasts();
    const fixture = await render(
      stub({ progressions: { cannon: CANNON } }, null, [], new Error('nope')),
      toasts,
    );
    await click(fixture, byAria(fixture, 'Delete progression')[0]);
    await click(fixture, byLabel(fixture, 'Delete'));
    expect(toasts.errors).toEqual(['nope']);
  });
});

describe('ProgressionsSection — the editor modal', () => {
  it('opens on Add and coerces an id from the name', async () => {
    const fixture = await render(stub({ progressions: {} }));

    await click(fixture, byLabel(fixture, '+ Add Progression'));
    type(input(fixture, 'Cannon recharge'), 'Cannon recharge');
    await settle(fixture);
    expect(input(fixture, 'cannon').value).toBe('cannon-recharge');
  });

  it('fixes the id on an existing entry — a tool file addresses it', async () => {
    const fixture = await render(stub({ progressions: { cannon: CANNON } }));

    await click(fixture, byAria(fixture, 'Edit progression')[0]);
    expect(input(fixture, 'cannon').disabled).toBe(true);
    expect(input(fixture, 'cannon').value).toBe('cannon');
  });

  it('saves an edit with updatedAt stamped, so the next turn reports it', async () => {
    const puts: Array<Record<string, unknown>> = [];
    const fixture = await render(stub({ progressions: { cannon: CANNON } }, null, puts));

    await click(fixture, byAria(fixture, 'Edit progression')[0]);
    type(input(fixture, 'Cannon recharge'), 'Main gun recharge');
    await settle(fixture);
    await click(fixture, byLabel(fixture, 'Save changes'));

    expect(puts).toHaveLength(1);
    const written = (puts[0]['metadata'] as Record<string, unknown>)['progressions'] as Record<
      string,
      Record<string, unknown>
    >;
    expect(written['cannon']['name']).toBe('Main gun recharge');
    expect(typeof written['cannon']['updatedAt']).toBe('string');
  });

  it('refuses a create whose id collides with one already carried', async () => {
    const fixture = await render(stub({ progressions: { cannon: CANNON } }));

    await click(fixture, byLabel(fixture, '+ Add Progression'));
    type(input(fixture, 'Cannon recharge'), 'cannon');
    await settle(fixture);
    await click(fixture, byLabel(fixture, 'Add progression'));

    expect(text(fixture)).toMatch(/already carries a progression/);
  });

  it('refuses a span that ends before it begins, and does not PUT', async () => {
    const puts: Array<Record<string, unknown>> = [];
    const fixture = await render(stub({ progressions: {} }, null, puts));

    await click(fixture, byLabel(fixture, '+ Add Progression'));
    type(input(fixture, 'Cannon recharge'), 'Fuse');
    const times = Array.from(
      el(fixture).querySelectorAll('input[type="datetime-local"]') as NodeListOf<HTMLInputElement>,
    );
    type(times[0], '2026-09-08T14:00');
    type(times[1], '2026-09-08T13:00');
    await settle(fixture);
    await click(fixture, byLabel(fixture, 'Add progression'));

    expect(text(fixture)).toMatch(/strictly after startTime/);
    expect(puts).toHaveLength(0);
  });

  it('shows the live preview line the character would read', async () => {
    const fixture = await render(stub({ progressions: { cannon: CANNON } }));
    await click(fixture, byAria(fixture, 'Edit progression')[0]);
    expect(text(fixture)).toMatch(/As Cannon recharge would read it now/);
  });

  it('offers every documented placeholder in the legend', async () => {
    const fixture = await render(stub({ progressions: { cannon: CANNON } }));
    await click(fixture, byAria(fixture, 'Edit progression')[0]);
    for (const token of ['{{elapsedWhole}}', '{{percent}}', '{{quantity}}', '{{increment}}']) {
      expect(text(fixture)).toContain(token);
    }
  });

  it('writes the cadence back in the schema’s own grammar', async () => {
    const puts: Array<Record<string, unknown>> = [];
    const fixture = await render(stub({ progressions: { cannon: CANNON } }, null, puts));

    await click(fixture, byAria(fixture, 'Edit progression')[0]);
    const radios = Array.from(
      el(fixture).querySelectorAll('input[type="radio"]') as NodeListOf<HTMLInputElement>,
    );
    radios[2].click();
    await settle(fixture);
    type(el(fixture).querySelector('[aria-label="Cadence count"]') as HTMLInputElement, '2');
    type(
      el(fixture).querySelector('[aria-label="Cadence unit"]') as HTMLSelectElement,
      'd',
      'change',
    );
    await settle(fixture);
    await click(fixture, byLabel(fixture, 'Save changes'));

    expect(puts).toHaveLength(1);
    const written = (puts[0]['metadata'] as Record<string, unknown>)['progressions'] as Record<
      string,
      Record<string, unknown>
    >;
    expect(written['cannon']['reportFrequency']).toBe('2d');
  });
});
