import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { type AnnounceableMessage, ReplyAnnouncer } from './reply-announcer';

/** P4.D358 — v4 `components/chat/ReplyAnnouncer.tsx` (`10e09b7c3`), case for case. */
@Component({
  imports: [ReplyAnnouncer],
  template: `<qt-reply-announcer [messages]="messages()" [nameFor]="nameFor" />`,
})
class Host {
  readonly messages = signal<readonly AnnounceableMessage[]>([]);
  readonly nameFor = (pid: string | null | undefined): string | undefined =>
    pid === 'p-ada' ? 'Ada' : undefined;
}

const user = (id: string): AnnounceableMessage => ({ id, role: 'USER', content: 'hi', participantId: 'p-me' });
const reply = (id: string, content = 'Good evening.', participantId: string | null = 'p-ada'): AnnounceableMessage => ({
  id,
  role: 'ASSISTANT',
  content,
  participantId,
});

function render(initial: readonly AnnounceableMessage[]) {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  fixture.componentInstance.messages.set(initial);
  fixture.detectChanges();
  const region = () => fixture.nativeElement.querySelector('[role="status"]') as HTMLElement;
  const push = (...m: AnnounceableMessage[]) => {
    fixture.componentInstance.messages.update((prev) => [...prev, ...m]);
    fixture.detectChanges();
    fixture.detectChanges();
  };
  return { fixture, region, push };
}

describe('ReplyAnnouncer (v4 ReplyAnnouncer.tsx)', () => {
  it('is a polite, atomic, visually hidden live region', () => {
    const { region } = render([]);
    expect(region().getAttribute('aria-live')).toBe('polite');
    expect(region().getAttribute('aria-atomic')).toBe('true');
    expect(region().classList.contains('sr-only')).toBe(true);
  });

  it('stays silent on first render with history already present', () => {
    const { region } = render([user('u1'), reply('a1')]);
    expect(region().textContent?.trim()).toBe('');
  });

  it('speaks a new ASSISTANT row as "<name> replied: <text>"', () => {
    const { region, push } = render([user('u1'), reply('a1')]);
    push(user('u2'), reply('a2', 'Quite  so.\n\nIndeed.'));
    expect(region().textContent?.trim()).toBe('Ada replied: Quite so. Indeed.');
  });

  it('ignores system-sender rows and USER rows (no re-announce)', () => {
    const { region, push } = render([reply('a1')]);
    push({ id: 's1', role: 'ASSISTANT', content: 'Aurora speaks', participantId: null, systemSender: 'AURORA' });
    expect(region().textContent?.trim()).toBe('');
    push(user('u2'));
    expect(region().textContent?.trim()).toBe('');
  });

  it('cuts at 300 characters and ends in an ellipsis', () => {
    const { region, push } = render([]);
    push(reply('a1', 'x'.repeat(400)));
    expect(region().textContent?.trim()).toBe(`Ada replied: ${'x'.repeat(300)}…`);
  });

  it('falls back to "A character" for an unknown participant', () => {
    const { region, push } = render([]);
    push(reply('a1', 'Hello.', 'p-unknown'));
    expect(region().textContent?.trim()).toBe('A character replied: Hello.');
  });

  it('is silent on a re-render with the SAME latest id', () => {
    const { fixture, region, push } = render([]);
    push(reply('a1', 'One.'));
    expect(region().textContent?.trim()).toBe('Ada replied: One.');
    // A new array with the same latest reply (a refetch) says nothing new.
    fixture.componentInstance.messages.set([reply('a1', 'One.')]);
    fixture.detectChanges();
    expect(region().textContent?.trim()).toBe('Ada replied: One.');
    push(user('u9'));
    expect(region().textContent?.trim()).toBe('Ada replied: One.');
  });
});
