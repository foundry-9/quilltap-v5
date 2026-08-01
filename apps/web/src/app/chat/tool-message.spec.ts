import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import type { ChatDetail, MessageDto } from '../core/core-contract';
import { ToolMessage } from './tool-message';
import { ToastService } from '../ui/toast.service';

/**
 * Byte-fidelity component spec for {@link ToolMessage} — the SPA analog of the
 * differential requirement. Every rendered label is asserted against a literal
 * TRANSCRIBED from v4 `components/chat/ToolMessage.tsx` (line cited inline).
 * There is no NDJSON oracle for Angular markup, so fidelity rides transcription
 * + review (the precedent: every P4.6 SPA lane).
 */

function msg(over: Partial<MessageDto>): MessageDto {
  return {
    id: 'm',
    role: 'TOOL',
    content: '',
    tokenCount: null,
    promptTokens: null,
    completionTokens: null,
    createdAt: '2026-02-01T12:34:00.000Z',
    swipeGroupId: null,
    swipeIndex: null,
    participantId: null,
    attachments: [],
    provider: null,
    modelName: null,
    targetParticipantIds: null,
    isSilentMessage: null,
    systemSender: null,
    systemKind: null,
    hostEvent: null,
    customAnnouncer: null,
    carinaMeta: null,
    pendingExternalPrompt: null,
    pendingExternalPromptFull: null,
    pendingExternalAttachments: null,
    reasoningContent: null,
    reasoningSegments: null,
    ...over,
  };
}

/** A minimal chat with one llm character (p1 = Aria) and one user character (pu). */
const CHAT = {
  id: 'c1',
  participants: [
    {
      id: 'p1',
      type: 'CHARACTER',
      controlledBy: 'llm',
      character: { id: 'a1', name: 'Aria', title: null, avatarUrl: null },
    },
    {
      id: 'pu',
      type: 'CHARACTER',
      controlledBy: 'user',
      character: { id: 'au', name: 'Captain', title: null, avatarUrl: null },
    },
  ],
  user: { id: 'u1', name: 'Charles', image: null },
} as unknown as ChatDetail;

function render(inputs: {
  message: MessageDto;
  embedded?: boolean;
  chat?: ChatDetail;
}): ComponentFixture<ToolMessage> {
  TestBed.configureTestingModule({ imports: [ToolMessage] });
  const fixture = TestBed.createComponent(ToolMessage);
  fixture.componentRef.setInput('message', inputs.message);
  fixture.componentRef.setInput('chat', inputs.chat ?? CHAT);
  fixture.componentRef.setInput('embedded', inputs.embedded ?? false);
  fixture.detectChanges();
  return fixture;
}

function el(fixture: ComponentFixture<ToolMessage>): HTMLElement {
  return fixture.nativeElement as HTMLElement;
}

function text(fixture: ComponentFixture<ToolMessage>): string {
  return (el(fixture).textContent ?? '').replace(/\s+/g, ' ').trim();
}

/** A generic character-initiated tool envelope. */
function toolContent(extra: Record<string, unknown> = {}): string {
  return JSON.stringify({
    toolName: 'rng',
    success: true,
    result: 'Rolled 1d6: [4]',
    prompt: '1d6',
    arguments: { type: 6 },
    ...extra,
  });
}

/** The toast stack this render raised, newest last. */
function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

