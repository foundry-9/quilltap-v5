import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { CoreDispatchError } from '../core/core-contract';
import { P4D330_SPA_LANDED } from '../../testing/p4d334-gates';
import { CustomToolsPopup } from './custom-tools-popup';
import type { CustomToolRosterEntry, CustomToolsRoster } from './custom-tools.api';
import { ToastService } from '../ui/toast.service';

/**
 * The composer's wand and its two-phase run dialog. The v5-specific cases
 * (button gating, broken-file badges, refetch-on-open, the inline run error)
 * carry over from the popup this replaced; the two-phase cases are v4's
 * `__tests__/unit/components/chat/CustomToolRunDialog.test.tsx` (faab6881)
 * ported case for case.
 */

/** v4's UNLOCK: a number parameter, a rich vocabulary, public by default. */
const UNLOCK: CustomToolRosterEntry = {
  name: 'unlock',
  title: 'Force the Lock',
  description: 'Attempt to pick the lock.',
  parameters: {
    bonus: {
      type: 'number',
      default: 0,
      description: 'Skill bonus added to the roll.',
      min: -5,
      max: 5,
    },
  },
  references: {
    value: true,
    roll: true,
    dice: true,
    llm: false,
    params: ['bonus'],
    metadata: ['hasAnsibleAccess'],
    state: ['player.health'],
  },
  defaultVisibility: 'public',
  sourceTier: 'project',
  asCharacterId: 'char-1',
  definitionPath: 'Tools/unlock.tool.json',
  mountPointId: 'mount-1',
  mountName: 'Kepler Notes',
};

/** v4's WHEEL: a string parameter, quotes only its oracle, whispers by default. */
const WHEEL: CustomToolRosterEntry = {
  name: 'wheel',
  title: 'Spin the Wheel',
  description: 'Let the wheel decide.',
  parameters: {
    wish: { type: 'string', default: 'Something interesting.', description: 'What you would like of it.' },
  },
  references: { value: false, roll: false, dice: false, llm: true, params: [], metadata: [], state: [] },
  defaultVisibility: 'whisper',
  sourceTier: 'general',
  asCharacterId: 'char-1',
  definitionPath: 'Tools/wheel.tool.json',
  mountPointId: 'mount-1',
  mountName: 'Kepler Notes',
};

/** A roster with one no-param tool that defaults to a whisper, plus one broken file. */
function roster(over: Partial<CustomToolsRoster> = {}): CustomToolsRoster {
  return {
    tools: [
      {
        name: 'pick_lock',
        title: 'Pick Lock',
        description: 'Try the tumblers.',
        parameters: { scale: { type: 'integer', default: 3, min: 1, max: 6 } },
        defaultVisibility: 'whisper',
        sourceTier: 'global',
        asCharacterId: 'char-1',
        definitionPath: 'Tools/pick-lock.tool.json',
        mountName: 'Toolbox',
        mountPointId: 'mount-1',
      },
    ],
    errors: [],
    ...over,
  };
}

interface Stub {
  calls: Record<string, unknown>[];
  /** Mutable, so a case can change what the NEXT list call answers. */
  list: CustomToolsRoster;
  client: Partial<CoreClient>;
}

/** dispatchData answers the list verb with `list`, the run verb with `run` (or throws). */
function stub(
  list: CustomToolsRoster,
  run: Record<string, unknown> | Error = { messages: [], result: {} },
): Stub {
  const calls: Record<string, unknown>[] = [];
  const state = { list };
  const dispatchData = vi.fn(async (req: Record<string, unknown>) => {
    calls.push(req);
    if (req['type'] === 'chatCustomToolsList') return state.list as unknown as Record<string, unknown>;
    if (req['type'] === 'chatCustomToolRun') {
      if (run instanceof Error) throw run;
      return run;
    }
    return {};
  });
  return {
    calls,
    get list() {
      return state.list;
    },
    set list(next: CustomToolsRoster) {
      state.list = next;
    },
    client: { dispatchData: dispatchData as unknown as CoreClient['dispatchData'] },
  };
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 8; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function mount(s: Stub): Promise<ComponentFixture<CustomToolsPopup>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [CustomToolsPopup],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: s.client }],
  });
  const fixture = TestBed.createComponent(CustomToolsPopup);
  fixture.componentRef.setInput('chatId', 'chat-1');
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

function button(fixture: ComponentFixture<CustomToolsPopup>): HTMLButtonElement | null {
  return fixture.nativeElement.querySelector('button[aria-label="Custom tools"]');
}

function dialog(fixture: ComponentFixture<CustomToolsPopup>): HTMLElement | null {
  return fixture.nativeElement.querySelector('[role="dialog"]');
}

function text(fixture: ComponentFixture<CustomToolsPopup>): string {
  return (fixture.nativeElement.textContent ?? '').replace(/\s+/g, ' ');
}

/** Every button in the dialog whose text contains `needle`. */
function buttons(fixture: ComponentFixture<CustomToolsPopup>, needle: string): HTMLButtonElement[] {
  return [...fixture.nativeElement.querySelectorAll('button')].filter((b: HTMLButtonElement) =>
    (b.textContent ?? '').includes(needle),
  ) as HTMLButtonElement[];
}

async function open(fixture: ComponentFixture<CustomToolsPopup>): Promise<void> {
  button(fixture)!.click();
  fixture.detectChanges();
  await settle(fixture);
}

