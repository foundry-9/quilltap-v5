import { ChangeDetectionStrategy, Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../core/core-client';
import { coreStreamStub } from '../core/core-client.testing';
import type { ChatGalleryResult } from '../core/core-contract';
import { chatKeys } from './chat-keys';
import { injectChatGallery, type ChatGallery } from './chat-gallery.api';

type Req = { type: string; [k: string]: unknown };

function stub(answer: ChatGalleryResult | 'error', seen: Req[] = []): CoreClient {
  return {
    ...coreStreamStub(),
    dispatchData: async (req: Req) => {
      seen.push(req);
      if (answer === 'error') {
        throw new Error("unknown variant `chatGallery`");
      }
      return answer as unknown as Record<string, unknown>;
    },
  } as unknown as CoreClient;
}

function result(over: Partial<ChatGalleryResult> = {}): ChatGalleryResult {
  return {
    entries: [],
    counts: {
      'story-background': 0,
      avatar: 0,
      portrait: 0,
      generated: 0,
      attachment: 0,
      kept: 0,
      inline: 0,
    },
    total: 0,
    ...over,
  };
}

@Component({
  selector: 'qt-chat-gallery-api-host',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: '',
})
class Host {
  readonly chatId = signal<string | null>('chat-1');
  readonly enabled = signal(true);
  readonly api: ChatGallery = injectChatGallery(() => this.chatId(), {
    enabled: () => this.enabled(),
  });
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function host(core: CoreClient): Promise<ComponentFixture<Host>> {
  TestBed.configureTestingModule({
    imports: [Host],
    providers: [
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      { provide: CoreClient, useValue: core },
    ],
  });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

/**
 * `injectChatGallery` — v4 `useChatGallery.ts` (P4.D176, §C.3). ONE read
 * backs both the grid and the Organize drawer's count (bug 129's fix).
 */
describe('injectChatGallery', () => {
  it('reads under `chatKeys.gallery` and exposes entries/counts/total', async () => {
    const core = stub(result({ total: 3, entries: [{ id: 'f-1' } as never] }));
    const fixture = await host(core);
    expect(fixture.componentInstance.api.total()).toBe(3);
    expect(fixture.componentInstance.api.entries()).toHaveLength(1);
    expect(chatKeys.gallery('chat-1')).toEqual(['chat', 'chat-1', 'gallery']);
  });

  it('resolves entry urls through apiUrl (identity in the browser test env)', async () => {
    const core = stub(
      result({ entries: [{ id: 'f-1', url: '/api/v1/files/f-1' } as never] }),
    );
    const fixture = await host(core);
    expect(fixture.componentInstance.api.entries()[0]!.url).toBe('/api/v1/files/f-1');
  });

  it('seeds EMPTY_COUNTS (all seven sources at 0) before the query answers', async () => {
    const core = stub(result());
    TestBed.configureTestingModule({
      imports: [Host],
      providers: [
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      { provide: CoreClient, useValue: core },
    ],
    });
    const fixture = TestBed.createComponent(Host);
    fixture.detectChanges();
    // Before settling — the query has not answered yet.
    expect(fixture.componentInstance.api.counts()).toEqual({
      'story-background': 0,
      avatar: 0,
      portrait: 0,
      generated: 0,
      attachment: 0,
      kept: 0,
      inline: 0,
    });
    expect(fixture.componentInstance.api.hasData()).toBe(false);
  });

  it("hasData flips true only once the query has ANSWERED — 0 total is still data", async () => {
    const core = stub(result({ total: 0 }));
    const fixture = await host(core);
    expect(fixture.componentInstance.api.total()).toBe(0);
    expect(fixture.componentInstance.api.hasData()).toBe(true);
  });

  it('a verb the server does not yet implement leaves hasData false (the pre-P4.D174 shape)', async () => {
    const core = stub('error');
    const fixture = await host(core);
    expect(fixture.componentInstance.api.isError()).toBe(true);
    expect(fixture.componentInstance.api.hasData()).toBe(false);
    expect(fixture.componentInstance.api.total()).toBe(0);
  });

  it('`enabled: false` defers the read entirely, and never reports loading', async () => {
    const seen: Req[] = [];
    const core = stub(result(), seen);
    TestBed.configureTestingModule({
      imports: [Host],
      providers: [
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      { provide: CoreClient, useValue: core },
    ],
    });
    const fixture = TestBed.createComponent(Host);
    fixture.componentInstance.enabled.set(false);
    fixture.detectChanges();
    await settle(fixture);
    expect(seen.filter((r) => r.type === 'chatGallery')).toEqual([]);
    expect(fixture.componentInstance.api.isLoading()).toBe(false);
  });

  it('a null chat id defers the read whatever `enabled` says', async () => {
    const seen: Req[] = [];
    const core = stub(result(), seen);
    TestBed.configureTestingModule({
      imports: [Host],
      providers: [
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      { provide: CoreClient, useValue: core },
    ],
    });
    const fixture = TestBed.createComponent(Host);
    fixture.componentInstance.chatId.set(null);
    fixture.detectChanges();
    await settle(fixture);
    expect(seen.filter((r) => r.type === 'chatGallery')).toEqual([]);
  });

  it('invalidate() re-triggers the read', async () => {
    const seen: Req[] = [];
    const core = stub(result(), seen);
    const fixture = await host(core);
    const before = seen.filter((r) => r.type === 'chatGallery').length;
    fixture.componentInstance.api.invalidate();
    await settle(fixture);
    expect(seen.filter((r) => r.type === 'chatGallery').length).toBeGreaterThan(before);
  });
});
