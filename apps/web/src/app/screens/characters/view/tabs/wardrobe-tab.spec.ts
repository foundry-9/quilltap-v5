import { TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../../core/core-client';
import { WardrobeDialogService } from '../../../../wardrobe/wardrobe-dialog.service';
import { CharacterWardrobeTab } from './wardrobe-tab';

// The embedded qt-character-choose-outfit-card injects CoreClient + the query
// client; provide stubs so the tab renders.
const stubCore = { dispatchData: async () => ({}) } as unknown as CoreClient;

describe('CharacterWardrobeTab (v4 CharacterDetailView.tsx 8bf3cb5f)', () => {
  function render() {
    TestBed.configureTestingModule({
      imports: [CharacterWardrobeTab],
      providers: [
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: stubCore },
      ],
    });
    return TestBed.createComponent(CharacterWardrobeTab);
  }

  it('the button is ENABLED and opens the global dialog with {characterId} (v4 :229)', () => {
    const fixture = render();
    const service = TestBed.inject(WardrobeDialogService);
    fixture.componentRef.setInput('characterId', 'c1');
    fixture.componentRef.setInput('characterName', 'Aria');
    fixture.detectChanges();

    const el = fixture.nativeElement as HTMLElement;
    // v4's intro prose stays verbatim.
    expect(el.textContent).toContain('The wardrobe lives in a global dialog');
    const button = el.querySelector('button') as HTMLButtonElement;
    expect(button.textContent).toContain('Open wardrobe for Aria');
    // The P4.9f2 stub retirement: no `disabled`, no "not yet available" title.
    expect(button.disabled).toBe(false);
    expect(button.getAttribute('title')).toBeNull();

    button.click();
    expect(service.isOpen()).toBe(true);
    expect(service.context()).toEqual({ characterId: 'c1' });
  });

  it('mounts the dressing-instructions section for this character (v4 b86bb1a5 :330-335)', () => {
    const seen: { type: string; [k: string]: unknown }[] = [];
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [CharacterWardrobeTab],
      providers: [
        provideTanStackQuery(new QueryClient()),
        {
          provide: CoreClient,
          useValue: {
            dispatchData: async (req: { type: string }) => {
              seen.push(req);
              return {};
            },
          } as unknown as CoreClient,
        },
      ],
    });
    const fixture = TestBed.createComponent(CharacterWardrobeTab);
    fixture.componentRef.setInput('characterId', 'c1');
    fixture.detectChanges();

    const el = fixture.nativeElement as HTMLElement;
    expect(el.querySelector('qt-wardrobe-instructions-section')).not.toBeNull();
    expect(el.textContent).toContain('Dressing Instructions');
    // It reads THIS character's own file — the section takes the container, so
    // a wrong id here would silently edit somebody else's instructions.
    expect(seen).toContainEqual({ type: 'characterWardrobeInstructionsGet', characterId: 'c1' });
  });

  it('renders the canChooseOutfit checkbox reflecting the input (v4 8bf3cb5f)', () => {
    const fixture = render();
    fixture.componentRef.setInput('characterId', 'c1');
    fixture.componentRef.setInput('characterName', 'Aria');
    fixture.componentRef.setInput('canChooseOutfit', true);
    fixture.detectChanges();

    const el = fixture.nativeElement as HTMLElement;
    const cb = el.querySelector('input[type="checkbox"]') as HTMLInputElement;
    expect(cb).not.toBeNull();
    expect(cb.checked).toBe(true);
    expect(el.textContent).toContain('Let this character choose their opening outfit');
  });
});
