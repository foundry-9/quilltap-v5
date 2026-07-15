import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { LLMInspectorPanel } from './llm-inspector-panel';
import type { LlmLogDto } from './llm-logs.api';

function log(overrides: Partial<LlmLogDto> = {}): LlmLogDto {
  return {
    id: 'log-1',
    userId: 'user-1',
    type: 'CHAT_MESSAGE',
    provider: 'openai',
    modelName: 'gpt-4o',
    request: { messageCount: 1, messages: [], toolCount: 0 },
    response: { content: 'hi', contentLength: 2 },
    createdAt: '2026-02-01T00:00:00.000Z',
    updatedAt: '2026-02-01T00:00:00.000Z',
    ...overrides,
  };
}

interface Inputs {
  isOpen?: boolean;
  logs?: LlmLogDto[];
  loading?: boolean;
  scrollToMessageId?: string | null;
  loggingEnabled?: boolean;
}

function render(inputs: Inputs = {}): ComponentFixture<LLMInspectorPanel> {
  // Reset first so a test may render more than once (TestBed refuses to
  // reconfigure an instantiated module).
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [LLMInspectorPanel] });
  const fixture = TestBed.createComponent(LLMInspectorPanel);
  fixture.componentRef.setInput('isOpen', inputs.isOpen ?? true);
  fixture.componentRef.setInput('logs', inputs.logs ?? []);
  fixture.componentRef.setInput('loading', inputs.loading ?? false);
  fixture.componentRef.setInput('scrollToMessageId', inputs.scrollToMessageId ?? null);
  fixture.componentRef.setInput('loggingEnabled', inputs.loggingEnabled ?? true);
  fixture.detectChanges();
  return fixture;
}

function text(fixture: ComponentFixture<LLMInspectorPanel>): string {
  return fixture.nativeElement.textContent.replace(/\s+/g, ' ').trim();
}

function entryIds(fixture: ComponentFixture<LLMInspectorPanel>): string[] {
  return (Array.from(fixture.nativeElement.querySelectorAll('[data-log-id]')) as HTMLElement[]).map(
    (el) => el.getAttribute('data-log-id')!,
  );
}

function select(fixture: ComponentFixture<LLMInspectorPanel>): HTMLSelectElement {
  return fixture.nativeElement.querySelector('select[aria-label="Filter log entries"]');
}

function setFilter(fixture: ComponentFixture<LLMInspectorPanel>, value: string): void {
  const el = select(fixture);
  el.value = value;
  el.dispatchEvent(new Event('change'));
  fixture.detectChanges();
}

describe('LLMInspectorPanel — the panel shell (v4 LLMInspectorPanel.tsx:119-126)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('titles the slide-over "LLM Inspector" with v4’s distinct aria-label', () => {
    const fixture = render();
    const panel = fixture.nativeElement.querySelector('.qt-slide-over-panel');
    expect(fixture.nativeElement.querySelector('.qt-slide-over-header h2').textContent.trim()).toBe(
      'LLM Inspector',
    );
    expect(panel.getAttribute('aria-label')).toBe('LLM Inspector Panel');
  });

  it('forwards its open state to the slide-over', () => {
    const fixture = render({ isOpen: false });
    expect(
      fixture.nativeElement.querySelector('.qt-slide-over-panel').getAttribute('data-open'),
    ).toBe('false');
  });

  it('re-emits the slide-over’s close', () => {
    const fixture = render();
    const spy = vi.fn();
    fixture.componentInstance.close.subscribe(spy);
    (
      fixture.nativeElement.querySelector('button[aria-label="Close panel"]') as HTMLButtonElement
    ).click();
    expect(spy).toHaveBeenCalled();
  });
});

describe('LLMInspectorPanel — chronological order (v4 :54-55)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('reverses the DESC rows the server returns into oldest-first', () => {
    // The list arrives newest-first; the panel reads top-to-bottom as a
    // transcript, so it reverses.
    const fixture = render({
      logs: [log({ id: 'newest' }), log({ id: 'middle' }), log({ id: 'oldest' })],
    });
    expect(entryIds(fixture)).toEqual(['oldest', 'middle', 'newest']);
  });
});

