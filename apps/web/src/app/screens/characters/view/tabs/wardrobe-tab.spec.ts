import { TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { WardrobeDialogService } from '../../../../wardrobe/wardrobe-dialog.service';
import { CharacterWardrobeTab } from './wardrobe-tab';

describe('CharacterWardrobeTab (v4 CharacterDetailView.tsx:218-235)', () => {
  it('the button is ENABLED and opens the global dialog with {characterId} (v4 :229)', () => {
    TestBed.configureTestingModule({ imports: [CharacterWardrobeTab] });
    const service = TestBed.inject(WardrobeDialogService);
    const fixture = TestBed.createComponent(CharacterWardrobeTab);
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
});
