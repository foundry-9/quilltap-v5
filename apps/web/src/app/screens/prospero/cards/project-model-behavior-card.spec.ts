import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { CoreDispatchError } from '../../../core/core-contract';
import type { ProjectDetail, RoleplayTemplateDto } from '../../../core/core-contract';
import { ToastService } from '../../../ui/toast.service';
import { ProjectToolSettingsModal } from '../project-tool-settings-modal';
import { ProjectModelBehaviorCard } from './project-model-behavior-card';

function project(over: Partial<ProjectDetail>): ProjectDetail {
  return {
    id: 'proj1',
    name: 'Project',
    description: null,
    instructions: null,
    color: null,
    icon: null,
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
    state: null,
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
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

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

describe('ProjectModelBehaviorCard — roleplay template picker', () => {
  it('displays the stored template once the async options arrive (dogfood #6)', async () => {
    const client: Partial<CoreClient> = {
      dispatchData: (async () => [tmpl('t1', 'Epic'), tmpl('t2', 'Cozy')]) as unknown as CoreClient['dispatchData'],
    };
    TestBed.configureTestingModule({
      imports: [ProjectModelBehaviorCard],
      providers: [
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: client },
      ],
    });
    const fixture = TestBed.createComponent(ProjectModelBehaviorCard);
    // The stored value is set BEFORE the options load — the classic reset trap.
    fixture.componentRef.setInput('project', project({ defaultRoleplayTemplateId: 't1' }));
    fixture.componentRef.setInput('defaultOpen', true);
    fixture.detectChanges();
    await settle(fixture);

    const select = fixture.nativeElement.querySelector(
      'select[aria-label="Default Roleplay Template"]',
    ) as HTMLSelectElement;
    // The `[selected]`-per-option binding must reflect the stored id, not reset to "".
    expect(select.value).toBe('t1');
    const selectedOption = select.options[select.selectedIndex];
    expect(selectedOption.textContent?.trim()).toBe('Epic');
  });

  it('saves the chosen template id (empty → null) on change', async () => {
    const dispatchData = vi.fn(async (req: { type: string }) => {
      if (req.type === 'roleplayTemplateList') {
        return [tmpl('t1', 'Epic')];
      }
      return { project: project({ defaultRoleplayTemplateId: 't1' }) };
    });
    TestBed.configureTestingModule({
      imports: [ProjectModelBehaviorCard],
      providers: [
        provideTanStackQuery(new QueryClient()),
        {
          provide: CoreClient,
          useValue: { dispatchData: dispatchData as unknown as CoreClient['dispatchData'] },
        },
      ],
    });
    const fixture = TestBed.createComponent(ProjectModelBehaviorCard);
    fixture.componentRef.setInput('project', project({ defaultRoleplayTemplateId: null }));
    fixture.componentRef.setInput('defaultOpen', true);
    fixture.detectChanges();
    await settle(fixture);

    const select = fixture.nativeElement.querySelector(
      'select[aria-label="Default Roleplay Template"]',
    ) as HTMLSelectElement;
    select.value = 't1';
    select.dispatchEvent(new Event('change'));
    await settle(fixture);

    expect(dispatchData).toHaveBeenCalledWith(
      expect.objectContaining({
        type: 'projectUpdate',
        projectId: 'proj1',
        project: { defaultRoleplayTemplateId: 't1' },
      }),
    );
  });
});

describe('ProjectModelBehaviorCard — the Default Tool Settings row (P4.9E4B)', () => {
  async function renderCard(over: Partial<ProjectDetail>): Promise<ComponentFixture<ProjectModelBehaviorCard>> {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [ProjectModelBehaviorCard],
      providers: [
        provideTanStackQuery(new QueryClient()),
        {
          provide: CoreClient,
          useValue: {
            dispatchData: (async () => []) as unknown as CoreClient['dispatchData'],
          },
        },
      ],
    });
    const fixture = TestBed.createComponent(ProjectModelBehaviorCard);
    fixture.componentRef.setInput('project', project(over));
    fixture.componentRef.setInput('defaultOpen', true);
    fixture.detectChanges();
    await settle(fixture);
    return fixture;
  }

  function configureButton(fixture: ComponentFixture<unknown>): HTMLButtonElement {
    return Array.from(
      fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
    ).find((b) => b.textContent?.trim() === 'Configure')!;
  }

  it('summarises an untouched project as "All tools enabled", with Configure LIVE', async () => {
    const fixture = await renderCard({});
    expect(fixture.nativeElement.textContent).toContain('All tools enabled');
    // The stub's disabled affordance is gone.
    expect(configureButton(fixture).disabled).toBe(false);
  });

  it('pluralises v4’s counts and joins both halves', async () => {
    const one = await renderCard({ defaultDisabledTools: ['a'] });
    expect(one.nativeElement.textContent).toContain('1 tool disabled');

    const many = await renderCard({
      defaultDisabledTools: ['a', 'b'],
      defaultDisabledToolGroups: ['g'],
    });
    expect(many.nativeElement.textContent).toContain('2 tools disabled, 1 group disabled');

    const groupsOnly = await renderCard({ defaultDisabledToolGroups: ['g', 'h'] });
    expect(groupsOnly.nativeElement.textContent).toContain('2 groups disabled');
    expect(groupsOnly.nativeElement.textContent).not.toContain('tool disabled');
  });

  it('opens the dialog and adopts its result at once (v4 handleToolSettingsSuccess)', async () => {
    const fixture = await renderCard({});
    configureButton(fixture).click();
    fixture.detectChanges();
    await settle(fixture);
    expect(fixture.nativeElement.textContent).toContain('Default Tool Settings');

    // v4 replaces its local arrays from the success payload, so the summary moves
    // before any refetch lands.
    const dialog = fixture.debugElement.query(By.directive(ProjectToolSettingsModal))
      .componentInstance as ProjectToolSettingsModal;
    dialog.saved.emit({ disabledTools: ['a', 'b'], disabledToolGroups: [] });
    await settle(fixture);
    expect(fixture.nativeElement.textContent).toContain('2 tools disabled');
  });
});

