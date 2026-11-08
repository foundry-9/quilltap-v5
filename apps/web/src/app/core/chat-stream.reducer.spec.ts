import { describe, expect, it } from 'vitest';

import type { ChatStreamFrame } from './core-contract';
import { foldChatFrames, initialChatStreamState, reduceChatFrame, type StreamMessage } from './chat-stream.reducer';
import {
  FIXTURE_CHAT_ID,
  doneFrame,
  goldenStream,
  midStreamErrorTrace,
  multiTurnChainTrace,
  singleTurnTrace,
  skipDoneTrace,
  toolBatchTrace,
  type WireFrame,
} from './__fixtures__/frame-trace';

/**
 * P4.D384 (v4 `604a082c4`): the Salon's fold over `kind`-tagged frames. Every
 * case that used to fold a multi-key bag is re-expressed as the kind v4 now
 * sends; the cases whose shape no longer exists on the wire are deleted with a
 * one-line note where they stood. The pre-frames spec lives on, verbatim, over
 * the legacy fold (`chat-stream.reducer.legacy.spec.ts`).
 */

/** Strip the scope tag (the consumer filters by chatId, then folds the frame). */
function framesFor(trace: WireFrame[], chatId: string): ChatStreamFrame[] {
  return trace
    .filter((e) => e.chatId === chatId)
    .map(({ chatId: _c, roomId: _r, progressId: _p, ...frame }) => frame as ChatStreamFrame);
}

function assistant(messages: StreamMessage[]) {
  return messages.filter((m): m is Extract<StreamMessage, { kind: 'assistant' }> => m.kind === 'assistant');
}

const seat = (participantId: string) => ({ participantId, concurrent: true as const });

