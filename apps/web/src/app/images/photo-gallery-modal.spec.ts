import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { ChatFileDto } from '../core/core-contract';
import { PhotoGalleryModal } from './photo-gallery-modal';

function file(over: Partial<ChatFileDto>): ChatFileDto {
  return {
    id: 'f-1',
    filename: 'shot.png',
    filepath: 'chat/shot.png',
    mimeType: 'image/png',
    ...over,
  };
}

function stubClient(files: ChatFileDto[]): Partial<CoreClient> {
  return {
    dispatchData: (async (req: { type: string }) => {
      if (req.type === 'chatFilesList') return { files };
      return {};
    }) as CoreClient['dispatchData'],
    dispatch: (async () => ({ type: 'ack', data: {} })) as CoreClient['dispatch'],
  };
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<PhotoGalleryModal>> {
  TestBed.configureTestingModule({
    imports: [PhotoGalleryModal],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(PhotoGalleryModal);
  fixture.componentRef.setInput('chatId', 'chat-1');
  fixture.detectChanges();
  for (let i = 0; i < 5; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

describe('PhotoGalleryModal (chat mode)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('renders a thumbnail per image file off the id-keyed thumbnail route', async () => {
    const fixture = await render(stubClient([file({ id: 'f-1' }), file({ id: 'f-2' })]));
    const imgs = fixture.nativeElement.querySelectorAll('.qt-chat-attachment-image');
    expect(imgs.length).toBe(2);
    expect((imgs[0] as HTMLImageElement).getAttribute('src')).toBe(
      '/api/v1/files/f-1?action=thumbnail&size=120',
    );
  });

  it('filters out non-image files', async () => {
    const fixture = await render(
      stubClient([file({ id: 'i', mimeType: 'image/webp' }), file({ id: 'd', mimeType: 'application/pdf' })]),
    );
    expect(fixture.nativeElement.querySelectorAll('.qt-chat-attachment-image').length).toBe(1);
  });

  it('shows the empty state when the chat has no images', async () => {
    const fixture = await render(stubClient([]));
    expect(fixture.nativeElement.textContent).toContain('No photos in this chat');
  });

  it('opens the lightbox when a thumbnail is clicked', async () => {
    const fixture = await render(stubClient([file({ id: 'f-1' })]));
    expect(fixture.nativeElement.querySelector('qt-image-modal')).toBeNull();
    (fixture.nativeElement.querySelector('.qt-chat-attachment-button') as HTMLButtonElement).click();
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('qt-image-modal')).not.toBeNull();
  });
});
