import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { beforeEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type { ParticipantDetail } from '../../core/core-contract';
import { cleanArguments, categoryInfo, RunToolModal } from './run-tool-modal';
import type { AvailableTool } from './tool-settings';
import { ToastService } from '../../ui/toast.service';

/** Run Tool (v4 `RunToolModal.tsx`). */

interface Req {
  type: string;
  [k: string]: unknown;
}

const sent: Req[] = [];
let runAnswer: Record<string, unknown> | Error = { success: true };
let tools: AvailableTool[] = [];

function stubClient(): Partial<CoreClient> {
  return {
    dispatchData: (async (req: Req) => {
      sent.push(req);
      if (req['type'] === 'toolsList') return { tools };
      if (runAnswer instanceof Error) throw runAnswer;
      return runAnswer;
    }) as unknown as CoreClient['dispatchData'],
  };
}

function participant(id: string, characterId: string, name: string): ParticipantDetail {
  return {
    id,
    type: 'CHARACTER',
    displayOrder: 0,
    isActive: true,
    controlledBy: 'llm',
    status: 'active',
    character: { id: characterId, name, title: null, avatarUrl: null, defaultImageId: null, defaultImage: null },
    connectionProfile: null,
    imageProfile: null,
    createdAt: '',
    updatedAt: '',
  } as ParticipantDetail;
}

@Component({
  imports: [RunToolModal],
  template: `
    <qt-run-tool-modal
      [chatId]="'chat-1'"
      [participants]="participants()"
      (executed)="ran = ran + 1"
      (close)="closed = closed + 1"
    />
  `,
})
class Host {
  readonly participants = signal<ParticipantDetail[]>([
    participant('p1', 'char-1', 'Lorian'),
    participant('p2', 'char-2', 'Riya'),
  ]);
  ran = 0;
  closed = 0;
}

async function render(): Promise<ComponentFixture<Host>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [Host],
    providers: [
      { provide: CoreClient, useValue: stubClient() },
      provideTanStackQuery(new QueryClient()),
    ],
  });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

