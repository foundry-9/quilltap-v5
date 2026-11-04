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

// --- P4.D352: the Novel template's SPA half (v4 778b97b2b at 236c04e35) ------

function radios(fixture: ComponentFixture<unknown>): HTMLInputElement[] {
  return Array.from(
    (fixture.nativeElement as HTMLElement).querySelectorAll<HTMLInputElement>(
      'input[type="radio"][name="narrationMode"]',
    ),
  );
}

function checkboxByText(fixture: ComponentFixture<unknown>, text: string): HTMLInputElement {
  const label = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('label')).find(
    (l) => (l.textContent ?? '').replace(/\s+/g, ' ').trim().startsWith(text),
  );
  if (!label) throw new Error(`no label starting "${text}"`);
  return label.querySelector('input[type="checkbox"]') as HTMLInputElement;
}

describe('TemplateFormModal — unmarked prose (P4.D352)', () => {
  it('offers a third narration radio, None — plain prose, which turns the Unmarked Text style on', async () => {
    const fixture = await render([], null);
    const root = fixture.nativeElement as HTMLElement;
    expect(radios(fixture)).toHaveLength(3);
    expect(root.textContent).toContain(
      'How narration and action text is marked in this template’s formatting — or, as in a proper novel, not marked at all.'.replace('’', "'"),
    );
    expect(root.querySelector('input[list="qt-style-options-unmarked"]')).toBeNull();

    const none = radios(fixture)[2];
    expect(none.closest('label')!.textContent!.trim()).toBe('None — plain prose');
    none.click();
    await settle(fixture);

    expect(root.textContent).toContain('Narration carries no marks; style it under Unmarked Text below.');
    expect(root.querySelector('input[placeholder="*"]')).toBeNull();
    expect(checkboxByText(fixture, 'Style unmarked text').checked).toBe(true);
    const style = root.querySelector<HTMLInputElement>('input[list="qt-style-options-unmarked"]')!;
    expect(style.value).toBe('qt-roleplay-plain');
    // R4: the style field's visible label is paired by id.
    expect(root.querySelector(`label[for="${style.id}"]`)!.textContent!.trim()).toBe(
      'Style (CSS class)',
    );
    expect(root.querySelectorAll('#qt-style-options-unmarked option')[0].textContent).toBe(
      'Plain (no chip — dress it with flourishes)',
    );
  });

  it('saves narrationDelimiters: null, the unmarked style with italic, and the asterisks flag', async () => {
    const seen: Req[] = [];
    const fixture = await render(seen, null);
    setInput(fixture, 'input[placeholder="My Custom RP Style"]', 'Prose');
    promptEditor(fixture).setMarkdown('Write prose.');
    await settle(fixture);
    radios(fixture)[2].click();
    await settle(fixture);
    // Italic in the unmarked flourishes, asterisks off.
    const italic = checkboxByText(fixture, 'Italic');
    italic.click();
    await settle(fixture);
    checkboxByText(fixture, 'Markdown reads asterisks').click();
    await settle(fixture);
    clickButtonWithText(fixture, 'Create Template');
    await settle(fixture);

    const create = seen.find((r) => r.type === 'roleplayTemplateCreate');
    const body = create!['template'] as Record<string, unknown>;
    expect(body['narrationDelimiters']).toBeNull();
    expect(body['unmarkedStyle']).toEqual({
      style: 'qt-roleplay-plain',
      addOns: { bold: false, italic: true, reverse: false, underline: 'none', border: 'none', font: '' },
    });
    expect(body['markdownAsterisks']).toBe(false);
  });

  it('seeds every control from a Novel-shaped template', async () => {
    const fixture = await render(
      [],
      tmpl({
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
      }),
    );
    expect(radios(fixture)[2].checked).toBe(true);
    expect(checkboxByText(fixture, 'Style unmarked text').checked).toBe(true);
    expect(checkboxByText(fixture, 'Markdown reads asterisks').checked).toBe(false);
    expect(checkboxByText(fixture, 'Straight quotes also match curly ones').checked).toBe(true);
    expect(checkboxByText(fixture, 'May run unclosed to the end of a paragraph').checked).toBe(true);
    // The unmarked flourishes come first in the form; their Italic is ticked.
    expect(checkboxByText(fixture, 'Italic').checked).toBe(true);
  });

  it('allows Save in none mode with empty narration inputs (v4 :868)', async () => {
    const fixture = await render([], tmpl({ narrationDelimiters: null }));
    const save = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('button')).find(
      (b) => b.textContent?.trim() === 'Save Changes',
    ) as HTMLButtonElement;
    expect(save.disabled).toBe(false);
  });
});
