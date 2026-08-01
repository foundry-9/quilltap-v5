import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { beforeEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../core/core-client';
import { ToastService } from '../ui/toast.service';
import { ChatProjectModal } from './chat-project-modal';

/** Assign to Project (v4 `ChatProjectModal.tsx`). */

interface Req {
  type: string;
  [k: string]: unknown;
}

const sent: Req[] = [];
let updateFails: string | null = null;

function stubClient(): Partial<CoreClient> {
  return {
    dispatch: (async (req: Req) => {
      sent.push(req);
      if (updateFails) return { type: 'error', data: { message: updateFails } };
      return { type: 'chat', data: { chat: {} } };
    }) as CoreClient['dispatch'],
    dispatchData: (async () => ({
      projects: [
        { id: 'p1', name: 'The Long Voyage' },
        { id: 'p2', name: 'Shore Leave' },
      ],
    })) as unknown as CoreClient['dispatchData'],
  };
}

@Component({
  imports: [ChatProjectModal],
  template: `
    <qt-chat-project-modal
      [chatId]="'chat-1'"
      [projectId]="projectId()"
      [projectName]="projectName()"
      (assigned)="emitted = emitted + 1"
      (close)="closed = closed + 1"
    />
  `,
})
class Host {
  readonly projectId = signal<string | null>(null);
  readonly projectName = signal<string | null>(null);
  emitted = 0;
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

/** TanStack's query resolution needs macrotask turns, not just `whenStable`. */
async function settle(fixture: ComponentFixture<Host>): Promise<void> {
  for (let i = 0; i < 8; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

function picker(fixture: ComponentFixture<Host>): HTMLSelectElement {
  return fixture.nativeElement.querySelector('#chat-project');
}

function save(fixture: ComponentFixture<Host>): HTMLButtonElement {
  return (Array.from(fixture.nativeElement.querySelectorAll('button')) as HTMLButtonElement[]).find(
    (b) => b.textContent!.trim() === 'Save' || b.textContent!.trim() === 'Saving...',
  )!;
}

async function choose(fixture: ComponentFixture<Host>, value: string): Promise<void> {
  picker(fixture).value = value;
  picker(fixture).dispatchEvent(new Event('change'));
  await fixture.whenStable();
  fixture.detectChanges();
}

/** The toast stack this render raised, newest last. */
function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

describe('ChatProjectModal', () => {
  beforeEach(() => {
    sent.length = 0;
    updateFails = null;
  });

  it('lists the projects under a real "No project" choice', async () => {
    const fixture = await render();
    expect(
      Array.from(picker(fixture).options).map((o) => [o.value, o.textContent!.trim()]),
    ).toEqual([
      ['', 'No project'],
      ['p1', 'The Long Voyage'],
      ['p2', 'Shore Leave'],
    ]);
    expect(fixture.nativeElement.textContent).not.toContain('Currently in:');
  });

  it('names the current project and seeds the picker from it', async () => {
    const fixture = await render();
    fixture.componentInstance.projectId.set('p2');
    fixture.componentInstance.projectName.set('Shore Leave');
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('Currently in:');
    expect(picker(fixture).value).toBe('p2');
  });

  it('files the chat and reports v4’s wording', async () => {
    const fixture = await render();
    await choose(fixture, 'p1');
    save(fixture).click();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(sent).toEqual([
      { type: 'chatUpdate', chatId: 'chat-1', chat: { projectId: 'p1' } },
    ]);
    expect(fixture.componentInstance.emitted).toBe(1);
    expect(toasts()).toEqual([{ type: 'success', message: 'Chat moved to "The Long Voyage"' }]);
    expect(fixture.componentInstance.closed).toBe(1);
  });

  it('detaches with an explicit null — "No project" is a choice, not an empty state', async () => {
    const fixture = await render();
    fixture.componentInstance.projectId.set('p1');
    fixture.componentInstance.projectName.set('The Long Voyage');
    fixture.detectChanges();
    await choose(fixture, '');
    save(fixture).click();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(sent).toEqual([{ type: 'chatUpdate', chatId: 'chat-1', chat: { projectId: null } }]);
    expect(toasts()).toEqual([{ type: 'success', message: 'Chat removed from project' }]);
  });

  it('closes without writing when the choice is the chat’s current project', async () => {
    const fixture = await render();
    fixture.componentInstance.projectId.set('p1');
    fixture.detectChanges();
    save(fixture).click();
    await fixture.whenStable();
    fixture.detectChanges();
    expect(sent).toEqual([]);
    expect(fixture.componentInstance.closed).toBe(1);
  });

  it('reports a failed assignment and stays open', async () => {
    updateFails = 'the filing cabinet is locked';
    const fixture = await render();
    await choose(fixture, 'p1');
    save(fixture).click();
    await fixture.whenStable();
    fixture.detectChanges();
    expect(toasts()).toEqual([{ type: 'error', message: 'the filing cabinet is locked' }]);
    expect(fixture.componentInstance.closed).toBe(0);
  });
});
