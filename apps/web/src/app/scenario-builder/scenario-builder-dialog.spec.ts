import { TestBed, type ComponentFixture } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { CoreResponse, ScopedEvent } from '../core/core-contract';
import { RichEditor } from '../editor/rich-editor';
import { ToastService } from '../ui/toast.service';
import { SAVE_NEEDS_A_NAME, type SavedScenarioTarget } from './save-scenario-dialog';
import { ScenarioBuilderDialog } from './scenario-builder-dialog';
import { fakeCore, profile, type FakeCore } from './scenario-builder-dialog.testing';

/**
 * The Scenario Builder dialog (v4 `ScenarioBuilderDialog.tsx` at `d1c06cd9d`).
 *
 * The first describe blocks are v4's own `ScenarioBuilderDialog.test.tsx`
 * cases, by name (the five `describeHostActivity` cases live in
 * `host-activity.spec.ts` with the recorded corpus); the rest pin the rules
 * v4's suite does not reach — the no-tools note, the warning's two suffixes,
 * the running pane's marks, Stop, the close refusal, Enter-to-revise, and the
 * save dialog's targets, description lock, validation, toast and invalidation.
 */

const DEFAULT_PROFILE = profile({ id: 'profile-default', name: 'The Usual', isDefault: true });
const NO_TOOLS_PROFILE = profile({
  id: 'profile-no-tools',
  name: 'Cheap Model',
  allowToolUse: false,
});
const NO_WEB_PROFILE = profile({ id: 'profile-no-web', name: 'No Web', allowWebSearch: false });

const CAST = [{ id: 'char-1', name: 'Alice' }];

interface Rendered {
  fixture: ComponentFixture<ScenarioBuilderDialog>;
  fake: FakeCore;
  used: string[];
  saved: SavedScenarioTarget[];
  closed: { count: number };
  toasts: { showSuccess: ReturnType<typeof vi.fn>; showError: ReturnType<typeof vi.fn> };
  queryClient: QueryClient;
  el: HTMLElement;
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function render(
  opts: {
    profiles?: Record<string, unknown>[];
    cast?: { id: string; name: string }[];
    projectId?: string | null;
    projectName?: string | null;
    chatId?: string | null;
    configure?: (fake: FakeCore) => void;
    /** Pre-seed the query cache (with the app's 5 s `staleTime`, `app.config.ts`). */
    seed?: (queryClient: QueryClient) => void;
  } = {},
): Promise<Rendered> {
  const fake = fakeCore();
  fake.profiles = opts.profiles ?? [DEFAULT_PROFILE];
  opts.configure?.(fake);
  const toasts = { showSuccess: vi.fn(), showError: vi.fn() };
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false, ...(opts.seed ? { staleTime: 5_000 } : {}) } },
  });
  opts.seed?.(queryClient);
  TestBed.configureTestingModule({
    imports: [ScenarioBuilderDialog],
    providers: [
      provideTanStackQuery(queryClient),
      { provide: CoreClient, useValue: fake.core },
      { provide: ToastService, useValue: toasts },
    ],
  });
  const fixture = TestBed.createComponent(ScenarioBuilderDialog);
  fixture.componentRef.setInput('cast', opts.cast ?? CAST);
  if (opts.projectId !== undefined) fixture.componentRef.setInput('projectId', opts.projectId);
  if (opts.projectName !== undefined) {
    fixture.componentRef.setInput('projectName', opts.projectName);
  }
  if (opts.chatId !== undefined) fixture.componentRef.setInput('chatId', opts.chatId);
  const used: string[] = [];
  const saved: SavedScenarioTarget[] = [];
  const closed = { count: 0 };
  fixture.componentInstance.use.subscribe((s) => used.push(s));
  fixture.componentInstance.saved.subscribe((t) => saved.push(t));
  fixture.componentInstance.closed.subscribe(() => closed.count++);
  fixture.detectChanges();
  await settle(fixture);
  return {
    fixture,
    fake,
    used,
    saved,
    closed,
    toasts,
    queryClient,
    el: fixture.nativeElement as HTMLElement,
  };
}

afterEach(() => TestBed.resetTestingModule());

function byId<T extends HTMLElement>(r: Rendered, id: string): T {
  const found = r.el.querySelector(`#${id}`);
  if (!found) throw new Error(`#${id} not rendered`);
  return found as T;
}

function button(r: Rendered, text: string): HTMLButtonElement {
  const found = Array.from(r.el.querySelectorAll('button')).find(
    (b) => (b.textContent ?? '').trim() === text,
  );
  if (!found) throw new Error(`button "${text}" not rendered`);
  return found as HTMLButtonElement;
}

