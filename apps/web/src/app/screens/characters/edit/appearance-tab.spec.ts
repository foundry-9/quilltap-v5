import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { RichEditor } from '../../../editor/rich-editor';
import { CharacterAppearanceTab } from './appearance-tab';

type Req = { type: string; [k: string]: unknown };

const PHYSICAL = {
  name: 'Lorian',
  headAndShouldersPrompt: 'A narrow face, *fox-like*.',
  shortPrompt: 'Tall, in grey.',
  mediumPrompt: 'Tall, in a **grey** coat.',
  longPrompt: 'Tall, in a grey coat, gloved.',
  completePrompt: 'Tall, in a grey coat, gloved, with a silver pin.',
  fullDescription: 'A full account of the man.',
};

/**
 * `mountPointId` is the character's `characterDocumentMountPointId` — its
 * document vault. Vaulted by default; `null` drives v4's `disabledHint` arm.
 */
function stubClient(
  seen: Req[],
  over: Partial<typeof PHYSICAL> = {},
  guidelines = '',
  mountPointId: string | null = 'mp1',
): Partial<CoreClient> {
  return {
    dispatchData: (async (req: Req) => {
      seen.push(req);
      if (req.type === 'characterGet') {
        return {
          character: {
            id: 'c1',
            characterDocumentMountPointId: mountPointId,
            physicalDescription: { ...PHYSICAL, ...over },
          },
        };
      }
      if (req.type === 'characterDepictionGuidelines') {
        return { content: guidelines };
      }
      return {};
    }) as unknown as CoreClient['dispatchData'],
  };
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<CharacterAppearanceTab>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [CharacterAppearanceTab],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(CharacterAppearanceTab);
  fixture.componentRef.setInput('characterId', 'c1');
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

/** The editor behind the field with this aria-label. */
function editorFor(fixture: ComponentFixture<unknown>, ariaLabel: string): RichEditor {
  const found = fixture.debugElement
    .queryAll(By.directive(RichEditor))
    .find((d) => (d.componentInstance as RichEditor).ariaLabel() === ariaLabel);
  if (!found) throw new Error(`no editor labelled "${ariaLabel}"`);
  return found.componentInstance as RichEditor;
}

function clickButtonWithText(fixture: ComponentFixture<unknown>, text: string): void {
  const button = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('button')).find(
    (b) => (b as HTMLButtonElement).textContent?.trim() === text,
  ) as HTMLButtonElement;
  button.click();
  fixture.detectChanges();
}