describe('chat stream reducer', () => {
  it('interleaves content (append) and reasoning (replace), then emits the assistant bubble on done', () => {
    const frames = framesFor(singleTurnTrace, FIXTURE_CHAT_ID);
    const beforeDone = frames.slice(0, -1).reduce(reduceChatFrame, initialChatStreamState());
    expect(beforeDone.content).toBe('Cats are wonderful.');
    expect(beforeDone.reasoning).toBe('Let me think about cats');
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
    expect(final.content).toBe('');
    expect(final.reasoning).toBe('');
    expect(final.finalDone?.messageId).toBe('m1');
    expect(final.finalDone?.outcome).toBe('finished');
  });

  it('filters frames by chatId — the other-chat content never reaches this chat', () => {
    const final = foldChatFrames(framesFor(singleTurnTrace, FIXTURE_CHAT_ID));
    expect(assistant(final.messages)[0].content).toBe('Cats are wonderful.');
    expect(final.content).toBe('');
  });

  it('an empty content delta is ignored — the previous state comes back by reference (v4 `if (!frame.text) return`)', () => {
    const prev = initialChatStreamState();
    expect(reduceChatFrame(prev, { kind: 'content', text: '' })).toBe(prev);
  });

  it('splices a tool batch at the prose offset and marks the (flat) result on the latest batch', () => {
    const frames = framesFor(toolBatchTrace, FIXTURE_CHAT_ID);
    const afterDetect = frames.slice(0, 2).reduce(reduceChatFrame, initialChatStreamState());
    expect(afterDetect.toolBatches).toHaveLength(1);
    expect(afterDetect.toolBatches[0].offset).toBe('Let me look '.length);
    expect(afterDetect.toolBatches[0].calls[0]).toMatchObject({
      name: 'search',
      status: 'pending',
      arguments: { query: 'cats' },
    });
    const beforeDone = frames.slice(0, -1).reduce(reduceChatFrame, initialChatStreamState());
    expect(beforeDone.toolBatches[0].calls[0]).toMatchObject({ status: 'success', result: { hits: 3 } });
    expect(beforeDone.toolBatches[0].offset).toBe(12);
    const final = frames.reduce(reduceChatFrame, initialChatStreamState());
    expect(assistant(final.messages)[0].id).toBe('m2');
    expect(final.finalDone?.toolsExecuted).toBe(true);
  });

  it("carries a failing tool's sibling `error` sentence onto the call (Bug 84)", () => {
    const detected = reduceChatFrame(initialChatStreamState(), {
      kind: 'toolsDetected',
      names: ['generate_image'],
      arguments: [{ prompt: 'a cat' }],
    });
    const failed = reduceChatFrame(detected, {
      kind: 'toolResult',
      index: 0,
      name: 'generate_image',
      success: false,
      result: null,
      error: 'Error: Image generation is not enabled for this chat',
    });
    expect(failed.toolBatches[0].calls[0]).toMatchObject({
      name: 'generate_image',
      status: 'error',
      errorText: 'Error: Image generation is not enabled for this chat',
    });
  });

  it('leaves `errorText` undefined on a success frame, which carries no `error`', () => {
    const detected = reduceChatFrame(initialChatStreamState(), {
      kind: 'toolsDetected',
      names: ['generate_image'],
      arguments: [],
    });
    const ok = reduceChatFrame(detected, {
      kind: 'toolResult',
      index: 0,
      name: 'generate_image',
      success: true,
      result: { images: [{}] },
    });
    expect(ok.toolBatches[0].calls[0].status).toBe('success');
    expect(ok.toolBatches[0].calls[0].errorText).toBeUndefined();
  });

  it('a toolResult without an index matches the batch call by name', () => {
    const detected = reduceChatFrame(initialChatStreamState(), {
      kind: 'toolsDetected',
      names: ['a', 'b'],
      arguments: [{}, {}],
    });
    const s = reduceChatFrame(detected, { kind: 'toolResult', name: 'b', success: true });
    expect(s.toolBatches[0].calls.map((c) => c.status)).toEqual(['pending', 'success']);
  });

  it('handles a multi-turn chain: one bubble per turn, terminal chainComplete', () => {
    const final = foldChatFrames(framesFor(multiTurnChainTrace, FIXTURE_CHAT_ID));
    const bubbles = assistant(final.messages);
    expect(bubbles.map((m) => m.id)).toEqual(['m9', 'm10', 'm11']);
    expect(bubbles[0]).toMatchObject({ content: 'Shall we begin?', participantId: 'p0', intermediate: false });
    expect(bubbles[1]).toMatchObject({ content: 'Hi, I am Ada.', participantId: 'p1', intermediate: true });
    expect(bubbles[2]).toMatchObject({ content: 'And I am Bob.', participantId: 'p2', intermediate: true });
    expect(final.inChain).toBe(false);
    expect(final.finished).toBe(true);
    expect(final.respondingParticipantId).toBeNull();
    expect(final.content).toBe('');
  });

  it('handles a skip (nothing to add) done: surfaces the Host note, emits no bubble', () => {
    const final = foldChatFrames(framesFor(skipDoneTrace, FIXTURE_CHAT_ID));
    expect(assistant(final.messages)).toHaveLength(0);
    const host = final.messages.find((m) => m.kind === 'host');
    expect(host?.id).toBe('host-1');
    expect(final.finalDone).toMatchObject({ outcome: 'skipped', skipped: true, yielded: false, skippedParticipantId: 'p3' });
    expect(final.content).toBe('');
  });

  it('records a mid-stream error frame (`message` + `: details`) and halts streaming', () => {
    const final = foldChatFrames(framesFor(midStreamErrorTrace, FIXTURE_CHAT_ID));
    expect(final.error).toBe('Failed to generate response: boom');
    expect(final.streaming).toBe(false);
    expect(final.waitingForResponse).toBe(false);
  });

  it('dedupes posted messages by id', () => {
    const posted = { kind: 'posted', what: 'carinaAnswer', message: { id: 'carina-1', type: 'message' } } as const;
    const s2 = reduceChatFrame(reduceChatFrame(initialChatStreamState(), posted), posted);
    expect(s2.messages.filter((m) => m.kind === 'carina')).toHaveLength(1);
  });

  it('maps each posted `what` onto its bubble kind', () => {
    const s = foldChatFrames([
      { kind: 'posted', what: 'carinaAnswer', message: { id: 'c' } },
      { kind: 'posted', what: 'hostAnnouncement', message: { id: 'h' } },
      { kind: 'posted', what: 'pascalResult', message: { id: 'p', systemSender: 'pascal' } },
    ]);
    expect(s.messages.map((m) => [m.kind, m.id])).toEqual([
      ['carina', 'c'],
      ['host', 'h'],
      ['pascal', 'p'],
    ]);
    expect((s.messages[2] as Extract<StreamMessage, { kind: 'pascal' }>).message).toMatchObject({ systemSender: 'pascal' });
  });

  it('carries the done frame’s attachmentResults onto finalDone, the same object (bug 94)', () => {
    const ledger = { sent: ['file-ok'], failed: [{ id: 'file-bad', error: 'Provider cannot send images' }] };
    const done = reduceChatFrame(initialChatStreamState(), doneFrame({ messageId: 'm1', attachmentResults: ledger }));
    expect(done.finalDone?.attachmentResults).toBe(ledger);
  });

  it('reads a null ledger as null', () => {
    const s = reduceChatFrame(initialChatStreamState(), doneFrame({ messageId: 'm1', attachmentResults: null }));
    expect(s.finalDone?.attachmentResults).toBeNull();
  });
  // (deleted: "an ABSENT ledger" — v4's schema makes `attachmentResults`
  // required on a `done`; a frame without it never reaches the fold.)

  it('a pendingExternalTurn FRAME sets the Courier flag, spreading the last ledger forward unchanged', () => {
    const ledger = { sent: [], failed: [{ id: 'file-bad', error: 'Provider cannot send images' }] };
    const done = reduceChatFrame(initialChatStreamState(), doneFrame({ messageId: 'm1', attachmentResults: ledger }));
    const parked = reduceChatFrame(done, {
      kind: 'pendingExternalTurn',
      messageId: 'm-5',
      participantId: 'p-ext',
      characterName: 'Dee',
    });
    expect(parked.finalDone?.attachmentResults).toBe(ledger);
    expect(parked.finalDone?.pendingExternalTurn).toBe(true);
  });

  it('a done with `pendingExternalTurn: true` takes the Courier arm: no bubble, nothing waiting', () => {
    const s = foldChatFrames([
      { kind: 'turnStart', participantId: 'p-ext', characterName: 'Dee', chainDepth: 0 },
      doneFrame({ messageId: 'm-5', participantId: 'p-ext', pendingExternalTurn: true }),
    ]);
    expect(assistant(s.messages)).toHaveLength(0);
    expect(s.finalDone?.pendingExternalTurn).toBe(true);
    expect(s.waitingForResponse).toBe(false);
    expect(s.respondingParticipantId).toBeNull();
  });

  it('carries reason and paused off a chainComplete frame', () => {
    const s = reduceChatFrame(initialChatStreamState(), {
      kind: 'chainComplete',
      reason: 'error',
      nextSpeakerId: null,
      paused: true,
      chainDepth: 2,
    });
    expect(s.finished).toBe(true);
    expect(s.chainReason).toBe('error');
    expect(s.chainPaused).toBe(true);
  });

  it('reads an absent or false `paused` / `heldUserTurn` alike', () => {
    const base = { kind: 'chainComplete', reason: 'no_next_speaker', nextSpeakerId: null, chainDepth: 0 } as const;
    expect(reduceChatFrame(initialChatStreamState(), base).chainPaused).toBe(false);
    expect(reduceChatFrame(initialChatStreamState(), { ...base, paused: false }).chainPaused).toBe(false);
    expect(reduceChatFrame(initialChatStreamState(), { ...base, paused: true }).chainHeldUserTurn).toBe(false);
    expect(reduceChatFrame(initialChatStreamState(), { ...base, paused: true, heldUserTurn: false }).chainHeldUserTurn).toBe(false);
  });
  // (deleted: "a missing reason reads as null" — `reason` is required on a
  // chainComplete frame now; the vertical's `|| 'no_next_speaker'` default
  // stays for the non-chained final done, where `chainReason` is null.)

  it('starts a fresh stream unpaused with no chain reason', () => {
    const fresh = initialChatStreamState();
    expect(fresh.chainPaused).toBe(false);
    expect(fresh.chainReason).toBeNull();
    expect(fresh.chainHeldUserTurn).toBe(false);
  });

  it('carries `heldUserTurn` off the chain-complete frame (bug 137, §C.4)', () => {
    const held = reduceChatFrame(initialChatStreamState(), {
      kind: 'chainComplete',
      reason: 'paused',
      nextSpeakerId: null,
      chainDepth: 0,
      paused: true,
      heldUserTurn: true,
    });
    expect(held.chainHeldUserTurn).toBe(true);
    expect(held.chainPaused).toBe(true);
  });

  it('applies a (flat) answer-confirmation revision to the bubble content', () => {
    const done = foldChatFrames(framesFor(singleTurnTrace, FIXTURE_CHAT_ID));
    const revised = reduceChatFrame(done, {
      kind: 'confirmationResult',
      messageId: 'm1',
      confirmed: true,
      revised: true,
      notes: 'tightened',
      content: 'Cats are truly wonderful.',
    });
    const bubble = assistant(revised.messages)[0];
    expect(bubble.content).toBe('Cats are truly wonderful.');
    expect(bubble.confirmationRevised).toBe(true);
    expect(bubble.confirmed).toBe(true);
    expect(bubble.confirmationNotes).toBe('tightened');
  });

  it('a confirmation without `content` leaves the bubble text alone', () => {
    const done = foldChatFrames(framesFor(singleTurnTrace, FIXTURE_CHAT_ID));
    const s = reduceChatFrame(done, { kind: 'confirmationResult', messageId: 'm1', confirmed: false, revised: false, notes: null });
    expect(assistant(s.messages)[0]).toMatchObject({ content: 'Cats are wonderful.', confirmed: false, confirmationChecked: true });
  });

  // P4.145 — v4's `fileProcessing` frame is server telemetry no v4 client
  // reads. An entry's NESTED `error` is never the stream's error.
  const failedEntryFrame: ChatStreamFrame = {
    kind: 'fileProcessing',
    info: [{ filename: 'a.txt', type: 'unsupported', usedImageDescriptionLLM: false, error: 'not supported' }],
  };

  it('ignores a fileProcessing frame — the previous state comes back by reference', () => {
    const prev = initialChatStreamState();
    expect(reduceChatFrame(prev, failedEntryFrame)).toBe(prev);
  });

  it('a fileProcessing frame mid-stream leaves the live turn untouched (a nested error is not the stream error)', () => {
    const frames = framesFor(singleTurnTrace, FIXTURE_CHAT_ID);
    const mid = frames.slice(0, -1).reduce(reduceChatFrame, initialChatStreamState());
    const after = reduceChatFrame(mid, failedEntryFrame);
    expect(after).toBe(mid);
    expect(after.error).toBeNull();
  });

  it('ignores a swipeDone frame on a Salon send (v4 `handleFrame`)', () => {
    const prev = initialChatStreamState();
    expect(reduceChatFrame(prev, { kind: 'swipeDone', message: { id: 'm' } })).toBe(prev);
  });
});

