import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { ImageMetadata } from './image-metadata';
import type { CharacterGalleryLink, DetailCharacter } from './images.api';

/**
 * The character-album panel. v4 has NO test for `ImageMetadata.tsx` — every
 * case here pins a transcribed v4 behavior by `file:line` (this suite is the
 * component's only fidelity record).
 */
describe('ImageMetadata (the character-album panel)', () => {
  afterEach(() => TestBed.resetTestingModule());

  const characters: DetailCharacter[] = [
    { id: 'c-bram', name: 'Bramwell', defaultImageId: null },
    { id: 'c-aria', name: 'Aria', defaultImageId: 'img-1' },
    { id: 'c-zed', name: 'Zed', defaultImageId: null },
  ];
  const links: CharacterGalleryLink[] = [
    { characterId: 'c-aria', characterName: 'Aria', linkId: 'l-1' },
    { characterId: 'c-bram', characterName: 'Bramwell', linkId: 'l-2' },
  ];

  interface Emitted {
    add: string[];
    remove: string[];
    avatar: string[];
  }

  async function render(
    inputs: Partial<{
      imageId: string;
      characters: DetailCharacter[];
      loadingEntities: boolean;
      characterGalleryLinks: CharacterGalleryLink[];
      savingToGalleryFor: ReadonlySet<string>;
      settingAvatar: ReadonlySet<string>;
    }> = {},
  ): Promise<{ fixture: ComponentFixture<ImageMetadata>; emitted: Emitted }> {
    TestBed.configureTestingModule({ imports: [ImageMetadata] });
    const fixture = TestBed.createComponent(ImageMetadata);
    fixture.componentRef.setInput('imageId', inputs.imageId ?? 'img-1');
    fixture.componentRef.setInput('characters', inputs.characters ?? characters);
    fixture.componentRef.setInput('loadingEntities', inputs.loadingEntities ?? false);
    fixture.componentRef.setInput('characterGalleryLinks', inputs.characterGalleryLinks ?? links);
    fixture.componentRef.setInput('savingToGalleryFor', inputs.savingToGalleryFor ?? new Set());
    fixture.componentRef.setInput('settingAvatar', inputs.settingAvatar ?? new Set());
    const emitted: Emitted = { add: [], remove: [], avatar: [] };
    fixture.componentInstance.addToCharacterGallery.subscribe((id) => emitted.add.push(id));
    fixture.componentInstance.removeFromCharacterGallery.subscribe((id) => emitted.remove.push(id));
    fixture.componentInstance.setAsAvatar.subscribe((id) => emitted.avatar.push(id));
    fixture.detectChanges();
    return { fixture, emitted };
  }

  // ImageMetadata.tsx:55-57 — the loading line.
  it('shows the loading line while entities load', async () => {
    const { fixture } = await render({ loadingEntities: true });
    expect(fixture.nativeElement.textContent).toContain('Loading characters...');
    expect(fixture.nativeElement.textContent).not.toContain('In Photo Albums');
  });

  // ImageMetadata.tsx:29-50 — the in-gallery/available split, each sorted by
  // localeCompare; :59-110 the "In Photo Albums" section.
  it('splits characters into sorted In-Photo-Albums rows and the save select', async () => {
    const { fixture } = await render();
    const rows = Array.from(
      fixture.nativeElement.querySelectorAll('.qt-bg-muted\\/50'),
    ) as HTMLElement[];
    // Aria + Bramwell hold the image (sorted: Aria before Bramwell).
    expect(rows.length).toBe(2);
    expect(rows[0].textContent).toContain('Aria');
    expect(rows[1].textContent).toContain('Bramwell');
    // Zed is available in the select.
    const options = Array.from(
      fixture.nativeElement.querySelectorAll('select option'),
    ) as HTMLOptionElement[];
    expect(options.map((o) => o.textContent?.trim())).toEqual(['Select a character...', 'Zed']);
  });

  // ImageMetadata.tsx:66 / :77-80 — the Avatar badge when
  // `character.defaultImageId === imageId`; the Set Avatar button otherwise.
  it('renders the Avatar badge for the current avatar and Set Avatar for the rest', async () => {
    const { fixture } = await render();
    const rows = Array.from(
      fixture.nativeElement.querySelectorAll('.qt-bg-muted\\/50'),
    ) as HTMLElement[];
    expect(rows[0].textContent).toContain('Avatar');
    expect(rows[0].querySelector('button[title="Set as avatar"]')).toBeNull();
    expect(rows[1].querySelector('button[title="Set as avatar"]')).toBeTruthy();
  });

  // ImageMetadata.tsx:82-92 — Set Avatar emits (v4 calls
  // `onSetAsAvatar('character', id)`; EntityType is only 'character').
  it('emits setAsAvatar with the character id', async () => {
    const { fixture, emitted } = await render();
    (
      fixture.nativeElement.querySelector('button[title="Set as avatar"]') as HTMLButtonElement
    ).click();
    expect(emitted.avatar).toEqual(['c-bram']);
  });

  // ImageMetadata.tsx:67 / :87-91 — the in-flight avatar key is
  // `character-{id}-avatar`, rendering the '...' label.
  it('disables the Set Avatar button while its key is in settingAvatar', async () => {
    const { fixture } = await render({ settingAvatar: new Set(['character-c-bram-avatar']) });
    const button = fixture.nativeElement.querySelector(
      'button[title="Set as avatar"]',
    ) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    expect(button.textContent).toContain('...');
  });

  // ImageMetadata.tsx:94-104 — the per-character remove.
  it('emits removeFromCharacterGallery from the × button', async () => {
    const { fixture, emitted } = await render();
    const rows = Array.from(
      fixture.nativeElement.querySelectorAll('.qt-bg-muted\\/50'),
    ) as HTMLElement[];
    (rows[0].querySelector('button[title="Remove from photo album"]') as HTMLButtonElement).click();
    expect(emitted.remove).toEqual(['c-aria']);
  });

  // ImageMetadata.tsx:121-139 — picking a character emits the add and the
  // select snaps back to '' (v4's controlled `value=""`).
  it('emits addToCharacterGallery on select and resets the select', async () => {
    const { fixture, emitted } = await render();
    const select = fixture.nativeElement.querySelector('select') as HTMLSelectElement;
    select.value = 'c-zed';
    select.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    expect(emitted.add).toEqual(['c-zed']);
    expect(select.value).toBe('');
  });

  // ImageMetadata.tsx:143-146 — the no-characters message.
  it('shows the no-characters message when the roster is empty', async () => {
    const { fixture } = await render({ characters: [], characterGalleryLinks: [] });
    expect(fixture.nativeElement.textContent).toContain('No characters available.');
  });
});
