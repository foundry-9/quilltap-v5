import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { ActivatedRoute, convertToParamMap, provideRouter } from '@angular/router';
import { of } from 'rxjs';
import {
  QueryClient,
  QueryObserver,
  provideTanStackQuery,
} from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { RichEditor } from '../../editor/rich-editor';
import { CoreDispatchError } from '../../core/core-contract';
import type {
  CharacterListItem,
  ProjectDetail,
  ProjectFileDto,
  ProjectRosterCharacter,
  RoleplayTemplateDto,
} from '../../core/core-contract';
import { QuickHideService } from '../../quick-hide/quick-hide.service';
import { characterKeys } from '../characters/characters.api';
import { WORKSPACE_BACKDROP_REGISTRY, WORKSPACE_TAB_ID } from '../../workspace/workspace-contract';
import type { WorkspaceBackdropEntry } from '../../workspace/workspace-contract';
import { ProjectCharactersCard } from './cards/project-characters-card';
import { ProjectFilesCard } from './cards/project-files-card';
import { ProjectImageGenerationCard } from './cards/project-image-generation-card';
import { ProjectModelBehaviorCard } from './cards/project-model-behavior-card';
import { fetchProjectBackground, projectKeys, shouldPassivePollBackground } from './projects.api';
import { ProjectDetailScreen } from './project-detail';
import { ProsperoList } from './prospero-list';
import { ToastService } from '../../ui/toast.service';

interface DispatchReq {
  type: string;
  [k: string]: unknown;
}

function stubClient(handler: (req: DispatchReq) => unknown): Partial<CoreClient> {
  return {
    dispatchData: (async (req: DispatchReq) => {
      const out = handler(req);
      if (out instanceof Error) {
        throw out;
      }
      return (out ?? {}) as Record<string, unknown>;
    }) as CoreClient['dispatchData'],
  };
}

async function settle(fixture: ComponentFixture<unknown>, ticks = 8): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

function project(over: Partial<ProjectDetail> = {}): ProjectDetail {
  return {
    id: 'p1',
    name: 'Airship Saga',
    description: 'A grand tale',
    instructions: 'Be dramatic',
    color: '#334455',
    icon: '📁',
    allowAnyCharacter: false,
    characterRoster: [],
    defaultAgentModeEnabled: null,
    defaultAvatarGenerationEnabled: null,
    defaultImageProfileId: null,
    defaultRoleplayTemplateId: null,
    defaultAlertCharactersOfLanternImages: null,
    answerConfirmationOverride: null,
    defaultDisabledTools: [],
    defaultDisabledToolGroups: [],
    backgroundDisplayMode: 'theme',
    state: {},
    createdAt: '2024-01-01T00:00:00Z',
    updatedAt: '2024-01-01T00:00:00Z',
    ...over,
  };
}

function tmpl(id: string, name: string): RoleplayTemplateDto {
  return {
    id,
    userId: 'u',
    name,
    systemPrompt: 'x',
    isBuiltIn: false,
    tags: [],
    delimiters: [],
    renderingPatterns: [],
    narrationDelimiters: '*',
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
  };
}

/** The toast stack this render raised, newest last. */
function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

describe('ProsperoList', () => {
  async function render(client: Partial<CoreClient>): Promise<ComponentFixture<ProsperoList>> {
    TestBed.configureTestingModule({
      imports: [ProsperoList],
      providers: [
        provideRouter([]),
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: client },
      ],
    });
    const fixture = TestBed.createComponent(ProsperoList);
    fixture.detectChanges();
    await settle(fixture);
    return fixture;
  }

  it('shows the empty state when there are no projects', async () => {
    const fixture = await render(
      stubClient((r) => (r.type === 'projectList' ? { projects: [] } : {})),
    );
    expect(fixture.nativeElement.textContent).toContain('No projects yet');
  });

  it('renders a card with counts and an Open link', async () => {
    const fixture = await render(
      stubClient((r) =>
        r.type === 'projectList'
          ? {
              projects: [
                {
                  id: 'p1',
                  name: 'Airship Saga',
                  description: 'A grand tale',
                  color: null,
                  icon: null,
                  createdAt: '2024-01-01T00:00:00Z',
                  updatedAt: '2024-01-01T00:00:00Z',
                  _count: { chats: 4, files: 2, characters: 3 },
                },
              ],
            }
          : {},
      ),
    );
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Airship Saga');
    expect(text).toContain('4 chats • 2 files');
    expect(fixture.nativeElement.querySelector('a[href="/prospero/p1"]')).toBeTruthy();
  });

  it('delete goes through the confirm dialog and toasts on failure', async () => {
    const projects = [
      {
        id: 'p1',
        name: 'Doomed',
        description: null,
        color: null,
        icon: null,
        createdAt: '2024-01-01T00:00:00Z',
        updatedAt: '2024-01-01T00:00:00Z',
        _count: { chats: 0, files: 0, characters: 0 },
      },
    ];
    const fixture = await render(
      stubClient((r) => {
        if (r.type === 'projectList') return { projects };
        if (r.type === 'projectDelete') return new Error('Failed to delete project');
        return {};
      }),
    );
    // Open the confirm dialog via the card trash button.
    (
      fixture.nativeElement.querySelector(
        'button[aria-label="Delete project"]',
      ) as HTMLButtonElement
    ).click();
    await settle(fixture);
    // The dialog's Delete confirm.
    const confirm = [...fixture.nativeElement.querySelectorAll('button')].find(
      (b: HTMLButtonElement) => b.textContent?.trim() === 'Delete',
    ) as HTMLButtonElement;
    expect(confirm).toBeTruthy();
    confirm.click();
    await settle(fixture);
    expect(toasts().at(-1)).toEqual({ type: 'error', message: 'Failed to delete project' });
  });
});

/**
 * In-tab drill (p4.9j2, v4 `ProsperoView` `selectedProjectId`): hosted as a
 * workspace tab, a card's Open drills IN PLACE (renders the detail embedded);
 * the detail header's back restores the list.
 */
describe('ProsperoList (in-tab drill)', () => {
  const card = {
    id: 'p1',
    name: 'Airship Saga',
    description: null,
    color: null,
    icon: null,
    createdAt: '2024-01-01T00:00:00Z',
    updatedAt: '2024-01-01T00:00:00Z',
    _count: { chats: 0, files: 0, characters: 0 },
  };

  function handler(r: DispatchReq): unknown {
    switch (r.type) {
      case 'projectList':
        return { projects: [card] };
      case 'projectGet':
        return { project: project({ id: 'p1', name: 'Airship Saga' }) };
      case 'projectMountPointList':
        return { mountPoints: [] };
      case 'projectChatList':
        return { chats: [], total: 0 };
      default:
        return {};
    }
  }

  async function render(): Promise<ComponentFixture<ProsperoList>> {
    TestBed.configureTestingModule({
      imports: [ProsperoList],
      providers: [
        provideRouter([]),
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: stubClient(handler) },
        { provide: WORKSPACE_TAB_ID, useValue: 'tab-p' },
      ],
    });
    const fixture = TestBed.createComponent(ProsperoList);
    fixture.detectChanges();
    await settle(fixture);
    return fixture;
  }

  it('Open drills in place (no /prospero/:id anchor) and back restores the list', async () => {
    const fixture = await render();
    // The Open affordance is a button, not a routerLink anchor.
    expect(fixture.nativeElement.querySelector('a[href="/prospero/p1"]')).toBeNull();
    const open = [...fixture.nativeElement.querySelectorAll('button')].find(
      (b: HTMLButtonElement) => b.textContent?.trim() === 'Open',
    ) as HTMLButtonElement;
    expect(open).toBeTruthy();

    open.click();
    await settle(fixture);
    // The detail renders in place.
    expect(fixture.nativeElement.querySelector('qt-project-detail')).toBeTruthy();

    // The header's back restores the list.
    const back = [...fixture.nativeElement.querySelectorAll('button')].find(
      (b: HTMLButtonElement) =>
        b.textContent?.includes('Projects') && !b.textContent?.includes('Create'),
    ) as HTMLButtonElement;
    expect(back).toBeTruthy();
    back.click();
    await settle(fixture);
    expect(fixture.nativeElement.querySelector('qt-project-detail')).toBeNull();
    expect(fixture.nativeElement.textContent).toContain('Create Project');
  });

  // P4.d16 (v4 `8d86847a`): the deep-link drill payload.
  it('opens already drilled into initialProjectId, and follows a re-target', async () => {
    TestBed.configureTestingModule({
      imports: [ProsperoList],
      providers: [
        provideRouter([]),
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: stubClient(handler) },
        { provide: WORKSPACE_TAB_ID, useValue: 'tab-p' },
      ],
    });
    const fixture = TestBed.createComponent(ProsperoList);
    fixture.componentRef.setInput('initialProjectId', 'p1');
    fixture.detectChanges();
    await settle(fixture);
    expect(fixture.nativeElement.querySelector('qt-project-detail')).toBeTruthy();

    // A re-open of the open tab refreshes the payload — the drill follows it.
    fixture.componentRef.setInput('initialProjectId', 'p2');
    fixture.detectChanges();
    await settle(fixture);
    expect(
      fixture.debugElement
        .query(By.directive(ProjectDetailScreen))
        .componentInstance.projectIdInput(),
    ).toBe('p2');
  });
});

