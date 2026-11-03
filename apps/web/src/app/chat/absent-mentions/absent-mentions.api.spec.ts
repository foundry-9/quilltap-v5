/** v4 `hooks/useAbsentMentions.ts:19-43` at `236c04e35` — ONE read per chat, split per message. */
import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { coreStreamStub } from '../../core/core-client.testing';
import type { AbsentMention } from '../../core/core-contract';
import { RealtimeService } from '../../core/realtime.service';
import { injectAbsentMentions, type AbsentMentions } from './absent-mentions.api';

const mention = (messageId: string, text: string): AbsentMention => ({
  messageId,
  characterId: 'abigail',
  text,
  card: { name: 'Abigail', aliases: [], pronouns: null, body: '' },
});

@Component({ template: '' })
class Host {
  readonly chatId = signal<string | null>('chat-1');
  readonly absent: AbsentMentions = injectAbsentMentions(this.chatId);
}

async function mount(
  reply: unknown = { mentions: [mention('m1', 'Abby'), mention('m2', 'Abigail')] },
) {
  const dispatchData = vi.fn(async () => reply);
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    providers: [
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      { provide: CoreClient, useValue: { ...coreStreamStub(), dispatchData } },
    ],
  });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return { fixture, host: fixture.componentInstance, dispatchData };
}

describe('injectAbsentMentions', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('reads once per chat through chatAbsentMentions, then splits per message', async () => {
    const { host, dispatchData } = await mount();
    expect(dispatchData).toHaveBeenCalledTimes(1);
    expect(dispatchData).toHaveBeenCalledWith({ type: 'chatAbsentMentions', chatId: 'chat-1' });
    expect(
      host.absent
        .mentionsFor('m1')()
        .map((m) => m.text),
    ).toEqual(['Abby']);
    expect(
      host.absent
        .mentionsFor('m2')()
        .map((m) => m.text),
    ).toEqual(['Abigail']);
    expect(host.absent.all()).toHaveLength(2);
    // N rows, still one read.
    for (const id of ['m1', 'm2', 'm3', 'm4']) host.absent.mentionsFor(id)();
    expect(dispatchData).toHaveBeenCalledTimes(1);
  });

  it('a message with none gets the one shared empty answer; the signal is memoised per id', async () => {
    const { host } = await mount();
    expect(host.absent.mentionsFor('m9')()).toBe(host.absent.mentionsFor('m8')());
    expect(host.absent.mentionsFor('m1')).toBe(host.absent.mentionsFor('m1'));
  });

  it('an answer without the key reads as none', async () => {
    const { host } = await mount({});
    expect(host.absent.all()).toEqual([]);
  });

  it('no chat id → no read', async () => {
    const dispatchData = vi.fn(async () => ({ mentions: [] }));
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      providers: [
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: { ...coreStreamStub(), dispatchData } },
      ],
    });
    const fixture = TestBed.createComponent(Host);
    fixture.componentInstance.chatId.set(null);
    fixture.detectChanges();
    await new Promise((r) => setTimeout(r, 0));
    expect(dispatchData).not.toHaveBeenCalled();
  });

  it('the fallback poll is off while the socket is up and 60 s while it is down', async () => {
    await mount();
    const realtime = TestBed.inject(RealtimeService);
    realtime.connected.set(true);
    expect(realtime.refetchInterval(60_000)).toBe(false);
    realtime.connected.set(false);
    expect(realtime.refetchInterval(60_000)).toBe(60_000);
  });
});