/** P4.D301 — bug 242: only a CHAINED `turnStart` (`chainDepth > 0`) enters a chain. */
describe('chat stream reducer — the depth-0 turnStart (bug 242)', () => {
  const TURN_START: ChatStreamFrame = { kind: 'turnStart', participantId: 'participant-1', characterName: 'Ada', chainDepth: 0 };

  it('runs the end-of-turn path after a depth-0 turnStart and done (v4 case 1)', () => {
    const final = foldChatFrames([
      TURN_START,
      { kind: 'content', text: 'Good evening to you.' },
      doneFrame({ messageId: 'assistant-1', participantId: 'participant-1' }),
    ]);
    expect(final.inChain).toBe(false);
    const bubbles = assistant(final.messages);
    expect(bubbles).toHaveLength(1);
    expect(bubbles[0]).toMatchObject({ id: 'assistant-1', content: 'Good evening to you.', intermediate: false });
    expect(final.finalDone?.messageId).toBe('assistant-1');
  });

  it("carries outcome 'empty' to the final done after a depth-0 turnStart (v4 case 2 — v5 converges)", () => {
    const final = foldChatFrames([
      TURN_START,
      doneFrame({ outcome: 'empty', messageId: null, participantId: 'participant-1', emptyResponseReason: 'Nothing.' }),
    ]);
    expect(final.finalDone).toMatchObject({ outcome: 'empty', emptyResponse: true, emptyResponseReason: 'Nothing.' });
    expect(final.waitingForResponse).toBe(false);
  });

  it('a depth-1 turnStart still enters the chain', () => {
    expect(reduceChatFrame(initialChatStreamState(), { ...TURN_START, chainDepth: 1 }).inChain).toBe(true);
  });

  it('a depth-0 turnStart never takes a chain back out (v4 only declines to set)', () => {
    const inChain = reduceChatFrame(initialChatStreamState(), { ...TURN_START, chainDepth: 1 });
    expect(reduceChatFrame(inChain, TURN_START).inChain).toBe(true);
  });
  // (deleted: "an absent chainDepth reads as 0" — `chainDepth` is required.)

  it("emits a bubble for an empty-content depth-0 done with a messageId — v5's one bug-242 symptom", () => {
    const final = foldChatFrames([
      TURN_START,
      { kind: 'content', text: '   ' },
      doneFrame({ messageId: 'assistant-2', participantId: 'participant-1' }),
    ]);
    expect(assistant(final.messages)).toEqual([expect.objectContaining({ id: 'assistant-2', intermediate: false })]);
  });
});

