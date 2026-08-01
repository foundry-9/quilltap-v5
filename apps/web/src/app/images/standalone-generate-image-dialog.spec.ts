import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { ParticipantDetail } from '../core/core-contract';
import type { GeneratedImage } from './generate-image-dialog';
import { StandaloneGenerateImageDialog } from './standalone-generate-image-dialog';
import { ToastService } from '../ui/toast.service';

interface Recorder {
  calls: Record<string, unknown>[];
}

function image(over: Partial<GeneratedImage> = {}): GeneratedImage {
  return { id: 'img-1', filename: 'one.png', filepath: 'tool/one.png', mimeType: 'image/png', ...over };
}

function participant(over: Partial<ParticipantDetail>): ParticipantDetail {
  return {
    id: 'p1',
    type: 'CHARACTER',
    displayOrder: 0,
    isActive: true,
    controlledBy: 'llm',
    status: 'active',
    character: null,
    connectionProfile: null,
    imageProfile: null,
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    ...over,
  } as ParticipantDetail;
}

function withCharacter(id: string, name: string, controlledBy: 'llm' | 'user'): ParticipantDetail {
  return participant({
    id: `part-${id}`,
    controlledBy,
    character: { id, name } as ParticipantDetail['character'],
  });
}

function stubClient(opts: {
  characters?: { id: string; name: string }[];
  generate?: unknown;
  generateError?: string;
  recorder?: Recorder;
}): Partial<CoreClient> {
  return {
    dispatchData: (async (req: Record<string, unknown>) => {
      opts.recorder?.calls.push(req);
      if (req['type'] === 'characterList') return { characters: opts.characters ?? [] };
      return { profiles: [] };
    }) as unknown as CoreClient['dispatchData'],
    dispatch: (async (req: Record<string, unknown>) => {
      opts.recorder?.calls.push(req);
      if (opts.generateError) return { type: 'error', data: { message: opts.generateError } };
      return { type: 'ok', data: opts.generate ?? { success: true, data: [image()] } };
    }) as unknown as CoreClient['dispatch'],
  };
}

