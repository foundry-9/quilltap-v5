import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type { CoreResponse } from '../../core/core-contract';
import { WhisperDialog } from './whisper-dialog';

interface Stub {
  calls: Record<string, unknown>[];
  /** Settle the pending dispatch — the v5 analogue of v4's stream draining. */
  finish: (err?: Error) => void;
  client: Partial<CoreClient>;
}

/** A `dispatch` the spec settles by hand, so the close-then-drain order is observable. */
function stub(): Stub {
  const calls: Record<string, unknown>[] = [];
  let settle!: (err?: Error) => void;
  const dispatch = vi.fn(
    (req: Record<string, unknown>) =>
      new Promise<CoreResponse>((resolve, reject) => {
        calls.push(req);
        settle = (err?: Error) =>
          err ? reject(err) : resolve({ type: 'chatSend', data: {} } as unknown as CoreResponse);
      }),
  );
  return {
    calls,
    finish: (err) => settle(err),
    client: { dispatch: dispatch as unknown as CoreClient['dispatch'] },
  };
}

async function settleTicks(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

function mount(s: Stub, speakingAs: string | null = null): ComponentFixture<WhisperDialog> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [WhisperDialog],
    providers: [{ provide: CoreClient, useValue: s.client }],
  });
  const fixture = TestBed.createComponent(WhisperDialog);
  fixture.componentRef.setInput('chatId', 'chat-1');
  fixture.componentRef.setInput('targetName', 'Bram');
  fixture.componentRef.setInput('targetParticipantId', 'p-bram');
  fixture.componentRef.setInput('speakingAsParticipantId', speakingAs);
  fixture.detectChanges();
  return fixture;
}

function textarea(fixture: ComponentFixture<unknown>): HTMLTextAreaElement {
  return fixture.nativeElement.querySelector('textarea') as HTMLTextAreaElement;
}

function type(fixture: ComponentFixture<unknown>, value: string): void {
  const el = textarea(fixture);
  el.value = value;
  el.dispatchEvent(new Event('input'));
  fixture.detectChanges();
}

function whisperButton(fixture: ComponentFixture<unknown>): HTMLButtonElement {
  return [...fixture.nativeElement.querySelectorAll('.qt-dialog-footer button')].find((b) =>
    (b as HTMLElement).textContent?.includes('Whisper'),
  ) as HTMLButtonElement;
}

describe('WhisperDialog (v4 components/chat/WhisperDialog.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('names the target in the title, the blurb and the placeholder', () => {
    const fixture = mount(stub());
    const text = (fixture.nativeElement.textContent ?? '').replace(/\s+/g, ' ');
    expect(text).toContain('Whisper to Bram');
    expect(text).toContain('only be visible to you and Bram');
    expect(textarea(fixture).placeholder).toBe('Write a private message to Bram...');
  });

  it('posts the ORDINARY send with targetParticipantIds — no Post Office verb', async () => {
    const s = stub();
    const fixture = mount(s, 'p-cleo');
    type(fixture, '  Meet me on the ridge.  ');
    whisperButton(fixture).click();
    await settleTicks(fixture);
    expect(s.calls).toEqual([
      {
        type: 'chatSend',
        chatId: 'chat-1',
        content: 'Meet me on the ridge.',
        targetParticipantIds: ['p-bram'],
        speakingAsParticipantId: 'p-cleo',
      },
    ]);
  });

  it('omits speakingAsParticipantId when the operator has not chosen one', async () => {
    const s = stub();
    const fixture = mount(s, null);
    type(fixture, 'psst');
    whisperButton(fixture).click();
    await settleTicks(fixture);
    expect(s.calls[0]!['speakingAsParticipantId']).toBeUndefined();
  });

  it('closes IMMEDIATELY and only emits sent once the turn completes (v4’s sequencing)', async () => {
    const s = stub();
    const fixture = mount(s);
    const seen: string[] = [];
    fixture.componentInstance.close.subscribe(() => seen.push('close'));
    fixture.componentInstance.sent.subscribe(() => seen.push('sent'));

    type(fixture, 'psst');
    whisperButton(fixture).click();
    // The dispatch is in flight and the dialog has ALREADY closed.
    expect(seen).toEqual(['close']);
    expect(s.calls).toHaveLength(1);

    s.finish();
    await settleTicks(fixture);
    expect(seen).toEqual(['close', 'sent']);
  });

  it('swallows a failed whisper to the console (v4 logs, never surfaces)', async () => {
    const s = stub();
    const fixture = mount(s);
    const sent = vi.fn();
    fixture.componentInstance.sent.subscribe(sent);
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});

    type(fixture, 'psst');
    whisperButton(fixture).click();
    s.finish(new Error('turn failed'));
    await settleTicks(fixture);

    expect(sent).not.toHaveBeenCalled();
    expect(spy).toHaveBeenCalledWith('Failed to send whisper:', expect.any(Error));
    spy.mockRestore();
  });

  it('will not send an empty or whitespace-only whisper', async () => {
    const s = stub();
    const fixture = mount(s);
    expect(whisperButton(fixture).disabled).toBe(true);
    type(fixture, '   ');
    expect(whisperButton(fixture).disabled).toBe(true);
    expect(s.calls).toHaveLength(0);
  });

  it('Enter sends, Shift+Enter does not, Escape closes (v4 handleKeyDown)', async () => {
    const s = stub();
    const fixture = mount(s);
    const closed = vi.fn();
    fixture.componentInstance.close.subscribe(closed);
    type(fixture, 'psst');

    textarea(fixture).dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Enter', shiftKey: true, bubbles: true }),
    );
    await settleTicks(fixture);
    expect(s.calls).toHaveLength(0);

    textarea(fixture).dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await settleTicks(fixture);
    expect(s.calls).toHaveLength(1);

    textarea(fixture).dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    expect(closed).toHaveBeenCalled();
  });

  it('a backdrop click closes it', () => {
    const fixture = mount(stub());
    const closed = vi.fn();
    fixture.componentInstance.close.subscribe(closed);
    (fixture.nativeElement.querySelector('.qt-bg-overlay-caption') as HTMLElement).click();
    expect(closed).toHaveBeenCalled();
  });
});
