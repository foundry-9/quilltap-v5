import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

/**
 * `MessageList.revealMessage` (P4.D384) — v4's `?msg=` / in-conversation-search
 * settle loop (`SalonView.tsx`, `4f5047241`, pinned at `624d0d25c`) over the
 * virtualizer. The frame clock and the layout are stubbed: jsdom has no layout,
 * so the container's `clientHeight` / `scrollTop` / rect and each row's rect are
 * planted per case, and `requestAnimationFrame` is a hand-driven queue.
 *
 * The virtualizer forces an empty window when the scroll container reports
 * offsetHeight 0 (JSDOM's answer); the size stub is `message-list.spec.ts`'s.
 */
beforeAll(() => {
  const proto = globalThis.HTMLElement?.prototype;
  if (proto && !('__qtSizeStubbed' in proto)) {
    Object.defineProperty(proto, '__qtSizeStubbed', { value: true });
    Object.defineProperty(proto, 'offsetHeight', { configurable: true, get: () => 800 });
    Object.defineProperty(proto, 'offsetWidth', { configurable: true, get: () => 800 });
  }
});

import { CoreClient } from '../core/core-client';
import type { ChatDetail, MessageDto } from '../core/core-contract';
import { MessageList } from './message-list';

function message(over: Partial<MessageDto>): MessageDto {
  return {
    id: 'm',
    role: 'ASSISTANT',
    content: 'hello',
    tokenCount: null,
    promptTokens: null,
    completionTokens: null,
    createdAt: '2026-01-01T00:00:01.000Z',
    swipeGroupId: null,
    swipeIndex: null,
    participantId: null,
    attachments: [],
    provider: null,
    modelName: null,
    routeTrail: null,
    targetParticipantIds: null,
    isSilentMessage: null,
    systemSender: null,
    systemKind: null,
    hostEvent: null,
    customAnnouncer: null,
    carinaMeta: null,
    pendingExternalPrompt: null,
    pendingExternalPromptFull: null,
    pendingExternalAttachments: null,
    reasoningContent: null,
    reasoningSegments: null,
    ...over,
  };
}

function chatDetail(): ChatDetail {
  return {
    id: 'chat-1',
    title: 'Reveal',
    contextSummary: null,
    roleplayTemplateId: null,
    chatType: 'salon',
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    isPaused: false,
    isManuallyRenamed: false,
    participants: [],
    user: { id: 'u', name: 'Bertie', image: null },
    messages: [],
    projectId: null,
    projectName: null,
    turnSkippingEnabled: null,
    agentModeEnabled: false,
    resolvedAgentModeEnabled: false,
    agentModeSource: 'global',
    isDangerousChat: false,
    dangerCategories: [],
    conciergeState: 'moderated',
    conciergeSetBy: null,
    conciergeReason: null,
    conciergeRefusalCount: 0,
    offSceneCharacters: [],
    lastTurnParticipantId: null,
  };
}

const MESSAGES: MessageDto[] = [
  message({ id: 'm1', content: 'first' }),
  message({ id: 'm2', content: 'second' }),
  // Two collapsed Staff announcements → ONE announcement-group render item.
  message({ id: 'h1', role: 'SYSTEM', content: 'The Host notes a thing.', systemSender: 'host' }),
  message({ id: 'h2', role: 'SYSTEM', content: 'The Host notes another.', systemSender: 'host' }),
  message({ id: 'm3', content: 'third' }),
];

/** A hand-driven animation-frame queue. */
let frames: Map<number, FrameRequestCallback>;
let nextFrame: number;
function runFrame(): boolean {
  const first = frames.entries().next();
  if (first.done) return false;
  const [id, cb] = first.value;
  frames.delete(id);
  cb(0);
  return true;
}
function runFrames(n: number): number {
  let ran = 0;
  while (ran < n && runFrame()) ran++;
  return ran;
}

interface Layout {
  clientHeight: number;
  scrollTop: number;
  /** The most `scrollTop` can reach (the container can scroll no further). */
  maxScrollTop: number;
  /** Each row's top at scrollTop 0, relative to the container's top. */
  rowTop: number;
}