describe('ProjectModelBehaviorCard', () => {
  async function render(
    client: Partial<CoreClient>,
    proj: ProjectDetail,
  ): Promise<ComponentFixture<ProjectModelBehaviorCard>> {
    TestBed.configureTestingModule({
      imports: [ProjectModelBehaviorCard],
      providers: [
        provideRouter([]),
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: client },
      ],
    });
    const fixture = TestBed.createComponent(ProjectModelBehaviorCard);
    fixture.componentRef.setInput('project', proj);
    fixture.componentRef.setInput('defaultOpen', true);
    fixture.detectChanges();
    await settle(fixture);
    return fixture;
  }

  it('immediate-saves the agent mode select and toasts on success/failure (P4.29, v4 has no inline surface)', async () => {
    const seen: DispatchReq[] = [];
    const fixture = await render(
      stubClient((r) => {
        seen.push(r);
        return r.type === 'projectUpdate' ? new Error('nope') : {};
      }),
      project(),
    );
    // Change the agent-mode select AFTER first render (async-options lesson).
    const agentSelect = fixture.nativeElement.querySelector(
      'select[aria-label="Agent Mode"]',
    ) as HTMLSelectElement;
    agentSelect.value = 'enabled';
    agentSelect.dispatchEvent(new Event('change'));
    await settle(fixture);
    const put = seen.find((r) => r.type === 'projectUpdate');
    expect(put).toMatchObject({ projectId: 'p1', project: { defaultAgentModeEnabled: true } });
    // The failed immediate save toasts, with no inline alert (v4 has none here).
    expect(fixture.nativeElement.querySelector('.qt-alert-error')).toBeNull();
    expect(toasts()).toEqual([{ type: 'error', message: 'nope' }]);
  });

  it('the Default Roleplay Template select is live (P4.6r) and enabled', async () => {
    const fixture = await render(
      stubClient((r) => (r.type === 'roleplayTemplateList' ? [] : {})),
      project(),
    );
    const rp = fixture.nativeElement.querySelector(
      'select[aria-label="Default Roleplay Template"]',
    ) as HTMLSelectElement;
    expect(rp.disabled).toBe(false);
  });

  it("toasts each of v4's three agent-mode success sentences", async () => {
    const fixture = await render(
      stubClient(() => ({})),
      project({ defaultAgentModeEnabled: null }),
    );
    const agentSelect = fixture.nativeElement.querySelector(
      'select[aria-label="Agent Mode"]',
    ) as HTMLSelectElement;
    agentSelect.value = 'enabled';
    agentSelect.dispatchEvent(new Event('change'));
    await settle(fixture);
    agentSelect.value = 'disabled';
    agentSelect.dispatchEvent(new Event('change'));
    await settle(fixture);
    agentSelect.value = 'inherit';
    agentSelect.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(toasts()).toEqual([
      { type: 'success', message: 'Agent mode enabled by default for project' },
      { type: 'success', message: 'Agent mode disabled by default for project' },
      { type: 'success', message: 'Agent mode set to inherit from global/character' },
    ]);
  });

  it('toasts the answer-confirmation success sentence, and a failure', async () => {
    const fixture = await render(
      stubClient(() => ({})),
      project({ answerConfirmationOverride: null }),
    );
    const select = fixture.nativeElement.querySelector(
      'select[aria-label="Answer Confirmation"]',
    ) as HTMLSelectElement;
    select.value = 'ON';
    select.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(toasts()).toEqual([
      { type: 'success', message: 'Answer confirmation enabled by default for project' },
    ]);

    TestBed.resetTestingModule();
    const failing = await render(
      stubClient((r) => (r.type === 'projectUpdate' ? new Error('confirmation rejected') : {})),
      project({ answerConfirmationOverride: 'ON' }),
    );
    const failSelect = failing.nativeElement.querySelector(
      'select[aria-label="Answer Confirmation"]',
    ) as HTMLSelectElement;
    failSelect.value = 'OFF';
    failSelect.dispatchEvent(new Event('change'));
    await settle(failing);
    // A fresh TestBed module ⇒ a fresh ToastService; only this render's toast.
    expect(toasts()).toEqual([{ type: 'error', message: 'confirmation rejected' }]);
  });

  it('toasts the roleplay-template success sentences (set + inherit), and a failure', async () => {
    const fixture = await render(
      stubClient((r) => (r.type === 'roleplayTemplateList' ? [tmpl('t1', 'Epic')] : {})),
      project({ defaultRoleplayTemplateId: null }),
    );
    const select = fixture.nativeElement.querySelector(
      'select[aria-label="Default Roleplay Template"]',
    ) as HTMLSelectElement;
    select.value = 't1';
    select.dispatchEvent(new Event('change'));
    await settle(fixture);
    select.value = '';
    select.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(toasts()).toEqual([
      { type: 'success', message: 'Default roleplay template set for project' },
      { type: 'success', message: 'Roleplay template set to inherit from global' },
    ]);

    TestBed.resetTestingModule();
    const failing = await render(
      stubClient((r) => {
        if (r.type === 'roleplayTemplateList') return [tmpl('t1', 'Epic')];
        return r.type === 'projectUpdate' ? new Error('template rejected') : {};
      }),
      project({ defaultRoleplayTemplateId: null }),
    );
    const failSelect = failing.nativeElement.querySelector(
      'select[aria-label="Default Roleplay Template"]',
    ) as HTMLSelectElement;
    failSelect.value = 't1';
    failSelect.dispatchEvent(new Event('change'));
    await settle(failing);
    expect(toasts()).toEqual([{ type: 'error', message: 'template rejected' }]);
  });
});

/**
 * P4.D247 — the project Characters card as v4 `9753d0eb2` left it (every
 * string quoted from `git show e5c6bd0c0:"app/prospero/[id]/components/
 * CharactersCard.tsx"` and `…/hooks/useProjectDetail.ts`). The `project()`
 * builder carries the WIRE shape: `characterRoster` holds the enriched entries
 * `projectGet` sends (`api/projects.rs` `enrich_project`) and there is no `roster` key —
 * the phantom the card read since P4.6l (survey §D1).
 */
