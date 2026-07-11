import { ComponentFixture, TestBed } from '@angular/core/testing';
import { ActivatedRoute, convertToParamMap, provideRouter } from '@angular/router';
import { of } from 'rxjs';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type { ProjectDetail } from '../../core/core-contract';
import { ProjectCharactersCard } from './cards/project-characters-card';
import { ProjectModelBehaviorCard } from './cards/project-model-behavior-card';
import { ProjectDetailScreen } from './project-detail';
import { ProsperoList } from './prospero-list';

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
    roster: [],
    defaultAgentModeEnabled: null,
    defaultAvatarGenerationEnabled: null,
    defaultImageProfileId: null,
    defaultRoleplayTemplateId: null,
    defaultAlertCharactersOfLanternImages: null,
    answerConfirmationOverride: null,
    backgroundDisplayMode: 'theme',
    state: {},
    createdAt: '2024-01-01T00:00:00Z',
    updatedAt: '2024-01-01T00:00:00Z',
    ...over,
  };
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

  it('delete goes through the confirm dialog and surfaces an alert on failure', async () => {
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
    expect(fixture.nativeElement.querySelector('.qt-alert-error')).toBeTruthy();
    expect(fixture.nativeElement.textContent).toContain('Failed to delete project');
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

  it('immediate-saves the agent mode select and surfaces an alert when it fails', async () => {
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
    expect(put).toMatchObject({ projectId: 'p1', defaultAgentModeEnabled: true });
    // The failed immediate save surfaces the alert.
    expect(fixture.nativeElement.querySelector('.qt-alert-error')).toBeTruthy();
  });

  it('the Default Roleplay Template select is a disabled (deferred) affordance', async () => {
    const fixture = await render(
      stubClient(() => ({})),
      project(),
    );
    const rp = fixture.nativeElement.querySelector(
      'select[aria-label="Default Roleplay Template"]',
    ) as HTMLSelectElement;
    expect(rp.disabled).toBe(true);
  });
});

describe('ProjectCharactersCard', () => {
  async function render(
    client: Partial<CoreClient>,
    proj: ProjectDetail,
  ): Promise<ComponentFixture<ProjectCharactersCard>> {
    TestBed.configureTestingModule({
      imports: [ProjectCharactersCard],
      providers: [
        provideRouter([]),
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: client },
      ],
    });
    const fixture = TestBed.createComponent(ProjectCharactersCard);
    fixture.componentRef.setInput('project', proj);
    fixture.componentRef.setInput('defaultOpen', true);
    fixture.detectChanges();
    await settle(fixture);
    return fixture;
  }

  it('toggles Allow Any Character with an immediate PUT and alerts on failure', async () => {
    const seen: DispatchReq[] = [];
    const fixture = await render(
      stubClient((r) => {
        seen.push(r);
        return r.type === 'projectUpdate' ? new Error('boom') : {};
      }),
      project({ allowAnyCharacter: false }),
    );
    const toggle = fixture.nativeElement.querySelector(
      'button[aria-label="Allow Any Character"]',
    ) as HTMLButtonElement;
    toggle.click();
    await settle(fixture);
    expect(seen.find((r) => r.type === 'projectUpdate')).toMatchObject({
      projectId: 'p1',
      allowAnyCharacter: true,
    });
    expect(fixture.nativeElement.querySelector('.qt-alert-error')).toBeTruthy();
  });

  it('renders the roster with the no-add-picker note when empty', async () => {
    const fixture = await render(
      stubClient(() => ({})),
      project({ roster: [] }),
    );
    expect(fixture.nativeElement.textContent).toContain(
      'Characters are added when chats are associated',
    );
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
      name: 'Renamed Saga',
      description: 'A grand tale',
      instructions: 'Be dramatic',
    });
  });

  it('surfaces an alert when the header save fails', async () => {
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
    expect(fixture.nativeElement.textContent).toContain('save failed');
  });
});