function render(): {
  fixture: ComponentFixture<MessageList>;
  list: MessageList;
  container: HTMLElement;
  scrollToIndex: ReturnType<typeof vi.fn>;
  layout: Layout;
} {
  TestBed.configureTestingModule({
    imports: [MessageList],
    providers: [
      {
        provide: CoreClient,
        useValue: { dispatch: vi.fn(), events$: { subscribe: () => ({ unsubscribe() {} }) } },
      },
    ],
  });
  const fixture = TestBed.createComponent(MessageList);
  fixture.componentRef.setInput('messages', MESSAGES);
  fixture.componentRef.setInput('chat', chatDetail());
  fixture.detectChanges();
  const list = fixture.componentInstance;
  const container = fixture.nativeElement.querySelector('.qt-chat-messages') as HTMLElement;

  const layout: Layout = { clientHeight: 600, scrollTop: 0, maxScrollTop: 10_000, rowTop: 500 };
  Object.defineProperty(container, 'clientHeight', { configurable: true, get: () => layout.clientHeight });
  Object.defineProperty(container, 'scrollTop', {
    configurable: true,
    get: () => layout.scrollTop,
    set: (v: number) => {
      layout.scrollTop = Math.max(0, Math.min(layout.maxScrollTop, v));
    },
  });
  container.getBoundingClientRect = () => ({ top: 0 }) as DOMRect;
  // Every candidate row moves up as the container scrolls.
  for (const el of container.querySelectorAll<HTMLElement>('[id^="message-"], .qt-chat-announcement-group')) {
    el.getBoundingClientRect = () => ({ top: layout.rowTop - layout.scrollTop }) as DOMRect;
  }

  const virtualizer = (list as unknown as { virtualizer: { scrollToIndex: (...a: unknown[]) => void } })
    .virtualizer;
  const scrollToIndex = vi.fn();
  vi.spyOn(virtualizer, 'scrollToIndex').mockImplementation(scrollToIndex);
  return { fixture, list, container, scrollToIndex, layout };
}

function rowOf(container: HTMLElement, id: string): HTMLElement {
  const el = container.querySelector<HTMLElement>(`[id="message-${id}"]`);
  if (!el) throw new Error(`row ${id} not mounted — the fixture premise failed`);
  return el;
}