describe('ProjectCharactersCard', () => {
  const HIDDEN_TAG = 't-hidden';
  const EXPLAINER =
    'Any character in a project chat may read and edit its files and borrow from its wardrobe. Turn this off to choose who may.';

  function rosterChar(
    id: string,
    name: string,
    over: Partial<ProjectRosterCharacter> = {},
  ): ProjectRosterCharacter {
    return { id, name, defaultImage: null, tags: [], chatCount: 0, ...over };
  }

  function listChar(
    id: string,
    name: string,
    over: Partial<CharacterListItem> = {},
  ): CharacterListItem {
    return {
      id,
      name,
      title: null,
      defaultImageId: null,
      defaultImage: null,
      tags: [],
      ...over,
    } as CharacterListItem;
  }

  /** A held promise: the test decides when the stubbed dispatch answers. */
  function held<T>(): { promise: Promise<T>; release: (v: T) => void } {
    let release!: (v: T) => void;
    const promise = new Promise<T>((r) => (release = r));
    return { promise, release };
  }

  async function render(
    client: Partial<CoreClient>,
    proj: ProjectDetail,
    opts: { queryClient?: QueryClient; defaultOpen?: boolean } = {},
  ): Promise<ComponentFixture<ProjectCharactersCard>> {
    TestBed.configureTestingModule({
      imports: [ProjectCharactersCard],
      providers: [
        provideRouter([]),
        provideTanStackQuery(opts.queryClient ?? new QueryClient()),
        { provide: CoreClient, useValue: client },
        // The real service needs `tagList` + localStorage; the card only asks
        // the one question (v4 `shouldHideByIds`).
        {
          provide: QuickHideService,
          useValue: {
            shouldHideByIds: (ids?: ReadonlyArray<string | null | undefined>) =>
              (ids ?? []).includes(HIDDEN_TAG),
          },
        },
      ],
    });
    const fixture = TestBed.createComponent(ProjectCharactersCard);
    fixture.componentRef.setInput('project', proj);
    fixture.componentRef.setInput('defaultOpen', opts.defaultOpen ?? true);
    fixture.detectChanges();
    await settle(fixture);
    return fixture;
  }

  function el(fixture: ComponentFixture<unknown>): HTMLElement {
    return fixture.nativeElement as HTMLElement;
  }

  /** Whitespace-collapsed text (Angular collapses template whitespace too). */
  function text(fixture: ComponentFixture<unknown>): string {
    return (el(fixture).textContent ?? '').replace(/\s+/g, ' ');
  }

  function buttonNamed(
    fixture: ComponentFixture<unknown>,
    label: string,
  ): HTMLButtonElement | null {
    return (
      (Array.from(el(fixture).querySelectorAll('button')).find(
        (b) => (b.textContent ?? '').trim() === label,
      ) as HTMLButtonElement | undefined) ?? null
    );
  }

  function searchBox(fixture: ComponentFixture<unknown>): HTMLInputElement | null {
    return el(fixture).querySelector('input[aria-label="Search characters to add"]');
  }

  /** The picker's candidate rows, in render order. */
  function rows(fixture: ComponentFixture<unknown>): HTMLButtonElement[] {
    return Array.from(el(fixture).querySelectorAll('ul li button')) as HTMLButtonElement[];
  }

  function rowNames(fixture: ComponentFixture<unknown>): string[] {
    return rows(fixture).map((r) => (r.querySelector('.text-sm')?.textContent ?? '').trim());
  }

  async function openPicker(fixture: ComponentFixture<unknown>): Promise<void> {
    const add = buttonNamed(fixture, 'Add character');
    expect(add, 'the closed picker button').not.toBeNull();
    add!.click();
    await settle(fixture);
  }

  async function type(fixture: ComponentFixture<unknown>, value: string): Promise<void> {
    const box = searchBox(fixture);
    expect(box, 'the picker search box').not.toBeNull();
    box!.value = value;
    box!.dispatchEvent(new Event('input'));
    await settle(fixture);
  }

  function toggle(fixture: ComponentFixture<unknown>): HTMLButtonElement {
    return el(fixture).querySelector(
      'button[aria-label="Allow Any Character"]',
    ) as HTMLButtonElement;
  }

  // --- V1 — the defect: the roster renders from `characterRoster` ----------

  it('V1 renders the roster from the wire `characterRoster` (the phantom-`roster` defect)', async () => {
    const fixture = await render(
      stubClient(() => ({})),
      project({
        characterRoster: [rosterChar('c1', 'Bertie', { chatCount: 1 }), rosterChar('c2', 'Clara')],
      }),
    );
    const t = text(fixture);
    expect(t).toContain('2 characters in roster');
    expect(t).toContain('Bertie');
    expect(t).toContain('Clara');
    expect(t).toContain('1 chat');
    expect(t).toContain('0 chats');
  });

  // --- V2 / V3 — the two Allow Any modes -------------------------------------

  it('V2 Allow Any ON: the open subtitle, the explainer, no picker, no grid even over a roster', async () => {
    const fixture = await render(
      stubClient(() => ({})),
      project({ allowAnyCharacter: true, characterRoster: [rosterChar('c1', 'Bertie')] }),
    );
    const t = text(fixture);
    expect(t).toContain('Open to every character');
    expect(t).toContain('Every character may use the project files and shared wardrobe.');
    const explainer = Array.from(el(fixture).querySelectorAll('p')).find((p) =>
      (p.textContent ?? '').includes('Any character in a project chat'),
    );
    // v4's two source lines render joined by ONE space (JSX whitespace rule).
    expect(explainer?.textContent?.trim()).toBe(EXPLAINER);
    expect(t).not.toContain('Add character');
    expect(t).not.toContain('Bertie');
    expect(el(fixture).querySelector('button[title="Remove from roster"]')).toBeNull();
    expect(t).not.toContain('in roster');
  });

  it('V3 Allow Any OFF: the roster-only description, the picker button, no explainer', async () => {
    const fixture = await render(
      stubClient(() => ({})),
      project({ allowAnyCharacter: false }),
    );
    const t = text(fixture);
    expect(t).toContain('Only roster characters may use the project files and shared wardrobe.');
    expect(buttonNamed(fixture, 'Add character')).not.toBeNull();
    expect(t).not.toContain('Any character in a project chat');
    expect(t).not.toContain('Any character can join project chats.');
  });

  // --- V4 — the empty and all-hidden branches --------------------------------

  it('V4 an empty roster says so with the hint; an all-hidden roster says so WITHOUT it', async () => {
    const empty = await render(
      stubClient(() => ({})),
      project({ characterRoster: [] }),
    );
    expect(text(empty)).toContain('No characters in the roster yet.');
    expect(text(empty)).toContain(
      'Until someone is added, no character may use the project files or shared wardrobe.',
    );
    expect(text(empty)).not.toContain('Characters are added when chats are associated');

    TestBed.resetTestingModule();
    const hidden = await render(
      stubClient(() => ({})),
      project({ characterRoster: [rosterChar('c1', 'Shade', { tags: [HIDDEN_TAG] })] }),
    );
    const t = text(hidden);
    expect(t).toContain('No visible characters (some may be hidden).');
    expect(t).not.toContain('Until someone is added');
    expect(t).not.toContain('Shade');
    expect(t).toContain('0 characters in roster');
  });

  // --- V5 — the remove button at rest ----------------------------------------

  it('V5 the remove button is visible at rest and names its character', async () => {
    const fixture = await render(
      stubClient(() => ({})),
      project({ characterRoster: [rosterChar('c1', 'Bertie'), rosterChar('c2', '')] }),
    );
    const remove = el(fixture).querySelector(
      'button[aria-label="Remove Bertie from roster"]',
    ) as HTMLButtonElement | null;
    expect(remove).not.toBeNull();
    expect(remove!.classList).toContain('opacity-60');
    expect(remove!.classList).toContain('group-hover:opacity-100');
    expect(remove!.classList).toContain('focus:opacity-100');
    expect(remove!.classList).not.toContain('opacity-0');
    expect(remove!.getAttribute('title')).toBe('Remove from roster');
    expect(
      el(fixture).querySelector('button[aria-label="Remove character from roster"]'),
    ).not.toBeNull();
  });

  // --- V6 — the picker's fetch gate (expanded ∧ !allowAny ∧ pickerOpen) ------

  it('V6 the characters list is fetched only while expanded, roster-editable and open', async () => {
    const seen: DispatchReq[] = [];
    const qc = new QueryClient();
    const client = stubClient((r) => {
      seen.push(r);
      return r.type === 'characterList' ? { characters: [listChar('c9', 'Bram')] } : {};
    });
    const lists = () => seen.filter((r) => r.type === 'characterList').length;

    const fixture = await render(client, project({ allowAnyCharacter: false }), {
      queryClient: qc,
    });
    expect(lists(), 'nothing before Add character').toBe(0);
    await openPicker(fixture);
    expect(lists(), 'exactly one after opening').toBe(1);

    // Collapsed with the picker left open: an invalidation must not refetch.
    (el(fixture).querySelector('.qt-collapsible-card-header') as HTMLButtonElement).click();
    await settle(fixture);
    expect(searchBox(fixture), 'the body is collapsed').toBeNull();
    await qc.invalidateQueries({ queryKey: characterKeys.list() });
    await settle(fixture);
    expect(lists(), 'no refetch while collapsed').toBe(1);

    // Allow Any ON with the picker state left open: likewise.
    (el(fixture).querySelector('.qt-collapsible-card-header') as HTMLButtonElement).click();
    await settle(fixture);
    const afterReopen = lists();
    fixture.componentRef.setInput('project', project({ allowAnyCharacter: true }));
    await settle(fixture);
    await qc.invalidateQueries({ queryKey: characterKeys.list() });
    await settle(fixture);
    expect(lists(), 'no refetch with Allow Any ON').toBe(afterReopen);

    TestBed.resetTestingModule();
    const onSeen: DispatchReq[] = [];
    await render(
      stubClient((r) => {
        onSeen.push(r);
        return {};
      }),
      project({ allowAnyCharacter: true }),
    );
    expect(onSeen.filter((r) => r.type === 'characterList')).toEqual([]);
  });

  // --- V7 — the candidates pipeline ------------------------------------------

  it('V7 candidates: roster + hidden excluded, search by name or title, localeCompare order', async () => {
    const seen: DispatchReq[] = [];
    const fixture = await render(
      stubClient((r) => {
        seen.push(r);
        return r.type === 'characterList'
          ? {
              characters: [
                listChar('c-zed', 'Zed', { title: 'The Quiet One' }),
                listChar('c-aria', 'Aria'),
                // A quick-hidden ROSTER member whose list entry is not tagged
                // (a warm shared cache can disagree with the enrichment): v4
                // builds `onRoster` from the RAW roster, so it stays excluded.
                listChar('c-cleo', 'Cleo'),
                listChar('c-hid', 'Hidra', { tags: [HIDDEN_TAG] }),
                listChar('c-dora', 'Dora'),
                listChar('c-bram', 'Bram', { title: 'Engineer' }),
              ],
            }
          : {};
      }),
      project({
        characterRoster: [
          rosterChar('c-aria', 'Aria'),
          rosterChar('c-cleo', 'Cleo', { tags: [HIDDEN_TAG] }),
        ],
      }),
    );
    await openPicker(fixture);
    expect(seen.find((r) => r.type === 'characterList')).toEqual({ type: 'characterList' });
    expect(rowNames(fixture)).toEqual(['Bram', 'Dora', 'Zed']);
    // The title line only when truthy.
    expect(rows(fixture)[0].textContent).toContain('Engineer');
    expect(rows(fixture)[1].querySelectorAll('.qt-text-xs').length).toBe(1);

    await type(fixture, '  BRA ');
    expect(rowNames(fixture)).toEqual(['Bram']);
    await type(fixture, 'quiet');
    expect(rowNames(fixture)).toEqual(['Zed']);
    await type(fixture, 'zzz');
    expect(rows(fixture)).toEqual([]);
    expect(text(fixture)).toContain('No characters match.');
    expect(text(fixture)).not.toContain('Every character is already on the roster.');
  });

  it('V7 an exhausted list says everyone is on the roster; a held list says Loading', async () => {
    const fixture = await render(
      stubClient((r) =>
        r.type === 'characterList' ? { characters: [listChar('c-aria', 'Aria')] } : {},
      ),
      project({ characterRoster: [rosterChar('c-aria', 'Aria')] }),
    );
    await openPicker(fixture);
    expect(text(fixture)).toContain('Every character is already on the roster.');
    expect(text(fixture)).not.toContain('No characters match.');

    TestBed.resetTestingModule();
    const gate = held<Record<string, unknown>>();
    const loading = await render(
      stubClient((r) => (r.type === 'characterList' ? gate.promise : {})),
      project({}),
    );
    await openPicker(loading);
    expect(text(loading)).toContain('Loading characters…');
    gate.release({ characters: [listChar('c-bram', 'Bram')] });
    await settle(loading);
    expect(text(loading)).not.toContain('Loading characters…');
    expect(rowNames(loading)).toEqual(['Bram']);
  });

  // --- V8 / V9 — add -----------------------------------------------------------

  it('V8 add: every row disabled while in flight; the refetch lands before the toast and the re-enable', async () => {
    const seen: DispatchReq[] = [];
    const qc = new QueryClient();
    const addGate = held<Record<string, unknown>>();
    let getGate: ReturnType<typeof held<Record<string, unknown>>> | null = null;
    const client = stubClient((r) => {
      seen.push(r);
      if (r.type === 'characterList') {
        return { characters: [listChar('c-bram', 'Bram'), listChar('c-dora', 'Dora')] };
      }
      if (r.type === 'projectCharacterAdd') return addGate.promise;
      if (r.type === 'projectGet') return getGate ? getGate.promise : { project: project() };
      return {};
    });
    // The detail screen's own query (`project-detail.ts`) — the observer that
    // makes `invalidateQueries(detail)` a real, awaited refetch.
    const detail = new QueryObserver(qc, {
      queryKey: projectKeys.detail('p1'),
      queryFn: () => client.dispatchData!({ type: 'projectGet', projectId: 'p1' }),
    });
    const unsubscribe = detail.subscribe(() => undefined);
    try {
      const fixture = await render(client, project({}), { queryClient: qc });
      await openPicker(fixture);
      expect(rowNames(fixture)).toEqual(['Bram', 'Dora']);

      rows(fixture)[0].click();
      await settle(fixture);
      expect(seen.find((r) => r.type === 'projectCharacterAdd')).toMatchObject({
        projectId: 'p1',
        characterId: 'c-bram',
      });
      expect(rows(fixture).every((b) => b.disabled)).toBe(true);
      expect(rows(fixture)[0].textContent).toContain('Adding…');
      expect(rows(fixture)[1].textContent).toContain('Add');
      expect(rows(fixture)[1].textContent).not.toContain('Adding…');

      // The add answers; the detail refetch is now in flight and HELD.
      getGate = held();
      addGate.release({ success: true });
      await settle(fixture);
      expect(seen.filter((r) => r.type === 'projectGet').length).toBe(2);
      expect(toasts(), 'no toast before the refetch lands').toEqual([]);
      expect(rows(fixture).every((b) => b.disabled)).toBe(true);
      expect(rows(fixture)[0].textContent).toContain('Adding…');

      getGate.release({ project: project() });
      await settle(fixture);
      expect(toasts()).toEqual([{ type: 'success', message: 'Character added to the roster' }]);
      expect(rows(fixture).some((b) => b.disabled)).toBe(false);
      expect(rows(fixture)[0].textContent).not.toContain('Adding…');
      // The picker stays open after a successful add.
      expect(searchBox(fixture)).not.toBeNull();
    } finally {
      unsubscribe();
    }
  });

  it("V9 add errors: the server's sentence verbatim, else the fixed fallback; rows re-enabled", async () => {
    const archived = 'That character is archived; rehydrate them before adding them to a roster.';
    for (const [thrown, expected] of [
      [new CoreDispatchError({ kind: 'bad-request', message: archived }), archived],
      [new CoreDispatchError({ kind: 'bad-request', message: '' }), 'Failed to add character'],
    ] as const) {
      TestBed.resetTestingModule();
      const fixture = await render(
        stubClient((r) => {
          if (r.type === 'characterList') return { characters: [listChar('c-bram', 'Bram')] };
          if (r.type === 'projectCharacterAdd') return thrown;
          return {};
        }),
        project({}),
      );
      await openPicker(fixture);
      rows(fixture)[0].click();
      await settle(fixture);
      expect(toasts()).toEqual([{ type: 'error', message: expected }]);
      expect(rows(fixture)[0].disabled).toBe(false);
      expect(rows(fixture)[0].textContent).not.toContain('Adding…');
    }
  });

  // --- V10 — toggle ------------------------------------------------------------

  it("V10 toggle: v4's two success sentences on the PUT body's flag", async () => {
    const seen: DispatchReq[] = [];
    const on = await render(
      stubClient((r) => {
        seen.push(r);
        return r.type === 'projectUpdate' ? { project: project({ allowAnyCharacter: true }) } : {};
      }),
      project({ allowAnyCharacter: false }),
    );
    toggle(on).click();
    await settle(on);
    expect(seen.find((r) => r.type === 'projectUpdate')).toMatchObject({
      projectId: 'p1',
      project: { allowAnyCharacter: true },
    });
    expect(toasts()).toEqual([
      { type: 'success', message: 'Every character may now use the project files and wardrobe' },
    ]);

    TestBed.resetTestingModule();
    const off = await render(
      stubClient((r) =>
        r.type === 'projectUpdate' ? { project: project({ allowAnyCharacter: false }) } : {},
      ),
      project({ allowAnyCharacter: true }),
    );
    toggle(off).click();
    await settle(off);
    expect(toasts()).toEqual([
      { type: 'success', message: 'Only roster characters may use the project files and wardrobe' },
    ]);
  });

  it("V10 toggle errors: a refusal is v4's fixed sentence, a thrown Error its message, else the fallback", async () => {
    const cases: [unknown, string][] = [
      [new CoreDispatchError({ kind: 'not-found', message: 'boom' }), 'Failed to update project'],
      [new Error('network down'), 'network down'],
      ['not an error', 'Failed to update setting'],
    ];
    for (const [thrown, expected] of cases) {
      TestBed.resetTestingModule();
      const fixture = await render(
        {
          dispatchData: (async (req: DispatchReq) => {
            if (req.type === 'projectUpdate') throw thrown;
            return {};
          }) as CoreClient['dispatchData'],
        },
        project({ allowAnyCharacter: false }),
      );
      toggle(fixture).click();
      await settle(fixture);
      expect(fixture.nativeElement.querySelector('.qt-alert-error')).toBeNull();
      expect(toasts()).toEqual([{ type: 'error', message: expected }]);
    }
  });

  // --- V11 — remove ------------------------------------------------------------

  it("V11 remove: v4's success sentence; a refusal is v4's fixed sentence", async () => {
    const characterRoster = [rosterChar('c1', 'Bertie')];
    const seen: DispatchReq[] = [];
    const fixture = await render(
      stubClient((r) => {
        seen.push(r);
        return {};
      }),
      project({ characterRoster }),
    );
    (
      el(fixture).querySelector(
        'button[aria-label="Remove Bertie from roster"]',
      ) as HTMLButtonElement
    ).click();
    await settle(fixture);
    expect(seen.find((r) => r.type === 'projectCharacterRemove')).toMatchObject({
      projectId: 'p1',
      characterId: 'c1',
    });
    expect(toasts()).toEqual([{ type: 'success', message: 'Character removed from the roster' }]);

    TestBed.resetTestingModule();
    const failing = await render(
      stubClient((r) =>
        r.type === 'projectCharacterRemove'
          ? new CoreDispatchError({ kind: 'not-found', message: 'cannot remove' })
          : {},
      ),
      project({ characterRoster }),
    );
    (
      el(failing).querySelector(
        'button[aria-label="Remove Bertie from roster"]',
      ) as HTMLButtonElement
    ).click();
    await settle(failing);
    // A fresh TestBed module ⇒ a fresh ToastService; only this render's toast.
    expect(toasts()).toEqual([{ type: 'error', message: 'Failed to remove character' }]);
  });

  // --- V12 — Done clears the search --------------------------------------------

  it('V12 Done closes the picker and clears the search', async () => {
    const fixture = await render(
      stubClient((r) =>
        r.type === 'characterList' ? { characters: [listChar('c-bram', 'Bram')] } : {},
      ),
      project({}),
    );
    await openPicker(fixture);
    await type(fixture, 'bra');
    buttonNamed(fixture, 'Done')!.click();
    await settle(fixture);
    expect(searchBox(fixture)).toBeNull();
    await openPicker(fixture);
    expect(searchBox(fixture)!.value).toBe('');
    expect(searchBox(fixture)!.getAttribute('placeholder')).toBe('Search characters…');
  });

  // --- V13 — the shared cache key ----------------------------------------------

  it('V13 the picker reads the SHARED characterKeys.list() entry (no second fetch when warm)', async () => {
    const seen: DispatchReq[] = [];
    const qc = new QueryClient({ defaultOptions: { queries: { staleTime: 60_000 } } });
    qc.setQueryData(characterKeys.list(), [listChar('c-bram', 'Bram')]);
    const fixture = await render(
      stubClient((r) => {
        seen.push(r);
        return {};
      }),
      project({}),
      { queryClient: qc },
    );
    await openPicker(fixture);
    expect(rowNames(fixture)).toEqual(['Bram']);
    expect(seen.filter((r) => r.type === 'characterList')).toEqual([]);
  });
});

