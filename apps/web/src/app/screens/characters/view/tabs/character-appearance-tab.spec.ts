import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../../core/core-client';
import type { CharacterDetail } from '../../../../core/core-contract';
import { RichEditor } from '../../../../editor/rich-editor';
import { CharacterAppearanceTab } from './character-appearance-tab';

type Req = { type: string; [k: string]: unknown };

function character(): CharacterDetail {
  return {
    id: 'c1',
    name: 'Lorian',
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
  fixture.componentRef.setInput('character', character());
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
