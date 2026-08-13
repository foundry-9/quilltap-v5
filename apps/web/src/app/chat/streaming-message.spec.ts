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