/**
 * v4 `useProjectDetail.ts` at `07b8f0209` — each handler throws a FIXED
 * sentence on a non-OK response (the body is never read) and catches
 * `err instanceof Error ? err.message : '<catch fallback>'`. So a refusal
 * toasts the THROWN sentence, a plain `Error` its own message, and anything
 * else the catch fallback. All three handlers here throw a sentence that is
 * NOT their catch fallback (agent mode and answer confirmation gain
 * `setting`; the roleplay template drops `default`, v4 `:211/:218`) — the rows
 * that were red when the refusal leaked into the catch's `fallback` slot.
 */
describe('ProjectModelBehaviorCard — failure toasts (v4 three-way branch)', () => {
  // `stored` is what the select shows for `project({})` — where every failure
  // must leave it (dogfood #143: v4's controlled select snaps back).
  const handlers: {
    label: string;
    value: string;
    stored: string;
    thrown: string;
    fallback: string;
  }[] = [
    // v4 `:110-132` handleSaveAgentMode — thrown != catch fallback.
    {
      label: 'Agent Mode',
      value: 'enabled',
      stored: 'inherit',
      thrown: 'Failed to update agent mode setting',
      fallback: 'Failed to update agent mode',
    },
    // v4 `:134-156` handleSaveAnswerConfirmationOverride — thrown != catch fallback.
    {
      label: 'Answer Confirmation',
      value: 'ON',
      stored: 'inherit',
      thrown: 'Failed to update answer confirmation setting',
      fallback: 'Failed to update answer confirmation',
    },
    // v4 `:203-222` handleSaveDefaultRoleplayTemplate.
    {
      label: 'Default Roleplay Template',
      value: 't1',
      stored: '',
      thrown: 'Failed to update default roleplay template',
      fallback: 'Failed to update roleplay template',
    },
  ];

  for (const h of handlers) {
    it(`${h.label}: a refusal is v4's fixed sentence, a thrown Error its message, else the catch fallback`, async () => {
      const cases: [unknown, string][] = [
        [new CoreDispatchError({ kind: 'not-found', message: 'boom' }), h.thrown],
        [new Error('network down'), 'network down'],
        ['not an error', h.fallback],
      ];
      for (const [thrown, expected] of cases) {
        TestBed.resetTestingModule();
        TestBed.configureTestingModule({
          imports: [ProjectModelBehaviorCard],
          providers: [
            provideTanStackQuery(new QueryClient()),
            {
              provide: CoreClient,
              useValue: {
                dispatchData: (async (req: { type: string }) => {
                  if (req.type === 'projectUpdate') throw thrown;
                  if (req.type === 'roleplayTemplateList') return [tmpl('t1', 'Epic')];
                  return {};
                }) as unknown as CoreClient['dispatchData'],
              },
            },
          ],
        });
        const fixture = TestBed.createComponent(ProjectModelBehaviorCard);
        fixture.componentRef.setInput('project', project({}));
        fixture.componentRef.setInput('defaultOpen', true);
        fixture.detectChanges();
        await settle(fixture);

        const select = fixture.nativeElement.querySelector(
          `select[aria-label="${h.label}"]`,
        ) as HTMLSelectElement;
        select.value = h.value;
        // The chosen option must exist, or the revert assertion below is vacuous.
        expect(select.value).toBe(h.value);
        select.dispatchEvent(new Event('change'));
        await settle(fixture);

        expect(fixture.nativeElement.querySelector('.qt-alert-error')).toBeNull();
        expect(
          TestBed.inject(ToastService)
            .toasts()
            .map((t) => ({ type: t.type, message: t.message })),
        ).toEqual([{ type: 'error', message: expected }]);
        // The refused choice does not stay on screen (dogfood #143).
        expect(select.value).toBe(h.stored);
      }
    });
  }
});

