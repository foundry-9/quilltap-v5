import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { TextSelection } from 'prosemirror-state';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ChatComposer } from '../../chat/chat-composer';
import { CoreClient } from '../../core/core-client';
import { DocumentPane } from '../../documents/document-pane';
import type { ActiveDocument } from '../../documents/document-api';
import type { OpenDocEntry } from '../../documents/document-mode';
import '../jsdom-range-shim';
import { RichEditor } from '../rich-editor';
import { EMOJI_PROFILE } from './profiles/emoji';
import { UNICODE_PROFILE } from './profiles/unicode';

/**
 * The wiring: which hosts mount the typeaheads, where the toggles come from,
 * and the picker's insert path on the editor handle.
 *
 * v4 mounts `CharTypeaheadPlugin` in exactly two places —
 * `LexicalComposerWrapper` (the Salon composer) and `DocumentPane` — and the
 * plugin reads `chat_settings` itself. This suite pins that inventory: two
 * hosts on, every form field off.
 *
 * @module editor/char-insert/char-insert-wiring.spec
 */

function settingsClient(settings: Record<string, unknown>): CoreClient {
  return {
    dispatchExpect: vi.fn(async () => ({ type: 'chatSettings', data: settings })),
    dispatchData: vi.fn(async () => ({ tools: [], errors: [] })),
  } as unknown as CoreClient;
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

describe('the typeahead mounts — v4 mounts in TWO hosts, and only two', () => {
  afterEach(() => TestBed.resetTestingModule());
  beforeEach(() => localStorage.clear());

  function composer(settings: Record<string, unknown>): ComponentFixture<ChatComposer> {
    TestBed.configureTestingModule({
      imports: [ChatComposer],
      providers: [
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: settingsClient(settings) },
      ],
    });
    const fixture = TestBed.createComponent(ChatComposer);
    fixture.componentRef.setInput('chatId', 'chat-1');
    fixture.detectChanges();
    return fixture;
  }

  function editorOf(fixture: ComponentFixture<unknown>): RichEditor {
    return fixture.debugElement.query(By.directive(RichEditor)).componentInstance as RichEditor;
  }

  it('turns BOTH typeaheads on in the composer when the settings row is unset (v4 `?? true`)', async () => {
    const fixture = composer({});
    await settle(fixture);

    const editor = editorOf(fixture);
    expect(editor.composerEmoji()).toBe(true);
    expect(editor.composerUnicode()).toBe(true);
  });

  it('honours an explicit false on each key independently', async () => {
    const fixture = composer({ composerEmoji: false, composerUnicode: true });
    await settle(fixture);

    const editor = editorOf(fixture);
    expect(editor.composerEmoji()).toBe(false);
    expect(editor.composerUnicode()).toBe(true);
  });

  it('mounts them in the Document-Mode pane too', async () => {
    const document: ActiveDocument = {
      id: 'd1',
      filePath: 'notes.md',
      scope: 'project',
      displayTitle: 'Notes',
      content: 'hello',
    };
    const entry: OpenDocEntry = {
      document,
      isDirty: false,
      isSaving: false,
      isLLMEditing: false,
      contentVersion: 1,
      attentionTop: null,
      focusRequest: null,
    };

    TestBed.configureTestingModule({
      imports: [DocumentPane],
      providers: [
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: settingsClient({ composerUnicode: false }) },
      ],
    });
    const fixture = TestBed.createComponent(DocumentPane);
    fixture.componentRef.setInput('entry', entry);
    fixture.componentRef.setInput('mode', 'split');
    fixture.detectChanges();
    await settle(fixture);

    const editor = editorOf(fixture);
    expect(editor.composerEmoji()).toBe(true);
    expect(editor.composerUnicode()).toBe(false);
  });

  it('leaves a bare editor — every form field — with both OFF', () => {
    TestBed.configureTestingModule({ imports: [RichEditor] });
    const fixture = TestBed.createComponent(RichEditor);
    fixture.detectChanges();

    expect(fixture.componentInstance.composerEmoji()).toBe(false);
    expect(fixture.componentInstance.composerUnicode()).toBe(false);
  });
});

describe('the picker insert path on the editor handle', () => {
  afterEach(() => TestBed.resetTestingModule());
  beforeEach(() => localStorage.clear());

  function bareEditor(): ComponentFixture<RichEditor> {
    TestBed.configureTestingModule({ imports: [RichEditor] });
    const fixture = TestBed.createComponent(RichEditor);
    // No trailing space in the seed: markdown round-trips it away, and the
    // contract under test is that the INSERT adds none of its own.
    fixture.componentRef.setInput('value', 'hi');
    fixture.detectChanges();
    return fixture;
  }

  /** A fresh view's caret sits at the document start; put it where a writer's is. */
  function caretToEnd(fixture: ComponentFixture<RichEditor>): void {
    fixture.componentInstance.runCommand((state, dispatch) => {
      dispatch?.(state.tr.setSelection(TextSelection.atEnd(state.doc)));
      return true;
    });
  }

  it('inserts at the caret with NO trailing space and records a recent', async () => {
    const fixture = bareEditor();
    await settle(fixture);
    caretToEnd(fixture);

    fixture.componentInstance.insertChar(EMOJI_PROFILE, '😄');

    // `word😄`, not `word 😄 ` — an inserted character is punctuation-adjacent.
    expect(fixture.componentInstance.getMarkdown()).toBe('hi😄');
    expect(JSON.parse(localStorage.getItem(EMOJI_PROFILE.recentsStorageKey)!)).toEqual(['😄']);
  });

  it('keeps the two profiles` recents lists apart', async () => {
    const fixture = bareEditor();
    await settle(fixture);
    caretToEnd(fixture);

    fixture.componentInstance.insertChar(UNICODE_PROFILE, '→');

    expect(JSON.parse(localStorage.getItem(UNICODE_PROFILE.recentsStorageKey)!)).toEqual(['→']);
    expect(localStorage.getItem(EMOJI_PROFILE.recentsStorageKey)).toBeNull();
  });

  it('works with the typeahead toggles OFF — the flags govern the trigger only', async () => {
    const fixture = bareEditor();
    await settle(fixture);
    caretToEnd(fixture);
    expect(fixture.componentInstance.composerEmoji()).toBe(false);

    fixture.componentInstance.insertChar(EMOJI_PROFILE, '😄');

    expect(fixture.componentInstance.getMarkdown()).toBe('hi😄');
  });
});
