import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { Router, provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { EMPTY } from 'rxjs';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { RichEditor } from '../../../editor/rich-editor';
import {
  WORKSPACE_HANDLE,
  WORKSPACE_TAB_ID,
  type WorkspaceHandle,
} from '../../../workspace/workspace-contract';
import { NewCharacter, buildCreateCharacterBag } from './new-character';

function stubClient(
  onDispatch?: (req: { type: string; [k: string]: unknown }) => void,
): Partial<CoreClient> {
  return {
    events$: EMPTY,
    dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
      onDispatch?.(req);
      switch (req.type) {
        case 'connectionProfileList':
          return {
            profiles: [{ id: 'p1', name: 'GPT-4', provider: 'OPENAI', modelName: 'gpt-4' }],
          };
        case 'characterCreate':
          return { character: { id: 'new-char-id' } };
        case 'characterUpdate':
          return { character: { id: 'new-char-id' } };
        case 'characterWardrobeCreate':
          return { wardrobeItem: { id: `item-${Math.random()}` } };
        case 'characterScenarioCreate':
          return {
            scenario: {
              id: `sc-${Math.random()}`,
              title: req['title'],
              content: req['content'],
              createdAt: '2026-01-01T00:00:00Z',
              updatedAt: '2026-01-01T00:00:00Z',
            },
          };
        default:
          return {};
      }
    }) as CoreClient['dispatchData'],
  };
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<NewCharacter>> {
  TestBed.configureTestingModule({
    imports: [NewCharacter],
    providers: [
      // A real (if trivial) route so `router.navigate` resolves instead of
      // rejecting with NG04002 (no match).
      provideRouter([{ path: '**', component: NewCharacter }]),
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: client },
    ],
  });
  const fixture = TestBed.createComponent(NewCharacter);
  fixture.detectChanges();
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

function setInput(fixture: ComponentFixture<NewCharacter>, id: string, value: string): void {
  const el = (fixture.nativeElement as HTMLElement).querySelector<
    HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement
  >(`#${id}`)!;
  el.value = value;
  el.dispatchEvent(new Event(el.tagName === 'SELECT' ? 'change' : 'input'));
  fixture.detectChanges();
}

/** The markdown fields render in order; index 0 is Identity (qt-markdown-field). */
function setMarkdownField(fixture: ComponentFixture<NewCharacter>, index: number, value: string): void {
  const editors = fixture.debugElement.queryAll(By.directive(RichEditor));
  (editors[index].componentInstance as RichEditor).setMarkdown(value);
  fixture.detectChanges();
}

describe('buildCreateCharacterBag', () => {
  it('drops empty optional UUID / URL fields to undefined', () => {
    const bag = buildCreateCharacterBag({
      name: 'Bertie',
      title: '',
      identity: '',
      description: '',
      manifesto: '',
      personality: '',
      scenario: '',
      firstMessage: '',
      exampleDialogues: '',
      systemPrompt: '',
      avatarUrl: '',
      defaultConnectionProfileId: '',
    });
    expect(bag['avatarUrl']).toBeUndefined();
    expect(bag['defaultConnectionProfileId']).toBeUndefined();
    expect(bag['name']).toBe('Bertie');
  });

  it('keeps populated optional fields', () => {
    const bag = buildCreateCharacterBag({
      name: 'Bertie',
      title: '',
      identity: '',
      description: '',
      manifesto: '',
      personality: '',
      scenario: '',
      firstMessage: '',
      exampleDialogues: '',
      systemPrompt: '',
      avatarUrl: 'https://example.com/a.png',
      defaultConnectionProfileId: 'profile-1',
    });
    expect(bag['avatarUrl']).toBe('https://example.com/a.png');
    expect(bag['defaultConnectionProfileId']).toBe('profile-1');
  });
});