async function render(
  client: Partial<CoreClient>,
  participants: ParticipantDetail[] = [],
): Promise<ComponentFixture<StandaloneGenerateImageDialog>> {
  TestBed.configureTestingModule({
    imports: [StandaloneGenerateImageDialog],
    providers: [{ provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(StandaloneGenerateImageDialog);
  fixture.componentRef.setInput('chatId', 'chat-7');
  fixture.componentRef.setInput('participants', participants);
  fixture.detectChanges();
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

type Inner = StandaloneGenerateImageDialog & {
  selectedProfileId: { set(v: string | null): void };
  prompt: { (): string; set(v: string): void };
  imageCount: { set(v: number): void };
  error: { (): string | null };
  onGenerate(): Promise<void>;
};

function inner(fixture: ComponentFixture<StandaloneGenerateImageDialog>): Inner {
  return fixture.componentInstance as unknown as Inner;
}

/** The message of the last toast this render raised. */
function lastToast(): string | undefined {
  const list = TestBed.inject(ToastService).toasts();
  return list.at(-1)?.message;
}

describe('StandaloneGenerateImageDialog (v4 components/chat/StandaloneGenerateImageDialog.tsx)', () => {
  it('sends prompt + chatId + count (v4 :83-87)', async () => {
    const rec: Recorder = { calls: [] };
    const fixture = await render(stubClient({ recorder: rec }));
    inner(fixture).selectedProfileId.set('p1');
    inner(fixture).prompt.set('a cat');
    inner(fixture).imageCount.set(4);
    await inner(fixture).onGenerate();
    expect(rec.calls.find((c) => c['type'] === 'imageProfileGenerate')).toEqual({
      type: 'imageProfileGenerate',
      imageProfileId: 'p1',
      prompt: 'a cat',
      chatId: 'chat-7',
      count: 4,
    });
  });

  it('sends the prompt UNTRIMMED, unlike the /generate-image screen (v4 :84 vs :116)', async () => {
    const rec: Recorder = { calls: [] };
    const fixture = await render(stubClient({ recorder: rec }));
    inner(fixture).selectedProfileId.set('p1');
    inner(fixture).prompt.set('  a cat  ');
    await inner(fixture).onGenerate();
    const call = rec.calls.find((c) => c['type'] === 'imageProfileGenerate')!;
    expect(call['prompt']).toBe('  a cat  ');
  });

  it('emits the images and the server expandedPrompt for the tool result (v4 :102-109)', async () => {
    const fixture = await render(
      stubClient({
        generate: {
          success: true,
          data: [image({ id: 'a' }), image({ id: 'b' })],
          expandedPrompt: 'a cat, expanded by the server',
        },
      }),
    );
    const emitted: { images: GeneratedImage[]; prompt: string }[] = [];
    fixture.componentInstance.generated.subscribe((e) => emitted.push(e));
    inner(fixture).selectedProfileId.set('p1');
    inner(fixture).prompt.set('a cat');
    await inner(fixture).onGenerate();
    expect(emitted).toHaveLength(1);
    expect(emitted[0].prompt).toBe('a cat, expanded by the server');
    expect(emitted[0].images.map((i) => i.id)).toEqual(['a', 'b']);
  });

  it('falls back to the typed prompt when the server sends no expandedPrompt (v4 :102)', async () => {
    const fixture = await render(stubClient({ generate: { success: true, data: [image()] } }));
    const emitted: { prompt: string }[] = [];
    fixture.componentInstance.generated.subscribe((e) => emitted.push(e));
    inner(fixture).selectedProfileId.set('p1');
    inner(fixture).prompt.set('a cat');
    await inner(fixture).onGenerate();
    expect(emitted[0].prompt).toBe('a cat');
  });

  it('clears the prompt and closes on success (v4 :103,:110)', async () => {
    const fixture = await render(stubClient({ generate: { success: true, data: [image()] } }));
    let closed = 0;
    fixture.componentInstance.close.subscribe(() => closed++);
    inner(fixture).selectedProfileId.set('p1');
    inner(fixture).prompt.set('a cat');
    await inner(fixture).onGenerate();
    expect(inner(fixture).prompt()).toBe('');
    expect(closed).toBe(1);
    // v4 `:100` — the success toast, count included.
    expect(lastToast()).toBe('Generated 1 image(s) - attached to next message');
  });

  it('requires success:true — a truthy payload without it is the empty arm (v4 :99)', async () => {
    const fixture = await render(stubClient({ generate: { data: [image()] } }));
    inner(fixture).selectedProfileId.set('p1');
    inner(fixture).prompt.set('a cat');
    await inner(fixture).onGenerate();
    expect(lastToast()).toBe('No images generated');
  });

  it('uses v4\'s dialog wording — "No images generated", not the screen\'s (v4 :112)', async () => {
    const fixture = await render(stubClient({ generate: { success: true, data: [] } }));
    inner(fixture).selectedProfileId.set('p1');
    inner(fixture).prompt.set('a cat');
    await inner(fixture).onGenerate();
    expect(lastToast()).toBe('No images generated');
    expect(lastToast()).not.toBe('No images were generated');
  });

  it('checks the prompt BEFORE the profile — v4 reverses the screen (v4 :65,:70)', async () => {
    const fixture = await render(stubClient({}));
    // Neither set: the prompt complaint wins here, the profile one on the page.
    await inner(fixture).onGenerate();
    expect(lastToast()).toBe('Please enter a prompt');
  });

  it('complains about the profile once a prompt is present (v4 :70-73)', async () => {
    const fixture = await render(stubClient({}));
    inner(fixture).prompt.set('a cat');
    await inner(fixture).onGenerate();
    expect(lastToast()).toBe('Please select an image profile');
  });

  it('surfaces a server error (v4 :90-95)', async () => {
    const fixture = await render(stubClient({ generateError: 'the kiln is cold' }));
    inner(fixture).selectedProfileId.set('p1');
    inner(fixture).prompt.set('a cat');
    await inner(fixture).onGenerate();
    expect(lastToast()).toBe('the kiln is cold');
  });

  it('offers the user character as a {{me}} quick insert labelled by name (v4 :172-181)', async () => {
    const fixture = await render(stubClient({}), [withCharacter('u1', 'Riya', 'user')]);
    const text = fixture.nativeElement.textContent;
    expect(text).toContain('Riya');
    expect(text).toContain('{{me}}');
  });

  it('offers each non-user character as its own named insert (v4 :183-196)', async () => {
    const fixture = await render(stubClient({}), [
      withCharacter('c1', 'Lorian', 'llm'),
      withCharacter('u1', 'Riya', 'user'),
    ]);
    const text = fixture.nativeElement.textContent;
    expect(text).toContain('{{Lorian}}');
    // The user character is offered as {{me}}, never as {{Riya}} (v4 :130-131).
    expect(text).not.toContain('{{Riya}}');
  });

  it('offers exactly the counts 1-4 (v4 :245-248)', async () => {
    const fixture = await render(stubClient({}));
    const opts = Array.from(
      fixture.nativeElement.querySelectorAll('#standalone-image-count option'),
    ) as HTMLOptionElement[];
    expect(opts.map((o) => o.value)).toEqual(['1', '2', '3', '4']);
  });

  it('keeps Generate disabled until both a profile and a prompt are set (v4 :261)', async () => {
    const fixture = await render(stubClient({}));
    const button = (
      Array.from(fixture.nativeElement.querySelectorAll('button')) as HTMLButtonElement[]
    ).find((b) => b.textContent!.includes('Generate Image'))!;
    expect(button.disabled).toBe(true);
    inner(fixture).selectedProfileId.set('p1');
    inner(fixture).prompt.set('a cat');
    fixture.detectChanges();
    expect(button.disabled).toBe(false);
  });
});
