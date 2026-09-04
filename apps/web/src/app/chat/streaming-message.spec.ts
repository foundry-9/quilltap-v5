import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { singleTurnTrace, FIXTURE_CHAT_ID } from '../core/__fixtures__/frame-trace';
import { foldChatFrames } from '../core/chat-stream.reducer';
import { StreamingMessage } from './streaming-message';

/**
 * Reducer → rendered bubble integration: fold a real frame trace (filtered to the
 * subject chat, as the consumer does) and assert the live bubble shows the
 * accumulated content and the server status.
 */
describe('StreamingMessage', () => {
  function render(state: ReturnType<typeof foldChatFrames>): ComponentFixture<StreamingMessage> {
    TestBed.configureTestingModule({ imports: [StreamingMessage] });
    const fixture = TestBed.createComponent(StreamingMessage);
    fixture.componentRef.setInput('state', state);
    fixture.detectChanges();
    return fixture;
  }

  it('renders accumulated streamed content while the turn is in flight', () => {
    // Fold the trace up to (not including) the done frame — the live bubble state.
    const midStream = singleTurnTrace
      .filter((f) => f.chatId === FIXTURE_CHAT_ID && !f.done)
      .map((f) => {
        const { chatId, ...frame } = f;
        void chatId;
        return frame;
      });
    const state = foldChatFrames(midStream);
    const fixture = render(state);
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Cats are wonderful.');
    expect(text).toContain('Friday is composing a reply…');
  });

  // v4 renders no inline stream error — the Salon toasts it
  // (`useSSEStreaming.ts:1024-1027`) and the bubble simply stops. The folded
  // state still CARRIES the sentence; only the render is v4's silence.
  it('records a mid-stream error frame without rendering it', () => {
    const state = foldChatFrames([
      { content: 'Partial…' },
      { error: 'Failed to generate response', details: 'boom' },
    ]);
    expect(state.error).toBe('Failed to generate response: boom');
    const fixture = render(state);
    expect(fixture.nativeElement.textContent).not.toContain('Failed to generate response');
  });

  /**
   * The waiting quill at v5's analogs of v4's four call sites (v4 spreads them
   * over StreamingMessage, ChatComposer's status strip and PendingToolCalls).
   * v4 shipped no tests with `deab0e5d`, so these pin its documented semantics:
   * where the indicator appears, where the pulsing dot survives, and where the
   * label is suppressed because the surrounding region is already labelled.
   */
  describe('the waiting quill', () => {
    function quills(fixture: ComponentFixture<StreamingMessage>): HTMLElement[] {
      return Array.from(fixture.nativeElement.querySelectorAll('.qt-thinking-indicator'));
    }

    it('shows the large labelled quill while awaiting the first chunk', () => {
      const fixture = render(foldChatFrames([{ turnStart: true }]));
      const found = quills(fixture);
      expect(found).toHaveLength(1);
      expect(found[0].classList.contains('w-12')).toBe(true);
      expect(found[0].getAttribute('aria-label')).toBe('Writing…');
      // v4 renders the bare quill here, not a bubble.
      expect(fixture.nativeElement.querySelector('.qt-chat-message-assistant')).toBeNull();
    });

    it('trails the live prose with the small quill once content streams', () => {
      const fixture = render(foldChatFrames([{ content: 'Cats are ' }, { content: 'wonderful.' }]));
      const found = quills(fixture);
      expect(found).toHaveLength(1);
      expect(found[0].classList.contains('w-4')).toBe(true);
    });

    it('carries the quill in the status strip only while streaming, unlabelled', () => {
      const fixture = render(
        foldChatFrames([{ status: { stage: 'streaming', message: 'Friday is composing…' } }]),
      );
      const strip = fixture.nativeElement.querySelector('.qt-chat-response-status-icon');
      expect(strip.querySelector('.qt-thinking-indicator')).not.toBeNull();
      expect(strip.querySelector('.qt-thinking-indicator').getAttribute('aria-label')).toBeNull();
      expect(strip.querySelector('.animate-pulse')).toBeNull();
    });

    // One `it` per stage: TestBed cannot be reconfigured once instantiated, so a
    // loop INSIDE a single case would fail on its second render.
    for (const stage of ['compressing', 'gathering', 'building', 'sending', 'tool_executing']) {
      it(`keeps the pulsing dot on the ${stage} stage`, () => {
        const fixture = render(foldChatFrames([{ status: { stage, message: `${stage}…` } }]));
        const strip = fixture.nativeElement.querySelector('.qt-chat-response-status-icon');
        expect(strip.querySelector('.animate-pulse')).not.toBeNull();
        expect(strip.querySelector('.qt-thinking-indicator')).toBeNull();
      });
    }

    /**
     * v4 `fed5b5da` — the trailing indicator takes a line of its own when the
     * trailing prose segment is empty and the part before it is a tool batch.
     * v4 shipped no test with the fix, so these pin both arms of its
     * `standaloneIndicator` conditional against v5's flattened render.
     */
    describe('the standalone indicator above a tool block', () => {
      /**
       * The TRAILING quill's own wrapper span (the element carrying the classes
       * the call site passes) — the one indicator that is neither a tool row's
       * nor the status strip's.
       */
      function wrapper(fixture: ComponentFixture<StreamingMessage>): HTMLElement {
        const hosts: HTMLElement[] = Array.from(
          fixture.nativeElement.querySelectorAll('qt-quill-animation'),
        );
        const trailing = hosts.filter(
          (h) => !h.closest('.qt-chat-tool-embedded') && !h.closest('.qt-chat-response-status-icon'),
        );
        expect(trailing).toHaveLength(1);
        return trailing[0].querySelector('span') as HTMLElement;
      }

      it('keeps the inline form while prose is still arriving after a tool batch', () => {
        const fixture = render(
          foldChatFrames([
            { content: 'Let me look ' },
            { toolsDetected: 1, toolNames: ['search_memories'] },
            { content: 'and here it is.' },
          ]),
        );
        expect(fixture.nativeElement.querySelector('div.mt-3')).toBeNull();
        expect(wrapper(fixture).classList.contains('inline-block')).toBe(true);
      });

      it('gives the indicator its own line when the trailing segment is empty', () => {
        const fixture = render(
          foldChatFrames([
            { content: 'Let me look ' },
            { toolsDetected: 1, toolNames: ['search_memories'] },
          ]),
        );
        const standalone = fixture.nativeElement.querySelector('div.mt-3') as HTMLElement;
        expect(standalone).not.toBeNull();
        expect(standalone.querySelector('.qt-thinking-indicator')).not.toBeNull();
        // v4 drops `inline-block` on this arm and keeps `ml-2`.
        const classes = wrapper(fixture).classList;
        expect(classes.contains('inline-block')).toBe(false);
        expect(classes.contains('ml-2')).toBe(true);
        // …and it sits BELOW the tool block, which is the whole point.
        const rows = fixture.nativeElement.querySelectorAll('.qt-chat-tool-embedded');
        expect(rows).toHaveLength(1);
        expect(
          rows[0].compareDocumentPosition(standalone) & Node.DOCUMENT_POSITION_FOLLOWING,
        ).toBeTruthy();
      });

      it('stays inline when no tool batch precedes it', () => {
        const fixture = render(foldChatFrames([{ content: 'Cats are wonderful.' }]));
        expect(fixture.nativeElement.querySelector('div.mt-3')).toBeNull();
        expect(wrapper(fixture).classList.contains('inline-block')).toBe(true);
      });
    });

    it('marks a pending tool row with an unlabelled quill, settled rows with ✓/✗', () => {
      const fixture = render(
        foldChatFrames([
          { content: 'thinking about it' },
          { toolsDetected: 2, toolNames: ['search_memories', 'read_conversation'] },
          { toolResult: { name: 'search_memories', success: true, result: 'ok' } },
        ]),
      );
      const rows: HTMLElement[] = Array.from(
        fixture.nativeElement.querySelectorAll('.qt-chat-tool-embedded'),
      );
      expect(rows).toHaveLength(2);
      expect(rows[0].textContent).toContain('✓');
      expect(rows[0].querySelector('.qt-thinking-indicator')).toBeNull();
      const pending = rows[1].querySelector('.qt-thinking-indicator');
      expect(pending).not.toBeNull();
      expect(pending!.getAttribute('aria-label')).toBeNull();
    });
  });
});

