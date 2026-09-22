import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { coreStreamStub } from '../core/core-client.testing';
import type { PendingInformBatch } from '../core/core-contract';
import { RealtimeService } from '../core/realtime.service';
import { ToastService } from '../ui/toast.service';
import { PendingInformChips, firstInformLine } from './pending-inform-chips';

/**
 * Parity spec against v4 `components/chat/PendingInformChips.tsx` and its own
 * `__tests__/unit/components/chat/PendingInformChips.test.tsx` at `f45a517a9`
 * (`e7d77bb60`). Five cases, in v4's order and under v4's own names:
 *
 *   renders nothing when no inform is pending;
 *   names the pending seats and hovers the first line of the passage;
 *   skips a batch whose seats have all left the chat;
 *   cancels the batch through `?action=cancel-inform`;
 *   does not poll while the realtime socket is up.
 *
 * v4's fixture ids and passage are carried verbatim. Two transport
 * differences, both by construction: the reads/writes are `chatInformsList` /
 * `chatInformCancel` dispatches rather than v4's two `fetch`es, and the poll
 * gate is v5's `RealtimeService.refetchInterval` — the real service, driven
 * through the real `CoreClient.connection` signal — rather than v4's mocked
 * `useRealtimeRefetchInterval` hook. The last case is therefore STRONGER than
 * v4's: v4 asserts its mock was called with `60_000` and answered false; this
 * drives the actual connection state and asserts the interval the query gets
 * on BOTH sides of it.
 */

const ALICE = 'aaaa1111-1111-1111-1111-111111111111';
const BOB = 'bbbb2222-2222-2222-2222-222222222222';
const GHOST = 'dddd4444-4444-4444-4444-444444444444';

const NAMES: Record<string, string> = { [ALICE]: 'Alice', [BOB]: 'Bob' };

const BATCH: PendingInformBatch = {
  batchId: 'batch-1',
  contentMarkdown:
    'You see that Alice slipped the letter into her sleeve.\n\nShe is not subtle.',
  createdAt: '2026-09-19T21:14:00.000Z',
  recordMessageId: 'msg-1',
  pendingParticipantIds: [ALICE, BOB],
};

interface Stub {
  calls: Record<string, unknown>[];
  stream: ReturnType<typeof coreStreamStub>;
  client: Partial<CoreClient>;
}

function stub(batches: PendingInformBatch[], cancel?: Error): Stub {
  const calls: Record<string, unknown>[] = [];
  const stream = coreStreamStub();
  const dispatchData = vi.fn(async (req: Record<string, unknown>) => {
    calls.push(req);
    if (req['type'] === 'chatInformCancel') {
      if (cancel) throw cancel;
      return { success: true, removed: 1, recordDeleted: true };
    }
    return { batches };
  });
  return {
    calls,
    stream,
    client: { ...stream, dispatchData: dispatchData as unknown as CoreClient['dispatchData'] },
  };
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 8; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function mount(s: Stub): Promise<ComponentFixture<PendingInformChips>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [PendingInformChips],
    providers: [
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: s.client },
    ],
  });
  // The realtime hub is a root service the app root injects at bootstrap; a
  // spec that leans on its `connected` gate must do the same.
  TestBed.inject(RealtimeService);
  const fixture = TestBed.createComponent(PendingInformChips);
  fixture.componentRef.setInput('chatId', 'chat-1');
  fixture.componentRef.setInput('participantNames', NAMES);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

function chipLabels(fixture: ComponentFixture<unknown>): string[] {
  return [...fixture.nativeElement.querySelectorAll('.qt-chat-tool-result-chip span')].map((s) =>
    (s.textContent ?? '').trim(),
  );
}

describe('PendingInformChips (v4 components/chat/PendingInformChips.tsx @ f45a517a9)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('renders nothing when no inform is pending', async () => {
    const s = stub([]);
    const fixture = await mount(s);
    expect(s.calls.some((c) => c['type'] === 'chatInformsList')).toBe(true);
    expect(fixture.nativeElement.querySelector('.qt-chat-attachment-list')).toBeNull();
  });

  it('names the pending seats and hovers the first line of the passage', async () => {
    const fixture = await mount(stub([BATCH]));
    expect(chipLabels(fixture)).toContain('Informing Alice, Bob before their next turn');
    const chip = fixture.nativeElement.querySelector('.qt-chat-tool-result-chip');
    expect(chip.getAttribute('title')).toBe(
      'You see that Alice slipped the letter into her sleeve.',
    );
  });

  it('skips a batch whose seats have all left the chat', async () => {
    const fixture = await mount(
      stub([{ ...BATCH, batchId: 'batch-2', pendingParticipantIds: [GHOST] }]),
    );
    expect(fixture.nativeElement.querySelector('.qt-chat-tool-result-chip')).toBeNull();
    // And the wrapper goes with it — a nameless batch must not leave an empty row.
    expect(fixture.nativeElement.querySelector('.qt-chat-attachment-list')).toBeNull();
  });

  it('cancels the batch through chatInformCancel', async () => {
    const s = stub([BATCH]);
    const fixture = await mount(s);
    const remove = fixture.nativeElement.querySelector(
      '[aria-label="Withdraw the inform for Alice, Bob"]',
    ) as HTMLButtonElement;
    expect(remove).toBeTruthy();
    remove.click();
    await settle(fixture);

    const call = s.calls.find((c) => c['type'] === 'chatInformCancel');
    expect(call).toEqual({ type: 'chatInformCancel', chatId: 'chat-1', batchId: 'batch-1' });
    expect(TestBed.inject(ToastService).toasts().map((t) => t.message)).toContain(
      'The note has been withdrawn',
    );
  });

  it('does not poll while the realtime socket is up', async () => {
    const s = stub([BATCH]);
    const fixture = await mount(s);
    const realtime = TestBed.inject(RealtimeService);

    // The component asks the gate for its interval rather than hard-coding one,
    // and the gate answers false whenever the socket is connected.
    s.stream.connection.set('open');
    await settle(fixture);
    expect(realtime.connected()).toBe(true);
    expect(realtime.refetchInterval(60_000)).toBe(false);

    // …and degrades to the pre-realtime cadence when it is not.
    s.stream.connection.set('closed');
    await settle(fixture);
    expect(realtime.refetchInterval(60_000)).toBe(60_000);
  });
});

describe('firstInformLine (v4 `firstLine`)', () => {
  it('takes the first NON-BLANK line, trimmed', () => {
    expect(firstInformLine('\n\n  You see the gate.  \nand more')).toBe('You see the gate.');
  });

  it('answers the empty string for a passage with nothing in it', () => {
    expect(firstInformLine('')).toBe('');
    expect(firstInformLine('\n   \n')).toBe('');
  });
});
