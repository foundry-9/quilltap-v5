import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { beforeEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../core/core-client';
import { ToastService } from '../ui/toast.service';
import type { MessageDto, ParticipantDetail } from '../core/core-contract';
import { BulkCharacterReplaceModal, UNASSIGNED_USER } from './bulk-character-replace-modal';

/**
 * Bulk Character Replace (v4 `BulkCharacterReplaceModal.tsx`). The load-bearing
 * behaviour is the `__UNASSIGNED__` sentinel: the one selection that becomes a
 * real `null` on the wire, and the one the affected-count must not treat as
 * "nothing chosen".
 */

interface Req {
  type: string;
  [k: string]: unknown;
}

const sent: Req[] = [];
let answer: Record<string, unknown> | Error = { messagesUpdated: 3, memoriesDeleted: 2 };

function stubClient(): Partial<CoreClient> {
  return {
    dispatchData: (async (req: Req) => {
      sent.push(req);
      if (answer instanceof Error) throw answer;
      return answer;
    }) as unknown as CoreClient['dispatchData'],
  };
}

function participant(id: string, name: string, controlledBy: 'llm' | 'user'): ParticipantDetail {
  return {
    id,
    type: 'CHARACTER',
    displayOrder: 0,
    isActive: true,
    controlledBy,
    status: 'active',
    character: {
      id: `char-${id}`,
      name,
      title: null,
      avatarUrl: null,
      defaultImageId: null,
      defaultImage: null,
    },
    connectionProfile: null,
    imageProfile: null,
    createdAt: '',
    updatedAt: '',
  } as ParticipantDetail;
}

function message(id: string, role: string, participantId: string | null): MessageDto {
  return { id, role, participantId } as unknown as MessageDto;
}

@Component({
  imports: [BulkCharacterReplaceModal],
  template: `
    <qt-bulk-character-replace-modal
      [chatId]="'chat-1'"
      [participants]="participants()"
      [messages]="messages()"
      (reattributed)="emitted = emitted + 1"
      (close)="closed = closed + 1"
    />
  `,
})
class Host {
  readonly participants = signal<ParticipantDetail[]>([
    participant('p1', 'Lorian', 'llm'),
    participant('p2', 'Riya', 'user'),
  ]);
  readonly messages = signal<MessageDto[]>([
    message('m1', 'ASSISTANT', 'p1'),
    message('m2', 'assistant', 'p1'),
    message('m3', 'USER', 'p2'),
    message('m4', 'USER', null),
  ]);
  emitted = 0;
  closed = 0;
}

async function render(): Promise<ComponentFixture<Host>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [Host],
    providers: [{ provide: CoreClient, useValue: stubClient() }],
  });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return fixture;
}

function select(fixture: ComponentFixture<Host>, id: string): HTMLSelectElement {
  return fixture.nativeElement.querySelector(`#${id}`);
}

function choose(fixture: ComponentFixture<Host>, id: string, value: string): void {
  const el = select(fixture, id);
  el.value = value;
  el.dispatchEvent(new Event('change'));
  fixture.detectChanges();
}

function submit(fixture: ComponentFixture<Host>): HTMLButtonElement {
  return (Array.from(fixture.nativeElement.querySelectorAll('button')) as HTMLButtonElement[]).find(
    (b) => b.textContent!.trim().startsWith('Re-attribut'),
  )!;
}

function options(fixture: ComponentFixture<Host>, id: string): string[] {
  return Array.from(select(fixture, id).options).map((o) => o.value);
}

/** The toast stack this render raised, newest last. */
function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

