import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { ConnectionProfileDto, CoreRequest, ProviderInfo } from '../../../core/core-contract';
import { ProfileModal } from './profile-modal';

function provider(over: Partial<ProviderInfo>): ProviderInfo {
  return {
    id: 'OPENAI',
    name: 'OPENAI',
    displayName: 'OpenAI',
    description: '',
    abbreviation: 'AI',
    type: 'llm',
    capabilities: { chat: true, imageGeneration: false, embeddings: false, webSearch: false },
    configRequirements: { requiresApiKey: true, requiresBaseUrl: false },
    ...over,
  };
}

function stubClient(over: Partial<CoreClient> = {}): Partial<CoreClient> {
  return {
    dispatchExpect: vi.fn(async () => ({
      type: 'connectionProfile',
      data: { profile: { id: 'new' } },
    })) as unknown as CoreClient['dispatchExpect'],
    dispatchData: vi.fn(async () => ({})) as CoreClient['dispatchData'],
    ...over,
  };
}

async function render(
  inputs: {
    takenNames?: Set<string>;
    providers?: ProviderInfo[];
    profile?: ConnectionProfileDto | null;
  },
  client: Partial<CoreClient> = stubClient(),
): Promise<ComponentFixture<ProfileModal>> {
  // Reset first: a couple of cases render twice in one `it` to compare two
  // bags, and TestBed refuses a second `configureTestingModule` once
  // instantiated.
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [ProfileModal],
    providers: [{ provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(ProfileModal);
  fixture.componentRef.setInput('profile', inputs.profile ?? null);
  fixture.componentRef.setInput('providers', inputs.providers ?? [provider({})]);
  fixture.componentRef.setInput('apiKeys', []);
  fixture.componentRef.setInput('takenNames', inputs.takenNames ?? new Set<string>());
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return fixture;
}

function typeName(fixture: ComponentFixture<ProfileModal>, value: string): void {
  const input = (fixture.nativeElement as HTMLElement).querySelector<HTMLInputElement>(
    '#qt-pf-name',
  )!;
  input.value = value;
  input.dispatchEvent(new Event('input'));
  fixture.detectChanges();
}

describe('ProfileModal (duplicate-name validation)', () => {
  it('flags a name already taken by another profile and disables submit', async () => {
    const fixture = await render({ takenNames: new Set(['my gpt-4 profile']) });
    typeName(fixture, 'My GPT-4 Profile'); // collides case-insensitively
    // Give it a model so only the name is at fault.
    fixture.componentInstance['setField']('modelName', 'gpt-4');
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain(
      'Another connection profile already bears this name. Names must be unique.',
    );
    const submit = Array.from(fixture.nativeElement.querySelectorAll('button')).find(
      (b) => (b as HTMLButtonElement).textContent?.trim() === 'Create Profile',
    ) as HTMLButtonElement;
    expect(submit.disabled).toBe(true);
  });

  it('accepts a unique name and enables submit', async () => {
    const fixture = await render({ takenNames: new Set(['other profile']) });
    typeName(fixture, 'My Fresh Profile');
    fixture.componentInstance['setField']('modelName', 'gpt-4');
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).not.toContain('already bears this name');
    const submit = Array.from(fixture.nativeElement.querySelectorAll('button')).find(
      (b) => (b as HTMLButtonElement).textContent?.trim() === 'Create Profile',
    ) as HTMLButtonElement;
    expect(submit.disabled).toBe(false);
  });

  it('submits a create with the built request body', async () => {
    const dispatchExpect = vi.fn(async (_req: CoreRequest) => ({
      type: 'connectionProfile',
      data: { profile: { id: 'new' } },
    }));
    const fixture = await render(
      {},
      stubClient({ dispatchExpect: dispatchExpect as unknown as CoreClient['dispatchExpect'] }),
    );
    typeName(fixture, 'Fresh One');
    fixture.componentInstance['setField']('modelName', 'gpt-4');
    fixture.detectChanges();
    const submit = Array.from(fixture.nativeElement.querySelectorAll('button')).find(
      (b) => (b as HTMLButtonElement).textContent?.trim() === 'Create Profile',
    ) as HTMLButtonElement;
    submit.click();
    await fixture.whenStable();
    const call = dispatchExpect.mock.calls[0][0] as unknown as {
      type: string;
      profile: Record<string, unknown>;
    };
    expect(call.type).toBe('connectionProfileCreate');
    expect(call.profile['name']).toBe('Fresh One');
    expect(call.profile['modelName']).toBe('gpt-4');
    expect(call.profile['transport']).toBe('api');
    expect(call.profile['parameters']).toMatchObject({
      temperature: 1,
      max_tokens: 4096,
      top_p: 1,
    });
  });
});

/**
 * The multi-character prefill checkbox (P4.D81 unit 2; v4
 * `ProfileModal.tsx:764-793` + `:237` from `23af7146`). The payload carries and
 * the load seed are pinned in `profile-form.spec.ts`; these are the modal's own
 * three obligations — render, re-seed, warn.
 */
describe('ProfileModal (multi-character prefill)', () => {
  const WARNING = "Anthropic's recent models reject a request handed over mid-turn";

  function prefillBox(fixture: ComponentFixture<ProfileModal>): HTMLInputElement {
    const label = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('label')).find(
      (l) =>
        l.textContent?.includes('Announce the speaker in multi-character scenes ([Name] prefill)'),
    );
    return label!.querySelector('input')!;
  }

  it('renders ticked on a provider whose default is on, with the help text and no warning', async () => {
    const fixture = await render({});
    expect(prefillBox(fixture).checked).toBe(true);
    expect(fixture.nativeElement.textContent).toContain(
      'Ticked, a multi-character turn is handed to the model already opened with',
    );
    expect(fixture.nativeElement.textContent).toContain(
      'for any model that spends its reply wondering whether the name was addressed to it',
    );
    expect(fixture.nativeElement.textContent).not.toContain(WARNING);
  });

  it('a provider switch re-seeds a NEW profile from the new provider default', async () => {
    const fixture = await render({
      providers: [
        provider({}),
        provider({ id: 'ANTHROPIC', name: 'ANTHROPIC', displayName: 'Anthropic' }),
      ],
    });
    expect(prefillBox(fixture).checked).toBe(true);
    fixture.componentInstance['onProviderChange']('ANTHROPIC');
    fixture.detectChanges();
    expect(prefillBox(fixture).checked).toBe(false);
    // ...and back again: the re-seed is a seed, not a one-way latch.
    fixture.componentInstance['onProviderChange']('OPENAI');
    fixture.detectChanges();
    expect(prefillBox(fixture).checked).toBe(true);
  });

  it('a provider switch does NOT clobber an EXISTING profile’s saved choice', async () => {
    // v4 keeps the re-seed inside the `if (!profile?.id)` guard alongside
    // allowToolUse/supportsImageUpload (`:230-238`).
    const fixture = await render({
      profile: {
        id: 'cp1',
        name: 'Saved',
        provider: 'OPENAI',
        modelName: 'gpt-4',
        parameters: {},
        isDefault: false,
        multiCharacterPrefill: false,
      },
      providers: [
        provider({}),
        provider({ id: 'ANTHROPIC', name: 'ANTHROPIC', displayName: 'Anthropic' }),
      ],
    });
    expect(prefillBox(fixture).checked).toBe(false);
    fixture.componentInstance['onProviderChange']('ANTHROPIC');
    fixture.detectChanges();
    expect(prefillBox(fixture).checked).toBe(false);
  });

  it('warns only when the box is ticked AGAINST the provider default', async () => {
    const fixture = await render({
      profile: {
        id: 'cp1',
        name: 'Saved',
        provider: 'ANTHROPIC',
        modelName: 'claude-sonnet-4-5-20250929',
        parameters: {},
        isDefault: false,
        multiCharacterPrefill: null,
      },
      providers: [provider({ id: 'ANTHROPIC', name: 'ANTHROPIC', displayName: 'Anthropic' })],
    });
    // Null → the Anthropic default (off), so no warning yet.
    expect(prefillBox(fixture).checked).toBe(false);
    expect(fixture.nativeElement.textContent).not.toContain(WARNING);

    const box = prefillBox(fixture);
    box.checked = true;
    box.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain(WARNING);
    expect(fixture.nativeElement.textContent).toContain(
      'Leave this unticked unless you know your model tolerates it.',
    );
  });

  it('ticking it on a provider that already defaults ON raises no warning', async () => {
    const fixture = await render({});
    const box = prefillBox(fixture);
    box.checked = false;
    box.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).not.toContain(WARNING);
    box.checked = true;
    box.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).not.toContain(WARNING);
  });
});