/** P4.D301 — bug 241: the error frame's two persistence flags. */
describe('chat stream reducer — the error frame persistence flags (bug 241)', () => {
  it('records both flags off a full error frame', () => {
    const s = reduceChatFrame(initialChatStreamState(), {
      kind: 'error',
      message: 'Failed to generate response',
      errorType: 'Error',
      details: 'No API key configured for this connection profile',
      userMessagePersisted: false,
      pendingToolResultsPersisted: true,
    });
    expect(s.error).toBe('Failed to generate response: No API key configured for this connection profile');
    expect(s.errorSubmission).toEqual({ userMessagePersisted: false, pendingToolResultsPersisted: true });
  });

  it('an empty `details` adds nothing to the message', () => {
    const s = reduceChatFrame(initialChatStreamState(), { kind: 'error', message: 'Failed', errorType: 'fatal_error', details: '' });
    expect(s.error).toBe('Failed');
    expect(s.errorSubmission).toEqual({});
  });
  // (deleted: "a string 'true' is not a flag" — the parser drops such a frame
  // whole; `stream-frames.oracle.spec.ts` pins the drop.)

  it('starts null', () => {
    expect(initialChatStreamState().errorSubmission).toBeNull();
  });
});

describe('chat stream reducer — done outcomes, raise a hand / stop (v4 2b5e8ee24, 604a082c4)', () => {
  it.each([
    ['finished', { skipped: false, yielded: false, stopped: false, emptyResponse: false }],
    ['skipped', { skipped: true, yielded: false, stopped: false, emptyResponse: false }],
    ['yielded', { skipped: true, yielded: true, stopped: false, emptyResponse: false }],
    ['stopped', { skipped: false, yielded: false, stopped: true, emptyResponse: false }],
    ['empty', { skipped: false, yielded: false, stopped: false, emptyResponse: true }],
  ] as const)('outcome %s derives the legacy flags in one place (v4 legacyOutcomeFlags)', (outcome, flags) => {
    const s = reduceChatFrame(initialChatStreamState(), doneFrame({ outcome, messageId: 'm' }));
    expect(s.finalDone).toMatchObject({ outcome, ...flags });
  });

  it.each(['skipped', 'yielded', 'stopped'] as const)('a %s done resets without a bubble, even with a messageId', (outcome) => {
    const s = foldChatFrames([
      { kind: 'turnStart', participantId: 'p2', characterName: 'Bertie', chainDepth: 0 },
      { kind: 'content', text: 'Well' },
      doneFrame({ outcome, messageId: 'm-x', participantId: 'p2' }),
    ]);
    expect(assistant(s.messages)).toHaveLength(0);
    expect(s.content).toBe('');
    expect(s.streaming).toBe(false);
  });

  it('a stop with prose is an ordinary done', () => {
    const s = foldChatFrames([
      { kind: 'turnStart', participantId: 'p2', characterName: 'Bertie', chainDepth: 0 },
      { kind: 'content', text: 'Well, as I was say' },
      doneFrame({ messageId: 'm1', participantId: 'p2' }),
    ]);
    expect(s.finalDone?.stopped).toBe(false);
    expect(assistant(s.messages)).toHaveLength(1);
  });

  it('carries handAnswered as a counted transition', () => {
    const one = reduceChatFrame(initialChatStreamState(), {
      kind: 'handAnswered',
      participantId: 'p2',
      characterName: 'Bertie',
      yielded: false,
    });
    expect(one.handAnswered).toEqual({ participantId: 'p2', characterName: 'Bertie', yielded: false });
    expect(one.handAnsweredSeq).toBe(1);
    const two = reduceChatFrame(one, { kind: 'handAnswered', participantId: 'p3', characterName: '', yielded: true });
    expect(two.handAnsweredSeq).toBe(2);
    expect(two.handAnswered).toEqual({ participantId: 'p3', characterName: '', yielded: true });
  });

  it('carries handAsked as a counted transition (v4 9eab8d2da)', () => {
    const s = reduceChatFrame(initialChatStreamState(), { kind: 'handAsked', participantId: 'p2', characterName: 'Bertie' });
    expect(s.handAsked).toEqual({ participantId: 'p2', characterName: 'Bertie' });
    expect(s.handAskedSeq).toBe(1);
    expect(s.handAnsweredSeq).toBe(0);
  });

  it('THE HAND-SWALLOW FIX: a seat-tagged hand frame moves the carry (item 1, red on the CSS parent)', () => {
    const asked = reduceChatFrame(initialChatStreamState(), {
      kind: 'handAsked',
      participantId: 'p1',
      characterName: 'Friday',
      seat: seat('p1'),
    });
    expect(asked.handAskedSeq).toBe(1);
    const answered = reduceChatFrame(asked, {
      kind: 'handAnswered',
      participantId: 'p1',
      characterName: 'Friday',
      yielded: false,
      seat: seat('p1'),
    });
    expect(answered.handAnsweredSeq).toBe(1);
    expect(answered.handAnswered).toEqual({ participantId: 'p1', characterName: 'Friday', yielded: false });
  });

  it('folds the bare-yield frame order whole: one host note, no assistant bubble', () => {
    const s = foldChatFrames([
      { kind: 'turnStart', participantId: 'p2', characterName: 'Bertie', chainDepth: 1 },
      { kind: 'handAsked', participantId: 'p2', characterName: 'Bertie' },
      { kind: 'handAnswered', participantId: 'p2', characterName: 'Bertie', yielded: true },
      {
        kind: 'posted',
        what: 'hostAnnouncement',
        message: { id: 'host-y', type: 'message', systemSender: 'host', content: 'Bertie yields the floor to you.' },
      },
      doneFrame({ outcome: 'yielded', messageId: null, participantId: 'p2', skippedParticipantId: 'p2' }),
      { kind: 'chainComplete', reason: 'yielded', nextSpeakerId: null, chainDepth: 1, paused: false },
    ]);
    expect(s.messages.filter((m) => m.kind === 'host')).toHaveLength(1);
    expect(assistant(s.messages)).toHaveLength(0);
    expect(s.finished).toBe(true);
    expect(s.chainReason).toBe('yielded');
  });
});

