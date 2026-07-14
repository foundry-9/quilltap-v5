import { Component, signal, viewChild } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { MarkdownField } from './markdown-field';

@Component({
  imports: [MarkdownField],
  template: `<qt-markdown-field
    [value]="value()"
    (contentChange)="last.set($event)"
  />`,
})
class Host {
  readonly field = viewChild.required(MarkdownField);
  readonly value = signal('');
  readonly last = signal('');
}

async function render(initial = ''): Promise<ComponentFixture<Host>> {
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  fixture.componentInstance.value.set(initial);
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return fixture;
}

function button(fixture: ComponentFixture<Host>, typeClass: string): HTMLButtonElement {
  return fixture.nativeElement.querySelector(`.qt-formatting-button-${typeClass}`) as HTMLButtonElement;
}

/** The RichEditor content markdown, read via the field's editor handle. */
function markdown(fixture: ComponentFixture<Host>): string {
  return (fixture.componentInstance.field() as unknown as { editor: () => { getMarkdown(): string } })
    .editor()
    .getMarkdown();
}

describe('MarkdownField — shared form-field editor', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('renders the toolbar button inventory (v4 MARKDOWN_FORMATS + code block)', async () => {
    const fixture = await render('body');
    for (const t of ['bold', 'italic', 'h1', 'h2', 'h3', 'blockquote', 'code-block']) {
      expect(button(fixture, t), t).not.toBeNull();
    }
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
});
