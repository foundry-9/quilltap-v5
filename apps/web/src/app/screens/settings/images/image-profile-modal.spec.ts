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

function providerSelect(fixture: ComponentFixture<ImageProfileModal>): HTMLSelectElement {
  const selects = Array.from(
    fixture.nativeElement.querySelectorAll('select'),
  ) as HTMLSelectElement[];
  return selects[0];
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

/**
 * The options-schema / LoRA wiring (v4 `84f33ce94`'s `ImageProfileForm.tsx`
 * hunks plus `2ece98c90`'s `hfToken` prop).
 *
 * ⚠ `imageProfileOptionsSchema` is P4.D138's verb, running in a parallel lane,
 * so every case here drives a scripted `CoreClient` rather than a live server.
 */

interface SchemaDispatch {
  type: string;
  provider?: string;
  model?: string;
  apiKeyId?: string;
}

const TINY_SCHEMA = {
  groups: [{ title: 'Rendering', fields: [{ key: 'steps', label: 'Steps', type: 'number' }] }],
};

const SUPPORT_2 = { maxLoras: 2, sourceKinds: ['hf-repo'] };

/**
 * A client whose `options-schema` answer is scripted per call, so a case can
 * script "generic first, then the real thing once the catalog lands".
 */
function schemaClient(opts: {
  answers: Array<Record<string, unknown> | 'reject'>;
  listModels?: Record<string, unknown>;
  seen?: SchemaDispatch[];
}): Partial<CoreClient> {
  let n = 0;
  return {
    dispatchData: (async (req: SchemaDispatch) => {
      opts.seen?.push(req);
      if (req.type === 'imageProfileOptionsSchema') {
        const answer = opts.answers[Math.min(n, opts.answers.length - 1)];
        n += 1;
        if (answer === 'reject') throw new Error('boom');
        return answer;
      }
      if (req.type === 'imageProfileListModels') {
        return opts.listModels ?? { provider: 'OPENAI', models: [], source: 'builtin' };
      }
      if (req.type === 'imageProviderList') return { providers: [] };
      return {};
    }) as unknown as CoreClient['dispatchData'],
    dispatchExpect: (async () => ({ data: { apiKeys: [KEY] } })) as unknown as CoreClient['dispatchExpect'],
  };
}

/** The hosted LoRA editor instance, for the inputs the DOM cannot show. */
function loraEditor(fixture: ComponentFixture<ImageProfileModal>): {
  hfToken: () => string | undefined;
} {
  const node = (fixture.nativeElement as HTMLElement).querySelector('qt-lora-list-editor');
  if (!node) throw new Error('LoRA editor not rendered');
  return fixture.debugElement.query((d) => d.nativeElement === node).componentInstance as {
    hfToken: () => string | undefined;
  };
}

function has(fixture: ComponentFixture<ImageProfileModal>, selector: string): boolean {
  return (fixture.nativeElement as HTMLElement).querySelector(selector) !== null;
}

/** The raw-JSON textarea — v5's legacy arm, v4's `ImageProfileParameters`. */
function hasLegacyTextarea(fixture: ComponentFixture<ImageProfileModal>): boolean {
  return has(fixture, 'textarea');
}

describe('ImageProfileModal — the options-schema swap', () => {
  it('renders the shared panel when the plugin declares a schema, and NOT the legacy arm', async () => {
    const fixture = await render(
      schemaClient({ answers: [{ optionsSchema: TINY_SCHEMA, loraSupport: null }] }),
    );
    expect(has(fixture, 'qt-provider-options-panel')).toBe(true);
    expect(hasLegacyTextarea(fixture)).toBe(false);
  });

  it('falls back to the legacy arm when the plugin declares none (v4 `:537-556`)', async () => {
    const fixture = await render(
      schemaClient({ answers: [{ optionsSchema: null, loraSupport: null }] }),
    );
    expect(has(fixture, 'qt-provider-options-panel')).toBe(false);
    // "A provider whose editor offers nothing at all would be worse than a
    // slightly stale size list."
    expect(hasLegacyTextarea(fixture)).toBe(true);
  });

  it('falls back to the legacy arm when the FETCH itself fails (v4 `:214-220`)', async () => {
    const fixture = await render(schemaClient({ answers: ['reject'] }));
    expect(has(fixture, 'qt-provider-options-panel')).toBe(false);
    expect(hasLegacyTextarea(fixture)).toBe(true);
  });

  it('a failed fetch CLEARS a schema already on screen rather than leaving it stale', async () => {
    // v4's comment is the assertion: "rather than leaving a stale schema from
    // the previous provider on screen".
    //
    // ⚠ This case changes the MODEL, not the provider, and that is the whole
    // point: `onProviderChange` clears both eagerly, so a provider switch
    // would show a cleared panel even if the catch arm did nothing at all.
    // Only a same-provider model change reaches the catch arm alone.
    const fixture = await render(
      schemaClient({ answers: [{ optionsSchema: TINY_SCHEMA, loraSupport: SUPPORT_2 }, 'reject'] }),
      profile({ modelName: 'dall-e-3' }),
    );
    expect(has(fixture, 'qt-provider-options-panel')).toBe(true);
    expect(has(fixture, 'qt-lora-list-editor div')).toBe(true);

    const select = modelSelect(fixture);
    select.value = 'dall-e-2';
    select.dispatchEvent(new Event('change'));
    for (let i = 0; i < 8; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    expect(has(fixture, 'qt-provider-options-panel')).toBe(false);
    expect(has(fixture, 'qt-lora-list-editor div')).toBe(false);
  });

  it('a provider change clears BOTH eagerly, before any answer lands (v4 `:268-272`)', async () => {
    // The companion arm: the old provider's schema and LoRA cap describe a
    // provider we have just left. This one is measured by never letting the
    // refetch resolve at all.
    const fixture = await render(
      schemaClient({ answers: [{ optionsSchema: TINY_SCHEMA, loraSupport: SUPPORT_2 }] }),
    );
    expect(has(fixture, 'qt-provider-options-panel')).toBe(true);

    const select = providerSelect(fixture);
    select.value = 'NANOGPT';
    select.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    // Synchronously after the change — no awaits, so nothing has answered.
    expect(has(fixture, 'qt-provider-options-panel')).toBe(false);
    expect(has(fixture, 'qt-lora-list-editor div')).toBe(false);
  });

  it('asks again once a live model fetch lands, because the catalog now exists', async () => {
    // v4 `:168-170` + `:101-111`: a plugin may build its schema from a catalog
    // it can only load with a key, so the cold-cache answer is generic.
    const seen: SchemaDispatch[] = [];
    const fixture = await render(
      schemaClient({
        answers: [{ optionsSchema: null, loraSupport: null }, { optionsSchema: TINY_SCHEMA, loraSupport: null }],
        listModels: { provider: 'OPENAI', models: ['m1'], source: 'provider' },
        seen,
      }),
      profile({ apiKeyId: 'k1' }),
    );
    for (let i = 0; i < 8; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    expect(seen.filter((r) => r.type === 'imageProfileOptionsSchema').length).toBeGreaterThanOrEqual(
      2,
    );
    expect(has(fixture, 'qt-provider-options-panel')).toBe(true);
  });

  it('sends the NORMALIZED provider and the selected model', async () => {
    const seen: SchemaDispatch[] = [];
    await render(
      schemaClient({ answers: [{ optionsSchema: null, loraSupport: null }], seen }),
      profile({ provider: 'GOOGLE_IMAGEN', modelName: 'imagen-4.0-generate-001', apiKeyId: null }),
    );
    const asked = seen.filter((r) => r.type === 'imageProfileOptionsSchema');
    expect(asked.length).toBeGreaterThan(0);
    expect(asked.some((r) => r.provider === 'GOOGLE')).toBe(true);
    expect(asked.some((r) => r.model === 'imagen-4.0-generate-001')).toBe(true);
    // The legacy alias is never what reaches the wire.
    expect(asked.some((r) => r.provider === 'GOOGLE_IMAGEN')).toBe(false);
  });

  it('a superseded answer never lands — the cancelled flag (v4 `:195,209,216`)', async () => {
    // The race the flag exists for: a slow answer for a model the user has
    // already left must not overwrite the newer one. Scripted by holding the
    // FIRST answer open until after the second has landed.
    let releaseFirst: ((v: Record<string, unknown>) => void) | null = null;
    const first = new Promise<Record<string, unknown>>((resolve) => {
      releaseFirst = resolve;
    });
    let n = 0;
    const client: Partial<CoreClient> = {
      dispatchData: (async (req: SchemaDispatch) => {
        if (req.type === 'imageProfileOptionsSchema') {
          n += 1;
          return n === 1 ? first : { optionsSchema: null, loraSupport: null };
        }
        if (req.type === 'imageProfileListModels') {
          return { provider: 'OPENAI', models: [], source: 'builtin' };
        }
        if (req.type === 'imageProviderList') return { providers: [] };
        return {};
      }) as unknown as CoreClient['dispatchData'],
      dispatchExpect: (async () => ({
        data: { apiKeys: [KEY] },
      })) as unknown as CoreClient['dispatchExpect'],
    };

    const fixture = await render(client, profile({ modelName: 'dall-e-3' }));
    const select = modelSelect(fixture);
    select.value = 'dall-e-2';
    select.dispatchEvent(new Event('change'));
    for (let i = 0; i < 8; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    // The second answer has landed: no schema.
    expect(has(fixture, 'qt-provider-options-panel')).toBe(false);

    // Now the first, stale answer arrives carrying a schema. It must be
    // dropped on the floor.
    releaseFirst!({ optionsSchema: TINY_SCHEMA, loraSupport: SUPPORT_2 });
    for (let i = 0; i < 8; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    expect(has(fixture, 'qt-provider-options-panel')).toBe(false);
    expect(has(fixture, 'qt-lora-list-editor div')).toBe(false);
  });
});

describe('ImageProfileModal — the LoRA editor', () => {
  it('draws nothing when the model declares no support', async () => {
    const fixture = await render(
      schemaClient({ answers: [{ optionsSchema: null, loraSupport: null }] }),
    );
    expect(text(fixture)).not.toContain('LoRA Adapters (Optional)');
  });

  it('draws the editor when it does', async () => {
    const fixture = await render(
      schemaClient({ answers: [{ optionsSchema: null, loraSupport: SUPPORT_2 }] }),
    );
    expect(text(fixture)).toContain('LoRA Adapters (Optional)');
    expect(text(fixture)).toContain('This model accepts up to 2 adapters.');
  });

  it('hands it the stored list', async () => {
    const fixture = await render(
      schemaClient({ answers: [{ optionsSchema: null, loraSupport: SUPPORT_2 }] }),
      profile({ parameters: { loras: [{ source: 'owner/one' }] } }),
    );
    expect(text(fixture)).toContain('Adapter 1');
  });

  it('...and only when the bag holds an ARRAY under the key (v4 `:329-331`)', async () => {
    const fixture = await render(
      schemaClient({ answers: [{ optionsSchema: null, loraSupport: SUPPORT_2 }] }),
      profile({ parameters: { loras: 'not-a-list' } }),
    );
    expect(text(fixture)).toContain(
      'No adapters attached — the model generates in its own native manner.',
    );
  });

  it('passes the profile’s hf_api_token down', async () => {
    const fixture = await render(
      schemaClient({ answers: [{ optionsSchema: null, loraSupport: SUPPORT_2 }] }),
      profile({ parameters: { hf_api_token: 'hf_secret' } }),
    );
    expect(loraEditor(fixture).hfToken()).toBe('hf_secret');
  });

  it('...and only when it is a string (v4 `:564-568`)', async () => {
    const fixture = await render(
      schemaClient({ answers: [{ optionsSchema: null, loraSupport: SUPPORT_2 }] }),
      profile({ parameters: { hf_api_token: 42 } }),
    );
    expect(loraEditor(fixture).hfToken()).toBeUndefined();
  });
});

describe('ImageProfileModal — writing the parameters bag', () => {
  type Writer = {
    setParameter: (w: { key: string; value: unknown }) => void;
    setLoras: (l: Array<{ source: string }>) => void;
    parametersBag: () => Record<string, unknown>;
  };

  async function writer(parameters: Record<string, unknown> = {}): Promise<Writer> {
    const fixture = await render(
      schemaClient({ answers: [{ optionsSchema: TINY_SCHEMA, loraSupport: SUPPORT_2 }] }),
      profile({ parameters }),
    );
    return fixture.componentInstance as unknown as Writer;
  }

  it('writes a value into the bag, preserving its neighbours', async () => {
    const w = await writer({ size: '1024x1024' });
    w.setParameter({ key: 'steps', value: 30 });
    expect(w.parametersBag()).toEqual({ size: '1024x1024', steps: 30 });
  });

  it('DELETES the key on undefined — an empty box means "unset" (v4 `:320-324`)', async () => {
    const w = await writer({ steps: 30, size: '1024x1024' });
    w.setParameter({ key: 'steps', value: undefined });
    expect(w.parametersBag()).toEqual({ size: '1024x1024' });
  });

  it('...and on the empty string, which is the shape a cleared text box sends', async () => {
    // Storing '' would send a blank string to a provider that reads the key's
    // presence, not its truthiness.
    const w = await writer({ hf_api_token: 'hf_secret' });
    w.setParameter({ key: 'hf_api_token', value: '' });
    expect(w.parametersBag()).toEqual({});
  });

  it('keeps a falsy value that is NOT undefined or empty', async () => {
    const w = await writer({});
    w.setParameter({ key: 'steps', value: 0 });
    w.setParameter({ key: 'flag', value: false });
    expect(w.parametersBag()).toEqual({ steps: 0, flag: false });
  });

  it('writes the LoRA list under the reserved key', async () => {
    const w = await writer({ size: '1024x1024' });
    w.setLoras([{ source: 'owner/one' }]);
    expect(w.parametersBag()).toEqual({ size: '1024x1024', loras: [{ source: 'owner/one' }] });
  });

  it('DELETES the reserved key when the list empties (v4 `:333-343`)', async () => {
    const w = await writer({ size: '1024x1024', loras: [{ source: 'owner/one' }] });
    w.setLoras([]);
    expect(w.parametersBag()).toEqual({ size: '1024x1024' });
    // Not `loras: []` — an empty array would ride to the provider as a
    // declared-and-empty list rather than as no list at all.
    expect('loras' in w.parametersBag()).toBe(false);
  });
});