// === P4.D344: Storyteller mode ===
describe('chat stream reducer — the storyteller board frames (P4.D344)', () => {
  const bidOf = (participantId: string) => ({
    participantId,
    characterId: `c-${participantId}`,
    characterName: participantId,
    status: 'bid' as const,
    tier: 'simple' as const,
    statedTier: 'simple' as const,
    tierRaised: false,
    tools: [],
    plan: 'Tea.',
    reason: null,
    talkativeness: 0.5,
    silent: false,
  });
  const round = {
    id: 'r-1',
    chatId: 'chat-1',
    triggerMessageId: 'm-1',
    status: 'open' as const,
    bids: [],
    createdAt: 'x',
    updatedAt: 'x',
  };

  it('starts empty', () => {
    expect(initialChatStreamState().storyteller).toBeNull();
    expect(initialChatStreamState().storytellerSeq).toBe(0);
  });

  it('carries each of the three frames with a bumped seq (the SAME event shapes the board reads), and folds nothing else from them', () => {
    const base = { ...initialChatStreamState(), waitingForResponse: true, content: 'held' };
    const a = reduceChatFrame(base, { kind: 'storytellerRound', round });
    expect(a).toEqual({ ...base, storyteller: { kind: 'round', round }, storytellerSeq: 1 });
    // storytellerBid is FLAT on the wire now (was nested).
    const b = reduceChatFrame(a, { kind: 'storytellerBid', roundId: 'r-1', bid: bidOf('x') });
    expect(b.storyteller).toEqual({ kind: 'bid', roundId: 'r-1', bid: bidOf('x') });
    expect(b.storytellerSeq).toBe(2);
    const d = reduceChatFrame(b, { kind: 'roundOpen', roundId: 'r-1' });
    expect(d.storyteller).toEqual({ kind: 'open', roundId: 'r-1' });
    expect(d.storytellerSeq).toBe(3);
    expect(d.content).toBe('held');
    expect(d.waitingForResponse).toBe(true);
  });
  // (deleted: "a stray status beside the board key" and "a roundOpen without a
  // roundId" — one frame is one kind; a roundOpen needs its roundId to parse.)

  it('a seat-tagged board frame is carried exactly as an untagged one', () => {
    const s = reduceChatFrame(initialChatStreamState(), { kind: 'roundOpen', roundId: 'r-1', seat: seat('a') });
    expect(s.storyteller).toEqual({ kind: 'open', roundId: 'r-1' });
    expect(s.concurrentStreams).toEqual([]);
  });

  it("a storyteller send's chainComplete rides the string chainReason — round_open — and leaves nothing waiting", () => {
    const s = foldChatFrames([
      { kind: 'status', status: { stage: 'initializing', message: '…' } },
      { kind: 'turnStart', participantId: 'p-1', characterName: 'Ada', chainDepth: 0 },
      { kind: 'storytellerRound', round },
      { kind: 'storytellerBid', roundId: 'r-1', bid: bidOf('p-1') },
      { kind: 'roundOpen', roundId: 'r-1' },
      { kind: 'chainComplete', reason: 'round_open', nextSpeakerId: null, chainDepth: 0 },
    ]);
    expect(s.chainReason).toBe('round_open');
    expect(s.chainPaused).toBe(false);
    expect(s.waitingForResponse).toBe(false);
    expect(s.finished).toBe(true);
    expect(s.messages).toEqual([]);
    expect(s.storytellerSeq).toBe(3);
  });
});

