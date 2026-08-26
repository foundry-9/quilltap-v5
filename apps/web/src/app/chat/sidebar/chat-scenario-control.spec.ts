import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { beforeEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { CoreDispatchError } from '../../core/core-contract';
import { ToastService } from '../../ui/toast.service';
import { ChatScenarioControl } from './chat-scenario-control';

/**
 * The Salon sidebar's in-chat scenario picker (v4
 * `components/chat/ChatScenarioControl.tsx` @ `44a8137e`) — a transcription
 * parity spec against v4's source, plus the seeding arm that IS the bug the
 * commit fixes.
 */

interface Req {
  type: string;
  [k: string]: unknown;
}

const sent: Req[] = [];
let saveAnswer: Record<string, unknown> | Error = {
  scenarioText: 'x',
  changed: true,
  message: 'Scenario updated',
};

const PROJECT_SCENARIOS = [
  {
    path: 'Scenarios/inn.md',
    filename: 'inn.md',
    name: 'The Inn',
    isDefault: false,
    rawIsDefault: false,
    body: 'A low-ceilinged room.',
    lastModified: '',
    createdAt: '',
    updatedAt: '',
  },
];
const GENERAL_SCENARIOS = [
  {
    path: 'Scenarios/road.md',
    filename: 'road.md',
    name: 'The Road',
    isDefault: false,
    rawIsDefault: false,
    body: 'A road in the rain.',
    lastModified: '',
    createdAt: '',
    updatedAt: '',
  },
];
const CHARACTER_SCENARIOS = [{ id: 'cs-1', title: 'A Duel', content: 'Pistols at dawn.' }];

function stubClient(): Partial<CoreClient> {
  return {
    dispatchData: (async (req: Req) => {
      sent.push(req);
      switch (req['type']) {
        case 'scenarioList':
          return { mountPointId: 'mp', scenarios: GENERAL_SCENARIOS, warnings: [] };
        case 'projectScenarioList':
          return { mountPointId: 'mp', scenarios: PROJECT_SCENARIOS, warnings: [] };
        case 'groupScenariosUnion':
          return { groupScenarios: [] };
        case 'characterScenarioList':
          return { scenarios: CHARACTER_SCENARIOS };
        case 'chatSetScenario':
          if (saveAnswer instanceof Error) throw saveAnswer;
          return saveAnswer;
        default:
          return {};
      }
    }) as unknown as CoreClient['dispatchData'],
  };
}

@Component({
  imports: [ChatScenarioControl],
  template: `
    <qt-chat-scenario-control
      [chatId]="'chat-1'"
      [projectId]="projectId()"
      [scenarioText]="scenarioText()"
      [llmCharacterIds]="llmCharacterIds()"
      [singleLlmCharacterId]="singleLlmCharacterId()"
      [enabled]="enabled()"
      (chatUpdated)="refetched = refetched + 1"
    />
  `,
})
class Host {
  readonly projectId = signal<string | null>('proj-1');
  readonly scenarioText = signal<string | null>(null);
  readonly llmCharacterIds = signal<string[]>(['char-1']);
  readonly singleLlmCharacterId = signal<string | null>('char-1');
  readonly enabled = signal(true);
  refetched = 0;
}

async function render(): Promise<ComponentFixture<Host>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [Host],
    providers: [
      { provide: CoreClient, useValue: stubClient() },
      provideTanStackQuery(new QueryClient()),
    ],
  });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return fixture;
}

async function settle(fixture: ComponentFixture<Host>): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    await fixture.whenStable();
    fixture.detectChanges();
  }
}

function selectEl(fixture: ComponentFixture<Host>): HTMLSelectElement | null {
  return fixture.nativeElement.querySelector('select');
}

function textarea(fixture: ComponentFixture<Host>): HTMLTextAreaElement | null {
  return fixture.nativeElement.querySelector('textarea');
}

