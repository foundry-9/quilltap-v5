import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { RoleplayTemplateDto } from '../../../core/core-contract';
import { RichEditor } from '../../../editor/rich-editor';
import { TemplateFormModal } from './template-form-modal';

function tmpl(over: Partial<RoleplayTemplateDto> = {}): RoleplayTemplateDto {
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

type Req = { type: string; [k: string]: unknown };

function stubClient(seen: Req[]): Partial<CoreClient> {
  return {
    dispatchData: (async (req: Req) => {
      seen.push(req);
      return { template: tmpl() };
    }) as unknown as CoreClient['dispatchData'],
  };
}

async function render(
  seen: Req[],
  template: RoleplayTemplateDto | null = null,
): Promise<ComponentFixture<TemplateFormModal>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [TemplateFormModal],
    providers: [{ provide: CoreClient, useValue: stubClient(seen) }],
  });
  const fixture = TestBed.createComponent(TemplateFormModal);
  fixture.componentRef.setInput('template', template);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

/** The LLM-prompt editor (qt-markdown-field's RichEditor handle). */
function promptEditor(fixture: ComponentFixture<unknown>): RichEditor {
  return fixture.debugElement.query(By.directive(RichEditor)).componentInstance as RichEditor;
}

function setInput(fixture: ComponentFixture<unknown>, selector: string, value: string): void {
  const input = (fixture.nativeElement as HTMLElement).querySelector(selector) as HTMLInputElement;
  input.value = value;
  input.dispatchEvent(new Event('input'));
}

function clickButtonWithText(fixture: ComponentFixture<unknown>, text: string): void {
  const button = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('button')).find(
    (b) => (b as HTMLButtonElement).textContent?.trim() === text,
  ) as HTMLButtonElement;
  button.click();
  fixture.detectChanges();
}

describe('TemplateFormModal — the LLM-prompt markdown field', () => {
  it('seeds the field from the edited template (v4 remountKey = the template id)', async () => {
    const fixture = await render([], tmpl({ systemPrompt: 'Narrate in *asterisks*.' }));
    expect(promptEditor(fixture).getMarkdown()).toBe('Narrate in *asterisks*.');
  });

  it('carries an edited prompt into the roleplayTemplateUpdate payload', async () => {
    const seen: Req[] = [];
    const fixture = await render(seen, tmpl());

    promptEditor(fixture).setMarkdown('Narrate in **bold**, never in plain.');
    await settle(fixture);
    clickButtonWithText(fixture, 'Save Changes');
    await settle(fixture);

    const update = seen.find((r) => r.type === 'roleplayTemplateUpdate');
    expect(update).toBeTruthy();
    expect(update!['templateId']).toBe('t1');
    expect((update!['template'] as Record<string, unknown>)['systemPrompt']).toBe(
      'Narrate in **bold**, never in plain.',
    );
  });

  it('re-saves an untouched prompt byte-identical', async () => {
    // No user edit: the stored prompt must round-trip through the editor
    // unchanged (the absorb-once seam means the mount is not an edit).
    const systemPrompt = 'Narrate in *asterisks*.\n\nUse {{char}} and **never** {{user}}.';
    const seen: Req[] = [];
    const fixture = await render(seen, tmpl({ systemPrompt }));

    clickButtonWithText(fixture, 'Save Changes');
    await settle(fixture);

    const update = seen.find((r) => r.type === 'roleplayTemplateUpdate');
    expect((update!['template'] as Record<string, unknown>)['systemPrompt']).toBe(systemPrompt);
  });

  it('carries the prompt into a create payload', async () => {
    const seen: Req[] = [];
    const fixture = await render(seen, null);

    setInput(fixture, 'input[placeholder="My Custom RP Style"]', 'Terse');
    promptEditor(fixture).setMarkdown('Be brief.');
    await settle(fixture);
    clickButtonWithText(fixture, 'Create Template');
    await settle(fixture);

    const create = seen.find((r) => r.type === 'roleplayTemplateCreate');
    expect(create).toBeTruthy();
    expect((create!['template'] as Record<string, unknown>)['systemPrompt']).toBe('Be brief.');
  });
});
