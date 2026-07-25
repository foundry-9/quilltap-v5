import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type { CharacterListItem } from '../../core/core-contract';
import { RichEditor } from '../../editor/rich-editor';
import { ComposeMailDialog, type ComposeMailParticipant } from './compose-mail-dialog';

function character(over: Partial<CharacterListItem> = {}): CharacterListItem {
  return {
    id: 'char-1',
    name: 'Bram',
    title: null,
    description: null,
    defaultImageId: null,
    defaultImage: null,
    isFavorite: false,
    controlledBy: 'llm',
    canBeCarina: false,
    defaultConnectionProfileId: null,
    defaultPartnerId: null,
    defaultPartnerName: null,
    defaultTimestampConfig: null,
    defaultScenarioId: null,
    defaultSystemPromptId: null,
    defaultImageProfileId: null,
    npc: false,
    createdAt: '2026-02-01T00:00:00.000Z',
    tags: [],
    updatedAt: '2026-02-01T00:00:00.000Z',
    systemPrompts: [],
    scenarios: [],
    _count: { chats: 0 },
    ...over,
  };
}

interface Stub {
  calls: Record<string, unknown>[];
  client: Partial<CoreClient>;
}

function stub(opts: {
  characters?: CharacterListItem[];
  letters?: Array<{ path: string; from: string; sentAt: string }> | Error;
  send?: Record<string, unknown> | Error;
}): Stub {
  const calls: Record<string, unknown>[] = [];
  const dispatchData = vi.fn(async (req: Record<string, unknown>) => {
    calls.push(req);
    switch (req['type']) {
      case 'characterList':
        return { characters: opts.characters ?? [] };
      case 'chatMailboxList':
        if (opts.letters instanceof Error) throw opts.letters;
        return { letters: opts.letters ?? [] };
      case 'chatSendMail':
        if (opts.send instanceof Error) throw opts.send;
        return opts.send ?? { success: true, path: 'Mail/out.md' };
      default:
        return {};
    }
  });
  return { calls, client: { dispatchData: dispatchData as unknown as CoreClient['dispatchData'] } };
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 8; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function mount(
  s: Stub,
  participants: ComposeMailParticipant[],
): Promise<ComponentFixture<ComposeMailDialog>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [ComposeMailDialog],
    providers: [
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: s.client },
    ],
  });
  const fixture = TestBed.createComponent(ComposeMailDialog);
  fixture.componentRef.setInput('chatId', 'chat-1');
  fixture.componentRef.setInput('participants', participants);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

/** See the announcement spec: the editor's emit is deferred by a microtask. */
async function setBody(fixture: ComponentFixture<ComposeMailDialog>, value: string): Promise<void> {
  const editor = fixture.debugElement.query(By.directive(RichEditor))
    .componentInstance as RichEditor;
  editor.setMarkdown(value);
  await settle(fixture);
}

function sendButton(fixture: ComponentFixture<unknown>): HTMLButtonElement {
  return fixture.nativeElement.querySelector(
    '[qt-modal-footer] .qt-button-primary',
  ) as HTMLButtonElement;
}

function select(fixture: ComponentFixture<unknown>, id: string): HTMLSelectElement {
  return fixture.nativeElement.querySelector(`#${id}`) as HTMLSelectElement;
}

const CLEO: ComposeMailParticipant = { id: 'c-cleo', name: 'Cleo', controlledBy: 'user' };
const ARIA: ComposeMailParticipant = { id: 'c-aria', name: 'Aria', controlledBy: 'llm' };
const RUE: ComposeMailParticipant = { id: 'c-rue', name: 'Rue', controlledBy: 'user' };

