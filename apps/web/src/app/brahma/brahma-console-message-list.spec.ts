/**
 * BrahmaConsoleMessageList — asserted against v4
 * `BrahmaConsoleMessageList.tsx`: the empty-state prompt, user/assistant
 * bubbles (assistant hidden when its prose is empty), run_sql TOOL messages
 * surfaced as cards (other tools silent), and the live streaming block
 * (reasoning + run_sql cards + prose + the working indicators).
 */

import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, beforeAll, describe, expect, it } from 'vitest';

beforeAll(() => {
  const proto = globalThis.HTMLElement?.prototype as unknown as {
    scrollIntoView?: () => void;
  };
  if (proto && !proto.scrollIntoView) {
    proto.scrollIntoView = () => undefined;
  }
});

import type { PendingToolCall } from '../core/chat-stream.reducer';
import { BrahmaConsoleMessageList } from './brahma-console-message-list';
import type { BrahmaConsoleMessage } from './brahma-wire';

function msg(over: Partial<BrahmaConsoleMessage> = {}): BrahmaConsoleMessage {
  return {
    id: `m-${Math.random().toString(36).slice(2)}`,
    role: 'USER',
    content: 'hi',
    createdAt: '2026-01-01T00:00:00.000Z',
    ...over,
  };
}

/** A persisted run_sql TOOL message (the `saveToolMessages` envelope). */
function toolMsg(over: Partial<BrahmaConsoleMessage> = {}): BrahmaConsoleMessage {
  return msg({
    role: 'TOOL',
    content: JSON.stringify({
      toolName: 'run_sql',
      success: true,
      result: JSON.stringify({
        columns: ['n'],
        rows: [{ n: 3 }],
        rowCount: 1,
        truncated: false,
      }),
      arguments: { sql: 'SELECT count(*) AS n FROM characters', database: 'main' },
    }),
    ...over,
  });
}

async function render(inputs: {
  messages: BrahmaConsoleMessage[];
  streamingContent?: string;
  streamingReasoning?: string;
  streamingToolCalls?: PendingToolCall[];
  isStreaming?: boolean;
  isExecutingTools?: boolean;
}): Promise<ComponentFixture<BrahmaConsoleMessageList>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [BrahmaConsoleMessageList] });
  const fixture = TestBed.createComponent(BrahmaConsoleMessageList);
  fixture.componentRef.setInput('messages', inputs.messages);
  if (inputs.streamingContent !== undefined)
    fixture.componentRef.setInput('streamingContent', inputs.streamingContent);
  if (inputs.streamingReasoning !== undefined)
    fixture.componentRef.setInput('streamingReasoning', inputs.streamingReasoning);
  if (inputs.streamingToolCalls !== undefined)
    fixture.componentRef.setInput('streamingToolCalls', inputs.streamingToolCalls);
  if (inputs.isStreaming !== undefined)
    fixture.componentRef.setInput('isStreaming', inputs.isStreaming);
  if (inputs.isExecutingTools !== undefined)
    fixture.componentRef.setInput('isExecutingTools', inputs.isExecutingTools);
  fixture.detectChanges();
  await Promise.resolve();
  fixture.detectChanges();
  return fixture;
}

function text(fixture: ComponentFixture<unknown>): string {
  return (fixture.nativeElement as HTMLElement).textContent ?? '';
}

describe('BrahmaConsoleMessageList (v4 BrahmaConsoleMessageList.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('shows the empty prompt with no messages and no stream', async () => {
    const fixture = await render({ messages: [] });
    expect(text(fixture)).toContain('A direct line to the engine of your choosing.');
  });

  it('renders user and non-empty assistant bubbles', async () => {
    const fixture = await render({
      messages: [
        msg({ id: 'u1', role: 'USER', content: 'What is 2+2?' }),
        msg({ id: 'a1', role: 'ASSISTANT', content: 'Four.' }),
      ],
    });
    expect(text(fixture)).toContain('What is 2+2?');
    expect(text(fixture)).toContain('Four.');
  });

  it('hides an empty intermediate assistant turn', async () => {
    const fixture = await render({
      messages: [msg({ id: 'a1', role: 'ASSISTANT', content: '   ' })],
    });
    // No bubble → falls back to the empty prompt.
    expect(text(fixture)).toContain('A direct line to the engine');
  });

  it('surfaces a run_sql TOOL message as a card', async () => {
    const fixture = await render({ messages: [toolMsg({ id: 't1' })] });
    expect(text(fixture)).toContain('Ran SQL');
    expect(text(fixture)).toContain('SELECT count(*) AS n FROM characters');
    expect(text(fixture)).toContain('1 row');
  });

  it('leaves a non-run_sql TOOL message silent', async () => {
    const search = msg({
      id: 't2',
      role: 'TOOL',
      content: JSON.stringify({ toolName: 'search', success: true, result: '[]', arguments: {} }),
    });
    const fixture = await render({ messages: [search] });
    expect(text(fixture)).not.toContain('Ran SQL');
    expect(text(fixture)).toContain('A direct line to the engine');
  });

  it('renders a live run_sql card and streamed prose while streaming', async () => {
    const calls: PendingToolCall[] = [
      {
        id: 'tool-0-0',
        name: 'run_sql',
        status: 'success',
        arguments: { sql: 'SELECT 1', database: 'main' },
        result: { columns: ['x'], rows: [{ x: 1 }], rowCount: 1, truncated: false },
      },
    ];
    const fixture = await render({
      messages: [],
      isStreaming: true,
      streamingContent: 'Here is the answer.',
      streamingToolCalls: calls,
    });
    expect(text(fixture)).toContain('Ran SQL');
    expect(text(fixture)).toContain('Here is the answer.');
  });

  it('shows "Consulting the stacks…" while executing tools with no cards yet', async () => {
    const fixture = await render({
      messages: [],
      isStreaming: true,
      isExecutingTools: true,
    });
    expect(text(fixture)).toContain('Consulting the stacks…');
  });

  it('shows "Thinking…" before anything lands', async () => {
    const fixture = await render({ messages: [], isStreaming: true });
    expect(text(fixture)).toContain('Thinking…');
  });

  // Dogfood #74: the transcript must scroll WITHIN the list, not push the
  // console's sticky header off the workspace tab. React renders the
  // `flex-1 overflow-y-auto` div as the node itself; Angular wraps it in this
  // host element, so the host must carry the flex sizing or the inner scroll
  // container has no bounded parent and the whole tab scrolls.
  it('bounds itself as a flex child so the header stays put (the host carries the sizing)', async () => {
    const fixture = await render({ messages: [msg({ id: 'u1', content: 'hi' })] });
    const host = fixture.nativeElement as HTMLElement;
    for (const cls of ['flex', 'flex-col', 'flex-1', 'min-h-0']) {
      expect(host.classList.contains(cls)).toBe(true);
    }
    // The inner scroll container is the one that scrolls, and it can shrink.
    const scroller = host.querySelector('.overflow-y-auto') as HTMLElement | null;
    expect(scroller).not.toBeNull();
    expect(scroller!.classList.contains('flex-1')).toBe(true);
    expect(scroller!.classList.contains('min-h-0')).toBe(true);
  });
});
