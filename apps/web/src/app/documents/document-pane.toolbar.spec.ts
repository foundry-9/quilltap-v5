import { ComponentFixture, TestBed } from '@angular/core/testing';
import { selectAll } from 'prosemirror-commands';
import { By } from '@angular/platform-browser';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import oracle from '../editor/__fixtures__/delimiter-oracle.json';
import { FormattingToolbar } from '../editor/formatting-toolbar';
import { RichEditor } from '../editor/rich-editor';
import type { NarrationDelimiters, TemplateDelimiter } from '../core/core-contract';
import { DocumentPane } from './document-pane';
import type { ActiveDocument } from './document-api';
import type { OpenDocEntry } from './document-mode';

/**
 * The Document-Mode pane's formatting toolbar (P4.9L2) — v4
 * `DocumentPane.tsx:323-355, :686-693`.
 *
 * The button inventory, the delimiter list and every transform are already
 * byte-diffed against v4's REAL code elsewhere (`editor/
 * formatting-toolbar.delimiters.spec.ts`, `editor/delimiter-transforms.spec.ts`,
 * `editor/source-transforms.spec.ts`), all over the SAME recorded corpus this
 * file reads (`apps/web/oracle/delimiter-transforms.test.ts` → section D,
 * `delimiter-oracle.json`). What only this file can pin is THIS pane's wiring:
 *
 *  - the mount shape — a `.qt-doc-toolbar` row above the editor, for the files
 *    v4 wraps in a Lexical composer (markdown) and no others;
 *  - **the no-Nar shape.** v4's `DocToolbar` passes no `narrationDelimiters`,
 *    so the pane shows the template's own delimiters and NOT the synthesized
 *    narration button the composer shows for the very same template. The two
 *    recorded vectors are the proof: v4 rendered `['Th','As','OOC','Rank']` for
 *    that template with no narration and `['Nar', …]` with it;
 *  - the shared `showSource` signal — one signal, two controls;
 *  - the source branch acting on THIS pane's textarea, through the frontmatter
 *    recombine seam;
 *  - the delimiter split between the two hosts (the Salon passes the template's
 *    entries, the standalone view passes none — v4 passes no
 *    `roleplayTemplateId` there either).
 *
 * @module documents/document-pane.toolbar.spec
 */

interface ButtonListVector {
  name: string;
  template: TemplateDelimiter[];
  narration: NarrationDelimiters | null;
  buttons: { label: string; title: string; className: string }[];
  dividers: number;
}

const buttonLists = (oracle as unknown as { buttonLists: ButtonListVector[] }).buttonLists;

/** v4's rendering for THIS pane's shape: a template, and no narration passed. */
const NO_NARRATION = buttonLists.find((v) => v.narration === null && v.template.length > 0)!;
/** v4's rendering for a host that passes neither — the standalone view. */
const NEITHER = buttonLists.find((v) => v.narration === null && v.template.length === 0)!;
/** The SAME template as {@link NO_NARRATION}, with narration — the composer's shape. */
const WITH_NARRATION = buttonLists.find(
  (v) => v.narration === '*' && v.template.length === NO_NARRATION.template.length,
)!;

/** A plain wrap delimiter for the rich-branch routing beat. */
const WRAP_TILDE: TemplateDelimiter = {
  kind: 'wrap',
  name: 'Thoughts',
  buttonName: 'Th',
  delimiters: '~',
  style: 'qt-rp-thoughts',
};

function entry(
  over: Partial<ActiveDocument> = {},
  state: Partial<OpenDocEntry> = {},
): OpenDocEntry {
  const document: ActiveDocument = {
    id: over.id ?? 'd1',
    filePath: over.filePath ?? 'doc.md',
    scope: over.scope ?? 'project',
    mountPoint: over.mountPoint,
    displayTitle: over.displayTitle ?? 'My Notes',
    content: over.content ?? 'hello world',
    mtime: over.mtime,
  };
  return {
    document,
    isDirty: false,
    isSaving: false,
    isLLMEditing: false,
    contentVersion: 1,
    attentionTop: null,
    focusRequest: null,
    ...state,
  };
}

