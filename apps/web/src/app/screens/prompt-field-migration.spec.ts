import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { ProjectDetail } from '../core/core-contract';
import { PROMPT_FIELD_HINTS } from '../ui/prompt-field-hints';
import { CharacterAppearanceTab } from './characters/edit/appearance-tab';
import { INITIAL_CHARACTER_FORM_DATA } from './characters/edit/character-form';
import { CharacterDetailsTab } from './characters/edit/details-tab';
import { PromptModal } from './characters/edit/prompt-modal';
import { NewCharacter } from './characters/new/new-character';
import type { ProjectEditForm } from './prospero/cards/project-header';
import { ProjectSettingsCard } from './prospero/cards/project-settings-card';
import { TemplateFormModal } from './settings/templates/template-form-modal';

/**
 * The `a6870c5a` migration sweep, pinned where it matters: every prompt-bearing
 * field's rendered header now comes from `PROMPT_FIELD_HINTS`, and every
 * hand-rolled copy of that copy is GONE.
 *
 * Two things are asserted, over the real rendered DOM of each migrated surface
 * rather than over the source text:
 *
 * 1. **Convergence** — the character CREATE and EDIT forms render byte-identical
 *    headers for the seven fields they share. That divergence is the stated
 *    reason v4 wrote `PromptFieldLabel`: the two forms had drifted apart in
 *    their duplicated helper text, and v5 had transcribed both drifted copies.
 * 2. **No survivors** — none of the retired hand-rolled sentences appears
 *    anywhere in the rendered output. A migration that left one behind (or
 *    shadowed rather than deleted it) reds here.
 */

interface RenderedLabel {
  label: string;
  helper: string | undefined;
  example: string | undefined;
}

const WRITTEN_AS = 'Written as: ';

function labels(fixture: ComponentFixture<unknown>): RenderedLabel[] {
  const hosts = Array.from(
    (fixture.nativeElement as HTMLElement).querySelectorAll('qt-prompt-field-label'),
  );
  return hosts.map((host) => {
    const paragraphs = Array.from(host.querySelectorAll('p')).map((p) => p.textContent ?? '');
    const written = paragraphs.find((t) => t.startsWith(WRITTEN_AS));
    return {
      label: host.querySelector('label')?.textContent ?? '',
      helper: paragraphs.find((t) => !t.startsWith(WRITTEN_AS)),
      example: written?.slice(WRITTEN_AS.length),
    };
  });
}

function byLabel(fixture: ComponentFixture<unknown>): Map<string, RenderedLabel> {
  return new Map(labels(fixture).map((l) => [l.label, l]));
}

/** Every hand-rolled sentence the sweep retired, in the exact rendered form. */
const RETIRED_COPY = [
  // identity / description / manifesto / personality (edit AND create)
  'occupation, public reputation. The shallow first impression.',
  'Not physical appearance (that lives in physical descriptions).',
  'anchor everything else. What this character is, at root.',
  'What the character knows about themselves — inner drivers of speech and behaviour, motivations, beliefs.',
  // first message / example dialogues / system prompt
  'The character’s opening message to start conversations.',
  'Example conversations to guide the AI’s responses.',
  'Custom system instructions (will be combined with auto-generated prompt).',
  // the create form's singular scenario
  'Describe the setting and context for conversations.',
  // the system-prompt modal's Content field
  'Supports Markdown formatting.',
  // the roleplay-template modal's LLM Prompt
  'The formatting instructions prepended to the character',
  // the project Settings card
  'These instructions are included in system prompts for all project chats.',
];

function expectNoRetiredCopy(fixture: ComponentFixture<unknown>): void {
  const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
  expect(RETIRED_COPY.filter((s) => text.includes(s))).toEqual([]);
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

// --- the character EDIT details tab -----------------------------------------

@Component({
  imports: [CharacterDetailsTab],
  template: `<qt-character-details-tab [form]="form()" />`,
})
class DetailsHost {
  readonly form = signal(INITIAL_CHARACTER_FORM_DATA);
}

async function renderDetails(): Promise<ComponentFixture<DetailsHost>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [DetailsHost],
    providers: [
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: { dispatchData: async () => ({}) } },
    ],
  });
  const fixture = TestBed.createComponent(DetailsHost);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

