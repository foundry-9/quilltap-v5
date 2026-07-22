import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { beforeEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { ChatSection, type ChatSectionState } from './chat-section';

/**
 * The sidebar's Chat section — above all **the Story's Clock**: the copy
 * (byte-for-byte from v4 `ChatSidebar.tsx:1147-1165`), the `{chat:
 * {timelineMode}}` write, and the value the select keeps afterwards.
 */

interface Req {
  type: string;
  [k: string]: unknown;
}

const sent: Req[] = [];
let failNext = false;

function stubClient(): Partial<CoreClient> {
  return {
    dispatch: (async (req: Req) => {
      sent.push(req);
      if (failNext) {
        failNext = false;
        return { type: 'error', data: { message: 'the clock is stuck' } };
      }
      return { type: 'chat', data: { chat: {} } };
    }) as CoreClient['dispatch'],
    dispatchData: (async () => ({})) as CoreClient['dispatchData'],
    dispatchExpect: (async () => ({
      type: 'apiKeys',
      data: { apiKeys: [], count: 0 },
    })) as unknown as CoreClient['dispatchExpect'],
  };
}

function state(over: Partial<ChatSectionState> = {}): ChatSectionState {
  return {
    roleplayTemplateId: null,
    timelineMode: null,
    imageProfileId: null,
    alertCharactersOfLanternImages: null,
    projectId: null,
    projectName: null,
    ...over,
  };
}

@Component({
  imports: [ChatSection],
  template: `
    <qt-chat-section
      [chatId]="'chat-1'"
      [state]="chatState()"
      [sectionOpen]="true"
      (chatUpdated)="refetched = refetched + 1"
    />
  `,
})
class Host {
  readonly chatState = signal<ChatSectionState>(state());
  refetched = 0;
}

async function render(): Promise<ComponentFixture<Host>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [Host],
    providers: [
      { provide: CoreClient, useValue: stubClient() },
      provideTanStackQuery(new QueryClient()),
      provideRouter([]),
    ],
  });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return fixture;
}

function clockSelect(fixture: ComponentFixture<Host>): HTMLSelectElement {
  const labels = Array.from(fixture.nativeElement.querySelectorAll('label')) as HTMLElement[];
  const label = labels.find((l) => l.textContent?.includes('The Story’s Clock'))!;
  return label.querySelector('select') as HTMLSelectElement;
}

function clockHelp(fixture: ComponentFixture<Host>): string {
  const labels = Array.from(fixture.nativeElement.querySelectorAll('label')) as HTMLElement[];
  const label = labels.find((l) => l.textContent?.includes('The Story’s Clock'))!;
  return (label.querySelector('span.qt-text-secondary') as HTMLElement).textContent!
    .replace(/\s+/g, ' ')
    .trim();
}

async function choose(
  fixture: ComponentFixture<Host>,
  select: HTMLSelectElement,
  value: string,
): Promise<void> {
  select.value = value;
  select.dispatchEvent(new Event('change'));
  await fixture.whenStable();
  fixture.detectChanges();
}

describe('ChatSection — the Story’s Clock', () => {
  beforeEach(() => {
    sent.length = 0;
    failNext = false;
  });

  it('offers v4’s two options and reads a null column as Real time', async () => {
    const fixture = await render();
    const select = clockSelect(fixture);
    expect(Array.from(select.options).map((o) => [o.value, o.textContent?.trim()])).toEqual([
      ['realtime', 'Real time'],
      ['narrative', 'Story time'],
    ]);
    expect(select.value).toBe('realtime');
  });

  it('carries v4’s helper copy for both clocks, byte for byte', async () => {
    const fixture = await render();
    expect(clockHelp(fixture)).toBe(
      'Memories are filed by the calendar on the wall — “last Tuesday” means the actual Tuesday. ' +
        'Switch to Story time if this chat runs on a fictional clock.',
    );

    fixture.componentInstance.chatState.set(state({ timelineMode: 'narrative' }));
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(clockHelp(fixture)).toBe(
      'The tale keeps its own hours: the Commonplace Book files memories by the story’s reckoning ' +
        '— the third night at sea, the eve of the coronation — rather than the calendar on the wall.',
    );
  });

  it('writes {chat:{timelineMode}} , keeps the choice, and asks the parent to refetch', async () => {
    const fixture = await render();
    await choose(fixture, clockSelect(fixture), 'narrative');

    expect(sent).toEqual([
      { type: 'chatUpdate', chatId: 'chat-1', chat: { timelineMode: 'narrative' } },
    ]);
    expect(fixture.componentInstance.refetched).toBe(1);
    // The chat GET never returns the column (a v4 route gap ported faithfully),
    // so the select would snap back without the local adoption.
    expect(clockSelect(fixture).value).toBe('narrative');
    expect(fixture.nativeElement.textContent).toContain('The story now keeps its own hours');

    await choose(fixture, clockSelect(fixture), 'realtime');
    expect(sent[1]).toEqual({
      type: 'chatUpdate',
      chatId: 'chat-1',
      chat: { timelineMode: 'realtime' },
    });
    expect(clockSelect(fixture).value).toBe('realtime');
    expect(fixture.nativeElement.textContent).toContain(
      'The story is back on the clock on the wall',
    );
  });

  it('keeps the previous clock and shows the error when the write fails', async () => {
    const fixture = await render();
    failNext = true;
    await choose(fixture, clockSelect(fixture), 'narrative');

    expect(fixture.componentInstance.refetched).toBe(0);
    expect(clockSelect(fixture).value).toBe('realtime');
    expect(fixture.nativeElement.textContent).toContain('the clock is stuck');
  });
});

describe('ChatSection — the other per-chat controls', () => {
  beforeEach(() => {
    sent.length = 0;
    failNext = false;
  });

  it('writes the roleplay template, the image profile and the announce tri-state through the chat bag', async () => {
    const fixture = await render();
    const selects = Array.from(
      fixture.nativeElement.querySelectorAll('select'),
    ) as HTMLSelectElement[];
    // Order follows v4's panel: template, clock, image provider, announce.
    const [template, , imageProvider, announce] = selects;

    await choose(fixture, template, '');
    await choose(fixture, imageProvider, '');
    await choose(fixture, announce, 'disabled');

    expect(sent).toEqual([
      { type: 'chatUpdate', chatId: 'chat-1', chat: { roleplayTemplateId: null } },
      { type: 'chatUpdate', chatId: 'chat-1', chat: { imageProfileId: null } },
      { type: 'chatUpdate', chatId: 'chat-1', chat: { alertCharactersOfLanternImages: false } },
    ]);
    expect(fixture.nativeElement.textContent).toContain(
      'Lantern images will stay silent for this chat',
    );
  });

  it('shows the project entry only when the chat has one', async () => {
    const fixture = await render();
    expect(fixture.nativeElement.textContent).not.toContain('Project:');

    fixture.componentInstance.chatState.set(
      state({ projectId: 'p1', projectName: 'The Long Voyage' }),
    );
    fixture.detectChanges();
    const link = fixture.nativeElement.querySelector('a.qt-tool-palette-button') as HTMLAnchorElement;
    expect(link.textContent).toContain('Project: The Long Voyage');
    expect(link.getAttribute('href')).toBe('/prospero/p1');
  });
});