describe('LLMInspectorPanel — the filter (v4 :11-29, :57-62, :94-104)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('offers v4’s seven options, in v4’s order, with v4’s copy', () => {
    const fixture = render();
    const options = (Array.from(select(fixture).options) as HTMLOptionElement[]).map((o) => [
      o.value,
      o.textContent!.trim(),
    ]);
    expect(options).toEqual([
      ['all', 'All'],
      ['chat', 'Chat Messages'],
      ['memory', 'Memory'],
      ['system', 'System Ops'],
      ['image', 'Image'],
      ['safety', 'Safety'],
      ['other', 'Other'],
    ]);
  });

  it('defaults to all', () => {
    expect(select(render()).value).toBe('all');
  });

  it.each([
    ['chat', ['CHAT_MESSAGE', 'TOOL_CONTINUATION']],
    ['memory', ['MEMORY_EXTRACTION']],
    ['system', ['TITLE_GENERATION', 'SUMMARIZATION', 'CONTEXT_COMPRESSION']],
    ['image', ['IMAGE_PROMPT_CRAFTING', 'IMAGE_DESCRIPTION', 'APPEARANCE_RESOLUTION']],
    ['safety', ['DANGER_CLASSIFICATION']],
    ['other', ['CHARACTER_WIZARD', 'AI_IMPORT']],
  ])('the %s group admits exactly its v4 types', (filter, types) => {
    const every = [
      'CHAT_MESSAGE',
      'TOOL_CONTINUATION',
      'MEMORY_EXTRACTION',
      'TITLE_GENERATION',
      'SUMMARIZATION',
      'CONTEXT_COMPRESSION',
      'IMAGE_PROMPT_CRAFTING',
      'IMAGE_DESCRIPTION',
      'APPEARANCE_RESOLUTION',
      'DANGER_CLASSIFICATION',
      'CHARACTER_WIZARD',
      'AI_IMPORT',
    ];
    const fixture = render({ logs: every.map((t) => log({ id: t, type: t })).reverse() });
    setFilter(fixture, filter);
    expect(entryIds(fixture).sort()).toEqual([...types].sort());
  });

  it('all does NOT filter — a type in no group is reachable only there', () => {
    // FILTER_GROUPS.all is null ("skip filtering"), not a list of every type;
    // SCENE_STATE_TRACKING belongs to no group, so `other` must not show it.
    const fixture = render({ logs: [log({ id: 'scene', type: 'SCENE_STATE_TRACKING' })] });
    expect(entryIds(fixture)).toEqual(['scene']);
    setFilter(fixture, 'other');
    expect(entryIds(fixture)).toEqual([]);
  });
});

describe('LLMInspectorPanel — the entry count (v4 :90-92)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('pluralizes: "1 entry" vs "N entries", and counts ZERO as entries', () => {
    expect(text(render({ logs: [log()] }))).toContain('1 entry');
    expect(text(render({ logs: [log({ id: 'a' }), log({ id: 'b' })] }))).toContain('2 entries');
    expect(text(render({ logs: [] }))).toContain('0 entries');
  });

  it('counts the FILTERED entries, not the whole list', () => {
    const fixture = render({
      logs: [log({ id: 'a', type: 'CHAT_MESSAGE' }), log({ id: 'b', type: 'MEMORY_EXTRACTION' })],
    });
    expect(text(fixture)).toContain('2 entries');
    setFilter(fixture, 'memory');
    expect(text(fixture)).toContain('1 entry');
  });
});

describe('LLMInspectorPanel — the refresh button (v4 :106-115)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('carries v4’s title and emits refresh', () => {
    const fixture = render();
    const button = fixture.nativeElement.querySelector('button[aria-label="Refresh logs"]');
    expect(button.getAttribute('title')).toBe('Refresh logs');
    expect(button.querySelector('qt-icon').getAttribute('name')).toBe('refresh');

    const spy = vi.fn();
    fixture.componentInstance.refresh.subscribe(spy);
    button.click();
    expect(spy).toHaveBeenCalled();
  });
});