// --- the character CREATE page ----------------------------------------------

async function renderCreate(): Promise<ComponentFixture<NewCharacter>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [NewCharacter],
    providers: [
      provideRouter([{ path: '**', component: NewCharacter }]),
      provideTanStackQuery(new QueryClient()),
      {
        provide: CoreClient,
        useValue: { dispatchData: async () => ({ profiles: [] }) },
      },
    ],
  });
  const fixture = TestBed.createComponent(NewCharacter);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

describe('the a6870c5a prompt-field migration', () => {
  it('the edit form draws all seven prompt headers from the hints table', async () => {
    const fixture = await renderDetails();
    const seen = byLabel(fixture);

    for (const key of [
      'identity',
      'description',
      'manifesto',
      'personality',
      'firstMessage',
      'exampleDialogues',
      'systemPrompt',
    ] as const) {
      const hint = PROMPT_FIELD_HINTS[key];
      const rendered = seen.get(`${hint.label} (Optional)`);
      expect(rendered, `${key} header missing from the edit form`).toBeDefined();
      expect(rendered?.helper).toBe(hint.helper);
      expect(rendered?.example).toBe((hint as { example?: string }).example);
    }
    expectNoRetiredCopy(fixture);
  });

  it('the create form draws all eight prompt headers from the hints table', async () => {
    const fixture = await renderCreate();
    const seen = byLabel(fixture);

    for (const key of [
      'identity',
      'description',
      'manifesto',
      'personality',
      'scenario',
      'firstMessage',
      'exampleDialogues',
      'systemPrompt',
    ] as const) {
      const hint = PROMPT_FIELD_HINTS[key];
      const rendered = seen.get(`${hint.label} (Optional)`);
      expect(rendered, `${key} header missing from the create form`).toBeDefined();
      expect(rendered?.helper).toBe(hint.helper);
      expect(rendered?.example).toBe((hint as { example?: string }).example);
    }
    expectNoRetiredCopy(fixture);
  });

  it('CONVERGENCE: create and edit render identical headers for their shared fields', async () => {
    const edit = byLabel(await renderDetails());
    const create = byLabel(await renderCreate());

    for (const key of [
      'identity',
      'description',
      'manifesto',
      'personality',
      'firstMessage',
      'exampleDialogues',
      'systemPrompt',
    ] as const) {
      const label = `${PROMPT_FIELD_HINTS[key].label} (Optional)`;
      expect(create.get(label), `${key} on create`).toEqual(edit.get(label));
    }
  });

  it('the create form keeps its Import Template control in the label row', async () => {
    const fixture = await renderCreate();
    const host = (fixture.nativeElement as HTMLElement).querySelector(
      'qt-prompt-field-label:has(button)',
    );
    expect(host?.querySelector('label')?.textContent).toBe('System Prompt (Optional)');
    const button = host?.querySelector('button') as HTMLButtonElement;
    expect(button.textContent?.trim()).toBe('Import Template');
    expect(button.disabled).toBe(true);
  });

  it('the system-prompt modal labels Content, required, with the Markdown suffix', async () => {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({ imports: [PromptModal] });
    const fixture = TestBed.createComponent(PromptModal);
    fixture.detectChanges();
    await settle(fixture);

    const [content] = labels(fixture);
    expect(content.label).toBe('Content *');
    expect(content.helper).toBe(
      `${PROMPT_FIELD_HINTS.systemPrompt.helper} Markdown is supported, and {{char}} / {{user}} substitute the character and user names.`,
    );
    expect(content.example).toBe(PROMPT_FIELD_HINTS.systemPrompt.example);
    expectNoRetiredCopy(fixture);
  });

  it('the roleplay-template modal labels LLM Prompt, required, with the placeholder suffix', async () => {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [TemplateFormModal],
      providers: [{ provide: CoreClient, useValue: { dispatchData: async () => ({}) } }],
    });
    const fixture = TestBed.createComponent(TemplateFormModal);
    fixture.detectChanges();
    await settle(fixture);

    const [prompt] = labels(fixture);
    expect(prompt.label).toBe('LLM Prompt *');
    expect(prompt.helper).toBe(
      `${PROMPT_FIELD_HINTS.roleplayTemplatePrompt.helper} You can use placeholders like {{char}} and {{user}}.`,
    );
    expect(prompt.example).toBe(PROMPT_FIELD_HINTS.roleplayTemplatePrompt.example);
    expectNoRetiredCopy(fixture);
  });

  it('the project Settings card labels Project Instructions with NO "(Optional)"', async () => {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [ProjectSettingsCard],
      providers: [{ provide: CoreClient, useValue: { dispatchData: async () => ({}) } }],
    });
    const fixture = TestBed.createComponent(ProjectSettingsCard);
    fixture.componentRef.setInput('project', {
      id: 'p1',
      name: 'The Estate',
      instructions: '',
    } as ProjectDetail);
    fixture.componentRef.setInput('form', { instructions: '' } as ProjectEditForm);
    fixture.componentRef.setInput('defaultOpen', true);
    fixture.detectChanges();
    await settle(fixture);

    const [instructions] = labels(fixture);
    expect(instructions.label).toBe('Project Instructions');
    expect(instructions.helper).toBe(PROMPT_FIELD_HINTS.projectInstructions.helper);
    expect(instructions.example).toBe(PROMPT_FIELD_HINTS.projectInstructions.example);
    expectNoRetiredCopy(fixture);
  });

  it('the appearance tab carries ONE physicalDescription note above the five variants', async () => {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [CharacterAppearanceTab],
      providers: [
        provideTanStackQuery(new QueryClient()),
        {
          provide: CoreClient,
          useValue: {
            dispatchData: async (req: { type: string }) =>
              req.type === 'characterGet'
                ? { character: { id: 'c1', characterDocumentMountPointId: 'mp1' } }
                : {},
          },
        },
      ],
    });
    const fixture = TestBed.createComponent(CharacterAppearanceTab);
    fixture.componentRef.setInput('characterId', 'c1');
    fixture.detectChanges();
    await settle(fixture);

    const rendered = labels(fixture);
    expect(rendered).toHaveLength(1);
    expect(rendered[0].label).toBe('Physical Description');
    expect(rendered[0].helper).toBe(PROMPT_FIELD_HINTS.physicalDescription.helper);
    expect(rendered[0].example).toBe(PROMPT_FIELD_HINTS.physicalDescription.example);
  });

  it('the scenarios block keeps its custom header, folds in the stage clause, and shows the example', async () => {
    const fixture = await renderDetails();
    const block = (fixture.nativeElement as HTMLElement).querySelector(
      'qt-scenario-editor',
    ) as HTMLElement;

    // NOT a `qt-prompt-field-label` — v4 deliberately left this one hand-rolled.
    expect(block.querySelector('qt-prompt-field-label')).toBeNull();
    // The helper is literal markup wrapped across source lines, so compare on
    // the collapsed text the browser actually shows.
    const paragraphs = Array.from(block.querySelectorAll('p')).map((p) =>
      (p.textContent ?? '').replace(/\s+/g, ' ').trim(),
    );
    expect(paragraphs[0]).toBe(
      'Named settings and contexts for conversations — the stage, never the actor. Each scenario can be selected when starting a chat. Stored in the vault’s Scenarios/ folder.',
    );
    // P4.D121 slotted v4's archiving note between the helper and the example
    // (v4 `CharacterBasicInfo.tsx:409-412`).
    expect(paragraphs[1]).toBe(
      'Archiving a scenario keeps it here but hides it from the chat pickers unless “Show archived” is ticked there. Chats already using it are unaffected.',
    );
    expect(paragraphs[2]).toBe(`${WRITTEN_AS}${PROMPT_FIELD_HINTS.scenario.example}`);
  });
});
