import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { TasksQueueCard } from './tasks-queue-card';
import type { JobDetail, QueueData } from './tasks-queue.api';

function job(over: Partial<JobDetail> = {}): JobDetail {
  return {
    id: 'job-1',
    type: 'MEMORY_EXTRACTION',
    typeName: 'Memory Extraction',
    status: 'PENDING',
    priority: 0,
    attempts: 0,
    maxAttempts: 3,
    scheduledAt: new Date().toISOString(),
    startedAt: null,
    lastError: null,
    estimatedTokens: 1500,
    ...over,
  };
}

function queueData(over: Partial<QueueData> = {}): QueueData {
  return {
    stats: { pending: 1, processing: 0, failed: 0, completed: 5, dead: 0, paused: 0, activeTotal: 1 },
    jobs: [job()],
    totalEstimatedTokens: 1500,
    processorStatus: { running: false },
    maxConcurrentJobs: 4,
    ...over,
  };
}

interface Stub {
  calls: { type: string; [k: string]: unknown }[];
  client: Partial<CoreClient>;
}

function stub(data: QueueData): Stub {
  const calls: { type: string; [k: string]: unknown }[] = [];
  const dispatchData = vi.fn(async (req: { type: string; jobId?: string }) => {
    calls.push(req);
    switch (req.type) {
      case 'systemTasksQueue':
        return data as unknown as Record<string, unknown>;
      case 'systemJobGet':
        return { job: { ...job({ id: req.jobId }), payload: { foo: 1 }, createdAt: '', updatedAt: '', userId: 'u' } };
      default:
        return {};
    }
  });
  return { calls, client: { dispatchData: dispatchData as unknown as CoreClient['dispatchData'] } };
}

async function mount(s: Stub): Promise<ComponentFixture<TasksQueueCard>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [TasksQueueCard],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: s.client }],
  });
  const fixture = TestBed.createComponent(TasksQueueCard);
  fixture.detectChanges();
  for (let i = 0; i < 8; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

describe('TasksQueueCard', () => {
  it('renders the concurrency slider, stats, and a job row', async () => {
    const fixture = await mount(stub(queueData()));
    const text = fixture.nativeElement.textContent;
    expect(text).toContain('Simultaneous Labours — 4');
    expect(text).toContain('Memory Extraction');
    expect(text).toContain('Queue Stopped');
    expect(fixture.nativeElement.querySelector('input[type="range"]')?.getAttribute('max')).toBe('32');
  });

  it('start is enabled when stopped with active jobs, and dispatches control', async () => {
    const s = stub(queueData());
    const fixture = await mount(s);
    const start = Array.from(fixture.nativeElement.querySelectorAll('button')).find(
      (b) => (b as HTMLButtonElement).textContent?.includes('Start Queue'),
    ) as HTMLButtonElement;
    expect(start.disabled).toBe(false);
    start.click();
    await new Promise((r) => setTimeout(r, 0));
    expect(s.calls).toContainEqual({ type: 'systemTasksQueueControl', action: 'start' });
  });

  it('commits a changed concurrency on drag end', async () => {
    const s = stub(queueData());
    const fixture = await mount(s);
    const slider = fixture.nativeElement.querySelector('input[type="range"]') as HTMLInputElement;
    slider.dispatchEvent(new Event('mousedown'));
    slider.value = '8';
    slider.dispatchEvent(new Event('input'));
    slider.dispatchEvent(new Event('mouseup'));
    await new Promise((r) => setTimeout(r, 0));
    expect(s.calls).toContainEqual({ type: 'systemJobConcurrencySet', maxConcurrentJobs: 8 });
  });

  it('opens the detail modal when a job is viewed', async () => {
    const fixture = await mount(stub(queueData()));
    const view = fixture.nativeElement.querySelector('button[title="View Details"]') as HTMLButtonElement;
    view.click();
    for (let i = 0; i < 4; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    expect(fixture.nativeElement.querySelector('[role="dialog"]')).not.toBeNull();
    expect(fixture.nativeElement.textContent).toContain('Job Parameters');
  });

  it('shows the empty state when there are no jobs', async () => {
    const fixture = await mount(stub(queueData({ jobs: [], stats: { pending: 0, processing: 0, failed: 0, completed: 0, dead: 0, paused: 0, activeTotal: 0 } })));
    expect(fixture.nativeElement.textContent).toContain('Queue is empty');
  });
});
