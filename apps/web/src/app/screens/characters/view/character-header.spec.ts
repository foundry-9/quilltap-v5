import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { afterEach, describe, expect, it } from 'vitest';

import type { CharacterDetail } from '../../../core/core-contract';
import { CharacterHeader } from './character-header';

/**
 * The header's three inline toggles (v4 `94f041764`,
 * `app/aurora/[id]/view/components/CharacterHeader.tsx:166-191`, read at
 * `624d0d25c`; P4.D383 item 9): unparameterised names, `aria-pressed`, the
 * favourite's 24px target and its `aria-hidden` glyph. Titles unchanged.
 */

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
  } as CharacterDetail;
}

function render(c: CharacterDetail) {
  TestBed.configureTestingModule({ imports: [CharacterHeader], providers: [provideRouter([])] });
  const fixture = TestBed.createComponent(CharacterHeader);
  fixture.componentRef.setInput('character', c);
  fixture.detectChanges();
  const byTitle = (t: string) =>
    (fixture.nativeElement as HTMLElement).querySelector(`button[title="${t}"]`) as HTMLButtonElement;
  return { fixture, byTitle };
}

afterEach(() => TestBed.resetTestingModule());

describe('CharacterHeader — the toggle names and states (v4 CharacterHeader.tsx:166-191)', () => {
  it('names the three toggles and reports them pressed', () => {
    const { byTitle } = render(character({ isFavorite: true, canBeCarina: true, controlledBy: 'user' }));
    const buttons = [
      byTitle('Remove from favorites'),
      byTitle('Disable Carina answers (@-queries)'),
      byTitle('Switch to LLM control'),
    ];
    expect(buttons.map((b) => b.getAttribute('aria-label'))).toEqual([
      'Favorite',
      'Carina answers (@-queries)',
      'You play this character',
    ]);
    expect(buttons.map((b) => b.getAttribute('aria-pressed'))).toEqual(['true', 'true', 'true']);
  });

  it('reports not-pressed when each toggle is off', () => {
    const { byTitle } = render(character());
    const buttons = [
      byTitle('Add to favorites'),
      byTitle('Enable Carina answers (@-queries)'),
      byTitle('Switch to user control'),
    ];
    expect(buttons.map((b) => b.getAttribute('aria-pressed'))).toEqual(['false', 'false', 'false']);
  });

  it('sizes the favourite to a 24px target and hides its glyph from assistive tech', () => {
    const { byTitle } = render(character({ isFavorite: true }));
    const fav = byTitle('Remove from favorites');
    expect(fav.className).toBe(
      'text-2xl qt-text-favorite transition-transform hover:scale-110 disabled:opacity-50 inline-flex items-center justify-center min-w-6 min-h-6',
    );
    expect(fav.querySelector('span[aria-hidden="true"]')?.textContent?.trim()).toBe('⭐');
  });
});
