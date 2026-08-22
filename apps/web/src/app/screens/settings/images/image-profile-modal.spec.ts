import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { ImageProfileDto } from '../../../core/core-contract';
import { ImageProfileModal } from './image-profile-modal';

/**
 * The Fetch Models flow, pinned against v4
 * `components/image-profiles/ImageProfileForm.tsx` at `d5830439` — the state
 * transitions (`:135-175`) and the four source labels (`:427-440`). The label
 * strings are transcribed character-for-character; they are the whole point of
 * the pin, so they are asserted whole rather than by substring.
 */

interface Dispatched {
  type: string;
  provider?: string;
  apiKeyId?: string;
}

const KEY = {
  id: 'k1',
  label: 'My Key',
  provider: 'OPENAI',
  isActive: true,
  lastUsed: null,
  createdAt: '2024-01-01T00:00:00.000Z',
  updatedAt: '2024-01-01T00:00:00.000Z',
  keyPreview: 'sk-…',
};

function profile(over: Partial<ImageProfileDto>): ImageProfileDto {
  return {
    id: 'p1',
    userId: 'u1',
    name: 'Profile',
    provider: 'OPENAI',
    apiKeyId: 'k1',
    baseUrl: null,
    modelName: 'dall-e-3',
    parameters: {},
    isDefault: false,
    isDangerousCompatible: false,
    tags: [],
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    apiKey: null,
    ...over,
  };
}

/**
 * A CoreClient whose `imageProfileListModels` answer is scripted. `models` is
 * undefined to make the verb REJECT, which is how a hard request failure
 * reaches v4's catch-branch fallback.
 */
function stubClient(
  listModels: Record<string, unknown> | 'reject',
  seen: Dispatched[] = [],
): Partial<CoreClient> {
  return {
    dispatchData: (async (req: Dispatched) => {
      seen.push(req);
      if (req.type === 'imageProfileListModels') {
        if (listModels === 'reject') {
          throw new Error('boom');
        }
        return listModels;
      }
      if (req.type === 'imageProviderList') {
        return { providers: [] };
      }
      return {};
    }) as unknown as CoreClient['dispatchData'],
    dispatchExpect: (async () => ({
      data: { apiKeys: [KEY] },
    })) as unknown as CoreClient['dispatchExpect'],
  };
}