describe('ProjectDetailScreen', () => {
  async function render(
    client: Partial<CoreClient>,
  ): Promise<ComponentFixture<ProjectDetailScreen>> {
    TestBed.configureTestingModule({
      imports: [ProjectDetailScreen],
      providers: [
        provideRouter([]),
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: client },
        {
          provide: ActivatedRoute,
          useValue: {
            paramMap: of(convertToParamMap({ id: 'p1' })),
            snapshot: { paramMap: convertToParamMap({ id: 'p1' }) },
          },
        },
      ],
    });
    const fixture = TestBed.createComponent(ProjectDetailScreen);
    fixture.detectChanges();
    await settle(fixture);
    return fixture;
  }

  function baseHandler(over?: (r: DispatchReq) => unknown) {
    return (r: DispatchReq) => {
      const custom = over?.(r);
      if (custom !== undefined) return custom;
      switch (r.type) {
        case 'projectGet':
          return { project: project() };
        case 'projectMountPointList':
          return { mountPoints: [] };
        case 'projectChatList':
          return { chats: [], total: 0 };
        default:
          return {};
      }
    };
  }

  it('renders the header and Scriptorium card from projectGet', async () => {
    const fixture = await render(stubClient(baseHandler()));
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Airship Saga');
    expect(text).toContain('The Scriptorium');
  });

  it('edits the title and saves name/description/instructions together', async () => {
    const seen: DispatchReq[] = [];
    const fixture = await render(
      stubClient((r) => {
        seen.push(r);
        return baseHandler()(r);
      }),
    );
    // Enter edit mode on the header.
    (
      [...fixture.nativeElement.querySelectorAll('button')].find(
        (b: HTMLButtonElement) => b.textContent?.trim() === 'Edit',
      ) as HTMLButtonElement
    ).click();
    await settle(fixture);
    const nameInput = fixture.nativeElement.querySelector(
      'input[aria-label="Project name"]',
    ) as HTMLInputElement;
    nameInput.value = 'Renamed Saga';
    nameInput.dispatchEvent(new Event('input'));
    await settle(fixture);
    (
      [...fixture.nativeElement.querySelectorAll('button')].find(
        (b: HTMLButtonElement) => b.textContent?.trim() === 'Save',
      ) as HTMLButtonElement
    ).click();
    await settle(fixture);
    const put = seen.find((r) => r.type === 'projectUpdate');
    expect(put).toMatchObject({
      projectId: 'p1',
      project: {
        name: 'Renamed Saga',
        description: 'A grand tale',
        instructions: 'Be dramatic',
      },
    });
    // v4 `useProjectDetail.ts:79` — toast only, no inline surface (P4.29).
    expect(toasts()).toEqual([{ type: 'success', message: 'Project updated!' }]);
  });

  it('toasts a failed header save with NO inline alert (v4 has none on this path, P4.29)', async () => {
    const fixture = await render(
      stubClient(
        baseHandler((r) => (r.type === 'projectUpdate' ? new Error('save failed') : undefined)),
      ),
    );
    (
      [...fixture.nativeElement.querySelectorAll('button')].find(
        (b: HTMLButtonElement) => b.textContent?.trim() === 'Edit',
      ) as HTMLButtonElement
    ).click();
    await settle(fixture);
    (
      [...fixture.nativeElement.querySelectorAll('button')].find(
        (b: HTMLButtonElement) => b.textContent?.trim() === 'Save',
      ) as HTMLButtonElement
    ).click();
    await settle(fixture);
    expect(fixture.nativeElement.querySelector('qt-error-alert')).toBeNull();
    expect(toasts()).toEqual([{ type: 'error', message: 'save failed' }]);
  });

  /**
   * v4 `useProjectDetail.ts:64-86` (`handleSave`) at `07b8f0209`: a non-OK
   * response throws the FIXED `Failed to update project` (the body is never
   * read); the catch toasts `err.message`, else the same sentence.
   */
  it("header save errors: a refusal is v4's fixed sentence, a thrown Error its message, else the fallback", async () => {
    const cases: [unknown, string][] = [
      [new CoreDispatchError({ kind: 'not-found', message: 'boom' }), 'Failed to update project'],
      [new Error('network down'), 'network down'],
      ['not an error', 'Failed to update project'],
    ];
    for (const [thrown, expected] of cases) {
      TestBed.resetTestingModule();
      const handler = baseHandler();
      const fixture = await render({
        dispatchData: (async (req: DispatchReq) => {
          if (req.type === 'projectUpdate') throw thrown;
          return (handler(req) ?? {}) as Record<string, unknown>;
        }) as CoreClient['dispatchData'],
      });
      (
        [...fixture.nativeElement.querySelectorAll('button')].find(
          (b: HTMLButtonElement) => b.textContent?.trim() === 'Edit',
        ) as HTMLButtonElement
      ).click();
      await settle(fixture);
      (
        [...fixture.nativeElement.querySelectorAll('button')].find(
          (b: HTMLButtonElement) => b.textContent?.trim() === 'Save',
        ) as HTMLButtonElement
      ).click();
      await settle(fixture);
      expect(fixture.nativeElement.querySelector('qt-error-alert')).toBeNull();
      expect(toasts()).toEqual([{ type: 'error', message: expected }]);
    }
  });

  /**
   * P4.D90: the `prospero` tab-activation entry sweeps the whole `['projects']`
   * prefix, which includes this screen's `projects.detail` read. v4 needed an
   * `isEditing` guard on its re-activation refresh because `fetchProject`
   * re-seeds `editForm` unconditionally (`ProjectDetailView.tsx:104-113`); v5
   * seeds ONCE PER ID, so a refetch of the SAME project cannot clobber
   * in-progress typing and the guard is unnecessary. Pinned here so the
   * measurement cannot rot silently.
   */
  it('a project refetch does not clobber in-progress header edits (v4 isEditing guard unnecessary)', async () => {
    // The refetch must answer a CHANGED project: TanStack's structural sharing
    // keeps the previous object reference for a deep-equal payload, and a
    // reference that never changes cannot re-run the seed effect at all — the
    // spec would pass whatever the guard did (the `false-green` class).
    let reads = 0;
    const fixture = await render(
      stubClient((r) => {
        if (r.type !== 'projectGet') return undefined;
        reads += 1;
        return { project: project({ name: reads > 1 ? 'Renamed Elsewhere' : 'Airship Saga' }) };
      }),
    );
    (
      [...fixture.nativeElement.querySelectorAll('button')].find(
        (b: HTMLButtonElement) => b.textContent?.trim() === 'Edit',
      ) as HTMLButtonElement
    ).click();
    await settle(fixture);
    const nameInput = fixture.nativeElement.querySelector(
      'input[aria-label="Project name"]',
    ) as HTMLInputElement;
    nameInput.value = 'Half-typed name';
    nameInput.dispatchEvent(new Event('input'));
    await settle(fixture);

    // What tab re-activation does to this screen.
    await TestBed.inject(QueryClient).invalidateQueries({ queryKey: projectKeys.all });
    await settle(fixture);
    expect(reads).toBeGreaterThan(1);

    const after = fixture.nativeElement.querySelector(
      'input[aria-label="Project name"]',
    ) as HTMLInputElement;
    expect(after.value).toBe('Half-typed name');
  });
});

