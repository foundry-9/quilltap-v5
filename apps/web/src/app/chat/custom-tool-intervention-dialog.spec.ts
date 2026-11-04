import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { coreStreamStub } from '../core/core-client.testing';
import { type ScopedEvent } from '../core/core-contract';
import { RealtimeService } from '../core/realtime.service';
import { Modal } from '../ui/modal';
import { ToastService } from '../ui/toast.service';
import type {
  InterventionHoldView,
  InterventionPreview,
  InterventionRunView,
} from './custom-tool-interventions.api';
import {
  CustomToolInterventionDialog,
  formatNumber,
  literalFrom,
  summarizeWhen,
} from './custom-tool-intervention-dialog';

/**
 * Parity spec against v4 `components/chat/CustomToolInterventionDialog.tsx` and
 * its own `__tests__/unit/components/chat/CustomToolInterventionDialog.test.tsx`
 * at the pin `236c04e35` (`839575d89` + `8ae8a127c` + `b02d902f3`): the
 * nineteen cases under v4's five describes and v4's own names, the fixtures
 * `runView` / `previewOf` / `holdView` transcribed from `:28-80`.
 *
 * One transport difference, by construction: v4 routes `fetch` by URL action;
 * this stub routes `CoreClient.dispatchData` by request `type` — the four
 * `InterventionWire` verbs — and records every call. v4's `waitFor` polling
 * stands in as `until`, on real timers (the 300 ms debounce runs for real, as
 * v4's does).
 *
 * v5-authored cases follow: the realtime hint, the older-server degradation
 * (R-6), the failed resolve, the dice bounds, the announcer, the effect boxes,
 * the pins count, the announcement line, and the three pure helpers.
 */

// ---------------------------------------------------------------------------
// Fixtures — shaped like InterventionHoldView (v4 lib/pascal/intervention/view.ts)
// ---------------------------------------------------------------------------

function runView(overrides: Partial<InterventionRunView> = {}): InterventionRunView {
  return {
    params: { bonus: 0 },
    rollForm: 'range',
    raw: 0.7,
    rollSpan: { min: 0, max: 1, maxExclusive: true },
    diceBreakdown: '',
    value: 70,
    valueSpan: { min: 0, max: 100, maxExclusive: true },
    outcomeIndex: 0,
    state: 'success',
    message: 'The house pays out.',
    title: 'Wager',
    visibility: 'public',
    announcer: 'pascal',
    effects: [],
    ...overrides,
  };
}

function previewOf(run: InterventionRunView): InterventionPreview {
  return { run, staleAnswer: false, clampedParams: [], warnings: [] };
}

const ALL_FIELDS: InterventionHoldView['eligible']['fields'] = [
  'roll',
  'value',
  'outcome',
  'message',
  'state',
  'title',
  'visibility',
  'announcer',
];

