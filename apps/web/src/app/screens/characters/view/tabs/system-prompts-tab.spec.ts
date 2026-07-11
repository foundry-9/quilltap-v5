import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { describe, expect, it } from 'vitest';

import type { CharacterDetail } from '../../../../core/core-contract';
import { CharacterSystemPromptsTab } from './system-prompts-tab';

function character(promptContent: string): CharacterDetail {
  return {
    id: 'c1',
    name: 'Friday',
    systemPrompts: [{ id: 'sp1', name: 'Friday as Executive Assistant', content: promptContent, isDefault: true }],
  } as unknown as CharacterDetail;
}

async function render(promptContent: string) {
  TestBed.configureTestingModule({
    imports: [CharacterSystemPromptsTab],
    providers: [provideRouter([])],
  });
  const fixture = TestBed.createComponent(CharacterSystemPromptsTab);
  fixture.componentRef.setInput('characterId', 'c1');
  fixture.componentRef.setInput('character', character(promptContent));
  fixture.componentRef.setInput('defaultPartnerName', 'Chief');
  fixture.detectChanges();
  return fixture;
}

describe('CharacterSystemPromptsTab', () => {
  it('renders the prompt body byte-exact — no template whitespace leaks into the <pre> (finding #5)', async () => {
    // The name occurrences exercise the highlight segmentation; the guard is
    // that segmentation + the <pre> wrapper add NO characters of their own.
    const content = '# Friday — Executive Assistant Prompt\nYou are Friday. You call him Chief.';
    const fixture = await render(content);
    const code = fixture.nativeElement.querySelector('pre code') as HTMLElement;
    expect(code).toBeTruthy();
    expect(code.textContent).toBe(content);
  });

  it('highlights hardcoded character and partner names inside the prompt', async () => {
    const fixture = await render('You are Friday. You call him Chief.');
    const highlighted = Array.from(
      fixture.nativeElement.querySelectorAll('pre code span[title]'),
    ) as HTMLElement[];
    expect(highlighted.map((s) => s.textContent)).toEqual(['Friday', 'Chief']);
  });
});