describe('ProjectImageGenerationCard', () => {
  async function render(
    client: Partial<CoreClient>,
    proj: ProjectDetail,
  ): Promise<ComponentFixture<ProjectImageGenerationCard>> {
    TestBed.configureTestingModule({
      imports: [ProjectImageGenerationCard],
      providers: [
        provideRouter([]),
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: client },
      ],
    });
    const fixture = TestBed.createComponent(ProjectImageGenerationCard);
    fixture.componentRef.setInput('project', proj);
    fixture.componentRef.setInput('defaultOpen', true);
    fixture.detectChanges();
    await settle(fixture);
    return fixture;
  }

  /** The Save button of the card's first (lantern) aesthetic field. */
  function saveButton(fixture: ComponentFixture<ProjectImageGenerationCard>): HTMLButtonElement {
    return [
      ...fixture.nativeElement
        .querySelector('qt-project-aesthetic-field')!
        .querySelectorAll('button'),
    ].find((b: HTMLButtonElement) => b.textContent?.includes('Save')) as HTMLButtonElement;
  }

  it('immediate-saves the background display mode and toasts (P4.29, v4 has no inline surface)', async () => {
    const seen: DispatchReq[] = [];
    const fixture = await render(
      stubClient((r) => {
        seen.push(r);
        if (r.type === 'projectAestheticGet') return { content: '' };
        return r.type === 'projectUpdate' ? new Error('bg fail') : {};
      }),
      project(),
    );
    const bg = fixture.nativeElement.querySelector(
      'select[aria-label="Story Backgrounds"]',
    ) as HTMLSelectElement;
    bg.value = 'latest_chat';
    bg.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(seen.find((r) => r.type === 'projectUpdate')).toMatchObject({
      projectId: 'p1',
      project: { backgroundDisplayMode: 'latest_chat' },
    });
    expect(fixture.nativeElement.querySelector('.qt-alert-error')).toBeNull();
    expect(toasts()).toEqual([{ type: 'error', message: 'bg fail' }]);
  });

  /**
   * v4 `useProjectDetail.ts` at `07b8f0209`: each handler throws a FIXED
   * sentence on a non-OK response (the body is never read) and catches
   * `err instanceof Error ? err.message : '<catch fallback>'`. Avatar
   * generation and the background mode throw a sentence that is NOT their
   * catch fallback (`…avatar generation setting`, `…background display mode`).
   */
  const imageHandlers: { label: string; value: string; thrown: string; fallback: string }[] = [
    // v4 `:158-180` handleSaveAvatarGeneration — thrown != catch fallback.
    {
      label: 'Avatar Generation',
      value: 'enabled',
      thrown: 'Failed to update avatar generation setting',
      fallback: 'Failed to update avatar generation',
    },
    // v4 `:182-201` handleSaveDefaultImageProfile.
    {
      label: 'Default Image Profile',
      value: '',
      thrown: 'Failed to update default image profile',
      fallback: 'Failed to update image profile',
    },
    // v4 `:224-246` handleSaveAlertCharactersOfLanternImages — one sentence both ways.
    {
      label: 'Announce Lantern Images',
      value: 'enabled',
      thrown: 'Failed to update Lantern image announcement setting',
      fallback: 'Failed to update Lantern image announcement setting',
    },
    // v4 `:248-269` handleSaveBackgroundDisplayMode — thrown != catch fallback.
    {
      label: 'Story Backgrounds',
      value: 'latest_chat',
      thrown: 'Failed to update background display mode',
      fallback: 'Failed to update background mode',
    },
  ];

  for (const h of imageHandlers) {
    it(`${h.label}: a refusal is v4's fixed sentence, a thrown Error its message, else the catch fallback`, async () => {
      const cases: [unknown, string][] = [
        [new CoreDispatchError({ kind: 'not-found', message: 'boom' }), h.thrown],
        [new Error('network down'), 'network down'],
        ['not an error', h.fallback],
      ];
      for (const [thrown, expected] of cases) {
        TestBed.resetTestingModule();
        const fixture = await render(
          {
            dispatchData: (async (req: DispatchReq) => {
              if (req.type === 'projectUpdate') throw thrown;
              if (req.type === 'projectAestheticGet') return { content: '' };
              return {};
            }) as CoreClient['dispatchData'],
          },
          project(),
        );
        const select = fixture.nativeElement.querySelector(
          `select[aria-label="${h.label}"]`,
        ) as HTMLSelectElement;
        select.value = h.value;
        select.dispatchEvent(new Event('change'));
        await settle(fixture);
        expect(fixture.nativeElement.querySelector('.qt-alert-error')).toBeNull();
        expect(toasts()).toEqual([{ type: 'error', message: expected }]);
      }
    });
  }

  it('offers only the two modes that survived 4.9, with their hints (P4.D146, v4 70505745a)', async () => {
    // 'project' read a field only the Latest chat path ever wrote and 'static'
    // read a field nothing writes; both were retired. A stale option here would
    // let a user pick a mode the server's update schema now refuses outright.
    const fixture = await render(
      stubClient((r) => (r.type === 'projectAestheticGet' ? { content: '' } : {})),
      project({ backgroundDisplayMode: 'latest_chat' }),
    );
    const bg = fixture.nativeElement.querySelector(
      'select[aria-label="Story Backgrounds"]',
    ) as HTMLSelectElement;
    expect([...bg.options].map((o) => o.value)).toEqual(['theme', 'latest_chat']);
    expect([...bg.options].map((o) => o.textContent?.trim())).toEqual([
      'Use theme background (no image)',
      'Latest chat background',
    ]);
    // The hint follows the selected mode; only two survive. It is the `p`
    // immediately after the select (the card's own description is a different
    // `p.qt-text-xs` further up, which is why this is not a descendant query).
    expect((bg.nextElementSibling as HTMLElement | null)?.textContent?.trim()).toBe(
      'Shows the most recent background from any chat in this project.',
    );
  });

  it("the theme hint is the other survivor's (P4.D146)", async () => {
    // Its own `it` because `render` configures the TestBed, which may be
    // configured once per test.
    const themed = await render(
      stubClient((r) => (r.type === 'projectAestheticGet' ? { content: '' } : {})),
      project({ backgroundDisplayMode: 'theme' }),
    );
    const themedSelect = themed.nativeElement.querySelector(
      'select[aria-label="Story Backgrounds"]',
    ) as HTMLSelectElement;
    expect((themedSelect.nextElementSibling as HTMLElement | null)?.textContent?.trim()).toBe(
      'No background image, uses your theme colors.',
    );
  });

  it('the Default Image Profile select is live (P4.6r) and enabled', async () => {
    const fixture = await render(
      stubClient((r) => (r.type === 'projectAestheticGet' ? { content: '' } : {})),
      project(),
    );
    const prof = fixture.nativeElement.querySelector(
      'select[aria-label="Default Image Profile"]',
    ) as HTMLSelectElement;
    expect(prof.disabled).toBe(false);
  });

  it("toasts each of v4's avatar-generation and Lantern-announcement sentences", async () => {
    const fixture = await render(
      stubClient((r) => (r.type === 'projectAestheticGet' ? { content: '' } : {})),
      project({
        defaultAvatarGenerationEnabled: null,
        defaultAlertCharactersOfLanternImages: null,
      }),
    );
    const avatar = fixture.nativeElement.querySelector(
      'select[aria-label="Avatar Generation"]',
    ) as HTMLSelectElement;
    avatar.value = 'enabled';
    avatar.dispatchEvent(new Event('change'));
    await settle(fixture);
    avatar.value = 'disabled';
    avatar.dispatchEvent(new Event('change'));
    await settle(fixture);

    const announce = fixture.nativeElement.querySelector(
      'select[aria-label="Announce Lantern Images"]',
    ) as HTMLSelectElement;
    announce.value = 'enabled';
    announce.dispatchEvent(new Event('change'));
    await settle(fixture);
    announce.value = 'inherit';
    announce.dispatchEvent(new Event('change'));
    await settle(fixture);

    expect(toasts()).toEqual([
      { type: 'success', message: 'Avatar generation enabled by default for project' },
      { type: 'success', message: 'Avatar generation disabled by default for project' },
      {
        type: 'success',
        message: 'Lantern image announcements enabled by default for project',
      },
      {
        type: 'success',
        message: 'Lantern image announcements set to inherit from global',
      },
    ]);
  });

  it('toasts the image-profile success sentences (set + inherit), and a failure', async () => {
    const fixture = await render(
      stubClient((r) => {
        if (r.type === 'projectAestheticGet') return { content: '' };
        return {};
      }),
      project({ defaultImageProfileId: 'ip1' }),
    );
    const select = fixture.nativeElement.querySelector(
      'select[aria-label="Default Image Profile"]',
    ) as HTMLSelectElement;
    select.value = '';
    select.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(toasts()).toEqual([
      { type: 'success', message: 'Image profile set to inherit from global' },
    ]);

    TestBed.resetTestingModule();
    const failing = await render(
      stubClient((r) => {
        if (r.type === 'projectAestheticGet') return { content: '' };
        return r.type === 'projectUpdate' ? new Error('profile rejected') : {};
      }),
      project({ defaultImageProfileId: 'ip1' }),
    );
    const failSelect = failing.nativeElement.querySelector(
      'select[aria-label="Default Image Profile"]',
    ) as HTMLSelectElement;
    failSelect.value = '';
    failSelect.dispatchEvent(new Event('change'));
    await settle(failing);
    expect(toasts()).toEqual([{ type: 'error', message: 'profile rejected' }]);
  });

  it('toasts each Story-Backgrounds mode label on success', async () => {
    const fixture = await render(
      stubClient((r) => (r.type === 'projectAestheticGet' ? { content: '' } : {})),
      project({ backgroundDisplayMode: 'theme' }),
    );
    const bg = fixture.nativeElement.querySelector(
      'select[aria-label="Story Backgrounds"]',
    ) as HTMLSelectElement;
    // [P4.D146 / v4 70505745a] Two labels, not four: `modeLabels` lost its
    // 'project' and 'static' entries with the options they named.
    bg.value = 'latest_chat';
    bg.dispatchEvent(new Event('change'));
    await settle(fixture);
    bg.value = 'theme';
    bg.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(toasts()).toEqual([
      { type: 'success', message: 'Background set to latest chat background' },
      { type: 'success', message: 'Background set to theme background' },
    ]);
  });

  it('saves the edited aesthetic content through projectAestheticSet', async () => {
    const saved: DispatchReq[] = [];
    const content = '# House style\n\n- muted **sepia** tones\n';
    const fixture = await render(
      stubClient((r) => {
        if (r.type === 'projectAestheticGet') {
          return r['kind'] === 'lantern' ? { content } : { content: '' };
        }
        if (r.type === 'projectAestheticSet') saved.push(r);
        return {};
      }),
      project(),
    );
    // The lantern field is the first aesthetic editor on the card. The editor
    // DISPLAYS a canonical serialization — here that drops the file's trailing
    // newline — because it mounts with the loaded content already in hand.
    const lantern = fixture.debugElement.queryAll(By.directive(RichEditor))[0];
    expect(lantern.componentInstance.getMarkdown()).toBe(content.trimEnd());
    // A genuine edit dirties the field, which is what unlocks Save (v4
    // `AestheticEditorField.tsx:127`). What the editor then writes back is its
    // own serialization of the edited document — v4 saves exactly the same
    // bytes, from the same round trip through its Lexical markdown bridge.
    const h2 = fixture.nativeElement
      .querySelector('qt-project-aesthetic-field')!
      .querySelector('.qt-formatting-button-h2') as HTMLButtonElement;
    h2.click();
    await settle(fixture);
    const saveBtn = saveButton(fixture);
    expect(saveBtn.disabled).toBe(false);
    saveBtn.click();
    await settle(fixture);
    expect(saved.find((r) => r.type === 'projectAestheticSet')).toMatchObject({
      projectId: 'p1',
      kind: 'lantern',
      content: lantern.componentInstance.getMarkdown(),
    });
  });

  it('a load never dirties the field, so it cannot re-normalize the stored bytes', async () => {
    // The sharp edge of the markdown swap: `__bold__` parses to the same tree as
    // `**bold**` and serializes to the latter. If a load surfaced as an edit,
    // merely opening the card and clicking Save would silently rewrite the
    // user's file in normalized bytes. v4 cannot do this — its mount parse is
    // tagged external-sync and skipped by the change listener
    // (MarkdownBridgePlugin.tsx:168-181), so `dirty` stays false and the Save
    // button stays disabled. Neither may we: the field mounts with its content
    // in hand and absorbs the one mount emit, so nothing marks it dirty and
    // there is no reachable Save to write with. Re-point the field at an
    // ungated load and this assertion fails.
    const content = 'Muted __sepia__ tones.';
    const fixture = await render(
      stubClient((r) =>
        r.type === 'projectAestheticGet' ? { content: r['kind'] === 'lantern' ? content : '' } : {},
      ),
      project(),
    );
    // The editor displays the canonical form...
    expect(
      fixture.debugElement.queryAll(By.directive(RichEditor))[0].componentInstance.getMarkdown(),
    ).toBe('Muted **sepia** tones.');
    // ...and the load left the field pristine, so Save is unreachable.
    expect(saveButton(fixture).disabled).toBe(true);
  });
});

