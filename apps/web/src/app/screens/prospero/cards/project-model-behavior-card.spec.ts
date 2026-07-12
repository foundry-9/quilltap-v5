import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { ProjectDetail, RoleplayTemplateDto } from '../../../core/core-contract';
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
