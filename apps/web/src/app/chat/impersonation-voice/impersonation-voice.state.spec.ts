import { TestBed } from '@angular/core/testing';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { ToastService } from '../../ui/toast.service';
import {
  ImpersonationVoiceState,
  type ImpersonationVoiceHost,
  type InterceptArgs,
  type PendingSend,
  type RehearsalTarget,
} from './impersonation-voice.state';

/**
 * The In Their Own Words state machine (v4 `useImpersonationVoice`). Every
 * transition v4's hook makes, plus the bypass latch's three-stage life (set by a
 * dialog send → consumed by the next intercept → cleared on every close).
 */

const SEAT_ID = 'seat-evangeline';

const TARGET: RehearsalTarget = {
  participantId: SEAT_ID,
  characterName: 'Evangeline',
  characterTitle: 'the Aeronaut',
  profileName: 'Her own desk',
  modelName: 'gpt-seat',
  systemPrompts: [],
  selectedSystemPromptId: null,
};

function interceptArgs(over: Partial<InterceptArgs> = {}): InterceptArgs {
  return {
    text: 'I tell him I will take the job.',
    seat: { id: SEAT_ID, type: 'CHARACTER', controlledBy: 'llm' },
    seatTarget: TARGET,
    enabled: true,
    impersonatingParticipantIds: [SEAT_ID],
    fileIds: [],
    pending: [],
    ...over,
  };
}

interface Rig {
  state: ImpersonationVoiceState;
  dispatched: Record<string, unknown>[];
  sent: { final: string; stash: PendingSend }[];
  focused: number;
  errors: string[];
  answer: (req: Record<string, unknown>) => Promise<Record<string, unknown>>;
  /** Make the host's send loop back into `intercept`, v4's "on the way out". */
  reentrantSend: (fn: (state: ImpersonationVoiceState) => void) => void;
}

function rig(
  answer: (req: Record<string, unknown>) => Promise<Record<string, unknown>> = async () => ({
    success: true,
    proposedMarkdown: 'I shall take the position, sir.',
    profileName: 'Her own desk',
    modelName: 'gpt-seat',
  }),
  chatId: string | null = 'chat-1',
): Rig {
  const dispatched: Record<string, unknown>[] = [];
  const sent: { final: string; stash: PendingSend }[] = [];
  const errors: string[] = [];
  const box = { focused: 0 };

  const core = {
    dispatchData: vi.fn(async (req: Record<string, unknown>) => {
      dispatched.push(req);
      return answer(req);
    }),
  };
  const toasts = { showError: (m: string) => errors.push(m) };

  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    providers: [
      ImpersonationVoiceState,
      { provide: CoreClient, useValue: core as unknown as CoreClient },
      { provide: ToastService, useValue: toasts as unknown as ToastService },
    ],
  });
  const state = TestBed.inject(ImpersonationVoiceState);
  let reentry: ((state: ImpersonationVoiceState) => void) | null = null;
  const host: ImpersonationVoiceHost = {
    chatId: () => chatId,
    sendFinal: (final, stash) => {
      sent.push({ final, stash });
      reentry?.(state);
    },
    focusComposer: () => (box.focused += 1),
  };
  state.attach(host);

  return {
    state,
    dispatched,
    sent,
    errors,
    answer,
    reentrantSend: (fn: (state: ImpersonationVoiceState) => void) => {
      reentry = fn;
    },
    get focused() {
      return box.focused;
    },
  } as Rig;
}

/** Flush the preview's microtasks. */
const flush = () => new Promise((r) => setTimeout(r, 0));