describe('ProjectFilesCard', () => {
  /**
   * A row exactly as `projectFileList` answers it — copied from the dogfood
   * copy's LUC Ranch project (store-backed, 2026-10-03). The card had read
   * `fileName` / `fileSizeBytes`, which this wire (v4's and v5's) never
   * carries, so every row rendered nameless at `0 B` (dogfood #135).
   */
  function storeRow(over: Partial<ProjectFileDto> = {}): ProjectFileDto {
    return {
      id: '26373ad4-bbe3-4e37-8e15-cf7c77cd385b',
      originalFilename: 'description.md',
      filename: 'description.md',
      mimeType: 'text/markdown',
      size: 39,
      category: 'DOCUMENT',
      description: null,
      projectId: 'p1',
      folderPath: null,
      width: null,
      height: null,
      createdAt: '2026-06-08T00:36:38.374Z',
      updatedAt: '2026-06-08T00:36:38.374Z',
      mountPointId: 'mp-1',
      relativePath: 'description.md',
      ...over,
    };
  }

  /** A legacy files-table row (no mount keys), v4's second branch. */
  function legacyRow(over: Partial<ProjectFileDto> = {}): ProjectFileDto {
    return {
      id: 'f1',
      userId: 'u1',
      originalFilename: 'map.png',
      filename: 'map.png',
      mimeType: 'image/png',
      size: 2048,
      category: 'IMAGE',
      description: null,
      projectId: 'p1',
      folderPath: '/',
      width: 64,
      height: 64,
      createdAt: '2024-01-01T00:00:00Z',
      updatedAt: '2024-01-01T00:00:00Z',
      ...over,
    };
  }

  async function render(files: ProjectFileDto[]): Promise<ComponentFixture<ProjectFilesCard>> {
    const client = stubClient((r) => (r.type === 'projectFileList' ? { files } : {}));
    TestBed.configureTestingModule({
      imports: [ProjectFilesCard],
      providers: [
        provideRouter([]),
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: client },
      ],
    });
    const fixture = TestBed.createComponent(ProjectFilesCard);
    fixture.componentRef.setInput('projectId', 'p1');
    fixture.componentRef.setInput('defaultOpen', true);
    fixture.detectChanges();
    await settle(fixture);
    return fixture;
  }

  /** Each file row's two lines — the name line and the `size • category` line. */
  function rowTexts(fixture: ComponentFixture<ProjectFilesCard>): string[] {
    return [...fixture.nativeElement.querySelectorAll('.max-h-64 > button')].map(
      (b: HTMLButtonElement) =>
        [...b.querySelectorAll('p')]
          .map((p) => p.textContent?.replace(/\s+/g, ' ').trim() ?? '')
          .join(' | '),
    );
  }

  it('shows the empty state when there are no files', async () => {
    const empty = await render([]);
    expect(empty.nativeElement.textContent).toContain('No files in this project yet');
  });

  it("names each row by v4's originalFilename and sizes it by v4's size", async () => {
    const fixture = await render([storeRow(), legacyRow()]);
    expect(rowTexts(fixture)).toEqual([
      'description.md | 39 B • DOCUMENT',
      'map.png | 2.0 KB • IMAGE',
    ]);
  });

  it("feeds v4's FileThumbnail the row: a store image through the mount blob, a legacy one through the thumbnail route", async () => {
    const fixture = await render([
      storeRow({ id: 's1', originalFilename: 'cover.png', mimeType: 'image/png', relativePath: 'art/cover.png' }),
      legacyRow(),
    ]);
    const imgs = [...fixture.nativeElement.querySelectorAll('img')] as HTMLImageElement[];
    expect(imgs.map((i) => i.getAttribute('alt'))).toEqual(['cover.png', 'map.png']);
    expect(imgs[0].getAttribute('src')).toContain('/api/v1/mount-points/mp-1/blobs/art/cover.png');
    expect(imgs[1].getAttribute('src')).toContain('/api/v1/files/f1?action=thumbnail&size=40');
  });

  it("caps the list at the first 10 files, says v4's +N more files, and disables Browse All Files", async () => {
    const many = Array.from({ length: 14 }, (_, i) =>
      storeRow({ id: `f${i}`, originalFilename: `f${i}.md`, relativePath: `f${i}.md` }),
    );
    const fixture = await render(many);
    expect(rowTexts(fixture).length).toBe(10);
    expect(fixture.nativeElement.textContent).toContain('+4 more files');
    const browse = [...fixture.nativeElement.querySelectorAll('button')].find(
      (b: HTMLButtonElement) => b.textContent?.trim() === 'Browse All Files',
    ) as HTMLButtonElement;
    expect(browse.disabled).toBe(true);
  });
});

