import { Component, signal, viewChild } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { RichEditor } from './rich-editor';

@Component({
  imports: [RichEditor],
  template: `<qt-rich-editor
    [value]="value()"
    [submitOnEnter]="submitOnEnter()"
    [submitOnModEnter]="submitOnModEnter()"
    (contentChange)="last.set($event)"
    (submit)="submitted.set(submitted() + 1)"
  />`,
})
class Host {
  readonly editor = viewChild.required(RichEditor);
  readonly value = signal('');
  readonly submitOnEnter = signal(false);
  readonly submitOnModEnter = signal(false);
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

/** prosemirror-keymap's own Mac detection — decides whether `Mod` is Cmd/Ctrl. */
const IS_MAC = typeof navigator !== 'undefined' && /Mac|iP(hone|[oa]d)/.test(navigator.platform);

/** Dispatch an Enter keydown on the ProseMirror content element. */
function pressEnter(fixture: ComponentFixture<Host>, mods: KeyboardEventInit = {}): void {
  const el = fixture.nativeElement.querySelector('.qt-rich-editor-content') as HTMLElement;
  el.dispatchEvent(
    new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true, ...mods }),
  );
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

  // --- submit modes (v4 KeyboardPlugin branches) ----------------------------

  it('chat mode: plain Enter emits submit', async () => {
    const fixture = await render();
    fixture.componentInstance.submitOnEnter.set(true);
    fixture.detectChanges();
    pressEnter(fixture);
    expect(fixture.componentInstance.submitted()).toBe(1);
  });

  it('composition mode: plain Enter does NOT submit (inserts a paragraph)', async () => {
    const fixture = await render();
    // submitOnEnter=false, submitOnModEnter=true — the composition posture.
    fixture.componentInstance.submitOnModEnter.set(true);
    fixture.detectChanges();
    pressEnter(fixture);
    expect(fixture.componentInstance.submitted()).toBe(0);
  });

  it('composition mode: Cmd/Ctrl+Enter emits submit (v4 isMac branch)', async () => {
    const fixture = await render();
    fixture.componentInstance.submitOnModEnter.set(true);
    fixture.detectChanges();
    pressEnter(fixture, IS_MAC ? { metaKey: true } : { ctrlKey: true });
    expect(fixture.componentInstance.submitted()).toBe(1);
  });

  it('chat mode: Cmd/Ctrl+Enter does NOT submit (submitOnModEnter off)', async () => {
    const fixture = await render();
    fixture.componentInstance.submitOnEnter.set(true);
    fixture.detectChanges();
    pressEnter(fixture, IS_MAC ? { metaKey: true } : { ctrlKey: true });
    expect(fixture.componentInstance.submitted()).toBe(0);
  });
});
