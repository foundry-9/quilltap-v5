import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import emojiDataset from '../../../public/emoji/emoji-index.v1.json';
import { stubFetch, type FetchStub } from './char-insert/char-typeahead-harness';
import { EMOJI_PROFILE } from './char-insert/profiles/emoji';
import { UNICODE_PROFILE } from './char-insert/profiles/unicode';
import { FormattingToolbar, type CharInsert } from './formatting-toolbar';

/**
 * The toolbar's two picker buttons (v4 `FormattingToolbar.tsx:505-560`).
 *
 * The point worth pinning: **neither button is gated by its composer setting.**
 * Those flags govern the automatic `:` / `\` triggers, which are the part that
 * can surprise; an explicit button press never can. The toolbar has no settings
 * input at all, which is how that stays true.
 *
 * @module editor/formatting-toolbar.spec
 */
describe('FormattingToolbar — the emoji and symbol pickers', () => {
  let fixture: ComponentFixture<FormattingToolbar>;
  let fetchStub: FetchStub;
  const inserts: CharInsert[] = [];

  beforeEach(() => {
    localStorage.clear();
    inserts.length = 0;
    EMOJI_PROFILE.loader.resetForTests();
    UNICODE_PROFILE.loader.resetForTests();
    fetchStub = stubFetch('emoji-index.v1.json', emojiDataset);

    TestBed.configureTestingModule({ imports: [FormattingToolbar] });
    fixture = TestBed.createComponent(FormattingToolbar);
    fixture.componentInstance.insertChar.subscribe((event) => inserts.push(event));
    fixture.detectChanges();
  });

  afterEach(() => {
    fetchStub.restore();
    TestBed.resetTestingModule();
  });

  function root(): HTMLElement {
    return fixture.nativeElement as HTMLElement;
  }

  function button(label: string): HTMLButtonElement {
    return root().querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!;
  }

  it('shows both glyph buttons with v4`s titles', () => {
    expect(button('Insert emoji').textContent!.trim()).toBe('☺');
    expect(button('Insert emoji').getAttribute('title')).toBe(
      'Insert emoji (or type `:` and a name)',
    );
    expect(button('Insert a symbol').textContent!.trim()).toBe('Ω');
    expect(button('Insert a symbol').getAttribute('title')).toBe(
      'Insert a symbol (or type `\\` and a name)',
    );
  });

  it('opens and closes the emoji picker on the button, flipping aria-expanded', () => {
    expect(button('Insert emoji').getAttribute('aria-expanded')).toBe('false');

    button('Insert emoji').click();
    fixture.detectChanges();

    expect(root().querySelector('qt-emoji-picker-popover')).not.toBeNull();
    expect(button('Insert emoji').getAttribute('aria-expanded')).toBe('true');

    button('Insert emoji').click();
    fixture.detectChanges();

    expect(root().querySelector('qt-emoji-picker-popover')).toBeNull();
  });

  it('shows one picker at a time', () => {
    button('Insert emoji').click();
    fixture.detectChanges();
    button('Insert a symbol').click();
    fixture.detectChanges();

    expect(root().querySelector('qt-emoji-picker-popover')).toBeNull();
    expect(root().querySelector('qt-unicode-picker-popover')).not.toBeNull();
  });

  it('emits the pick with the profile whose recents it belongs to', async () => {
    button('Insert emoji').click();
    fixture.detectChanges();
    for (let i = 0; i < 4; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }

    const cell = root().querySelector<HTMLButtonElement>('.qt-emoji-picker-cell')!;
    cell.click();
    fixture.detectChanges();

    expect(inserts).toEqual([{ profile: EMOJI_PROFILE, char: '😀' }]);
    // Picking closes the picker (v4's `onClose` on commit).
    expect(root().querySelector('qt-emoji-picker-popover')).toBeNull();
  });

  it('keeps the editor`s selection — mousedown is prevented on both buttons', () => {
    for (const label of ['Insert emoji', 'Insert a symbol']) {
      const event = new MouseEvent('mousedown', { bubbles: true, cancelable: true });
      button(label).dispatchEvent(event);
      expect(event.defaultPrevented).toBe(true);
    }
  });

  it('disables both buttons with the toolbar', () => {
    fixture.componentRef.setInput('disabled', true);
    fixture.detectChanges();

    expect(button('Insert emoji').disabled).toBe(true);
    expect(button('Insert a symbol').disabled).toBe(true);
  });
});