describe('ImpersonationVoiceState.intercept', () => {
  it('takes the submit over and opens on the stashed draft', async () => {
    const r = rig();
    expect(r.state.intercept(interceptArgs({ fileIds: ['f-1'], pending: ['roll'] }))).toBe(true);
    expect(r.state.isOpen()).toBe(true);
    expect(r.state.seed()).toBe('I tell him I will take the job.');
    expect(r.state.target()).toEqual(TARGET);
    // v4 sets stage BEFORE awaiting, so the dialog opens on the quill.
    expect(r.state.stage()).toBe('generating');
    await flush();
    expect(r.state.stage()).toBe('review');
    expect(r.state.proposal()).toBe('I shall take the position, sir.');
    expect(r.state.resolvedVoice()).toEqual({
      profileName: 'Her own desk',
      modelName: 'gpt-seat',
    });
  });

  it('sends the §B wire shape, with both optional ids OMITTED on the first run', async () => {
    const r = rig();
    r.state.intercept(interceptArgs({ text: '  a padded line  ' }));
    await flush();
    expect(r.dispatched).toEqual([
      {
        type: 'chatImpersonationVoicePreview',
        chatId: 'chat-1',
        participantId: SEAT_ID,
        // v4 `seedMarkdown: seed.trim()`.
        seedMarkdown: 'a padded line',
      },
    ]);
    expect('connectionProfileId' in r.dispatched[0]).toBe(false);
    expect('systemPromptId' in r.dispatched[0]).toBe(false);
  });

  it('declines and changes nothing when the gate says no', () => {
    const r = rig();
    expect(r.state.intercept(interceptArgs({ enabled: false }))).toBe(false);
    expect(r.state.isOpen()).toBe(false);
    expect(r.dispatched).toEqual([]);
  });

  it('declines when the gate fires but no target could be built', () => {
    const r = rig();
    expect(r.state.intercept(interceptArgs({ seatTarget: null }))).toBe(false);
    expect(r.state.isOpen()).toBe(false);
    expect(r.dispatched).toEqual([]);
  });

  it('an attachment-only send is never rehearsed', () => {
    const r = rig();
    expect(r.state.intercept(interceptArgs({ text: '   ', fileIds: ['f-1'] }))).toBe(false);
    expect(r.state.intercept(interceptArgs({ text: '', pending: ['roll'] }))).toBe(false);
    expect(r.dispatched).toEqual([]);
  });

  it('resets the overrides and the resolved voice on every open', async () => {
    const r = rig();
    r.state.intercept(interceptArgs());
    await flush();
    r.state.changeProfile('p-2');
    await flush();
    r.state.close();

    r.state.intercept(interceptArgs());
    expect(r.state.profileOverride()).toBeNull();
    expect(r.state.systemPromptOverride()).toBeNull();
    expect(r.state.resolvedVoice()).toBeNull();
  });
});

describe('ImpersonationVoiceState — the failed preview', () => {
  it('stays in review with an empty proposal, so both doors stay open', async () => {
    const r = rig(async () => {
      throw new Error('No connection profile to rewrite with');
    });
    r.state.intercept(interceptArgs());
    await flush();
    expect(r.state.stage()).toBe('review');
    expect(r.state.proposal()).toBe('');
    expect(r.errors).toEqual(['No connection profile to rewrite with']);
  });

  it("falls back to v4's sentence when the refusal carries none", async () => {
    const r = rig(async () => {
      throw new Error('');
    });
    r.state.intercept(interceptArgs());
    await flush();
    expect(r.errors).toEqual(['Failed to restate the line in character']);
    expect(r.state.stage()).toBe('review');
  });

  it('an empty proposedMarkdown is not an error, just an empty proposal', async () => {
    const r = rig(async () => ({ success: true, proposedMarkdown: '   ' }));
    r.state.intercept(interceptArgs());
    await flush();
    expect(r.state.proposal()).toBe('');
    expect(r.errors).toEqual([]);
    expect(r.state.stage()).toBe('review');
  });

  it('leaves the resolved voice alone when the body names neither (v4 raw-body gate)', async () => {
    const r = rig(async () => ({ success: true, proposedMarkdown: 'said so' }));
    r.state.intercept(interceptArgs());
    await flush();
    expect(r.state.resolvedVoice()).toBeNull();
  });

  it('sets the resolved voice when only ONE of the two is named', async () => {
    const r = rig(async () => ({
      success: true,
      proposedMarkdown: 'said so',
      profileName: 'A stand-in desk',
    }));
    r.state.intercept(interceptArgs());
    await flush();
    expect(r.state.resolvedVoice()).toEqual({ profileName: 'A stand-in desk', modelName: '' });
  });
});

describe('ImpersonationVoiceState — the re-runs', () => {
  it('regenerate re-runs on the LIVE draft with the current overrides', async () => {
    const r = rig();
    r.state.intercept(interceptArgs());
    await flush();
    r.state.setSeed('I tell him nothing at all.');
    r.state.changeSystemPrompt('sp-2');
    await flush();
    r.dispatched.length = 0;
    r.state.regenerate();
    await flush();
    expect(r.dispatched).toEqual([
      {
        type: 'chatImpersonationVoicePreview',
        chatId: 'chat-1',
        participantId: SEAT_ID,
        seedMarkdown: 'I tell him nothing at all.',
        systemPromptId: 'sp-2',
      },
    ]);
  });

  it('changing the profile drops the proposal and re-runs', async () => {
    const r = rig();
    r.state.intercept(interceptArgs());
    await flush();
    expect(r.state.proposal()).not.toBe('');

    r.state.changeProfile('p-2');
    // Synchronously: generating, and the old proposal is gone.
    expect(r.state.stage()).toBe('generating');
    expect(r.state.proposal()).toBe('');
    await flush();
    expect(r.state.profileOverride()).toBe('p-2');
    expect(r.dispatched[1]['connectionProfileId']).toBe('p-2');
  });

  it('clearing the profile back to "their own voice" OMITS the key again', async () => {
    const r = rig();
    r.state.intercept(interceptArgs());
    await flush();
    r.state.changeProfile('p-2');
    await flush();
    r.state.changeProfile(null);
    await flush();
    expect(r.state.profileOverride()).toBeNull();
    expect('connectionProfileId' in r.dispatched[2]).toBe(false);
  });

  it('a re-run with no open rehearsal does nothing', async () => {
    const r = rig();
    r.state.regenerate();
    r.state.changeProfile('p-2');
    r.state.changeSystemPrompt('sp-2');
    await flush();
    expect(r.dispatched).toEqual([]);
  });
});

