import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { coreStreamStub } from '../../../core/core-client.testing';
import { ToastService } from '../../../ui/toast.service';
import { ConversationSummaryRegenerateCard } from './conversation-summary-regenerate-card';

/**
 * Ported from v4's `conversation-summary-regenerate-card.tsx`: the swallowed
 * status read, the in-flight line, the single-click enqueue toasting the
 * server's message, and the error path.
 */

const toasts = { showSuccess: vi.fn(), showError: vi.fn() } as unknown as ToastService;

interface Stub {
  client: Partial<CoreClient>;
  regen: ReturnType<typeof vi.fn>;
}

function stubClient(opts: {
  status?: () => Promise<number>;
  regen?: () => Promise<{ message?: string }>;
}): Stub {
  const regen = vi.fn(opts.regen ?? (async () => ({ message: 'Re-mirroring in the background' })));
  return {
    regen,
    client: {
      ...coreStreamStub(),
      conversationSummariesStatus: (opts.status ??
        (async () => 0)) as unknown as CoreClient['conversationSummariesStatus'],
      regenerateConversationSummaries: regen as unknown as CoreClient['regenerateConversationSummaries'],
    } as unknown as Partial<CoreClient>,
  };
}

async function render(stub: Stub): Promise<ComponentFixture<ConversationSummaryRegenerateCard>> {
  vi.clearAllMocks();
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [ConversationSummaryRegenerateCard],
    providers: [
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: stub.client },
      { provide: ToastService, useValue: toasts },
    ],
  });
  const fixture = TestBed.createComponent(ConversationSummaryRegenerateCard);
  fixture.detectChanges();
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

function text(fixture: ComponentFixture<ConversationSummaryRegenerateCard>): string {
  return (fixture.nativeElement as HTMLElement).textContent ?? '';
}

function regenButton(
  fixture: ComponentFixture<ConversationSummaryRegenerateCard>,
): HTMLButtonElement {
  return (fixture.nativeElement as HTMLElement).querySelector('button')!;
}

describe('ConversationSummaryRegenerateCard', () => {
  it('shows no in-flight line when nothing is running', async () => {
    const fixture = await render(stubClient({ status: async () => 0 }));
    expect(text(fixture)).not.toContain('In flight');
    expect(regenButton(fixture).textContent).toContain('Regenerate conversation summaries');
  });

  it('shows the in-flight line with correct pluralisation', async () => {
    const fixture = await render(stubClient({ status: async () => 3 }));
    expect(text(fixture)).toContain('In flight: 3 regenerations.');
  });

  it('singular in-flight line at one', async () => {
    const fixture = await render(stubClient({ status: async () => 1 }));
    expect(text(fixture)).toContain('In flight: 1 regeneration.');
  });

  it('swallows a status-read failure — no error, the button still works', async () => {
    const fixture = await render(
      stubClient({
        status: async () => {
          throw new Error('status boom');
        },
      }),
    );
    expect(text(fixture)).not.toContain('status boom');
    expect(regenButton(fixture).disabled).toBe(false);
  });

  it('enqueues on click and toasts the server message', async () => {
    const stub = stubClient({ regen: async () => ({ message: 'Summaries queued' }) });
    const fixture = await render(stub);
    regenButton(fixture).click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    expect(stub.regen).toHaveBeenCalledOnce();
    expect(toasts.showSuccess).toHaveBeenCalledWith('Summaries queued');
  });

  it('surfaces an enqueue failure inline and toasts it', async () => {
    const stub = stubClient({
      regen: async () => {
        throw new Error('Failed to start regeneration');
      },
    });
    const fixture = await render(stub);
    regenButton(fixture).click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    expect(text(fixture)).toContain('Failed to start regeneration');
    expect(toasts.showError).toHaveBeenCalledWith('Failed to start regeneration');
  });
});
