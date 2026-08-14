import { ComponentFixture, TestBed } from '@angular/core/testing';
import { selectAll } from 'prosemirror-commands';
import { By } from '@angular/platform-browser';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { TemplateDelimiter } from '../core/core-contract';
import { RichEditor } from '../editor/rich-editor';
import { ChatComposer, type ComposerSend } from './chat-composer';

/**
 * The composer's formatting toolbar (P4.9L) — v4 `ChatComposer.tsx:320-345`.
 *
 * The transforms themselves are already differential-tested against v4's real
 * code (`editor/delimiter-transforms.spec.ts`, `editor/source-transforms.spec
 * .ts`). What this file pins is the WIRING that lane cannot see: that the
 * toolbar appears only in composition mode, that a delimiter button reaches the
 * editor and comes back out as the sent bytes, that the source toggle
 * round-trips through the same bytes, and that a source-mode button transforms
 * the textarea rather than the hidden editor.
 *
 * @module chat/chat-composer.toolbar.spec
 */

beforeEach(() => localStorage.clear());

const emptyRosterClient = {
  dispatchData: vi.fn(async () => ({ tools: [], errors: [] })),
} as unknown as CoreClient;

const THOUGHTS: TemplateDelimiter = {
  kind: 'wrap',
  name: 'Thoughts',
  buttonName: 'Th',
  delimiters: '~',
  style: 'qt-rp-thoughts',
};
const OOC: TemplateDelimiter = {
  kind: 'linePrefix',
  name: 'Out of character',
  buttonName: 'OOC',
  marker: '// ',
  style: 'qt-rp-ooc',
};

function render(inputs: Record<string, unknown> = {}): ComponentFixture<ChatComposer> {
  TestBed.configureTestingModule({
    imports: [ChatComposer],
    providers: [
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: emptyRosterClient },
    ],
  });
  const fixture = TestBed.createComponent(ChatComposer);
  fixture.componentRef.setInput('chatId', 'chat-1');
  for (const [key, value] of Object.entries(inputs)) {
    fixture.componentRef.setInput(key, value);
  }
  fixture.detectChanges();
  return fixture;
}

async function settle(fixture: ComponentFixture<ChatComposer>): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

function editor(fixture: ComponentFixture<ChatComposer>): RichEditor {
  return fixture.debugElement.query(By.directive(RichEditor)).componentInstance as RichEditor;
}

function root(fixture: ComponentFixture<ChatComposer>): HTMLElement {
  return fixture.nativeElement as HTMLElement;
}

function delimiterButton(fixture: ComponentFixture<ChatComposer>, title: string): HTMLButtonElement {
  const button = root(fixture).querySelector<HTMLButtonElement>(
    `.qt-rp-annotation-button[title="${title}"]`,
  );
  if (!button) throw new Error(`no delimiter button titled ${title}`);
  return button;
}

function sourceTextarea(fixture: ComponentFixture<ChatComposer>): HTMLTextAreaElement {
  const area = root(fixture).querySelector<HTMLTextAreaElement>('.qt-source-mode-textarea');
  if (!area) throw new Error('source textarea not showing');
  return area;
}

