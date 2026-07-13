import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { ChatComposer, type ComposerSend } from './chat-composer';

function jsonResponse(body: unknown, ok = true, status = 200): Response {
  return { ok, status, json: async () => body } as unknown as Response;
}

function render(): ComponentFixture<ChatComposer> {
  TestBed.configureTestingModule({ imports: [ChatComposer] });
  const fixture = TestBed.createComponent(ChatComposer);
  fixture.componentRef.setInput('chatId', 'chat-1');
  fixture.detectChanges();
  return fixture;
}

async function settle(fixture: ComponentFixture<ChatComposer>): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

function pickFile(fixture: ComponentFixture<ChatComposer>, name = 'pic.png'): void {
  const input = fixture.nativeElement.querySelector('input[type="file"]') as HTMLInputElement;
  const file = new File([new Uint8Array([1, 2, 3])], name, { type: 'image/png' });
  Object.defineProperty(input, 'files', { value: [file], configurable: true });
  input.dispatchEvent(new Event('change'));
}

const UPLOADED = {
  file: { id: 'f-1', filename: 'pic.png', filepath: 'chat/pic.png', mimeType: 'image/png', url: '/api/v1/files/f-1' },
};

describe('ChatComposer — attach affordance', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    TestBed.resetTestingModule();
  });

  it('uploads a picked file and shows a chip', async () => {
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(UPLOADED));
    vi.stubGlobal('fetch', fetchMock);
    const fixture = render();
    pickFile(fixture);
    await settle(fixture);
    expect(fetchMock).toHaveBeenCalledWith(
      '/api/v1/chats/chat-1/files',
      expect.objectContaining({ method: 'POST' }),
    );
    expect(fixture.nativeElement.querySelector('.qt-chat-attachment-chip')).not.toBeNull();
    expect(fixture.nativeElement.textContent).toContain('pic.png');
  });

  it('sends the content and attached file ids, then clears', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(UPLOADED)));
    const fixture = render();
    let sent: ComposerSend | undefined;
    fixture.componentInstance.send.subscribe((e) => (sent = e));
    pickFile(fixture);
    await settle(fixture);
    const textarea = fixture.nativeElement.querySelector('textarea') as HTMLTextAreaElement;
    textarea.value = 'look at this';
    textarea.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    fixture.nativeElement.querySelector('form').dispatchEvent(new Event('submit'));
    expect(sent).toEqual({ content: 'look at this', fileIds: ['f-1'] });
    // Chips + text cleared after send.
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('.qt-chat-attachment-chip')).toBeNull();
  });

  it('enables send with an attachment and no text', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(UPLOADED)));
    const fixture = render();
    const sendBtn = () => fixture.nativeElement.querySelector('.qt-chat-composer-send') as HTMLButtonElement;
    expect(sendBtn().disabled).toBe(true);
    pickFile(fixture);
    await settle(fixture);
    expect(sendBtn().disabled).toBe(false);
  });

  it('removes an attached chip', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(UPLOADED)));
    const fixture = render();
    pickFile(fixture);
    await settle(fixture);
    (fixture.nativeElement.querySelector('.qt-chat-attachment-chip-remove') as HTMLButtonElement).click();
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('.qt-chat-attachment-chip')).toBeNull();
  });

  it('opens the conflict dialog on a duplicate and resolves with a resolution', async () => {
    const dup = {
      duplicate: true,
      conflictType: 'filename',
      existingFile: { id: 'old', filename: 'pic.png', size: 100, createdAt: '2026-01-01', sha256: 'a' },
      newFile: { filename: 'pic.png', size: 120, sha256: 'b' },
    };
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse(dup))
      .mockResolvedValueOnce(jsonResponse(UPLOADED));
    vi.stubGlobal('fetch', fetchMock);
    const fixture = render();
    pickFile(fixture);
    await settle(fixture);
    expect(fixture.nativeElement.querySelector('qt-file-conflict-dialog')).not.toBeNull();
    (fixture.nativeElement.querySelector('.qt-dialog-footer button:last-child') as HTMLButtonElement).click();
    await settle(fixture);
    // The resolution re-uploaded with the conflicting file id.
    const secondCall = fetchMock.mock.calls[1];
    const form = secondCall[1].body as FormData;
    expect(form.get('resolution')).toBe('replace');
    expect(form.get('conflictingFileId')).toBe('old');
    expect(fixture.nativeElement.querySelector('qt-file-conflict-dialog')).toBeNull();
  });
});
