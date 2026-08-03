import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../../core/core-client';
import type { CharacterDetail, CharacterListItem } from '../../../../core/core-contract';
import { ToastService } from '../../../../ui/toast.service';
import { CharacterDetailsTab } from './details-tab';

function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

/**
 * dogfood-#6 regression (the final select-audit site, closing the standing
 * audit): the reverse-`{{user}}` dialog's `<select>` lists dynamic options
 * (`otherUserControlled()`, a computed over the async `userControlledCharacters`
 * input). Binding the selection with `[value]` on the `<select>` before those
 * options render leaves the native control blank; the fix binds per-option with
 * `[selected]`, which re-evaluates as the options arrive.
 */

/** The protected surface the regression drives directly (the open button only
 *  appears when the character carries `{{user}}` literals — orthogonal here). */
interface Internal {
  reverseUserSelection: { set(v: string): void };
  showReverseUserDialog: { set(v: boolean): void };
  openReverseUserDialog(): void;
}

function character(over: Partial<CharacterDetail> = {}): CharacterDetail {
  return {
    id: 'self',
    name: 'Bertie',
    title: null,
    identity: null,
    description: null,
    manifesto: null,
    personality: null,
    scenarios: [],
    firstMessage: null,
    exampleDialogues: null,
    systemPrompts: [],
    physicalDescription: null,
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

function partner(id: string, name: string, title: string | null = null): CharacterListItem {
  return {
    id,
    name,
    title,
    description: '',
    defaultImageId: null,
    defaultImage: null,
    isFavorite: false,
    controlledBy: 'user',
    canBeCarina: false,
    defaultConnectionProfileId: null,
    defaultPartnerId: null,
    defaultPartnerName: null,
    defaultTimestampConfig: null,
    defaultScenarioId: null,
    defaultSystemPromptId: null,
    defaultImageProfileId: null,
    npc: false,
    createdAt: '2024-01-01T00:00:00.000Z',
    tags: [],
    updatedAt: '2024-01-01T00:00:00.000Z',
    systemPrompts: [],
    scenarios: [],
    _count: { chats: 0 },
  };
}

function render(
  client: Partial<CoreClient> = { dispatchData: async () => ({}) },
  char: CharacterDetail = character(),
): ComponentFixture<CharacterDetailsTab> {
  TestBed.configureTestingModule({
    imports: [CharacterDetailsTab],
    providers: [
      provideRouter([]),
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: client },
    ],
  });
  const fixture = TestBed.createComponent(CharacterDetailsTab);
  fixture.componentRef.setInput('characterId', 'self');
  fixture.componentRef.setInput('character', char);
  fixture.componentRef.setInput('defaultPartnerName', 'Jeeves');
  fixture.componentRef.setInput('userControlledCharacters', []);
  fixture.detectChanges();
  return fixture;
}

function reverseSelect(fixture: ComponentFixture<CharacterDetailsTab>): HTMLSelectElement {
  const select = fixture.nativeElement.querySelector('.qt-select') as HTMLSelectElement | null;
  if (!select) throw new Error('reverse-user select not rendered');
  return select;
}

describe('CharacterDetailsTab — reverse-{{user}} select (dogfood-#6)', () => {
  it('reflects the stored selection when the options arrive AFTER first render', () => {
    const fixture = render();
    const inst = fixture.componentInstance as unknown as Internal;

    // Open the dialog before any user-controlled options exist (the real gesture:
    // the async userControlledCharacters input has not resolved yet).
    inst.showReverseUserDialog.set(true);
    fixture.detectChanges();
    expect(reverseSelect(fixture).options.length).toBe(0);

    // The options resolve, and a non-default selection is chosen.
    fixture.componentRef.setInput('userControlledCharacters', [
      partner('other1', 'Jeeves'),
      partner('other2', 'Wooster', 'the younger'),
    ]);
    inst.reverseUserSelection.set('other2');
    fixture.detectChanges();

    const select = reverseSelect(fixture);
    // No [value] on the <select> — the fix is per-option [selected].
    expect(select.getAttribute('ng-reflect-value')).toBeNull();
    // The stored value displays even though the options rendered after the bind.
    expect(select.value).toBe('other2');
    const opts = Array.from(select.options);
    expect(opts.find((o) => o.value === 'other2')!.selected).toBe(true);
    expect(opts.find((o) => o.value === 'other1')!.selected).toBe(false);
    // The current character is excluded from the option list.
    expect(opts.map((o) => o.value)).toEqual(['other1', 'other2']);
  });

  it('a native change updates the selection, reflected per-option', () => {
    const fixture = render();
    const inst = fixture.componentInstance as unknown as Internal;
    fixture.componentRef.setInput('userControlledCharacters', [
      partner('other1', 'Jeeves'),
      partner('other2', 'Wooster'),
    ]);
    inst.openReverseUserDialog(); // seeds the first option (other1) + shows the dialog
    fixture.detectChanges();

    const select = reverseSelect(fixture);
    expect(select.value).toBe('other1');

    select.value = 'other2';
    select.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    expect(select.value).toBe('other2');
    expect(Array.from(select.options).find((o) => o.value === 'other2')!.selected).toBe(true);
  });
});

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

/** v4 `useCharacterView.ts:237-308` — the four template replace/restore toasts. */
describe('CharacterDetailsTab — template replace/restore toasts', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  function replaceCharButton(fixture: ComponentFixture<CharacterDetailsTab>): HTMLButtonElement {
    return [...fixture.nativeElement.querySelectorAll('button')].find(
      (b: HTMLButtonElement) =>
        b.title?.startsWith('Replace') && b.title?.includes('with {{char}}'),
    ) as HTMLButtonElement;
  }

  it('toasts the dynamic success sentence and dispatches characterUpdate', async () => {
    const seen: { type: string; [k: string]: unknown }[] = [];
    const fixture = render(
      {
        dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
          seen.push(req);
          return {};
        }) as CoreClient['dispatchData'],
      },
      character({ identity: 'Bertie is a gentleman.' }),
    );

    const btn = replaceCharButton(fixture);
    expect(btn).toBeTruthy();
    btn.click();
    await settle(fixture);

    expect(seen.some((r) => r.type === 'characterUpdate')).toBe(true);
    expect(toasts()).toEqual([
      { type: 'success', message: 'Replaced character name with {{char}}' },
    ]);
  });

  it('toasts "No replacements needed" without dispatching, for an empty transform', async () => {
    const seen: { type: string; [k: string]: unknown }[] = [];
    const fixture = render({
      dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
        seen.push(req);
        return {};
      }) as CoreClient['dispatchData'],
    });
    const inst = fixture.componentInstance as unknown as {
      replaceTemplate(type: 'char' | 'user'): Promise<void>;
    };
    // No {{char}} literal present in this character's fields, so the
    // transform is a no-op — call the handler directly since the button
    // itself only renders once `templateCounts().charCount > 0`.
    await inst.replaceTemplate('char');
    await settle(fixture);

    expect(seen.some((r) => r.type === 'characterUpdate')).toBe(false);
    expect(toasts()).toEqual([{ type: 'success', message: 'No replacements needed' }]);
  });

  it('joins collected partial-failure messages into one error toast', async () => {
    const fixture = render(
      {
        dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
          if (req.type === 'characterUpdate') {
            throw new Error('the character record is locked');
          }
          return {};
        }) as CoreClient['dispatchData'],
      },
      character({ identity: 'Bertie is a gentleman.' }),
    );

    replaceCharButton(fixture).click();
    await settle(fixture);

    expect(toasts()).toEqual([{ type: 'error', message: 'the character record is locked' }]);
  });

  it('reverseTemplate toasts its own dynamic sentence', async () => {
    const seen: { type: string; [k: string]: unknown }[] = [];
    const fixture = render(
      {
        dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
          seen.push(req);
          return {};
        }) as CoreClient['dispatchData'],
      },
      character({ identity: 'This is {{char}}.' }),
    );
    const inst = fixture.componentInstance as unknown as {
      reverseTemplate(type: 'char' | 'user', chosenName?: string): Promise<void>;
    };
    await inst.reverseTemplate('char');
    await settle(fixture);

    expect(seen.some((r) => r.type === 'characterUpdate')).toBe(true);
    expect(toasts()).toEqual([{ type: 'success', message: 'Restored {{char}} to Bertie' }]);
  });
});