describe('CharacterAppearanceTab (edit) — the seven markdown fields', () => {
  it('seeds every prompt field from the loaded physical description', async () => {
    const fixture = await render(stubClient([]));
    expect(editorFor(fixture, 'Head and shoulders prompt').getMarkdown()).toBe(
      PHYSICAL.headAndShouldersPrompt,
    );
    expect(editorFor(fixture, 'Short prompt').getMarkdown()).toBe(PHYSICAL.shortPrompt);
    expect(editorFor(fixture, 'Medium prompt').getMarkdown()).toBe(PHYSICAL.mediumPrompt);
    expect(editorFor(fixture, 'Long prompt').getMarkdown()).toBe(PHYSICAL.longPrompt);
    expect(editorFor(fixture, 'Complete prompt').getMarkdown()).toBe(PHYSICAL.completePrompt);
    expect(editorFor(fixture, 'Full description').getMarkdown()).toBe(PHYSICAL.fullDescription);
  });

  it('carries an edited prompt into the characterUpdate payload, leaving the rest untouched', async () => {
    const seen: Req[] = [];
    const fixture = await render(stubClient(seen));

    editorFor(fixture, 'Medium prompt').setMarkdown('Tall, in a **charcoal** coat.');
    await settle(fixture);
    clickButtonWithText(fixture, 'Save Physical Descriptions');
    await settle(fixture);

    const update = seen.find((r) => r.type === 'characterUpdate');
    expect(update).toBeTruthy();
    const pd = (update!['character'] as Record<string, unknown>)['physicalDescription'] as Record<
      string,
      unknown
    >;
    expect(pd['mediumPrompt']).toBe('Tall, in a **charcoal** coat.');
    expect(pd['shortPrompt']).toBe(PHYSICAL.shortPrompt);
    expect(pd['name']).toBe('Lorian');
  });

  it('an untouched save re-sends the stored prompts byte-identical', async () => {
    // The editors gate on the query, so they mount with content and absorb their
    // one emit — a load is not an edit, and non-canonical stored bytes survive.
    const seen: Req[] = [];
    const fixture = await render(stubClient(seen, { shortPrompt: 'Tall, in __grey__.' }));

    // Displayed canonically...
    expect(editorFor(fixture, 'Short prompt').getMarkdown()).toBe('Tall, in **grey**.');
    clickButtonWithText(fixture, 'Save Physical Descriptions');
    await settle(fixture);

    // ...but saved verbatim.
    const update = seen.find((r) => r.type === 'characterUpdate');
    const pd = (update!['character'] as Record<string, unknown>)['physicalDescription'] as Record<
      string,
      unknown
    >;
    expect(pd['shortPrompt']).toBe('Tall, in __grey__.');
  });

  it('the depiction guidelines field seeds and saves through its own verb', async () => {
    const seen: Req[] = [];
    const fixture = await render(stubClient(seen, {}, 'Never depicted *unmasked*.'));

    expect(editorFor(fixture, 'Depiction guidelines').getMarkdown()).toBe(
      'Never depicted *unmasked*.',
    );

    editorFor(fixture, 'Depiction guidelines').setMarkdown('Never depicted **unmasked**.');
    await settle(fixture);
    clickButtonWithText(fixture, 'Save Guidelines');
    await settle(fixture);

    const save = seen.find((r) => r.type === 'characterDepictionGuidelinesUpdate');
    expect(save).toBeTruthy();
    expect(save!['content']).toBe('Never depicted **unmasked**.');
  });
});

describe('CharacterAppearanceTab (edit) — the no-vault depiction-guidelines hint', () => {
  // v4's warning, verbatim (`CharacterEditView.tsx:346`). Pinned here so a
  // reworded copy in the component fails rather than silently drifting.
  const HINT =
    'This character has no document vault yet, so depiction guidelines cannot be stored. Save the character once to provision its vault, then return here.';

  it('suppresses the editor behind the verbatim warning, and fetches nothing', async () => {
    const seen: Req[] = [];
    const fixture = await render(stubClient(seen, {}, '', null));

    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain(HINT);

    // The editor AND its Save are gone — v4 replaces both (`:113-114`).
    expect(() => editorFor(fixture, 'Depiction guidelines')).toThrow();
    const saveButton = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
    ).find((b) => b.textContent?.trim() === 'Save Guidelines');
    expect(saveButton).toBeUndefined();

    // v4 short-circuits the load before `fetch` (`AestheticEditorField.tsx:54-56`):
    // there is no vault to read from, so the verb must never be dispatched.
    expect(seen.some((r) => r.type === 'characterDepictionGuidelines')).toBe(false);

    // The physical-description block is untouched by the guidelines arm.
    expect(editorFor(fixture, 'Short prompt').getMarkdown()).toBe(PHYSICAL.shortPrompt);
  });

  it('leaves the editor active — and the warning absent — for a vaulted character', async () => {
    const seen: Req[] = [];
    const fixture = await render(stubClient(seen, {}, 'Never depicted *unmasked*.'));

    expect((fixture.nativeElement as HTMLElement).textContent ?? '').not.toContain(HINT);
    expect(editorFor(fixture, 'Depiction guidelines').getMarkdown()).toBe(
      'Never depicted *unmasked*.',
    );
    expect(seen.some((r) => r.type === 'characterDepictionGuidelines')).toBe(true);
  });
});
