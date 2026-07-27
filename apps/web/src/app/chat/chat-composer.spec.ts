import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { RichEditor } from '../editor/rich-editor';
import { compileRules } from '../editor/text-replacement';
import { ChatComposer, type ComposerSend, type PendingToolResultChip } from './chat-composer';

// Draft persistence keys off chatId in localStorage; keep every test isolated.
beforeEach(() => localStorage.clear());

function jsonResponse(body: unknown, ok = true, status = 200): Response {
  return { ok, status, json: async () => body } as unknown as Response;
}

/** The embedded custom-tools popup dispatches a roster on mount; an empty one
 *  keeps its button hidden so it never interferes with the composer's own tests. */
const emptyRosterClient = {
  dispatchData: vi.fn(async () => ({ tools: [], errors: [] })),
} as unknown as CoreClient;

function render(): ComponentFixture<ChatComposer> {
  TestBed.configureTestingModule({
    imports: [ChatComposer],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: emptyRosterClient }],
  });
  const fixture = TestBed.createComponent(ChatComposer);
  fixture.componentRef.setInput('chatId', 'chat-1');
  fixture.detectChanges();
  return fixture;
}

async function settle(fixture: ComponentFixture<ChatComposer>): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

function richEditor(fixture: ComponentFixture<ChatComposer>): RichEditor {
  return fixture.debugElement.query(By.directive(RichEditor)).componentInstance as RichEditor;
}

function pickFile(fixture: ComponentFixture<ChatComposer>, name = 'pic.png'): void {
  const input = fixture.nativeElement.querySelector('input[type="file"]') as HTMLInputElement;
  const file = new File([new Uint8Array([1, 2, 3])], name, { type: 'image/png' });
  Object.defineProperty(input, 'files', { value: [file], configurable: true });
  input.dispatchEvent(new Event('change'));
}

const UPLOADED = {
  file: { id: 'f-1', filename: 'pic.png', filepath: 'chat/pic.png', mimeType: 'image/png', url: '/api/v1/files/f-1' },
};

