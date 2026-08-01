import { Component, signal, viewChild } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { TextSelection } from 'prosemirror-state';
import { afterEach, describe, expect, it } from 'vitest';

import './jsdom-range-shim';
import { MarkdownField } from './markdown-field';
import type { RichEditor } from './rich-editor';

@Component({
  imports: [MarkdownField],
  template: `<qt-markdown-field
    [value]="value()"
    [minHeight]="minHeight()"
    [showSourceToggle]="showSourceToggle()"
    (contentChange)="last.set($event); emits.set(emits() + 1)"
  />`,
})
class Host {
  readonly field = viewChild.required(MarkdownField);
  readonly value = signal('');
  readonly minHeight = signal('');
  readonly showSourceToggle = signal(true);
  readonly last = signal('');
  readonly emits = signal(0);
}

async function render(initial = '', minHeight = ''): Promise<ComponentFixture<Host>> {
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  fixture.componentInstance.value.set(initial);
  fixture.componentInstance.minHeight.set(minHeight);
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return fixture;
}

/** The raw-source textarea — present only in source mode. */
function sourceArea(fixture: ComponentFixture<Host>): HTMLTextAreaElement | null {
  return fixture.nativeElement.querySelector('.qt-markdown-field-source');
}

/** Click the source toggle and settle (the swap re-mounts the editor). */
async function toggleSource(fixture: ComponentFixture<Host>): Promise<void> {
  button(fixture, 'source').click();
  fixture.detectChanges();
  await fixture.whenStable();
  await new Promise((r) => queueMicrotask(() => r(null)));
  fixture.detectChanges();
}

/** The `qt-rich-editor` element — where the field applies v4's minHeight. */
function editorEl(fixture: ComponentFixture<Host>): HTMLElement {
  return fixture.nativeElement.querySelector('qt-rich-editor') as HTMLElement;
}

function button(fixture: ComponentFixture<Host>, typeClass: string): HTMLButtonElement {
  return fixture.nativeElement.querySelector(`.qt-formatting-button-${typeClass}`) as HTMLButtonElement;
}

/** The field's `RichEditor` handle (a `protected viewChild`, cast for the test). */
function richEditor(fixture: ComponentFixture<Host>): RichEditor {
  return (fixture.componentInstance.field() as unknown as { editor: () => RichEditor }).editor();
}

/** The RichEditor content markdown, read via the field's editor handle. */
function markdown(fixture: ComponentFixture<Host>): string {
  return richEditor(fixture).getMarkdown();
}

