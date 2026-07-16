import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../../core/core-client';
import type { CharacterDetail } from '../../../../core/core-contract';
import { RichEditor } from '../../../../editor/rich-editor';
import { CharacterAppearanceTab } from './character-appearance-tab';

type Req = { type: string; [k: string]: unknown };

/**
 * `mountPointId` is the character's `characterDocumentMountPointId` — its
 * document vault. Vaulted by default; `null` drives v4's `disabledHint` arm.
 */
function character(mountPointId: string | null = 'mp1'): CharacterDetail {
  return {
    id: 'c1',
    name: 'Lorian',
    characterDocumentMountPointId: mountPointId,
    physicalDescription: { shortPrompt: 'Tall, in grey.' },
  } as unknown as CharacterDetail;
}

function stubClient(seen: Req[], guidelines: string): Partial<CoreClient> {
  return {
    dispatchData: (async (req: Req) => {
      seen.push(req);
      if (req.type === 'characterDepictionGuidelines') {
        return { content: guidelines };
      }
      return {};
    }) as unknown as CoreClient['dispatchData'],
  };
}

async function render(
  seen: Req[],
  guidelines: string,
  mountPointId: string | null = 'mp1',
): Promise<ComponentFixture<CharacterAppearanceTab>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [CharacterAppearanceTab],
    providers: [
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: stubClient(seen, guidelines) },
    ],
  });
  const fixture = TestBed.createComponent(CharacterAppearanceTab);
  fixture.componentRef.setInput('characterId', 'c1');
  fixture.componentRef.setInput('character', character(mountPointId));
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

function editor(fixture: ComponentFixture<unknown>): RichEditor {
  return fixture.debugElement.query(By.directive(RichEditor)).componentInstance as RichEditor;
}

function clickButtonWithText(fixture: ComponentFixture<unknown>, text: string): void {
  const button = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('button')).find(
    (b) => (b as HTMLButtonElement).textContent?.trim() === text,
  ) as HTMLButtonElement;
  button.click();
  fixture.detectChanges();
}

describe('CharacterAppearanceTab (view) — the guidelines markdown field', () => {
  it('seeds the field from the loaded guidelines', async () => {
    const fixture = await render([], 'Never depicted *unmasked*.');
    expect(editor(fixture).getMarkdown()).toBe('Never depicted *unmasked*.');
  });

  it('carries an edit into characterDepictionGuidelinesUpdate', async () => {
    const seen: Req[] = [];
    const fixture = await render(seen, 'Never depicted unmasked.');

    editor(fixture).setMarkdown('Never depicted **unmasked**.');
    await settle(fixture);
    clickButtonWithText(fixture, 'Save');
    await settle(fixture);

    const save = seen.find((r) => r.type === 'characterDepictionGuidelinesUpdate');
    expect(save).toBeTruthy();
    expect(save!['characterId']).toBe('c1');
    expect(save!['content']).toBe('Never depicted **unmasked**.');
  });

  it('an untouched save re-sends the stored guidelines byte-identical', async () => {
    // The field mounts behind the query's pending gate, so it holds the content
    // at mount and absorbs its one emit: a load is never an edit.
    const seen: Req[] = [];
    const fixture = await render(seen, 'Never depicted __unmasked__.');

    expect(editor(fixture).getMarkdown()).toBe('Never depicted **unmasked**.');
    clickButtonWithText(fixture, 'Save');
    await settle(fixture);

    const save = seen.find((r) => r.type === 'characterDepictionGuidelinesUpdate');
    expect(save!['content']).toBe('Never depicted __unmasked__.');
  });
});

describe('CharacterAppearanceTab (view) — the no-vault depiction-guidelines hint', () => {
  // v4's warning, verbatim (`CharacterEditView.tsx:346`). Pinned here so a
  // reworded copy in the component fails rather than silently drifting.
  const HINT =
    'This character has no document vault yet, so depiction guidelines cannot be stored. Save the character once to provision its vault, then return here.';

  it('suppresses the editor behind the verbatim warning, and fetches nothing', async () => {
    const seen: Req[] = [];
    const fixture = await render(seen, 'Never depicted unmasked.', null);

    expect((fixture.nativeElement as HTMLElement).textContent ?? '').toContain(HINT);

    // The editor AND its Save are gone — v4 replaces both (`:113-114`).
    expect(fixture.debugElement.query(By.directive(RichEditor))).toBeNull();
    const saveButton = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
    ).find((b) => b.textContent?.trim() === 'Save');
    expect(saveButton).toBeUndefined();

    // v4 short-circuits the load before `fetch` (`AestheticEditorField.tsx:54-56`):
    // there is no vault to read from, so the verb must never be dispatched.
    expect(seen.some((r) => r.type === 'characterDepictionGuidelines')).toBe(false);

    // The read-only physical-description block is untouched by the guidelines arm.
    expect((fixture.nativeElement as HTMLElement).textContent).toContain('Tall, in grey.');
  });

  it('leaves the editor active — and the warning absent — for a vaulted character', async () => {
    const seen: Req[] = [];
    const fixture = await render(seen, 'Never depicted *unmasked*.');

    expect((fixture.nativeElement as HTMLElement).textContent ?? '').not.toContain(HINT);
    expect(editor(fixture).getMarkdown()).toBe('Never depicted *unmasked*.');
    expect(seen.some((r) => r.type === 'characterDepictionGuidelines')).toBe(true);
  });
});