describe('ChatComposer — attach affordance', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    TestBed.resetTestingModule();
  });

  it('uploads a picked file and shows a chip', async () => {
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(UPLOADED));
    vi.stubGlobal('fetch', fetchMock);
    const fixture = render();
    pickFile(fixture);
    await settle(fixture);
    expect(fetchMock).toHaveBeenCalledWith(
      '/api/v1/chats/chat-1/files',
      expect.objectContaining({ method: 'POST' }),
    );
    expect(fixture.nativeElement.querySelector('.qt-chat-attachment-chip')).not.toBeNull();
    expect(fixture.nativeElement.textContent).toContain('pic.png');
  });

  it('sends the markdown read from the editor handle plus file ids, then clears', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(UPLOADED)));
    const fixture = render();
    await settle(fixture);
    let sent: ComposerSend | undefined;
    fixture.componentInstance.send.subscribe((e) => (sent = e));
    pickFile(fixture);
    await settle(fixture);
    richEditor(fixture).setMarkdown('look at this');
    await settle(fixture);
    fixture.nativeElement.querySelector('form').dispatchEvent(new Event('submit'));
    expect(sent).toEqual({ content: 'look at this', fileIds: ['f-1'] });
    // Chips + editor cleared after send.
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('.qt-chat-attachment-chip')).toBeNull();
    expect(richEditor(fixture).getMarkdown()).toBe('');
  });

  it('keeps a user-typed *narration* literal in the sent content (v4 dialect)', async () => {
    vi.stubGlobal('fetch', vi.fn());
    const fixture = render();
    await settle(fixture);
    let sent: ComposerSend | undefined;
    fixture.componentInstance.send.subscribe((e) => (sent = e));
    richEditor(fixture).setMarkdown('*She waves.* Then _softly_ speaks.');
    await settle(fixture);
    fixture.nativeElement.querySelector('form').dispatchEvent(new Event('submit'));
    expect(sent?.content).toBe('*She waves.* Then _softly_ speaks.');
  });

  it('enables send with an attachment and no text', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(UPLOADED)));
    const fixture = render();
    const sendBtn = () => fixture.nativeElement.querySelector('.qt-chat-composer-send') as HTMLButtonElement;
    expect(sendBtn().disabled).toBe(true);
    pickFile(fixture);
    await settle(fixture);
    expect(sendBtn().disabled).toBe(false);
  });

  it('removes an attached chip', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(UPLOADED)));
    const fixture = render();
    pickFile(fixture);
    await settle(fixture);
    (fixture.nativeElement.querySelector('.qt-chat-attachment-chip-remove') as HTMLButtonElement).click();
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('.qt-chat-attachment-chip')).toBeNull();
  });

  it('composition toggle: bindings, active state, title, and emit (v4 documentEditingMode)', async () => {
    vi.stubGlobal('fetch', vi.fn());
    const fixture = render();
    await settle(fixture);
    const toggle = () =>
      fixture.nativeElement.querySelector(
        'button[aria-label="Toggle composition mode"]',
      ) as HTMLButtonElement;

    // Default (chat mode): Enter sends, mod-enter off; toggle inactive.
    expect(richEditor(fixture).submitOnEnter()).toBe(true);
    expect(richEditor(fixture).submitOnModEnter()).toBe(false);
    expect(toggle().classList.contains('qt-chat-toolbar-button-active')).toBe(false);
    expect(toggle().getAttribute('title')).toContain('Switch to composition mode');

    // A click emits the flipped value; the composer does not self-persist.
    let emitted: boolean | undefined;
    fixture.componentInstance.compositionModeChange.subscribe((v) => (emitted = v));
    toggle().click();
    expect(emitted).toBe(true);

    // Salon drives it back in → bindings flip, button active, title updates.
    fixture.componentRef.setInput('compositionMode', true);
    fixture.detectChanges();
    expect(richEditor(fixture).submitOnEnter()).toBe(false);
    expect(richEditor(fixture).submitOnModEnter()).toBe(true);
    expect(toggle().classList.contains('qt-chat-toolbar-button-active')).toBe(true);
    expect(toggle().getAttribute('title')).toBe('Switch to chat mode (Enter to send)');
  });

  it('opens the conflict dialog on a duplicate and resolves with a resolution', async () => {
    const dup = {
      duplicate: true,
      conflictType: 'filename',
      existingFile: { id: 'old', filename: 'pic.png', size: 100, createdAt: '2026-01-01', sha256: 'a' },
      newFile: { filename: 'pic.png', size: 120, sha256: 'b' },
    };
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse(dup))
      .mockResolvedValueOnce(jsonResponse(UPLOADED));
    vi.stubGlobal('fetch', fetchMock);
    const fixture = render();
    pickFile(fixture);
    await settle(fixture);
    expect(fixture.nativeElement.querySelector('qt-file-conflict-dialog')).not.toBeNull();
    (fixture.nativeElement.querySelector('.qt-dialog-footer button:last-child') as HTMLButtonElement).click();
    await settle(fixture);
    // The resolution re-uploaded with the conflicting file id.
    const secondCall = fetchMock.mock.calls[1];
    const form = secondCall[1].body as FormData;
    expect(form.get('resolution')).toBe('replace');
    expect(form.get('conflictingFileId')).toBe('old');
    expect(fixture.nativeElement.querySelector('qt-file-conflict-dialog')).toBeNull();
  });
});

describe('ChatComposer — draft persistence (v4 useDraftPersistence)', () => {
  const KEY = 'quilltap-draft-chat-1';

  afterEach(() => {
    vi.unstubAllGlobals();
    localStorage.clear();
    TestBed.resetTestingModule();
  });

  it('restores a saved draft into the editor on mount', async () => {
    localStorage.setItem(KEY, 'a restored draft');
    const fixture = render();
    await settle(fixture);
    expect(richEditor(fixture).getMarkdown()).toBe('a restored draft');
  });

  it('saves the draft after the 800ms debounce', async () => {
    const fixture = render();
    await settle(fixture);
    richEditor(fixture).setMarkdown('work in progress');
    await settle(fixture);
    expect(localStorage.getItem(KEY)).toBeNull(); // not yet — still debouncing
    await new Promise((r) => setTimeout(r, 850));
    expect(localStorage.getItem(KEY)).toBe('work in progress');
  });

  it('removes the key when the draft goes blank', async () => {
    localStorage.setItem(KEY, 'stale');
    const fixture = render();
    await settle(fixture);
    richEditor(fixture).setMarkdown('');
    await settle(fixture);
    await new Promise((r) => setTimeout(r, 850));
    expect(localStorage.getItem(KEY)).toBeNull();
  });

  it('clears the draft immediately on a successful send', async () => {
    vi.stubGlobal('fetch', vi.fn());
    localStorage.setItem(KEY, 'to send');
    const fixture = render();
    await settle(fixture);
    richEditor(fixture).setMarkdown('to send');
    await settle(fixture);
    fixture.nativeElement.querySelector('form').dispatchEvent(new Event('submit'));
    expect(localStorage.getItem(KEY)).toBeNull();
  });
});

