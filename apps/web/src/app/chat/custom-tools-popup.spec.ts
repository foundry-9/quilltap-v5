import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { CoreDispatchError } from '../core/core-contract';
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
