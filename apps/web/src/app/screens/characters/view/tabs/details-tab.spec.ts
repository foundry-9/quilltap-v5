import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../../core/core-client';
import type { CharacterDetail, CharacterListItem } from '../../../../core/core-contract';
import { CharacterDetailsTab } from './details-tab';

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

function render(): ComponentFixture<CharacterDetailsTab> {
  TestBed.configureTestingModule({
    imports: [CharacterDetailsTab],
    providers: [
      provideRouter([]),
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: { dispatchData: async () => ({}) } },
    ],
  });
  const fixture = TestBed.createComponent(CharacterDetailsTab);
  fixture.componentRef.setInput('characterId', 'self');
  fixture.componentRef.setInput('character', character());
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
