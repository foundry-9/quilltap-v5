import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { LlmLogDto } from '../../../chat/llm-logs.api';
import { LlmLogsCard } from './llm-logs-card';

function sampleLog(over: Partial<LlmLogDto> = {}): LlmLogDto {
  return {
    id: 'log-1',
    userId: 'u1',
    type: 'MEMORY_EXTRACTION',
    provider: 'deepseek',
    modelName: 'deepseek-chat',
    request: { messageCount: 1, messages: [], toolCount: 0 },
    response: { content: 'ok', contentLength: 2 },
    usage: { promptTokens: 10, completionTokens: 5, totalTokens: 15 },
    durationMs: 900,
    createdAt: '2026-07-24T12:00:00.000Z',
    updatedAt: '2026-07-24T12:00:00.000Z',
    ...over,
  };
}

async function mount(logs: LlmLogDto[]): Promise<ComponentFixture<LlmLogsCard>> {
  const client: Partial<CoreClient> = {
    dispatchData: (async () => ({ logs })) as CoreClient['dispatchData'],
  };
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [LlmLogsCard],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(LlmLogsCard);
  fixture.detectChanges();
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

describe('LlmLogsCard', () => {
  it('shows the empty state when there are no logs', async () => {
    const fixture = await mount([]);
    expect(fixture.nativeElement.textContent).toContain('No LLM logs yet');
  });

  it('renders a row per log with the type badge and provider/model', async () => {
    const fixture = await mount([sampleLog()]);
    const text = fixture.nativeElement.textContent;
    expect(text).toContain('Memory'); // MEMORY_EXTRACTION → Memory
    expect(text).toContain('deepseek/deepseek-chat');
    expect(text).toContain('15 tokens');
  });

  it('opens the viewer modal when a row is clicked', async () => {
    const fixture = await mount([sampleLog()]);
    expect(fixture.nativeElement.querySelector('[role="dialog"]')).toBeNull();
    const row = fixture.nativeElement.querySelector('.space-y-2 button') as HTMLButtonElement;
    row.click();
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('[role="dialog"]')).not.toBeNull();
  });
});

/**
 * The `686954937` drift (P4.D181): Wire Records labels the new log type (v4
 * `llm-logs-card.tsx:44`). v4's map falls through to the RAW type for anything
 * unlisted, so the pre-fix rendering of a `VOICE_REWRITE` row was the bare
 * enum name — which is what makes this row a discriminator and not decoration.
 */
describe('LlmLogsCard — the VOICE_REWRITE label (v4 686954937)', () => {
  it('reads "Voice Rewrite", not the raw type', async () => {
    const fixture = await mount([sampleLog({ type: 'VOICE_REWRITE' })]);
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Voice Rewrite');
    expect(text).not.toContain('VOICE_REWRITE');
  });
});

