import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { CoreResponse } from '../core/core-contract';
import { ImageModal } from './image-modal';

interface DispatchLog {
  requests: { type: string; [k: string]: unknown }[];
}

function stubClient(log: DispatchLog, response?: CoreResponse): Partial<CoreClient> {
  return {
    dispatch: (async (req: { type: string; [k: string]: unknown }) => {
      log.requests.push(req);
      return response ?? { type: 'ack', data: {} };
    }) as CoreClient['dispatch'],
  };
}

function render(
  inputs: Record<string, unknown>,
  client: Partial<CoreClient>,
): ComponentFixture<ImageModal> {
  TestBed.configureTestingModule({
    imports: [ImageModal],
    providers: [{ provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(ImageModal);
  fixture.componentRef.setInput('src', '/api/v1/files/f-1');
  fixture.componentRef.setInput('filename', 'portrait.webp');
  for (const [k, v] of Object.entries(inputs)) fixture.componentRef.setInput(k, v);
  fixture.detectChanges();
  return fixture;
}

async function settle(fixture: ComponentFixture<ImageModal>): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

describe('ImageModal', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('renders the image src and filename caption', () => {
    const fixture = render({}, stubClient({ requests: [] }));
    const img = fixture.nativeElement.querySelector('img') as HTMLImageElement;
    expect(img.getAttribute('src')).toBe('/api/v1/files/f-1');
    expect(fixture.nativeElement.textContent).toContain('portrait.webp');
  });

  it('hides save + delete controls when no fileId is provided', () => {
    const fixture = render({ characterId: 'c-1' }, stubClient({ requests: [] }));
    expect(fixture.nativeElement.querySelector('[title="Delete image"]')).toBeNull();
    // Only download/copy/close chrome — no save-to-gallery user icons.
    expect(fixture.nativeElement.querySelector("[title^=\"Save to\"]")).toBeNull();
  });

  it('shows the save-to-character control and dispatches characterPhotoSaveById', async () => {
    const log: DispatchLog = { requests: [] };
    const fixture = render(
      { fileId: 'f-1', characterId: 'c-1', characterName: 'Lorian' },
      stubClient(log),
    );
    const btn = fixture.nativeElement.querySelector(
      "[title=\"Save to Lorian's photo album\"]",
    ) as HTMLButtonElement;
    expect(btn).not.toBeNull();
    btn.click();
    await settle(fixture);
    expect(log.requests[0]).toEqual({ type: 'characterPhotoSaveById', characterId: 'c-1', fileId: 'f-1' });
  });

  it('deletes via chatFileDelete then emits deleted + close', async () => {
    const log: DispatchLog = { requests: [] };
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const fixture = render({ fileId: 'f-1' }, stubClient(log));
    const deleted = vi.fn();
    const closed = vi.fn();
    fixture.componentInstance.deleted.subscribe(deleted);
    fixture.componentInstance.close.subscribe(closed);
    (fixture.nativeElement.querySelector('[title="Delete image"]') as HTMLButtonElement).click();
    await settle(fixture);
    expect(log.requests[0]).toEqual({ type: 'chatFileDelete', fileId: 'f-1' });
    expect(deleted).toHaveBeenCalled();
    expect(closed).toHaveBeenCalled();
  });

  it('does not delete when the confirm is declined', async () => {
    const log: DispatchLog = { requests: [] };
    vi.spyOn(window, 'confirm').mockReturnValue(false);
    const fixture = render({ fileId: 'f-1' }, stubClient(log));
    (fixture.nativeElement.querySelector('[title="Delete image"]') as HTMLButtonElement).click();
    await settle(fixture);
    expect(log.requests).toHaveLength(0);
  });

  it('emits close from the backdrop and the close button', () => {
    const fixture = render({}, stubClient({ requests: [] }));
    const closed = vi.fn();
    fixture.componentInstance.close.subscribe(closed);
    (fixture.nativeElement.querySelector('[title="Close"]') as HTMLButtonElement).click();
    expect(closed).toHaveBeenCalledTimes(1);
    (fixture.nativeElement.querySelector('[role="dialog"]') as HTMLElement).click();
    expect(closed).toHaveBeenCalledTimes(2);
  });
});