describe('ImpersonationVoiceState — the five doors', () => {
  it('Send posts the proposal with the stash and closes', async () => {
    const r = rig();
    r.state.intercept(interceptArgs({ fileIds: ['f-1'], pending: ['roll'] }));
    await flush();
    r.state.send(r.state.proposal());
    expect(r.sent).toEqual([
      {
        final: 'I shall take the position, sir.',
        stash: {
          seed: 'I tell him I will take the job.',
          fileIds: ['f-1'],
          pending: ['roll'],
        },
      },
    ]);
    expect(r.state.isOpen()).toBe(false);
    expect(r.state.stage()).toBe('idle');
  });

  it('Send as written posts the operator’s own bytes, edits included', async () => {
    const r = rig();
    r.state.intercept(interceptArgs());
    await flush();
    r.state.setSeed('My own words, then.');
    r.state.sendAsWritten();
    expect(r.sent.map((s) => s.final)).toEqual(['My own words, then.']);
  });

  it('Edit original closes without sending and returns the cursor', async () => {
    const r = rig();
    r.state.intercept(interceptArgs());
    await flush();
    r.state.editOriginal();
    expect(r.sent).toEqual([]);
    expect(r.state.isOpen()).toBe(false);
    expect(r.focused).toBe(1);
  });

  it('Cancel closes without sending and returns the cursor', async () => {
    const r = rig();
    r.state.intercept(interceptArgs());
    await flush();
    r.state.cancel();
    expect(r.sent).toEqual([]);
    expect(r.state.isOpen()).toBe(false);
    expect(r.focused).toBe(1);
  });

  it('a send with nothing stashed does nothing', () => {
    const r = rig();
    r.state.send('anything');
    r.state.sendAsWritten();
    expect(r.sent).toEqual([]);
  });
});

describe('ImpersonationVoiceState — the bypass latch', () => {
  /**
   * ⚠ Measured on v4, not taken from its prose. v4's `send` reads
   *
   * ```js
   * bypassOnceRef.current = true
   * void sendMessage(...)
   * close()            // ← and `close` sets bypassOnceRef.current = false
   * ```
   *
   * so the latch is armed for exactly the SYNCHRONOUS duration of the send call
   * and is cleared on the very next line. Its own comment says as much — "so the
   * gate lets it through **if it is ever consulted on the way out**" — and in
   * v4's real flow it never is, because the dialog's Send calls `sendMessage`
   * directly and never re-enters the composer's `onSubmit`. It is a guard
   * against a host whose send path loops back, not a latch that outlives the
   * dialog. Ported verbatim, quirk included, and pinned in both halves here.
   */
  it('is armed for the duration of the send: a submit raised from inside it passes through', async () => {
    const r = rig();
    const fromInside: boolean[] = [];
    r.reentrantSend((state) => {
      // Two submits from inside one send: the first spends the latch, the
      // second finds it spent (v4 "consumed by the next intercept").
      fromInside.push(state.intercept(interceptArgs()));
      fromInside.push(state.intercept(interceptArgs()));
    });

    r.state.intercept(interceptArgs());
    await flush();
    r.state.sendAsWritten();
    expect(fromInside).toEqual([false, true]);
  });

  it('close() on the next line clears it, so the submit AFTER a send rehearses again', async () => {
    const r = rig();
    r.state.intercept(interceptArgs());
    await flush();
    r.state.sendAsWritten();
    expect(r.state.intercept(interceptArgs())).toBe(true);
  });

  it('every close clears it, whether or not anything was sent', async () => {
    const r = rig();
    r.state.intercept(interceptArgs());
    await flush();
    r.state.cancel();
    expect(r.state.intercept(interceptArgs())).toBe(true);
  });
});

describe('ImpersonationVoiceState — no chat', () => {
  it('refuses loudly rather than dispatching a chat-less preview', async () => {
    const r = rig(undefined, null);
    expect(r.state.intercept(interceptArgs())).toBe(true);
    await flush();
    expect(r.dispatched).toEqual([]);
    expect(r.errors).toEqual(['Failed to restate the line in character']);
    expect(r.state.stage()).toBe('review');
  });
});
