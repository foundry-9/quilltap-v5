import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { DocumentPane } from './document-pane';
import type { ActiveDocument } from './document-api';
import type { OpenDocEntry } from './document-mode';

function entry(over: Partial<ActiveDocument> = {}, state: Partial<OpenDocEntry> = {}): OpenDocEntry {
  const document: ActiveDocument = {
    id: over.id ?? 'd1',
    filePath: over.filePath ?? 'notes.txt',
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

function render(e: OpenDocEntry): ComponentFixture<DocumentPane> {
  // Reset so a spec that renders more than once doesn't hit an already-
  // instantiated test module (the P4.6t multi-render lesson).
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [DocumentPane] });
  const fixture = TestBed.createComponent(DocumentPane);
  fixture.componentRef.setInput('entry', e);
  fixture.componentRef.setInput('mode', 'split');
  fixture.detectChanges();
  return fixture;
}

describe('DocumentPane', () => {
  it('renders the title, the qtap:// URL, and the plain-text status', () => {
    const fixture = render(entry());
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('My Notes');
    expect(text).toContain('qtap://project/notes.txt');
    expect(text).toContain('Plain text');
    expect(text).toContain('2 words');
    expect(text).toContain('Saved');
  });

  it('reflects the save status (Unsaved / Saving)', () => {
    expect(render(entry({}, { isDirty: true })).nativeElement.textContent).toContain('Unsaved');
    expect(render(entry({}, { isSaving: true })).nativeElement.textContent).toContain('Saving...');
  });

  it('a non-markdown textarea holds the whole content', () => {
    const fixture = render(entry({ filePath: 'data.json', content: '{"a":1}' }));
    const textarea = fixture.nativeElement.querySelector('textarea') as HTMLTextAreaElement;
    expect(textarea.value).toBe('{"a":1}');
    expect(fixture.nativeElement.textContent).not.toContain('Document Info');
  });

  it('a markdown file shows the frontmatter table and edits only the body', () => {
    const md = '---\ntitle: Deep\ntags: [a, b]\n---\n# Heading\nBody text here.';
    const fixture = render(entry({ filePath: 'doc.md', displayTitle: 'Deep', content: md }));
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Document Info');
    expect(text).toContain('title');
    // chips from the tags array
    expect(text).toContain('a');
    expect(text).toContain('b');
    // Markdown status + word count over the BODY only
    expect(text).toContain('Markdown');
    const textarea = fixture.nativeElement.querySelector('textarea') as HTMLTextAreaElement;
    expect(textarea.value).toBe('# Heading\nBody text here.');
  });

  it('markdown edits recombine the frontmatter block before emitting', () => {
    const md = '---\ntitle: Deep\n---\nold body';
    const fixture = render(entry({ filePath: 'doc.md', content: md }));
    const emitted: string[] = [];
    fixture.componentInstance.contentChange.subscribe((c) => emitted.push(c));
    const textarea = fixture.nativeElement.querySelector('textarea') as HTMLTextAreaElement;
    textarea.value = 'new body';
    textarea.dispatchEvent(new Event('input'));
    expect(emitted).toEqual(['---\ntitle: Deep\n---\nnew body']);
  });

  it('editing the title and pressing Enter emits a rename', () => {
    const fixture = render(entry({ displayTitle: 'Old' }));
    const emitted: string[] = [];
    fixture.componentInstance.rename.subscribe((t) => emitted.push(t));

    fixture.nativeElement.querySelector('.qt-doc-title').dispatchEvent(new Event('click'));
    fixture.detectChanges();
    const input = fixture.nativeElement.querySelector('.qt-doc-title-input') as HTMLInputElement;
    input.value = 'New Title';
    input.dispatchEvent(new Event('input'));
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    expect(emitted).toEqual(['New Title']);
  });

  it('the close button emits close', () => {
    const fixture = render(entry());
    const emitted: number[] = [];
    fixture.componentInstance.close.subscribe(() => emitted.push(1));
    const closeBtn = fixture.nativeElement.querySelector(
      '[aria-label="Exit document mode"]',
    ) as HTMLButtonElement;
    closeBtn.click();
    expect(emitted).toEqual([1]);
  });

  it('disables the editor while the LLM is editing', () => {
    const fixture = render(entry({}, { isLLMEditing: true }));
    const textarea = fixture.nativeElement.querySelector('textarea') as HTMLTextAreaElement;
    expect(textarea.disabled).toBe(true);
    expect(fixture.nativeElement.textContent).toContain('AI editing...');
  });

  it('emits blur when focus leaves the editor (focusout bubbles; blur does not)', () => {
    // The flush-on-blur save rides this output. A container `(blur)` listener
    // never fires in a real browser — the wiring must be `(focusout)`.
    const fixture = render(entry());
    const emitted: number[] = [];
    fixture.componentInstance.blur.subscribe(() => emitted.push(1));
    const textarea = fixture.nativeElement.querySelector('textarea') as HTMLTextAreaElement;
    textarea.dispatchEvent(new FocusEvent('focusout', { bubbles: true }));
    expect(emitted).toEqual([1]);
  });
});