function queryButton(r: Rendered, text: string): HTMLButtonElement | undefined {
  return Array.from(r.el.querySelectorAll('button')).find(
    (b) => (b.textContent ?? '').trim() === text,
  ) as HTMLButtonElement | undefined;
}

function type(el: HTMLInputElement | HTMLSelectElement, value: string): void {
  el.value = value;
  el.dispatchEvent(new Event(el instanceof HTMLSelectElement ? 'change' : 'input'));
}

async function fillInputs(r: Rendered): Promise<void> {
  type(byId<HTMLInputElement>(r, 'scenario-builder-location'), 'the Lantern Inn');
  type(byId<HTMLInputElement>(r, 'scenario-builder-time'), 'a rainy evening');
  await settle(r.fixture);
}

async function setTheScene(r: Rendered): Promise<void> {
  button(r, 'Set the scene').click();
  await settle(r.fixture);
}

/** The review pane's draft editor (the only markdown field on that pane). */
function sceneEditor(r: Rendered): RichEditor {
  return r.fixture.debugElement.query(By.directive(RichEditor)).componentInstance as RichEditor;
}

function statusText(r: Rendered): string | null {
  return r.el.querySelector('[role="status"]')?.textContent?.replace(/\s+/g, ' ').trim() ?? null;
}

function alertText(r: Rendered): string | null {
  return r.el.querySelector('[role="alert"]')?.textContent?.replace(/\s+/g, ' ').trim() ?? null;
}

async function runToReview(r: Rendered, scenario = 'Rain on the cobbles.'): Promise<void> {
  r.fake.buildFrames.push([{ done: true, scenario }]);
  await fillInputs(r);
  await setTheScene(r);
  expect(r.el.querySelector('[aria-label="The scene"]')).not.toBeNull();
}

describe('ScenarioBuilderDialog — model selection (v4)', () => {
  it('preselects the isDefault profile in the Model select', async () => {
    const r = await render({ profiles: [profile({ id: 'p-a' }), DEFAULT_PROFILE] });
    expect(byId<HTMLSelectElement>(r, 'scenario-builder-profile').value).toBe('profile-default');
  });

  it('renders a no-tools profile as a disabled option labelled "(no tools)"', async () => {
    const r = await render({ profiles: [DEFAULT_PROFILE, NO_TOOLS_PROFILE] });
    const option = Array.from(r.el.querySelectorAll('option')).find((o) =>
      /Cheap Model \(no tools\)/.test(o.textContent ?? ''),
    ) as HTMLOptionElement;
    expect(option).toBeDefined();
    expect(option.disabled).toBe(true);
  });
});

describe('ScenarioBuilderDialog — the profile cache entry (the d1c06cd9d unification review)', () => {
  it('reads its own entry, not the bare key the home page fills with a flag-less shape', async () => {
    // The home page / Brahma console cache `{id, name, provider, modelName[,
    // isDefault]}` under the bare `['connectionProfiles']` — no tool flags.
    const r = await render({
      profiles: [DEFAULT_PROFILE, NO_TOOLS_PROFILE],
      seed: (qc) =>
        qc.setQueryData(
          ['connectionProfiles'],
          [DEFAULT_PROFILE, NO_TOOLS_PROFILE].map(
            ({ id, name, provider, modelName, isDefault }) => ({
              id,
              name,
              provider,
              modelName,
              isDefault,
            }),
          ),
        ),
    });
    const option = Array.from(r.el.querySelectorAll('option')).find((o) =>
      /Cheap Model \(no tools\)/.test(o.textContent ?? ''),
    ) as HTMLOptionElement;
    expect(option).toBeDefined();
    expect(option.disabled).toBe(true);
  });
});

describe('ScenarioBuilderDialog — web-search warning (v4)', () => {
  it('shows the web-unreachable warning in real mode when the profile lacks allowWebSearch', async () => {
    const r = await render({ profiles: [NO_WEB_PROFILE] });
    (r.el.querySelector('input[value="real"]') as HTMLInputElement).click();
    await settle(r.fixture);
    expect(statusText(r)).toMatch(/wider world is out of reach/i);
  });

  it('does not show the warning in in-world mode', async () => {
    const r = await render({ profiles: [NO_WEB_PROFILE] });
    // Default mode is in-world with a non-empty cast.
    expect(statusText(r)).toBeNull();
  });
});