function saveButton(fixture: ComponentFixture<Host>): HTMLButtonElement {
  return fixture.nativeElement.querySelector('button') as HTMLButtonElement;
}

function toasts(): string[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => t.message);
}

beforeEach(() => {
  sent.length = 0;
  saveAnswer = { scenarioText: 'x', changed: true, message: 'Scenario updated' };
});

describe('ChatScenarioControl (v4 components/chat/ChatScenarioControl.tsx @ 44a8137e)', () => {
  it('renders v4’s label, button and standing caution verbatim', async () => {
    const fixture = await render();
    const text = (fixture.nativeElement as HTMLElement).textContent!.replace(/\s+/g, ' ').trim();
    expect(text).toContain('Scenario');
    expect(saveButton(fixture).textContent!.trim()).toBe('Change scenario');
    expect(text).toContain(
      'Changing the scene rewrites every character’s standing instructions, and the Host announces the revision to the company.',
    );
    expect(textarea(fixture)!.getAttribute('placeholder')).toBe(
      'Set the scene for this conversation…',
    );
  });

  it('fetches nothing until the section has been opened once (v4 `enabled`)', async () => {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [Host],
      providers: [
        { provide: CoreClient, useValue: stubClient() },
        provideTanStackQuery(new QueryClient()),
      ],
    });
    const fixture = TestBed.createComponent(Host);
    fixture.componentInstance.enabled.set(false);
    fixture.detectChanges();
    await settle(fixture);
    expect(sent).toEqual([]);
  });

  it('fetches the three keyed tiers once opened', async () => {
    const fixture = await render();
    await settle(fixture);
    const types = sent.map((r) => r.type).sort();
    expect(types).toEqual([
      'characterScenarioList',
      'groupScenariosUnion',
      'projectScenarioList',
      'scenarioList',
    ]);
    expect(sent.find((r) => r.type === 'projectScenarioList')!['projectId']).toBe('proj-1');
    expect(sent.find((r) => r.type === 'characterScenarioList')!['characterId']).toBe('char-1');
  });

  it('skips the project tier when the chat has no project', async () => {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [Host],
      providers: [
        { provide: CoreClient, useValue: stubClient() },
        provideTanStackQuery(new QueryClient()),
      ],
    });
    const fixture = TestBed.createComponent(Host);
    fixture.componentInstance.projectId.set(null);
    fixture.detectChanges();
    await settle(fixture);
    expect(sent.some((r) => r.type === 'projectScenarioList')).toBe(false);
  });

  /**
   * v4 gates the CHARACTER tier on `singleLlmCharacterId` — with two characters
   * present there is no unambiguous "the character", so the tier is withheld
   * rather than merely empty.
   */
  it('offers the character tier for one LLM character and withholds it for several', async () => {
    const one = await render();
    await settle(one);
    expect(
      [...one.nativeElement.querySelectorAll('optgroup')].map((g: HTMLElement) =>
        g.getAttribute('label'),
      ),
    ).toContain('Character Scenarios');

    sent.length = 0;
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [Host],
      providers: [
        { provide: CoreClient, useValue: stubClient() },
        provideTanStackQuery(new QueryClient()),
      ],
    });
    const several = TestBed.createComponent(Host);
    several.componentInstance.llmCharacterIds.set(['char-1', 'char-2']);
    several.componentInstance.singleLlmCharacterId.set(null);
    several.detectChanges();
    await settle(several);
    expect(sent.some((r) => r.type === 'characterScenarioList')).toBe(false);
    expect(
      [...several.nativeElement.querySelectorAll('optgroup')].map((g: HTMLElement) =>
        g.getAttribute('label'),
      ),
    ).not.toContain('Character Scenarios');
  });

  it('keys the group tier on the comma-joined SORTED character ids', async () => {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [Host],
      providers: [
        { provide: CoreClient, useValue: stubClient() },
        provideTanStackQuery(new QueryClient()),
      ],
    });
    const fixture = TestBed.createComponent(Host);
    fixture.componentInstance.llmCharacterIds.set(['zeta', 'alpha']);
    fixture.componentInstance.singleLlmCharacterId.set(null);
    fixture.detectChanges();
    await settle(fixture);
    expect(sent.find((r) => r.type === 'groupScenariosUnion')!['characterIds']).toEqual([
      'alpha',
      'zeta',
    ]);
  });

  /**
   * THE bug `44a8137e` fixes. v4's chat GET never projected `scenarioText`, so
   * the picker could only ever open on "Custom…" — even immediately after a
   * save. With the projection in hand, a scene whose text matches a preset
   * exactly opens on that preset, with no textarea in sight.
   */
  it('opens on the PRESET whose body matches the scene in force', async () => {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [Host],
      providers: [
        { provide: CoreClient, useValue: stubClient() },
        provideTanStackQuery(new QueryClient()),
      ],
    });
    const fixture = TestBed.createComponent(Host);
    fixture.componentInstance.scenarioText.set('A low-ceilinged room.');
    fixture.detectChanges();
    await settle(fixture);

    expect(selectEl(fixture)!.value).toBe('project:Scenarios/inn.md');
    expect(textarea(fixture)).toBeNull();
    expect((fixture.nativeElement as HTMLElement).textContent).toContain('A low-ceilinged room.');
  });

  it('matches a general and a character preset too, trimming both sides', async () => {
    for (const [scene, expected] of [
      ['  A road in the rain.\n', 'general:Scenarios/road.md'],
      ['Pistols at dawn.', 'cs-1'],
    ] as const) {
      TestBed.resetTestingModule();
      TestBed.configureTestingModule({
        imports: [Host],
        providers: [
          { provide: CoreClient, useValue: stubClient() },
          provideTanStackQuery(new QueryClient()),
        ],
      });
      const fixture = TestBed.createComponent(Host);
      fixture.componentInstance.scenarioText.set(scene);
      fixture.detectChanges();
      await settle(fixture);
      expect(selectEl(fixture)!.value).toBe(expected);
    }
  });

  it('opens on Custom with the text ready to edit when nothing matches', async () => {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [Host],
      providers: [
        { provide: CoreClient, useValue: stubClient() },
        provideTanStackQuery(new QueryClient()),
      ],
    });
    const fixture = TestBed.createComponent(Host);
    fixture.componentInstance.scenarioText.set('A scene nobody wrote down.');
    fixture.detectChanges();
    await settle(fixture);

    expect(selectEl(fixture)!.value).toBe('__custom__');
    expect(textarea(fixture)!.value).toBe('A scene nobody wrote down.');
  });

  it('posts the tier fields for a preset and NO free text', async () => {
    const fixture = await render();
    await settle(fixture);
    const el = selectEl(fixture)!;
    el.value = 'project:Scenarios/inn.md';
    el.dispatchEvent(new Event('change'));
    fixture.detectChanges();

    saveButton(fixture).click();
    await settle(fixture);

    const req = sent.find((r) => r.type === 'chatSetScenario')!;
    expect(req).toEqual({
      type: 'chatSetScenario',
      chatId: 'chat-1',
      projectScenarioPath: 'Scenarios/inn.md',
    });
    expect(fixture.componentInstance.refetched).toBe(1);
  });

  it('posts free text under Custom, and an EMPTY box clears the scene', async () => {
    const fixture = await render();
    await settle(fixture);
    saveButton(fixture).click();
    await settle(fixture);

    expect(sent.find((r) => r.type === 'chatSetScenario')).toEqual({
      type: 'chatSetScenario',
      chatId: 'chat-1',
      scenario: '',
    });
  });

  it('carries typed free text through', async () => {
    const fixture = await render();
    await settle(fixture);
    const box = textarea(fixture)!;
    box.value = 'They meet at dusk.';
    box.dispatchEvent(new Event('input'));
    fixture.detectChanges();

    saveButton(fixture).click();
    await settle(fixture);
    expect(sent.find((r) => r.type === 'chatSetScenario')!['scenario']).toBe('They meet at dusk.');
  });

  /**
   * v4 toasts whatever the server called it — including the no-op arm. There is
   * no separate "nothing happened" UX; `changed: false` simply reads
   * "Scenario unchanged". (Tier 2 of the order: surveyed, not invented.)
   */
  it('toasts the server’s own message, the no-op arm included', async () => {
    for (const message of ['Scenario updated', 'Scenario cleared', 'Scenario unchanged'] as const) {
      saveAnswer = { scenarioText: null, changed: message !== 'Scenario unchanged', message };
      const fixture = await render();
      await settle(fixture);
      saveButton(fixture).click();
      await settle(fixture);
      expect(toasts()).toContain(message);
    }
  });

  it('falls back to v4’s own sentence when the reply carries no message', async () => {
    saveAnswer = { scenarioText: null, changed: true };
    const fixture = await render();
    await settle(fixture);
    saveButton(fixture).click();
    await settle(fixture);
    expect(toasts()).toContain('Scenario updated');
  });

  it('surfaces a failure’s sentence and does NOT tell the parent to refetch', async () => {
    saveAnswer = new CoreDispatchError({
      kind: 'not_found',
      message: 'Chat not found',
    } as never);
    const fixture = await render();
    await settle(fixture);
    saveButton(fixture).click();
    await settle(fixture);
    expect(toasts()).toContain('Chat not found');
    expect(fixture.componentInstance.refetched).toBe(0);
  });

  it('hides the dropdown entirely when no tier has anything to offer', async () => {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [Host],
      providers: [
        {
          provide: CoreClient,
          useValue: {
            dispatchData: (async (req: Req) => {
              sent.push(req);
              if (req['type'] === 'chatSetScenario') return saveAnswer;
              return { mountPointId: null, scenarios: [], warnings: [], groupScenarios: [] };
            }) as unknown as CoreClient['dispatchData'],
          },
        },
        provideTanStackQuery(new QueryClient()),
      ],
    });
    const fixture = TestBed.createComponent(Host);
    fixture.detectChanges();
    await settle(fixture);
    expect(selectEl(fixture)).toBeNull();
    // The free-text box still stands — Custom is always available.
    expect(textarea(fixture)).not.toBeNull();
  });

  /** The host must be a real box: an inline custom element loses the stack's gap. */
  /**
   * P4.D121 — "Show archived" (v4 `d25dacc1`). The flip is a NEW request on
   * every tier, and the flag is part of each cache key, so the archived-free
   * answer and the archived-inclusive one never overwrite each other.
   */
  it('renders the Show archived checkbox and re-fetches all four tiers on the flip', async () => {
    const fixture = await render();
    const box = (fixture.nativeElement as HTMLElement).querySelector(
      'input[type="checkbox"].qt-checkbox',
    ) as HTMLInputElement;
    expect(box).toBeTruthy();
    expect(box.checked).toBe(false);
    expect((fixture.nativeElement as HTMLElement).textContent).toContain('Show archived');

    const listTypes = ['scenarioList', 'projectScenarioList', 'characterScenarioList'];
    for (const t of listTypes) {
      expect(sent.filter((r) => r['type'] === t).every((r) => r['includeArchived'] === false)).toBe(
        true,
      );
    }
    const before = sent.filter((r) => listTypes.includes(r['type'] as string)).length;

    box.checked = true;
    box.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    await settle(fixture);

    const after = sent.filter((r) => listTypes.includes(r['type'] as string));
    expect(after.length).toBeGreaterThan(before);
    expect(after.slice(before).every((r) => r['includeArchived'] === true)).toBe(true);
  });

  it('gives its host a block display', async () => {
    const fixture = await render();
    const host = fixture.nativeElement.querySelector('qt-chat-scenario-control') as HTMLElement;
    expect(host.classList.contains('block')).toBe(true);
  });
});
