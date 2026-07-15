import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { CharacterSystemPrompt } from '../../../core/core-contract';
import { RichEditor } from '../../../editor/rich-editor';
import { CharacterSystemPromptsTab } from './system-prompts-tab';

function prompt(over: Partial<CharacterSystemPrompt> = {}): CharacterSystemPrompt {
  return {
    id: 'p1',
    name: 'Romantic',
    content: 'Speak tenderly.',
    isDefault: false,
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    ...over,
  };
}

function stubClient(
  prompts: CharacterSystemPrompt[],
  onDispatch?: (req: { type: string; [k: string]: unknown }) => void,
): Partial<CoreClient> {
  return {
    dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
      onDispatch?.(req);
      if (req.type === 'characterPromptList') {
        return { prompts };
      }
      return {};
    }) as CoreClient['dispatchData'],
  };
}

async function render(
  client: Partial<CoreClient>,
): Promise<ComponentFixture<CharacterSystemPromptsTab>> {
  TestBed.configureTestingModule({
    imports: [CharacterSystemPromptsTab],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(CharacterSystemPromptsTab);
  fixture.componentRef.setInput('characterId', 'char-1');
  fixture.componentRef.setInput('characterName', 'Bertie');
  fixture.detectChanges();
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

/** The prompt modal's content editor (qt-markdown-field's RichEditor handle). */
function contentEditor(fixture: ComponentFixture<unknown>): RichEditor {
  return fixture.debugElement.query(By.directive(RichEditor)).componentInstance as RichEditor;
}

function clickButtonWithText(fixture: ComponentFixture<unknown>, text: string): void {
  const button = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('button')).find(
    (b) => (b as HTMLButtonElement).textContent?.trim() === text,
  ) as HTMLButtonElement;
  button.click();
  fixture.detectChanges();
}

describe('CharacterSystemPromptsTab', () => {
  it('shows the empty state and disables Import Template', async () => {
    const fixture = await render(stubClient([]));
    expect(fixture.nativeElement.textContent).toContain('No system prompts yet');
    const importButton = Array.from(fixture.nativeElement.querySelectorAll('button')).find(
      (b) => (b as HTMLButtonElement).textContent?.trim() === 'Import Template',
    ) as HTMLButtonElement;
    expect(importButton.disabled).toBe(true);
  });

  it('creating a prompt dispatches characterPromptCreate', async () => {
    const seen: Array<{ type: string; [k: string]: unknown }> = [];
    const fixture = await render(stubClient([], (req) => seen.push(req)));

    clickButtonWithText(fixture, '+ Add Prompt');
    await settle(fixture);
    const nameInput = fixture.nativeElement.querySelector(
      'input[placeholder="e.g., Romantic, Companion, Professional"]',
    ) as HTMLInputElement;
    nameInput.value = 'Romantic';
    nameInput.dispatchEvent(new Event('input'));
    contentEditor(fixture).setMarkdown('Speak tenderly.');
    await settle(fixture);

    clickButtonWithText(fixture, 'Create');
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();

    const createCall = seen.find((r) => r.type === 'characterPromptCreate');
    expect(createCall).toBeTruthy();
    expect(createCall!['characterId']).toBe('char-1');
    expect(createCall!['name']).toBe('Romantic');
    expect(createCall!['content']).toBe('Speak tenderly.');
  });

  it('editing a prompt dispatches characterPromptUpdate', async () => {
    const seen: Array<{ type: string; [k: string]: unknown }> = [];
    const fixture = await render(stubClient([prompt()], (req) => seen.push(req)));

    const editButton = fixture.nativeElement.querySelector('[title="Edit"]') as HTMLButtonElement;
    editButton.click();
    fixture.detectChanges();
    await settle(fixture);

    clickButtonWithText(fixture, 'Update');
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();

    const updateCall = seen.find((r) => r.type === 'characterPromptUpdate');
    expect(updateCall).toBeTruthy();
    expect(updateCall!['promptId']).toBe('p1');
  });

  it('the edit modal seeds the markdown field and re-saves it byte-identical', async () => {
    const seen: Array<{ type: string; [k: string]: unknown }> = [];
    const fixture = await render(
      stubClient([prompt({ content: 'Speak *tenderly*, and never of the rain.' })], (req) =>
        seen.push(req),
      ),
    );

    const editButton = fixture.nativeElement.querySelector('[title="Edit"]') as HTMLButtonElement;
    editButton.click();
    fixture.detectChanges();
    await settle(fixture);

    expect(contentEditor(fixture).getMarkdown()).toBe('Speak *tenderly*, and never of the rain.');

    // Save without touching the field: the stored bytes must round-trip
    // untouched (the absorb-once seam means the mount is not an edit).
    clickButtonWithText(fixture, 'Update');
    await settle(fixture);

    const updateCall = seen.find((r) => r.type === 'characterPromptUpdate');
    expect(updateCall!['content']).toBe('Speak *tenderly*, and never of the rain.');
  });

  it('an edit made in the markdown field reaches the update payload', async () => {
    const seen: Array<{ type: string; [k: string]: unknown }> = [];
    const fixture = await render(stubClient([prompt()], (req) => seen.push(req)));

    const editButton = fixture.nativeElement.querySelector('[title="Edit"]') as HTMLButtonElement;
    editButton.click();
    fixture.detectChanges();
    await settle(fixture);

    contentEditor(fixture).setMarkdown('Speak **plainly**.');
    await settle(fixture);
    clickButtonWithText(fixture, 'Update');
    await settle(fixture);

    const updateCall = seen.find((r) => r.type === 'characterPromptUpdate');
    expect(updateCall!['content']).toBe('Speak **plainly**.');
  });

  it('the star button dispatches characterPromptSetDefault', async () => {
    const seen: Array<{ type: string; [k: string]: unknown }> = [];
    const fixture = await render(stubClient([prompt()], (req) => seen.push(req)));

    const starButton = fixture.nativeElement.querySelector(
      '[title="Set as default"]',
    ) as HTMLButtonElement;
    starButton.click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();

    const setDefaultCall = seen.find((r) => r.type === 'characterPromptSetDefault');
    expect(setDefaultCall).toBeTruthy();
    expect(setDefaultCall!['promptId']).toBe('p1');
  });

  it('the trash button dispatches characterPromptDelete', async () => {
    const seen: Array<{ type: string; [k: string]: unknown }> = [];
    const fixture = await render(stubClient([prompt()], (req) => seen.push(req)));

    const deleteButton = fixture.nativeElement.querySelector(
      '[title="Delete"]',
    ) as HTMLButtonElement;
    deleteButton.click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();

    const deleteCall = seen.find((r) => r.type === 'characterPromptDelete');
    expect(deleteCall).toBeTruthy();
    expect(deleteCall!['promptId']).toBe('p1');
  });
});