describe('ScenarioBuilderDialog — running a build (v4)', () => {
  it('POSTs to the build endpoint with mode, location, time, profile, and cast ids', async () => {
    const r = await render();
    r.fake.buildFrames.push([{ done: true, scenario: 'Rain on the cobbles.' }]);
    await fillInputs(r);
    await setTheScene(r);
    expect(r.fake.buildBodies).toHaveLength(1);
    expect(r.fake.buildBodies[0]).toMatchObject({
      mode: 'in-world',
      location: 'the Lantern Inn',
      time: 'a rainy evening',
      connectionProfileId: 'profile-default',
      characterIds: ['char-1'],
    });
  });

  it('shows the activity list and settles a tool call, then shows the draft on done', async () => {
    const r = await render();
    r.fake.buildFrames.push([
      { toolsDetected: 1, toolNames: ['search'], toolArguments: [{ query: 'inn' }] },
      { toolResult: { index: 0, success: true } },
      { done: true, scenario: 'Rain on the cobbles.' },
    ]);
    await fillInputs(r);
    await setTheScene(r);
    expect(r.el.querySelector('[aria-label="The scene"]')).not.toBeNull();
    expect(sceneEditor(r).getMarkdown()).toBe('Rain on the cobbles.');
  });

  it('"Use this scene" calls onUse with the draft and closes the dialog', async () => {
    const r = await render();
    await runToReview(r);
    button(r, 'Use this scene').click();
    expect(r.used).toEqual(['Rain on the cobbles.']);
    expect(r.closed.count).toBe(1);
  });

  it('Revise re-POSTs with priorDraft and revision', async () => {
    const r = await render();
    await runToReview(r);
    r.fake.buildFrames.push([{ done: true, scenario: 'Rain on the cobbles, two hours later.' }]);
    type(byId<HTMLInputElement>(r, 'scenario-builder-revision'), 'move it two hours later');
    await settle(r.fixture);
    button(r, 'Revise').click();
    await settle(r.fixture);
    expect(r.fake.buildBodies).toHaveLength(2);
    expect(r.fake.buildBodies[1]).toMatchObject({
      priorDraft: 'Rain on the cobbles.',
      revision: 'move it two hours later',
    });
    expect(sceneEditor(r).getMarkdown()).toBe('Rain on the cobbles, two hours later.');
  });

  it('a failed revise keeps the draft visible, shows the error, and leaves Use enabled', async () => {
    const r = await render();
    await runToReview(r);
    r.fake.buildFrames.push([{ error: 'boom' }]);
    type(byId<HTMLInputElement>(r, 'scenario-builder-revision'), 'try again');
    await settle(r.fixture);
    button(r, 'Revise').click();
    await settle(r.fixture);
    expect(alertText(r)).toContain('boom');
    expect(sceneEditor(r).getMarkdown()).toBe('Rain on the cobbles.');
    expect(button(r, 'Use this scene').disabled).toBe(false);
  });
});

describe('ScenarioBuilderDialog — saving (v4)', () => {
  it('"Save as scenario…" opens the save dialog', async () => {
    const r = await render();
    await runToReview(r);
    button(r, 'Save as scenario…').click();
    await settle(r.fixture);
    expect(r.el.textContent).toContain('File this scene as a scenario');
  });

  it('saving to Quilltap General POSTs {filename, name, body} and reports the saved path', async () => {
    const r = await render();
    await runToReview(r);
    r.fake.saveResponse = {
      type: 'scenarioBuilder',
      data: { path: 'Scenarios/rain.md' },
    } as unknown as CoreResponse;
    button(r, 'Save as scenario…').click();
    await settle(r.fixture);
    type(byId<HTMLInputElement>(r, 'save-scenario-name'), 'Rain on the cobbles');
    await settle(r.fixture);
    button(r, 'Save').click();
    await settle(r.fixture);
    expect(r.saved).toEqual([{ kind: 'general', path: 'Scenarios/rain.md' }]);
    const req = r.fake.requests.find((q) => q['type'] === 'scenarioCreate');
    expect(req).toEqual({
      type: 'scenarioCreate',
      scenario: {
        filename: 'Rain on the cobbles',
        name: 'Rain on the cobbles',
        body: 'Rain on the cobbles.',
      },
    });
  });

  it("saving to a character POSTs {title, content} to that character's scenarios endpoint", async () => {
    const r = await render();
    await runToReview(r);
    r.fake.saveResponse = {
      type: 'scenarioBuilder',
      data: { scenario: { id: 'scn-1' } },
    } as unknown as CoreResponse;
    button(r, 'Save as scenario…').click();
    await settle(r.fixture);
    type(byId<HTMLInputElement>(r, 'save-scenario-name'), "Alice's Rain");
    type(byId<HTMLSelectElement>(r, 'save-scenario-target'), 'character:char-1');
    await settle(r.fixture);
    button(r, 'Save').click();
    await settle(r.fixture);
    const req = r.fake.requests.find((q) => q['type'] === 'characterScenarioCreate');
    expect(req).toEqual({
      type: 'characterScenarioCreate',
      characterId: 'char-1',
      title: "Alice's Rain",
      content: 'Rain on the cobbles.',
    });
    expect(r.saved).toEqual([
      {
        kind: 'character',
        characterId: 'char-1',
        scenarioId: 'scn-1',
        title: "Alice's Rain",
        content: 'Rain on the cobbles.',
      },
    ]);
  });

  it('a 400 response keeps the save dialog open and shows the error', async () => {
    const r = await render();
    await runToReview(r);
    r.fake.saveResponse = {
      type: 'error',
      data: { kind: 'validation', message: 'That name is already in use.' },
    } as unknown as CoreResponse;
    button(r, 'Save as scenario…').click();
    await settle(r.fixture);
    type(byId<HTMLInputElement>(r, 'save-scenario-name'), 'Rain on the cobbles');
    await settle(r.fixture);
    button(r, 'Save').click();
    await settle(r.fixture);
    expect(alertText(r)).toBe('That name is already in use.');
    expect(r.el.textContent).toContain('File this scene as a scenario');
    expect(r.saved).toEqual([]);
  });
});

