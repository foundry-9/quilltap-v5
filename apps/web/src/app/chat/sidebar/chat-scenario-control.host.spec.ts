import { Component, input, output, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type { SavedScenarioTarget } from '../../scenario-builder/save-scenario-dialog';
import { ScenarioBuilderDialog } from '../../scenario-builder/scenario-builder-dialog';
import { scenarioKeys } from '../../scenario/scenario.api';
import { ChatScenarioControl } from './chat-scenario-control';

/**
 * "Ask the Host to set the scene" in the Salon sidebar's scenario control (v4
 * `ChatScenarioControl.tsx` at `d1c06cd9d`). v4 ships no test for these hunks,
 * so the pins are written against its source: the button (disabled while
 * saving), the dialog's inputs (`castCharacters`, the project, THIS chat), Use
 * → the custom draft with NOTHING dispatched (the existing Change-scenario
 * button persists it, which is what makes the Host announce the revision), and
 * the saved-preset selection read from the caches the save dialog already
 * invalidated — the project only for this chat's own, a group only when that
 * group offers that path, a character only for the lone LLM character.
 *
 * The dialog is swapped for a same-selector stub (as v4's own New Chat test
 * does); the dialog itself is pinned by `scenario-builder-dialog.spec.ts`.
 */

@Component({
  selector: 'qt-scenario-builder-dialog',
  template: '<span data-testid="builder-stub"></span>',
})
class StubBuilderDialog {
  readonly cast = input<readonly { id: string; name: string }[]>([]);
  readonly projectId = input<string | null>(null);
  readonly projectName = input<string | null>(null);
  readonly chatId = input<string | null>(null);
  readonly closed = output<void>();
  readonly use = output<string>();
  readonly saved = output<SavedScenarioTarget>();
}

interface Req {
  type: string;
  [k: string]: unknown;
}

const GENERAL = [
  {
    path: 'Scenarios/road.md',
    filename: 'road.md',
    name: 'The Road',
    isDefault: false,
    body: 'A road in the rain.',
  },
];
const PROJECT = [
  {
    path: 'Scenarios/inn.md',
    filename: 'inn.md',
    name: 'The Inn',
    isDefault: false,
    body: 'A low-ceilinged room.',
  },
];

@Component({
  imports: [ChatScenarioControl],
  template: `
    <qt-chat-scenario-control
      [chatId]="'chat-1'"
      [projectId]="projectId()"
      [scenarioText]="null"
      [llmCharacterIds]="llmCharacterIds()"
      [singleLlmCharacterId]="singleLlmCharacterId()"
      [castCharacters]="castCharacters()"
      [projectName]="projectName()"
      [enabled]="true"
    />
  `,
})
class Host {
  readonly projectId = signal<string | null>('proj-1');
  readonly projectName = signal<string | null>('The Siege');
  readonly llmCharacterIds = signal<string[]>(['char-1']);
  readonly singleLlmCharacterId = signal<string | null>('char-1');
  readonly castCharacters = signal([
    { id: 'char-1', name: 'Alice' },
    { id: 'char-9', name: 'Me' },
  ]);
}

interface Rendered {
  fixture: ComponentFixture<Host>;
  sent: Req[];
  queryClient: QueryClient;
  holdSave: { release: () => void };
}

async function settle(fixture: ComponentFixture<Host>): Promise<void> {
  for (let i = 0; i < 5; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function render(prepare?: (host: Host) => void, holdSave = false): Promise<Rendered> {
  const sent: Req[] = [];
  const hold = { release: () => undefined as void };
  const dispatchData = async (req: Req) => {
    sent.push(req);
    switch (req.type) {
      case 'scenarioList':
        return { scenarios: GENERAL };
      case 'projectScenarioList':
        return { scenarios: PROJECT };
      case 'groupScenariosUnion':
        return {
          groupScenarios: [
            {
              groupId: 'g-1',
              groupName: 'Aeronauts',
              scenarios: [{ ...GENERAL[0], path: 'G/a.md' }],
            },
          ],
        };
      case 'characterScenarioList':
        return { scenarios: [] };
      case 'chatSetScenario':
        if (holdSave) await new Promise<void>((r) => (hold.release = r));
        return { scenarioText: 'x', changed: true, message: 'Scenario updated' };
      default:
        return {};
    }
  };
  const queryClient = new QueryClient();
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [Host],
    providers: [
      { provide: CoreClient, useValue: { dispatchData } },
      provideTanStackQuery(queryClient),
    ],
  });
  TestBed.overrideComponent(ChatScenarioControl, {
    remove: { imports: [ScenarioBuilderDialog] },
    add: { imports: [StubBuilderDialog] },
  });
  await TestBed.compileComponents();
  const fixture = TestBed.createComponent(Host);
  prepare?.(fixture.componentInstance);
  fixture.detectChanges();
  await settle(fixture);
  return { fixture, sent, queryClient, holdSave: hold };
}

function hostButton(r: Rendered): HTMLButtonElement {
  const found = Array.from(
    (r.fixture.nativeElement as HTMLElement).querySelectorAll('button'),
  ).find((b) => /Ask the Host to set the scene/.test(b.textContent ?? ''));
  if (!found) throw new Error('the Host button is not rendered');
  return found as HTMLButtonElement;
}

async function openBuilder(r: Rendered): Promise<StubBuilderDialog> {
  hostButton(r).click();
  await settle(r.fixture);
  const stub = r.fixture.debugElement.query(By.directive(StubBuilderDialog));
  if (!stub) throw new Error('the builder dialog did not mount');
  return stub.componentInstance as StubBuilderDialog;
}

function selectValue(r: Rendered): string {
  return (r.fixture.nativeElement.querySelector('#chat-scenario-select') as HTMLSelectElement)
    .value;
}

function textarea(r: Rendered): HTMLTextAreaElement | null {
  return r.fixture.nativeElement.querySelector('textarea');
}

async function saveWith(
  target: SavedScenarioTarget,
  prepare?: (host: Host) => void,
  seedCache?: (qc: QueryClient) => void,
): Promise<Rendered> {
  const r = await render(prepare);
  seedCache?.(r.queryClient);
  const builder = await openBuilder(r);
  builder.saved.emit(target);
  await settle(r.fixture);
  return r;
}

describe('ChatScenarioControl — Ask the Host to set the scene (v4 d1c06cd9d)', () => {
  it('renders the button with the Host’s portrait, disabled while a save is in flight', async () => {
    const r = await render(undefined, true);
    const button = hostButton(r);
    expect(button.querySelector('img')!.getAttribute('src')).toBe(
      '/images/avatars/host-avatar.webp',
    );
    expect(button.disabled).toBe(false);
    (
      r.fixture.nativeElement.querySelector('button.qt-tool-palette-button') as HTMLButtonElement
    ).click();
    await settle(r.fixture);
    expect(hostButton(r).disabled).toBe(true);
    r.holdSave.release();
    await settle(r.fixture);
    expect(hostButton(r).disabled).toBe(false);
  });

  it('hands the dialog every present character, the project and THIS chat', async () => {
    const r = await render();
    const builder = await openBuilder(r);
    expect(builder.cast()).toEqual([
      { id: 'char-1', name: 'Alice' },
      { id: 'char-9', name: 'Me' },
    ]);
    expect(builder.projectId()).toBe('proj-1');
    expect(builder.projectName()).toBe('The Siege');
    expect(builder.chatId()).toBe('chat-1');
    builder.closed.emit();
    await settle(r.fixture);
    expect(r.fixture.debugElement.query(By.directive(StubBuilderDialog))).toBeNull();
  });

  it('Use makes the scene the custom text and dispatches NOTHING; Change scenario then persists it', async () => {
    const r = await render();
    const builder = await openBuilder(r);
    const before = r.sent.length;
    builder.use.emit('Rain on the cobbles.');
    await settle(r.fixture);
    expect(selectValue(r)).toBe('__custom__');
    expect(textarea(r)!.value).toBe('Rain on the cobbles.');
    expect(r.sent.slice(before).some((q) => q.type === 'chatSetScenario')).toBe(false);
    (
      r.fixture.nativeElement.querySelector('button.qt-tool-palette-button') as HTMLButtonElement
    ).click();
    await settle(r.fixture);
    expect(r.sent.find((q) => q.type === 'chatSetScenario')).toEqual({
      type: 'chatSetScenario',
      chatId: 'chat-1',
      scenario: 'Rain on the cobbles.',
    });
  });
});

describe('ChatScenarioControl — selecting a scene the Host just filed', () => {
  it('selects a general preset the cache now carries', async () => {
    const r = await saveWith({ kind: 'general', path: 'Scenarios/road.md' });
    expect(selectValue(r)).toBe('general:Scenarios/road.md');
    expect(textarea(r)).toBeNull();
  });

  it('leaves the picker alone when the general cache lacks the path', async () => {
    const r = await saveWith({ kind: 'general', path: 'Scenarios/nowhere.md' });
    expect(selectValue(r)).toBe('__custom__');
  });

  it('selects a project preset only for this chat’s own project', async () => {
    const mine = await saveWith({
      kind: 'project',
      projectId: 'proj-1',
      path: 'Scenarios/inn.md',
    });
    expect(selectValue(mine)).toBe('project:Scenarios/inn.md');
    const other = await saveWith({
      kind: 'project',
      projectId: 'proj-2',
      path: 'Scenarios/inn.md',
    });
    expect(selectValue(other)).toBe('__custom__');
  });

  it('selects a group preset only when that group offers that path', async () => {
    const offered = await saveWith({ kind: 'group', groupId: 'g-1', path: 'G/a.md' });
    expect(selectValue(offered)).toBe('group:g-1:G/a.md');
    const wrongGroup = await saveWith({ kind: 'group', groupId: 'g-2', path: 'G/a.md' });
    expect(selectValue(wrongGroup)).toBe('__custom__');
  });

  it('reads the Show-archived cache when that is what the picker shows', async () => {
    const r = await render();
    // The picker is showing the archived-inclusive general list …
    (r.fixture.nativeElement.querySelector('input[type="checkbox"]') as HTMLInputElement).click();
    await settle(r.fixture);
    // … and only THAT cache carries the new path.
    r.queryClient.setQueryData(scenarioKeys.general(false), []);
    const builder = await openBuilder(r);
    builder.saved.emit({ kind: 'general', path: 'Scenarios/road.md' });
    await settle(r.fixture);
    expect(selectValue(r)).toBe('general:Scenarios/road.md');
  });

  it('selects a character scenario only for the lone LLM character', async () => {
    const target: SavedScenarioTarget = {
      kind: 'character',
      characterId: 'char-1',
      scenarioId: 'scn-1',
      title: 'Rain',
      content: 'Rain.',
    };
    const lone = await saveWith(target);
    // Selected in the draft (the option itself arrives with the refetched list).
    expect(textarea(lone)).toBeNull();
    const several = await saveWith(target, (host) => {
      host.llmCharacterIds.set(['char-1', 'char-2']);
      host.singleLlmCharacterId.set(null);
    });
    expect(selectValue(several)).toBe('__custom__');
    expect(textarea(several)).not.toBeNull();
  });
});
