import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { RoleplayTemplateDto } from '../../../core/core-contract';
import { RoleplayTemplatesCard } from './roleplay-templates-card';
import { TemplateFormModal } from './template-form-modal';

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

async function render(
  client: Partial<CoreClient>,
  queryClient = new QueryClient(),
): Promise<ComponentFixture<RoleplayTemplatesCard>> {
  TestBed.configureTestingModule({
    imports: [RoleplayTemplatesCard],
    providers: [provideTanStackQuery(queryClient), { provide: CoreClient, useValue: client }],
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
  return Array.from(fixture.nativeElement.querySelectorAll('button')).map(
    (b) => (b as HTMLButtonElement).textContent?.trim() ?? '',
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

  it('lists the three built-ins incl. Novel, and Copy as New seeds the form from Novel (P4.D352)', async () => {
    // The Novel bag: v4 `lib/chat/novel-template.ts:26-52` + `:94-119` at
    // 236c04e35 (`renderingPatterns: []` here — the card never reads them).
    const novel = tmpl({
      id: 'n',
      name: 'Novel',
      isBuiltIn: true,
      userId: null,
      narrationDelimiters: null,
      unmarkedStyle: {
        style: 'qt-roleplay-plain',
        addOns: { bold: false, italic: true, reverse: false, underline: 'none', border: 'none', font: '' },
      },
      markdownAsterisks: false,
      delimiters: [
        {
          kind: 'wrap',
          name: 'Dialogue',
          buttonName: 'Dia',
          delimiters: '"',
          style: 'qt-chat-dialogue',
          matchCurlyQuotes: true,
          closeOptional: true,
        },
      ],
    });
    const fixture = await render(
      stubClient([
        tmpl({ id: 'a', name: 'Standard', isBuiltIn: true, userId: null }),
        tmpl({ id: 'b', name: 'Quilltap RP', isBuiltIn: true, userId: null }),
        novel,
      ]),
    );
    const cards = Array.from(fixture.nativeElement.querySelectorAll('.qt-card')).filter((el) =>
      Array.from((el as HTMLElement).querySelectorAll('button')).some(
        (b) => b.textContent?.trim() === 'Copy as New',
      ),
    ) as HTMLElement[];
    expect(cards).toHaveLength(3);
    const copy = Array.from(cards[2].querySelectorAll('button')).find(
      (b) => b.textContent?.trim() === 'Copy as New',
    ) as HTMLButtonElement;
    copy.click();
    for (let i = 0; i < 4; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    const modal = fixture.debugElement.query((d) => d.componentInstance instanceof TemplateFormModal);
    const root = modal.nativeElement as HTMLElement;
    const checkedRadio = root.querySelector<HTMLInputElement>('input[name="narrationMode"]:checked')!;
    expect(checkedRadio.closest('label')!.textContent!.trim()).toBe('None — plain prose');
    const boxText = (t: string) =>
      (Array.from(root.querySelectorAll('label'))
        .find((l) => (l.textContent ?? '').replace(/\s+/g, ' ').trim().startsWith(t))!
        .querySelector('input[type="checkbox"]') as HTMLInputElement).checked;
    expect(boxText('Style unmarked text')).toBe(true);
    expect(boxText('Markdown reads asterisks')).toBe(false);
    expect(boxText('Straight quotes also match curly ones')).toBe(true);
    expect(boxText('May run unclosed')).toBe(true);
    expect(root.querySelector<HTMLInputElement>('input[placeholder="My Custom RP Style"]')!.value).toBe(
      'Novel (Copy)',
    );
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

  /**
   * v4 `3b8b6f42b` (`useRoleplayTemplates.ts:247-249`): "The picker reads the
   * shared settings row, so the saved value has to land in the cache or the
   * selection snaps back until the next refetch." v5 split that row across two
   * spellings (`['chat-settings']` here, `['chatSettings']` on the Chat tab);
   * ruling R-8 converges the card onto `chatSettingsKeys.all` and seeds it from
   * the PUT's returned row.
   */
  it('a default-template change seeds [chatSettings] with the PUT’s returned row', async () => {
    let row: Record<string, unknown> = { defaultRoleplayTemplateId: null, marker: 'get' };
    const dispatchExpect = vi.fn(
      async (req: { type: string; settings?: Record<string, unknown> }) => {
        if (req.type === 'chatSettingsUpdate') {
          row = { ...row, ...(req.settings ?? {}), marker: 'put' };
        }
        return { type: 'chatSettings', data: row };
      },
    );
    const dispatchData = vi.fn(
      async (req: { type: string; settings?: Record<string, unknown> }) => {
        if (req.type === 'chatSettingsUpdate') {
          row = { ...row, ...(req.settings ?? {}), marker: 'put' };
          return row;
        }
        return [tmpl({ id: 'u', name: 'Mine' })];
      },
    );
    const queryClient = new QueryClient();
    const fixture = await render(
      stubClient([], {
        dispatchExpect: dispatchExpect as unknown as CoreClient['dispatchExpect'],
        dispatchData: dispatchData as unknown as CoreClient['dispatchData'],
      }),
      queryClient,
    );
    const select = fixture.nativeElement.querySelector('select') as HTMLSelectElement;
    select.value = 'u';
    select.dispatchEvent(new Event('change'));
    for (let i = 0; i < 6; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    expect(queryClient.getQueryData(['chatSettings'])).toEqual({
      defaultRoleplayTemplateId: 'u',
      marker: 'put',
    });
  });

  it('no query lives under the retired chat-settings spelling after mount', async () => {
    const queryClient = new QueryClient();
    await render(stubClient([]), queryClient);
    expect(queryClient.getQueryCache().find({ queryKey: ['chat-settings'] })).toBeUndefined();
    expect(queryClient.getQueryCache().find({ queryKey: ['chatSettings'] })).toBeDefined();
  });
});
