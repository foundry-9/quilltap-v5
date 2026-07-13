import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { AlbumOption, CoreResponse, MessageAttachment } from '../core/core-contract';
import { SaveImageDialog } from './save-image-dialog';

const IMG: MessageAttachment = {
  id: 'file-1',
  filename: 'sketch.png',
  filepath: 'uploads/sketch.png',
  mimeType: 'image/png',
};

function albums(): AlbumOption[] {
  return [
    { mountPointId: 'mp-char', name: 'Lorian', kind: 'character', isUserCharacter: false },
    { mountPointId: 'mp-you', name: 'Riya', kind: 'character', isUserCharacter: true },
    { mountPointId: 'mp-gen', name: 'Quilltap General', kind: 'general', isDefault: true },
  ];
}

interface DispatchLog {
  requests: { type: string; [k: string]: unknown }[];
}

function stubClient(log: DispatchLog, albumList: AlbumOption[], saveResponse?: CoreResponse): Partial<CoreClient> {
  return {
    dispatchData: (async (req: { type: string }) => {
      if (req.type === 'chatPhotoAlbums') return { albums: albumList };
      return {};
    }) as CoreClient['dispatchData'],
    dispatch: (async (req: { type: string; [k: string]: unknown }) => {
      log.requests.push(req);
      return saveResponse ?? { type: 'ack', data: { mountPoint: 'mp-gen', relativePath: 'photos/x.webp' } };
    }) as CoreClient['dispatch'],
  };
}

async function render(
  client: Partial<CoreClient>,
  attachments: MessageAttachment[] = [IMG],
): Promise<ComponentFixture<SaveImageDialog>> {
  TestBed.configureTestingModule({
    imports: [SaveImageDialog],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(SaveImageDialog);
  fixture.componentRef.setInput('chatId', 'chat-1');
  fixture.componentRef.setInput('messageId', 'm-1');
  fixture.componentRef.setInput('attachments', attachments);
  fixture.detectChanges();
  for (let i = 0; i < 5; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

describe('SaveImageDialog', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('groups albums into kind optgroups and labels the user character', async () => {
    const fixture = await render(stubClient({ requests: [] }, albums()));
    const groups = fixture.nativeElement.querySelectorAll('optgroup');
    expect(groups.length).toBe(2); // character + general (no project/document-store)
    expect(groups[0].getAttribute('label')).toBe('Character');
    expect(groups[1].getAttribute('label')).toBe('Quilltap General');
    expect(fixture.nativeElement.textContent).toContain('Riya (you)');
  });

  it('shows the empty state when no albums are available', async () => {
    const fixture = await render(stubClient({ requests: [] }, []));
    expect(fixture.nativeElement.textContent).toContain('No photo albums are available');
  });

  it('saves via messageSaveImage with the default album and trimmed caption', async () => {
    const log: DispatchLog = { requests: [] };
    const fixture = await render(stubClient(log, albums()));
    const caption = fixture.nativeElement.querySelector('#save-image-caption') as HTMLInputElement;
    caption.value = '  by the sea  ';
    caption.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    (fixture.nativeElement.querySelector('.qt-dialog-footer button:last-child') as HTMLButtonElement).click();
    await new Promise((r) => setTimeout(r, 0));
    expect(log.requests[0]).toEqual({
      type: 'messageSaveImage',
      chatId: 'chat-1',
      messageId: 'm-1',
      fileId: 'file-1',
      mountPointId: 'mp-gen',
      caption: 'by the sea',
    });
  });

  it('omits the multi-image picker when there is a single attachment', async () => {
    const fixture = await render(stubClient({ requests: [] }, albums()));
    // The picker buttons only render with >1 image (the single preview img has no button).
    expect(fixture.nativeElement.querySelectorAll('.qt-chat-attachment-button').length).toBe(0);
  });

  it('shows a picker button per image when the message carries several', async () => {
    const second: MessageAttachment = { ...IMG, id: 'file-2', filename: 'two.png' };
    const fixture = await render(stubClient({ requests: [] }, albums()), [IMG, second]);
    expect(fixture.nativeElement.querySelectorAll('.qt-chat-attachment-button').length).toBe(2);
  });
});