describe('MessageList.revealMessage — v4’s settle loop (P4.D384)', () => {
  beforeEach(() => {
    frames = new Map();
    nextFrame = 1;
    vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
      const id = nextFrame++;
      frames.set(id, cb);
      return id;
    });
    vi.stubGlobal('cancelAnimationFrame', (id: number) => {
      frames.delete(id);
    });
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it('asks the virtualizer with align start, nudges the row 12px below the top, then flashes it for 3000ms', () => {
    const { list, container, scrollToIndex, layout } = render();
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    const row = rowOf(container, 'm2');

    list.revealMessage('m2');
    expect(scrollToIndex).toHaveBeenCalledWith(1, { align: 'start' });

    runFrame(); // offset 500 - 0 - 12 = 488 → nudge
    expect(layout.scrollTop).toBe(488);
    expect(row.classList.contains('qt-memory-source-highlight')).toBe(false);
    runFrame(); // offset 0 → flash
    expect(row.classList.contains('qt-memory-source-highlight')).toBe(true);
    expect(frames.size).toBe(0);

    vi.advanceTimersByTime(2999);
    expect(row.classList.contains('qt-memory-source-highlight')).toBe(true);
    vi.advanceTimersByTime(1);
    expect(row.classList.contains('qt-memory-source-highlight')).toBe(false);
  });

  it('flashes at once when the row is already within 2px of its place', () => {
    const { list, container, layout } = render();
    layout.rowTop = 14; // offset 2
    list.revealMessage('m1');
    runFrame();
    expect(layout.scrollTop).toBe(0);
    expect(rowOf(container, 'm1').classList.contains('qt-memory-source-highlight')).toBe(true);
  });

  it('stops nudging once the container can scroll no further, and flashes', () => {
    const { list, container, layout } = render();
    layout.maxScrollTop = 100;
    list.revealMessage('m3');
    runFrame(); // scrollTop 0 → clamped 100
    expect(layout.scrollTop).toBe(100);
    runFrame(); // offset still 388, scrollTop moved → nudge again (stays 100)
    expect(rowOf(container, 'm3').classList.contains('qt-memory-source-highlight')).toBe(false);
    runFrame(); // scrollTop did not move → flash
    expect(rowOf(container, 'm3').classList.contains('qt-memory-source-highlight')).toBe(true);
    expect(frames.size).toBe(0);
  });

  it('re-asks the virtualizer each frame while the row is unmounted, giving up after 40 attempts', () => {
    const { list, container, scrollToIndex } = render();
    rowOf(container, 'm2').removeAttribute('id'); // "not mounted"
    list.revealMessage('m2');
    const ran = runFrames(1000);
    expect(ran).toBe(41); // 40 attempts + the one that sees attempts > 40
    expect(scrollToIndex).toHaveBeenCalledTimes(1 + 40);
    expect(frames.size).toBe(0);
  });

  it('a hidden container waits without spending an attempt, then reveals once shown', () => {
    const { list, container, layout } = render();
    layout.clientHeight = 0;
    list.revealMessage('m2');
    expect(runFrames(100)).toBe(100); // more than 40 frames, still waiting
    expect(frames.size).toBe(1);
    layout.clientHeight = 600;
    runFrames(2);
    expect(rowOf(container, 'm2').classList.contains('qt-memory-source-highlight')).toBe(true);
  });

  it('a container hidden for more than 600 frames gives up', () => {
    const { list, container, layout } = render();
    layout.clientHeight = 0;
    list.revealMessage('m2');
    expect(runFrames(10_000)).toBe(601);
    expect(frames.size).toBe(0);
    expect(rowOf(container, 'm2').classList.contains('qt-memory-source-highlight')).toBe(false);
  });

  it('a message in a collapsed announcement group scrolls to and flashes the GROUP row', () => {
    const { list, container, scrollToIndex } = render();
    const chip = rowOf(container, 'h2');
    expect(chip.hasAttribute('data-message-id')).toBe(true);
    const group = chip.closest<HTMLElement>('.qt-chat-announcement-group');
    expect(group).not.toBeNull();

    list.revealMessage('h2');
    expect(scrollToIndex).toHaveBeenCalledWith(2, { align: 'start' });
    runFrames(5);
    expect(group!.classList.contains('qt-memory-source-highlight')).toBe(true);
    expect(chip.classList.contains('qt-memory-source-highlight')).toBe(false);
  });

  it('a target not in the transcript is a silent no-op', () => {
    const { list, scrollToIndex } = render();
    list.revealMessage('not-here');
    expect(scrollToIndex).not.toHaveBeenCalled();
    expect(frames.size).toBe(0);
  });

  it('a new reveal cancels the one in flight', () => {
    const { list, container, layout } = render();
    layout.clientHeight = 0;
    list.revealMessage('m1');
    runFrames(3);
    list.revealMessage('m2');
    expect(frames.size).toBe(1);
    layout.clientHeight = 600;
    runFrames(5);
    expect(rowOf(container, 'm1').classList.contains('qt-memory-source-highlight')).toBe(false);
    expect(rowOf(container, 'm2').classList.contains('qt-memory-source-highlight')).toBe(true);
  });

  it('destroy cancels the loop and the pending flash removal', () => {
    const { fixture, list, container, layout } = render();
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    layout.rowTop = 12;
    list.revealMessage('m1');
    runFrame();
    const row = rowOf(container, 'm1');
    expect(row.classList.contains('qt-memory-source-highlight')).toBe(true);

    layout.clientHeight = 0;
    list.revealMessage('m2');
    expect(frames.size).toBe(1);
    const [revealFrame] = [...frames.keys()];
    fixture.destroy();
    // Only the reveal's own frame is asserted: the virtualizer adapter may queue
    // frames of its own on teardown.
    expect(frames.has(revealFrame)).toBe(false);
    // The flash's removal timer was cleared with the list: the class stays put.
    vi.advanceTimersByTime(5000);
    expect(row.classList.contains('qt-memory-source-highlight')).toBe(true);
  });

  it('exposes the controller’s isInitialScrollDone for the Salon’s ?msg= gate', () => {
    const { list } = render();
    const controller = (list as unknown as { autoScroll: { isInitialScrollDone: unknown } }).autoScroll;
    expect(list.isInitialScrollDone).toBe(controller.isInitialScrollDone);
    expect(typeof list.isInitialScrollDone()).toBe('boolean');
  });
});
