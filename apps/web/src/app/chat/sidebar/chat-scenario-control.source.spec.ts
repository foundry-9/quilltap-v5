import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { beforeEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { CoreDispatchError, type ScenarioSourceInfo } from '../../core/core-contract';
import { ToastService } from '../../ui/toast.service';
import { chatKeys } from '../chat-keys';
import { ChatScenarioControl } from './chat-scenario-control';

/**
 * The in-chat scenario picker (Salon sidebar → Chat → Scenario), transcribed
 * case for case from v4 `__tests__/unit/components/chat/chat-scenario-control.test.tsx`
 * at the pin `83334b9a5` (rewritten by `545e299bb`; 21 cases).
 *
 * The subtle part is the seeding rule. The control never copies the chat's
 * current scene into state; it DERIVES the picker position from it, so the
 * position keeps settling as the four option tiers arrive — and stops the
 * moment the user makes a choice of their own. A chat that recorded where its
 * scene came from (`chatGetScenarioSource`) is seeded from that source
 * instead — the body match is only the fallback for a chat with no source.
 * The provenance line and "Reload from source" hang off the same answer.
 *
 * v4 stubs `fetch` by URL; v5 stubs the dispatch verbs one for one. v4's
 * `toast` mock is v5's `ToastService` stub. Three P4.D302 rows follow v4's
 * 21: L-K's verbatim refusal, L-H(3)'s save ordering, and the store preview.
 */

interface Req {
  type: string;
  [k: string]: unknown;
}

const GENERAL = [
  { path: 'general/masquerade.md', filename: 'masquerade.md', name: 'A Masquerade', isDefault: false, body: 'Masks are worn.' },
];
const PROJECT = [
  { path: 'project/atrium.md', filename: 'atrium.md', name: 'The Atrium', isDefault: false, body: 'Glass overhead.' },
];
const CHARACTER = [{ id: 'sc-1', title: 'Her Study', content: 'Books to the ceiling.' }];
const GROUP = [
  {
    groupId: 'g-1',
    groupName: 'The Company',
    scenarios: [{ path: 'group/parlour.md', filename: 'parlour.md', name: 'The Parlour', isDefault: false, body: 'A low fire.' }],
  },
];

/** A chat that recorded no source — the body match decides. */
const NO_SOURCE: ScenarioSourceInfo = {
  source: null,
  owner: null,
  scenarioName: null,
  scenarioId: null,
  exists: false,
  changed: false,
  notes: null,
};

/** A chat whose scene was taken from the project's Atrium. */
function atriumSource(overrides: Partial<ScenarioSourceInfo> = {}): ScenarioSourceInfo {
  return {
    source: { mountPointId: 'mp-proj', path: 'project/atrium.md', bodySha256: 'abc' },
    owner: { scope: 'project', id: 'proj-1', name: 'Glasshouse' },
    scenarioName: 'The Atrium',
    scenarioId: null,
    exists: true,
    changed: false,
    notes: null,
    ...overrides,
  };
}

let sent: Req[] = [];
let answer: (req: Req) => unknown = () => ({});
const toasts = { success: [] as string[], error: [] as string[] };

/** Route each verb to its payload; anything else is an empty tier. */
function stubTiers(source: ScenarioSourceInfo = NO_SOURCE): void {
  answer = (req) => {
    switch (req.type) {
      case 'chatGetScenarioSource':
        return source;
      case 'chatSetScenario':
        return { scenarioText: 'x', changed: true, message: 'Scenario updated' };
      case 'scenarioList':
        return { scenarios: GENERAL };
      case 'projectScenarioList':
        return { scenarios: PROJECT };
      case 'groupScenariosUnion':
        return { groupScenarios: GROUP };
      case 'characterScenarioList':
        return { scenarios: CHARACTER };
      case 'scenarioReadByRef':
        return { scenario: { body: 'A borrowed scene.' }, owner: {} };
      default:
        return {};
    }
  };
}

const core = {
  dispatchData: async (req: Req) => {
    sent.push(req);
    const out = answer(req);
    if (out instanceof Error) throw out;
    return out;
  },
} as unknown as CoreClient;

@Component({
  imports: [ChatScenarioControl],
  template: `
    <qt-chat-scenario-control
      [chatId]="'chat-1'"
      [projectId]="projectId()"
      [scenarioText]="scenarioText()"
      [llmCharacterIds]="['char-1']"
      [singleLlmCharacterId]="'char-1'"
      [enabled]="enabled()"
      (chatUpdated)="onChatUpdated()"
    />
  `,
})
class Host {
  readonly projectId = signal<string | null>('proj-1');
  readonly scenarioText = signal<string | null>(null);
  readonly enabled = signal(true);
  updates = 0;
  /** How many source reads had been made at the moment the parent was told. */
  sourceReadsAtUpdate: number | null = null;
  /** Whether the draft was already dropped then (the Notes box re-derived). */
  sourceDataAtUpdate: unknown = null;
  onChatUpdated(): void {
    this.updates += 1;
    this.sourceReadsAtUpdate = sent.filter((r) => r.type === 'chatGetScenarioSource').length;
    this.sourceDataAtUpdate = queryClient.getQueryData(chatKeys.scenarioSource('chat-1'));
  }
}

let queryClient: QueryClient;

async function render(
  props: { scenarioText?: string | null; enabled?: boolean } = {},
): Promise<ComponentFixture<Host>> {
  queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [Host],
    providers: [
      { provide: CoreClient, useValue: core },
      provideTanStackQuery(queryClient),
      {
        provide: ToastService,
        useValue: {
          showSuccess: (m: string) => toasts.success.push(m),
          showError: (m: string) => toasts.error.push(m),
        },
      },
    ],
  });
  const fixture = TestBed.createComponent(Host);
  if (props.scenarioText !== undefined) fixture.componentInstance.scenarioText.set(props.scenarioText);
  if (props.enabled !== undefined) fixture.componentInstance.enabled.set(props.enabled);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