/**
 * The schema-driven provider-options panel in its host (P4.D84; v4
 * `ProfileModal.tsx:194-196` + `:899-908`). The renderer's own semantics are
 * pinned in `provider-options-panel.spec.ts`; these are the modal's four
 * obligations — draw the active provider's schema and nothing else, merge each
 * write into the PRESERVED bag, delete on `undefined`, and carry it all to the
 * save payload.
 *
 * The P4.D81 hardcoded Enable Thinking row is GONE, and with it the
 * `'true'`-string tolerance it carried: v4's generic `BooleanField` compares
 * `value === true`, so a hand-edited string shows OFF in both apps now (the
 * order's "measure v4 and match IT"). Nothing is silently rewritten — an
 * untouched box writes nothing, so the string still reaches the wire, which
 * does accept it.
 */
describe('ProfileModal (schema-driven provider options)', () => {
  const OLLAMA_SCHEMA = {
    groups: [
      {
        title: 'Ollama Options',
        fields: [
          {
            key: 'enable_thinking',
            label: 'Enable Thinking',
            type: 'boolean',
            default: false,
            helpText:
              'Let thinking-capable models (Qwen3, DeepSeek-R1, and kin) reason before answering.',
          },
          {
            key: 'thinking_effort',
            label: 'Thinking Effort',
            type: 'enum',
            default: '',
            showIf: { field: 'enable_thinking', equals: true },
            enumValues: [
              { value: '', label: 'Model default' },
              { value: 'high', label: 'High' },
            ],
          },
        ],
      },
      {
        title: 'Sampling',
        fields: [{ key: 'top_k', label: 'Top K', type: 'number' }],
      },
    ],
  };

  const OLLAMA = provider({
    id: 'OLLAMA',
    name: 'OLLAMA',
    displayName: 'Ollama',
    configRequirements: { requiresApiKey: false, requiresBaseUrl: true },
    optionsSchema: OLLAMA_SCHEMA,
  });

  function box(fixture: ComponentFixture<ProfileModal>, id: string): HTMLInputElement | null {
    return (fixture.nativeElement as HTMLElement).querySelector<HTMLInputElement>(`#${id}`);
  }

  function ollamaProfile(parameters: Record<string, unknown>): ConnectionProfileDto {
    return {
      id: 'cp-ollama',
      name: 'Local Qwen',
      provider: 'OLLAMA',
      modelName: 'qwen3:8b',
      parameters,
      isDefault: false,
    };
  }

  it('draws nothing when the active provider declares no schema', async () => {
    // GOOGLE is the one bundled plugin without a schema at `93ed8abf`, and the
    // wire serves it as null (Contract B).
    const fixture = await render({ providers: [provider({ optionsSchema: null })] });
    expect(box(fixture, 'pof-enable_thinking')).toBeNull();
    expect(fixture.nativeElement.textContent).not.toContain('Ollama Options');
  });

  it('draws the active provider’s groups and fields, help text and all', async () => {
    const fixture = await render({ profile: ollamaProfile({}), providers: [OLLAMA] });
    expect(fixture.nativeElement.textContent).toContain('Ollama Options');
    expect(fixture.nativeElement.textContent).toContain('Sampling');
    expect(box(fixture, 'pof-enable_thinking')!.checked).toBe(false);
    expect(box(fixture, 'pof-top_k')).not.toBeNull();
    expect(fixture.nativeElement.textContent).toContain(
      'Let thinking-capable models (Qwen3, DeepSeek-R1, and kin) reason before answering.',
    );
  });

  it('reads a stored boolean true, and shows the STRING form as off (v4 `:149`)', async () => {
    const on = await render({
      profile: ollamaProfile({ enable_thinking: true }),
      providers: [OLLAMA],
    });
    expect(box(on, 'pof-enable_thinking')!.checked).toBe(true);

    const str = await render({
      profile: ollamaProfile({ enable_thinking: 'true' }),
      providers: [OLLAMA],
    });
    expect(box(str, 'pof-enable_thinking')!.checked).toBe(false);
  });

  it('a showIf field appears once its guard is satisfied through the panel', async () => {
    const fixture = await render({ profile: ollamaProfile({}), providers: [OLLAMA] });
    expect(box(fixture, 'pof-thinking_effort')).toBeNull();
    const thinking = box(fixture, 'pof-enable_thinking')!;
    thinking.checked = true;
    thinking.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    expect(box(fixture, 'pof-thinking_effort')).not.toBeNull();
  });

  it('a provider switch swaps which schema is drawn', async () => {
    const fixture = await render({ providers: [provider({ optionsSchema: null }), OLLAMA] });
    expect(fixture.nativeElement.textContent).not.toContain('Ollama Options');
    fixture.componentInstance['onProviderChange']('OLLAMA');
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('Ollama Options');
  });

  it('writes into the PRESERVED bag and carries every unrendered key to the save', async () => {
    const dispatchExpect = vi.fn(async (_req: CoreRequest) => ({
      type: 'connectionProfile',
      data: { profile: { id: 'cp-ollama' } },
    }));
    const fixture = await render(
      {
        // `num_ctx` and `enableZDR` have no control in this schema; both must
        // survive the save untouched (mutation guard: rebuilding the bag from
        // the rendered fields reddens this case).
        profile: ollamaProfile({
          temperature: 0.4,
          num_ctx: 40960,
          enableZDR: true,
          enable_thinking: 'true',
        }),
        providers: [OLLAMA],
      },
      stubClient({ dispatchExpect: dispatchExpect as unknown as CoreClient['dispatchExpect'] }),
    );
    const thinking = box(fixture, 'pof-enable_thinking')!;
    thinking.checked = true;
    thinking.dispatchEvent(new Event('change'));
    fixture.detectChanges();

    const submit = Array.from(fixture.nativeElement.querySelectorAll('button')).find(
      (b) => (b as HTMLButtonElement).textContent?.trim() === 'Update Profile',
    ) as HTMLButtonElement;
    submit.click();
    await fixture.whenStable();

    // The edit path fires a models fetch on open, so pick the save by type.
    const calls = dispatchExpect.mock.calls.map(
      (c) => c[0] as unknown as { type: string; profile: Record<string, unknown> },
    );
    const call = calls.find((c) => c.type === 'connectionProfileUpdate')!;
    expect(call).toBeDefined();
    expect(call.profile['parameters']).toEqual({
      temperature: 0.4,
      max_tokens: 1000,
      top_p: 1,
      // The string form is REPLACED by a real boolean, not left as text.
      enable_thinking: true,
      num_ctx: 40960,
      enableZDR: true,
    });
  });

  it('a cleared number field DELETES its key rather than storing a hole (v4 `:208-210`)', async () => {
    const dispatchExpect = vi.fn(async (_req: CoreRequest) => ({
      type: 'connectionProfile',
      data: { profile: { id: 'cp-ollama' } },
    }));
    const fixture = await render(
      {
        profile: ollamaProfile({ top_k: 20, num_ctx: 8192 }),
        providers: [OLLAMA],
      },
      stubClient({ dispatchExpect: dispatchExpect as unknown as CoreClient['dispatchExpect'] }),
    );
    const topK = box(fixture, 'pof-top_k')!;
    topK.value = '';
    topK.dispatchEvent(new Event('input'));
    fixture.detectChanges();

    const submit = Array.from(fixture.nativeElement.querySelectorAll('button')).find(
      (b) => (b as HTMLButtonElement).textContent?.trim() === 'Update Profile',
    ) as HTMLButtonElement;
    submit.click();
    await fixture.whenStable();

    const call = dispatchExpect.mock.calls
      .map((c) => c[0] as unknown as { type: string; profile: Record<string, unknown> })
      .find((c) => c.type === 'connectionProfileUpdate')!;
    const parameters = call.profile['parameters'] as Record<string, unknown>;
    expect('top_k' in parameters).toBe(false);
    expect(parameters['num_ctx']).toBe(8192);
  });
});