/**
 * The project story-background resolver (P4.D92, v4 bug 80's read half —
 * `useStoryBackground(null, projectId, …)` over `?action=get-background`).
 */
describe('fetchProjectBackground', () => {
  it('reads the BARE body verbatim — the url is already the byte route', async () => {
    const seen: DispatchReq[] = [];
    const client = stubClient((r) => {
      seen.push(r);
      return {
        backgroundUrl: '/api/v1/files/bg-1',
        displayMode: 'latest_chat',
        sourceChatId: 'c1',
      };
    }) as CoreClient;
    expect(await fetchProjectBackground(client, 'p1')).toEqual({
      backgroundUrl: '/api/v1/files/bg-1',
      displayMode: 'latest_chat',
      sourceChatId: 'c1',
    });
    expect(seen).toEqual([{ type: 'projectBackgroundGet', projectId: 'p1' }]);
  });

  it("the 'theme' arm carries a null url and no source chat", async () => {
    const client = stubClient(() => ({ backgroundUrl: null, displayMode: 'theme' })) as CoreClient;
    expect(await fetchProjectBackground(client, 'p1')).toEqual({
      backgroundUrl: null,
      displayMode: 'theme',
      sourceChatId: null,
    });
  });
});

/**
 * P4.D92 — bug 80's fix (v4 `c6ff8051`): the project detail REPORTS its story
 * background to the workspace backdrop. v4's own regression test
 * (`__tests__/unit/components/workspace/workspace-backdrop.test.tsx`, "lets a
 * drilled-into detail replace its list view background") pins the LIST→DETAIL
 * reporter swap, because v4 had two reporters racing over one tab key. v5's
 * Prospero list reports nothing (no subsystem-background machinery exists —
 * the standing divergence), so the measurable v5 mirror is the other half of
 * that same guarantee: exactly one entry lives under the tab key while the
 * detail is mounted, and it is GONE the moment the detail is not.
 *
 * The wiring is driven through the component's real lifecycle — render, data
 * arrival, `fixture.destroy()` — never a private method (the `979652a9` round's
 * §3 class: a spec that calls the method cannot see a reporter that was never
 * wired to the view at all).
 */