describe('MarkdownField — shared form-field editor', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('renders the toolbar button inventory (v4 MARKDOWN_FORMATS + code block)', async () => {
    const fixture = await render('body');
    for (const t of ['bold', 'italic', 'h1', 'h2', 'h3', 'blockquote', 'code-block', 'indent', 'outdent']) {
      expect(button(fixture, t), t).not.toBeNull();
    }
  });

  it('carries v4\'s indent/outdent titles verbatim (FormattingToolbar.tsx:437-449)', async () => {
    const fixture = await render('body');
    expect(button(fixture, 'outdent').getAttribute('title')).toBe('Outdent list item (Shift+Tab)');
    expect(button(fixture, 'outdent').getAttribute('aria-label')).toBe('Outdent list item');
    expect(button(fixture, 'indent').getAttribute('title')).toBe('Indent list item (Tab)');
    expect(button(fixture, 'indent').getAttribute('aria-label')).toBe('Indent list item');
  });

  it('H2 button converts the block at the cursor to a heading', async () => {
    const fixture = await render('a title');
    button(fixture, 'h2').click();
    expect(markdown(fixture)).toBe('## a title');
  });

  it('blockquote button wraps the block', async () => {
    const fixture = await render('quote me');
    button(fixture, 'blockquote').click();
    expect(markdown(fixture)).toBe('> quote me');
  });

  it('the code-block button toggles in and back out (v4 CODE/​/CODE)', async () => {
    const fixture = await render('snippet');
    const codeBtn = button(fixture, 'code-block');
    codeBtn.click();
    fixture.detectChanges();
    expect(markdown(fixture)).toBe('```\nsnippet\n```');
    expect(codeBtn.textContent?.trim()).toBe('/CODE');
    codeBtn.click();
    expect(markdown(fixture)).toBe('snippet');
  });

  it('preserves the textarea contract: no emit on load (absorb-once)', async () => {
    const fixture = await render('__bold__');
    await new Promise((r) => queueMicrotask(() => r(null)));
    fixture.detectChanges();
    // The initial serialization normalization (__bold__ → **bold**) is absorbed,
    // not surfaced as a change — exactly like a textarea, which never emits on
    // load. The editor still DISPLAYS the normalized form.
    expect(fixture.componentInstance.last()).toBe('');
    expect(markdown(fixture)).toBe('**bold**');
  });

  it('emits on a genuine edit (toolbar action)', async () => {
    const fixture = await render('a title');
    button(fixture, 'h2').click();
    fixture.detectChanges();
    expect(fixture.componentInstance.last()).toBe('## a title');
  });

  it('applies minHeight to the editor body (v4 minHeight, any CSS value)', async () => {
    const fixture = await render('body', '14rem');
    expect(editorEl(fixture).style.minHeight).toBe('14rem');
  });

  it('leaves the height content-driven by default (NOT v4\'s 12rem fallback)', async () => {
    const fixture = await render('body');
    // The P4.6al-adopted sites pass no minHeight and must stay pixel-unchanged,
    // so an unset input writes no style at all — see the component doc.
    expect(editorEl(fixture).style.minHeight).toBe('');
  });

  it('tracks a minHeight change without disturbing the content', async () => {
    const fixture = await render('body', '6rem');
    fixture.componentInstance.minHeight.set('10rem');
    fixture.detectChanges();
    expect(editorEl(fixture).style.minHeight).toBe('10rem');
    expect(markdown(fixture)).toBe('body');
  });
});

// ---------------------------------------------------------------------------
// List indent/outdent toolbar buttons (P4.D40, v4 item (f))
// ---------------------------------------------------------------------------

describe('MarkdownField — list indent/outdent buttons', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('the indent button nests the last item, the outdent button lifts it back (rich-text mode)', async () => {
    const fixture = await render('- alpha\n- beta\n- gamma');
    // Move the caret into the last item, the way a real caret placement would.
    richEditor(fixture).runCommand((state, dispatch) => {
      if (dispatch) dispatch(state.tr.setSelection(TextSelection.atEnd(state.doc)));
      return true;
    });

    button(fixture, 'indent').click();
    fixture.detectChanges();
    expect(markdown(fixture)).toBe('- alpha\n- beta\n  - gamma');

    button(fixture, 'outdent').click();
    fixture.detectChanges();
    expect(markdown(fixture)).toBe('- alpha\n- beta\n- gamma');
  });

  it('a toolbar indent click in source mode shifts the raw list line (v4 source branch)', async () => {
    const fixture = await render('- a\n- b');
    await toggleSource(fixture);
    const ta = sourceArea(fixture)!;
    ta.setSelectionRange(ta.value.length, ta.value.length); // caret on "- b"

    button(fixture, 'indent').click();
    fixture.detectChanges();
    expect(sourceArea(fixture)!.value).toBe('- a\n  - b');
    expect(fixture.componentInstance.last()).toBe('- a\n  - b');
  });
});

// ---------------------------------------------------------------------------
// The source-mode toggle (v4 MarkdownLexicalEditor `showSourceToggle`, :150)
// ---------------------------------------------------------------------------