async function render(
  client: Partial<CoreClient>,
  input?: ImageProfileDto | null,
): Promise<ComponentFixture<ImageProfileModal>> {
  TestBed.configureTestingModule({
    imports: [ImageProfileModal],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(ImageProfileModal);
  fixture.componentRef.setInput('profile', input ?? null);
  fixture.detectChanges();
  for (let i = 0; i < 8; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

function text(fixture: ComponentFixture<ImageProfileModal>): string {
  return fixture.nativeElement.textContent as string;
}

function fetchButton(fixture: ComponentFixture<ImageProfileModal>): HTMLButtonElement {
  const buttons = Array.from(
    fixture.nativeElement.querySelectorAll('button'),
  ) as HTMLButtonElement[];
  const button = buttons.find((b) => /Fetch Models|Fetching\.\.\./.test(b.textContent ?? ''));
  if (!button) {
    throw new Error('Fetch Models button not rendered');
  }
  return button;
}

function modelSelect(fixture: ComponentFixture<ImageProfileModal>): HTMLSelectElement {
  const selects = Array.from(
    fixture.nativeElement.querySelectorAll('select'),
  ) as HTMLSelectElement[];
  // Provider, API key, then Model — the third select in v4's field order.
  return selects[2];
}

describe('ImageProfileModal — Fetch Models', () => {
  it('auto-loads on open and reports a provider-sourced list (v4 :427-431)', async () => {
    const seen: Dispatched[] = [];
    const fixture = await render(
      stubClient({ provider: 'OPENAI', models: ['a', 'b'], source: 'provider' }, seen),
      profile({}),
    );
    expect(seen.some((r) => r.type === 'imageProfileListModels')).toBe(true);
    expect(text(fixture)).toContain('✓ 2 image models fetched from the provider');
  });

  it('says "model" singular for a one-model provider list', async () => {
    const fixture = await render(
      stubClient({ provider: 'OPENAI', models: ['only'], source: 'provider' }),
      profile({}),
    );
    expect(text(fixture)).toContain('✓ 1 image model fetched from the provider');
  });

  it('renders the keyless built-in sentence when no API key is selected', async () => {
    const fixture = await render(
      stubClient({ provider: 'OPENAI', models: ['a'], source: 'builtin' }),
      profile({ apiKeyId: null }),
    );
    expect(text(fixture)).toContain(
      "Showing the plugin's built-in model list — select an API key and Fetch Models to query the provider.",
    );
  });

  it('renders the plain built-in sentence when a key IS selected', async () => {
    const fixture = await render(
      stubClient({ provider: 'OPENAI', models: ['a'], source: 'builtin' }),
      profile({ apiKeyId: 'k1' }),
    );
    const body = text(fixture);
    expect(body).toContain("Showing the plugin's built-in model list.");
    expect(body).not.toContain('select an API key and Fetch Models');
  });

  it('names the live-fetch failure when the server reports one (v4 :434)', async () => {
    const fixture = await render(
      stubClient({
        provider: 'OPENAI',
        models: ['a'],
        source: 'builtin',
        fetchError: '401 Unauthorized',
      }),
      profile({ apiKeyId: 'k1' }),
    );
    expect(text(fixture)).toContain(
      "Couldn't fetch from the provider (401 Unauthorized) — showing the plugin's built-in list.",
    );
  });

  it('falls back to builtin WITHOUT the Couldn’t sentence when the request itself fails', async () => {
    const fixture = await render(stubClient('reject'), profile({ apiKeyId: 'k1' }));
    const body = text(fixture);
    expect(body).toContain("Showing the plugin's built-in model list.");
    expect(body).not.toContain("Couldn't fetch from the provider");
  });

  it('normalizes a legacy provider before asking the server (v4 :141)', async () => {
    const seen: Dispatched[] = [];
    const fixture = await render(
      stubClient(
        { provider: 'GOOGLE', models: ['imagen-4.0-generate-001'], source: 'provider' },
        seen,
      ),
      profile({ provider: 'GOOGLE_IMAGEN', modelName: 'imagen-4.0-generate-001' }),
    );
    await fixture.whenStable();
    const calls = seen.filter((r) => r.type === 'imageProfileListModels');
    expect(calls.length).toBeGreaterThan(0);
    expect(calls.every((c) => c.provider === 'GOOGLE')).toBe(true);
  });

  it('omits apiKeyId entirely when no key is selected (v4 :145-147)', async () => {
    const seen: Dispatched[] = [];
    await render(
      stubClient({ provider: 'OPENAI', models: [], source: 'builtin' }, seen),
      profile({ apiKeyId: null }),
    );
    const call = seen.find((r) => r.type === 'imageProfileListModels');
    expect(call).toBeDefined();
    expect('apiKeyId' in (call as object)).toBe(false);
  });

  it('disables the button without a key, with v4’s title (v4 :417-425)', async () => {
    const fixture = await render(
      stubClient({ provider: 'OPENAI', models: [], source: 'builtin' }),
      profile({ apiKeyId: null }),
    );
    const button = fetchButton(fixture);
    expect(button.disabled).toBe(true);
    expect(button.getAttribute('title')).toBe('Select an API key first');
    expect(button.textContent?.trim()).toBe('Fetch Models');
  });

  it('enables the button with a key, with v4’s other title', async () => {
    const fixture = await render(
      stubClient({ provider: 'OPENAI', models: ['a'], source: 'provider' }),
      profile({ apiKeyId: 'k1' }),
    );
    const button = fetchButton(fixture);
    expect(button.disabled).toBe(false);
    expect(button.getAttribute('title')).toBe('Query the provider for its image models');
  });

  it('prepends a saved model the fetched list omits, keeping it selected (v4 :402-415)', async () => {
    const fixture = await render(
      stubClient({ provider: 'OPENAI', models: ['a', 'b'], source: 'provider' }),
      profile({ modelName: 'retired-model' }),
    );
    const select = modelSelect(fixture);
    const options = Array.from(select.options).map((o) => o.value);
    expect(options).toEqual(['retired-model', 'a', 'b']);
    expect(select.value).toBe('retired-model');
  });

  it('leaves the Model select blank when the name matches no option (React parity)', async () => {
    const fixture = await render(
      stubClient({ provider: 'OPENAI', models: ['a', 'b'], source: 'provider' }),
      profile({ modelName: '' }),
    );
    const select = modelSelect(fixture);
    // v4 assigns select.value after mount, so an unmatched value leaves the
    // control blank rather than snapping to row 0.
    expect(select.selectedIndex).toBe(-1);
  });

  it('suppresses BOTH source labels while a fetch is in flight (v4 :427,:432)', async () => {
    let release: (() => void) | undefined;
    const client: Partial<CoreClient> = {
      dispatchData: (async (req: Dispatched) => {
        if (req.type === 'imageProfileListModels') {
          await new Promise<void>((r) => {
            release = r;
          });
          return { provider: 'OPENAI', models: ['a'], source: 'provider' };
        }
        if (req.type === 'imageProviderList') {
          return { providers: [] };
        }
        return {};
      }) as unknown as CoreClient['dispatchData'],
      dispatchExpect: (async () => ({
        data: { apiKeys: [KEY] },
      })) as unknown as CoreClient['dispatchExpect'],
    };
    TestBed.configureTestingModule({
      imports: [ImageProfileModal],
      providers: [
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: client },
      ],
    });
    const fixture = TestBed.createComponent(ImageProfileModal);
    fixture.componentRef.setInput('profile', profile({ apiKeyId: 'k1' }));
    fixture.detectChanges();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();

    const body = text(fixture);
    expect(body).toContain('Fetching...');
    expect(body).not.toContain('fetched from the provider');
    expect(body).not.toContain('built-in model list');
    expect(fetchButton(fixture).disabled).toBe(true);
    expect(modelSelect(fixture).disabled).toBe(true);

    release?.();
  });
  /**
   * The suppression only bites on a RE-fetch: during the first one
   * `modelsSource` is still null and neither label would render anyway. Mutation
   * testing caught this — dropping `!isFetchingModels() &&` from the guards left
   * every other test in this file green.
   */
  it('hides an already-shown source label during a RE-fetch (v4 :427,:432)', async () => {
    let calls = 0;
    let release: (() => void) | undefined;
    const client: Partial<CoreClient> = {
      dispatchData: (async (req: Dispatched) => {
        if (req.type === 'imageProfileListModels') {
          calls += 1;
          if (calls > 1) {
            await new Promise<void>((r) => {
              release = r;
            });
          }
          return { provider: 'OPENAI', models: ['a', 'b'], source: 'provider' };
        }
        if (req.type === 'imageProviderList') {
          return { providers: [] };
        }
        return {};
      }) as unknown as CoreClient['dispatchData'],
      dispatchExpect: (async () => ({
        data: { apiKeys: [KEY] },
      })) as unknown as CoreClient['dispatchExpect'],
    };
    const fixture = await render(client, profile({ apiKeyId: 'k1' }));

    // The first fetch settled and the tally is on screen.
    expect(text(fixture)).toContain('✓ 2 image models fetched from the provider');

    // A second fetch is now in flight; the stale tally must go.
    fetchButton(fixture).click();
    fixture.detectChanges();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();

    const body = text(fixture);
    expect(body).toContain('Fetching...');
    expect(body).not.toContain('fetched from the provider');

    release?.();
  });

  /**
   * v4 `:154` reads `data.source === 'provider' ? 'provider' : 'builtin'` — the
   * check is exact, so any other value (or none) reads as built-in rather than
   * being trusted through. Also caught by mutation testing.
   */
  it('treats any non-“provider” source as builtin (v4 :154)', async () => {
    const fixture = await render(
      stubClient({ provider: 'OPENAI', models: ['a'], source: 'cache' }),
      profile({ apiKeyId: 'k1' }),
    );
    const body = text(fixture);
    expect(body).toContain("Showing the plugin's built-in model list.");
    expect(body).not.toContain('fetched from the provider');
  });

  it('treats a missing source as builtin', async () => {
    const fixture = await render(
      stubClient({ provider: 'OPENAI', models: ['a'] }),
      profile({ apiKeyId: 'k1' }),
    );
    expect(text(fixture)).toContain("Showing the plugin's built-in model list.");
  });
});

/**
 * The structured Default Size panel — v4
 * `components/image-profiles/ImageProfileParameters.tsx:126-181`. The two cases
 * v4's drift commits added; the other four providers keep v5's JSON textarea
 * stand-in.
 */
describe('ImageProfileModal — the Default Size panel', () => {
  function sizeSelect(fixture: ComponentFixture<ImageProfileModal>): HTMLSelectElement | null {
    const selects = Array.from(
      fixture.nativeElement.querySelectorAll('select'),
    ) as HTMLSelectElement[];
    // Provider, API key, Model, then Size — the fourth, when it renders at all.
    return selects[3] ?? null;
  }

  function textarea(fixture: ComponentFixture<ImageProfileModal>): HTMLTextAreaElement | null {
    return fixture.nativeElement.querySelector('textarea');
  }

  it('renders Z.AI’s eight sizes and footnote verbatim (v4 :126-153)', async () => {
    const fixture = await render(
      stubClient({ provider: 'Z_AI', models: ['glm-image'], source: 'builtin' }),
      profile({ provider: 'Z_AI', modelName: 'glm-image' }),
    );
    const select = sizeSelect(fixture);
    expect(select).not.toBeNull();
    expect(
      Array.from((select as HTMLSelectElement).options).map((o) => o.textContent?.trim()),
    ).toEqual([
      'Square (1024x1024)',
      'Square (1280x1280)',
      'Landscape (1568x1056)',
      'Wide (1664x928)',
      'Landscape (1472x1104)',
      'Portrait (1056x1568)',
      'Tall (928x1664)',
      'Portrait (1104x1472)',
    ]);
    expect(text(fixture)).toContain("Z.AI's recommended sizes for CogView and GLM-Image");
  });

  it('renders NanoGPT’s seven sizes and footnote verbatim (v4 :155-181)', async () => {
    const fixture = await render(
      stubClient({ provider: 'NANOGPT', models: ['hidream'], source: 'builtin' }),
      profile({ provider: 'NANOGPT', modelName: 'hidream' }),
    );
    const select = sizeSelect(fixture);
    expect(Array.from((select as HTMLSelectElement).options).map((o) => o.value)).toEqual([
      '1024x1024',
      '1248x832',
      '1360x768',
      '1536x1024',
      '832x1248',
      '768x1360',
      '1024x1536',
    ]);
    expect(text(fixture)).toContain(
      "Common sizes across NanoGPT's image models; each model maps to its nearest native resolution",
    );
  });

  it('defaults the selection to 1024x1024 when the bag has no size (v4 :137)', async () => {
    const fixture = await render(
      stubClient({ provider: 'NANOGPT', models: ['hidream'], source: 'builtin' }),
      profile({ provider: 'NANOGPT', parameters: {} }),
    );
    expect((sizeSelect(fixture) as HTMLSelectElement).value).toBe('1024x1024');
  });

  it('shows a stored size as the selection', async () => {
    const fixture = await render(
      stubClient({ provider: 'NANOGPT', models: ['hidream'], source: 'builtin' }),
      profile({ provider: 'NANOGPT', parameters: { size: '1360x768' } }),
    );
    expect((sizeSelect(fixture) as HTMLSelectElement).value).toBe('1360x768');
  });

  /**
   * The trap this panel exists to avoid: v4's `handleChange` spreads the bag
   * (`:14-19`), so a key the panel never renders survives an edit. A rebuilt
   * bag would silently drop it.
   */
  it('preserves parameter keys it does not render when the size changes', async () => {
    const fixture = await render(
      stubClient({ provider: 'NANOGPT', models: ['hidream'], source: 'builtin' }),
      profile({
        provider: 'NANOGPT',
        parameters: { size: '1024x1024', negative_prompt: 'blurry', steps: 30 },
      }),
    );
    const select = sizeSelect(fixture) as HTMLSelectElement;
    select.value = '832x1248';
    select.dispatchEvent(new Event('change'));
    fixture.detectChanges();

    const bag = JSON.parse(
      (fixture.componentInstance as unknown as { parametersText: () => string }).parametersText(),
    ) as Record<string, unknown>;
    expect(bag).toEqual({ size: '832x1248', negative_prompt: 'blurry', steps: 30 });
  });

  it('leaves the size select blank for an off-list stored size (React parity)', async () => {
    const fixture = await render(
      stubClient({ provider: 'Z_AI', models: ['glm-image'], source: 'builtin' }),
      profile({ provider: 'Z_AI', parameters: { size: '4096x4096' } }),
    );
    expect((sizeSelect(fixture) as HTMLSelectElement).selectedIndex).toBe(-1);
  });

  it('keeps the JSON textarea for a provider v4 has no size case for', async () => {
    const fixture = await render(
      stubClient({ provider: 'OPENAI', models: ['dall-e-3'], source: 'builtin' }),
      profile({ provider: 'OPENAI' }),
    );
    expect(textarea(fixture)).not.toBeNull();
    expect(sizeSelect(fixture)).toBeNull();
  });

  it('hides the textarea for a provider that HAS a size case (v4 shows only the panel)', async () => {
    const fixture = await render(
      stubClient({ provider: 'Z_AI', models: ['glm-image'], source: 'builtin' }),
      profile({ provider: 'Z_AI' }),
    );
    expect(textarea(fixture)).toBeNull();
  });
});