describe('NewCharacter', () => {
  it('gives every prose field v4’s own minHeight', async () => {
    // v4 NewCharacterView.tsx:310/327/344/361/378/395/412/438. The eight fields
    // do NOT share a height — matched by aria-label, never by position, since
    // v4's own values are per-field. The mechanism is proven in
    // markdown-field.spec; this pins the VALUES.
    const fixture = await render(stubClient());
    const expected: Record<string, string> = {
      Identity: '6rem',
      Description: '8rem',
      Manifesto: '8rem',
      Personality: '8rem',
      Scenario: '8rem',
      'First Message': '6rem',
      'Example Dialogues': '12rem',
      'System Prompt': '8rem',
    };
    for (const [label, minHeight] of Object.entries(expected)) {
      // aria-label lands on ProseMirror's content div; the height is bound on
      // the qt-rich-editor element wrapping it.
      const content = fixture.nativeElement.querySelector(`[aria-label="${label}"]`) as HTMLElement;
      expect(content, label).not.toBeNull();
      const editor = content.closest('qt-rich-editor') as HTMLElement;
      expect(editor.style.minHeight, label).toBe(minHeight);
    }
  });

  it('renders the v4-verbatim header with a live AI Wizard affordance (P4.9K3)', async () => {
    const fixture = await render(stubClient());
    expect(fixture.nativeElement.querySelector('h1').textContent).toContain('Create Character');
    const wizard = Array.from(fixture.nativeElement.querySelectorAll('button')).find((b) =>
      (b as HTMLButtonElement).textContent?.includes('AI Wizard'),
    ) as HTMLButtonElement;
    expect(wizard.disabled).toBe(false);
  });

  it('submitting dispatches characterCreate with the expected bag and navigates', async () => {
    const seen: Array<{ type: string; [k: string]: unknown }> = [];
    const fixture = await render(stubClient((req) => seen.push(req)));
    const router = TestBed.inject(Router);
    const navigateSpy = vi.spyOn(router, 'navigate');

    setInput(fixture, 'name', 'Bertie Wooster');
    setInput(fixture, 'title', 'The Valet');
    setMarkdownField(fixture, 0, 'A gentleman about town'); // Identity
    setInput(fixture, 'avatarUrl', '');
    setInput(fixture, 'defaultConnectionProfileId', '');
    await new Promise((r) => setTimeout(r, 0)); // flush the field's contentChange
    fixture.detectChanges();

    const form = fixture.nativeElement.querySelector('form') as HTMLFormElement;
    form.dispatchEvent(new Event('submit', { cancelable: true }));
    await fixture.whenStable();
    await new Promise((r) => setTimeout(r, 0));

    const createCall = seen.find((r) => r.type === 'characterCreate');
    expect(createCall).toBeTruthy();
    const bag = createCall!['character'] as Record<string, unknown>;
    expect(bag['name']).toBe('Bertie Wooster');
    expect(bag['title']).toBe('The Valet');
    expect(bag['identity']).toBe('A gentleman about town');
    expect(bag['avatarUrl']).toBeUndefined();
    expect(bag['defaultConnectionProfileId']).toBeUndefined();

    expect(navigateSpy).toHaveBeenCalledWith(['/characters', 'new-char-id']);
  });

  // dogfood-#6 regression (P4.6aa select-audit rider): the connection-profile
  // options come from an async query, so the select binds the selection
  // per-option with [selected] (no [value] on the <select>). A profile chosen
  // once the options have rendered must be reflected by the native control.
  it('the default-profile select reflects the model via [selected], not [value]', async () => {
    const fixture = await render(stubClient());
    const select = fixture.nativeElement.querySelector(
      '#defaultConnectionProfileId',
    ) as HTMLSelectElement;
    // The async option (from connectionProfileList) has rendered.
    expect(Array.from(select.options).some((o) => o.value === 'p1')).toBe(true);
    // The <select> must NOT carry a [value] binding — the fix is per-option.
    expect(select.getAttribute('ng-reflect-value')).toBeNull();

    setInput(fixture, 'defaultConnectionProfileId', 'p1');
    // The native control (driven only by [selected]) reflects the model.
    expect(select.value).toBe('p1');
    expect(Array.from(select.options).find((o) => o.value === 'p1')!.selected).toBe(true);
  });
});

/**
 * Workspace-tab mode (p4.9j2, v4 `useCloseSelfTab`): back / Cancel / create all
 * close the tab via the handle instead of navigating.
 */
describe('NewCharacter (workspace-tab mode)', () => {
  function makeHandle(): { handle: WorkspaceHandle; closed: string[] } {
    const closed: string[] = [];
    return {
      closed,
      handle: {
        openTab: vi.fn(() => 'x'),
        closeTab: vi.fn((id: string) => closed.push(id)),
        refreshTab: vi.fn(),
      },
    };
  }

  async function render(
    client: Partial<CoreClient>,
    h: WorkspaceHandle,
  ): Promise<ComponentFixture<NewCharacter>> {
    TestBed.configureTestingModule({
      imports: [NewCharacter],
      providers: [
        provideRouter([{ path: '**', component: NewCharacter }]),
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: client },
        { provide: WORKSPACE_HANDLE, useValue: h },
        { provide: WORKSPACE_TAB_ID, useValue: 'tab-new' },
      ],
    });
    const fixture = TestBed.createComponent(NewCharacter);
    fixture.detectChanges();
    for (let i = 0; i < 4; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    return fixture;
  }

  it('renders back/Cancel as buttons (no /characters anchors) and closes on Cancel', async () => {
    const { handle, closed } = makeHandle();
    const fixture = await render(stubClient(), handle);
    const router = TestBed.inject(Router);
    const navigateSpy = vi.spyOn(router, 'navigate');

    expect(fixture.nativeElement.querySelector('a[href="/characters"]')).toBeNull();
    const cancel = Array.from(fixture.nativeElement.querySelectorAll('button')).find(
      (b) => (b as HTMLButtonElement).textContent?.trim() === 'Cancel',
    ) as HTMLButtonElement;
    cancel.click();
    expect(closed).toEqual(['tab-new']);
    expect(navigateSpy).not.toHaveBeenCalled();
  });

  it('closes the tab after a successful create instead of navigating', async () => {
    const { handle, closed } = makeHandle();
    const fixture = await render(stubClient(), handle);
    const router = TestBed.inject(Router);
    const navigateSpy = vi.spyOn(router, 'navigate');

    setInput(fixture, 'name', 'Bertie Wooster');
    const form = fixture.nativeElement.querySelector('form') as HTMLFormElement;
    form.dispatchEvent(new Event('submit', { cancelable: true }));
    await fixture.whenStable();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();

    expect(closed).toEqual(['tab-new']);
    expect(navigateSpy).not.toHaveBeenCalled();
  });
});