function holdView(id: string, overrides: Partial<InterventionHoldView> = {}): InterventionHoldView {
  return {
    id,
    tool: 'wager',
    title: `Wager ${id}`,
    characterName: 'Ada',
    entrance: 'llm',
    createdAt: 1_000,
    eligible: { fields: ALL_FIELDS, params: [], effects: [] },
    hasCharacter: true,
    definition: {
      parameters: {},
      outcomes: [
        { when: { gte: 50 }, state: 'success', message: 'The house pays out.' },
        { when: true, state: 'failure', message: 'The house keeps it.' },
      ],
      effects: [],
      revealOdds: true,
      hasLlm: false,
    },
    baseline: previewOf(runView({ title: `Wager ${id}` })),
    consults: [],
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// Dispatch routing
// ---------------------------------------------------------------------------

interface Call {
  type: string;
  chatId?: string;
  id?: string;
  overrides?: unknown;
  decision?: { kind: string };
}

interface World {
  holds: InterventionHoldView[];
  previewReply: InterventionPreview;
  reaskReply: InterventionPreview;
  calls: Call[];
  /** Set to make the list refuse (an Error) or the resolve refuse. */
  listError?: Error;
  resolveError?: Error;
  stream: ReturnType<typeof coreStreamStub>;
  client: Partial<CoreClient>;
  errors: string[];
}

function world(): World {
  const w: World = {
    holds: [],
    previewReply: previewOf(runView({ message: 'PREVIEWED MESSAGE' })),
    reaskReply: previewOf(runView({ message: 'RE-ASKED MESSAGE' })),
    calls: [],
    stream: coreStreamStub(),
    client: {},
    errors: [],
  };
  const dispatchData = vi.fn(async (req: Record<string, unknown>) => {
    const call = JSON.parse(JSON.stringify(req)) as Call;
    w.calls.push(call);
    switch (call.type) {
      case 'chatCustomToolInterventionsList':
        if (w.listError) throw w.listError;
        return { holds: w.holds };
      case 'chatCustomToolInterventionPreview':
        return { preview: w.previewReply };
      case 'chatCustomToolInterventionReask':
        return { preview: w.reaskReply, consults: [] };
      case 'chatCustomToolInterventionResolve':
        if (w.resolveError) throw w.resolveError;
        w.holds = w.holds.filter((h) => h.id !== call.id);
        return { resolved: call.decision?.kind };
      default:
        throw new Error(`no route for ${call.type}`);
    }
  });
  w.client = { ...w.stream, dispatchData: dispatchData as unknown as CoreClient['dispatchData'] };
  return w;
}

const callsTo = (w: World, type: string) => w.calls.filter((c) => c.type === type);

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

/** v4's `waitFor`, on real timers. */
async function until(fixture: ComponentFixture<unknown>, check: () => void, timeout = 3000): Promise<void> {
  const start = Date.now();
  for (;;) {
    await settle(fixture);
    try {
      check();
      return;
    } catch (err) {
      if (Date.now() - start > timeout) throw err;
      await new Promise((r) => setTimeout(r, 25));
    }
  }
}

async function renderDialog(w: World): Promise<ComponentFixture<CustomToolInterventionDialog>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [CustomToolInterventionDialog],
    providers: [
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      { provide: CoreClient, useValue: w.client },
    ],
  });
  TestBed.inject(RealtimeService);
  const toasts = TestBed.inject(ToastService);
  vi.spyOn(toasts, 'showError').mockImplementation((message: string) => {
    w.errors.push(message);
    return 'toast';
  });
  const fixture = TestBed.createComponent(CustomToolInterventionDialog);
  fixture.componentRef.setInput('chatId', 'chat-1');
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

const root = (f: ComponentFixture<unknown>) => f.nativeElement as HTMLElement;
const byTestId = (f: ComponentFixture<unknown>, id: string) => root(f).querySelector<HTMLElement>(`[data-testid="${id}"]`);
const byLabel = <T extends HTMLElement = HTMLElement>(f: ComponentFixture<unknown>, label: string) =>
  root(f).querySelector<T>(`[aria-label="${label}"]`);
const text = (f: ComponentFixture<unknown>) => root(f).textContent ?? '';

/** The element whose OWN trimmed text is exactly `t` (testing-library's `getByText`). */
function byText(f: ComponentFixture<unknown>, t: string | RegExp): HTMLElement | null {
  const all = [...root(f).querySelectorAll<HTMLElement>('*')];
  return (
    all.find((el) => {
      const own = [...el.childNodes]
        .filter((n) => n.nodeType === Node.TEXT_NODE)
        .map((n) => n.textContent ?? '')
        .join('')
        .replace(/\s+/g, ' ')
        .trim();
      return typeof t === 'string' ? own === t : t.test(own);
    }) ?? null
  );
}

function button(f: ComponentFixture<unknown>, label: string): HTMLButtonElement {
  const b = [...root(f).querySelectorAll<HTMLButtonElement>('button')].find(
    (el) => (el.textContent ?? '').trim() === label,
  );
  if (!b) throw new Error(`no button "${label}"`);
  return b;
}

function type(el: HTMLInputElement | HTMLTextAreaElement, value: string): void {
  el.value = value;
  el.dispatchEvent(new Event('input'));
}

function choose(el: HTMLSelectElement, value: string): void {
  el.value = value;
  el.dispatchEvent(new Event('change'));
}

afterEach(() => {
  TestBed.resetTestingModule();
  vi.restoreAllMocks();
});

// ---------------------------------------------------------------------------