async function click(fixture: ComponentFixture<CustomToolsPopup>, el: HTMLElement): Promise<void> {
  el.click();
  fixture.detectChanges();
  await settle(fixture);
}

/** The toast stack this render raised, newest last. */
function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

describe('CustomToolsPopup — the wand and its gating', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('hides the button entirely when the roster is empty and error-free', async () => {
    const fixture = await mount(stub({ tools: [], errors: [] }));
    expect(button(fixture)).toBeNull();
  });

  it('shows the button when a definition failed to load, even with zero runnable tools', async () => {
    const fixture = await mount(
      stub({
        tools: [],
        errors: [{ definitionPath: 'Tools/broken.tool.json', mountName: 'Toolbox', tier: 'global', reason: 'bad outcome table' }],
      }),
    );
    expect(button(fixture)).not.toBeNull();
    await open(fixture);
    // The badge renders the loader's verbatim reason.
    expect(text(fixture)).toContain('Tools/broken.tool.json');
    expect(text(fixture)).toContain('bad outcome table');
  });

  it('re-resolves the roster fresh on every open (v4 enabled: isOpen)', async () => {
    const s = stub(roster());
    const fixture = await mount(s);
    const listCalls = () => s.calls.filter((c) => c['type'] === 'chatCustomToolsList').length;
    const afterMount = listCalls();
    await open(fixture); // open → refetch
    expect(listCalls()).toBe(afterMount + 1);
  });

  it('shows the tool but NEVER its odds/outcome table', async () => {
    const fixture = await mount(stub(roster()));
    await open(fixture);
    expect(text(fixture)).toContain('Pick Lock');
    expect(text(fixture)).toContain('Try the tumblers.');
    // The payload carries no roll spec / outcome table, and the dialog shows none.
    expect(text(fixture)).not.toMatch(/outcome|odds|notation|dice/i);
  });

  it('surfaces the roster cap notice (droppedForCap)', async () => {
    const fixture = await mount(stub(roster({ droppedForCap: ['flip_coin', 'draw_card'] })));
    await open(fixture);
    expect(text(fixture)).toContain('these were left off: flip_coin, draw_card');
  });
});

// --- P4.6bb: the three Workbench entry points ------------------------------

describe('CustomToolsPopup — the Workbench entry points', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('always offers "New contrivance…" — authoring lives on the Workbench', async () => {
    const fixture = await mount(stub(roster()));
    await open(fixture);
    expect(text(fixture)).toContain('New contrivance');
  });

  it('a broken badge WITHOUT a mountPointId stays inert — nowhere to send you', async () => {
    const fixture = await mount(
      stub({
        tools: [],
        errors: [
          {
            definitionPath: 'Tools/broken.tool.json',
            mountName: 'Toolbox',
            tier: 'global',
            reason: 'bad outcome table',
          },
        ],
      }),
    );
    await open(fixture);
    const badge = buttons(fixture, 'Tools/broken.tool.json')[0];
    expect(badge?.disabled).toBe(true);
    expect(badge?.getAttribute('title')).toBeNull();
  });

  it('a broken badge WITH a mountPointId opens repair mode', async () => {
    const fixture = await mount(
      stub({
        tools: [],
        errors: [
          {
            definitionPath: 'Tools/broken.tool.json',
            mountPointId: 'm1',
            mountName: 'Toolbox',
            tier: 'global',
            reason: 'bad outcome table',
          },
        ],
      }),
    );
    await open(fixture);
    const badge = buttons(fixture, 'Tools/broken.tool.json')[0];
    expect(badge?.disabled).toBe(false);
    expect(badge?.getAttribute('title')).toBe("Open in Pascal's Workbench to repair");
  });
});

// --- P4.d21 (v4 CustomToolRunDialog.test.tsx, faab6881) --------------------

