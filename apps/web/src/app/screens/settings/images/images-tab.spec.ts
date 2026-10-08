import { TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { ImagesTab } from './images-tab';

/**
 * P4.D261 — v4 `7c8572869` `components/settings/tabs/ImagesTabContent.tsx:
 * 55-70` at the pin `f5e953a3f`: the fourth card, Wardrobe Images, between
 * Story Backgrounds and Default Aesthetics, deep-linkable as
 * `?section=wardrobe-images`.
 */
describe('ImagesTab — the Wardrobe Images card (v4 7c8572869)', () => {
  it('mounts the four cards in v4’s order with v4’s title and description', async () => {
    const core = {
      dispatchExpect: vi.fn(async () => ({ type: 'chatSettings', data: {} })),
      dispatchData: vi.fn(async () => ({ profiles: [] })),
    } as unknown as CoreClient;
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [ImagesTab],
      providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: core }],
    });
    const fixture = TestBed.createComponent(ImagesTab);
    fixture.detectChanges();
    const cards = [...(fixture.nativeElement as HTMLElement).querySelectorAll('qt-collapsible-card')];
    const meta = cards.map((c) => [
      c.getAttribute('title') ?? c.getAttribute('ng-reflect-title'),
      c.getAttribute('sectionId') ?? c.getAttribute('sectionid'),
    ]);
    expect(meta.map((m) => m[1])).toEqual([
      'image-profiles',
      'story-backgrounds',
      'wardrobe-images',
      'default-aesthetics',
    ]);
    const wardrobe = cards[2] as HTMLElement;
    expect(wardrobe.getAttribute('title')).toBe('Wardrobe Images');
    expect(wardrobe.getAttribute('description')).toBe(
      'Choose the artist who draws pictures of garments and outfits',
    );
    expect(wardrobe.querySelector('qt-wardrobe-images-card')).not.toBeNull();
  });
});
