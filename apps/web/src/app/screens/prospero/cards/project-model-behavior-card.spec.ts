import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { ProjectDetail, RoleplayTemplateDto } from '../../../core/core-contract';
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