describe('CustomToolRunDialog — choosing a tool', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('lists the roster first, then swaps in the chosen tool’s form', async () => {
    const fixture = await mount(stub({ tools: [UNLOCK, WHEEL], errors: [] }));
    await open(fixture);
    expect(text(fixture)).toContain('Spin the Wheel');

    await click(fixture, buttons(fixture, 'Force the Lock')[0]);

    // The picker is gone; the form has taken the body.
    expect(text(fixture)).not.toContain('Spin the Wheel');
    expect(fixture.nativeElement.querySelector('#custom-tool-unlock\\:\\:char-1-bonus')).not.toBeNull();
    expect(buttons(fixture, 'Run Force the Lock').length).toBe(1);
    // The dialog retitles itself.
    expect(fixture.nativeElement.querySelector('.qt-dialog-title').textContent.trim()).toBe(
      'Run Force the Lock',
    );
  });

  it('goes back to the roster from the form', async () => {
    const fixture = await mount(stub({ tools: [UNLOCK, WHEEL], errors: [] }));
    await open(fixture);
    await click(fixture, buttons(fixture, 'Force the Lock')[0]);
    await click(fixture, buttons(fixture, 'Choose another tool')[0]);

    expect(text(fixture)).toContain('Spin the Wheel');
    expect(fixture.nativeElement.querySelector('.qt-dialog-title').textContent.trim()).toBe(
      "Pascal's Table",
    );
  });

  it('drops back to the picker when the remembered tool leaves the roster', async () => {
    const s = stub({ tools: [UNLOCK, WHEEL], errors: [] });
    const fixture = await mount(s);
    await open(fixture);
    await click(fixture, buttons(fixture, 'Force the Lock')[0]);
    expect(buttons(fixture, 'Run Force the Lock').length).toBe(1);

    // Close WITHOUT going back — the selection survives a close on purpose.
    await click(fixture, buttons(fixture, 'Cancel')[0]);
    // The definition is renamed / disabled / gated away between resolutions.
    s.list = { tools: [WHEEL], errors: [] };
    await open(fixture); // reopening refetches, and the selection re-derives

    expect(buttons(fixture, 'Run Force the Lock').length).toBe(0);
    expect(text(fixture)).toContain('Spin the Wheel');
  });

  it('Escape, the backdrop, and Cancel all dismiss it', async () => {
    const fixture = await mount(stub({ tools: [UNLOCK, WHEEL], errors: [] }));

    await open(fixture);
    expect(dialog(fixture)).not.toBeNull();
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await settle(fixture);
    expect(dialog(fixture)).toBeNull();

    await open(fixture);
    await click(fixture, fixture.nativeElement.querySelector('.qt-dialog-overlay'));
    expect(dialog(fixture)).toBeNull();

    await open(fixture);
    await click(fixture, buttons(fixture, 'Cancel')[0]);
    expect(dialog(fixture)).toBeNull();
  });

  it('earns a search box only past six tools, and filters on it', async () => {
    const many = Array.from({ length: 7 }, (_, i) => ({
      ...UNLOCK,
      name: `tool_${i}`,
      title: i === 3 ? 'Force the Lock' : `Contrivance ${i}`,
    }));
    const fixture = await mount(stub({ tools: many, errors: [] }));
    await open(fixture);
    const box = fixture.nativeElement.querySelector('input[aria-label="Search the table"]');
    expect(box).not.toBeNull();

    box.value = 'force';
    box.dispatchEvent(new Event('input'));
    await settle(fixture);
    expect(text(fixture)).toContain('Force the Lock');
    expect(text(fixture)).not.toContain('Contrivance 0');
  });

  it('has no search box at six tools or fewer', async () => {
    const six = Array.from({ length: 6 }, (_, i) => ({ ...UNLOCK, name: `tool_${i}` }));
    const fixture = await mount(stub({ tools: six, errors: [] }));
    await open(fixture);
    expect(fixture.nativeElement.querySelector('input[aria-label="Search the table"]')).toBeNull();
  });

  it('says so when nothing on the table answers to the search', async () => {
    const many = Array.from({ length: 7 }, (_, i) => ({ ...UNLOCK, name: `tool_${i}` }));
    const fixture = await mount(stub({ tools: many, errors: [] }));
    await open(fixture);
    const box = fixture.nativeElement.querySelector('input[aria-label="Search the table"]');
    box.value = 'nothing-matches-this';
    box.dispatchEvent(new Event('input'));
    await settle(fixture);
    expect(text(fixture)).toContain('Nothing on the table answers to that.');
  });
});

describe('CustomToolRunDialog — what it remembers', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('keeps the tool, its values, and its privacy toggle across a close', async () => {
    const fixture = await mount(stub({ tools: [UNLOCK, WHEEL], errors: [] }));
    await open(fixture);
    await click(fixture, buttons(fixture, 'Force the Lock')[0]);

    const bonusId = '#custom-tool-unlock\\:\\:char-1-bonus';
    const bonus = fixture.nativeElement.querySelector(bonusId) as HTMLInputElement;
    bonus.value = '3';
    bonus.dispatchEvent(new Event('input'));
    const privacy = fixture.nativeElement.querySelector(
      '[role="dialog"] input[type="checkbox"]',
    ) as HTMLInputElement;
    privacy.checked = true;
    privacy.dispatchEvent(new Event('change'));
    await settle(fixture);

    await click(fixture, buttons(fixture, 'Cancel')[0]);
    expect(dialog(fixture)).toBeNull();

    await open(fixture);
    expect((fixture.nativeElement.querySelector(bonusId) as HTMLInputElement).value).toBe('3');
    expect(
      (fixture.nativeElement.querySelector('[role="dialog"] input[type="checkbox"]') as HTMLInputElement)
        .checked,
    ).toBe(true);
  });

  it('seeds the privacy toggle from the definition’s default visibility', async () => {
    const fixture = await mount(stub({ tools: [UNLOCK, WHEEL], errors: [] }));
    await open(fixture);
    await click(fixture, buttons(fixture, 'Spin the Wheel')[0]);
    expect(
      (fixture.nativeElement.querySelector('[role="dialog"] input[type="checkbox"]') as HTMLInputElement)
        .checked,
    ).toBe(true);
  });

  it('sends the coerced values, the privacy flag, and the perspective on run', async () => {
    const s = stub({ tools: [UNLOCK, WHEEL], errors: [] });
    const fixture = await mount(s);
    let ranCount = 0;
    fixture.componentInstance.ran.subscribe(() => (ranCount += 1));

    await open(fixture);
    await click(fixture, buttons(fixture, 'Force the Lock')[0]);
    const bonus = fixture.nativeElement.querySelector(
      '#custom-tool-unlock\\:\\:char-1-bonus',
    ) as HTMLInputElement;
    bonus.value = '2';
    bonus.dispatchEvent(new Event('input'));
    await settle(fixture);
    await click(fixture, buttons(fixture, 'Run Force the Lock')[0]);

    expect(s.calls.find((c) => c['type'] === 'chatCustomToolRun')).toEqual({
      type: 'chatCustomToolRun',
      chatId: 'chat-1',
      tool: 'unlock',
      parameters: { bonus: 2 },
      private: false,
      asCharacterId: 'char-1',
    });
    expect(ranCount).toBe(1);
    // The dialog closed on success.
    expect(dialog(fixture)).toBeNull();
  });

  it('runs a no-param tool with its coerced default and whispered visibility', async () => {
    const s = stub(roster());
    const fixture = await mount(s);
    await open(fixture);
    await click(fixture, buttons(fixture, 'Pick Lock')[0]);
    await click(fixture, buttons(fixture, 'Run Pick Lock')[0]);

    expect(s.calls.find((c) => c['type'] === 'chatCustomToolRun')).toEqual({
      type: 'chatCustomToolRun',
      chatId: 'chat-1',
      tool: 'pick_lock',
      parameters: { scale: 3 }, // coerced integer default
      private: true, // defaultVisibility 'whisper'
      asCharacterId: 'char-1',
    });
  });

  it('surfaces the 400 server message inline and does NOT emit ran on a run error', async () => {
    const s = stub(roster(), new CoreDispatchError({ kind: 'bad-request', message: 'Unknown custom tool "pick_lock".' }));
    const fixture = await mount(s);
    let ranCount = 0;
    fixture.componentInstance.ran.subscribe(() => (ranCount += 1));

    await open(fixture);
    await click(fixture, buttons(fixture, 'Pick Lock')[0]);
    await click(fixture, buttons(fixture, 'Run Pick Lock')[0]);

    expect(toasts()).toEqual([{ type: 'error', message: 'Unknown custom tool "pick_lock".' }]);
    expect(ranCount).toBe(0);
    // The dialog stayed open so the operator can see the failure.
    expect(dialog(fixture)).not.toBeNull();
  });
});