describe('ChatComposer — the formatting toolbar', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    TestBed.resetTestingModule();
  });

  it('shows the toolbar only in composition mode (v4 documentEditingMode)', async () => {
    const fixture = render();
    await settle(fixture);
    expect(root(fixture).querySelector('qt-formatting-toolbar')).toBeNull();

    fixture.componentRef.setInput('compositionMode', true);
    fixture.detectChanges();
    expect(root(fixture).querySelector('qt-formatting-toolbar')).not.toBeNull();
    // The markdown buttons and both pickers ride with it.
    expect(root(fixture).querySelector('.qt-formatting-button-bold')).not.toBeNull();
    expect(root(fixture).querySelector('button[aria-label="Insert emoji"]')).not.toBeNull();
  });

  it('shows the template’s delimiter buttons, narration first', async () => {
    const fixture = render({
      compositionMode: true,
      templateDelimiters: [THOUGHTS, OOC],
      narrationDelimiters: '*',
    });
    await settle(fixture);

    const labels = Array.from(
      root(fixture).querySelectorAll<HTMLButtonElement>('.qt-rp-annotation-button'),
    ).map((b) => b.textContent!.trim());
    expect(labels).toEqual(['Nar', 'Th', 'OOC']);
  });

  it('renders no delimiter section for a chat with no roleplay template', async () => {
    const fixture = render({ compositionMode: true });
    await settle(fixture);
    expect(root(fixture).querySelectorAll('.qt-rp-annotation-button')).toHaveLength(0);
  });

  it('a delimiter button reaches the WIRE: the sent bytes carry the marks', async () => {
    const fixture = render({
      compositionMode: true,
      templateDelimiters: [THOUGHTS],
      narrationDelimiters: '*',
    });
    await settle(fixture);

    editor(fixture).setMarkdown('she smiled');
    await settle(fixture);
    // Select the whole line, as a writer would before pressing a delimiter.
    editor(fixture).runCommand(selectAll);

    delimiterButton(fixture, 'Narration (*...*)').click();
    await settle(fixture);

    const sent: ComposerSend[] = [];
    fixture.componentInstance.send.subscribe((e) => sent.push(e));
    fixture.componentInstance['submit']();

    expect(sent).toHaveLength(1);
    expect(sent[0].content).toBe('*she smiled*');
  });

  it('a markdown button reaches the wire too (the shared command map)', async () => {
    const fixture = render({ compositionMode: true });
    await settle(fixture);

    editor(fixture).setMarkdown('shout');
    await settle(fixture);
    editor(fixture).runCommand(selectAll);

    root(fixture).querySelector<HTMLButtonElement>('.qt-formatting-button-bold')!.click();
    await settle(fixture);

    const sent: ComposerSend[] = [];
    fixture.componentInstance.send.subscribe((e) => sent.push(e));
    fixture.componentInstance['submit']();
    expect(sent[0].content).toBe('**shout**');
  });

  it('the source toggle round-trips the same bytes both ways', async () => {
    const fixture = render({ compositionMode: true });
    await settle(fixture);

    editor(fixture).setMarkdown('# Heading');
    await settle(fixture);

    // Entering seeds the textarea from the EDITOR's own serialization.
    root(fixture).querySelector<HTMLButtonElement>('.qt-formatting-button-source')!.click();
    fixture.detectChanges();
    expect(sourceTextarea(fixture).value).toBe('# Heading');
    // v4 keeps the editor mounted-but-hidden to preserve undo history.
    const host = root(fixture).querySelector<HTMLElement>('qt-rich-editor')!;
    expect(host.style.display).toBe('none');

    // An edit in the textarea comes back into the editor on the way out.
    const area = sourceTextarea(fixture);
    area.value = '# Heading\n\nAnd prose.';
    area.dispatchEvent(new Event('input'));
    fixture.detectChanges();

    root(fixture).querySelector<HTMLButtonElement>('.qt-formatting-button-source')!.click();
    await settle(fixture);
    expect(root(fixture).querySelector('.qt-source-mode-textarea')).toBeNull();
    expect(editor(fixture).getMarkdown()).toBe('# Heading\n\nAnd prose.');
  });

  it('a source-mode delimiter transforms the TEXTAREA, not the hidden editor', async () => {
    const fixture = render({
      compositionMode: true,
      templateDelimiters: [OOC],
      narrationDelimiters: '*',
    });
    await settle(fixture);

    editor(fixture).setMarkdown('a note');
    await settle(fixture);
    root(fixture).querySelector<HTMLButtonElement>('.qt-formatting-button-source')!.click();
    fixture.detectChanges();

    const area = sourceTextarea(fixture);
    area.setSelectionRange(3, 3);
    delimiterButton(fixture, 'Out of character (// …EOL)').click();
    fixture.detectChanges();

    expect(sourceTextarea(fixture).value).toBe('// a note');

    // And the SEND takes the bytes the writer can see — v5's ruled divergence
    // from v4, which reads its (suspend-synced) Lexical handle here and so
    // discards every source edit.
    const sent: ComposerSend[] = [];
    fixture.componentInstance.send.subscribe((e) => sent.push(e));
    fixture.componentInstance['submit']();
    expect(sent[0].content).toBe('// a note');
  });
});