describe('ToolMessage (v4 components/chat/ToolMessage.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  describe('envelope parse (:216-232)', () => {
    it('reads the old `toolName` key (embedded → the toolInfo display name)', () => {
      // The friendly display name renders in the emoji branch (embedded / no
      // author); the standalone named-author branch shows the raw `toolName`
      // in mono instead (:442, :459).
      const fixture = render({
        message: msg({ content: toolContent({ toolName: 'rng' }) }),
        embedded: true,
      });
      // rng → "Random Number Generator" (toolInfo :264-268).
      expect(text(fixture)).toContain('Random Number Generator');
    });

    it('falls back to the new `tool` key when `toolName` is absent', () => {
      const content = JSON.stringify({ tool: 'search', success: true, result: 'hits' });
      const fixture = render({ message: msg({ content }), embedded: true });
      // search → "Search" (toolInfo :249-253).
      expect(text(fixture)).toContain('Search');
    });

    it('uses the parse-failure fallback on unparseable content (:224-231)', () => {
      const fixture = render({ message: msg({ content: 'not json' }) });
      // toolName:'unknown', success:false → the ⚙️ fallback + "Failed" badge.
      expect(text(fixture)).toContain('unknown');
      expect(text(fixture)).toContain('Failed');
      // The response body is the fallback string, revealed on expand.
      const respToggle = [...el(fixture).querySelectorAll('button')].find((b) =>
        b.textContent?.includes('Tool Response'),
      )!;
      respToggle.click();
      fixture.detectChanges();
      expect(el(fixture).querySelector('.tool-response-content')!.textContent).toContain(
        'Unable to parse tool result',
      );
    });
  });

  describe('Success/Failed badge (:464-472)', () => {
    it('renders qt-badge-success + "Success" when success', () => {
      const fixture = render({ message: msg({ content: toolContent({ success: true }) }) });
      const badge = el(fixture).querySelector('.qt-badge-success')!;
      expect(badge).not.toBeNull();
      expect(badge.textContent!.trim()).toBe('Success');
    });

    it('renders qt-badge-destructive + "Failed" when not success', () => {
      const fixture = render({
        message: msg({ content: toolContent({ success: false, result: 'boom' }) }),
      });
      const badge = el(fixture).querySelector('.qt-badge-destructive')!;
      expect(badge).not.toBeNull();
      expect(badge.textContent!.trim()).toBe('Failed');
    });
  });

  describe('collapsed previews (:487-490, getPreviewText :174-178)', () => {
    it('shows the request preview (first line) collapsed and labels the section', () => {
      const fixture = render({
        message: msg({ content: toolContent({ prompt: 'roll 1d20\nsecond line' }) }),
      });
      expect(text(fixture)).toContain('Tool Request');
      // First line only, both sections collapsed by default.
      expect(text(fixture)).toContain('roll 1d20');
      expect(text(fixture)).not.toContain('second line');
    });

    it('truncates a long first line to 80 chars + "..."', () => {
      const long = 'x'.repeat(200);
      const fixture = render({ message: msg({ content: toolContent({ prompt: long }) }) });
      const preview = [...el(fixture).querySelectorAll('span')].find((s) =>
        s.textContent?.startsWith('xxxx'),
      )!;
      expect(preview.textContent).toBe('x'.repeat(80) + '...');
    });
  });

  describe('expand round-trip (:501-507, :545-599)', () => {
    it('reveals the pretty-printed request body on expand and re-collapses', () => {
      const fixture = render({
        message: msg({ content: toolContent({ prompt: undefined, arguments: { a: 1, b: 2 } }) }),
      });
      const reqToggle = [...el(fixture).querySelectorAll('button')].find((b) =>
        b.textContent?.includes('Tool Request'),
      )!;
      // Collapsed: no <pre> body yet.
      expect(el(fixture).querySelector('pre')).toBeNull();
      reqToggle.click();
      fixture.detectChanges();
      const pre = el(fixture).querySelector('pre')!;
      // JSON.stringify(arguments, null, 2) (formatRequestContent :183-188).
      expect(pre.textContent).toBe('{\n  "a": 1,\n  "b": 2\n}');
      // Glyph flips ▶ → ▼.
      expect(reqToggle.textContent).toContain('▼');
      reqToggle.click();
      fixture.detectChanges();
      expect(el(fixture).querySelector('pre')).toBeNull();
    });

    it('pretty-prints a JSON-string result on the response expand (formatResultContent :202-208)', () => {
      const fixture = render({
        message: msg({ content: toolContent({ result: '{"ok":true,"n":5}' }) }),
      });
      const respToggle = [...el(fixture).querySelectorAll('button')].find((b) =>
        b.textContent?.includes('Tool Response'),
      )!;
      respToggle.click();
      fixture.detectChanges();
      expect(el(fixture).querySelector('.tool-response-content pre')!.textContent).toBe(
        '{\n  "ok": true,\n  "n": 5\n}',
      );
    });
  });

  describe('attribution actor resolution (:375-378, :438-461)', () => {
    it('embedded: leads with the emoji + display name, no "ran" attribution (:447-460)', () => {
      const fixture = render({
        message: msg({ content: toolContent(), participantId: 'p1' }),
        embedded: true,
      });
      expect(text(fixture)).toContain('Random Number Generator');
      // Embedded drops the "<name> ran" attribution and the author name header.
      expect(text(fixture)).not.toContain('ran');
      expect(el(fixture).querySelector('.qt-chat-tool-embedded')).not.toBeNull();
      expect(el(fixture).querySelector('.qt-chat-message-row-tool')).toBeNull();
    });

    it('standalone character run: name header + "ran <tool>" (actor === header)', () => {
      const fixture = render({ message: msg({ content: toolContent(), participantId: 'p1' }) });
      expect(el(fixture).querySelector('.qt-chat-message-row-tool')).not.toBeNull();
      // headerAvatar.name = Aria; actor = Aria → attribution collapses to "ran rng".
      expect(text(fixture)).toContain('Aria');
      expect(text(fixture)).toContain('ran');
      expect(text(fixture)).toContain('rng');
    });

    it('standalone user run: operatorName drives "<operator> ran <tool>" (:376, :439-441)', () => {
      const fixture = render({
        message: msg({
          content: toolContent({ initiatedBy: 'user', operatorName: 'Charles' }),
          systemSender: 'prospero',
        }),
      });
      // headerAvatar.name = Prospero; actor = Charles ≠ Prospero → "Charles ran ".
      expect(text(fixture)).toContain('Prospero');
      expect(text(fixture)).toContain('Charles ran');
    });

    it('user run with no operator name falls back to "You" (:377)', () => {
      const fixture = render({
        message: msg({ content: toolContent({ initiatedBy: 'user' }), systemSender: 'prospero' }),
      });
      expect(text(fixture)).toContain('You ran');
    });
  });

  describe('whisper tag (:432-436)', () => {
    it('shows the italic "whisper" tag when the row is itself a whisper (named-author branch)', () => {
      const fixture = render({
        message: msg({
          content: toolContent(),
          participantId: 'p1',
          targetParticipantIds: ['pu'],
        }),
      });
      const tag = [...el(fixture).querySelectorAll('span')].find(
        (s) => s.textContent?.trim() === 'whisper',
      );
      expect(tag).toBeDefined();
      expect(tag!.className).toContain('italic');
    });

    it('omits the whisper tag when the row has no targets', () => {
      const fixture = render({ message: msg({ content: toolContent(), participantId: 'p1' }) });
      const tag = [...el(fixture).querySelectorAll('span')].find(
        (s) => s.textContent?.trim() === 'whisper',
      );
      expect(tag).toBeUndefined();
    });
  });

  describe('delegatedDisplay (:367)', () => {
    it('renders nothing when delegatedDisplay is true', () => {
      const fixture = render({
        message: msg({ content: toolContent({ delegatedDisplay: true }) }),
      });
      expect(text(fixture)).toBe('');
      expect(el(fixture).querySelector('.qt-chat-message-row-tool')).toBeNull();
      expect(el(fixture).querySelector('.qt-chat-tool-embedded')).toBeNull();
    });
  });

  describe('wardrobe notice (:90-146, :412-421)', () => {
    it('renders the action summary above the card for a successful wardrobe_create', () => {
      const content = JSON.stringify({
        toolName: 'wardrobe_create',
        success: true,
        result: { title: 'Velvet Coat', equipped: true },
      });
      const fixture = render({ message: msg({ content, participantId: 'p1' }) });
      const notice = el(fixture).querySelector('.qt-chat-wardrobe-notice')!;
      expect(notice).not.toBeNull();
      expect(notice.querySelector('.qt-chat-wardrobe-label')!.textContent).toBe('Wardrobe');
      expect(notice.textContent).toContain('Created and equipped "Velvet Coat".');
    });

    it('omits the notice when the wardrobe tool did not succeed', () => {
      const content = JSON.stringify({
        toolName: 'wardrobe_create',
        success: false,
        result: { title: 'X' },
      });
      const fixture = render({ message: msg({ content, participantId: 'p1' }) });
      expect(el(fixture).querySelector('.qt-chat-wardrobe-notice')).toBeNull();
    });
  });

  describe('copy feedback (inline status; v5 has no toast — chat-section §2)', () => {
    it('surfaces v4\'s "Request copied to clipboard" copy on the status line', async () => {
      const writes: string[] = [];
      // jsdom has no clipboard; install a stub.
      Object.defineProperty(navigator, 'clipboard', {
        configurable: true,
        value: { writeText: (t: string) => (writes.push(t), Promise.resolve()) },
      });
      const fixture = render({ message: msg({ content: toolContent({ prompt: 'roll' }) }) });
      const copyBtn = el(fixture).querySelector('button[aria-label="Copy request"]') as HTMLElement;
      copyBtn.click();
      await Promise.resolve();
      fixture.detectChanges();
      expect(writes).toEqual(['roll']);
      expect(toasts()).toEqual([{ type: 'success', message: 'Request copied to clipboard' }]);
    });
  });
});

/**
 * P4.26 — a standalone Staff-authored run wears its own portrait. v4 hands
 * `getMessageAvatar`'s whole result to `headerAvatar`
 * (`VirtualizedMessageList.tsx:240-247`), Staff rows included; v5 had
 * special-cased them to the display name with a hardcoded null avatar, because
 * `resolveMessageAuthor` had no Staff arm to defer to.
 */
describe('ToolMessage — the Staff header portrait (P4.26)', () => {
  it('gives a standalone Prospero run Prospero’s own portrait', () => {
    const fixture = render({
      message: msg({
        content: toolContent({ initiatedBy: 'user' }),
        systemSender: 'prospero',
        systemKind: 'tool-run',
      }),
    });
    const img = el(fixture).querySelector('img');
    expect(img?.getAttribute('src')).toBe('/images/avatars/prospero-avatar.webp');
    expect(img?.getAttribute('alt')).toBe('Prospero');
  });
});