describe('CustomToolRunDialog — the reference panel', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('names the placeholders this particular tool can quote', async () => {
    const fixture = await mount(stub({ tools: [UNLOCK, WHEEL], errors: [] }));
    await open(fixture);
    await click(fixture, buttons(fixture, 'Force the Lock')[0]);

    const body = text(fixture);
    expect(body).toContain('{{value}}');
    expect(body).toContain('{{roll}}');
    expect(body).toContain('{{params.bonus}}');
    expect(body).toContain('{{metadata.hasAnsibleAccess}}');
    expect(body).toContain('{{state.player.health}}');
    // Dice form, no oracle.
    expect(body).toContain('{{dice}}');
    expect(body).not.toContain('{{llm}}');
    // The declared description stands in for the parameter's meaning.
    expect(body).toContain('Skill bonus added to the roll.');
  });

  it('lists only what the tool actually quotes', async () => {
    const fixture = await mount(stub({ tools: [UNLOCK, WHEEL], errors: [] }));
    await open(fixture);
    await click(fixture, buttons(fixture, 'Spin the Wheel')[0]);

    // The wheel quotes its oracle and nothing else — not even its own value,
    // and not the `wish` parameter the form above is collecting.
    const body = text(fixture);
    expect(body).toContain('{{llm}}');
    expect(body).not.toContain('{{value}}');
    expect(body).not.toContain('{{roll}}');
    expect(body).not.toContain('{{dice}}');
    expect(body).not.toContain('{{params.wish}}');
    expect(body).not.toMatch(/\{\{metadata\./);
  });

  it('omits the panel entirely for a tool that quotes nothing', async () => {
    const fixture = await mount(
      stub({
        tools: [
          {
            ...WHEEL,
            references: { value: false, roll: false, dice: false, llm: false, params: [], metadata: [], state: [] },
          },
        ],
        errors: [],
      }),
    );
    await open(fixture);
    await click(fixture, buttons(fixture, 'Spin the Wheel')[0]);

    expect(text(fixture)).not.toContain('What this tool can quote');
    // The rest of the form is unaffected.
    expect(fixture.nativeElement.querySelector('[role="dialog"] input[type="checkbox"]')).not.toBeNull();
  });

  it('treats a roster from a server without §1 as "quotes nothing", not an error', async () => {
    const { references: _dropped, ...noRefs } = WHEEL;
    const fixture = await mount(stub({ tools: [noRefs], errors: [] }));
    await open(fixture);
    await click(fixture, buttons(fixture, 'Spin the Wheel')[0]);

    expect(text(fixture)).not.toContain('What this tool can quote');
    expect(buttons(fixture, 'Run Spin the Wheel').length).toBe(1);
  });

  it('says what the tool may WRITE, in its own sentence (v4 c4d4b0de)', async () => {
    const fixture = await mount(
      stub({
        tools: [
          {
            ...UNLOCK,
            references: {
              ...UNLOCK.references!,
              stateWrites: ['encounter.count'],
              metadataWrites: ['lockpick'],
            },
          },
        ],
        errors: [],
      }),
    );
    await open(fixture);
    await click(fixture, buttons(fixture, 'Force the Lock')[0]);

    const body = text(fixture);
    expect(body).toContain('When it runs, this tool may also write:');
    expect(body).toContain('state.encounter.count');
    expect(body).toContain('metadata.lockpick');
    expect(body).toContain('The record of what actually changed rides with the roll itself.');
    // EXACT, not toContain: the fragment assertions above were blind to the
    // stray space this caught (dogfood #51 follow-up), and they would be just
    // as blind to a separator that went missing between two targets.
    const sentence = body
      .slice(body.indexOf('When it runs'), body.indexOf('rides with the roll itself.') + 27)
      .replace(/\s+/g, ' ');
    expect(sentence).toBe(
      'When it runs, this tool may also write: state.encounter.count, metadata.lockpick.' +
        ' The record of what actually changed rides with the roll itself.',
    );
  });

  it('opens the panel for a tool that quotes nothing but writes something', async () => {
    // The panel's gate is reads OR writes: a tool with no placeholders at all
    // still has something to declare if it changes the world.
    const fixture = await mount(
      stub({
        tools: [
          {
            ...WHEEL,
            references: {
              value: false,
              roll: false,
              dice: false,
              llm: false,
              params: [],
              metadata: [],
              state: [],
              stateWrites: ['wheel.spins'],
            },
          },
        ],
        errors: [],
      }),
    );
    await open(fixture);
    await click(fixture, buttons(fixture, 'Spin the Wheel')[0]);

    expect(text(fixture)).toContain('What this tool can quote');
    expect(text(fixture)).toContain('state.wheel.spins');
  });

  it('opens the panel for a tool that only dresses, and says what it wears (v4 f0db8cbb8)', async () => {
    // A wardrobe gesture is a promise of its own: a tool that quotes nothing
    // and writes nothing but dresses the running character still earns the
    // panel, and its sentences follow the writes paragraph.
    const fixture = await mount(
      stub({
        tools: [
          {
            ...WHEEL,
            references: {
              value: false,
              roll: false,
              dice: false,
              llm: false,
              params: [],
              metadata: [],
              state: [],
              wears: ['Cloak of Night', 'Ribbon'],
              clearsSlots: ['hair'],
            },
          },
        ],
        errors: [],
      }),
    );
    await open(fixture);
    await click(fixture, buttons(fixture, 'Spin the Wheel')[0]);

    const body = text(fixture);
    expect(body).toContain('What this tool can quote');
    expect(body).toContain('It may dress the running character in');
    expect(body).toContain('Cloak of Night, Ribbon');
    expect(body).toContain('It may clear their');
    expect(body).toContain('hair slot. Only when you run it as a character — a run made as nobody dresses nobody.');
    expect(body).not.toContain('It may undress them of');
  });

  it('opens the panel for a tool that only takes stock of the bag, and says which keys (v4 a53024d3c)', async () => {
    const fixture = await mount(
      stub({
        tools: [
          {
            ...WHEEL,
            references: {
              value: false,
              roll: false,
              dice: false,
              llm: false,
              params: [],
              metadata: [],
              state: [],
              inventoryAggregates: ['equipped.names', 'owned.metadata.toolVersion.max'],
            } as never,
          },
        ],
        errors: [],
      }),
    );
    await open(fixture);
    await click(fixture, buttons(fixture, 'Spin the Wheel')[0]);

    const body = text(fixture).replace(/\s+/g, ' ');
    expect(body).toContain('What this tool can quote');
    expect(body).toContain(
      'It takes stock of everything they carry, reading equipped.names, owned.metadata.toolVersion.max. Run as nobody, it finds no inventory at all.',
    );
  });

  // The reads paragraph and the three gesture lines are P4.D330's component; the order
  // (reads → aggregate → gestures, as v4) is the unifier's R-9 handoff, so this row is
  // red until that lands and gated on the sibling.
  it.skipIf(!P4D330_SPA_LANDED)('opens the panel for a tool that only reads the inventory, and keeps v4’s order', async () => {
    const fixture = await mount(
      stub({
        tools: [
          {
            ...WHEEL,
            references: {
              value: false,
              roll: false,
              dice: false,
              llm: false,
              params: [],
              metadata: [],
              state: [],
              inventoryReads: ['Excalibur'],
              inventoryAggregates: ['equipped.names'],
              equips: ['magic bow'],
            } as never,
          },
        ],
        errors: [],
      }),
    );
    await open(fixture);
    await click(fixture, buttons(fixture, 'Spin the Wheel')[0]);
    const body = text(fixture).replace(/\s+/g, ' ');
    const reads = body.indexOf('It reads the inventory for');
    const aggregate = body.indexOf('It takes stock of everything they carry');
    const equips = body.indexOf('It may equip the running character with');
    expect(reads).toBeGreaterThan(-1);
    expect(reads).toBeLessThan(aggregate);
    expect(aggregate).toBeLessThan(equips);
  });

  it('reads a roster without the write vocabulary as "writes nothing"', async () => {
    // Older servers send neither key; that must read as silence, not an error.
    const fixture = await mount(stub({ tools: [UNLOCK, WHEEL], errors: [] }));
    await open(fixture);
    await click(fixture, buttons(fixture, 'Force the Lock')[0]);

    expect(text(fixture)).toContain('What this tool can quote');
    expect(text(fixture)).not.toContain('may also write');
  });

  it('gives a string parameter a textarea, and a number parameter a number input', async () => {
    const fixture = await mount(stub({ tools: [UNLOCK, WHEEL], errors: [] }));
    await open(fixture);
    await click(fixture, buttons(fixture, 'Spin the Wheel')[0]);
    const wish = fixture.nativeElement.querySelector('#custom-tool-wheel\\:\\:char-1-wish');
    expect(wish.tagName).toBe('TEXTAREA');

    await click(fixture, buttons(fixture, 'Choose another tool')[0]);
    await click(fixture, buttons(fixture, 'Force the Lock')[0]);
    const bonus = fixture.nativeElement.querySelector(
      '#custom-tool-unlock\\:\\:char-1-bonus',
    ) as HTMLInputElement;
    expect(bonus.tagName).toBe('INPUT');
    expect(bonus.type).toBe('number');
  });

  it('says plainly that what the operator types is not templated', async () => {
    const fixture = await mount(stub({ tools: [UNLOCK, WHEEL], errors: [] }));
    await open(fixture);
    await click(fixture, buttons(fixture, 'Force the Lock')[0]);
    expect(text(fixture)).toContain('sent exactly as written, braces and all');
  });
});

/**
 * Whose sheet the roll will consult, said out loud.
 *
 * Since v4 `e8a49597` (mirrored into v5 by P4.D24) the server sets
 * `characterLabel` on a SINGLE-variant row too, whenever it had to fall back to
 * someone other than the operator — an all-LLM room, or a gate the operator's own
 * character did not pass. The rendering was already unconditional on presence, but
 * no case in this file exercised it; these two are the coverage that lane recorded
 * as owed. Absence still means "runs as you".
 */
describe('CustomToolsPopup — the borrowed sheet', () => {
  afterEach(() => TestBed.resetTestingModule());

  const BORROWED: CustomToolRosterEntry = {
    ...UNLOCK,
    asCharacterId: 'char-9',
    characterLabel: 'Lorian',
  };

  it('names whose sheet a single-variant row will borrow, in both phases', async () => {
    const fixture = await mount(stub({ tools: [BORROWED], errors: [] }));
    await open(fixture);
    expect(text(fixture)).toContain('as Lorian');

    await click(fixture, buttons(fixture, 'Force the Lock')[0]);
    expect(text(fixture)).toContain('· as Lorian');
  });

  it('says nothing about a sheet when the run is the operator’s own', async () => {
    const fixture = await mount(stub({ tools: [UNLOCK], errors: [] }));
    await open(fixture);
    expect(text(fixture)).not.toContain('as Lorian');
    expect(text(fixture)).not.toContain(' · as ');
  });

  it('finds a tool by the character it will borrow from', async () => {
    const many = Array.from({ length: 6 }, (_, i) => ({ ...UNLOCK, name: `tool_${i}` }));
    const fixture = await mount(stub({ tools: [...many, BORROWED], errors: [] }));
    await open(fixture);
    const box = fixture.nativeElement.querySelector('input[aria-label="Search the table"]');
    box.value = 'lorian';
    box.dispatchEvent(new Event('input'));
    await settle(fixture);
    expect(text(fixture)).toContain('as Lorian');
    expect(text(fixture)).not.toContain('Nothing on the table answers to that.');
  });
});

/**
 * The progressions half of the reference panel (v4 `0587d1e96`'s
 * `CustomToolRunDialog.tsx` hunk).
 *
 * Progressions get their own SENTENCE rather than placeholder rows, and the
 * vocabulary carries IDS rather than `<id>.<field>` keys — v4's own reason:
 * "the interesting claim is WHICH timed condition the tool consults or adjusts,
 * not which of its dozen derived fields the author happened to quote", and a
 * field-level list would edge toward the odds the roster deliberately withholds.
 */
describe('CustomToolsPopup — what a tool says about progressions', () => {
  it('gives {{now}} a placeholder row of its own', async () => {
    const fixture = await mount(
      stub({
        tools: [{ ...UNLOCK, references: { ...UNLOCK.references!, now: true } }],
        errors: [],
      }),
    );
    await open(fixture);
    await click(fixture, buttons(fixture, 'Force the Lock')[0]);

    expect(text(fixture)).toContain('{{now}}');
    expect(text(fixture)).toContain('the moment the run began, to the millisecond');
  });

  it('gives {{character}} a row right after {{now}} (v4 a05c6f755)', async () => {
    const fixture = await mount(
      stub({
        tools: [
          { ...UNLOCK, references: { ...UNLOCK.references!, now: true, character: true } },
        ],
        errors: [],
      }),
    );
    await open(fixture);
    await click(fixture, buttons(fixture, 'Force the Lock')[0]);

    const body = text(fixture);
    expect(body).toContain('the name of the character the run is made for');
    expect(body.indexOf('{{character}}')).toBeGreaterThan(
      body.indexOf('the moment the run began, to the millisecond'),
    );
  });

  it('gives no {{character}} row to a tool that does not quote it', async () => {
    for (const character of [undefined, false]) {
      const fixture = await mount(
        stub({
          tools: [{ ...UNLOCK, references: { ...UNLOCK.references!, character } }],
          errors: [],
        }),
      );
      await open(fixture);
      await click(fixture, buttons(fixture, 'Force the Lock')[0]);
      expect(text(fixture)).not.toContain('{{character}}');
    }
  });

  it('names the progressions it consults, by id, in its own sentence', async () => {
    const fixture = await mount(
      stub({
        tools: [
          { ...UNLOCK, references: { ...UNLOCK.references!, progress: ['cannon', 'gestation'] } },
        ],
        errors: [],
      }),
    );
    await open(fixture);
    await click(fixture, buttons(fixture, 'Force the Lock')[0]);

    const body = text(fixture);
    // EXACT, for the reason the write sentence above is: a fragment assertion
    // would be blind to a missing separator or a stray space.
    const sentence = body
      .slice(
        body.indexOf('It consults your timed progressions'),
        body.indexOf('simply does not match.') + 22,
      )
      .replace(/\s+/g, ' ');
    expect(sentence).toBe(
      'It consults your timed progressions: cannon, gestation. A progression you are not carrying' +
        ' simply does not match.',
    );
  });

  it('lists a written progression by ID — never an <id>.<field> key', async () => {
    const fixture = await mount(
      stub({
        tools: [
          {
            ...UNLOCK,
            references: { ...UNLOCK.references!, progressWrites: ['cannon', 'gestation'] },
          },
        ],
        errors: [],
      }),
    );
    await open(fixture);
    await click(fixture, buttons(fixture, 'Force the Lock')[0]);

    // EXACT, not `toContain`: `progress.cannon` is a substring of
    // `progress.cannon.endTime`, so a fragment assertion is blind to the very
    // thing the vocabulary's id-not-key rule exists to prevent.
    const body = text(fixture);
    const sentence = body
      .slice(body.indexOf('When it runs'), body.indexOf('rides with the roll itself.') + 27)
      .replace(/\s+/g, ' ');
    expect(sentence).toBe(
      'When it runs, this tool may also write: progress.cannon, progress.gestation.' +
        ' The record of what actually changed rides with the roll itself.',
    );
  });

  it('opens the panel for a tool that only consults a progression', async () => {
    // The panel's gate is reads OR writes OR progressions: a tool with no
    // placeholders and no writes still has something to declare.
    const fixture = await mount(
      stub({
        tools: [
          {
            ...UNLOCK,
            references: {
              value: false,
              roll: false,
              dice: false,
              llm: false,
              params: [],
              metadata: [],
              state: [],
              progress: ['cannon'],
            },
          },
        ],
        errors: [],
      }),
    );
    await open(fixture);
    await click(fixture, buttons(fixture, 'Force the Lock')[0]);

    expect(text(fixture)).toContain('What this tool can quote');
    expect(text(fixture)).toContain('It consults your timed progressions:');
  });

  it('says nothing at all when the roster carries none of the three keys', async () => {
    // An older server sends no `progress` / `progressWrites` / `now`, which must
    // read as "consults nothing" rather than as an error.
    const fixture = await mount(stub({ tools: [UNLOCK], errors: [] }));
    await open(fixture);
    await click(fixture, buttons(fixture, 'Force the Lock')[0]);

    expect(text(fixture)).not.toContain('It consults your timed progressions');
    expect(text(fixture)).not.toContain('{{now}}');
    expect(text(fixture)).not.toContain('progress.');
  });
});

/**
 * v4 `CustomToolRunDialog.test.tsx:147-172` (`4877fc2da`, "initialTool (the
 * composer's `/` commands)") ported case for case, plus the nonce rules v4's
 * `appliedNonceRef` carries and the public `open()` the composer's slash host
 * calls.
 */
describe('CustomToolRunDialog — initialTool (the composer’s `/` commands)', () => {
  afterEach(() => TestBed.resetTestingModule());

  async function request(
    fixture: ComponentFixture<CustomToolsPopup>,
    initial: { name: string; asCharacterId: string | null; nonce: number },
  ): Promise<void> {
    fixture.componentRef.setInput('initialTool', initial);
    fixture.detectChanges();
    await settle(fixture);
    await settle(fixture);
  }

  it('opens straight onto the requested tool’s form', async () => {
    const fixture = await mount(stub({ tools: [UNLOCK, WHEEL], errors: [] }));
    await request(fixture, { name: 'wheel', asCharacterId: 'char-1', nonce: 1 });

    expect(buttons(fixture, 'Run Spin the Wheel').length).toBe(1);
    expect(text(fixture)).not.toContain('Force the Lock');
  });

  it('picks the per-character variant it was asked for', async () => {
    const s = stub({
      tools: [UNLOCK, { ...UNLOCK, characterLabel: 'Vivienne', asCharacterId: 'char-v' }],
      errors: [],
    });
    const fixture = await mount(s);
    await request(fixture, { name: 'unlock', asCharacterId: 'char-v', nonce: 1 });

    await click(fixture, buttons(fixture, 'Run Force the Lock')[0]);
    const runs = s.calls.filter((c) => c['type'] === 'chatCustomToolRun');
    expect(runs).toHaveLength(1);
    expect(runs[0]).toMatchObject({ tool: 'unlock', asCharacterId: 'char-v' });
  });

  it('stays on the picker and says so when nothing by that name is dealt', async () => {
    const fixture = await mount(stub({ tools: [UNLOCK, WHEEL], errors: [] }));
    await request(fixture, { name: 'nonesuch', asCharacterId: null, nonce: 1 });

    expect(text(fixture)).toContain('Force the Lock');
    expect(buttons(fixture, 'Run Force the Lock').length).toBe(0);
    expect(toasts()).toContainEqual({
      type: 'error',
      message: 'No tool named "nonesuch" is on Pascal\'s table in this chat',
    });
  });

  it('a null asCharacterId takes the first tool by that name', async () => {
    const s = stub({
      tools: [UNLOCK, { ...UNLOCK, characterLabel: 'Vivienne', asCharacterId: 'char-v' }],
      errors: [],
    });
    const fixture = await mount(s);
    await request(fixture, { name: 'unlock', asCharacterId: null, nonce: 1 });
    await click(fixture, buttons(fixture, 'Run Force the Lock')[0]);
    expect(s.calls.find((c) => c['type'] === 'chatCustomToolRun')).toMatchObject({
      asCharacterId: 'char-1',
    });
  });

  it('applies one nonce once: the wand later lands where the dialog remembers being', async () => {
    const fixture = await mount(stub({ tools: [UNLOCK, WHEEL], errors: [] }));
    await request(fixture, { name: 'wheel', asCharacterId: 'char-1', nonce: 1 });
    await click(fixture, buttons(fixture, 'Choose another tool')[0]);
    await click(fixture, buttons(fixture, 'Cancel')[0]);

    // The same nonce again changes nothing; the wand reopens on the picker.
    fixture.componentRef.setInput('initialTool', { name: 'wheel', asCharacterId: 'char-1', nonce: 1 });
    await settle(fixture);
    expect(dialog(fixture)).toBeNull();
    await open(fixture);
    await settle(fixture);
    expect(buttons(fixture, 'Run Spin the Wheel').length).toBe(0);
    expect(text(fixture)).toContain('Force the Lock');
  });

  it('does not re-apply its nonce when the roster refetches under an open dialog', async () => {
    const s = stub({ tools: [UNLOCK, WHEEL], errors: [] });
    const fixture = await mount(s);
    await request(fixture, { name: 'wheel', asCharacterId: 'char-1', nonce: 1 });
    await click(fixture, buttons(fixture, 'Choose another tool')[0]);

    // A realtime invalidation while the picker shows, bringing a changed roster
    // (v4's effect re-runs on `tools`; only the nonce stops a re-selection).
    s.list = { tools: [UNLOCK, WHEEL, { ...UNLOCK, name: 'unlock2', title: 'Pick the Lock' }], errors: [] };
    await fixture.debugElement.injector.get(QueryClient).invalidateQueries({ queryKey: ['customTools', 'chat-1'] });
    await settle(fixture);
    await settle(fixture);

    expect(text(fixture)).toContain('Pick the Lock');
    expect(buttons(fixture, 'Run Spin the Wheel').length).toBe(0);
    expect(text(fixture)).toContain('Force the Lock');
  });

  it('a character variant that is not dealt is NOT swapped for another', async () => {
    const fixture = await mount(stub({ tools: [UNLOCK, WHEEL], errors: [] }));
    await request(fixture, { name: 'unlock', asCharacterId: 'char-x', nonce: 1 });

    expect(buttons(fixture, 'Run Force the Lock').length).toBe(0);
    expect(toasts()).toContainEqual({
      type: 'error',
      message: 'No tool named "unlock" is on Pascal\'s table in this chat',
    });
  });

  it('a NEW nonce for the same tool re-selects it after Choose another tool', async () => {
    const fixture = await mount(stub({ tools: [UNLOCK, WHEEL], errors: [] }));
    await request(fixture, { name: 'wheel', asCharacterId: 'char-1', nonce: 1 });
    await click(fixture, buttons(fixture, 'Choose another tool')[0]);
    expect(buttons(fixture, 'Run Spin the Wheel').length).toBe(0);

    await request(fixture, { name: 'wheel', asCharacterId: 'char-1', nonce: 2 });
    expect(buttons(fixture, 'Run Spin the Wheel').length).toBe(1);
  });

  it('open() from outside, with the dialog already open, re-applies once the refetch settles', async () => {
    const s = stub({ tools: [UNLOCK, WHEEL], errors: [] });
    const fixture = await mount(s);
    await open(fixture);
    expect(buttons(fixture, 'Run Force the Lock').length).toBe(0);
    const listsBefore = s.calls.filter((c) => c['type'] === 'chatCustomToolsList').length;

    fixture.componentInstance.open({ name: 'unlock', asCharacterId: 'char-1' });
    fixture.detectChanges();
    await settle(fixture);
    await settle(fixture);

    expect(buttons(fixture, 'Run Force the Lock').length).toBe(1);
    // It waited for THIS open's fetch, not the cached roster.
    expect(s.calls.filter((c) => c['type'] === 'chatCustomToolsList').length).toBe(listsBefore + 1);
  });

  it('finds a tool added to a store mid-chat (it waits for the fresh roster)', async () => {
    const s = stub({ tools: [UNLOCK], errors: [] });
    const fixture = await mount(s);
    s.list = { tools: [UNLOCK, WHEEL], errors: [] };
    fixture.componentInstance.open({ name: 'wheel', asCharacterId: 'char-1' });
    fixture.detectChanges();
    await settle(fixture);
    await settle(fixture);
    expect(buttons(fixture, 'Run Spin the Wheel').length).toBe(1);
    expect(toasts()).toEqual([]);
  });

  it('exposes available() for the composer’s slash host', async () => {
    const fixture = await mount(stub({ tools: [UNLOCK], errors: [] }));
    expect(fixture.componentInstance.available()).toBe(true);
    const empty = await mount(stub({ tools: [], errors: [] }));
    expect(empty.componentInstance.available()).toBe(false);
  });
});
