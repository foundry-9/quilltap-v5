import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { GenerateImagePage, type GeneratedImage } from './generate-image-page';
import { insertPlaceholderAt } from './placeholder-insert';

interface Recorder {
  calls: Record<string, unknown>[];
}

function image(over: Partial<GeneratedImage> = {}): GeneratedImage {
  return {
    id: 'img-1',
    filename: 'one.png',
    filepath: 'tool/one.png',
    mimeType: 'image/png',
    ...over,
  };
}

/**
 * A CoreClient stub: `characterList` feeds the insert widget, `imageProfileList`
 * feeds the picker, and `imageProfileGenerate` returns whatever the test wants.
 */
function stubClient(opts: {
  characters?: { id: string; name: string }[];
  generate?: unknown;
  generateError?: string;
  recorder?: Recorder;
}): Partial<CoreClient> {
  const dispatchData = async (req: Record<string, unknown>) => {
    opts.recorder?.calls.push(req);
    if (req['type'] === 'characterList') return { characters: opts.characters ?? [] };
    return { profiles: [] };
  };
  const dispatch = async (req: Record<string, unknown>) => {
    opts.recorder?.calls.push(req);
    if (opts.generateError) return { type: 'error', data: { message: opts.generateError } };
    return { type: 'ok', data: opts.generate ?? { data: [] } };
  };
  return {
    dispatchData: dispatchData as unknown as CoreClient['dispatchData'],
    dispatch: dispatch as unknown as CoreClient['dispatch'],
  };
}