// --- Beyond v4's suite -------------------------------------------------------

describe('ScenarioBuilderDialog — inputs pane', () => {
  it('opens in real mode when the cast is empty (v4 `cast.length > 0 ? in-world : real`)', async () => {
    const r = await render({ cast: [] });
    expect((r.el.querySelector('input[value="real"]') as HTMLInputElement).checked).toBe(true);
    expect(r.el.textContent).toContain(
      'I shall consult the wider world, and your document stores besides.',
    );
  });

  it('opens in in-world mode with a cast, with the stores-only helper', async () => {
    const r = await render();
    expect((r.el.querySelector('input[value="in-world"]') as HTMLInputElement).checked).toBe(true);
    expect(r.el.textContent).toContain(
      'I shall read only the document stores this company could see — never the wider world.',
    );
  });

  it('carries v4’s placeholders and length caps', async () => {
    const r = await render();
    const location = byId<HTMLInputElement>(r, 'scenario-builder-location');
    const time = byId<HTMLInputElement>(r, 'scenario-builder-time');
    expect(location.placeholder).toBe("the Gare du Nord · the Lantern Inn at Vey's Crossing");
    expect(location.maxLength).toBe(500);
    expect(time.placeholder).toBe('an autumn evening, 1927 · now · the third day of the siege');
    expect(time.maxLength).toBe(200);
    expect(r.el.querySelector('[aria-label="Further details"]')).not.toBeNull();
  });

  it('marks the default "(Default)", falls back to the first tool-using profile, then the first', async () => {
    const noDefault = await render({
      profiles: [NO_TOOLS_PROFILE, profile({ id: 'p-b', name: 'B' })],
    });
    expect(byId<HTMLSelectElement>(noDefault, 'scenario-builder-profile').value).toBe('p-b');
    TestBed.resetTestingModule();
    // A default WITHOUT tools is skipped for the first profile that has them.
    const defaultNoTools = await render({
      profiles: [
        profile({ id: 'p-a', name: 'A' }),
        profile({ id: 'p-d', name: 'D', isDefault: true, allowToolUse: false }),
      ],
    });
    expect(byId<HTMLSelectElement>(defaultNoTools, 'scenario-builder-profile').value).toBe('p-a');
    const labels = Array.from(defaultNoTools.el.querySelectorAll('option')).map((o) =>
      (o.textContent ?? '').trim(),
    );
    expect(labels).toEqual(['A', 'D (Default) (no tools)']);
  });

  it('shows the no-tools note only when some profile lacks tools', async () => {
    const note = 'Profiles with tool use switched off are listed but cannot be chosen';
    const without = await render();
    expect(without.el.textContent).not.toContain(note);
    TestBed.resetTestingModule();
    const withIt = await render({ profiles: [DEFAULT_PROFILE, NO_TOOLS_PROFILE] });
    expect(withIt.el.textContent).toContain(note);
  });

  it('Set the scene waits for a location, a time and a tool-using profile', async () => {
    const r = await render();
    expect(button(r, 'Set the scene').disabled).toBe(true);
    type(byId<HTMLInputElement>(r, 'scenario-builder-location'), '  ');
    type(byId<HTMLInputElement>(r, 'scenario-builder-time'), 'now');
    await settle(r.fixture);
    expect(button(r, 'Set the scene').disabled).toBe(true);
    await fillInputs(r);
    expect(button(r, 'Set the scene').disabled).toBe(false);
    TestBed.resetTestingModule();
    const onlyNoTools = await render({ profiles: [NO_TOOLS_PROFILE] });
    await fillInputs(onlyNoTools);
    expect(button(onlyNoTools, 'Set the scene').disabled).toBe(true);
  });

  it('the warning says the profile forbids search, and names the missing provider otherwise', async () => {
    const r = await render({ profiles: [NO_WEB_PROFILE] });
    (r.el.querySelector('input[value="real"]') as HTMLInputElement).click();
    await settle(r.fixture);
    expect(statusText(r)).toBe(
      'I regret that the wider world is out of reach on this occasion — this profile does not permit web search. I shall make do with what I know and your stores, and keep the particulars general where I am unsure.',
    );
    TestBed.resetTestingModule();
    const noProvider = await render({
      configure: (fake) => {
        fake.capabilities = { webSearchConfigured: false, curlConfigured: false };
      },
    });
    (noProvider.el.querySelector('input[value="real"]') as HTMLInputElement).click();
    await settle(noProvider.fixture);
    expect(statusText(noProvider)).toBe(
      'I regret that the wider world is out of reach on this occasion — no search provider has been engaged. I shall make do with what I know and your stores, and keep the particulars general where I am unsure.',
    );
  });

  it('no warning in real mode when the profile may search and a provider is engaged', async () => {
    const r = await render();
    (r.el.querySelector('input[value="real"]') as HTMLInputElement).click();
    await settle(r.fixture);
    expect(statusText(r)).toBeNull();
    expect(r.fake.requests.some((q) => q['type'] === 'scenarioBuilderCapabilities')).toBe(true);
  });

  it('sends details raw, the project and chat ids, and null when absent', async () => {
    const r = await render({ projectId: 'proj-1', chatId: 'chat-1' });
    r.fake.buildFrames.push([{ done: true, scenario: 'x' }]);
    type(byId<HTMLInputElement>(r, 'scenario-builder-location'), '  the Lantern Inn  ');
    type(byId<HTMLInputElement>(r, 'scenario-builder-time'), ' dusk ');
    await settle(r.fixture);
    await setTheScene(r);
    expect(r.fake.buildBodies[0]).toEqual({
      mode: 'in-world',
      location: 'the Lantern Inn',
      time: 'dusk',
      details: '',
      connectionProfileId: 'profile-default',
      projectId: 'proj-1',
      characterIds: ['char-1'],
      chatId: 'chat-1',
    });
    TestBed.resetTestingModule();
    const bare = await render({ cast: [] });
    bare.fake.buildFrames.push([{ done: true, scenario: 'x' }]);
    await fillInputs(bare);
    await setTheScene(bare);
    expect(bare.fake.buildBodies[0]).toMatchObject({
      mode: 'real',
      projectId: null,
      chatId: null,
      characterIds: [],
    });
  });

  it('a failed first run lands back on the inputs with the error', async () => {
    const r = await render();
    r.fake.buildFrames.push([
      { error: 'The Host has been detained by circumstances beyond his control.' },
    ]);
    await fillInputs(r);
    await setTheScene(r);
    expect(r.el.querySelector('#scenario-builder-location')).not.toBeNull();
    expect(alertText(r)).toBe('The Host has been detained by circumstances beyond his control.');
  });
});