describe('BulkCharacterReplaceModal', () => {
  beforeEach(() => {
    sent.length = 0;
    answer = { messagesUpdated: 3, memoriesDeleted: 2 };
  });

  it('offers the unassigned sentinel only while such messages exist', async () => {
    const fixture = await render();
    expect(options(fixture, 'qt-bulk-source')).toEqual(['', UNASSIGNED_USER, 'p1', 'p2']);

    fixture.componentInstance.messages.set([message('m1', 'ASSISTANT', 'p1')]);
    fixture.detectChanges();
    expect(options(fixture, 'qt-bulk-source')).toEqual(['', 'p1', 'p2']);
  });

  it('counts matching messages, case-insensitively on the role', async () => {
    const fixture = await render();
    choose(fixture, 'qt-bulk-source', 'p1');
    choose(fixture, 'qt-bulk-target', 'p2');
    // Both of Lorian's messages, one of them stored lower-case.
    expect(fixture.nativeElement.textContent).toContain('2 messages will be re-attributed');

    const userOnly = Array.from(
      fixture.nativeElement.querySelectorAll('input[type="radio"]'),
    ).find((r) => (r as HTMLInputElement).value === 'USER') as HTMLInputElement;
    userOnly.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('0 messages will be re-attributed');
  });

  it('counts the sentinel’s messages rather than treating it as no selection', async () => {
    const fixture = await render();
    choose(fixture, 'qt-bulk-source', UNASSIGNED_USER);
    choose(fixture, 'qt-bulk-target', 'p1');
    expect(fixture.nativeElement.textContent).toContain('1 message will be re-attributed');
    expect(submit(fixture).disabled).toBe(false);
  });

  it('lowers ONLY the sentinel to a null source', async () => {
    const fixture = await render();
    choose(fixture, 'qt-bulk-source', UNASSIGNED_USER);
    choose(fixture, 'qt-bulk-target', 'p1');
    submit(fixture).click();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(sent).toEqual([
      {
        type: 'chatBulkReattribute',
        chatId: 'chat-1',
        sourceParticipantId: null,
        targetParticipantId: 'p1',
        roleFilter: 'both',
      },
    ]);
  });

  it('sends a participant source as its id', async () => {
    const fixture = await render();
    choose(fixture, 'qt-bulk-source', 'p1');
    choose(fixture, 'qt-bulk-target', 'p2');
    submit(fixture).click();
    await fixture.whenStable();
    fixture.detectChanges();
    expect(sent[0]['sourceParticipantId']).toBe('p1');
  });

  it('excludes the source from the targets — but not when the source is the sentinel', async () => {
    const fixture = await render();
    choose(fixture, 'qt-bulk-source', 'p1');
    expect(options(fixture, 'qt-bulk-target')).toEqual(['', 'p2']);

    choose(fixture, 'qt-bulk-source', UNASSIGNED_USER);
    expect(options(fixture, 'qt-bulk-target')).toEqual(['', 'p1', 'p2']);
  });

  it('clears a target that becomes the source', async () => {
    const fixture = await render();
    choose(fixture, 'qt-bulk-source', 'p1');
    choose(fixture, 'qt-bulk-target', 'p2');
    choose(fixture, 'qt-bulk-source', 'p2');
    expect(select(fixture, 'qt-bulk-target').value).toBe('');
    expect(submit(fixture).disabled).toBe(true);
  });

  it('pluralises both counts and drops the memories line at zero', async () => {
    const fixture = await render();
    choose(fixture, 'qt-bulk-source', 'p1');
    choose(fixture, 'qt-bulk-target', 'p2');
    submit(fixture).click();
    await fixture.whenStable();
    expect(fixture.componentInstance.emitted).toBe(1);
    expect(toasts()).toEqual([
      { type: 'success', message: '3 messages re-attributed to Riya. 2 memories deleted.' },
    ]);

    const second = await render();
    answer = { messagesUpdated: 1, memoriesDeleted: 0 };
    choose(second, 'qt-bulk-source', 'p1');
    choose(second, 'qt-bulk-target', 'p2');
    submit(second).click();
    await second.whenStable();
    expect(toasts()).toEqual([
      { type: 'success', message: '1 message re-attributed to Riya.' },
    ]);
  });

  it('refuses the whole dialog when there is nothing it could move', async () => {
    const fixture = await render();
    fixture.componentInstance.participants.set([participant('p1', 'Lorian', 'llm')]);
    fixture.componentInstance.messages.set([message('m1', 'ASSISTANT', 'p1')]);
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('needs at least 2 participants');
    expect(submit(fixture).disabled).toBe(true);
  });

  it('reports a failed re-attribution and stays open', async () => {
    answer = new Error('the ledger is jammed');
    const fixture = await render();
    choose(fixture, 'qt-bulk-source', 'p1');
    choose(fixture, 'qt-bulk-target', 'p2');
    submit(fixture).click();
    await fixture.whenStable();
    fixture.detectChanges();
    expect(toasts()).toEqual([{ type: 'error', message: 'the ledger is jammed' }]);
    expect(fixture.componentInstance.closed).toBe(0);
  });
});
