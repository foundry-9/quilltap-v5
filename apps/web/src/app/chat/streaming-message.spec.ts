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

  it('surfaces a mid-stream error frame', () => {
    const state = foldChatFrames([
      { content: 'Partial…' },
      { error: 'Failed to generate response', details: 'boom' },
    ]);
    const fixture = render(state);
    expect(fixture.nativeElement.textContent).toContain('Failed to generate response: boom');
  });
});