/**
 * The `affects: 'modelInput'` directive (OpenRouter's `useCustomModel`). v4
 * reads it straight off the parameters bag rather than through the panel's
 * directive callback (`ProfileModal.tsx:198-203`, `:577-596`).
 */
describe('ProfileModal (the modelInput directive)', () => {
  const OPENROUTER = provider({
    id: 'OPENROUTER',
    name: 'OPENROUTER',
    displayName: 'OpenRouter',
    optionsSchema: {
      groups: [
        {
          title: 'OpenRouter Options',
          fields: [
            {
              key: 'useCustomModel',
              label: 'Use Custom Model ID',
              type: 'boolean',
              default: false,
              affects: 'modelInput',
            },
          ],
        },
      ],
    },
  });

  function orProfile(parameters: Record<string, unknown>): ConnectionProfileDto {
    return {
      id: 'cp-or',
      name: 'Router',
      provider: 'OPENROUTER',
      modelName: 'openai/gpt-4o-mini',
      parameters,
      isDefault: false,
    };
  }

  it('leaves the ordinary model control in place when the flag is absent', async () => {
    const fixture = await render({ profile: orProfile({}), providers: [OPENROUTER] });
    const input = (fixture.nativeElement as HTMLElement).querySelector<HTMLInputElement>(
      '#qt-pf-model',
    )!;
    expect(input.placeholder).toBe('e.g., gpt-4');
  });

  it('swaps in the free-text model entry the moment the flag is set (v4 `:577`)', async () => {
    const fixture = await render({ profile: orProfile({}), providers: [OPENROUTER] });
    const flag = (fixture.nativeElement as HTMLElement).querySelector<HTMLInputElement>(
      '#pof-useCustomModel',
    )!;
    flag.checked = true;
    flag.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    const input = (fixture.nativeElement as HTMLElement).querySelector<HTMLInputElement>(
      '#qt-pf-model',
    )!;
    expect(input.placeholder).toBe('e.g., openai/gpt-4-turbo');
  });

  it('honours only a real boolean true, as v4’s guard does (v4 `:203`)', async () => {
    const fixture = await render({
      profile: orProfile({ useCustomModel: 'true' }),
      providers: [OPENROUTER],
    });
    const input = (fixture.nativeElement as HTMLElement).querySelector<HTMLInputElement>(
      '#qt-pf-model',
    )!;
    expect(input.placeholder).toBe('e.g., gpt-4');
  });
});