describe('ScenarioBuilderDialog — running pane', () => {
  async function startHeld(r: Rendered): Promise<string> {
    r.fake.holdBuild = true;
    await fillInputs(r);
    button(r, 'Set the scene').click();
    await settle(r.fixture);
    const build = r.fake.requests.find((q) => q['type'] === 'scenarioBuilderBuild')!;
    return build['runId'] as string;
  }

  function emit(r: Rendered, runId: string, frame: Record<string, unknown>): void {
    r.fake.stream.frames.next({
      type: 'scenarioBuilderProgress',
      progressId: runId,
      frame,
    } as unknown as ScopedEvent);
  }

  it('shows the Host out, each enquiry described, and the marks as they settle', async () => {
    const r = await render();
    const runId = await startHeld(r);
    expect(r.el.textContent).toContain('Pray bear with me; I am out making enquiries.');
    expect(r.el.textContent).toContain('The Host is out making enquiries…');
    expect(r.el.querySelector('[aria-label="The Host\'s enquiries"]')).toBeNull();
    emit(r, runId, {
      toolsDetected: 2,
      toolNames: ['search', 'doc_read_file'],
      toolArguments: [{ query: '"inn"' }, { path: 'Knowledge/vey.md' }],
    });
    await settle(r.fixture);
    const items = () =>
      Array.from(r.el.querySelectorAll('[aria-label="The Host\'s enquiries"] li'));
    expect(items().map((li) => (li.textContent ?? '').trim())).toEqual([
      'Leafing through the stores for “inn”',
      'Opening Knowledge/vey.md',
    ]);
    expect(items()[0].querySelector('qt-quill-animation')).not.toBeNull();
    emit(r, runId, { toolResult: { index: 0, success: true } });
    emit(r, runId, { toolResult: { index: 1, success: false, error: 'not found' } });
    await settle(r.fixture);
    expect(items()[0].querySelector('[aria-label="Done"]')).not.toBeNull();
    expect(items()[1].querySelector('[aria-label="Came to nothing"]')).not.toBeNull();
    emit(r, runId, { reasoning: 'The inn sits at the ford.' });
    await settle(r.fixture);
    expect(r.el.querySelector('qt-thinking-block')?.textContent).toContain(
      'The inn sits at the ford.',
    );
  });

  it('only Stop is offered, and the close button is refused while running', async () => {
    const r = await render();
    await startHeld(r);
    expect(queryButton(r, 'Stop')).toBeDefined();
    expect(queryButton(r, 'Cancel')).toBeUndefined();
    (r.el.querySelector('button[aria-label="Close"]') as HTMLButtonElement).click();
    await settle(r.fixture);
    expect(r.closed.count).toBe(0);
  });

  it('Stop aborts the run and returns to the inputs with no draft', async () => {
    const r = await render();
    const runId = await startHeld(r);
    button(r, 'Stop').click();
    await settle(r.fixture);
    expect(r.fake.requests.at(-1)).toEqual({ type: 'scenarioBuilderAbort', runId });
    expect(r.el.querySelector('#scenario-builder-location')).not.toBeNull();
    // The inputs survive the stop.
    expect(byId<HTMLInputElement>(r, 'scenario-builder-location').value).toBe('the Lantern Inn');
    expect(alertText(r)).toBeNull();
  });

  it('destroying the dialog mid-run aborts it', async () => {
    const r = await render();
    const runId = await startHeld(r);
    r.fixture.destroy();
    await new Promise((res) => setTimeout(res, 0));
    expect(r.fake.requests.at(-1)).toEqual({ type: 'scenarioBuilderAbort', runId });
  });
});

