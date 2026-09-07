import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import type { CharacterSystemPrompt } from '../../../../core/core-contract';
import { CharacterPromptPreviewModal } from './preview-modal';

const PROMPT: CharacterSystemPrompt = {
  id: 'p1',
  name: 'Companion',
  content: 'Speak warmly to {{user}} as {{char}}.',
  isDefault: true,
  createdAt: '2026-01-01T00:00:00Z',
  updatedAt: '2026-01-01T00:00:00Z',
};

async function render(): Promise<ComponentFixture<CharacterPromptPreviewModal>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [CharacterPromptPreviewModal] });
  const fixture = TestBed.createComponent(CharacterPromptPreviewModal);
  fixture.componentRef.setInput('prompt', PROMPT);
  fixture.componentRef.setInput('characterName', 'Aria');
  fixture.detectChanges();
  return fixture;
}

describe('CharacterPromptPreviewModal (v4 `PreviewModal.tsx`)', () => {
  it('titles the dialog with the prompt name alone — the isDefault badge JSX is v4 dead code', async () => {
    const fixture = await render();
    const title = (fixture.nativeElement as HTMLElement).querySelector('.qt-dialog-title');
    expect(title?.textContent?.trim()).toBe('Companion');
  });

  it('renders the content through the {{char}}/{{user}} highlighter', async () => {
    const fixture = await render();
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('Speak warmly to');
    expect(text).toContain('as');
  });

  it('Close emits close; Edit emits edit then close', async () => {
    const fixture = await render();
    let closed = 0;
    let edited: CharacterSystemPrompt | null = null;
    fixture.componentInstance.close.subscribe(() => closed++);
    fixture.componentInstance.edit.subscribe((p: CharacterSystemPrompt) => (edited = p));

    const buttons = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('button'));
    (buttons.find((b) => b.textContent?.trim() === 'Edit') as HTMLButtonElement).click();

    expect(edited).toEqual(PROMPT);
    expect(closed).toBe(1);
  });
});