async function settle(fixture: ComponentFixture<Host>): Promise<void> {
  for (let i = 0; i < 8; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

const el = (f: ComponentFixture<Host>) => f.nativeElement as HTMLElement;
const byLabel = <T extends HTMLElement>(f: ComponentFixture<Host>, label: string): T | null =>
  el(f).querySelector<T>(`[aria-label="${label}"]`);
const select = (f: ComponentFixture<Host>) => byLabel<HTMLSelectElement>(f, 'Scenario')!;
const customBox = (f: ComponentFixture<Host>) => byLabel<HTMLTextAreaElement>(f, 'Custom scenario text');
const notesBox = (f: ComponentFixture<Host>) => byLabel<HTMLTextAreaElement>(f, 'Notes beneath the scene');
const text = (f: ComponentFixture<Host>) => (el(f).textContent ?? '').replace(/\s+/g, ' ');
const buttonNamed = (f: ComponentFixture<Host>, name: string) =>
  [...el(f).querySelectorAll('button')].find((b) => b.textContent!.trim() === name) as
    | HTMLButtonElement
    | undefined;
/** The provenance line — the element whose text starts `From:`. */
const provenance = (f: ComponentFixture<Host>) =>
  [...el(f).querySelectorAll('div')].find((d) => /^\s*From:/.test(d.textContent ?? '')) ?? null;

async function type(f: ComponentFixture<Host>, box: HTMLTextAreaElement, value: string): Promise<void> {
  box.value = value;
  box.dispatchEvent(new Event('input'));
  await settle(f);
}

async function choose(f: ComponentFixture<Host>, value: string): Promise<void> {
  const s = select(f);
  s.value = value;
  s.dispatchEvent(new Event('change'));
  await settle(f);
}

async function clickSave(f: ComponentFixture<Host>): Promise<void> {
  buttonNamed(f, 'Change scenario')!.click();
  await settle(f);
}

/** The one `chatSetScenario`, minus the envelope v4's URL carried. */
function savedPayload(): Record<string, unknown> {
  const call = sent.find((r) => r.type === 'chatSetScenario');
  expect(call).toBeDefined();
  const { type: _t, chatId, ...body } = call!;
  expect(chatId).toBe('chat-1');
  return body;
}

beforeEach(() => {
  sent = [];
  toasts.success = [];
  toasts.error = [];
});

describe('ChatScenarioControl (v4 chat-scenario-control.test.tsx @ 83334b9a5)', () => {
  describe("seeding from the chat's current scene", () => {
    it('opens on Custom with the text ready to edit when the scene matches no preset', async () => {
      stubTiers();
      const f = await render({ scenarioText: 'A scene nobody wrote down.' });
      expect(customBox(f)!.value).toBe('A scene nobody wrote down.');
    });

    it('opens on Custom with an empty box when the chat has no scene at all', async () => {
      stubTiers();
      const f = await render({ scenarioText: null });
      expect(customBox(f)!.value).toBe('');
    });

    it('preselects the preset once its tier arrives, rather than staying on Custom', async () => {
      stubTiers();
      const f = await render({ scenarioText: 'Masks are worn.' });
      expect(customBox(f)).toBeNull();
      expect(text(f)).toContain('Masks are worn.');
    });

    it('matches a preset whose body differs only by surrounding whitespace', async () => {
      stubTiers();
      const f = await render({ scenarioText: '  Masks are worn.\n' });
      expect(customBox(f)).toBeNull();
    });

    it('reads a preset with notes layered beneath it as Custom, not as the preset', async () => {
      stubTiers();
      const withNotes = 'Masks are worn.\n\nAnd one guest is not who they claim.';
      const f = await render({ scenarioText: withNotes });
      expect(customBox(f)!.value).toBe(withNotes);
    });
  });

  describe('seeding from the recorded source', () => {
    it('prefers the source over the body match, preselecting a project preset whose body no longer matches', async () => {
      stubTiers(atriumSource({ notes: 'And the glass is cracked.' }));
      const f = await render({ scenarioText: 'Glass overhead, once.\n\nAnd the glass is cracked.' });
      expect(select(f).value).toBe('project:project/atrium.md');
      expect(customBox(f)).toBeNull();
      // The notes beneath the scene come from the source answer, not the text.
      expect(notesBox(f)!.value).toBe('And the glass is cracked.');
    });

    it('falls back to the body match when the chat recorded no source', async () => {
      stubTiers(NO_SOURCE);
      const f = await render({ scenarioText: 'Glass overhead.' });
      expect(select(f).value).toBe('project:project/atrium.md');
      expect(provenance(f)).toBeNull();
    });

    it('shows a source outside the hierarchy under "Elsewhere", labelled Owner › Name', async () => {
      stubTiers(
        atriumSource({
          source: { mountPointId: 'mp-far', path: 'Scenarios/attic.md', bodySha256: 'x' },
          owner: { scope: 'project', id: 'proj-other', name: 'Farhouse' },
          scenarioName: 'The Attic',
        }),
      );
      const f = await render({ scenarioText: 'Dust and trunks.' });
      expect(select(f).value).toBe('store:mp-far:Scenarios/attic.md');
      const elsewhere = select(f).querySelector('optgroup[label="Elsewhere"]');
      expect(elsewhere?.textContent!.trim()).toBe('Farhouse › The Attic');
    });
  });

  describe('provenance', () => {
    it('names where the scene came from, with no reload offered while the shelf copy is unchanged', async () => {
      stubTiers(atriumSource());
      const f = await render({ scenarioText: 'Glass overhead.' });
      expect(provenance(f)!.textContent!.replace(/\s+/g, ' ').trim()).toBe('From: Glasshouse › The Atrium');
      expect([...provenance(f)!.querySelectorAll('em')].map((e) => e.textContent)).toEqual([
        'Glasshouse',
        'The Atrium',
      ]);
      expect(buttonNamed(f, 'Reload from source')).toBeUndefined();
    });

    it('says so when the shelf copy is gone', async () => {
      stubTiers(atriumSource({ exists: false }));
      const f = await render({ scenarioText: 'Glass overhead.' });
      expect(provenance(f)!.textContent).toContain('From: Glasshouse › The Atrium — the shelf copy is gone');
      expect(buttonNamed(f, 'Reload from source')).toBeUndefined();
    });

    it('keeps the scene it has when the shelf copy is gone, rather than a ref that would save as notes alone', async () => {
      stubTiers(atriumSource({ exists: false, notes: 'Keep the cracked pane.' }));
      const scene = 'Glass overhead.\n\nKeep the cracked pane.';
      const f = await render({ scenarioText: scene });
      expect(text(f)).toContain('the shelf copy is gone');
      expect(customBox(f)!.value).toBe(scene);
    });

    it('offers Reload when the shelf copy changed, and reload posts the ref with the same notes', async () => {
      stubTiers(atriumSource({ changed: true, notes: 'Keep the cracked pane.' }));
      const f = await render({ scenarioText: 'Glass overhead, once.\n\nKeep the cracked pane.' });
      expect(provenance(f)!.textContent!.replace(/\s+/g, ' ')).toContain(
        'From: Glasshouse › The Atrium — the shelf copy has been revised Reload from source',
      );
      const reload = buttonNamed(f, 'Reload from source')!;
      expect(reload.className).toBe('qt-button-secondary qt-button-sm ml-1');
      reload.click();
      await settle(f);
      expect(f.componentInstance.updates).toBe(1);
      expect(savedPayload()).toEqual({
        scenarioRef: { mountPointId: 'mp-proj', path: 'project/atrium.md' },
        scenario: 'Keep the cracked pane.',
        // Only Reload asks for the revised copy; an ordinary save keeps the scene.
        reload: true,
      });
      expect(JSON.stringify(sent.find((r) => r.type === 'chatSetScenario'))).toBe(
        '{"type":"chatSetScenario","chatId":"chat-1","scenarioRef":{"mountPointId":"mp-proj","path":"project/atrium.md"},"scenario":"Keep the cracked pane.","reload":true}',
      );
    });
  });

  describe('the archived toggle', () => {
    it('re-fetches every tier with includeArchived so the two answers cache apart', async () => {
      stubTiers();
      const f = await render();
      expect(sent.some((r) => r['includeArchived'] === true)).toBe(false);
      const label = [...el(f).querySelectorAll('label')].find((l) => l.textContent!.includes('Show archived'))!;
      const box = label.querySelector('input')!;
      box.checked = true;
      box.dispatchEvent(new Event('change'));
      await settle(f);
      expect(sent.filter((r) => r['includeArchived'] === true).length).toBeGreaterThan(0);
    });
  });

  describe('saving', () => {
    it('posts the custom text to the scenario action and drops the draft afterwards', async () => {
      stubTiers();
      const f = await render();
      await type(f, customBox(f)!, 'Rain on the conservatory glass.');
      await clickSave(f);
      expect(f.componentInstance.updates).toBe(1);
      expect(savedPayload()).toEqual({ scenario: 'Rain on the conservatory glass.' });
      expect(toasts.success).toEqual(['Scenario updated']);
    });

    it('posts the preset pointer with the notes beneath it as `scenario`', async () => {
      stubTiers();
      const f = await render();
      await choose(f, 'general:general/masquerade.md');
      await type(f, notesBox(f)!, 'One guest lies.');
      await clickSave(f);
      expect(f.componentInstance.updates).toBe(1);
      expect(savedPayload()).toEqual({
        generalScenarioPath: 'general/masquerade.md',
        scenario: 'One guest lies.',
      });
    });

    it('does not carry a custom scene beneath a preset picked over it', async () => {
      stubTiers();
      const f = await render();
      await type(f, customBox(f)!, 'A storm at sea.');
      await choose(f, 'general:general/masquerade.md');
      expect(notesBox(f)!.value).toBe('');
      await clickSave(f);
      expect(f.componentInstance.updates).toBe(1);
      expect(savedPayload()).toEqual({ generalScenarioPath: 'general/masquerade.md', scenario: '' });
    });

    it("surfaces the server's own error text instead of a bare status code", async () => {
      answer = (req) =>
        req.type === 'chatSetScenario'
          ? new CoreDispatchError({ kind: 'conflict', message: 'That scene is archived' } as never)
          : {};
      const f = await render();
      await clickSave(f);
      expect(toasts.error).toEqual(['That scene is archived']);
    });

    it('re-enables the button after a failed save rather than staying stuck', async () => {
      answer = (req) => (req.type === 'chatSetScenario' ? new Error('network down') : {});
      const f = await render();
      await clickSave(f);
      expect(toasts.error).toEqual(['network down']);
      expect(buttonNamed(f, 'Change scenario')!.disabled).toBe(false);
    });
  });

  describe('the dropdown', () => {
    it('is rendered even when no tier has anything to offer, so Browse is always reachable', async () => {
      answer = (req) =>
        req.type === 'chatGetScenarioSource' ? NO_SOURCE : { scenarios: [], groupScenarios: [] };
      const f = await render();
      expect(customBox(f)).not.toBeNull();
      expect([...select(f).querySelectorAll('option')].map((o) => o.getAttribute('value'))).toEqual([
        '__custom__',
        '__browse__',
      ]);
    });

    it('appears once any tier has options', async () => {
      stubTiers();
      const f = await render();
      expect(select(f)).not.toBeNull();
    });
  });

  it('fetches nothing until the section has been opened once', async () => {
    stubTiers();
    const f = await render({ enabled: false });
    expect(customBox(f)).not.toBeNull();
    expect(sent).toEqual([]);
  });
});

describe('ChatScenarioControl — P4.D302 rows beyond v4’s file', () => {
  it('toasts a refusal byte-for-byte as the server said it (L-K)', async () => {
    stubTiers();
    const base = answer;
    answer = (req) =>
      req.type === 'chatSetScenario'
        ? new CoreDispatchError({
            kind: 'badRequest',
            message: 'Invalid scenarioRef: That store does not hold scenarios',
          } as never)
        : base(req);
    const f = await render();
    await clickSave(f);
    expect(toasts.error).toEqual(['Invalid scenarioRef: That store does not hold scenarios']);
    expect(f.componentInstance.updates).toBe(0);
  });

  it('drops the draft, AWAITS the source invalidation, THEN tells the parent (L-H(3))', async () => {
    stubTiers(atriumSource());
    const f = await render({ scenarioText: 'Glass overhead.' });
    await type(f, notesBox(f)!, 'And a draught.');
    await clickSave(f);
    expect(f.componentInstance.updates).toBe(1);
    // The source was invalidated AND refetched (the await) before the parent
    // heard of the save…
    expect(f.componentInstance.sourceReadsAtUpdate).toBe(2);
    expect(f.componentInstance.sourceDataAtUpdate).toBeDefined();
    // …and the control re-derives from the server's answer, not the draft.
    expect(sent.filter((r) => r.type === 'chatGetScenarioSource')).toHaveLength(2);
    expect(notesBox(f)!.value).toBe('');
  });

  it('previews a store selection’s body, read by ref, under the dropdown', async () => {
    stubTiers(
      atriumSource({
        source: { mountPointId: 'mp-far', path: 'Scenarios/attic.md', bodySha256: 'x' },
        owner: { scope: 'project', id: 'proj-other', name: 'Farhouse' },
        scenarioName: 'The Attic',
      }),
    );
    const f = await render({ scenarioText: 'Dust and trunks.' });
    expect(sent.filter((r) => r.type === 'scenarioReadByRef')).toEqual([
      { type: 'scenarioReadByRef', mountPointId: 'mp-far', path: 'Scenarios/attic.md' },
    ]);
    expect(text(f)).toContain('A borrowed scene.');
  });

  it('seeds a general, group and character source natively, and labels a path-only store', async () => {
    const ref = (path: string) => ({ mountPointId: 'mp-x', path, bodySha256: 'x' });
    const cases: Array<[Partial<ScenarioSourceInfo>, string]> = [
      [
        { owner: { scope: 'general', id: null, name: 'Quilltap General' }, source: ref('general/masquerade.md') },
        'general:general/masquerade.md',
      ],
      [
        { owner: { scope: 'group', id: 'g-1', name: 'The Company' }, source: ref('group/parlour.md') },
        'group:g-1:group/parlour.md',
      ],
      [{ owner: { scope: 'character', id: 'char-1', name: 'Alice' }, scenarioId: 'sc-1' }, 'sc-1'],
    ];
    for (const [over, value] of cases) {
      stubTiers(atriumSource(over));
      const f = await render({ scenarioText: 'x' });
      expect(select(f).value).toBe(value);
    }
    // A character source WITHOUT a scenarioId, and a source with no name, is a store.
    stubTiers(
      atriumSource({
        owner: { scope: 'character', id: 'char-1', name: 'Alice' },
        scenarioName: null,
        source: { mountPointId: 'mp-alice', path: 'Scenarios/her-study.md', bodySha256: 'x' },
      }),
    );
    const f = await render({ scenarioText: 'x' });
    expect(select(f).value).toBe('store:mp-alice:Scenarios/her-study.md');
    expect(select(f).querySelector('optgroup[label="Elsewhere"]')!.textContent!.trim()).toBe(
      'Alice › her-study',
    );
  });

  it('places the provenance line after the dropdown and before Show archived and the Host', async () => {
    stubTiers(atriumSource());
    const f = await render({ scenarioText: 'Glass overhead.' });
    const order = [...el(f).querySelectorAll('select, div, label, button')]
      .map((n) =>
        n.tagName === 'SELECT'
          ? 'select'
          : /^\s*From:/.test(n.textContent ?? '') && n.tagName === 'DIV'
            ? 'provenance'
            : n.textContent!.includes('Show archived') && n.tagName === 'LABEL'
              ? 'archived'
              : n.textContent!.includes('Ask the Host') && n.tagName === 'BUTTON'
                ? 'host'
                : null,
      )
      .filter((x) => x !== null);
    expect(order.filter((x, i) => order.indexOf(x) === i)).toEqual([
      'select',
      'provenance',
      'archived',
      'host',
    ]);
  });
});
