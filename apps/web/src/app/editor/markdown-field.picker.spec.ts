import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import emojiDataset from '../../../public/emoji/emoji-index.v1.json';
import { stubFetch, type FetchStub } from './char-insert/char-typeahead-harness';
import { EMOJI_PROFILE } from './char-insert/profiles/emoji';
import './jsdom-range-shim';
import { MarkdownField } from './markdown-field';

/**
 * The picker's HOST half: a character picked in the toolbar has to land in
 * whichever view the field is currently showing.
 *
 * @module editor/markdown-field.picker.spec
 */
describe('MarkdownField — a character picked from the toolbar', () => {
  let fixture: ComponentFixture<MarkdownField>;
  let fetchStub: FetchStub;
  const emitted: string[] = [];

  beforeEach(async () => {
    localStorage.clear();
    emitted.length = 0;
    EMOJI_PROFILE.loader.resetForTests();
    fetchStub = stubFetch('emoji-index.v1.json', emojiDataset);

    TestBed.configureTestingModule({ imports: [MarkdownField] });
    fixture = TestBed.createComponent(MarkdownField);
    fixture.componentRef.setInput('value', 'hello');
    fixture.componentInstance.contentChange.subscribe((text) => emitted.push(text));
    fixture.detectChanges();
    for (let i = 0; i < 4; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
  });

  afterEach(() => {
    fetchStub.restore();
    TestBed.resetTestingModule();
  });

  function root(): HTMLElement {
    return fixture.nativeElement as HTMLElement;
  }

  async function pickFirstEmoji(): Promise<void> {
    root().querySelector<HTMLButtonElement>('button[aria-label="Insert emoji"]')!.click();
    fixture.detectChanges();
    for (let i = 0; i < 4; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    root().querySelector<HTMLButtonElement>('.qt-emoji-picker-cell')!.click();
    fixture.detectChanges();
  }

  it('inserts into the editor and records the recent', async () => {
    await pickFirstEmoji();

    expect(emitted.at(-1)).toContain('😀');
    expect(JSON.parse(localStorage.getItem(EMOJI_PROFILE.recentsStorageKey)!)).toEqual(['😀']);
  });

  /**
   * DELIBERATE DIVERGENCE from v4, recorded in `markdown-field.ts`: v4 hands its
   * picker the Lexical editor unconditionally, so in raw-source mode the pick
   * lands in a document the textarea is about to replace — i.e. it is silently
   * lost. v5 inserts at the textarea's caret, the way the formatting buttons
   * already take a source branch.
   */
  it('inserts at the textarea caret in raw-source mode, where v4 loses the pick', async () => {
    root().querySelector<HTMLButtonElement>('button[aria-label="Edit markdown source"]')!.click();
    fixture.detectChanges();

    const textarea = root().querySelector<HTMLTextAreaElement>('textarea')!;
    textarea.selectionStart = textarea.selectionEnd = 2; // "he|llo"

    await pickFirstEmoji();

    expect(emitted.at(-1)).toBe('he😀llo');
    expect(JSON.parse(localStorage.getItem(EMOJI_PROFILE.recentsStorageKey)!)).toEqual(['😀']);
  });
});