function render(
  e: OpenDocEntry,
  delimiters: readonly TemplateDelimiter[] = [],
): ComponentFixture<DocumentPane> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [DocumentPane] });
  const fixture = TestBed.createComponent(DocumentPane);
  fixture.componentRef.setInput('entry', e);
  fixture.componentRef.setInput('mode', 'split');
  fixture.componentRef.setInput('delimiters', delimiters);
  fixture.detectChanges();
  return fixture;
}

function root(fixture: ComponentFixture<DocumentPane>): HTMLElement {
  return fixture.nativeElement as HTMLElement;
}

function toolbar(fixture: ComponentFixture<DocumentPane>): FormattingToolbar {
  return fixture.debugElement.query(By.directive(FormattingToolbar))
    .componentInstance as FormattingToolbar;
}

function richEditor(fixture: ComponentFixture<DocumentPane>): RichEditor {
  return fixture.debugElement.query(By.directive(RichEditor)).componentInstance as RichEditor;
}

function delimiterLabels(fixture: ComponentFixture<DocumentPane>): string[] {
  return Array.from(
    root(fixture).querySelectorAll<HTMLButtonElement>('.qt-rp-annotation-button'),
  ).map((b) => b.textContent!.trim());
}

function sourceTextarea(fixture: ComponentFixture<DocumentPane>): HTMLTextAreaElement {
  const area = root(fixture).querySelector<HTMLTextAreaElement>('textarea');
  if (!area) throw new Error('source textarea not showing');
  return area;
}

/** The toolbar's own source toggle (v4's strings), not the header button. */
function toolbarSourceButton(fixture: ComponentFixture<DocumentPane>): HTMLButtonElement {
  return root(fixture).querySelector<HTMLButtonElement>('.qt-formatting-button-source')!;
}

function headerSourceButton(fixture: ComponentFixture<DocumentPane>): HTMLButtonElement {
  return root(fixture).querySelector<HTMLButtonElement>('.qt-doc-header-button[aria-pressed]')!;
}

async function settle(fixture: ComponentFixture<DocumentPane>): Promise<void> {
  await fixture.whenStable();
  await new Promise((r) => queueMicrotask(() => r(null)));
  fixture.detectChanges();
}

