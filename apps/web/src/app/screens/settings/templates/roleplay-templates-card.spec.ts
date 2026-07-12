import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { RoleplayTemplateDto } from '../../../core/core-contract';
import { RoleplayTemplatesCard } from './roleplay-templates-card';

function tmpl(over: Partial<RoleplayTemplateDto>): RoleplayTemplateDto {
  return {
    id: 't1',
    userId: 'u1',
    name: 'My Style',
    description: null,
    systemPrompt: 'Format narration in asterisks.',
    isBuiltIn: false,
    tags: [],
    delimiters: [],
    renderingPatterns: [],
    narrationDelimiters: '*',
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    ...over,
  };
}

function stubClient(
  templates: RoleplayTemplateDto[],
  over: Partial<CoreClient> = {},
): Partial<CoreClient> {
  return {
    // fetchRoleplayTemplates reads the bare array off `dispatchData`.
    dispatchData: (async () => templates) as unknown as CoreClient['dispatchData'],
    dispatchExpect: (async () => ({
      type: 'chatSettings',
      data: { defaultRoleplayTemplateId: null },
    })) as unknown as CoreClient['dispatchExpect'],
    ...over,
  };
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<RoleplayTemplatesCard>> {
  TestBed.configureTestingModule({
    imports: [RoleplayTemplatesCard],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(RoleplayTemplatesCard);
  fixture.detectChanges();
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

function buttonLabels(fixture: ComponentFixture<RoleplayTemplatesCard>): string[] {
  return Array.from(fixture.nativeElement.querySelectorAll('button')).map((b) =>
    (b as HTMLButtonElement).textContent?.trim() ?? '',
  );
}

describe('RoleplayTemplatesCard', () => {
  it('renders built-ins read-only (Built-in badge, Preview + Copy as New, no Edit/Delete)', async () => {
    const fixture = await render(
      stubClient([tmpl({ id: 'b', name: 'Classic', isBuiltIn: true, userId: null })]),
    );
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Built-in Templates');
    expect(text).toContain('Classic');
    expect(text).toContain('Built-in');

    // Scope to the actual card (the Default Template section's <select> also
    // lists "Classic (Built-in)" as an option, so filter to cards WITH buttons).
    const builtInCard = Array.from(fixture.nativeElement.querySelectorAll('.qt-card')).find(
      (el) =>
        (el as HTMLElement).textContent?.includes('Classic') &&
        (el as HTMLElement).querySelectorAll('button').length > 0,
    ) as HTMLElement;
    const labels = Array.from(builtInCard.querySelectorAll('button')).map((b) =>
      (b as HTMLButtonElement).textContent?.trim(),
    );
    expect(labels).toContain('Preview');
    expect(labels).toContain('Copy as New');
    expect(labels).not.toContain('Edit');
    expect(labels).not.toContain('Delete');
  });

  it('renders My Templates with Edit + Delete for user templates', async () => {
    const fixture = await render(stubClient([tmpl({ id: 'u', name: 'Mine' })]));
    expect(buttonLabels(fixture)).toEqual(expect.arrayContaining(['Edit', 'Delete', 'Preview']));
  });

  it('shows the My Templates empty state when there are none', async () => {
    const fixture = await render(
      stubClient([tmpl({ id: 'b', name: 'Classic', isBuiltIn: true, userId: null })]),
    );
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('No custom templates yet');
  });

  it('deletes a user template through the inline confirm', async () => {
    const dispatchData = vi.fn(async (req: { type: string }) => {
      if (req.type === 'roleplayTemplateList') {
        return [tmpl({ id: 'u', name: 'Mine' })];
      }
      return {};
    });
    const fixture = await render(
      stubClient([], { dispatchData: dispatchData as unknown as CoreClient['dispatchData'] }),
    );

    const clickByText = (label: string): void => {
      const btn = Array.from(fixture.nativeElement.querySelectorAll('button')).find(
        (b) => (b as HTMLButtonElement).textContent?.trim() === label,
      ) as HTMLButtonElement;
      btn.click();
    };
    clickByText('Delete');
    fixture.detectChanges();
    clickByText('Confirm');
    for (let i = 0; i < 4; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    expect(dispatchData).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'roleplayTemplateDelete', templateId: 'u' }),
    );
  });
});
