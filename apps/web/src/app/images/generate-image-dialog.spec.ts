import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { CoreResponse, ParticipantDetail } from '../core/core-contract';
import { GenerateImageDialog } from './generate-image-dialog';

function participant(name: string, over: Partial<ParticipantDetail> = {}): ParticipantDetail {
  return {
    id: 'p-' + name,
    type: 'CHARACTER',
    displayOrder: 0,
    isActive: true,
    controlledBy: 'llm',
    status: 'active',
    character: { id: 'c-' + name, name, title: null, avatarUrl: null, defaultImageId: null, defaultImage: null },
    connectionProfile: null,
    imageProfile: null,
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    ...over,
  };
}

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
  client: Partial<CoreClient>,
  inputs: Record<string, unknown> = {},
): ComponentFixture<GenerateImageDialog> {
  TestBed.configureTestingModule({
    imports: [GenerateImageDialog],
    providers: [{ provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(GenerateImageDialog);
  fixture.componentRef.setInput('chatId', 'chat-1');
  fixture.componentRef.setInput('imageProfileId', 'ip-1');
  fixture.componentRef.setInput('participants', [participant('Lorian'), participant('Riya')]);
  for (const [k, v] of Object.entries(inputs)) fixture.componentRef.setInput(k, v);
  fixture.detectChanges();
  return fixture;
}

function typePrompt(fixture: ComponentFixture<GenerateImageDialog>, value: string): void {
  const ta = fixture.nativeElement.querySelector('#generate-prompt') as HTMLTextAreaElement;
  ta.value = value;
  ta.dispatchEvent(new Event('input'));
  fixture.detectChanges();
}

function generateButton(fixture: ComponentFixture<GenerateImageDialog>): HTMLButtonElement {
  return fixture.nativeElement.querySelector('.qt-dialog-footer button:last-child');
}

async function settle(fixture: ComponentFixture<GenerateImageDialog>): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

describe('GenerateImageDialog', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('degrades loudly when no image profile is configured', () => {
    const fixture = render(stubClient({ requests: [] }), { imageProfileId: null });
    expect(fixture.nativeElement.textContent).toContain('No image profile is configured');
    expect(generateButton(fixture).disabled).toBe(true);
  });

  it('inserts a {{Character}} placeholder from the quick buttons', () => {
    const fixture = render(stubClient({ requests: [] }));
    const lorianBtn = Array.from(fixture.nativeElement.querySelectorAll('button')).find(
      (b) => (b as HTMLButtonElement).textContent?.trim() === 'Lorian',
    ) as HTMLButtonElement;
    lorianBtn.click();
    fixture.detectChanges();
    expect((fixture.nativeElement.querySelector('#generate-prompt') as HTMLTextAreaElement).value).toContain(
      '{{Lorian}}',
    );
  });

  it('generates via imageProfileGenerate and emits the images + expanded prompt', async () => {
    const log: DispatchLog = { requests: [] };
    const response: CoreResponse = {
      type: 'ack',
      data: {
        success: true,
        data: [{ id: 'img-1', filename: 'a.webp', filepath: '/x/a.webp', mimeType: 'image/webp' }],
        expandedPrompt: 'a grand portrait of Lorian',
      },
    } as unknown as CoreResponse;
    const fixture = render(stubClient(log, response));
    const generated = vi.fn();
    fixture.componentInstance.generated.subscribe(generated);
    typePrompt(fixture, 'portrait of {{Lorian}}');
    generateButton(fixture).click();
    await settle(fixture);
    expect(log.requests[0]).toEqual({
      type: 'imageProfileGenerate',
      imageProfileId: 'ip-1',
      prompt: 'portrait of {{Lorian}}',
      chatId: 'chat-1',
      count: 1,
    });
    expect(generated).toHaveBeenCalledWith({
      images: [{ id: 'img-1', filename: 'a.webp', filepath: '/x/a.webp', mimeType: 'image/webp' }],
      prompt: 'a grand portrait of Lorian',
    });
  });

  it('surfaces a refusal/error message loudly', async () => {
    const log: DispatchLog = { requests: [] };
    const fixture = render(
      stubClient(log, {
        type: 'error',
        data: { kind: 'not_available', message: 'Image generation is not available yet.' },
      } as CoreResponse),
    );
    typePrompt(fixture, 'a picture');
    generateButton(fixture).click();
    await settle(fixture);
    expect(fixture.nativeElement.textContent).toContain('Image generation is not available yet.');
  });
});
