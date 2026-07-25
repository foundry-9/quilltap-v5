import { describe, expect, it, vi } from 'vitest';

import type { CoreClient } from '../../core/core-client';
import {
  STAFF_OPTIONS,
  fetchAnnouncementProfiles,
  fetchMailbox,
  mailboxKeys,
  postAnnouncement,
  previewAnnouncement,
  sendMail,
} from './post-office.api';

function coreStub(data: unknown): { core: CoreClient; dispatchData: ReturnType<typeof vi.fn> } {
  const dispatchData = vi.fn(async () => data as Record<string, unknown>);
  return { core: { dispatchData } as unknown as CoreClient, dispatchData };
}

describe('post-office.api — the §1 requests', () => {
  it('chatAnnouncementPost sends exactly the three frozen fields', async () => {
    const { core, dispatchData } = coreStub({ success: true });
    await postAnnouncement(core, {
      chatId: 'chat-1',
      contentMarkdown: 'The airship departs at dawn.',
      sender: { kind: 'staff', staffId: 'host' },
    });
    expect(dispatchData).toHaveBeenCalledWith({
      type: 'chatAnnouncementPost',
      chatId: 'chat-1',
      contentMarkdown: 'The airship departs at dawn.',
      sender: { kind: 'staff', staffId: 'host' },
    });
  });

  it('carries the character and custom sender arms verbatim', async () => {
    const { core, dispatchData } = coreStub({ success: true });
    await postAnnouncement(core, {
      chatId: 'chat-1',
      contentMarkdown: 'x',
      sender: { kind: 'character', characterId: 'char-7' },
    });
    expect(dispatchData.mock.calls[0]![0]).toMatchObject({
      sender: { kind: 'character', characterId: 'char-7' },
    });
    await postAnnouncement(core, {
      chatId: 'chat-1',
      contentMarkdown: 'x',
      sender: { kind: 'custom', displayName: 'A Distant Voice' },
    });
    expect(dispatchData.mock.calls[1]![0]).toMatchObject({
      sender: { kind: 'custom', displayName: 'A Distant Voice' },
    });
  });

  it('chatAnnouncementPreview OMITS systemPromptId when empty (v4 `|| undefined`)', async () => {
    const { core, dispatchData } = coreStub({ proposedMarkdown: 'ahoy' });
    await previewAnnouncement(core, {
      chatId: 'chat-1',
      seedMarkdown: 'seed',
      characterId: 'char-7',
      connectionProfileId: 'prof-1',
      systemPromptId: null,
    });
    expect(dispatchData).toHaveBeenCalledWith({
      type: 'chatAnnouncementPreview',
      chatId: 'chat-1',
      seedMarkdown: 'seed',
      characterId: 'char-7',
      connectionProfileId: 'prof-1',
    });
  });

  it('chatAnnouncementPreview sends systemPromptId when set, and trims the rewrite', async () => {
    const { core, dispatchData } = coreStub({ proposedMarkdown: '  ahoy, then.  ' });
    const proposed = await previewAnnouncement(core, {
      chatId: 'chat-1',
      seedMarkdown: 'seed',
      characterId: 'char-7',
      connectionProfileId: 'prof-1',
      systemPromptId: 'sp-2',
    });
    expect(dispatchData.mock.calls[0]![0]).toMatchObject({ systemPromptId: 'sp-2' });
    expect(proposed).toBe('ahoy, then.');
  });

  it('a missing proposedMarkdown reads as the empty string (the caller’s retry arm)', async () => {
    const { core } = coreStub({ success: true });
    expect(
      await previewAnnouncement(core, {
        chatId: 'c',
        seedMarkdown: 's',
        characterId: 'ch',
        connectionProfileId: 'p',
      }),
    ).toBe('');
  });

  it('chatSendMail sends the five frozen fields, null reply included', async () => {
    const { core, dispatchData } = coreStub({ success: true, path: 'Mail/2026-…md' });
    const path = await sendMail(core, {
      chatId: 'chat-1',
      fromCharacterId: 'char-a',
      toCharacterId: 'char-b',
      bodyMarkdown: 'Dear friend,',
      inReplyToPath: null,
    });
    expect(dispatchData).toHaveBeenCalledWith({
      type: 'chatSendMail',
      chatId: 'chat-1',
      fromCharacterId: 'char-a',
      toCharacterId: 'char-b',
      bodyMarkdown: 'Dear friend,',
      inReplyToPath: null,
    });
    expect(path).toBe('Mail/2026-…md');
  });

  it('chatMailboxList returns the letters, tolerating an envelope without them', async () => {
    const letters = [{ path: 'Mail/a.md', from: 'Bram', sentAt: '2026-02-01T00:00:00.000Z' }];
    expect(await fetchMailbox(coreStub({ letters }).core, 'chat-1', 'char-a')).toEqual(letters);
    expect(await fetchMailbox(coreStub({}).core, 'chat-1', 'char-a')).toEqual([]);
  });

  it('mailbox keys are per chat AND per character (v4 refetches when From changes)', () => {
    expect(mailboxKeys.byCharacter('chat-1', 'char-a')).toEqual(['mailbox', 'chat-1', 'char-a']);
    expect(mailboxKeys.byCharacter('chat-1', 'char-b')).not.toEqual(
      mailboxKeys.byCharacter('chat-1', 'char-a'),
    );
  });
});

describe('post-office.api — the profile projection (v4 InsertAnnouncementDialog:121-132)', () => {
  it('maps five fields with v4’s coercions and its empty-string fallbacks', async () => {
    const { core } = coreStub({
      profiles: [
        { id: 'p1', name: 'Cheap', provider: 'openai', modelName: 'gpt-4o-mini', isDefault: true },
        { id: 'p2' },
      ],
    });
    expect(await fetchAnnouncementProfiles(core)).toEqual([
      { id: 'p1', name: 'Cheap', provider: 'openai', modelName: 'gpt-4o-mini', isDefault: true },
      { id: 'p2', name: '', provider: '', modelName: '', isDefault: false },
    ]);
  });

  it('tolerates an envelope with no profiles member', async () => {
    expect(await fetchAnnouncementProfiles(coreStub({}).core)).toEqual([]);
  });
});

describe('post-office.api — the staff roster', () => {
  it('is v4’s ten, in v4’s DISPLAY order with v4’s labels', () => {
    expect(STAFF_OPTIONS.map((s) => s.id)).toEqual([
      'host',
      'librarian',
      'lantern',
      'aurora',
      'concierge',
      'prospero',
      'commonplaceBook',
      'ariel',
      'suparna',
      'pascal',
    ]);
    // The diacritic is load-bearing — it is the Post Office's own courier.
    expect(STAFF_OPTIONS.find((s) => s.id === 'suparna')?.label).toBe('Suparṇā');
    expect(STAFF_OPTIONS.find((s) => s.id === 'pascal')?.label).toBe('Pascal the Croupier');
  });
});
