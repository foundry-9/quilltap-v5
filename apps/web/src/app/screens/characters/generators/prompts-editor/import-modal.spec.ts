import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { CharacterPromptImportModal, type PromptTemplate } from './import-modal';

const TEMPLATES: PromptTemplate[] = [
  {
    id: 't1',
    name: 'Romantic',
    content: 'Speak tenderly.',
    description: 'A gentle voice',
    isBuiltIn: true,
    category: 'Companion',
    modelHint: 'GPT-4',
  },
  {
    id: 't2',
    name: 'My Custom Prompt',
    content: 'Custom body.',
    description: null,
    isBuiltIn: false,
    category: null,
    modelHint: null,
  },
];

async function render(templates: PromptTemplate[] = [], loading = false): Promise<ComponentFixture<CharacterPromptImportModal>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [CharacterPromptImportModal] });
  const fixture = TestBed.createComponent(CharacterPromptImportModal);
  fixture.componentRef.setInput('templates', templates);
  fixture.componentRef.setInput('loading', loading);
  fixture.detectChanges();
  return fixture;
}

describe('CharacterPromptImportModal (v4 `ImportModal.tsx`)', () => {
  it("shows v4's empty-state copy when there are no templates (today's v5 reality — no promptTemplateList verb)", async () => {
    const fixture = await render([]);
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('No templates available. Create templates in Settings > Prompts.');
  });

  it('groups Sample Prompts (built-in) separately from My Templates', async () => {
    const fixture = await render(TEMPLATES);
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('Sample Prompts');
    expect(text).toContain('My Templates');
    expect(text).toContain('Romantic');
    expect(text).toContain('My Custom Prompt');
  });

  it('clicking a template emits importPrompt with its content + name', async () => {
    const fixture = await render(TEMPLATES);
    let received: { content: string; suggestedName: string } | null = null;
    fixture.componentInstance.importPrompt.subscribe((e: { content: string; suggestedName: string }) => {
      received = e;
    });

    const button = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('button')).find(
      (b) => b.textContent?.includes('Romantic'),
    ) as HTMLButtonElement;
    button.click();

    expect(received).toEqual({ content: 'Speak tenderly.', suggestedName: 'Romantic' });
  });

  it('shows the loading state instead of the catalogue', async () => {
    const fixture = await render(TEMPLATES, true);
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('Loading templates...');
    expect(text).not.toContain('Romantic');
  });
});