async function render(
  client: Partial<CoreClient>,
): Promise<ComponentFixture<GenerateImagePage>> {
  TestBed.configureTestingModule({
    imports: [GenerateImagePage],
    providers: [provideRouter([]), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(GenerateImagePage);
  fixture.detectChanges();
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

/** Drive the component the way the template does, then settle. */
async function settle(fixture: ComponentFixture<GenerateImagePage>): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

function generateButton(fixture: ComponentFixture<GenerateImagePage>): HTMLButtonElement {
  const buttons = Array.from(
    fixture.nativeElement.querySelectorAll('button'),
  ) as HTMLButtonElement[];
  return buttons.find((b) => b.textContent!.includes('Generate Image'))!;
}

// Reaching past `protected` is the point here: these specs pin transcribed v4
// logic, not the template's private shape.
type Inner = GenerateImagePage & {
  selectedProfileId: { set(v: string | null): void };
  prompt: { set(v: string): void };
  imageCount: { set(v: number): void };
  generatedImages: { (): GeneratedImage[] };
  notice: { (): { kind: string; message: string } | null };
  onGenerate(): Promise<void>;
  visibleEntities: { (): { id: string; name: string }[] };
  onSearchInput(v: string): void;
};

function inner(fixture: ComponentFixture<GenerateImagePage>): Inner {
  return fixture.componentInstance as unknown as Inner;
}

describe('insertPlaceholderAt (v4 GenerateImageView.tsx:71-89)', () => {
  it('wraps the name in double braces', () => {
    expect(insertPlaceholderAt('', 'Alice', 0, 0).value).toBe('{{Alice}}');
  });

  it('inserts at the caret, keeping both sides', () => {
    expect(insertPlaceholderAt('a b', 'X', 2, 2).value).toBe('a {{X}}b');
  });

  it('REPLACES the selected range rather than inserting beside it (v4 :77-78)', () => {
    // `after` resumes at selectionEnd, so the selection is consumed.
    expect(insertPlaceholderAt('hello world', 'X', 0, 5).value).toBe('{{X}} world');
  });

  it('puts the caret after the closing braces so inserts chain (v4 :87)', () => {
    const first = insertPlaceholderAt('', 'me', 0, 0);
    expect(first.caret).toBe('{{me}}'.length);
    const second = insertPlaceholderAt(first.value, 'char', first.caret, first.caret);
    expect(second.value).toBe('{{me}}{{char}}');
  });
});

describe('GenerateImagePage (v4 app/generate-image/GenerateImageView.tsx)', () => {
  it('does not auto-select a profile — Generate starts disabled (v4 :37,:282)', async () => {
    const fixture = await render(stubClient({}));
    expect(generateButton(fixture).disabled).toBe(true);
  });

  it('keeps Generate disabled with a prompt but no profile (v4 :282)', async () => {
    const fixture = await render(stubClient({}));
    inner(fixture).prompt.set('a cat');
    await settle(fixture);
    expect(generateButton(fixture).disabled).toBe(true);
  });

  it('keeps Generate disabled with a profile but no prompt (v4 :282)', async () => {
    const fixture = await render(stubClient({}));
    inner(fixture).selectedProfileId.set('p1');
    await settle(fixture);
    expect(generateButton(fixture).disabled).toBe(true);
  });

  it('keeps Generate disabled for a whitespace-only prompt (v4 :282 trims)', async () => {
    const fixture = await render(stubClient({}));
    inner(fixture).selectedProfileId.set('p1');
    inner(fixture).prompt.set('   ');
    await settle(fixture);
    expect(generateButton(fixture).disabled).toBe(true);
  });

  it('enables Generate once both a profile and a prompt are present (v4 :282)', async () => {
    const fixture = await render(stubClient({}));
    inner(fixture).selectedProfileId.set('p1');
    inner(fixture).prompt.set('a cat');
    await settle(fixture);
    expect(generateButton(fixture).disabled).toBe(false);
  });

  it('sends prompt + count and NO chatId — the standalone contract (v4 :115-124)', async () => {
    const rec: Recorder = { calls: [] };
    const fixture = await render(stubClient({ recorder: rec }));
    inner(fixture).selectedProfileId.set('p1');
    inner(fixture).prompt.set('  a cat  ');
    inner(fixture).imageCount.set(3);
    await inner(fixture).onGenerate();
    const call = rec.calls.find((c) => c['type'] === 'imageProfileGenerate')!;
    expect(call).toEqual({
      type: 'imageProfileGenerate',
      imageProfileId: 'p1',
      prompt: 'a cat',
      count: 3,
    });
    expect('chatId' in call).toBe(false);
  });

  it('prepends new images, newest first (v4 :137)', async () => {
    const fixture = await render(stubClient({ generate: { data: [image({ id: 'first' })] } }));
    inner(fixture).selectedProfileId.set('p1');
    inner(fixture).prompt.set('a cat');
    await inner(fixture).onGenerate();
    (fixture.componentInstance as never as { core: unknown }).core;
    expect(inner(fixture).generatedImages().map((i) => i.id)).toEqual(['first']);
  });

  it('treats an empty 200 payload as the "No images were generated" error (v4 :140)', async () => {
    const fixture = await render(stubClient({ generate: { data: [] } }));
    inner(fixture).selectedProfileId.set('p1');
    inner(fixture).prompt.set('a cat');
    await inner(fixture).onGenerate();
    expect(inner(fixture).notice()).toEqual({
      kind: 'error',
      message: 'No images were generated',
    });
  });

  it('surfaces the server error message (v4 :129-130)', async () => {
    const fixture = await render(stubClient({ generateError: 'no api key' }));
    inner(fixture).selectedProfileId.set('p1');
    inner(fixture).prompt.set('a cat');
    await inner(fixture).onGenerate();
    expect(inner(fixture).notice()!.message).toBe('no api key');
  });

  it('reports the plural count on success (v4 :138)', async () => {
    const fixture = await render(
      stubClient({ generate: { data: [image({ id: 'a' }), image({ id: 'b' })] } }),
    );
    inner(fixture).selectedProfileId.set('p1');
    inner(fixture).prompt.set('a cat');
    await inner(fixture).onGenerate();
    expect(inner(fixture).notice()).toEqual({ kind: 'success', message: 'Generated 2 images' });
  });

  it('reports the singular count on success (v4 :138)', async () => {
    const fixture = await render(stubClient({ generate: { data: [image()] } }));
    inner(fixture).selectedProfileId.set('p1');
    inner(fixture).prompt.set('a cat');
    await inner(fixture).onGenerate();
    expect(inner(fixture).notice()).toEqual({ kind: 'success', message: 'Generated 1 image' });
  });

  it('offers exactly the counts 1-4 (v4 :270)', async () => {
    const fixture = await render(stubClient({}));
    const opts = Array.from(
      fixture.nativeElement.querySelectorAll('#generate-image-count option'),
    ) as HTMLOptionElement[];
    expect(opts.map((o) => o.value)).toEqual(['1', '2', '3', '4']);
    expect(opts[0].textContent!.trim()).toBe('1 image');
    expect(opts[1].textContent!.trim()).toBe('2 images');
  });

  it('shows the empty state before anything is generated (v4 :358-364)', async () => {
    const fixture = await render(stubClient({}));
    expect(fixture.nativeElement.textContent).toContain('No images generated yet');
  });

  it('sorts the insert list by name and caps the dropdown at ten (v4 :64,:241)', async () => {
    const many = Array.from({ length: 14 }, (_, i) => ({
      id: `c${i}`,
      name: `Char ${String(13 - i).padStart(2, '0')}`,
    }));
    const fixture = await render(stubClient({ characters: many }));
    const visible = inner(fixture).visibleEntities();
    expect(visible).toHaveLength(10);
    expect(visible[0].name).toBe('Char 00');
    expect(visible[9].name).toBe('Char 09');
  });

  it('filters the insert list case-insensitively (v4 :95-97)', async () => {
    const fixture = await render(
      stubClient({
        characters: [
          { id: 'a', name: 'Alice' },
          { id: 'b', name: 'Bob' },
        ],
      }),
    );
    inner(fixture).onSearchInput('aLi');
    await settle(fixture);
    expect(inner(fixture).visibleEntities().map((e) => e.name)).toEqual(['Alice']);
  });

  it('renders images through the id-keyed byte route, not the store filepath', async () => {
    const fixture = await render(
      stubClient({ generate: { data: [image({ id: 'img-9', filepath: 'tool/nine.png' })] } }),
    );
    inner(fixture).selectedProfileId.set('p1');
    inner(fixture).prompt.set('a cat');
    await inner(fixture).onGenerate();
    await settle(fixture);
    const img = fixture.nativeElement.querySelector('.grid img') as HTMLImageElement;
    expect(img.getAttribute('src')).toContain('/api/v1/files/img-9');
    expect(img.getAttribute('src')).not.toContain('tool/nine.png');
  });
});
