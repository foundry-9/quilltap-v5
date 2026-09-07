import { TestBed, type ComponentFixture } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import type { ChatCreateOutfitSelectionInput } from '../../core/core-contract';
import {
  OutfitSelector,
  computeSyncInitialMode,
  type OutfitSelectorCharacter,
} from './outfit-selector';

describe('computeSyncInitialMode (v4 8bf3cb5f)', () => {
  it('an LLM character flagged canChooseOutfit opens on llm_choose', () => {
    expect(computeSyncInitialMode({ id: 'a', name: 'Aria', canChooseOutfit: true })).toBe(
      'llm_choose',
    );
  });

  it('an unflagged LLM character opens on default', () => {
    expect(computeSyncInitialMode({ id: 'a', name: 'Aria' })).toBe('default');
    expect(computeSyncInitialMode({ id: 'a', name: 'Aria', canChooseOutfit: false })).toBe(
      'default',
    );
  });

  it('the user persona opens on default even when flagged (this dialog IS the choosing)', () => {
    expect(
      computeSyncInitialMode({ id: 'u', name: 'You', canChooseOutfit: true, isUserControlled: true }),
    ).toBe('default');
  });
});

describe('OutfitSelector seed emission', () => {
  function render(chars: OutfitSelectorCharacter[]): {
    fixture: ComponentFixture<OutfitSelector>;
    emitted: ChatCreateOutfitSelectionInput[][];
  } {
    TestBed.configureTestingModule({ imports: [OutfitSelector] });
    const fixture = TestBed.createComponent(OutfitSelector);
    const emitted: ChatCreateOutfitSelectionInput[][] = [];
    fixture.componentInstance.selectionsChange.subscribe((v) => emitted.push(v));
    fixture.componentRef.setInput('characters', chars);
    fixture.detectChanges();
    return { fixture, emitted };
  }

  it('emits the synchronous seed for each character (llm_choose for a flagged LLM char)', () => {
    const { emitted } = render([
      { id: 'a', name: 'Aria', canChooseOutfit: true },
      { id: 'b', name: 'Bram', canChooseOutfit: false },
    ]);
    const last = emitted[emitted.length - 1];
    expect(last).toEqual([
      { characterId: 'a', mode: 'llm_choose' },
      { characterId: 'b', mode: 'default' },
    ]);
  });

  it('renders the flagged LLM character with Let character choose pre-checked', () => {
    const { fixture } = render([{ id: 'a', name: 'Aria', canChooseOutfit: true }]);
    const el = fixture.nativeElement as HTMLElement;
    const checked = Array.from(el.querySelectorAll('input[type="radio"]')).find(
      (r) => (r as HTMLInputElement).checked,
    ) as HTMLInputElement;
    expect(checked.value).toBe('llm_choose');
  });
});