describe('ComposeMailDialog (v4 components/chat/ComposeMailDialog.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('signs only as a character the operator plays IN THIS CHAT', async () => {
    const s = stub({ characters: [character({ id: 'c-aria', name: 'Aria' })] });
    const fixture = await mount(s, [CLEO, ARIA]);
    const from = select(fixture, 'mail-from');
    const options = [...from.querySelectorAll('option')] as HTMLOptionElement[];
    expect(options.map((o) => o.value)).toEqual(['c-cleo']);
    // A lone sender is shown but locked (v4 renders a disabled select).
    expect(from.disabled).toBe(true);
  });

  it('says so plainly when the operator is playing nobody, and cannot send', async () => {
    const s = stub({ characters: [character()] });
    const fixture = await mount(s, [ARIA]);
    expect(fixture.nativeElement.textContent).toContain('there’s no one to sign the letter');
    expect(sendButton(fixture).disabled).toBe(true);
  });

  it('addresses ANY workspace character minus the sender, name-sorted', async () => {
    const s = stub({
      characters: [
        character({ id: 'c-dax', name: 'Dax' }),
        character({ id: 'c-cleo', name: 'Cleo' }),
        character({ id: 'c-aria', name: 'Aria' }),
      ],
    });
    const fixture = await mount(s, [CLEO, ARIA]);
    const options = [...select(fixture, 'mail-to').querySelectorAll('option')] as HTMLOptionElement[];
    // Cleo (the sender) is gone; Dax is offered even though he is not in the scene.
    expect(options.map((o) => o.textContent?.trim())).toEqual(['Aria', 'Dax']);
  });

  it('appends the recipient’s title when they have one (v4 `name — title`)', async () => {
    const s = stub({
      characters: [character({ id: 'c-aria', name: 'Aria', title: 'Sky-captain' })],
    });
    const fixture = await mount(s, [CLEO]);
    const option = select(fixture, 'mail-to').querySelector('option')!;
    expect(option.textContent?.replace(/\s+/g, ' ').trim()).toBe('Aria — Sky-captain');
  });

  it('reads the sender’s postbox, and re-reads it when the sender changes', async () => {
    const s = stub({ characters: [character({ id: 'c-aria', name: 'Aria' })] });
    const fixture = await mount(s, [CLEO, RUE, ARIA]);
    expect(s.calls.filter((c) => c['type'] === 'chatMailboxList')).toMatchObject([
      { chatId: 'chat-1', characterId: 'c-cleo' },
    ]);
    const from = select(fixture, 'mail-from');
    from.value = 'c-rue';
    from.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(s.calls.filter((c) => c['type'] === 'chatMailboxList')).toHaveLength(2);
    expect(s.calls.at(-1)).toMatchObject({ characterId: 'c-rue' });
  });

  it('offers the postbox letters after the default no-reply option', async () => {
    const s = stub({
      characters: [character({ id: 'c-aria' })],
      letters: [{ path: 'Mail/a.md', from: 'Bram', sentAt: '2026-02-03T00:00:00.000Z' }],
    });
    const fixture = await mount(s, [CLEO]);
    const options = [
      ...select(fixture, 'mail-reply').querySelectorAll('option'),
    ] as HTMLOptionElement[];
    expect(options[0]!.textContent?.trim()).toBe('No quoted reply.');
    expect(options[0]!.value).toBe('');
    expect(options[1]!.value).toBe('Mail/a.md');
    expect(options[1]!.textContent).toContain('From Bram');
  });

  it('an unreachable postbox leaves the dropdown usable (v4’s errored query is an empty list)', async () => {
    const s = stub({
      characters: [character({ id: 'c-aria' })],
      letters: new Error('mailbox unavailable'),
    });
    const fixture = await mount(s, [CLEO]);
    const options = select(fixture, 'mail-reply').querySelectorAll('option');
    expect(options).toHaveLength(1);
    await setBody(fixture, 'Still sendable.');
    expect(sendButton(fixture).disabled).toBe(false);
  });

  it('changing the sender drops any quoted reply', async () => {
    const s = stub({
      characters: [character({ id: 'c-aria' })],
      letters: [{ path: 'Mail/a.md', from: 'Bram', sentAt: '2026-02-03T00:00:00.000Z' }],
    });
    const fixture = await mount(s, [CLEO, RUE]);
    const reply = select(fixture, 'mail-reply');
    reply.value = 'Mail/a.md';
    reply.dispatchEvent(new Event('change'));
    await settle(fixture);
    const from = select(fixture, 'mail-from');
    from.value = 'c-rue';
    from.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(select(fixture, 'mail-reply').value).toBe('');
  });

  it('sends the trimmed body with a null reply, then emits posted and close', async () => {
    const s = stub({ characters: [character({ id: 'c-aria', name: 'Aria' })] });
    const fixture = await mount(s, [CLEO]);
    const seen: string[] = [];
    fixture.componentInstance.posted.subscribe(() => seen.push('posted'));
    fixture.componentInstance.close.subscribe(() => seen.push('close'));
    await setBody(fixture, '  Dear Aria,  ');
    sendButton(fixture).click();
    await settle(fixture);
    expect(s.calls.find((c) => c['type'] === 'chatSendMail')).toEqual({
      type: 'chatSendMail',
      chatId: 'chat-1',
      fromCharacterId: 'c-cleo',
      toCharacterId: 'c-aria',
      bodyMarkdown: 'Dear Aria,',
      inReplyToPath: null,
    });
    expect(seen).toEqual(['posted', 'close']);
  });

  it('carries a chosen quoted reply through as the path', async () => {
    const s = stub({
      characters: [character({ id: 'c-aria' })],
      letters: [{ path: 'Mail/a.md', from: 'Bram', sentAt: '2026-02-03T00:00:00.000Z' }],
    });
    const fixture = await mount(s, [CLEO]);
    const reply = select(fixture, 'mail-reply');
    reply.value = 'Mail/a.md';
    reply.dispatchEvent(new Event('change'));
    await setBody(fixture, 'As you asked,');
    sendButton(fixture).click();
    await settle(fixture);
    expect(s.calls.find((c) => c['type'] === 'chatSendMail')).toMatchObject({
      inReplyToPath: 'Mail/a.md',
    });
  });

  it('a refused letter shows the server’s own words and stays open', async () => {
    const s = stub({
      characters: [character({ id: 'c-aria' })],
      send: new Error(
        'You can only post a letter as a character you are playing in this scene.',
      ),
    });
    const fixture = await mount(s, [CLEO]);
    const closed = vi.fn();
    fixture.componentInstance.close.subscribe(closed);
    await setBody(fixture, 'Dear Aria,');
    sendButton(fixture).click();
    await settle(fixture);
    expect(fixture.nativeElement.textContent).toContain('a character you are playing in this scene');
    expect(closed).not.toHaveBeenCalled();
  });

  it('will not send an empty letter', async () => {
    const s = stub({ characters: [character({ id: 'c-aria' })] });
    const fixture = await mount(s, [CLEO]);
    expect(sendButton(fixture).disabled).toBe(true);
    await setBody(fixture, '   ');
    expect(sendButton(fixture).disabled).toBe(true);
  });
});
