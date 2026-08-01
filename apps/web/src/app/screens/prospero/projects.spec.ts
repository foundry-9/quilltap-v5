import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { ActivatedRoute, convertToParamMap, provideRouter } from '@angular/router';
import { of } from 'rxjs';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { RichEditor } from '../../editor/rich-editor';
import type { ProjectDetail, ProjectFileDto } from '../../core/core-contract';
import { WORKSPACE_TAB_ID } from '../../workspace/workspace-contract';
import { ProjectCharactersCard } from './cards/project-characters-card';
import { ProjectFilesCard } from './cards/project-files-card';
import { ProjectImageGenerationCard } from './cards/project-image-generation-card';
import { ProjectModelBehaviorCard } from './cards/project-model-behavior-card';
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
    roster: [],
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
      (b: HTMLButtonElement) => b.textContent?.includes('Projects') && !b.textContent?.includes('Create'),
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
      fixture.debugElement.query(By.directive(ProjectDetailScreen)).componentInstance
        .projectIdInput(),
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
    expect(put).toMatchObject({ projectId: 'p1', project: { defaultAgentModeEnabled: true } });
    // The failed immediate save surfaces the alert.
    expect(fixture.nativeElement.querySelector('.qt-alert-error')).toBeTruthy();
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
      project: { allowAnyCharacter: true },
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
      project: {
        name: 'Renamed Saga',
        description: 'A grand tale',
        instructions: 'Be dramatic',
      },
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
      ...fixture.nativeElement.querySelector('qt-project-aesthetic-field')!.querySelectorAll('button'),
    ].find((b: HTMLButtonElement) => b.textContent?.includes('Save')) as HTMLButtonElement;
  }

  it('immediate-saves the background display mode and alerts on failure', async () => {
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
    bg.value = 'project';
    bg.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(seen.find((r) => r.type === 'projectUpdate')).toMatchObject({
      projectId: 'p1',
      project: { backgroundDisplayMode: 'project' },
    });
    expect(fixture.nativeElement.querySelector('.qt-alert-error')).toBeTruthy();
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
        r.type === 'projectAestheticGet'
          ? { content: r['kind'] === 'lantern' ? content : '' }
          : {},
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
  function file(over: Partial<ProjectFileDto> = {}): ProjectFileDto {
    return {
      id: 'f1',
      fileName: 'map.png',
      mimeType: 'image/png',
      fileSizeBytes: 2048,
      category: 'image',
      filepath: '/uploads/map.png',
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

  it('shows the empty state when there are no files', async () => {
    const empty = await render([]);
    expect(empty.nativeElement.textContent).toContain('No files in this project yet');
  });

  it('lists files with size and category', async () => {
    const fixture = await render([file({ fileName: 'map.png', category: 'image' })]);
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('map.png');
    expect(text).toContain('2.0 KB');
    expect(text).toContain('image');
  });

  it('caps the list at the first 10 files and disables Browse All Files', async () => {
    const many = Array.from({ length: 14 }, (_, i) => file({ id: `f${i}`, fileName: `f${i}.png` }));
    const fixture = await render(many);
    const rows = fixture.nativeElement.querySelectorAll('img');
    expect(rows.length).toBe(10);
    const browse = [...fixture.nativeElement.querySelectorAll('button')].find(
      (b: HTMLButtonElement) => b.textContent?.trim() === 'Browse All Files',
    ) as HTMLButtonElement;
    expect(browse.disabled).toBe(true);
  });
});
