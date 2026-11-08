import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import emojiDataset from '../../../public/emoji/emoji-index.v1.json';
import { stubFetch, type FetchStub } from './char-insert/char-typeahead-harness';
import { EMOJI_PROFILE } from './char-insert/profiles/emoji';
import { UNICODE_PROFILE } from './char-insert/profiles/unicode';
import { tooltipContentOf } from '../../testing/tooltip-content';
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
    expect(tooltipContentOf(fixture, button('Insert emoji'))).toBe(
      'Insert emoji (or type `:` and a name)',
    );
    expect(button('Insert a symbol').textContent!.trim()).toBe('Ω');
    expect(tooltipContentOf(fixture, button('Insert a symbol'))).toBe(
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

/**
 * v4 `778b97b2b`: a template that takes asterisks away from Markdown
 * (`markdownAsterisks: false`, Novel) withholds Bold — `**` means nothing there
 * (`FormattingToolbar.tsx:486-489`). Absent → shown.
 */
describe('FormattingToolbar — the markdownAsterisks input (P4.D352)', () => {
  afterEach(() => TestBed.resetTestingModule());

  function markdownButtons(f: ComponentFixture<FormattingToolbar>): HTMLButtonElement[] {
    return [
      ...(f.nativeElement as HTMLElement).querySelectorAll<HTMLButtonElement>(
        '.qt-formatting-toolbar-section:first-child > qt-tooltip > button[class*="qt-formatting-button-"]',
      ),
    ].filter((b) => !/outdent|indent|code-block/.test(b.className));
  }

  it('shows Bold by default (v4: absent → shown)', () => {
    TestBed.configureTestingModule({ imports: [FormattingToolbar] });
    const f = TestBed.createComponent(FormattingToolbar);
    f.detectChanges();
    const buttons = markdownButtons(f);
    expect(buttons).toHaveLength(11);
    expect(tooltipContentOf(f, buttons[0])).toBe('Bold');
  });

  it('withholds Bold — and only Bold — when the template says false', () => {
    TestBed.configureTestingModule({ imports: [FormattingToolbar] });
    const f = TestBed.createComponent(FormattingToolbar);
    f.componentRef.setInput('markdownAsterisks', false);
    f.detectChanges();
    const buttons = markdownButtons(f);
    expect(buttons).toHaveLength(10);
    expect(buttons[0].textContent!.trim()).toBe('I');
    expect(buttons.some((b) => tooltipContentOf(f, b) === 'Bold')).toBe(false);
  });
});

/**
 * v4 `df8d4d70a` / `94f041764`: every Markdown button named by
 * `markdownButtonName` under a `qt-tooltip`, the code block's name and
 * `aria-pressed`, and no native `title` left anywhere in the toolbar.
 */
describe('FormattingToolbar — accessible names and tooltips (v4 df8d4d70a)', () => {
  afterEach(() => TestBed.resetTestingModule());

  function mount(inputs: Record<string, unknown> = {}): ComponentFixture<FormattingToolbar> {
    TestBed.configureTestingModule({ imports: [FormattingToolbar] });
    const f = TestBed.createComponent(FormattingToolbar);
    for (const [k, v] of Object.entries(inputs)) f.componentRef.setInput(k, v);
    f.detectChanges();
    return f;
  }

  it('names the eleven Markdown buttons as v4 does', () => {
    const f = mount();
    const names = [
      ...(f.nativeElement as HTMLElement).querySelectorAll<HTMLButtonElement>(
        '.qt-formatting-toolbar-section:first-child > qt-tooltip > button',
      ),
    ]
      .filter((b) => !/outdent|indent|code-block/.test(b.className))
      .map((b) => b.getAttribute('aria-label'));
    expect(names).toEqual([
      'Bold',
      'Italic',
      'Heading 1 (H1)',
      'Heading 2 (H2)',
      'Heading 3 (H3)',
      'Heading 4 (H4)',
      'Heading 5 (H5)',
      'Heading 6 (H6)',
      'Unordered List',
      'Ordered List',
      'Blockquote',
    ]);
  });

  it('names the code-block button and reports its state through aria-pressed', () => {
    const f = mount();
    const code = (f.nativeElement as HTMLElement).querySelector<HTMLButtonElement>('.qt-formatting-button-code-block')!;
    expect(code.getAttribute('aria-label')).toBe('Code block');
    expect(code.getAttribute('aria-pressed')).toBe('false');
    expect(tooltipContentOf(f, code)).toBe('Insert code block');
    f.componentRef.setInput('inCodeBlock', true);
    f.detectChanges();
    expect(code.getAttribute('aria-pressed')).toBe('true');
    expect(tooltipContentOf(f, code)).toBe('End code block');
  });

  it('tooltips outdent / indent / the delimiters / the source toggle, with no native title anywhere', () => {
    const f = mount({
      showSourceToggle: true,
      delimiters: [{ kind: 'wrap', name: 'Thoughts', buttonName: 'Thought', delimiters: ['~', '~'], style: 'qt-thought' }],
    });
    const root = f.nativeElement as HTMLElement;
    expect(tooltipContentOf(f, root.querySelector('.qt-formatting-button-outdent'))).toBe('Outdent list item (Shift+Tab)');
    expect(tooltipContentOf(f, root.querySelector('.qt-formatting-button-indent'))).toBe('Indent list item (Tab)');
    expect(tooltipContentOf(f, root.querySelector('.qt-formatting-button-source'))).toBe('Edit markdown source');
    const delimiter = root.querySelector('.qt-rp-annotation-button');
    expect(delimiter).not.toBeNull();
    expect(tooltipContentOf(f, delimiter)).toMatch(/\(~\.\.\.~\)$/);
    expect(root.querySelector('[title]')).toBeNull();
  });
});