describe('CustomToolInterventionDialog — what is shown', () => {
  it('renders nothing when nothing is held', async () => {
    const w = world();
    const fixture = await renderDialog(w);
    await until(fixture, () => expect(callsTo(w, 'chatCustomToolInterventionsList')).toHaveLength(1));
    expect(byTestId(fixture, 'custom-tool-intervention')).toBeNull();
    expect(text(fixture).trim()).toBe('');
  });

  it("asks for this chat's holds", async () => {
    const w = world();
    const fixture = await renderDialog(w);
    await until(fixture, () => expect(callsTo(w, 'chatCustomToolInterventionsList')).toHaveLength(1));
    expect(callsTo(w, 'chatCustomToolInterventionsList')[0]).toEqual({
      type: 'chatCustomToolInterventionsList',
      chatId: 'chat-1',
    });
  });

  it('shows the held run with its dealt preview', async () => {
    const w = world();
    w.holds = [holdView('a')];
    const fixture = await renderDialog(w);
    await until(fixture, () => expect(byText(fixture, 'Pascal holds: Wager a')).not.toBeNull());
    expect(byTestId(fixture, 'intervention-preview')!.textContent).toContain('The house pays out.');
    expect(text(fixture)).toMatch(/called by Ada/);
  });

  it('renders only the eligible fields', async () => {
    const w = world();
    w.holds = [holdView('a', { eligible: { fields: ['outcome', 'message'], params: [], effects: [] } })];
    const fixture = await renderDialog(w);
    await until(fixture, () => expect(byTestId(fixture, 'custom-tool-intervention')).not.toBeNull());

    expect(byText(fixture, 'The outcome')).not.toBeNull();
    expect(byText(fixture, 'The message')).not.toBeNull();
    // Hidden by the author: not rendered at all, and no params or effects either.
    for (const hidden of ['The roll', 'The value', 'The verdict', 'Parameters', 'Consequences', 'Presentation']) {
      expect(byText(fixture, hidden), hidden).toBeNull();
    }
  });

  it('shows the roll section when the roll is eligible', async () => {
    const w = world();
    w.holds = [holdView('a')];
    const fixture = await renderDialog(w);
    await until(fixture, () => expect(byTestId(fixture, 'custom-tool-intervention')).not.toBeNull());
    expect(byText(fixture, 'The roll')).not.toBeNull();
  });

  it('says what the draw and the value could have been', async () => {
    const w = world();
    w.holds = [holdView('a')];
    const fixture = await renderDialog(w);
    await until(fixture, () => expect(byTestId(fixture, 'custom-tool-intervention')).not.toBeNull());
    expect(byTestId(fixture, 'intervention-roll-span')!.textContent).toBe(
      'The table draws 0 up to (not including) 1.',
    );
    expect(byTestId(fixture, 'intervention-value-span')!.textContent!.trim()).toBe(
      'After the modifiers, it comes to 0 up to (not including) 100.',
    );
  });

  it('gives a dice roll its total and its faces', async () => {
    const dice = runView({
      rollForm: 'dice',
      notation: '3d6+2',
      raw: 12,
      value: 12,
      diceRolls: [3, 4, 3],
      diceBreakdown: '3d6+2: [3, 4, 3] + 2 = 12',
      rollSpan: { min: 5, max: 20 },
      valueSpan: { min: 5, max: 20 },
    });
    const w = world();
    w.holds = [
      holdView('a', {
        definition: { ...holdView('a').definition, diceSides: 6 },
        baseline: previewOf(dice),
      }),
    ];
    const fixture = await renderDialog(w);
    await until(fixture, () => expect(byTestId(fixture, 'custom-tool-intervention')).not.toBeNull());
    expect(byTestId(fixture, 'intervention-roll-span')!.textContent).toBe(
      '3d6+2 can come to 5 to 20, each die 1 to 6.',
    );
    expect(byTestId(fixture, 'intervention-value-span')).toBeNull();
  });
});

