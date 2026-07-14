import { Component, signal, viewChild } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { RichEditor } from './rich-editor';

@Component({
  imports: [RichEditor],
  template: `<qt-rich-editor
    [value]="value()"
    [submitOnEnter]="submitOnEnter()"
    (contentChange)="last.set($event)"
    (submit)="submitted.set(submitted() + 1)"
  />`,
})
class Host {
  readonly editor = viewChild.required(RichEditor);
  readonly value = signal('');
  readonly submitOnEnter = signal(false);
  readonly last = signal('');
  readonly submitted = signal(0);
}

async function render(): Promise<ComponentFixture<Host>> {
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return fixture;
}

describe('RichEditor — the ProseMirror handle', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('round-trips markdown set via the handle back out through getMarkdown', async () => {
    const fixture = await render();
    const editor = fixture.componentInstance.editor();
    editor.setMarkdown('an _italic_ and **bold** word');
    expect(editor.getMarkdown()).toBe('an _italic_ and **bold** word');
  });

  it('keeps a single-star narration literal (v4 dialect)', async () => {
    const fixture = await render();
    const editor = fixture.componentInstance.editor();
    editor.setMarkdown('*She turns away.*');
    expect(editor.getMarkdown()).toBe('*She turns away.*');
  });

  it('an external value change loads and normalizes once (absorb-once seam)', async () => {
    const fixture = await render();
    fixture.componentInstance.value.set('__bold__');
    fixture.detectChanges();
    await fixture.whenStable();
    await new Promise((r) => queueMicrotask(() => r(null)));
    fixture.detectChanges();
    // v4 normalizes __bold__ → **bold**; the editor emits the normalized form.
    expect(fixture.componentInstance.editor().getMarkdown()).toBe('**bold**');
    expect(fixture.componentInstance.last()).toBe('**bold**');
  });

  it('prependText inserts above the existing content', async () => {
    const fixture = await render();
    const editor = fixture.componentInstance.editor();
    editor.setMarkdown('the body');
    editor.prependText('_Outfit changed._');
    expect(editor.getMarkdown()).toBe('_Outfit changed._\n\nthe body');
  });
});