describe('MarkdownField — the source-mode toggle', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('shows the toggle by DEFAULT — v4 defaults showSourceToggle true (:150)', async () => {
    const fixture = await render('body');
    expect(button(fixture, 'source')).not.toBeNull();
  });

  it('hides the toggle when the host opts out', async () => {
    TestBed.configureTestingModule({ imports: [Host] });
    const fixture = TestBed.createComponent(Host);
    fixture.componentInstance.showSourceToggle.set(false);
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();
    expect(button(fixture, 'source')).toBeNull();
  });

  it('carries v4\'s titles and aria-pressed through the swap (:437-439)', async () => {
    const fixture = await render('body');
    const btn = button(fixture, 'source');
    expect(btn.getAttribute('title')).toBe('Edit markdown source');
    expect(btn.getAttribute('aria-label')).toBe('Edit markdown source');
    expect(btn.getAttribute('aria-pressed')).toBe('false');

    await toggleSource(fixture);
    expect(btn.getAttribute('title')).toBe('Switch to rich text editor');
    expect(btn.getAttribute('aria-label')).toBe('Switch to rich text editor');
    expect(btn.getAttribute('aria-pressed')).toBe('true');
  });

  it('swaps the editor for a textarea seeded from the editor\'s own bytes', async () => {
    // `__bold__` normalizes to `**bold**` on load; source mode must show the
    // editor's serialization, not the raw input (v4's textarea shows `value`,
    // which its bridge keeps equal to the editor's export).
    const fixture = await render('__bold__');
    expect(sourceArea(fixture)).toBeNull();

    await toggleSource(fixture);
    expect(fixture.nativeElement.querySelector('qt-rich-editor')).toBeNull();
    expect(sourceArea(fixture)!.value).toBe('**bold**');
  });

  it('gives the textarea v4\'s aria-label and spellcheck-off (:222-224)', async () => {
    const fixture = await render('body');
    await toggleSource(fixture);
    const ta = sourceArea(fixture)!;
    expect(ta.getAttribute('aria-label')).toBe('Editor (markdown source)');
    expect(ta.getAttribute('spellcheck')).toBe('false');
  });

  it('emits every textarea keystroke (v4\'s controlled onChange)', async () => {
    const fixture = await render('body');
    await toggleSource(fixture);
    const ta = sourceArea(fixture)!;

    ta.value = 'body!';
    ta.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    expect(fixture.componentInstance.last()).toBe('body!');
  });

  it('hands the edited source back to the editor on leave', async () => {
    const fixture = await render('body');
    await toggleSource(fixture);
    const ta = sourceArea(fixture)!;
    ta.value = '## a heading';
    ta.dispatchEvent(new Event('input'));
    fixture.detectChanges();

    await toggleSource(fixture);
    expect(sourceArea(fixture)).toBeNull();
    expect(markdown(fixture)).toBe('## a heading');
  });

  it('does NOT emit the re-parse on leave — v4 tags it external-sync and skips it', async () => {
    const fixture = await render('body');
    await toggleSource(fixture);
    const ta = sourceArea(fixture)!;
    // Raw bytes the editor will normalize (`__x__` → `**x**`).
    ta.value = '__x__';
    ta.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    expect(fixture.componentInstance.last()).toBe('__x__');
    const emitsAfterTyping = fixture.componentInstance.emits();

    await toggleSource(fixture);
    // The editor re-inits and re-serializes to `**x**`, but the parent must keep
    // the raw bytes the textarea gave it — v4's remount conversion is tagged
    // `external-sync`, which its update listener skips.
    expect(markdown(fixture)).toBe('**x**');
    expect(fixture.componentInstance.last()).toBe('__x__');
    expect(fixture.componentInstance.emits()).toBe(emitsAfterTyping);
  });

  it('still emits a genuine edit after the round trip (absorb-once is not stuck)', async () => {
    const fixture = await render('body');
    await toggleSource(fixture);
    await toggleSource(fixture);
    button(fixture, 'h2').click();
    fixture.detectChanges();
    expect(fixture.componentInstance.last()).toBe('## body');
  });

  it('a toolbar button in source mode transforms the RAW text (v4 source branch)', async () => {
    const fixture = await render('hello');
    await toggleSource(fixture);
    const ta = sourceArea(fixture)!;
    ta.setSelectionRange(0, 5);

    button(fixture, 'bold').click();
    fixture.detectChanges();
    // v4's sourceToggleWrap: the raw markdown gains literal `**`, and the parent
    // sees it immediately (no rich-text round trip).
    expect(sourceArea(fixture)!.value).toBe('**hello**');
    expect(fixture.componentInstance.last()).toBe('**hello**');
  });

  it('an H1 click in source mode ADDS the prefix again (v4 never toggles it off)', async () => {
    const fixture = await render('# title');
    await toggleSource(fixture);
    button(fixture, 'h1').click();
    fixture.detectChanges();
    expect(sourceArea(fixture)!.value).toBe('# # title');
  });
});