describe('CustomToolInterventionDialog — editing', () => {
  it('asks the server to recompute after the debounce and shows its message', async () => {
    const w = world();
    w.holds = [holdView('a')];
    const fixture = await renderDialog(w);
    await until(fixture, () => expect(byTestId(fixture, 'custom-tool-intervention')).not.toBeNull());

    // Pin the second outcome row.
    const row = byText(fixture, /→ failure/)!.closest('button') as HTMLButtonElement;
    row.click();
    fixture.detectChanges();

    // Debounced: not sent in the same tick.
    expect(callsTo(w, 'chatCustomToolInterventionPreview')).toHaveLength(0);

    await until(fixture, () => expect(callsTo(w, 'chatCustomToolInterventionPreview')).toHaveLength(1));
    expect(callsTo(w, 'chatCustomToolInterventionPreview')[0]).toMatchObject({
      chatId: 'chat-1',
      id: 'a',
      overrides: { outcomeIndex: 1 },
    });
    await until(fixture, () =>
      expect(byTestId(fixture, 'intervention-preview')!.textContent).toContain('PREVIEWED MESSAGE'),
    );
    expect(text(fixture)).toMatch(/1 change pinned/);
  });

  it('collapses a burst of edits into one request', async () => {
    const w = world();
    w.holds = [holdView('a')];
    const fixture = await renderDialog(w);
    await until(fixture, () => expect(byTestId(fixture, 'custom-tool-intervention')).not.toBeNull());

    const message = byLabel<HTMLTextAreaElement>(fixture, 'Message')!;
    type(message, 'Fi');
    type(message, 'Fix');
    type(message, 'Fixed.');

    await until(fixture, () => expect(callsTo(w, 'chatCustomToolInterventionPreview')).toHaveLength(1));
    await new Promise((r) => setTimeout(r, 400));
    expect(callsTo(w, 'chatCustomToolInterventionPreview')).toHaveLength(1);
    expect(callsTo(w, 'chatCustomToolInterventionPreview')[0]).toEqual({
      type: 'chatCustomToolInterventionPreview',
      chatId: 'chat-1',
      id: 'a',
      overrides: { message: 'Fixed.' },
    });
  });

  it('"Deal it" sends the pins as a deal decision', async () => {
    const w = world();
    w.holds = [holdView('a')];
    const fixture = await renderDialog(w);
    await until(fixture, () => expect(byTestId(fixture, 'custom-tool-intervention')).not.toBeNull());

    type(byLabel<HTMLTextAreaElement>(fixture, 'Message')!, 'Fixed.');
    await until(fixture, () => expect(callsTo(w, 'chatCustomToolInterventionPreview')).toHaveLength(1));
    await until(fixture, () => expect(button(fixture, 'Deal it').disabled).toBe(false));
    button(fixture, 'Deal it').click();

    await until(fixture, () => expect(callsTo(w, 'chatCustomToolInterventionResolve')).toHaveLength(1));
    expect(callsTo(w, 'chatCustomToolInterventionResolve')[0]).toEqual({
      type: 'chatCustomToolInterventionResolve',
      chatId: 'chat-1',
      id: 'a',
      decision: { kind: 'deal', overrides: { message: 'Fixed.' } },
    });
  });
});

describe('CustomToolInterventionDialog — several holds, and reopening', () => {
  it('shows the oldest and says how many more wait', async () => {
    const w = world();
    w.holds = [holdView('first'), holdView('second'), holdView('third')];
    const fixture = await renderDialog(w);
    await until(fixture, () => expect(byText(fixture, 'Pascal holds: Wager first')).not.toBeNull());
    expect(byText(fixture, 'Pascal holds: Wager second')).toBeNull();
    expect(text(fixture)).toMatch(/2 more waiting/);
  });

  it('says "1 more waiting" for a single follower', async () => {
    const w = world();
    w.holds = [holdView('first'), holdView('second')];
    const fixture = await renderDialog(w);
    await until(fixture, () => expect(byText(fixture, 'Pascal holds: Wager first')).not.toBeNull());
    expect(text(fixture)).toMatch(/1 more waiting/);
    // v4's bytes: a leading space inside the queue span, after "the deck.".
    expect(root(fixture).querySelector('.qt-intervention-queue')!.textContent).toBe(' 1 more waiting.');
    expect(text(fixture).replace(/\s+/g, ' ')).toContain('you touched the deck. 1 more waiting.');
  });

  it('moves on to the next hold once the first is answered', async () => {
    const w = world();
    w.holds = [holdView('first'), holdView('second')];
    const fixture = await renderDialog(w);
    await until(fixture, () => expect(byText(fixture, 'Pascal holds: Wager first')).not.toBeNull());

    button(fixture, 'As rolled').click();
    await until(fixture, () => expect(byText(fixture, 'Pascal holds: Wager second')).not.toBeNull());
    expect(text(fixture)).not.toMatch(/more waiting/);
  });

  it('reopens after a remount, re-reading the holds', async () => {
    const w = world();
    w.holds = [holdView('a')];
    const first = await renderDialog(w);
    await until(first, () => expect(byText(first, 'Pascal holds: Wager a')).not.toBeNull());
    first.destroy();
    expect(first.componentRef.hostView.destroyed).toBe(true);

    const second = await renderDialog(w);
    await until(second, () => expect(byText(second, 'Pascal holds: Wager a')).not.toBeNull());
    expect(callsTo(w, 'chatCustomToolInterventionsList').length).toBeGreaterThanOrEqual(2);
  });
});