describe('ChatComposer — text-replacement gating (v4 textReplacementsEnabled)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('forwards rules to the editor only when the gate is on', async () => {
    const rules = compileRules([
      {
        id: 'r1',
        fromText: 'teh',
        toText: 'the',
        caseSensitive: false,
        enabled: true,
        sortOrder: 0,
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-01T00:00:00.000Z',
      },
    ]);
    const fixture = render();
    await settle(fixture);
    fixture.componentRef.setInput('textReplacementRules', rules);
    fixture.componentRef.setInput('textReplacementsEnabled', true);
    fixture.detectChanges();
    expect(richEditor(fixture).textReplacementRules()).toBe(rules);

    fixture.componentRef.setInput('textReplacementsEnabled', false);
    fixture.detectChanges();
    expect(richEditor(fixture).textReplacementRules()).toBeNull();
  });
});

describe('ChatComposer — the Post Office gutter entries (v4 ComposerGutterTools)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('raises openAnnouncement and openMail from v4’s two row-1 buttons', () => {
    const fixture = render();
    const seen: string[] = [];
    fixture.componentInstance.openAnnouncement.subscribe(() => seen.push('announcement'));
    fixture.componentInstance.openMail.subscribe(() => seen.push('mail'));

    (
      fixture.nativeElement.querySelector(
        'button[aria-label="Insert announcement"]',
      ) as HTMLButtonElement
    ).click();
    (
      fixture.nativeElement.querySelector('button[aria-label="Post a letter"]') as HTMLButtonElement
    ).click();

    expect(seen).toEqual(['announcement', 'mail']);
  });

  it('carries v4’s titles verbatim', () => {
    const fixture = render();
    const megaphone = fixture.nativeElement.querySelector(
      'button[aria-label="Insert announcement"]',
    ) as HTMLButtonElement;
    const envelope = fixture.nativeElement.querySelector(
      'button[aria-label="Post a letter"]',
    ) as HTMLButtonElement;
    expect(megaphone.title).toBe('Insert announcement');
    expect(envelope.title).toBe('Post a letter');
  });

  it('orders the gutter group by v4’s grid fill order', () => {
    // v4 `ComposerGutterTools.tsx:36-46` fills a two-column grid left to right:
    // announcement, mail / library-file, camera / paperclip, RNG / wand — six
    // tools, all six now present (library file landed in P4.9E4B, the last empty
    // slot), flattened into v5's single row in v4's relative order.
    const fixture = render();
    const labels = [...fixture.nativeElement.querySelectorAll('.qt-chat-composer-actions button')]
      .map((b) => (b as HTMLElement).getAttribute('aria-label'))
      .filter((l): l is string =>
        [
          'Insert announcement',
          'Post a letter',
          'Attach file from library',
          'Generate image',
          'Attach a file',
        ].includes(l ?? ''),
      );
    expect(labels).toEqual([
      'Insert announcement',
      'Post a letter',
      'Attach file from library',
      'Generate image',
      'Attach a file',
    ]);
  });

  it('raises openLibrary from v4’s row-2 col-1 button, titled v4’s way', () => {
    const fixture = render();
    const seen: string[] = [];
    fixture.componentInstance.openLibrary.subscribe(() => seen.push('library'));
    const button = fixture.nativeElement.querySelector(
      'button[aria-label="Attach file from library"]',
    ) as HTMLButtonElement;
    expect(button.title).toBe('Attach file from library');
    button.click();
    expect(seen).toEqual(['library']);
  });

  it('puts a linked library file in the tray, once, and sends its id', async () => {
    const fixture = render();
    const linked = {
      id: 'lib-1',
      filename: 'atlas.pdf',
      filepath: 'uploads/atlas.pdf',
      mimeType: 'application/pdf',
      url: '/api/v1/files/lib-1',
    };
    fixture.componentInstance.addAttachedFile(linked);
    // A second link of the same file must not double the chip (nor the send id).
    fixture.componentInstance.addAttachedFile(linked);
    fixture.detectChanges();

    const chips = fixture.nativeElement.querySelectorAll('.qt-chat-attachment-chip');
    expect(chips).toHaveLength(1);
    expect(fixture.nativeElement.textContent).toContain('atlas.pdf');

    const sends: ComposerSend[] = [];
    fixture.componentInstance.send.subscribe((s: ComposerSend) => sends.push(s));
    richEditor(fixture).contentChange.emit('here it is');
    fixture.detectChanges();
    (fixture.nativeElement.querySelector('form') as HTMLFormElement).dispatchEvent(
      new Event('submit'),
    );
    await settle(fixture);
    expect(sends).toHaveLength(1);
    expect(sends[0].fileIds).toEqual(['lib-1']);
  });

  it('disables both while the composer is disabled (v4’s `disabled` prop)', () => {
    const fixture = render();
    fixture.componentRef.setInput('disabled', true);
    fixture.detectChanges();
    expect(
      (
        fixture.nativeElement.querySelector(
          'button[aria-label="Insert announcement"]',
        ) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    expect(
      (fixture.nativeElement.querySelector('button[aria-label="Post a letter"]') as HTMLButtonElement)
        .disabled,
    ).toBe(true);
  });
  /**
   * The pending tool-result chips (P4.9E1B — v4 `ChatComposer.tsx:284-302`). The
   * list is the Salon's; the composer only renders it, reports removals, and
   * counts it as sendable content.
   */
  const chip = (over: Partial<PendingToolResultChip> = {}): PendingToolResultChip => ({
    id: 'chip-1',
    tool: 'rng',
    displayName: 'Random Number Generator',
    icon: '🎲',
    summary: 'd20: 17',
    formattedResult: '🎲 Rolled 1d20: **17**',
    requestPrompt: 'Roll a d20',
    arguments: { type: 20, rolls: 1 },
    success: true,
    createdAt: '2026-07-26T00:00:00.000Z',
    ...over,
  });

  it('renders a pending roll as a chip, summary visible and the roll in its tooltip', () => {
    const fixture = render();
    fixture.componentRef.setInput('pendingToolResults', [chip()]);
    fixture.detectChanges();
    const el = fixture.nativeElement.querySelector('.qt-chat-tool-result-chip') as HTMLElement;
    expect(el.textContent).toContain('d20: 17');
    expect(el.textContent).toContain('🎲');
    expect(el.getAttribute('title')).toBe('Roll a d20\n\n🎲 Rolled 1d20: **17**');
  });

  it('reports the chip’s ✕ by id and leaves the removal to the Salon', () => {
    const fixture = render();
    fixture.componentRef.setInput('pendingToolResults', [chip({ id: 'chip-9' })]);
    fixture.detectChanges();
    const seen: string[] = [];
    fixture.componentInstance.removePendingToolResult.subscribe((id: string) => seen.push(id));
    (
      fixture.nativeElement.querySelector('[aria-label="Remove tool result"]') as HTMLButtonElement
    ).click();
    expect(seen).toEqual(['chip-9']);
    // Still rendered: the composer does not own the list.
    expect(fixture.nativeElement.querySelector('.qt-chat-tool-result-chip')).toBeTruthy();
  });

  it('a pending roll is enough to send on its own, with no text (v4 :456)', () => {
    const fixture = render();
    fixture.componentRef.setInput('hasActiveCharacters', true);
    fixture.detectChanges();
    const sendButton = () =>
      fixture.nativeElement.querySelector('button[type="submit"]') as HTMLButtonElement;
    expect(sendButton().disabled).toBe(true);
    fixture.componentRef.setInput('pendingToolResults', [chip()]);
    fixture.detectChanges();
    expect(sendButton().disabled).toBe(false);
  });

  it('mounts the RNG tool in the gutter', () => {
    const fixture = render();
    expect(
      fixture.nativeElement.querySelector('[aria-label="Random number generator"]'),
    ).toBeTruthy();
  });
});
