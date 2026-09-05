import { ComponentFixture, TestBed } from '@angular/core/testing';
import { ActivatedRoute, Router, convertToParamMap, provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { of } from 'rxjs';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { CharacterDetail, CoreRequest } from '../../../core/core-contract';
import { ToastService } from '../../../ui/toast.service';
import {
  WORKSPACE_HANDLE,
  WORKSPACE_TAB_ID,
  type WorkspaceHandle,
} from '../../../workspace/workspace-contract';
import { CharacterEdit } from './character-edit';

function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

function character(over: Partial<CharacterDetail> = {}): CharacterDetail {
  return {
    id: 'char-1',
    name: 'Bertie Wooster',
    title: 'The Valet',
    identity: 'A gentleman about town',
    description: 'Affable and a little dim',
    manifesto: 'Never let a pal down',
    personality: 'Cheerful, easily flustered',
    scenarios: [],
    firstMessage: 'I say, old bean!',
    exampleDialogues: '',
    systemPrompt: '',
    systemPrompts: [],
    physicalDescription: null,
    avatarUrl: null,
    defaultImageId: null,
    defaultImage: null,
    defaultConnectionProfileId: null,
    controlledBy: 'llm',
    isFavorite: false,
    canBeCarina: false,
    npc: false,
    defaultAgentModeEnabled: null,
    defaultHelpToolsEnabled: null,
    canDressThemselves: null,
    canCreateOutfits: null,
    defaultTimestampConfig: null,
    defaultScenarioId: null,
    defaultSystemPromptId: null,
    defaultPartnerId: null,
    defaultImageProfileId: null,
    aliases: [],
    pronouns: null,
    characterDocumentMountPointId: null,
    tags: [],
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    _count: { chats: 0 },
    ...over,
  };
}

function stubClient(
  char: CharacterDetail,
  onDispatch?: (req: { type: string; [k: string]: unknown }) => void,
): Partial<CoreClient> {
  return {
    dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
      onDispatch?.(req);
      switch (req.type) {
        case 'characterGet':
          return { character: char };
        case 'tagList':
          return { tags: [] };
        case 'characterUpdate':
          return { character: char };
        case 'characterCascadePreview':
          return {
            characterId: char.id,
            characterName: char.name,
            exclusiveChats: [],
            exclusiveCharacterImageCount: 0,
            exclusiveChatImageCount: 0,
            totalExclusiveImageCount: 0,
            memoryCount: 0,
          };
        case 'characterDelete':
          return { success: true, deletedChats: 0, deletedImages: 0, deletedMemories: 0 };
        default:
          return {};
      }
    }) as CoreClient['dispatchData'],
  } as Partial<CoreClient>;
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<CharacterEdit>> {
  TestBed.configureTestingModule({
    imports: [CharacterEdit],
    providers: [
      // A real (if trivial) route so `router.navigate` resolves instead of
      // rejecting with NG04002 (no match) — CharacterEdit itself is a fine
      // stand-in target, its content is irrelevant to these assertions.
      provideRouter([{ path: '**', component: CharacterEdit }]),
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: client },
      {
        provide: ActivatedRoute,
        useValue: {
          paramMap: of(convertToParamMap({ id: 'char-1' })),
          queryParamMap: of(convertToParamMap({})),
        },
      },
    ],
  });
  const fixture = TestBed.createComponent(CharacterEdit);
  fixture.detectChanges();
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

function findByText(
  fixture: ComponentFixture<CharacterEdit>,
  tag: string,
  text: string,
): HTMLElement | undefined {
  return Array.from(fixture.nativeElement.querySelectorAll(tag)).find(
    (el) => (el as HTMLElement).textContent?.trim() === text,
  ) as HTMLElement | undefined;
}