/**
 * P4.D388 (C10): v4 `c539c82e0`'s "Inline Brahma Engine" block in
 * `ModelBehaviorCard.tsx:157-170` and `useProjectDetail.ts:159-178`
 * (`handleSaveBrahmaConnectionProfile`) at the pin `624d0d25c`.
 */
describe('ProjectModelBehaviorCard — the inline Brahma engine (P4.D388, v4 c539c82e0)', () => {
  const PROFILES = [
    { id: 'p1', name: 'Aria GPT', provider: 'OPENAI', modelName: 'gpt-5', isDefault: true, allowToolUse: true },
  ];

  async function mountCard(
    over: Partial<ProjectDetail>,
    onUpdate: (req: Record<string, unknown>) => unknown = () => ({ project: project(over) }),
  ): Promise<{ fixture: ComponentFixture<ProjectModelBehaviorCard>; dispatchData: ReturnType<typeof vi.fn> }> {
    const dispatchData = vi.fn(async (req: Record<string, unknown>) => {
      if (req['type'] === 'projectUpdate') return onUpdate(req);
      if (req['type'] === 'roleplayTemplateList') return [];
      return {};
    });
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [ProjectModelBehaviorCard],
      providers: [
        provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
        {
          provide: CoreClient,
          useValue: {
            dispatchData,
            dispatchExpect: vi.fn(async () => ({ type: 'connectionProfiles', data: { profiles: PROFILES } })),
          },
        },
      ],
    });
    const fixture = TestBed.createComponent(ProjectModelBehaviorCard);
    fixture.componentRef.setInput('project', project(over));
    fixture.componentRef.setInput('defaultOpen', true);
    fixture.detectChanges();
    await settle(fixture);
    return { fixture, dispatchData };
  }

  const engineSelect = (f: ComponentFixture<unknown>) =>
    (f.nativeElement as HTMLElement).querySelector(
      'qt-inline-brahma-profile-select select',
    ) as HTMLSelectElement;
  const toasts = () =>
    TestBed.inject(ToastService)
      .toasts()
      .map((t) => ({ type: t.type, message: t.message }));

  it('the subtitle names the inline Brahma engine', async () => {
    const { fixture } = await mountCard({});
    expect((fixture.nativeElement as HTMLElement).textContent).toContain(
      'Agent mode, roleplay template, inline Brahma & tool defaults',
    );
  });

  it("carries v4's heading and paragraph between the template block and the tool row, labelled by the heading", async () => {
    const { fixture } = await mountCard({});
    const el = fixture.nativeElement as HTMLElement;
    const heading = Array.from(el.querySelectorAll('h4')).find(
      (h) => h.textContent?.trim() === 'Inline Brahma Engine',
    )!;
    expect(heading).toBeTruthy();
    expect(heading.className).toBe('qt-label text-foreground mb-1');
    const block = heading.parentElement!;
    expect(block.className).toBe('p-3 rounded-lg qt-border qt-bg-surface');
    const para = block.querySelector('p')!;
    expect(para.className).toBe('qt-text-xs qt-text-secondary mb-2');
    expect(para.textContent?.replace(/\s+/g, ' ').trim()).toBe(
      "The connection profile Brahma consults on when summoned with @Brahma: or @Brahma? in this project's chats. Leave on inherit to use the engine named under Settings → Chat → Brahma Console.",
    );
    expect(Array.from(para.querySelectorAll('code')).map((c) => c.textContent)).toEqual(['@Brahma:', '@Brahma?']);
    // Between Default Roleplay Template and Default Tool Settings.
    expect(block.previousElementSibling!.querySelector('h4')!.textContent?.trim()).toBe('Default Roleplay Template');
    expect(block.nextElementSibling!.querySelector('h4')!.textContent?.trim()).toBe('Default Tool Settings');
    const select = engineSelect(fixture);
    expect(heading.id).not.toBe('');
    expect(select.getAttribute('aria-labelledby')).toBe(heading.id);
    expect(select.getAttribute('class')).toBe('qt-input w-full max-w-xs');
    expect(select.options[0].textContent?.trim()).toBe('Inherit from global setting');
  });

  it('two cards get different heading ids', async () => {
    const a = await mountCard({});
    const idA = (a.fixture.nativeElement as HTMLElement).querySelector('qt-inline-brahma-profile-select select')!.getAttribute('aria-labelledby');
    const b = await mountCard({});
    const idB = (b.fixture.nativeElement as HTMLElement).querySelector('qt-inline-brahma-profile-select select')!.getAttribute('aria-labelledby');
    expect(idA).not.toBe(idB);
  });

  it('shows the stored engine', async () => {
    const { fixture } = await mountCard({ brahmaConnectionProfileId: 'p1' });
    expect(engineSelect(fixture).value).toBe('p1');
  });

  it("choosing an engine PUTs only brahmaConnectionProfileId and toasts v4's sentence", async () => {
    const { fixture, dispatchData } = await mountCard({});
    const select = engineSelect(fixture);
    select.value = 'p1';
    select.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(dispatchData).toHaveBeenCalledWith({
      type: 'projectUpdate',
      projectId: 'proj1',
      project: { brahmaConnectionProfileId: 'p1' },
    });
    expect(toasts()).toEqual([{ type: 'success', message: 'Inline Brahma engine set for project' }]);
  });

  it('choosing inherit PUTs null and toasts the inherit sentence', async () => {
    const { fixture, dispatchData } = await mountCard({ brahmaConnectionProfileId: 'p1' });
    const select = engineSelect(fixture);
    select.value = '';
    select.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(dispatchData).toHaveBeenCalledWith({
      type: 'projectUpdate',
      projectId: 'proj1',
      project: { brahmaConnectionProfileId: null },
    });
    expect(toasts()).toEqual([
      { type: 'success', message: 'Inline Brahma engine set to inherit from global' },
    ]);
  });

  it('a refusal toasts the FIXED sentence, logs, and leaves the stored engine showing', async () => {
    const err = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const { fixture } = await mountCard({}, () => {
      throw new CoreDispatchError({ kind: 'validation', message: 'no such profile' });
    });
    const select = engineSelect(fixture);
    select.value = 'p1';
    select.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(toasts()).toEqual([{ type: 'error', message: 'Failed to update the inline Brahma engine' }]);
    expect(err).toHaveBeenCalledWith(
      'useProjectDetail: save inline Brahma profile error',
      'Failed to update the inline Brahma engine',
    );
    expect(select.value).toBe('');
    err.mockRestore();
  });

  it('a thrown Error toasts its own message', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const { fixture } = await mountCard({}, () => {
      throw new Error('network down');
    });
    const select = engineSelect(fixture);
    select.value = 'p1';
    select.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(toasts()).toEqual([{ type: 'error', message: 'network down' }]);
    vi.restoreAllMocks();
  });
});