/**
 * The live bubble's avatar column — v4 `StreamingMessage.tsx:85-96`, ported by
 * P4.75. v4's markup, line by line:
 *
 * ```tsx
 * {shouldShowAvatars && (                                            // :85
 *   <div className={`flex-shrink-0 qt-chat-desktop-avatar${
 *      isDangerousChat ? ' qt-chat-avatar-dangerous' : ''}`}>         // :85
 *     <Avatar name={respondingCharacter?.name || 'AI'}                // :86
 *             title={null} src={respondingCharacter} size="chat"      // :87-89
 *             showName showTitle className="… w-32 …" />              // :90-92
 *   </div>
 * )}
 * ```
 *
 * `flex-shrink-0` is the CSS rule's own `@apply` in v5 (`_chat.css:2517`), so
 * the column carries the two semantic classes exactly as the settled rows do
 * (`message-row.ts:87-92`) — the shape P4.D131's ring already targets.
 */
describe('StreamingMessage — the responding avatar column (v4 :85-96)', () => {
  function renderWith(
    inputs: Record<string, unknown>,
  ): ComponentFixture<StreamingMessage> {
    // Reset first: the ring case renders twice (calm, then flagged) in one `it`.
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({ imports: [StreamingMessage] });
    const fixture = TestBed.createComponent(StreamingMessage);
    fixture.componentRef.setInput('state', foldChatFrames([{ content: 'Live prose' }]));
    for (const [k, v] of Object.entries(inputs)) fixture.componentRef.setInput(k, v);
    fixture.detectChanges();
    return fixture;
  }

  const ada = {
    id: 'c1',
    name: 'Ada',
    title: null,
    avatarUrl: 'uploads/ada.png',
    defaultImageId: null,
    defaultImage: null,
  };

  it('renders no column when the gate is off (v4 :85 — `shouldShowAvatars &&`)', () => {
    const fixture = renderWith({ showAvatar: false, respondingCharacter: ada });
    expect(fixture.nativeElement.querySelector('.qt-chat-desktop-avatar')).toBeNull();
  });

  it('opens the row with the responding character under the gate (v4 :85-89)', () => {
    const fixture = renderWith({ showAvatar: true, respondingCharacter: ada });
    const column = fixture.nativeElement.querySelector('.qt-chat-desktop-avatar') as HTMLElement;
    expect(column).not.toBeNull();
    // The column is the row's FIRST child, ahead of the body (v4 :85 precedes :98).
    const row = fixture.nativeElement.querySelector('.qt-chat-message-row-assistant') as HTMLElement;
    expect(row.firstElementChild).toBe(column);
    const img = column.querySelector('img') as HTMLImageElement;
    expect(img.getAttribute('alt')).toBe('Ada');
    // v4's `getAvatarSrc` string branch: a bare path gains a leading slash.
    expect(img.getAttribute('src')).toBe('/uploads/ada.png');
  });

  it("names an absent character 'AI' (v4 :86 — `respondingCharacter?.name || 'AI'`)", () => {
    const fixture = renderWith({ showAvatar: true, respondingCharacter: null });
    const column = fixture.nativeElement.querySelector('.qt-chat-desktop-avatar') as HTMLElement;
    // No src, so `qt-avatar` falls back to the initial of the name.
    expect(column.textContent?.trim()).toBe('A');
    // The initial alone would accept any fallback beginning with A — pin the
    // WHOLE string through the computed the template binds (unify §3 review).
    expect((fixture.componentInstance as unknown as { avatarName: () => string }).avatarName()).toBe('AI');
    expect(column.querySelector('img')).toBeNull();
  });

  it("falls back to 'AI' for a character with a blank name (v4 :86 is `||`, not `??`)", () => {
    const fixture = renderWith({ showAvatar: true, respondingCharacter: { ...ada, name: '', avatarUrl: null } });
    expect(
      (fixture.nativeElement.querySelector('.qt-chat-desktop-avatar') as HTMLElement).textContent?.trim(),
    ).toBe('A');
    expect((fixture.componentInstance as unknown as { avatarName: () => string }).avatarName()).toBe('AI');
  });

  it('wears the danger ring only on a flagged chat (v4 :85 ternary)', () => {
    const calm = renderWith({ showAvatar: true, respondingCharacter: ada, isDangerousChat: false });
    expect(
      (calm.nativeElement.querySelector('.qt-chat-desktop-avatar') as HTMLElement).classList.contains(
        'qt-chat-avatar-dangerous',
      ),
    ).toBe(false);
    const flagged = renderWith({ showAvatar: true, respondingCharacter: ada, isDangerousChat: true });
    expect(
      (flagged.nativeElement.querySelector('.qt-chat-desktop-avatar') as HTMLElement).classList.contains(
        'qt-chat-avatar-dangerous',
      ),
    ).toBe(true);
  });

  /**
   * v4 renders ONE assistant row whose BODY switches between the waiting quill
   * and the bubble (`:98-104`), so the column is present in both states. v5
   * splits the row in two `@if`s, which is why both need the column — a
   * regression that dropped it from the waiting arm would show as an avatar that
   * pops in when the first token lands.
   */
  it('is present in the waiting state too (v4 :85 precedes the :99 branch)', () => {
    TestBed.configureTestingModule({ imports: [StreamingMessage] });
    const fixture = TestBed.createComponent(StreamingMessage);
    fixture.componentRef.setInput('state', foldChatFrames([{ turnStart: true }]));
    fixture.componentRef.setInput('showAvatar', true);
    fixture.componentRef.setInput('respondingCharacter', ada);
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('.qt-chat-desktop-avatar')).not.toBeNull();
    expect(fixture.nativeElement.querySelector('.qt-thinking-indicator')).not.toBeNull();
  });
});