describe('CharacterEdit', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('gives every Details-tab prose field v4’s own minHeight', async () => {
    // v4 CharacterBasicInfo.tsx:346/364/382/400/517/535/553. The seven fields do
    // NOT share a height — matched by aria-label, never by position, since v4's
    // own values are per-field. (The tab's scenario field is v4 :495, 6rem; it
    // lives in scenario-editor.ts and has carried its value since P4.6aq.) The
    // mechanism is proven in markdown-field.spec; this pins the VALUES.
    const fixture = await render(stubClient(character()));
    const expected: Record<string, string> = {
      Identity: '6rem',
      Description: '8rem',
      Manifesto: '8rem',
      Personality: '8rem',
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

  it('renders the header with the character name and the Details tab by default', async () => {
    const fixture = await render(stubClient(character()));
    expect(fixture.nativeElement.textContent).toContain('Edit: Bertie Wooster');
    expect(fixture.nativeElement.querySelector('#name')).toBeTruthy();
  });

  it('the dirty guard confirms before navigating away on Cancel', async () => {
    const fixture = await render(stubClient(character()));
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(false);
    const router = TestBed.inject(Router);
    const navigateSpy = vi.spyOn(router, 'navigate');

    const nameInput = fixture.nativeElement.querySelector('#name') as HTMLInputElement;
    nameInput.value = 'Reginald Jeeves';
    nameInput.dispatchEvent(new Event('input'));
    fixture.detectChanges();

    const cancelButton = findByText(fixture, 'button', 'Cancel') as HTMLButtonElement;
    cancelButton.click();
    fixture.detectChanges();

    expect(confirmSpy).toHaveBeenCalled();
    expect(navigateSpy).not.toHaveBeenCalled();
  });

  it('Cancel navigates away without confirming when the form is unchanged', async () => {
    const fixture = await render(stubClient(character()));
    const confirmSpy = vi.spyOn(window, 'confirm');
    const router = TestBed.inject(Router);
    const navigateSpy = vi.spyOn(router, 'navigate');

    const cancelButton = findByText(fixture, 'button', 'Cancel') as HTMLButtonElement;
    cancelButton.click();
    fixture.detectChanges();

    expect(confirmSpy).not.toHaveBeenCalled();
    expect(navigateSpy).toHaveBeenCalledWith(['/characters', 'char-1']);
  });

  it('Delete Character opens the cascade dialog, dispatches characterDelete, and navigates to the roster', async () => {
    const seen: CoreRequest[] = [];
    const fixture = await render(
      stubClient(character(), (req) => seen.push(req as unknown as CoreRequest)),
    );
    const router = TestBed.inject(Router);
    const navigateSpy = vi.spyOn(router, 'navigate');

    // Open the delete dialog from the edit-view danger zone.
    const trigger = fixture.nativeElement.querySelector(
      '.mt-6 button.qt-button-destructive',
    ) as HTMLButtonElement;
    expect(trigger.textContent?.trim()).toBe('Delete Character');
    trigger.click();
    fixture.detectChanges();
    // Let the cascade-preview query settle so the confirm button enables.
    for (let i = 0; i < 5; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }

    const confirm = fixture.nativeElement.querySelector(
      'qt-character-delete-dialog button.qt-button-destructive',
    ) as HTMLButtonElement;
    expect(confirm).toBeTruthy();
    confirm.click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();

    const deleteCall = seen.find(
      (r) => (r as unknown as { type: string }).type === 'characterDelete',
    ) as unknown as { characterId: string; cascadeChats: boolean; cascadeImages: boolean };
    expect(deleteCall).toMatchObject({
      characterId: 'char-1',
      cascadeChats: true,
      cascadeImages: true,
    });
    expect(navigateSpy).toHaveBeenCalledWith(['/characters']);
  });

  it('"Save Character" dispatches ONE characterUpdate with the exact PUT bag', async () => {
    const seen: CoreRequest[] = [];
    const fixture = await render(
      stubClient(character(), (req) => seen.push(req as unknown as CoreRequest)),
    );

    const nameInput = fixture.nativeElement.querySelector('#name') as HTMLInputElement;
    nameInput.value = 'Reginald Jeeves';
    nameInput.dispatchEvent(new Event('input'));
    fixture.detectChanges();

    const form = fixture.nativeElement.querySelector('form') as HTMLFormElement;
    form.dispatchEvent(new Event('submit', { cancelable: true }));
    await fixture.whenStable();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();

    const updateCalls = seen.filter(
      (r) => (r as unknown as { type: string }).type === 'characterUpdate',
    );
    expect(updateCalls.length).toBe(1);

    const call = updateCalls[0] as unknown as {
      type: string;
      characterId: string;
      character: Record<string, unknown>;
    };
    expect(call.characterId).toBe('char-1');
    const bag = call.character;
    expect(bag).toEqual({
      name: 'Reginald Jeeves',
      aliases: [],
      pronouns: null,
      title: 'The Valet',
      identity: 'A gentleman about town',
      description: 'Affable and a little dim',
      manifesto: 'Never let a pal down',
      personality: 'Cheerful, easily flustered',
      scenarios: [],
      firstMessage: 'I say, old bean!',
      exampleDialogues: '',
      systemPrompt: '',
      tags: [],
      systemTransparency: false,
      coreWhisperEnabled: null,
      canBeCarina: false,
    });
  });

  it('toasts "Character saved successfully!" on a successful save (v4 useCharacterEdit.ts:250)', async () => {
    const fixture = await render(stubClient(character()));
    const form = fixture.nativeElement.querySelector('form') as HTMLFormElement;
    form.dispatchEvent(new Event('submit', { cancelable: true }));
    await fixture.whenStable();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();

    expect(toasts()).toEqual([{ type: 'success', message: 'Character saved successfully!' }]);
  });

  it('toasts AND keeps the inline banner on a failed save (v4 :254-263, a BOTH row)', async () => {
    const client = stubClient(character());
    (client as { dispatchData: unknown }).dispatchData = (async (req: {
      type: string;
      [k: string]: unknown;
    }) => {
      if (req.type === 'characterUpdate') {
        throw new Error('the registry rejected the new name');
      }
      return (await (stubClient(character()).dispatchData as (r: typeof req) => Promise<unknown>)(
        req,
      )) as unknown;
    }) as CoreClient['dispatchData'];
    const fixture = await render(client);
    const form = fixture.nativeElement.querySelector('form') as HTMLFormElement;
    form.dispatchEvent(new Event('submit', { cancelable: true }));
    await fixture.whenStable();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();

    expect(toasts()).toEqual([{ type: 'error', message: 'the registry rejected the new name' }]);
    expect(fixture.nativeElement.textContent).toContain('the registry rejected the new name');
  });
  /**
   * v4 4.8.2 `fe63547a` — the Core whisper card STACKS. `qt-select` is
   * full-width, so the old two-column flex row gave the dropdown nearly the
   * whole card and wrapped the label into a column a few characters wide.
   * Label and description now span the card with the dropdown beneath. The two
   * checkbox cards above it keep the side-by-side layout.
   */
  it('stacks the Core whisper card — label and description span it, select beneath (v4 fe63547a)', async () => {
    const fixture = await render(stubClient(character()));
    const select = fixture.nativeElement.querySelector('#coreWhisperEnabled') as HTMLElement;
    expect(select).toBeTruthy();

    const card = select.closest('.qt-card') as HTMLElement;
    expect(card).toBeTruthy();
    // No two-column wrapper anywhere between the card and its controls.
    expect(card.querySelector('.flex.items-start.justify-between')).toBeNull();
    // The select is a direct child of the card, beneath the description.
    expect(select.parentElement).toBe(card);

    const label = card.querySelector('label[for="coreWhisperEnabled"]') as HTMLElement;
    const description = card.querySelector('p') as HTMLElement;
    expect(label.parentElement).toBe(card);
    expect(description.parentElement).toBe(card);
    // v4 puts the gap on the description rather than the select.
    expect(description.classList.contains('mb-3')).toBe(true);
    // Document order: label, description, select.
    expect(Array.from(card.children).indexOf(label)).toBeLessThan(
      Array.from(card.children).indexOf(description),
    );
    expect(Array.from(card.children).indexOf(description)).toBeLessThan(
      Array.from(card.children).indexOf(select),
    );
  });

  /**
   * v4 `0506517d3` correction (f1): the EDIT view's hook gained the success
   * toast the detail view's already had — before that, toggling the
   * outfit-choice flag from the edit view saved silently. v5 has ONE card
   * component hosted by BOTH views, so the sentences are pinned once on the
   * card itself (`choose-outfit-card.spec.ts`); what needs pinning HERE is that
   * the edit view is a host of it, i.e. that the toast reaches this view too —
   * which is the whole of what v4's correction bought.
   */
  it('hosts the outfit-choice card on the Wardrobe tab, toast and all (f1)', async () => {
    const fixture = await render(stubClient(character({ canChooseOutfit: true })));
    fixture.componentRef.setInput('tab', 'wardrobe');
    fixture.detectChanges();
    for (let i = 0; i < 4; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }

    const card = fixture.nativeElement.querySelector(
      'qt-character-choose-outfit-card',
    ) as HTMLElement;
    expect(card).toBeTruthy();
    const box = card.querySelector('input[type="checkbox"]') as HTMLInputElement;
    // The host feeds it the loaded character's flag, not a constant.
    expect(box.checked).toBe(true);

    box.checked = false;
    box.dispatchEvent(new Event('change'));
    for (let i = 0; i < 4; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    expect(toasts().at(-1)).toEqual({
      type: 'success',
      message: 'New chats will use this character\u2019s default opening outfit',
    });
  });
});

/**
 * Workspace-tab mode (p4.9j2, v4 `useCloseSelfTab`): identity comes from the
 * `characterId` input (NO `ActivatedRoute`); save/cancel close the tab via the
 * handle instead of navigating.
 */
describe('CharacterEdit (workspace-tab mode)', () => {
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
  ): Promise<ComponentFixture<CharacterEdit>> {
    TestBed.configureTestingModule({
      imports: [CharacterEdit],
      providers: [
        provideRouter([{ path: '**', component: CharacterEdit }]),
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: client },
        { provide: WORKSPACE_HANDLE, useValue: h },
        { provide: WORKSPACE_TAB_ID, useValue: 'tab-4' },
      ],
    });
    const fixture = TestBed.createComponent(CharacterEdit);
    fixture.componentRef.setInput('characterId', 'char-1');
    fixture.detectChanges();
    for (let i = 0; i < 6; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    return fixture;
  }

  it('loads from the characterId input with no ActivatedRoute', async () => {
    const { handle } = makeHandle();
    const fixture = await render(stubClient(character()), handle);
    expect(fixture.nativeElement.textContent).toContain('Edit: Bertie Wooster');
  });

  it('Cancel closes the tab instead of navigating (unchanged form)', async () => {
    const { handle, closed } = makeHandle();
    const fixture = await render(stubClient(character()), handle);
    const router = TestBed.inject(Router);
    const navigateSpy = vi.spyOn(router, 'navigate');

    const cancelButton = findByText(fixture, 'button', 'Cancel') as HTMLButtonElement;
    cancelButton.click();
    fixture.detectChanges();

    expect(closed).toEqual(['tab-4']);
    expect(navigateSpy).not.toHaveBeenCalled();
  });

  it('Save closes the tab instead of navigating', async () => {
    const { handle, closed } = makeHandle();
    const fixture = await render(stubClient(character()), handle);
    const router = TestBed.inject(Router);
    const navigateSpy = vi.spyOn(router, 'navigate');

    const form = fixture.nativeElement.querySelector('form') as HTMLFormElement;
    form.dispatchEvent(new Event('submit', { cancelable: true }));
    await fixture.whenStable();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();

    expect(closed).toEqual(['tab-4']);
    expect(navigateSpy).not.toHaveBeenCalled();
  });
});