describe('chat stream reducer — concurrent storyteller seats (P4.D344, v4 handleSeatFrame)', () => {
  it('keeps one bubble per seat, interleaved, and leaves the shared slot alone', () => {
    const base = { ...initialChatStreamState(), waitingForResponse: true };
    const s = foldChatFrames(
      [
        { kind: 'turnStart', participantId: 'a', characterName: 'A', chainDepth: 0, seat: seat('a') },
        { kind: 'turnStart', participantId: 'b', characterName: 'B', chainDepth: 0, seat: seat('b') },
        { kind: 'content', text: 'Tea', seat: seat('a') },
        { kind: 'content', text: 'Cof', seat: seat('b') },
        { kind: 'content', text: ' first.', seat: seat('a') },
        { kind: 'content', text: 'fee.', seat: seat('b') },
      ],
      base,
    );
    expect(s.concurrentStreams).toEqual([
      { participantId: 'a', content: 'Tea first.' },
      { participantId: 'b', content: 'Coffee.' },
    ]);
    expect(s.waitingForResponse).toBe(false);
    expect(s.content).toBe('');
    expect(s.streaming).toBe(false);
    expect(s.respondingParticipantId).toBeNull();
  });

  it('the SEAT names the bubble, not a frame field', () => {
    const s = reduceChatFrame(initialChatStreamState(), {
      kind: 'turnStart',
      participantId: 'own',
      characterName: 'Ada',
      chainDepth: 0,
      seat: seat('seat-pid'),
    });
    expect(s.concurrentStreams).toEqual([{ participantId: 'seat-pid', content: '' }]);
  });

  it('a content frame for an unknown seat opens its bubble; an empty one is ignored', () => {
    const s = reduceChatFrame(initialChatStreamState(), { kind: 'content', text: 'Hm.', seat: seat('c') });
    expect(s.concurrentStreams).toEqual([{ participantId: 'c', content: 'Hm.' }]);
    expect(reduceChatFrame(s, { kind: 'content', text: '', seat: seat('d') })).toBe(s);
  });

  it('done settles the bubble with v4’s fields; doneLeavesNoReply outcomes push nothing', () => {
    let s = foldChatFrames([
      { kind: 'turnStart', participantId: 'a', characterName: 'A', chainDepth: 0, seat: seat('a') },
      { kind: 'content', text: 'Tea first.', seat: seat('a') },
      { kind: 'turnStart', participantId: 'b', characterName: 'B', chainDepth: 0, seat: seat('b') },
      { kind: 'content', text: 'Nothing.', seat: seat('b') },
    ]);
    s = reduceChatFrame(s, { ...doneFrame({ messageId: 'm-a', participantId: 'a', provider: 'p', modelName: 'm' }), seat: seat('a') });
    s = reduceChatFrame(s, { ...doneFrame({ outcome: 'skipped', messageId: 'm-b', participantId: 'b' }), seat: seat('b') });
    expect(s.concurrentStreams).toEqual([]);
    expect(s.messages).toEqual([
      {
        kind: 'assistant',
        id: 'm-a',
        content: 'Tea first.',
        participantId: 'a',
        provider: 'p',
        modelName: 'm',
        routeTrail: null,
        isSilentMessage: undefined,
        reasoningContent: null,
        reasoningSegments: null,
        intermediate: true,
      },
    ]);
    expect(reduceChatFrame(s, { ...doneFrame({ messageId: 'm-a' }), seat: seat('a') }).messages).toHaveLength(1);
  });

  it('a seat-tagged posted surfaces deduped; a seat-tagged error is recorded bare', () => {
    const s = foldChatFrames([
      { kind: 'posted', what: 'hostAnnouncement', message: { id: 'h' }, seat: seat('a') },
      { kind: 'posted', what: 'hostAnnouncement', message: { id: 'h' }, seat: seat('b') },
      { kind: 'error', message: 'Bo fell over', errorType: 'x', details: 'boom', seat: seat('b') },
    ]);
    expect(s.messages.map((m) => m.id)).toEqual(['h']);
    expect(s.error).toBe('Bo fell over');
  });

  it.each([
    { kind: 'reasoning', text: 'x' },
    { kind: 'status', status: { stage: 's', message: 'm' } },
    { kind: 'toolsDetected', names: ['a'], arguments: [{}] },
    { kind: 'toolResult', name: 'a', success: true },
    { kind: 'turnComplete', participantId: 'a', messageId: 'm', chainDepth: 1 },
    { kind: 'chainComplete', reason: 'user_turn', nextSpeakerId: null, chainDepth: 0 },
    { kind: 'pendingExternalTurn', messageId: 'm', participantId: 'a', characterName: 'A' },
    { kind: 'confirmationResult', messageId: 'm', confirmed: true, revised: false, notes: null },
    { kind: 'swipeDone', message: { id: 'm' } },
    { kind: 'fileProcessing', info: [] },
  ] as ChatStreamFrame[])('a seat-tagged $kind is ignored (the state comes back by reference)', (frame) => {
    const prev = initialChatStreamState();
    expect(reduceChatFrame(prev, { ...frame, seat: seat('a') } as ChatStreamFrame)).toBe(prev);
  });
});
// === end P4.D344 ===