describe('NewCharacter — the AI Wizard + Import Template (P4.9K3)', () => {
  it('opens the wizard and stages text fields immediately on Apply', async () => {
    const fixture = await render(stubClient());

    const wizardButton = Array.from(fixture.nativeElement.querySelectorAll('button')).find(
      (b) => (b as HTMLButtonElement).textContent?.includes('AI Wizard'),
    ) as HTMLButtonElement;
    wizardButton.click();
    fixture.detectChanges();

    const modal = fixture.debugElement.query((d) => d.name === 'qt-ai-wizard-modal');
    expect(modal).not.toBeNull();
    modal.componentInstance.apply.emit({ identity: 'A wandering minstrel.' });
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();

    const identityField = fixture.nativeElement.querySelector('[aria-label="Identity"]') as HTMLElement;
    expect(identityField.textContent).toContain('A wandering minstrel.');
  });

  it('queues physicalDescription/wardrobe/scenarios/properties and applies them AFTER creation', async () => {
    const seen: { type: string; [k: string]: unknown }[] = [];
    const fixture = await render(stubClient((req) => seen.push(req)));

    const wizardButton = Array.from(fixture.nativeElement.querySelectorAll('button')).find(
      (b) => (b as HTMLButtonElement).textContent?.includes('AI Wizard'),
    ) as HTMLButtonElement;
    wizardButton.click();
    fixture.detectChanges();

    const modal = fixture.debugElement.query((d) => d.name === 'qt-ai-wizard-modal');
    modal.componentInstance.apply.emit({
      name: 'Ariadne',
      physicalDescription: {
        name: 'Ariadne',
        headAndShouldersPrompt: 'h',
        shortPrompt: 's',
        mediumPrompt: 'm',
        longPrompt: 'l',
        completePrompt: 'c',
        fullDescription: 'A tall librarian.',
      },
      scenarios: [{ title: 'The Athenaeum', content: 'Dust motes in the light.' }],
      wardrobeItems: [{ title: 'Reading Robe', description: '', types: ['top'], isDefault: true }],
      properties: { pronouns: { subject: 'she', object: 'her', possessive: 'her' }, aliases: ['Ari'] },
    });
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();

    // Nothing pending dispatches before creation — they're queued.
    expect(seen.some((r) => r.type === 'characterWardrobeCreate')).toBe(false);
    expect(seen.some((r) => r.type === 'characterScenarioCreate')).toBe(false);

    const form = fixture.nativeElement.querySelector('form') as HTMLFormElement;
    form.dispatchEvent(new Event('submit', { cancelable: true }));
    await fixture.whenStable();
    for (let i = 0; i < 6; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }

    // v4's order: scenarios, physical description, properties, wardrobe.
    const scenarioReq = seen.find((r) => r.type === 'characterScenarioCreate');
    expect(scenarioReq).toMatchObject({ characterId: 'new-char-id', title: 'The Athenaeum' });

    const pdReq = seen.find((r) => r.type === 'characterUpdate' && (r['character'] as Record<string, unknown>)?.['physicalDescription']);
    expect(pdReq).toBeDefined();

    const propsReq = seen.find(
      (r) => r.type === 'characterUpdate' && (r['character'] as Record<string, unknown>)?.['pronouns'],
    );
    expect((propsReq?.['character'] as Record<string, unknown>)?.['aliases']).toEqual(['Ari']);

    const wardrobeReq = seen.find((r) => r.type === 'characterWardrobeCreate');
    expect(wardrobeReq).toMatchObject({
      characterId: 'new-char-id',
      item: { title: 'Reading Robe' },
    });

    // The ORDER itself (v4 `NewCharacterView.tsx:144-192`): a reordering of
    // `onSubmit` must redden this, not just a dropped call (the §3 review's pin).
    const indices = [scenarioReq, pdReq, propsReq, wardrobeReq].map((r) => seen.indexOf(r!));
    expect(indices.every((i) => i >= 0)).toBe(true);
    expect([...indices].sort((a, b) => a - b)).toEqual(indices);
  });

  it('Import Template opens a real modal with v4\'s empty-state copy (no promptTemplateList verb)', async () => {
    const fixture = await render(stubClient());

    const importButton = Array.from(fixture.nativeElement.querySelectorAll('button')).find(
      (b) => (b as HTMLButtonElement).textContent?.trim() === 'Import Template',
    ) as HTMLButtonElement;
    expect(importButton.disabled).toBe(false);
    importButton.click();
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('No templates available');
  });
});