describe('ProjectDetailScreen — the workspace backdrop reporter (bug 80)', () => {
  interface RegistryCall {
    op: 'report' | 'clear';
    tabId: string;
    entry?: WorkspaceBackdropEntry;
  }

  function fakeRegistry(calls: RegistryCall[]) {
    return {
      report: (tabId: string, entry: WorkspaceBackdropEntry) =>
        calls.push({ op: 'report', tabId, entry }),
      clear: (tabId: string) => calls.push({ op: 'clear', tabId }),
    };
  }

  async function render(opts: {
    backgroundUrl: string | null;
    mode?: ProjectDetail['backgroundDisplayMode'];
    tabId?: string | null;
    calls: RegistryCall[];
  }): Promise<ComponentFixture<ProjectDetailScreen>> {
    const client = stubClient((r: DispatchReq) => {
      switch (r.type) {
        case 'projectGet':
          return { project: project({ backgroundDisplayMode: opts.mode ?? 'latest_chat' }) };
        case 'projectBackgroundGet':
          return {
            backgroundUrl: opts.backgroundUrl,
            displayMode: opts.mode ?? 'latest_chat',
            ...(opts.backgroundUrl ? { sourceChatId: 'c1' } : {}),
          };
        case 'projectMountPointList':
          return { mountPoints: [] };
        case 'projectChatList':
          return { chats: [], total: 0 };
        default:
          return {};
      }
    });
    TestBed.configureTestingModule({
      imports: [ProjectDetailScreen],
      providers: [
        provideRouter([]),
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: client },
        {
          provide: ActivatedRoute,
          useValue: {
            paramMap: of(convertToParamMap({ id: 'p1' })),
            snapshot: { paramMap: convertToParamMap({ id: 'p1' }) },
          },
        },
        ...(opts.tabId === null
          ? []
          : [
              { provide: WORKSPACE_TAB_ID, useValue: opts.tabId ?? 'tab-7' },
              { provide: WORKSPACE_BACKDROP_REGISTRY, useValue: fakeRegistry(opts.calls) },
            ]),
      ],
    });
    const fixture = TestBed.createComponent(ProjectDetailScreen);
    fixture.detectChanges();
    await settle(fixture);
    return fixture;
  }

  it('reports the resolved background under its own tab id, isSalon false', async () => {
    const calls: RegistryCall[] = [];
    const fixture = await render({ backgroundUrl: '/api/v1/files/bg-1', calls });
    expect(calls.filter((c) => c.op === 'report')).toEqual([
      {
        op: 'report',
        tabId: 'tab-7',
        entry: { url: '/api/v1/files/bg-1', isSalon: false },
      },
    ]);
    fixture.destroy();
  });

  it('clears the entry when the detail goes away (drill-back / tab close)', async () => {
    const calls: RegistryCall[] = [];
    const fixture = await render({ backgroundUrl: '/api/v1/files/bg-1', calls });
    expect(calls.at(-1)?.op).toBe('report');
    fixture.destroy();
    expect(calls.at(-1)).toEqual({ op: 'clear', tabId: 'tab-7' });
  });

  it("reports NOTHING for 'theme' mode — v5 has no Prospero subsystem fallback", async () => {
    const calls: RegistryCall[] = [];
    const fixture = await render({ backgroundUrl: null, mode: 'theme', calls });
    expect(calls.some((c) => c.op === 'report')).toBe(false);
    expect(calls.some((c) => c.op === 'clear')).toBe(true);
    fixture.destroy();
  });

  it('is inert outside the workspace (routed mode reports nothing)', async () => {
    const calls: RegistryCall[] = [];
    const fixture = await render({ backgroundUrl: '/api/v1/files/bg-1', tabId: null, calls });
    expect(calls).toEqual([]);
    fixture.destroy();
  });

  /**
   * v4 keeps the legacy per-view CSS variable alongside the backdrop report
   * (`ProjectDetailView.tsx:148-151`), because the routed page renders outside
   * `.qt-workspace` where the `::before` layer still works. v5 serves that route
   * whenever the workspace-tabs flag is off, so the variable is carried too.
   */
  it("sets v4's legacy --story-background-url on the page container", async () => {
    const fixture = await render({ backgroundUrl: '/api/v1/files/bg-1', tabId: null, calls: [] });
    const container = fixture.nativeElement.querySelector('.qt-page-container') as HTMLElement;
    expect(container.getAttribute('style') ?? '').toContain(
      "--story-background-url: url('/api/v1/files/bg-1')",
    );
    fixture.destroy();
  });

  // No background → the variable is absent, so the
  // `:not([style*="--story-background-url"])::before` rule keeps the layer
  // hidden (v4 passes `undefined` for the whole style object). Its own `it`:
  // TestBed cannot be configured twice in one case.
  it('omits the legacy variable when there is no background', async () => {
    const fixture = await render({ backgroundUrl: null, mode: 'theme', tabId: null, calls: [] });
    const container = fixture.nativeElement.querySelector('.qt-page-container') as HTMLElement;
    expect(container.getAttribute('style') ?? '').not.toContain('--story-background-url');
    fixture.destroy();
  });
});

/** v4's passive-poll gate (`ProjectDetailView.tsx:90`), quirk included. */
describe('shouldPassivePollBackground', () => {
  it("polls for every mode except 'theme'", () => {
    expect(shouldPassivePollBackground('latest_chat')).toBe(true);
    expect(shouldPassivePollBackground('project')).toBe(true);
    expect(shouldPassivePollBackground('static')).toBe(true);
    expect(shouldPassivePollBackground('theme')).toBe(false);
  });

  it('polls while the project is still loading (v4 `project?.…` is undefined)', () => {
    expect(shouldPassivePollBackground(undefined)).toBe(true);
    expect(shouldPassivePollBackground(null)).toBe(true);
  });
});