/**
 * v4's golden streams (`useSSEStreaming-golden.test.tsx:120-179`), replayed
 * from the SAME frames v4 replays (the stream-frames corpus, section d) and
 * asserting v5's visible outcome.
 */
describe('chat stream reducer — v4 golden streams', () => {
  it('plain reply: one assistant bubble with the confirmation patched on', () => {
    const s = foldChatFrames(goldenStream('plain-reply'));
    const bubbles = assistant(s.messages);
    expect(bubbles).toHaveLength(1);
    expect(bubbles[0]).toMatchObject({ id: 'm-1', content: 'Good evening, old thing.', participantId: 'p-ada', confirmed: true, confirmationChecked: true });
    expect(s.finished).toBe(true);
    expect(s.chainReason).toBe('user_turn');
    expect(s.finalDone?.attachmentResults).toEqual({ sent: ['f-1'], failed: [] });
  });

  it('tool reply: the batch resolves, the Pascal outcome surfaces, the bubble carries the prose', () => {
    const frames = goldenStream('tool-reply');
    const beforeDone = frames.slice(0, frames.findIndex((f) => f.kind === 'done')).reduce(reduceChatFrame, initialChatStreamState());
    expect(beforeDone.toolBatches).toHaveLength(1);
    expect(beforeDone.toolBatches[0].offset).toBe('Let me look. '.length);
    expect(beforeDone.toolBatches[0].calls.map((c) => c.status)).toEqual(['success', 'error']);
    expect(beforeDone.toolBatches[0].calls[1].errorText).toBe('Error: no image profile');
    const s = foldChatFrames(frames);
    expect(s.messages.map((m) => [m.kind, m.id])).toEqual([
      ['pascal', 'm-pascal'],
      ['assistant', 'm-2'],
    ]);
    expect(assistant(s.messages)[0].content).toBe('Let me look. Found it.');
    expect(s.finalDone?.toolsExecuted).toBe(true);
  });

  it('storyteller all-at-once with a raised hand: board events, both hand frames carried, one Host note, one bubble', () => {
    const s = foldChatFrames(goldenStream('storyteller-concurrent-hand'));
    expect(s.storytellerSeq).toBe(3);
    expect(s.storyteller).toEqual({ kind: 'open', roundId: 'r-1' });
    expect(s.handAskedSeq).toBe(1);
    expect(s.handAsked).toEqual({ participantId: 'p-bo', characterName: 'Bo' });
    expect(s.handAnsweredSeq).toBe(1);
    expect(s.handAnswered).toEqual({ participantId: 'p-bo', characterName: 'Bo', yielded: true });
    expect(s.messages.filter((m) => m.kind === 'host').map((m) => m.id)).toEqual(['m-host']);
    // Ada settles a bubble with her seat's text; Bo's yield leaves none; the
    // chained Cy that follows the round speaks on the shared slot.
    expect(assistant(s.messages).map((m) => [m.participantId, m.content, m.intermediate])).toEqual([
      ['p-ada', 'I say', true],
      ['p-cy', 'Next.', true],
    ]);
    expect(s.messages.filter((m) => m.kind === 'carina').map((m) => m.id)).toEqual(['m-carina']);
    expect(s).toMatchObject({ chainReason: 'yielded', chainPaused: true, chainHeldUserTurn: false });
    expect(s.concurrentStreams).toEqual([]);
  });

  it('courier: the pending-external flag rides finalDone, no bubble', () => {
    const s = foldChatFrames(goldenStream('courier-external'));
    expect(s.finalDone?.pendingExternalTurn).toBe(true);
    expect(assistant(s.messages)).toHaveLength(0);
    expect(s.finished).toBe(true);
  });

  it('failures: the rescue status, the empty / stopped / skipped outcomes, then the error', () => {
    const frames = goldenStream('failures');
    expect(reduceChatFrame(initialChatStreamState(), frames[0]).status?.stage).toBe('failing-over');
    expect(frames.filter((f) => f.kind === 'done').map((f) => (f as { outcome: string }).outcome)).toEqual(['empty', 'stopped', 'skipped']);
    const s = foldChatFrames(frames);
    expect(assistant(s.messages)).toHaveLength(0);
    expect(s.error).toBe('Failed to send: boom');
    expect(s.errorSubmission).toEqual({ userMessagePersisted: true, pendingToolResultsPersisted: false });
  });

  it('swipe: a Salon fold ignores swipeDone (the swipe reader is `regeneration.state.ts`)', () => {
    const s = foldChatFrames(goldenStream('swipe'));
    expect(s.content).toBe('Ano');
    expect(s.reasoning).toBe('thinking');
    expect(s.messages).toEqual([]);
  });
});