describe('ScenarioBuilderDialog — review pane', () => {
  it('carries the revise placeholder, and Enter revises', async () => {
    const r = await render();
    await runToReview(r);
    const input = byId<HTMLInputElement>(r, 'scenario-builder-revision');
    expect(input.placeholder).toBe('make it raining, and two hours later');
    expect(input.maxLength).toBe(2000);
    expect(button(r, 'Revise').disabled).toBe(true);
    r.fake.buildFrames.push([{ done: true, scenario: 'Later.' }]);
    type(input, '  later  ');
    await settle(r.fixture);
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    await settle(r.fixture);
    expect(r.fake.buildBodies[1]).toMatchObject({
      priorDraft: 'Rain on the cobbles.',
      revision: 'later',
    });
    // A successful revise clears the revision box.
    expect(byId<HTMLInputElement>(r, 'scenario-builder-revision').value).toBe('');
  });

  it('a failed revise names the draft untouched', async () => {
    const r = await render();
    await runToReview(r);
    r.fake.buildFrames.push([{ error: 'boom' }]);
    type(byId<HTMLInputElement>(r, 'scenario-builder-revision'), 'again');
    await settle(r.fixture);
    button(r, 'Revise').click();
    await settle(r.fixture);
    expect(alertText(r)).toBe('boom Your draft is untouched.');
  });

  it('an edited draft is what Use hands over; a blank draft disables Use and Save', async () => {
    const r = await render();
    await runToReview(r);
    sceneEditor(r).setMarkdown('Rain, and a stranger at the door.');
    await settle(r.fixture);
    button(r, 'Use this scene').click();
    expect(r.used).toEqual(['Rain, and a stranger at the door.']);
    TestBed.resetTestingModule();
    const blank = await render();
    await runToReview(blank);
    sceneEditor(blank).setMarkdown('   ');
    await settle(blank.fixture);
    expect(button(blank, 'Use this scene').disabled).toBe(true);
    expect(button(blank, 'Save as scenario…').disabled).toBe(true);
  });

  it('Cancel closes from the review pane', async () => {
    const r = await render();
    await runToReview(r);
    button(r, 'Cancel').click();
    expect(r.closed.count).toBe(1);
    expect(r.used).toEqual([]);
  });
});