describe('DocumentPane — the formatting toolbar', () => {
  beforeEach(() => localStorage.clear());
  afterEach(() => TestBed.resetTestingModule());

  it('mounts the toolbar in a .qt-doc-toolbar row above the editor (markdown only)', () => {
    const fixture = render(entry({ filePath: 'doc.md', content: '# Heading' }));
    const row = root(fixture).querySelector('.qt-doc-toolbar');
    expect(row).not.toBeNull();
    expect(row!.querySelector('qt-formatting-toolbar')).not.toBeNull();
    // The markdown buttons, the indent controls, the code-block toggle, both
    // pickers and the source toggle all ride with it (v4's whole inventory).
    expect(root(fixture).querySelector('.qt-formatting-button-bold')).not.toBeNull();
    expect(root(fixture).querySelector('.qt-formatting-button-indent')).not.toBeNull();
    expect(root(fixture).querySelector('.qt-formatting-button-code-block')).not.toBeNull();
    expect(root(fixture).querySelector('button[aria-label="Insert emoji"]')).not.toBeNull();
    expect(root(fixture).querySelector('button[aria-label="Insert a symbol"]')).not.toBeNull();
    expect(toolbarSourceButton(fixture)).not.toBeNull();

    // Above the editor, as v4 places it.
    const editorHost = root(fixture).querySelector('qt-rich-editor')!;
    expect(
      row!.compareDocumentPosition(editorHost) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it('shows no toolbar for a non-markdown file (v4 wraps only markdown in Lexical)', () => {
    const fixture = render(entry({ filePath: 'data.json', content: '{"a":1}' }));
    expect(root(fixture).querySelector('.qt-doc-toolbar')).toBeNull();
    expect(root(fixture).querySelector('qt-formatting-toolbar')).toBeNull();
  });

  it('shows the template’s delimiter buttons exactly as v4 rendered them', () => {
    const fixture = render(entry({ filePath: 'doc.md' }), NO_NARRATION.template);
    const buttons = Array.from(
      root(fixture).querySelectorAll<HTMLButtonElement>('.qt-rp-annotation-button'),
    );
    expect(buttons.map((b) => b.textContent!.trim())).toEqual(
      NO_NARRATION.buttons.map((b) => b.label),
    );
    expect(buttons.map((b) => b.getAttribute('title'))).toEqual(
      NO_NARRATION.buttons.map((b) => b.title),
    );
  });

  /**
   * v4's `DocToolbar` passes no `narrationDelimiters`, so this pane cannot show
   * the synthesized "Nar" button the composer shows for the SAME template. The
   * two recorded vectors bracket it; the binding's absence is pinned directly on
   * the mounted toolbar, since a `[narrationDelimiters]` added to the mount
   * would be invisible to the label assertion alone.
   */
  it('never shows the synthesized narration button (v4 passes no narrationDelimiters)', () => {
    const fixture = render(entry({ filePath: 'doc.md' }), NO_NARRATION.template);
    expect(toolbar(fixture).narrationDelimiters()).toBeNull();
    expect(delimiterLabels(fixture)).toEqual(NO_NARRATION.buttons.map((b) => b.label));
    // The same template DOES get a "Nar" button when narration is passed — that
    // is the composer, and this vector is what makes the omission measurable.
    expect(WITH_NARRATION.buttons.map((b) => b.label)[0]).toBe('Nar');
    expect(delimiterLabels(fixture)).not.toContain('Nar');
  });

  it('a host that passes no delimiters gets no delimiter section (the standalone view)', () => {
    const fixture = render(entry({ filePath: 'doc.md' }));
    expect(NEITHER.buttons).toHaveLength(0);
    expect(delimiterLabels(fixture)).toEqual([]);
    // The markdown buttons are still there — only the delimiter rail is absent.
    expect(root(fixture).querySelector('.qt-formatting-button-bold')).not.toBeNull();
  });

  it('the toolbar disables with the pane while the LLM is editing (v4 disabled prop)', () => {
    const fixture = render(
      entry({ filePath: 'doc.md' }, { isLLMEditing: true }),
      NO_NARRATION.template,
    );
    const buttons = Array.from(
      root(fixture).querySelectorAll<HTMLButtonElement>(
        '.qt-formatting-button, .qt-rp-annotation-button',
      ),
    );
    expect(buttons.length).toBeGreaterThan(0);
    expect(buttons.every((b) => b.disabled)).toBe(true);
  });

  // --- the shared showSource signal ----------------------------------------

  it('the header button and the toolbar toggle drive ONE signal, never out of step', () => {
    const fixture = render(entry({ filePath: 'doc.md', content: '# Heading' }));
    expect(toolbar(fixture).showSource()).toBe(false);
    expect(headerSourceButton(fixture).getAttribute('aria-pressed')).toBe('false');

    // Toolbar → header.
    toolbarSourceButton(fixture).click();
    fixture.detectChanges();
    expect(toolbar(fixture).showSource()).toBe(true);
    expect(headerSourceButton(fixture).getAttribute('aria-pressed')).toBe('true');
    expect(root(fixture).querySelector('textarea')).not.toBeNull();

    // Header → toolbar. One signal: the second control returns it, it does not
    // open a view of its own.
    headerSourceButton(fixture).click();
    fixture.detectChanges();
    expect(toolbar(fixture).showSource()).toBe(false);
    expect(toolbarSourceButton(fixture).getAttribute('aria-pressed')).toBe('false');
    expect(root(fixture).querySelector('textarea')).toBeNull();
  });

  /**
   * v4's `toggleSourceMode` flushes on the way OUT only. Both controls are
   * asserted: a header button wired straight to the signal keeps the two views
   * in agreement — and silently drops the flush, which is the whole reason
   * there is one handler.
   */
  it('leaving source mode flushes first, entering does not — from EITHER control', () => {
    const fixture = render(entry({ filePath: 'doc.md', content: '# Heading' }));
    const flushes: number[] = [];
    fixture.componentInstance.blur.subscribe(() => flushes.push(1));

    toolbarSourceButton(fixture).click();
    fixture.detectChanges();
    expect(flushes).toEqual([]); // entering source mode flushes nothing

    toolbarSourceButton(fixture).click();
    fixture.detectChanges();
    expect(flushes).toEqual([1]); // leaving flushes the raw bytes first

    // The header control takes the identical road, in both directions.
    headerSourceButton(fixture).click();
    fixture.detectChanges();
    expect(flushes).toEqual([1]);

    headerSourceButton(fixture).click();
    fixture.detectChanges();
    expect(flushes).toEqual([1, 1]);
  });

  // --- the source branch ----------------------------------------------------

  it('a markdown button in source mode transforms THIS pane’s textarea', () => {
    const md = '---\ntitle: Deep\n---\nold body';
    const fixture = render(entry({ filePath: 'doc.md', content: md }));
    const emitted: string[] = [];
    fixture.componentInstance.contentChange.subscribe((c) => emitted.push(c));

    toolbarSourceButton(fixture).click();
    fixture.detectChanges();
    const area = sourceTextarea(fixture);
    expect(area.value).toBe('old body');
    area.setSelectionRange(0, 3);

    root(fixture).querySelector<HTMLButtonElement>('.qt-formatting-button-bold')!.click();
    fixture.detectChanges();

    expect(sourceTextarea(fixture).value).toBe('**old** body');
    // …and the emit rides the pane's own recombine seam, so the frontmatter
    // block is still on the front of the bytes the host saves.
    expect(emitted).toEqual(['---\ntitle: Deep\n---\n**old** body']);
  });

  it('a delimiter button in source mode transforms the textarea too', () => {
    const fixture = render(entry({ filePath: 'doc.md', content: 'a note' }), NO_NARRATION.template);
    const emitted: string[] = [];
    fixture.componentInstance.contentChange.subscribe((c) => emitted.push(c));

    toolbarSourceButton(fixture).click();
    fixture.detectChanges();
    sourceTextarea(fixture).setSelectionRange(0, 6);

    const first = root(fixture).querySelector<HTMLButtonElement>('.qt-rp-annotation-button')!;
    first.click();
    fixture.detectChanges();

    // The bytes are whatever v4's transform produces for that delimiter — the
    // corpus proves the transform; here it is the ROUTING that is under test.
    expect(sourceTextarea(fixture).value).not.toBe('a note');
    expect(emitted).toEqual([sourceTextarea(fixture).value]);
  });

  it('a markdown button in rich mode reaches the EDITOR, not a textarea', async () => {
    const fixture = render(entry({ filePath: 'doc.md', content: 'shout' }));
    await settle(fixture);

    // Rich mode has no textarea at all — the source branch cannot be the one
    // that ran, so the bold marks below can only have come from the editor.
    expect(root(fixture).querySelector('textarea')).toBeNull();

    richEditor(fixture).setMarkdown('shout');
    await settle(fixture);
    richEditor(fixture).runCommand(selectAll);

    root(fixture).querySelector<HTMLButtonElement>('.qt-formatting-button-bold')!.click();
    await settle(fixture);

    expect(richEditor(fixture).getMarkdown()).toBe('**shout**');
  });

  it('a delimiter button in rich mode reaches the editor too', async () => {
    const fixture = render(entry({ filePath: 'doc.md', content: 'she smiled' }), [WRAP_TILDE]);
    await settle(fixture);
    richEditor(fixture).setMarkdown('she smiled');
    await settle(fixture);
    richEditor(fixture).runCommand(selectAll);

    root(fixture).querySelector<HTMLButtonElement>('.qt-rp-annotation-button')!.click();
    await settle(fixture);

    expect(richEditor(fixture).getMarkdown()).toBe('~she smiled~');
  });
});