describe('CustomToolInterventionDialog — answering without dealing', () => {
  it('Escape does not dismiss a held roll (v4 :280 closeOnEscape={false})', async () => {
    const w = world();
    w.holds = [holdView('a')];
    const fixture = await renderDialog(w);
    await until(fixture, () => expect(byTestId(fixture, 'custom-tool-intervention')).not.toBeNull());
    // The one owner of a document Escape here is qt-modal's — and it is off.
    const modal = fixture.debugElement.query(By.directive(Modal)).componentInstance as Modal;
    expect((modal as unknown as { closeOnEscape(): boolean }).closeOnEscape()).toBe(false);
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await settle(fixture);
    expect(byTestId(fixture, 'custom-tool-intervention')).not.toBeNull();
    expect(callsTo(w, 'chatCustomToolInterventionResolve')).toEqual([]);
  });

  it('"As rolled" posts an as-rolled decision', async () => {
    const w = world();
    w.holds = [holdView('a')];
    const fixture = await renderDialog(w);
    await until(fixture, () => expect(byTestId(fixture, 'custom-tool-intervention')).not.toBeNull());

    button(fixture, 'As rolled').click();
    await until(fixture, () => expect(callsTo(w, 'chatCustomToolInterventionResolve')).toHaveLength(1));
    expect(callsTo(w, 'chatCustomToolInterventionResolve')[0]).toMatchObject({
      id: 'a',
      decision: { kind: 'as-rolled' },
    });
    await until(fixture, () => expect(byTestId(fixture, 'custom-tool-intervention')).toBeNull());
  });

  it('"Set the table aside" asks first, and only then vetoes', async () => {
    const w = world();
    w.holds = [holdView('a')];
    const fixture = await renderDialog(w);
    await until(fixture, () => expect(byTestId(fixture, 'custom-tool-intervention')).not.toBeNull());

    button(fixture, 'Set the table aside').click();
    fixture.detectChanges();
    expect(text(fixture)).toMatch(/Sweep the cards away\? Nothing is announced and nothing changes\./);
    expect(callsTo(w, 'chatCustomToolInterventionResolve')).toHaveLength(0);

    button(fixture, 'Set it aside').click();
    await until(fixture, () => expect(callsTo(w, 'chatCustomToolInterventionResolve')).toHaveLength(1));
    expect(callsTo(w, 'chatCustomToolInterventionResolve')[0]).toMatchObject({ id: 'a', decision: { kind: 'veto' } });
    await until(fixture, () => expect(byTestId(fixture, 'custom-tool-intervention')).toBeNull());
  });

  it('"Keep the table" backs out of the veto without posting', async () => {
    const w = world();
    w.holds = [holdView('a')];
    const fixture = await renderDialog(w);
    await until(fixture, () => expect(byTestId(fixture, 'custom-tool-intervention')).not.toBeNull());

    button(fixture, 'Set the table aside').click();
    fixture.detectChanges();
    button(fixture, 'Keep the table').click();
    fixture.detectChanges();
    expect(button(fixture, 'As rolled')).toBeTruthy();
    expect(callsTo(w, 'chatCustomToolInterventionResolve')).toHaveLength(0);
  });
});