describe('SaveScenarioDialog — within the builder', () => {
  async function openSave(
    r: Rendered,
    location = 'the Lantern Inn',
    time = 'a rainy evening',
  ): Promise<void> {
    r.fake.buildFrames.push([{ done: true, scenario: 'Rain on the cobbles.' }]);
    type(byId<HTMLInputElement>(r, 'scenario-builder-location'), location);
    type(byId<HTMLInputElement>(r, 'scenario-builder-time'), time);
    await settle(r.fixture);
    await setTheScene(r);
    button(r, 'Save as scenario…').click();
    await settle(r.fixture);
  }

  it('defaults the name to "location — time"', async () => {
    const r = await render();
    await openSave(r);
    expect(byId<HTMLInputElement>(r, 'save-scenario-name').value).toBe(
      'the Lantern Inn — a rainy evening',
    );
    expect(byId<HTMLInputElement>(r, 'save-scenario-name').maxLength).toBe(100);
  });

  it('caps the default name at 100 characters', async () => {
    const r = await render();
    await openSave(r, 'L'.repeat(90), 'T'.repeat(30));
    expect(byId<HTMLInputElement>(r, 'save-scenario-name').value).toBe(
      `${'L'.repeat(90)} — ${'T'.repeat(7)}`,
    );
  });

  it('offers General, the project, the cast’s groups and each cast member', async () => {
    const r = await render({
      cast: [
        { id: 'char-2', name: 'Bob' },
        { id: 'char-1', name: 'Alice' },
      ],
      projectId: 'proj-1',
      projectName: 'The Siege',
      configure: (fake) => {
        fake.groups = [{ id: 'g-1', name: 'Aeronauts Club' }];
      },
    });
    await openSave(r);
    const options = Array.from(
      byId<HTMLSelectElement>(r, 'save-scenario-target').querySelectorAll('option'),
    ).map((o) => [o.value, (o.textContent ?? '').trim()]);
    expect(options).toEqual([
      ['general', 'Quilltap General'],
      ['project', 'Project: The Siege'],
      ['group:g-1', 'Group: Aeronauts Club'],
      ['character:char-2', 'Bob’s scenarios'],
      ['character:char-1', 'Alice’s scenarios'],
    ]);
    // The group list asked for the SORTED cast (v4's castKey).
    expect(r.fake.requests.find((q) => q['type'] === 'groupList')).toEqual({
      type: 'groupList',
      characterIds: ['char-1', 'char-2'],
    });
  });

  it('names an unnamed project "this project", and asks no groups for an empty cast', async () => {
    const r = await render({ cast: [], projectId: 'proj-1', projectName: null });
    await openSave(r);
    const labels = Array.from(
      byId<HTMLSelectElement>(r, 'save-scenario-target').querySelectorAll('option'),
    ).map((o) => (o.textContent ?? '').trim());
    expect(labels).toEqual(['Quilltap General', 'Project: this project']);
    expect(r.fake.requests.some((q) => q['type'] === 'groupList')).toBe(false);
  });

  it('files to the project and to a group with the file bag, description included', async () => {
    const r = await render({
      projectId: 'proj-1',
      configure: (fake) => {
        fake.groups = [{ id: 'g-1', name: 'Aeronauts Club' }];
        fake.saveResponse = (req) =>
          ({
            type: 'scenarioBuilder',
            data: { path: req['type'] === 'projectScenarioCreate' ? 'P/x.md' : 'G/x.md' },
          }) as unknown as CoreResponse;
      },
    });
    await openSave(r);
    type(byId<HTMLSelectElement>(r, 'save-scenario-target'), 'project');
    type(byId<HTMLInputElement>(r, 'save-scenario-description'), '  A wet night.  ');
    await settle(r.fixture);
    button(r, 'Save').click();
    await settle(r.fixture);
    expect(r.fake.requests.find((q) => q['type'] === 'projectScenarioCreate')).toEqual({
      type: 'projectScenarioCreate',
      projectId: 'proj-1',
      scenario: {
        filename: 'the Lantern Inn — a rainy evening',
        name: 'the Lantern Inn — a rainy evening',
        description: 'A wet night.',
        body: 'Rain on the cobbles.',
      },
    });
    expect(r.saved).toEqual([{ kind: 'project', projectId: 'proj-1', path: 'P/x.md' }]);

    button(r, 'Save as scenario…').click();
    await settle(r.fixture);
    type(byId<HTMLSelectElement>(r, 'save-scenario-target'), 'group:g-1');
    await settle(r.fixture);
    button(r, 'Save').click();
    await settle(r.fixture);
    expect(r.fake.requests.find((q) => q['type'] === 'groupScenarioCreate')).toMatchObject({
      groupId: 'g-1',
      scenario: { filename: 'the Lantern Inn — a rainy evening' },
    });
    expect(r.saved[1]).toEqual({ kind: 'group', groupId: 'g-1', path: 'G/x.md' });
  });

  it('locks the description for a character target, with v4’s note', async () => {
    const r = await render();
    await openSave(r);
    expect(byId<HTMLInputElement>(r, 'save-scenario-description').disabled).toBe(false);
    type(byId<HTMLSelectElement>(r, 'save-scenario-target'), 'character:char-1');
    await settle(r.fixture);
    expect(byId<HTMLInputElement>(r, 'save-scenario-description').disabled).toBe(true);
    expect(r.el.textContent).toContain('A character’s own scenarios keep a title and a body only.');
  });

  it('refuses a blank name with v4’s sentence and files nothing', async () => {
    const r = await render();
    await openSave(r);
    type(byId<HTMLInputElement>(r, 'save-scenario-name'), '   ');
    await settle(r.fixture);
    button(r, 'Save').click();
    await settle(r.fixture);
    expect(alertText(r)).toBe(SAVE_NEEDS_A_NAME);
    expect(r.fake.requests.some((q) => q['type'] === 'scenarioCreate')).toBe(false);
  });

  it('on success: invalidates the scenario lists, toasts, reports, and closes', async () => {
    const r = await render();
    const invalidate = vi.spyOn(r.queryClient, 'invalidateQueries');
    await openSave(r);
    button(r, 'Save').click();
    await settle(r.fixture);
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ['scenarios'] });
    expect(r.toasts.showSuccess).toHaveBeenCalledWith(
      '“the Lantern Inn — a rainy evening” has been filed among the scenarios.',
    );
    expect(r.saved).toEqual([{ kind: 'general', path: 'Scenarios/a-scene.md' }]);
    expect(r.el.textContent).not.toContain('File this scene as a scenario');
    // Back on the builder's review pane.
    expect(r.el.querySelector('[aria-label="The scene"]')).not.toBeNull();
  });

  it('a refusal with no message falls to v4’s catch sentence', async () => {
    const r = await render({
      configure: (fake) => {
        fake.saveResponse = {
          type: 'error',
          data: { kind: 'internal', message: '' },
        } as unknown as CoreResponse;
      },
    });
    await openSave(r);
    button(r, 'Save').click();
    await settle(r.fixture);
    expect(alertText(r)).toBe('The scenario could not be filed.');
  });

  it('Cancel closes the save dialog and keeps the draft', async () => {
    const r = await render();
    await openSave(r);
    // Scoped: the builder's own review-pane Cancel precedes it in the DOM.
    const saveCancel = Array.from(r.el.querySelectorAll('qt-save-scenario-dialog button')).find(
      (b) => (b.textContent ?? '').trim() === 'Cancel',
    ) as HTMLButtonElement;
    saveCancel.click();
    await settle(r.fixture);
    expect(r.el.textContent).not.toContain('File this scene as a scenario');
    expect(r.el.querySelector('[aria-label="The scene"]')).not.toBeNull();
    expect(r.closed.count).toBe(0);
  });
});

describe('ScenarioBuilderDialog — profile mapping (v4 mapProfiles)', () => {
  it('reads allowToolUse absent as true and allowWebSearch absent as false', async () => {
    const bare = { id: 'p-bare', name: 'Bare', provider: 'x', modelName: 'y' };
    const r = await render({ profiles: [bare] });
    // Tools: absent → allowed, so the option is enabled and preselected.
    const option = r.el.querySelector('option') as HTMLOptionElement;
    expect(option.disabled).toBe(false);
    expect((option.textContent ?? '').trim()).toBe('Bare');
    // Search: absent → forbidden, so real mode warns that the profile does not permit it.
    (r.el.querySelector('input[value="real"]') as HTMLInputElement).click();
    await settle(r.fixture);
    expect(statusText(r)).toContain('this profile does not permit web search');
  });
});
