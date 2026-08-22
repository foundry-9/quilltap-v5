import { describe, expect, it } from 'vitest';

import type { ScopedEvent } from './core-contract';
import { foldChatFrames, initialChatStreamState, reduceChatFrame, type StreamMessage } from './chat-stream.reducer';
import {
  FIXTURE_CHAT_ID,
  midStreamErrorTrace,
  multiTurnChainTrace,
  singleTurnTrace,
  skipDoneTrace,
  toolBatchTrace,
} from './__fixtures__/frame-trace';

/** Strip the scope tags (the consumer filters by chatId, then folds the frame). */
function framesFor(trace: ScopedEvent[], chatId: string) {
  return trace
    .filter((e) => e.chatId === chatId)
    .map(({ chatId: _c, roomId: _r, progressId: _p, ...frame }) => frame);
}

function assistant(messages: StreamMessage[]) {
  return messages.filter((m): m is Extract<StreamMessage, { kind: 'assistant' }> => m.kind === 'assistant');
}

describe('chat stream reducer', () => {
  it('interleaves content (append) and reasoning (replace), then emits the assistant bubble on done', () => {
    const frames = framesFor(singleTurnTrace, FIXTURE_CHAT_ID);
    // Fold everything except the terminal done to inspect the live buffer.
    const beforeDone = frames.slice(0, -1).reduce(reduceChatFrame, initialChatStreamState());
    expect(beforeDone.content).toBe('Cats are wonderful.'); // content appended
    expect(beforeDone.reasoning).toBe('Let me think about cats'); // reasoning replaced, not appended
    expect(beforeDone.streaming).toBe(true);
    expect(beforeDone.status?.stage).toBe('streaming');

    const final = frames.reduce(reduceChatFrame, initialChatStreamState());
    const bubbles = assistant(final.messages);
    expect(bubbles).toHaveLength(1);
    expect(bubbles[0]).toMatchObject({
      id: 'm1',
      content: 'Cats are wonderful.',
      participantId: 'p1',
      provider: 'ANTHROPIC',
      modelName: 'claude-sonnet-5',
      reasoningContent: 'Let me think about cats',
      intermediate: false,
    });
    // The live buffer resets after the bubble is emitted.
    expect(final.content).toBe('');
    expect(final.reasoning).toBe('');
    expect(final.finalDone?.messageId).toBe('m1');
  });

  it('filters frames by chatId — the other-chat content never reaches this chat', () => {
    // The raw trace carries an OTHER-chat content frame; folding only the
    // filtered frames must not include it.
    const final = foldChatFrames(framesFor(singleTurnTrace, FIXTURE_CHAT_ID));
    expect(assistant(final.messages)[0].content).toBe('Cats are wonderful.');
    expect(final.content).toBe('');
  });

  it('splices a tool batch at the prose offset and marks the result on the latest batch', () => {
    const frames = framesFor(toolBatchTrace, FIXTURE_CHAT_ID);
    // After detection (before the result / trailing content), the batch is anchored.
    const afterDetect = frames.slice(0, 2).reduce(reduceChatFrame, initialChatStreamState());
    expect(afterDetect.toolBatches).toHaveLength(1);
    expect(afterDetect.toolBatches[0].offset).toBe('Let me look '.length); // 12
    expect(afterDetect.toolBatches[0].calls[0]).toMatchObject({
      name: 'search',
      status: 'pending',
      arguments: { query: 'cats' },
    });

    const final = frames.reduce(reduceChatFrame, initialChatStreamState());
    // The batch persists on the state carried into the bubble; the call is resolved.
    const batch = final.toolBatches[0] ?? afterDetect.toolBatches[0];
    // Re-fold up to just before the terminal done to inspect the resolved batch.
    const beforeDone = frames.slice(0, -1).reduce(reduceChatFrame, initialChatStreamState());
    expect(beforeDone.toolBatches[0].calls[0]).toMatchObject({ status: 'success', result: { hits: 3 } });
    expect(beforeDone.toolBatches[0].offset).toBe(12);
    expect(batch).toBeDefined();
    expect(assistant(final.messages)[0].id).toBe('m2');
    expect(final.finalDone?.toolsExecuted).toBe(true);
  });

  it("carries a failing tool's sibling `error` sentence onto the call (Bug 84)", () => {
    // The emitter puts the sentence in `error`, a SIBLING of `result`, because
    // `result` is null on failure (`chat_events.rs:529`). The reducer must carry
    // it RAW — the executor's own `Error: ` prefix intact — so the render site
    // can resolve it; dropping it here is half of what v4's bug 84 was.
    const detected = reduceChatFrame(initialChatStreamState(), {
      toolsDetected: 1,
      toolNames: ['generate_image'],
      toolArguments: [{ prompt: 'a cat' }],
    });
    const failed = reduceChatFrame(detected, {
      toolResult: {
        index: 0,
        name: 'generate_image',
        success: false,
        result: null,
        error: 'Error: Image generation is not enabled for this chat',
      },
    });
    expect(failed.toolBatches[0].calls[0]).toMatchObject({
      name: 'generate_image',
      status: 'error',
      errorText: 'Error: Image generation is not enabled for this chat',
    });
  });

  it('leaves `errorText` undefined on a success frame, which carries no `error`', () => {
    const detected = reduceChatFrame(initialChatStreamState(), {
      toolsDetected: 1,
      toolNames: ['generate_image'],
    });
    const ok = reduceChatFrame(detected, {
      toolResult: { index: 0, name: 'generate_image', success: true, result: { images: [{}] } },
    });
    expect(ok.toolBatches[0].calls[0].status).toBe('success');
    expect(ok.toolBatches[0].calls[0].errorText).toBeUndefined();
  });

  it('handles a multi-turn chain: one bubble per turn, terminal chainComplete', () => {
    const final = foldChatFrames(framesFor(multiTurnChainTrace, FIXTURE_CHAT_ID));
    const bubbles = assistant(final.messages);
    expect(bubbles.map((m) => m.id)).toEqual(['m10', 'm11']);
    expect(bubbles[0]).toMatchObject({ content: 'Hi, I am Ada.', participantId: 'p1', intermediate: true });
    expect(bubbles[1]).toMatchObject({ content: 'And I am Bob.', participantId: 'p2', intermediate: true });
    expect(final.inChain).toBe(false);
    expect(final.finished).toBe(true);
    expect(final.respondingParticipantId).toBeNull();
    // Each turnStart reset the buffer, so no content leaked across turns.
    expect(final.content).toBe('');
  });

  it('handles a skip (nothing to add) done: surfaces the Host note, emits no bubble', () => {
    const final = foldChatFrames(framesFor(skipDoneTrace, FIXTURE_CHAT_ID));
    expect(assistant(final.messages)).toHaveLength(0);
    const host = final.messages.find((m) => m.kind === 'host');
    expect(host).toBeDefined();
    expect(host?.id).toBe('host-1');
    expect(final.finalDone?.skipped).toBe(true);
    expect(final.finalDone?.skippedParticipantId).toBe('p3');
    expect(final.content).toBe('');
  });

  it('records a mid-stream transport error and halts streaming', () => {
    const final = foldChatFrames(framesFor(midStreamErrorTrace, FIXTURE_CHAT_ID));
    expect(final.error).toBe('Failed to generate response: boom');
    expect(final.streaming).toBe(false);
    expect(final.waitingForResponse).toBe(false);
  });

  it('dedupes carina/host announcements by id', () => {
    const base = initialChatStreamState();
    const s1 = reduceChatFrame(base, { carinaAnswer: { id: 'carina-1', type: 'message' } });
    const s2 = reduceChatFrame(s1, { carinaAnswer: { id: 'carina-1', type: 'message' } });
    expect(s2.messages.filter((m) => m.kind === 'carina')).toHaveLength(1);
  });

  it('surfaces a pascalResult mid-turn and dedupes it by id (§4)', () => {
    const base = initialChatStreamState();
    const s1 = reduceChatFrame(base, {
      pascalResult: { id: 'pascal-1', type: 'message', systemSender: 'pascal' },
    });
    const s2 = reduceChatFrame(s1, {
      pascalResult: { id: 'pascal-1', type: 'message', systemSender: 'pascal' },
    });
    const pascal = s2.messages.filter((m) => m.kind === 'pascal');
    expect(pascal).toHaveLength(1);
    expect(pascal[0].id).toBe('pascal-1');
    expect((pascal[0] as Extract<StreamMessage, { kind: 'pascal' }>).message).toMatchObject({
      systemSender: 'pascal',
    });
  });

  it('applies an answer-confirmation revision to the bubble content', () => {
    const done = foldChatFrames(framesFor(singleTurnTrace, FIXTURE_CHAT_ID));
    const revised = reduceChatFrame(done, {
      confirmationResult: { messageId: 'm1', confirmed: true, revised: true, notes: 'tightened', content: 'Cats are truly wonderful.' },
    });
    const bubble = assistant(revised.messages)[0];
    expect(bubble.content).toBe('Cats are truly wonderful.');
    expect(bubble.confirmationRevised).toBe(true);
    expect(bubble.confirmed).toBe(true);
    expect(bubble.confirmationNotes).toBe('tightened');
  });
});