async function settle(fixture: ComponentFixture<Host>): Promise<void> {
  for (let i = 0; i < 8; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

function pick(fixture: ComponentFixture<Host>, name: string): HTMLButtonElement {
  return (Array.from(fixture.nativeElement.querySelectorAll('button')) as HTMLButtonElement[]).find(
    (b) => b.textContent!.includes(name),
  )!;
}

function runButton(fixture: ComponentFixture<Host>): HTMLButtonElement {
  return (Array.from(fixture.nativeElement.querySelectorAll('button')) as HTMLButtonElement[]).find(
    (b) => b.textContent!.trim() === 'Run Tool' || b.textContent!.trim() === 'Running...',
  )!;
}

/** The toast stack this render raised, newest last. */
function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

describe('cleanArguments', () => {
  it('drops undefined, null and empty string — and keeps false and zero', () => {
    expect(
      cleanArguments({ a: '', b: null, c: undefined, d: false, e: 0, f: 'x' }),
    ).toEqual({ d: false, e: 0, f: 'x' });
  });
});

describe('categoryInfo', () => {
  it('title-cases an unknown category and gives it the fallback icon', () => {
    expect(categoryInfo('memory')).toEqual({ label: 'Memory', icon: '🧠' });
    expect(categoryInfo('sundry')).toEqual({ label: 'Sundry', icon: '⚙️' });
  });
});

describe('RunToolModal', () => {
  beforeEach(() => {
    sent.length = 0;
    runAnswer = { success: true };
    tools = [
      {
        id: 'search_memories',
        name: 'Search Memories',
        description: 'Look things up',
        source: 'built-in',
        category: 'memory',
        parameters: {
          type: 'object',
          properties: { query: { type: 'string' }, limit: { type: 'integer', default: 5 } },
          required: ['query'],
        },
      },
      {
        id: 'quiet',
        name: 'Quiet Tool',
        description: 'Never offered',
        source: 'built-in',
        userInvocable: false,
      },
      {
        id: 'shell_run',
        name: 'Run In Workspace',
        description: 'Shell',
        source: 'built-in',
        category: 'shell',
        available: false,
        unavailableReason: 'No workspace is open',
      },
    ];
  });

  it('asks for the inventory WITH schemas, and hides non-invocable tools', async () => {
    const fixture = await render();
    expect(sent[0]).toEqual({ type: 'toolsList', chatId: 'chat-1', includeSchemas: true });
    expect(fixture.nativeElement.textContent).toContain('Search Memories');
    expect(fixture.nativeElement.textContent).not.toContain('Quiet Tool');
  });

  it('lists an unavailable tool with its reason, disabled', async () => {
    const fixture = await render();
    const row = pick(fixture, 'Run In Workspace');
    expect(row.disabled).toBe(true);
    expect(row.textContent).toContain('No workspace is open');
    expect(row.textContent).not.toContain('Shell');
  });

  it('searches over name, description and id', async () => {
    const fixture = await render();
    const box = fixture.nativeElement.querySelector('input[type="text"]') as HTMLInputElement;
    box.value = 'look things';
    box.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('Search Memories');

    box.value = 'shell_run';
    box.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('Run In Workspace');

    box.value = 'nothing at all';
    box.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('No tools match your search.');
  });

  it('pre-populates the schema defaults and keeps Run dead until it validates', async () => {
    const fixture = await render();
    pick(fixture, 'Search Memories').click();
    await settle(fixture);

    // `limit` carries a default, so it is pre-included and shown; `query` is
    // required and empty, so the form is not yet valid.
    expect(fixture.nativeElement.textContent).toContain('(incomplete)');
    expect(fixture.nativeElement.textContent).toContain('"limit": 5');
    expect(runButton(fixture).disabled).toBe(true);

    const query = fixture.nativeElement.querySelector('#field-query') as HTMLInputElement;
    query.value = 'the lighthouse';
    query.dispatchEvent(new Event('input'));
    await settle(fixture);
    expect(fixture.nativeElement.textContent).toContain('(valid)');
    expect(runButton(fixture).disabled).toBe(false);
  });

  it('sends the tool ID, the cleaned arguments and the CHARACTER id', async () => {
    const fixture = await render();
    pick(fixture, 'Search Memories').click();
    await settle(fixture);
    const query = fixture.nativeElement.querySelector('#field-query') as HTMLInputElement;
    query.value = 'the lighthouse';
    query.dispatchEvent(new Event('input'));
    await settle(fixture);

    runButton(fixture).click();
    await settle(fixture);

    expect(sent.find((r) => r.type === 'chatRunTool')).toEqual({
      type: 'chatRunTool',
      chatId: 'chat-1',
      // The id, not "Search Memories".
      toolName: 'search_memories',
      arguments: { limit: 5, query: 'the lighthouse' },
      characterId: 'char-1',
      private: false,
    });
    expect(fixture.componentInstance.ran).toBe(1);
  });

  it('treats a 200 that says success:false as a failure', async () => {
    runAnswer = { success: false, error: 'the tool declined' };
    const fixture = await render();
    pick(fixture, 'Search Memories').click();
    await settle(fixture);
    const query = fixture.nativeElement.querySelector('#field-query') as HTMLInputElement;
    query.value = 'x';
    query.dispatchEvent(new Event('input'));
    await settle(fixture);
    runButton(fixture).click();
    await settle(fixture);

    expect(toasts()).toEqual([{ type: 'error', message: 'the tool declined' }]);
    expect(fixture.componentInstance.ran).toBe(0);
    expect(fixture.componentInstance.closed).toBe(0);
  });

  it('says so rather than showing an empty form for a parameterless tool', async () => {
    tools = [
      { id: 'ping', name: 'Ping', description: 'no args', source: 'built-in', category: 'utility' },
    ];
    const fixture = await render();
    pick(fixture, 'Ping').click();
    await settle(fixture);
    expect(fixture.nativeElement.textContent).toContain('This tool requires no parameters.');
    expect(runButton(fixture).disabled).toBe(false);
  });

  it('Back returns to the picker and forgets the staged arguments', async () => {
    const fixture = await render();
    pick(fixture, 'Search Memories').click();
    await settle(fixture);
    pick(fixture, 'Back').click();
    await settle(fixture);
    expect(fixture.nativeElement.querySelector('input[aria-label="Search tools"]')).toBeTruthy();

    pick(fixture, 'Search Memories').click();
    await settle(fixture);
    expect(fixture.nativeElement.textContent).toContain('(incomplete)');
  });
});
