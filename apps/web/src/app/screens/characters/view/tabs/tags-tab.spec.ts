import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../../core/core-client';
import { CharacterTagsTab } from './tags-tab';

function stubClient(
  onDispatch: (req: { type: string; [k: string]: unknown }) => void,
): Partial<CoreClient> {
  return {
    dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
      onDispatch(req);
      switch (req.type) {
        case 'characterGetTags':
          return { tags: [{ id: 't1', name: 'valet', visualStyle: null }] };
        case 'tagList':
          return {
            tags: [
              { id: 't1', name: 'valet' },
              { id: 't2', name: 'jolly-good' },
            ],
          };
        case 'tagCreate':
          return { tag: { id: 't-new', name: (req['name'] as string) ?? '' } };
        default:
          return {};
      }
    }) as CoreClient['dispatchData'],
  };
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<CharacterTagsTab>> {
  TestBed.configureTestingModule({
    imports: [CharacterTagsTab],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(CharacterTagsTab);
  fixture.componentRef.setInput('characterId', 'c1');
  fixture.detectChanges();
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

describe('CharacterTagsTab', () => {
  it('renders the existing tag chip', async () => {
    const fixture = await render(stubClient(() => {}));
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('valet');
  });

  it('dispatches characterRemoveTag when a chip is removed', async () => {
    const seen: Array<{ type: string; [k: string]: unknown }> = [];
    const fixture = await render(stubClient((r) => seen.push(r)));
    const removeButton = fixture.nativeElement.querySelector(
      '[aria-label="Remove tag valet"]',
    ) as HTMLButtonElement;
    expect(removeButton).toBeTruthy();
    removeButton.click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    const req = seen.find((r) => r.type === 'characterRemoveTag');
    expect(req).toEqual({ type: 'characterRemoveTag', characterId: 'c1', tagId: 't1' });
  });

  it('dispatches characterAddTag when an existing suggestion is picked', async () => {
    const seen: Array<{ type: string; [k: string]: unknown }> = [];
    const fixture = await render(stubClient((r) => seen.push(r)));
    const addButton = Array.from(fixture.nativeElement.querySelectorAll('button')).find(
      (b) => (b as HTMLButtonElement).textContent?.trim() === '+ Add Tag',
    ) as HTMLButtonElement;
    addButton.click();
    fixture.detectChanges();
    const input = fixture.nativeElement.querySelector('input[type="text"]') as HTMLInputElement;
    input.value = 'jolly';
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    const suggestion = Array.from(fixture.nativeElement.querySelectorAll('li button')).find((b) =>
      (b as HTMLButtonElement).textContent?.includes('jolly-good'),
    ) as HTMLButtonElement;
    expect(suggestion).toBeTruthy();
    suggestion.click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    const req = seen.find((r) => r.type === 'characterAddTag');
    expect(req).toEqual({ type: 'characterAddTag', characterId: 'c1', tagId: 't2' });
  });

  it('creates and attaches a new tag via tagCreate then characterAddTag', async () => {
    const seen: Array<{ type: string; [k: string]: unknown }> = [];
    const fixture = await render(stubClient((r) => seen.push(r)));
    const addButton = Array.from(fixture.nativeElement.querySelectorAll('button')).find(
      (b) => (b as HTMLButtonElement).textContent?.trim() === '+ Add Tag',
    ) as HTMLButtonElement;
    addButton.click();
    fixture.detectChanges();
    const input = fixture.nativeElement.querySelector('input[type="text"]') as HTMLInputElement;
    input.value = 'brand-new-tag';
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    const createButton = Array.from(fixture.nativeElement.querySelectorAll('button')).find((b) =>
      (b as HTMLButtonElement).textContent?.includes('Create "brand-new-tag"'),
    ) as HTMLButtonElement;
    expect(createButton).toBeTruthy();
    createButton.click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    expect(seen.find((r) => r.type === 'tagCreate')).toEqual({
      type: 'tagCreate',
      name: 'brand-new-tag',
    });
    expect(seen.find((r) => r.type === 'characterAddTag')).toEqual({
      type: 'characterAddTag',
      characterId: 'c1',
      tagId: 't-new',
    });
  });
});