describe('LLMInspectorPanel — the empty states, in v4’s priority order (v4 :131-147)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('shows the spinner while loading', () => {
    const fixture = render({ loading: true });
    expect(fixture.nativeElement.querySelector('.animate-spin')).not.toBeNull();
  });

  it('loading OUTRANKS logging-disabled — a disabled chat still spins while the query settles', () => {
    const fixture = render({ loading: true, loggingEnabled: false });
    expect(fixture.nativeElement.querySelector('.animate-spin')).not.toBeNull();
    expect(text(fixture)).not.toContain('LLM logging is disabled.');
  });

  it('shows the logging-disabled state with v4’s copy and ban icon', () => {
    const fixture = render({ loggingEnabled: false });
    expect(text(fixture)).toContain('LLM logging is disabled.');
    expect(text(fixture)).toContain('Enable it in Chat Settings to record API interactions.');
    expect(fixture.nativeElement.querySelector('qt-icon[name="ban"]')).not.toBeNull();
  });

  it('logging-disabled OUTRANKS no-entries — even when logs somehow exist', () => {
    const fixture = render({ loggingEnabled: false, logs: [log()] });
    expect(text(fixture)).toContain('LLM logging is disabled.');
    expect(entryIds(fixture)).toEqual([]);
  });

  it('shows the no-entries state with v4’s copy and file icon', () => {
    const fixture = render({ logs: [] });
    expect(text(fixture)).toContain('No log entries yet.');
    expect(text(fixture)).toContain('Send a message to start recording LLM interactions.');
    expect(fixture.nativeElement.querySelector('qt-icon[name="file"]')).not.toBeNull();
  });

  it('shows the no-entries state when the FILTER empties the list', () => {
    const fixture = render({ logs: [log({ type: 'CHAT_MESSAGE' })] });
    setFilter(fixture, 'safety');
    expect(text(fixture)).toContain('No log entries yet.');
  });

  it('renders entries once there are any', () => {
    const fixture = render({ logs: [log({ id: 'x' })] });
    expect(entryIds(fixture)).toEqual(['x']);
    expect(text(fixture)).not.toContain('No log entries yet.');
  });
});

describe('LLMInspectorPanel — highlight + scroll-to-message (v4 :64-85, :153)', () => {
  afterEach(() => {
    vi.useRealTimers();
    TestBed.resetTestingModule();
  });

  it('highlights the entry whose messageId matches the scroll target', () => {
    const fixture = render({
      logs: [log({ id: 'a', messageId: 'm1' }), log({ id: 'b', messageId: 'm2' })],
      scrollToMessageId: 'm2',
    });
    const entries = fixture.nativeElement.querySelectorAll('.qt-inspector-entry');
    const byId = Object.fromEntries(
      (Array.from(entries) as HTMLElement[]).map((el) => [el.getAttribute('data-log-id'), el]),
    );
    expect(byId['b'].classList.contains('qt-inspector-entry-highlight')).toBe(true);
    expect(byId['a'].classList.contains('qt-inspector-entry-highlight')).toBe(false);
  });

  it('highlights nothing when there is no scroll target (v4 :153 ternary)', () => {
    const fixture = render({ logs: [log({ id: 'a', messageId: 'm1' })], scrollToMessageId: null });
    expect(fixture.nativeElement.querySelector('.qt-inspector-entry-highlight')).toBeNull();
  });

  it('scrolls the target entry into view after v4’s 300 ms animation delay', () => {
    vi.useFakeTimers();
    const fixture = render({
      logs: [log({ id: 'a', messageId: 'm1' })],
      scrollToMessageId: 'm1',
    });
    const target = fixture.nativeElement.querySelector('[data-log-id="a"]');
    const spy = vi.fn();
    target.scrollIntoView = spy;

    // Nothing before the delay elapses — the panel is still sliding in.
    vi.advanceTimersByTime(299);
    expect(spy).not.toHaveBeenCalled();

    vi.advanceTimersByTime(1);
    expect(spy).toHaveBeenCalledWith({ behavior: 'smooth', block: 'center' });
  });

  it('does not scroll while the panel is closed', () => {
    vi.useFakeTimers();
    const fixture = render({
      isOpen: false,
      logs: [log({ id: 'a', messageId: 'm1' })],
      scrollToMessageId: 'm1',
    });
    const target = fixture.nativeElement.querySelector('[data-log-id="a"]');
    const spy = vi.fn();
    target.scrollIntoView = spy;
    vi.advanceTimersByTime(500);
    expect(spy).not.toHaveBeenCalled();
  });

  it('does not scroll when the active FILTER excludes the target (v4 :70-71)', () => {
    // v4 confirms the target exists in the FILTERED results and gives up
    // otherwise, rather than scrolling to some other entry.
    vi.useFakeTimers();
    const fixture = render({
      logs: [log({ id: 'a', messageId: 'm1', type: 'CHAT_MESSAGE' })],
      scrollToMessageId: 'm1',
    });
    setFilter(fixture, 'safety');
    vi.advanceTimersByTime(500);
    // The entry is not even rendered under this filter.
    expect(entryIds(fixture)).toEqual([]);
  });
});
