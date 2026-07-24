import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { LLMLogViewerModal } from './llm-log-viewer-modal';
import type { LlmLogDto } from './llm-logs.api';

function log(over: Partial<LlmLogDto> = {}): LlmLogDto {
  return {
    id: 'log-1',
    userId: 'u1',
    type: 'CHAT_MESSAGE',
    provider: 'openai',
    modelName: 'gpt-5',
    request: {
      messageCount: 2,
      messages: [{ role: 'system', content: 'You are helpful', contentLength: 15, hasAttachments: false }],
      temperature: 0.7,
      maxTokens: 1000,
      toolCount: 3,
    },
    response: { content: 'Hello there', contentLength: 11 },
    usage: { promptTokens: 100, completionTokens: 50, totalTokens: 150 },
    durationMs: 1234,
    createdAt: '2026-07-24T12:00:00.000Z',
    updatedAt: '2026-07-24T12:00:00.000Z',
    ...over,
  };
}

async function mount(inputs: {
  isOpen: boolean;
  logs: LlmLogDto[];
}): Promise<ComponentFixture<LLMLogViewerModal>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [LLMLogViewerModal] });
  const fixture = TestBed.createComponent(LLMLogViewerModal);
  fixture.componentRef.setInput('isOpen', inputs.isOpen);
  fixture.componentRef.setInput('logs', inputs.logs);
  fixture.detectChanges();
  return fixture;
}

function tabByLabel(fixture: ComponentFixture<LLMLogViewerModal>, label: string): HTMLButtonElement {
  return Array.from(fixture.nativeElement.querySelectorAll('button')).find(
    (b) => (b as HTMLButtonElement).textContent?.trim() === label,
  ) as HTMLButtonElement;
}

describe('LLMLogViewerModal', () => {
  it('renders nothing when closed or when the log list is empty', async () => {
    const closed = await mount({ isOpen: false, logs: [log()] });
    expect(closed.nativeElement.querySelector('[role="dialog"]')).toBeNull();
    const empty = await mount({ isOpen: true, logs: [] });
    expect(empty.nativeElement.querySelector('[role="dialog"]')).toBeNull();
  });

  it('shows the request tab by default with provider/model/config', async () => {
    const fixture = await mount({ isOpen: true, logs: [log()] });
    const text = fixture.nativeElement.textContent;
    expect(text).toContain('openai');
    expect(text).toContain('gpt-5');
    expect(text).toContain('You are helpful'); // the message body from `content`
    expect(text).toContain('0.7'); // temperature
  });

  it('renders default temperature/maxTokens when absent', async () => {
    const fixture = await mount({
      isOpen: true,
      logs: [log({ request: { messageCount: 0, messages: [], toolCount: 0 } })],
    });
    expect(fixture.nativeElement.textContent).toContain('default');
  });

  it('renders the usage tab counts and duration', async () => {
    const fixture = await mount({ isOpen: true, logs: [log()] });
    tabByLabel(fixture, 'Usage').click();
    fixture.detectChanges();
    const text = fixture.nativeElement.textContent;
    expect(text).toContain('150'); // total tokens
    expect(text).toContain('1.23s'); // two-decimal duration
  });

  it('renders an error banner on the response tab when the response errored', async () => {
    const fixture = await mount({
      isOpen: true,
      logs: [log({ response: { content: '', contentLength: 0, error: 'rate limited' } })],
    });
    tabByLabel(fixture, 'Response').click();
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('rate limited');
  });

  it('shows the multi-log selector only when more than one log', async () => {
    const one = await mount({ isOpen: true, logs: [log()] });
    expect(one.nativeElement.querySelector('#log-select')).toBeNull();
    const two = await mount({ isOpen: true, logs: [log(), log({ id: 'log-2' })] });
    expect(two.nativeElement.querySelector('#log-select')).not.toBeNull();
  });
});