describe('CustomToolInterventionDialog — a consult the author hid', () => {
  function hiddenConsultHold(): InterventionHoldView {
    const base = holdView('a');
    return { ...base, definition: { ...base.definition, hasLlm: true } };
  }

  it('says when a change has put the answer out of date, and re-asks on request', async () => {
    const w = world();
    w.holds = [hiddenConsultHold()];
    w.previewReply = { ...previewOf(runView({ message: 'OLD ANSWER' })), staleAnswer: true };
    const fixture = await renderDialog(w);
    await until(fixture, () => expect(byTestId(fixture, 'custom-tool-intervention')).not.toBeNull());

    // Neither the prompt nor the answer is rendered, but the oracle is.
    expect(byText(fixture, 'Prompt')).toBeNull();
    expect(byTestId(fixture, 'intervention-hidden-consult')).not.toBeNull();
    expect(text(fixture)).toMatch(/still fits the question/);

    (byText(fixture, /→ failure/)!.closest('button') as HTMLButtonElement).click();
    await until(fixture, () => expect(text(fixture)).toMatch(/altered what the oracle would be asked/));

    button(fixture, 'Re-ask').click();
    await until(fixture, () => expect(callsTo(w, 'chatCustomToolInterventionReask')).toHaveLength(1));
    expect(callsTo(w, 'chatCustomToolInterventionReask')[0]).toMatchObject({ id: 'a', overrides: { outcomeIndex: 1 } });
    await until(fixture, () =>
      expect(byLabel<HTMLTextAreaElement>(fixture, 'Message')!.value).toBe('RE-ASKED MESSAGE'),
    );
  });

  it('shows no oracle section for a tool that consults none', async () => {
    const w = world();
    w.holds = [holdView('a')];
    const fixture = await renderDialog(w);
    await until(fixture, () => expect(byTestId(fixture, 'custom-tool-intervention')).not.toBeNull());
    expect(byTestId(fixture, 'intervention-hidden-consult')).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// v5-authored (P4.D346 item 9)
// ---------------------------------------------------------------------------

describe('CustomToolInterventionDialog — v5 transport and degradation', () => {
  it('a customToolInterventions hint for this chat re-reads the holds', async () => {
    const w = world();
    const fixture = await renderDialog(w);
    w.stream.connection.set('open');
    await settle(fixture);
    const before = callsTo(w, 'chatCustomToolInterventionsList').length;
    w.holds = [holdView('a')];
    w.stream.frames.next({ v: 1, topic: 'customToolInterventions', id: 'chat-1', at: 1 } as unknown as ScopedEvent);
    await until(fixture, () => expect(byText(fixture, 'Pascal holds: Wager a')).not.toBeNull());
    expect(callsTo(w, 'chatCustomToolInterventionsList').length).toBeGreaterThan(before);
  });

  it('a hint for another chat is ignored', async () => {
    const w = world();
    const fixture = await renderDialog(w);
    w.stream.connection.set('open');
    await settle(fixture);
    const before = callsTo(w, 'chatCustomToolInterventionsList').length;
    w.stream.frames.next({ v: 1, topic: 'customToolInterventions', id: 'chat-2', at: 1 } as unknown as ScopedEvent);
    await settle(fixture);
    expect(callsTo(w, 'chatCustomToolInterventionsList').length).toBe(before);
  });

  it('a failed resolve toasts "Pascal could not settle that roll" and still moves on', async () => {
    const w = world();
    w.holds = [holdView('a'), holdView('b')];
    w.resolveError = new Error('That run has already been settled');
    const fixture = await renderDialog(w);
    await until(fixture, () => expect(byText(fixture, 'Pascal holds: Wager a')).not.toBeNull());
    // Settled elsewhere: the server's list no longer carries it.
    w.holds = [holdView('b')];
    button(fixture, 'As rolled').click();
    await until(fixture, () => expect(byText(fixture, 'Pascal holds: Wager b')).not.toBeNull());
    expect(w.errors).toEqual(['That run has already been settled']);

    const w2 = world();
    w2.holds = [holdView('a')];
    w2.resolveError = 'not an Error' as unknown as Error;
    const second = await renderDialog(w2);
    await until(second, () => expect(byText(second, 'Pascal holds: Wager a')).not.toBeNull());
    button(second, 'As rolled').click();
    await until(second, () => expect(w2.errors).toEqual(['Pascal could not settle that roll']));
  });

  it('a failed preview toasts "Pascal could not reckon that change", a failed re-ask "The oracle would not answer again"', async () => {
    const w = world();
    w.holds = [{ ...holdView('a'), definition: { ...holdView('a').definition, hasLlm: true } }];
    const fixture = await renderDialog(w);
    await until(fixture, () => expect(byTestId(fixture, 'custom-tool-intervention')).not.toBeNull());
    (w.client.dispatchData as unknown as ReturnType<typeof vi.fn>).mockImplementation(async () => {
      throw 'refused';
    });
    type(byLabel<HTMLTextAreaElement>(fixture, 'Message')!, 'Fixed.');
    await until(fixture, () => expect(w.errors).toContain('Pascal could not reckon that change'));
    button(fixture, 'Re-ask').click();
    await until(fixture, () => expect(w.errors).toContain('The oracle would not answer again'));
  });
});

describe('CustomToolInterventionDialog — v5-authored sections', () => {
  it('the dice inputs are bounded 1..diceSides and pin only a complete set of finite faces', async () => {
    const dice = runView({ rollForm: 'dice', diceRolls: [3, 4], diceBreakdown: '2d6: [3, 4] = 7', raw: 7, value: 7 });
    const w = world();
    w.holds = [holdView('a', { definition: { ...holdView('a').definition, diceSides: 6 }, baseline: previewOf(dice) })];
    const fixture = await renderDialog(w);
    await until(fixture, () => expect(byTestId(fixture, 'custom-tool-intervention')).not.toBeNull());
    const die2 = byLabel<HTMLInputElement>(fixture, 'Die 2')!;
    expect(die2.getAttribute('min')).toBe('1');
    expect(die2.getAttribute('max')).toBe('6');
    expect(byLabel(fixture, 'Raw draw')).toBeNull();
    type(die2, '6');
    await until(fixture, () => expect(callsTo(w, 'chatCustomToolInterventionPreview')).toHaveLength(1));
    expect(callsTo(w, 'chatCustomToolInterventionPreview')[0]!.overrides).toEqual({ dice: [3, 6] });
  });

  it('the announcer select is absent when the hold has no character', async () => {
    const w = world();
    w.holds = [holdView('a', { hasCharacter: false, characterName: null })];
    const fixture = await renderDialog(w);
    await until(fixture, () => expect(byTestId(fixture, 'custom-tool-intervention')).not.toBeNull());
    expect(text(fixture)).toContain('Heard by');
    expect(text(fixture)).not.toContain('Announced by');
    expect(text(fixture)).toMatch(/called by a character\./);

    const named = world();
    named.holds = [holdView('a')];
    const withChar = await renderDialog(named);
    await until(withChar, () => expect(text(withChar)).toContain('Announced by'));
    const options = [...root(withChar).querySelectorAll('option')].map((o) => (o.textContent ?? '').trim());
    expect(options).toContain('Ada');
  });

  it('the consequences render a value box, an items box or a log box by kind, and only while the effect fires', async () => {
    const run = runView({
      effects: [
        { index: 0, kind: 'write', label: 'state.n', fired: true, value: 2 },
        { index: 1, kind: 'wardrobe', label: 'wear', fired: true, items: ['Hat', 'Coat'] },
        { index: 2, kind: 'log', label: 'log', fired: true, log: 'A line.' },
        { index: 3, kind: 'inventory', label: 'pick_up', fired: false, skipped: 'condition not met' },
      ],
    });
    const w = world();
    w.holds = [holdView('a', { eligible: { fields: [], params: [], effects: [0, 1, 2, 3] }, baseline: previewOf(run) })];
    // The server's recompute keeps the four effects (the default reply carries none).
    w.previewReply = previewOf(run);
    const fixture = await renderDialog(w);
    await until(fixture, () => expect(byText(fixture, 'Consequences')).not.toBeNull());
    expect(byLabel<HTMLInputElement>(fixture, 'Effect 1 value')!.value).toBe('2');
    expect(byLabel<HTMLTextAreaElement>(fixture, 'Effect 2 items')!.value).toBe('Hat\nCoat');
    expect(byLabel<HTMLTextAreaElement>(fixture, 'Effect 3 log line')!.value).toBe('A line.');
    expect(byLabel(fixture, 'Effect 4 items')).toBeNull();
    expect(text(fixture)).toContain('does not fire (condition not met)');

    type(byLabel<HTMLInputElement>(fixture, 'Effect 1 value')!, 'true');
    choose(byLabel<HTMLSelectElement>(fixture, 'Effect 4 mode')!, 'on');
    await until(fixture, () => expect(callsTo(w, 'chatCustomToolInterventionPreview')).toHaveLength(1));
    expect(callsTo(w, 'chatCustomToolInterventionPreview')[0]!.overrides).toEqual({
      effects: { '0': { mode: 'auto', value: true }, '3': { mode: 'on' } },
    });
    // Auto on a pin that holds only a mode drops it; an empty map drops `effects`.
    choose(byLabel<HTMLSelectElement>(fixture, 'Effect 4 mode')!, 'auto');
    type(byLabel<HTMLInputElement>(fixture, 'Effect 1 value')!, '2');
    await until(fixture, () => expect(callsTo(w, 'chatCustomToolInterventionPreview')).toHaveLength(2));
    expect(callsTo(w, 'chatCustomToolInterventionPreview')[1]!.overrides).toEqual({
      effects: { '0': { mode: 'auto', value: 2 } },
    });
  });

  it("the preview heading counts the pins in v4's words", async () => {
    const w = world();
    w.holds = [holdView('a')];
    const fixture = await renderDialog(w);
    await until(fixture, () => expect(byTestId(fixture, 'custom-tool-intervention')).not.toBeNull());
    expect(byText(fixture, 'As it will be announced')).not.toBeNull();
    (byText(fixture, /→ failure/)!.closest('button') as HTMLButtonElement).click();
    fixture.detectChanges();
    expect(byText(fixture, 'As it will be announced — 1 change pinned')).not.toBeNull();
    type(byLabel<HTMLTextAreaElement>(fixture, 'Message')!, 'Fixed.');
    fixture.detectChanges();
    expect(byText(fixture, 'As it will be announced — 2 changes pinned')).not.toBeNull();
    // The section's reset line reads "dealt: row 1 · reset".
    expect(text(fixture).replace(/\s+/g, ' ')).toContain('dealt: row 1 · reset');
  });

  it('the announcement line adds " · whispered" and " · in Ada\'s voice"', async () => {
    const w = world();
    w.holds = [
      holdView('a', { baseline: previewOf(runView({ chipLabel: 'Lucky', visibility: 'whisper', announcer: 'character' })) }),
    ];
    const fixture = await renderDialog(w);
    await until(fixture, () => expect(byTestId(fixture, 'custom-tool-intervention')).not.toBeNull());
    expect(byTestId(fixture, 'intervention-preview')!.querySelector('.font-semibold')!.textContent).toBe(
      "Lucky · whispered · in Ada's voice",
    );
  });

  it('the trigger entrance names the phrase it overheard', async () => {
    const w = world();
    w.holds = [holdView('a', { entrance: 'trigger', triggerPhrase: 'cut the deck' })];
    const fixture = await renderDialog(w);
    await until(fixture, () => expect(byTestId(fixture, 'custom-tool-intervention')).not.toBeNull());
    expect(text(fixture).replace(/\s+/g, ' ')).toContain(
      'overheard: “cut the deck”. The cards are dealt but not yet shown — adjust what you like, then deal it. No one at the table will know you touched the deck.',
    );
  });

  it('the ✕ and the backdrop do not dismiss a held roll', async () => {
    const w = world();
    w.holds = [holdView('a')];
    const fixture = await renderDialog(w);
    await until(fixture, () => expect(byTestId(fixture, 'custom-tool-intervention')).not.toBeNull());
    byLabel<HTMLButtonElement>(fixture, 'Close')!.click();
    (root(fixture).querySelector('.qt-dialog-overlay') as HTMLElement).click();
    await settle(fixture);
    expect(byTestId(fixture, 'custom-tool-intervention')).not.toBeNull();
    expect(callsTo(w, 'chatCustomToolInterventionResolve')).toHaveLength(0);
  });
});

describe('the pure helpers', () => {
  it('summarizeWhen over twelve shapes: true, one group, two groups, $param, a string operand, the llm pair, the other tests, and their mixes', () => {
    expect(summarizeWhen(true)).toBe('otherwise');
    expect(summarizeWhen({ gte: 50 })).toBe('value ≥ 50');
    expect(summarizeWhen([{ gte: 50 }, { lt: 0.25 }])).toBe('value ≥ 50 or value < 0.25');
    expect(summarizeWhen({ gte: { $param: 'dc' } })).toBe('value ≥ params.dc');
    expect(summarizeWhen({ params: { mood: { eq: 'grim' } } })).toBe('params.mood = "grim"');
    expect(summarizeWhen({ llm: { ok: true, contains: 'YES' } } as never)).toBe(
      '(consult answered and answer contains "YES")',
    );
    expect(summarizeWhen({ llm: { ok: false } } as never)).toBe('consult failed');
    expect(summarizeWhen({ state: { x: { eq: 1 } }, inventoryCount: { ring: { gte: 1 } } } as never)).toBe(
      'state, inventoryCount tests',
    );
    expect(summarizeWhen({ gte: 1, lt: 3.14159, roll: { gt: 0.5 } })).toBe(
      '(value ≥ 1 and value < 3.142 and roll > 0.5)',
    );
    expect(summarizeWhen({ metadata: { mood: { neq: true } } } as never)).toBe('metadata.mood ≠ true');
    expect(summarizeWhen({} as never)).toBe('always');
    expect(summarizeWhen({ gt: { $param: 'floor' }, params: { luck: { eq: true } }, progress: { 'p.percent': { gte: 50 } } } as never)).toBe(
      '(value > params.floor and params.luck = true and progress tests)',
    );
  });

  it('literalFrom over the five shapes', () => {
    expect(literalFrom('true')).toBe(true);
    expect(literalFrom('false')).toBe(false);
    expect(literalFrom('42')).toBe(42);
    expect(literalFrom('  ')).toBe('  ');
    expect(literalFrom('a key')).toBe('a key');
  });

  it('formatNumber over an integer and 0.123456', () => {
    expect(formatNumber(70)).toBe('70');
    expect(formatNumber(0.123456)).toBe('0.1235');
  });
});
