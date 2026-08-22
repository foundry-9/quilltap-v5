import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type {
  ApiKeyDto,
  ConnectionProfileDto,
  CoreRequest,
  ProviderInfo,
} from '../../../core/core-contract';
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
    apiKeys?: ApiKeyDto[];
  },
  client: Partial<CoreClient> = stubClient(),
): Promise<ComponentFixture<ProfileModal>> {
  // Reset first: a couple of cases render twice in one `it` to compare two
  // bags, and TestBed refuses a second `configureTestingModule` once
  // instantiated.
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [ProfileModal],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(ProfileModal);
  fixture.componentRef.setInput('profile', inputs.profile ?? null);
  fixture.componentRef.setInput('providers', inputs.providers ?? [provider({})]);
  fixture.componentRef.setInput('apiKeys', inputs.apiKeys ?? []);
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
 * The thinking-turn half of the prefill seed (v4 bug 85, `97d2fcb5`): the
 * model-change re-seed, the stored-null ONCE-correction on model-list-landing,
 * and the thinking warning — v4 `ProfileModal.tsx:226-254`, `:316-334`,
 * `:900-910`. Everything below also proves the graceful-absence contract: a
 * wire that omits `thinkingTurnRule` and the model facts must behave exactly
 * like the pre-thinking editor (the state this worktree's own server serves
 * until P4.D97 unifies).
 */
describe('ProfileModal (thinking-turn seeds and warning, bug 85)', () => {
  // v4's paragraph byte-for-byte, whitespace-normalized the way HTML renders
  // it (JSX and Angular templates collapse the same way).
  const THINKING_WARNING =
    'This model reasons before it answers, and a turn handed over already opened sits badly ' +
    'with that: some providers refuse the request outright, others quietly swallow the ' +
    'reasoning altogether. A model that deliberates over whose turn it is rarely needs the ' +
    'name put in its mouth — leave this unticked unless you have watched yours cope.';

  /** The rule the DeepSeek plugin declares at `12fe3e6f`. */
  const DEEPSEEK = provider({
    id: 'DEEPSEEK',
    name: 'DEEPSEEK',
    displayName: 'DeepSeek',
    thinkingTurnRule: {
      optionKey: 'thinking',
      enabledValues: ['enabled'],
      disabledValues: ['disabled'],
    },
  });

  /** The same provider as a rule-less wire serves it (pre-P4.D97). */
  const DEEPSEEK_NO_RULE = provider({ id: 'DEEPSEEK', name: 'DEEPSEEK', displayName: 'DeepSeek' });

  const THINKING_MODELS = {
    models: ['deepseek-v4-flash', 'deepseek-chat'],
    modelsWithInfo: [
      { id: 'deepseek-v4-flash', supportsThinking: true, thinksByDefault: true },
      { id: 'deepseek-chat' },
    ],
  };

  /** Facts-less info rows, as a wire that has not learned them serves. */
  const FACTLESS_MODELS = {
    models: ['deepseek-v4-flash', 'deepseek-chat'],
    modelsWithInfo: [{ id: 'deepseek-v4-flash' }, { id: 'deepseek-chat' }],
  };

  function modelsClient(payload: unknown = THINKING_MODELS): {
    client: Partial<CoreClient>;
    dispatchExpect: ReturnType<typeof vi.fn>;
  } {
    const dispatchExpect = vi.fn(async (req: CoreRequest) => {
      if ((req as { type: string }).type === 'modelFetch') {
        return { type: 'models', data: payload };
      }
      return { type: 'connectionProfile', data: { profile: { id: 'new' } } };
    });
    return {
      client: stubClient({
        dispatchExpect: dispatchExpect as unknown as CoreClient['dispatchExpect'],
      }),
      dispatchExpect,
    };
  }

  function prefillBox(fixture: ComponentFixture<ProfileModal>): HTMLInputElement {
    const label = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('label')).find(
      (l) =>
        l.textContent?.includes('Announce the speaker in multi-character scenes ([Name] prefill)'),
    );
    return label!.querySelector('input')!;
  }

  function normalizedText(fixture: ComponentFixture<ProfileModal>): string {
    return ((fixture.nativeElement as HTMLElement).textContent ?? '').replace(/\s+/g, ' ');
  }

  async function landModels(fixture: ComponentFixture<ProfileModal>): Promise<void> {
    await fixture.componentInstance['fetchModels']();
    fixture.detectChanges();
  }

  const NULL_ROW: ConnectionProfileDto = {
    id: 'cp-null',
    name: 'Never chose',
    provider: 'DEEPSEEK',
    modelName: 'deepseek-v4-flash',
    parameters: {},
    isDefault: false,
    multiCharacterPrefill: null,
  };

  it('a model pick re-seeds a NEW profile off for a model that reasons unasked — and back on', async () => {
    const { client } = modelsClient();
    const fixture = await render({ providers: [DEEPSEEK] }, client);
    fixture.componentInstance['onProviderChange']('DEEPSEEK');
    await landModels(fixture);
    expect(prefillBox(fixture).checked).toBe(true);

    fixture.componentInstance['onModelChange']('deepseek-v4-flash');
    fixture.detectChanges();
    expect(prefillBox(fixture).checked).toBe(false);

    // A seed, never a one-way latch: a model with no stated habit seeds it
    // back on.
    fixture.componentInstance['onModelChange']('deepseek-chat');
    fixture.detectChanges();
    expect(prefillBox(fixture).checked).toBe(true);
  });

  it('an explicit thinking-off profile choice keeps the seed on (bug 68 preserved)', async () => {
    const { client } = modelsClient();
    const fixture = await render({ providers: [DEEPSEEK] }, client);
    fixture.componentInstance['onProviderChange']('DEEPSEEK');
    await landModels(fixture);
    fixture.componentInstance['setParameter']('thinking', 'disabled');
    fixture.componentInstance['onModelChange']('deepseek-v4-flash');
    fixture.detectChanges();
    expect(prefillBox(fixture).checked).toBe(true);
  });

  it('does NOT re-seed on an EXISTING profile (v4 guards its re-seed `!profile?.id`)', async () => {
    const { client } = modelsClient();
    const fixture = await render(
      {
        profile: { ...NULL_ROW, multiCharacterPrefill: true },
        providers: [DEEPSEEK],
      },
      client,
    );
    fixture.componentInstance['onModelChange']('deepseek-v4-flash');
    fixture.detectChanges();
    expect(prefillBox(fixture).checked).toBe(true);
  });

  it('the seed is never a clamp — the box can be ticked straight back', async () => {
    const { client } = modelsClient();
    const fixture = await render({ providers: [DEEPSEEK] }, client);
    fixture.componentInstance['onProviderChange']('DEEPSEEK');
    await landModels(fixture);
    fixture.componentInstance['onModelChange']('deepseek-v4-flash');
    fixture.detectChanges();
    expect(prefillBox(fixture).checked).toBe(false);

    const box = prefillBox(fixture);
    box.checked = true;
    box.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    expect(prefillBox(fixture).checked).toBe(true);
  });

  it('warns, byte for byte, when the box is ticked on a thinking profile', async () => {
    const { client } = modelsClient();
    const fixture = await render({ providers: [DEEPSEEK] }, client);
    fixture.componentInstance['onProviderChange']('DEEPSEEK');
    await landModels(fixture);
    fixture.componentInstance['onModelChange']('deepseek-v4-flash');
    fixture.detectChanges();
    expect(normalizedText(fixture)).not.toContain(THINKING_WARNING);

    const box = prefillBox(fixture);
    box.checked = true;
    box.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    expect(normalizedText(fixture)).toContain(THINKING_WARNING);

    box.checked = false;
    box.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    expect(normalizedText(fixture)).not.toContain(THINKING_WARNING);
  });

  it('corrects a stored-null row once the model list lands, through the edit-time fetch', async () => {
    const { client } = modelsClient();
    const fixture = await render({ profile: NULL_ROW, providers: [DEEPSEEK] }, client);
    // ngOnInit's autoFetchModelsForEdit landed during render's whenStable —
    // the provider-default seed (on) has been corrected to the thinking
    // answer (off).
    expect(prefillBox(fixture).checked).toBe(false);
  });

  it('corrects through the MANUAL Fetch Models landing when the edit-time fetch failed', async () => {
    // Both model-list-landing sites carry the correction: a null row whose
    // auto-fetch died still corrects the moment the user fetches by hand.
    let calls = 0;
    const dispatchExpect = vi.fn(async (req: CoreRequest) => {
      if ((req as { type: string }).type === 'modelFetch') {
        calls += 1;
        if (calls === 1) throw new Error('endpoint asleep');
        return { type: 'models', data: THINKING_MODELS };
      }
      return { type: 'connectionProfile', data: { profile: { id: 'new' } } };
    });
    const fixture = await render(
      { profile: NULL_ROW, providers: [DEEPSEEK] },
      stubClient({ dispatchExpect: dispatchExpect as unknown as CoreClient['dispatchExpect'] }),
    );
    // The auto-fetch failed silently; the seed is still the provider default.
    expect(prefillBox(fixture).checked).toBe(true);

    await landModels(fixture);
    expect(prefillBox(fixture).checked).toBe(false);
  });

  it('the correction fires AT MOST ONCE — a later landing cannot fight the checkbox', async () => {
    const { client } = modelsClient();
    const fixture = await render({ profile: NULL_ROW, providers: [DEEPSEEK] }, client);
    expect(prefillBox(fixture).checked).toBe(false);

    // The user overrules the correction...
    const box = prefillBox(fixture);
    box.checked = true;
    box.dispatchEvent(new Event('change'));
    fixture.detectChanges();

    // ...and a second model-list landing (Fetch Models) must NOT re-correct.
    await landModels(fixture);
    expect(prefillBox(fixture).checked).toBe(true);
  });

  it('can NEVER fire on a row whose stored value is a boolean', async () => {
    const { client } = modelsClient();
    // A stored true on a thinking model is the user's own overrule — the
    // tri-state exists so they may. The landing must leave it alone (and the
    // warning, not a veto, is what they get).
    const fixture = await render(
      { profile: { ...NULL_ROW, multiCharacterPrefill: true }, providers: [DEEPSEEK] },
      client,
    );
    expect(prefillBox(fixture).checked).toBe(true);
    expect(normalizedText(fixture)).toContain(THINKING_WARNING);

    const { client: falseClient } = modelsClient();
    const fixture2 = await render(
      { profile: { ...NULL_ROW, multiCharacterPrefill: false }, providers: [DEEPSEEK] },
      falseClient,
    );
    expect(prefillBox(fixture2).checked).toBe(false);
  });

  it('degrades to the provider-rule-only seed when the wire omits rule and facts', async () => {
    // The pre-P4.D97 state on this worktree's own server: no thinkingTurnRule
    // on the provider, no facts on the models. Behavior must equal the
    // pre-thinking editor exactly — seed on, no correction, no warning.
    const { client } = modelsClient(FACTLESS_MODELS);
    const fixture = await render({ providers: [DEEPSEEK_NO_RULE] }, client);
    fixture.componentInstance['onProviderChange']('DEEPSEEK');
    await landModels(fixture);
    fixture.componentInstance['onModelChange']('deepseek-v4-flash');
    fixture.detectChanges();
    expect(prefillBox(fixture).checked).toBe(true);

    const box = prefillBox(fixture);
    box.checked = true;
    box.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    expect(normalizedText(fixture)).not.toContain(THINKING_WARNING);

    // And a stored-null row is left at the provider default, as today.
    const { client: nullClient } = modelsClient(FACTLESS_MODELS);
    const fixture2 = await render(
      { profile: NULL_ROW, providers: [DEEPSEEK_NO_RULE] },
      nullClient,
    );
    expect(prefillBox(fixture2).checked).toBe(true);
  });
});

/**
 * The tool-use seed hint and the two capability seeds on provider change
 * (Contract C + v4 `ProfileModal.tsx:228-243`, `:742-748`, bug 71).
 */
describe('ProfileModal (tool-use seed hint and capability seeds)', () => {
  const HINT =
    'This provider does not advertise tool support, so new profiles start with it off. ' +
    'If your endpoint does speak native function calling — llama-server with --jinja, ' +
    'vLLM, LM Studio — you may turn it on regardless.';

  const OAC = provider({
    id: 'OPENAI_COMPATIBLE',
    name: 'OPENAI_COMPATIBLE',
    displayName: 'OpenAI Compatible',
    capabilities: {
      chat: true,
      imageGeneration: false,
      embeddings: false,
      webSearch: false,
      toolUse: false,
    },
    configRequirements: { requiresApiKey: false, requiresBaseUrl: true },
  });
  const TOOLED = provider({
    capabilities: {
      chat: true,
      imageGeneration: false,
      embeddings: false,
      webSearch: false,
      toolUse: true,
    },
  });

  /** Collapse template whitespace the way the browser renders it. */
  function flat(fixture: ComponentFixture<ProfileModal>): string {
    return ((fixture.nativeElement as HTMLElement).textContent ?? '').replace(/\s+/g, ' ');
  }

  function toolBox(fixture: ComponentFixture<ProfileModal>): HTMLInputElement {
    const label = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('label')).find(
      (l) => l.textContent?.includes('Allow tool use'),
    );
    return label!.querySelector('input')!;
  }

  function imageBox(fixture: ComponentFixture<ProfileModal>): HTMLInputElement {
    const label = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('label')).find(
      (l) => l.textContent?.includes('Supports image attachments'),
    );
    return label!.querySelector('input')!;
  }

  it('renders the hint verbatim when the capability is false', async () => {
    const fixture = await render({ providers: [OAC] });
    fixture.componentInstance['onProviderChange']('OPENAI_COMPATIBLE');
    fixture.detectChanges();
    expect(flat(fixture)).toContain(HINT);
  });

  it('does not render it for a provider that does advertise tool support', async () => {
    const fixture = await render({ providers: [TOOLED] });
    expect(flat(fixture)).not.toContain('does not advertise tool support');
  });

  it('leaves the box editable — the capability is a SEED, never a clamp', async () => {
    const fixture = await render({ providers: [OAC] });
    fixture.componentInstance['onProviderChange']('OPENAI_COMPATIBLE');
    fixture.detectChanges();
    const box = toolBox(fixture);
    expect(box.checked).toBe(false);
    expect(box.disabled).toBe(false);
    box.checked = true;
    box.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    expect(toolBox(fixture).checked).toBe(true);
    // The hint stays put; it explains the seed, it does not track the box.
    expect(flat(fixture)).toContain('you may turn it on regardless');
  });

  it('re-seeds supportsImageUpload from the new provider on a NEW profile (v4 `:238-241`)', async () => {
    const OLLAMA = provider({
      id: 'OLLAMA',
      name: 'OLLAMA',
      displayName: 'Ollama',
      configRequirements: { requiresApiKey: false, requiresBaseUrl: true },
    });
    const fixture = await render({ providers: [provider({}), OLLAMA] });
    // OPENAI accepts image/jpeg in v4's table; OLLAMA does not.
    fixture.componentInstance['onProviderChange']('OPENAI');
    fixture.detectChanges();
    expect(imageBox(fixture).checked).toBe(true);
    fixture.componentInstance['onProviderChange']('OLLAMA');
    fixture.detectChanges();
    expect(imageBox(fixture).checked).toBe(false);
  });

  it('does NOT re-seed it on an EXISTING profile (v4 keeps it inside the `:235` guard)', async () => {
    const fixture = await render({
      profile: {
        id: 'cp1',
        name: 'Saved',
        provider: 'OPENAI',
        modelName: 'gpt-4',
        parameters: {},
        isDefault: false,
        supportsImageUpload: true,
      },
      providers: [
        provider({}),
        provider({
          id: 'OLLAMA',
          name: 'OLLAMA',
          displayName: 'Ollama',
          configRequirements: { requiresApiKey: false, requiresBaseUrl: true },
        }),
      ],
    });
    expect(imageBox(fixture).checked).toBe(true);
    fixture.componentInstance['onProviderChange']('OLLAMA');
    fixture.detectChanges();
    expect(imageBox(fixture).checked).toBe(true);
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

/**
 * Bug 73 — a base URL picked up from one provider must not follow the profile
 * onto a provider that neither shows nor takes one.
 *
 * These mirror v4's `__tests__/unit/components/settings/profile-modal-base-url.
 * test.tsx` at `d123658d` case for case, driven through the real gesture (the
 * provider dropdown) with the dispatch layer captured, so every assertion is on
 * the bytes that actually leave the form.
 *
 * v5 has FIVE outbound sites where v4 names four: the three form-driven probes
 * (connect / fetch models / test message), the save body, and the modal's
 * edit-time model fetch, which reads the SAVED profile rather than form state
 * and resolves its own requirement.
 */
describe('ProfileModal base URL (Bug 73)', () => {
  const OPENAI = provider({
    id: 'OPENAI',
    name: 'OPENAI',
    displayName: 'OpenAI',
    configRequirements: { requiresApiKey: true, requiresBaseUrl: false },
  });
  const OLLAMA = provider({
    id: 'OLLAMA',
    name: 'OLLAMA',
    displayName: 'Ollama',
    configRequirements: {
      requiresApiKey: false,
      requiresBaseUrl: true,
      baseUrlDefault: 'http://localhost:11434',
    },
  });
  const OAC = provider({
    id: 'OPENAI_COMPATIBLE',
    name: 'OPENAI_COMPATIBLE',
    displayName: 'OpenAI Compatible',
    configRequirements: {
      requiresApiKey: false,
      requiresBaseUrl: true,
      baseUrlDefault: 'http://localhost:8080/v1',
    },
  });
  const PROVIDERS = [OPENAI, OLLAMA, OAC];

  /** Every request the modal dispatched, in order, both dispatch entry points. */
  function recorder(): { client: Partial<CoreClient>; seen: CoreRequest[] } {
    const seen: CoreRequest[] = [];
    const client = stubClient({
      dispatchExpect: vi.fn(async (req: CoreRequest) => {
        seen.push(req);
        return req.type === 'modelFetch'
          ? { type: 'models', data: { models: [] } }
          : { type: 'connectionProfile', data: { profile: { id: 'new' } } };
      }) as unknown as CoreClient['dispatchExpect'],
      dispatchData: vi.fn(async (req: CoreRequest) => {
        seen.push(req);
        return { valid: true, message: 'ok', success: true };
      }) as unknown as CoreClient['dispatchData'],
    });
    return { client, seen };
  }

  function first(seen: CoreRequest[], type: string): Record<string, unknown> {
    const found = seen.find((r) => r.type === type);
    if (!found)
      throw new Error(`no ${type} dispatched (saw ${seen.map((r) => r.type).join(', ')})`);
    return found as unknown as Record<string, unknown>;
  }

  /** The real gesture: pick a provider from the dropdown. */
  function selectProvider(fixture: ComponentFixture<ProfileModal>, name: string): void {
    const select = (fixture.nativeElement as HTMLElement).querySelector<HTMLSelectElement>(
      '#qt-pf-provider',
    )!;
    select.value = name;
    select.dispatchEvent(new Event('change'));
    fixture.detectChanges();
  }

  function baseUrlBox(fixture: ComponentFixture<ProfileModal>): HTMLInputElement | null {
    return (fixture.nativeElement as HTMLElement).querySelector<HTMLInputElement>('#qt-pf-baseurl');
  }

  it('fills the base URL for Ollama and hides it again on OpenAI', async () => {
    const fixture = await render({ providers: PROVIDERS });
    selectProvider(fixture, 'OLLAMA');
    expect(baseUrlBox(fixture)!.value).toBe('http://localhost:11434');

    selectProvider(fixture, 'OPENAI');
    expect(baseUrlBox(fixture)).toBeNull();
  });

  it('does not send the stale base URL when connecting on the new provider', async () => {
    const { client, seen } = recorder();
    const fixture = await render({ providers: PROVIDERS }, client);
    selectProvider(fixture, 'OLLAMA');
    selectProvider(fixture, 'OPENAI');
    fixture.componentInstance['setField']('apiKeyId', 'key-openai');

    await fixture.componentInstance['connect']();
    const req = first(seen, 'connectionProfileTest');
    const sent = req['profile'] as Record<string, unknown>;
    expect(sent['provider']).toBe('OPENAI');
    expect(sent['baseUrl']).toBeUndefined();
  });

  it('does not send the stale base URL when fetching models or testing a message', async () => {
    const { client, seen } = recorder();
    const fixture = await render({ providers: PROVIDERS }, client);
    selectProvider(fixture, 'OLLAMA');
    selectProvider(fixture, 'OPENAI');
    fixture.componentInstance['setField']('modelName', 'gpt-4');

    await fixture.componentInstance['fetchModels']();
    expect(first(seen, 'modelFetch')['baseUrl']).toBeUndefined();

    await fixture.componentInstance['testMessage']();
    const message = first(seen, 'connectionProfileTestMessage')['profile'] as Record<
      string,
      unknown
    >;
    expect(message['baseUrl']).toBeUndefined();
  });

  it('saves the profile with an empty base URL, so the row cannot be written broken', async () => {
    const { client, seen } = recorder();
    const fixture = await render({ providers: PROVIDERS }, client);
    selectProvider(fixture, 'OLLAMA');
    selectProvider(fixture, 'OPENAI');
    typeName(fixture, 'Hosted');
    fixture.componentInstance['setField']('modelName', 'gpt-4');
    fixture.detectChanges();

    await fixture.componentInstance['submit']();
    const body = first(seen, 'connectionProfileCreate')['profile'] as Record<string, unknown>;
    expect(body['provider']).toBe('OPENAI');
    expect(Object.prototype.hasOwnProperty.call(body, 'baseUrl')).toBe(true);
    expect(body['baseUrl']).toBe('');
  });

  it('still sends the base URL on a provider that takes one', async () => {
    const { client, seen } = recorder();
    const fixture = await render({ providers: PROVIDERS }, client);
    selectProvider(fixture, 'OLLAMA');

    await fixture.componentInstance['connect']();
    const sent = first(seen, 'connectionProfileTest')['profile'] as Record<string, unknown>;
    expect(sent['baseUrl']).toBe('http://localhost:11434');
  });

  it('leaves a stored base URL alone when the provider list has not loaded', async () => {
    // The card renders the modal before the providers listing answers, and the
    // fetch can fail outright. An unknown provider is not evidence that it takes
    // no base URL, so an existing Ollama profile must not be cleared by a save.
    const { client, seen } = recorder();
    const fixture = await render(
      {
        providers: [],
        profile: {
          id: 'cp1',
          name: 'Local',
          provider: 'OLLAMA',
          modelName: 'qwen3',
          baseUrl: 'http://localhost:11434',
          parameters: {},
          isDefault: false,
        },
      },
      client,
    );
    await fixture.componentInstance['submit']();
    const body = first(seen, 'connectionProfileUpdate')['profile'] as Record<string, unknown>;
    expect(body['baseUrl']).toBe('http://localhost:11434');
  });

  it('does not swap a typed endpoint for ollama’s default on the way back', async () => {
    const fixture = await render({ providers: PROVIDERS });
    selectProvider(fixture, 'OPENAI_COMPATIBLE');
    const box = baseUrlBox(fixture)!;
    box.value = 'http://box.local:9090/v1';
    box.dispatchEvent(new Event('input'));
    fixture.detectChanges();

    selectProvider(fixture, 'OLLAMA');
    selectProvider(fixture, 'OPENAI_COMPATIBLE');

    expect(baseUrlBox(fixture)!.value).toBe('http://box.local:9090/v1');
  });

  it('keeps the hidden value in form state, so switching back restores it', async () => {
    // v4's comment on the chokepoint: "The value stays in form state so
    // switching back restores it; it simply never reaches the wire."
    const fixture = await render({ providers: PROVIDERS });
    selectProvider(fixture, 'OLLAMA');
    selectProvider(fixture, 'OPENAI');
    expect(baseUrlBox(fixture)).toBeNull();
    selectProvider(fixture, 'OLLAMA');
    expect(baseUrlBox(fixture)!.value).toBe('http://localhost:11434');
  });

  it('keeps the edit-time model fetch off a poisoned row’s endpoint', async () => {
    // v5's fifth site (v4 `ProfileModal.tsx:73-80`): the auto-fetch on open
    // reads the SAVED profile, so a row written before the fix must not point
    // the probe at the wrong endpoint.
    const { client, seen } = recorder();
    await render(
      {
        providers: PROVIDERS,
        profile: {
          id: 'cp1',
          name: 'Poisoned',
          provider: 'OPENAI',
          modelName: 'gpt-4',
          baseUrl: 'http://localhost:11434',
          parameters: {},
          isDefault: false,
        },
      },
      client,
    );
    expect(first(seen, 'modelFetch')['baseUrl']).toBeUndefined();
  });

  it('sends the saved base URL when the saved provider does take one', async () => {
    const { client, seen } = recorder();
    await render(
      {
        providers: PROVIDERS,
        profile: {
          id: 'cp1',
          name: 'Local',
          provider: 'OLLAMA',
          modelName: 'qwen3',
          baseUrl: 'http://localhost:11434',
          parameters: {},
          isDefault: false,
        },
      },
      client,
    );
    expect(first(seen, 'modelFetch')['baseUrl']).toBe('http://localhost:11434');
  });

  it('sends an unknown saved provider’s base URL rather than clearing the probe', async () => {
    const { client, seen } = recorder();
    await render(
      {
        providers: [],
        profile: {
          id: 'cp1',
          name: 'Local',
          provider: 'OLLAMA',
          modelName: 'qwen3',
          baseUrl: 'http://localhost:11434',
          parameters: {},
          isDefault: false,
        },
      },
      client,
    );
    expect(first(seen, 'modelFetch')['baseUrl']).toBe('http://localhost:11434');
  });
});

/**
 * The `Non-image attachments:` line under the provider select (v4
 * `ProfileModal.tsx:379-381`), banked at P4.D84 and landed with this round's
 * same-family work. The `baseUrl` argument v4 passes is unread by the static
 * table on both sides, so v5 omits it as it does for `supportsMimeType`.
 */
describe('ProfileModal (attachment support line)', () => {
  function flatText(fixture: ComponentFixture<ProfileModal>): string {
    return ((fixture.nativeElement as HTMLElement).textContent ?? '').replace(/\s+/g, ' ');
  }

  it('names the active provider’s attachment support, and follows the dropdown', async () => {
    const fixture = await render({
      providers: [
        provider({ id: 'OPENAI', name: 'OPENAI', displayName: 'OpenAI' }),
        provider({ id: 'ANTHROPIC', name: 'ANTHROPIC', displayName: 'Anthropic' }),
        provider({ id: 'OLLAMA', name: 'OLLAMA', displayName: 'Ollama' }),
      ],
    });
    expect(flatText(fixture)).toContain('Non-image attachments: Images (JPEG, PNG, GIF, WEBP)');

    fixture.componentInstance['onProviderChange']('ANTHROPIC');
    fixture.detectChanges();
    expect(flatText(fixture)).toContain(
      'Non-image attachments: Images (JPEG, PNG, GIF, WEBP), PDF documents, Text files (TXT)',
    );

    fixture.componentInstance['onProviderChange']('OLLAMA');
    fixture.detectChanges();
    expect(flatText(fixture)).toContain('Non-image attachments: No file attachments supported');
  });

  it('does not draw the line on the courier transport (v4 gates it on !isCourier)', async () => {
    const fixture = await render({});
    fixture.componentInstance['setField']('transport', 'courier');
    fixture.detectChanges();
    expect(flatText(fixture)).not.toContain('Non-image attachments:');
  });
});

/**
 * Bug 76 — an api key chosen for one provider must not follow the profile onto
 * a provider that neither shows it nor can use it.
 *
 * These mirror v4's `__tests__/unit/components/settings/profile-modal-api-key.
 * test.tsx` at `8bd802a3` case for case (all seven), driven through the real
 * gestures — the provider dropdown and the API Key select — with the dispatch
 * layer captured, so every assertion is on the bytes that actually leave the
 * form. Four of v4's seven fail pre-fix.
 *
 * v5 has FIVE outbound sites where v4 names four: the three form-driven probes
 * (connect / fetch models / test message), the save body, and the modal's
 * edit-time model fetch, which reads the SAVED profile rather than form state
 * and resolves its own requirement.
 *
 * Closes this port's dogfood finding #90.
 */
describe('ProfileModal api key (Bug 76)', () => {
  const ANTHROPIC = provider({
    id: 'ANTHROPIC',
    name: 'ANTHROPIC',
    displayName: 'Anthropic',
    configRequirements: { requiresApiKey: true, requiresBaseUrl: false },
  });
  const OPENAI = provider({
    id: 'OPENAI',
    name: 'OPENAI',
    displayName: 'OpenAI',
    configRequirements: { requiresApiKey: true, requiresBaseUrl: false },
  });
  const OLLAMA = provider({
    id: 'OLLAMA',
    name: 'OLLAMA',
    displayName: 'Ollama',
    configRequirements: {
      requiresApiKey: false,
      requiresBaseUrl: true,
      baseUrlDefault: 'http://localhost:11434',
    },
  });
  const PROVIDERS = [ANTHROPIC, OPENAI, OLLAMA];

  function apiKey(id: string, prov: string, label: string): ApiKeyDto {
    return {
      id,
      provider: prov,
      label,
      isActive: true,
      lastUsed: null,
      createdAt: '2026-01-01T00:00:00.000Z',
      updatedAt: '2026-01-01T00:00:00.000Z',
      keyPreview: 'sk-…abcd',
    };
  }

  const API_KEYS: ApiKeyDto[] = [
    apiKey('key-anthropic', 'ANTHROPIC', 'Anthropic key'),
    apiKey('key-openai', 'OPENAI', 'OpenAI key'),
  ];

  /** Every request the modal dispatched, in order, both dispatch entry points. */
  function recorder(): { client: Partial<CoreClient>; seen: CoreRequest[] } {
    const seen: CoreRequest[] = [];
    const client = stubClient({
      dispatchExpect: vi.fn(async (req: CoreRequest) => {
        seen.push(req);
        return req.type === 'modelFetch'
          ? { type: 'models', data: { models: [] } }
          : { type: 'connectionProfile', data: { profile: { id: 'new' } } };
      }) as unknown as CoreClient['dispatchExpect'],
      dispatchData: vi.fn(async (req: CoreRequest) => {
        seen.push(req);
        return { valid: true, message: 'ok', success: true };
      }) as unknown as CoreClient['dispatchData'],
    });
    return { client, seen };
  }

  function find(seen: CoreRequest[], type: string): Record<string, unknown> | undefined {
    return seen.find((r) => r.type === type) as unknown as Record<string, unknown> | undefined;
  }

  function first(seen: CoreRequest[], type: string): Record<string, unknown> {
    const found = find(seen, type);
    if (!found)
      throw new Error(`no ${type} dispatched (saw ${seen.map((r) => r.type).join(', ')})`);
    return found;
  }

  /** The real gesture: pick a provider from the dropdown. */
  function selectProvider(fixture: ComponentFixture<ProfileModal>, name: string): void {
    const select = (fixture.nativeElement as HTMLElement).querySelector<HTMLSelectElement>(
      '#qt-pf-provider',
    )!;
    select.value = name;
    select.dispatchEvent(new Event('change'));
    fixture.detectChanges();
  }

  function keySelect(fixture: ComponentFixture<ProfileModal>): HTMLSelectElement | null {
    return (fixture.nativeElement as HTMLElement).querySelector<HTMLSelectElement>('#qt-pf-key');
  }

  /** The real gesture: pick an api key from the select. */
  function selectApiKey(fixture: ComponentFixture<ProfileModal>, id: string): void {
    const select = keySelect(fixture)!;
    select.value = id;
    select.dispatchEvent(new Event('change'));
    fixture.detectChanges();
  }

  it('saves no api key after ANTHROPIC → OLLAMA, where the select is not even rendered', async () => {
    const { client, seen } = recorder();
    const fixture = await render({ providers: PROVIDERS, apiKeys: API_KEYS }, client);
    selectProvider(fixture, 'ANTHROPIC');
    selectApiKey(fixture, 'key-anthropic');

    selectProvider(fixture, 'OLLAMA');
    expect(keySelect(fixture)).toBeNull();

    typeName(fixture, 'Local');
    fixture.componentInstance['setField']('modelName', 'qwen3');
    fixture.detectChanges();

    await fixture.componentInstance['submit']();
    const body = first(seen, 'connectionProfileCreate')['profile'] as Record<string, unknown>;
    expect(body['provider']).toBe('OLLAMA');
    expect(body['apiKeyId']).toBeNull();
  });

  it('sends no api key on a hosted → hosted switch, where the select reads blank', async () => {
    const { client, seen } = recorder();
    const fixture = await render({ providers: PROVIDERS, apiKeys: API_KEYS }, client);
    selectProvider(fixture, 'ANTHROPIC');
    selectApiKey(fixture, 'key-anthropic');

    selectProvider(fixture, 'OPENAI');
    // The stored id is not among OpenAI's options, so the control shows blank.
    expect(keySelect(fixture)!.value).toBe('');

    await fixture.componentInstance['connect']();

    // Nothing reaches the wire: the blank control is taken at its word, and the
    // form says so rather than probing OpenAI with an Anthropic key.
    fixture.detectChanges();
    expect((fixture.nativeElement as HTMLElement).textContent).toContain(
      'API Key is required for this provider',
    );
    expect(find(seen, 'connectionProfileTest')).toBeUndefined();
  });

  it('clears the column on save after a hosted → hosted switch', async () => {
    const { client, seen } = recorder();
    const fixture = await render({ providers: PROVIDERS, apiKeys: API_KEYS }, client);
    selectProvider(fixture, 'ANTHROPIC');
    selectApiKey(fixture, 'key-anthropic');
    selectProvider(fixture, 'OPENAI');

    typeName(fixture, 'Hosted');
    fixture.componentInstance['setField']('modelName', 'gpt-4');
    fixture.detectChanges();

    await fixture.componentInstance['submit']();
    const body = first(seen, 'connectionProfileCreate')['profile'] as Record<string, unknown>;
    expect(body['provider']).toBe('OPENAI');
    expect(Object.prototype.hasOwnProperty.call(body, 'apiKeyId')).toBe(true);
    expect(body['apiKeyId']).toBeNull();
  });

  it('still sends the key the user picked for the provider that takes it', async () => {
    const { client, seen } = recorder();
    const fixture = await render({ providers: PROVIDERS, apiKeys: API_KEYS }, client);
    selectProvider(fixture, 'OPENAI');
    selectApiKey(fixture, 'key-openai');

    await fixture.componentInstance['connect']();
    const sent = first(seen, 'connectionProfileTest')['profile'] as Record<string, unknown>;
    expect(sent['provider']).toBe('OPENAI');
    expect(sent['apiKeyId']).toBe('key-openai');
  });

  it('restores the remembered key when the provider is switched back', async () => {
    const { client, seen } = recorder();
    const fixture = await render({ providers: PROVIDERS, apiKeys: API_KEYS }, client);
    selectProvider(fixture, 'ANTHROPIC');
    selectApiKey(fixture, 'key-anthropic');
    selectProvider(fixture, 'OLLAMA');
    selectProvider(fixture, 'ANTHROPIC');

    // `onProviderChange` never clears the field — the value is inert while it
    // cannot be shown, not destroyed.
    expect(keySelect(fixture)!.value).toBe('key-anthropic');

    await fixture.componentInstance['connect']();
    const sent = first(seen, 'connectionProfileTest')['profile'] as Record<string, unknown>;
    expect(sent['apiKeyId']).toBe('key-anthropic');
  });

  it('leaves a stored key alone when neither list has loaded', async () => {
    // The card renders the modal before the providers listing and the key list
    // answer, and either fetch can fail outright. Absence is not evidence — an
    // existing profile must not be cleared of its key by an ordinary save.
    const { client, seen } = recorder();
    const fixture = await render(
      {
        providers: [],
        apiKeys: [],
        profile: {
          id: 'cp1',
          name: 'Hosted',
          provider: 'ANTHROPIC',
          modelName: 'claude-sonnet-4-5-20250929',
          apiKeyId: 'key-anthropic',
          parameters: {},
          isDefault: false,
        },
      },
      client,
    );
    await fixture.componentInstance['submit']();
    const body = first(seen, 'connectionProfileUpdate')['profile'] as Record<string, unknown>;
    expect(body['apiKeyId']).toBe('key-anthropic');
  });

  it('heals a row already written with a mismatched key on its next ordinary save', async () => {
    // Written before this fix, or by import: the row is on OLLAMA and still
    // carries an Anthropic key. The update handler gates on
    // `apiKeyId !== undefined`, so the save must send `null` rather than
    // omitting the field, or the row stays broken and refused forever.
    const { client, seen } = recorder();
    const fixture = await render(
      {
        providers: PROVIDERS,
        apiKeys: API_KEYS,
        profile: {
          id: 'cp1',
          name: 'Local',
          provider: 'OLLAMA',
          modelName: 'qwen3',
          apiKeyId: 'key-anthropic',
          baseUrl: 'http://localhost:11434',
          parameters: {},
          isDefault: false,
        },
      },
      client,
    );
    await fixture.componentInstance['submit']();
    const body = first(seen, 'connectionProfileUpdate')['profile'] as Record<string, unknown>;
    expect(Object.prototype.hasOwnProperty.call(body, 'apiKeyId')).toBe(true);
    expect(body['apiKeyId']).toBeNull();
  });

  it('keeps the edit-time model fetch off a poisoned row’s key', async () => {
    // v5's fifth site (v4 `ProfileModal.tsx:84-88,100`): the auto-fetch on open
    // reads the SAVED profile, so a row written before the fix must not probe a
    // keyless endpoint with a key. NOTE the default is `?? true` where the
    // base-URL twin defaults `?? false`.
    const { client, seen } = recorder();
    await render(
      {
        providers: PROVIDERS,
        apiKeys: API_KEYS,
        profile: {
          id: 'cp1',
          name: 'Poisoned',
          provider: 'OLLAMA',
          modelName: 'qwen3',
          apiKeyId: 'key-anthropic',
          baseUrl: 'http://localhost:11434',
          parameters: {},
          isDefault: false,
        },
      },
      client,
    );
    expect(first(seen, 'modelFetch')['apiKeyId']).toBeUndefined();
  });

  it('sends the saved key when the saved provider does take one', async () => {
    const { client, seen } = recorder();
    await render(
      {
        providers: PROVIDERS,
        apiKeys: API_KEYS,
        profile: {
          id: 'cp1',
          name: 'Hosted',
          provider: 'ANTHROPIC',
          modelName: 'claude-sonnet-4-5-20250929',
          apiKeyId: 'key-anthropic',
          parameters: {},
          isDefault: false,
        },
      },
      client,
    );
    expect(first(seen, 'modelFetch')['apiKeyId']).toBe('key-anthropic');
  });

  it('sends an unknown saved provider’s key rather than clearing the probe', async () => {
    const { client, seen } = recorder();
    await render(
      {
        providers: [],
        apiKeys: API_KEYS,
        profile: {
          id: 'cp1',
          name: 'Hosted',
          provider: 'ANTHROPIC',
          modelName: 'claude-sonnet-4-5-20250929',
          apiKeyId: 'key-anthropic',
          parameters: {},
          isDefault: false,
        },
      },
      client,
    );
    expect(first(seen, 'modelFetch')['apiKeyId']).toBe('key-anthropic');
  });

  it('does not send the stale key when fetching models or testing a message', async () => {
    const { client, seen } = recorder();
    const fixture = await render({ providers: PROVIDERS, apiKeys: API_KEYS }, client);
    selectProvider(fixture, 'ANTHROPIC');
    selectApiKey(fixture, 'key-anthropic');
    selectProvider(fixture, 'OPENAI');
    fixture.componentInstance['setField']('modelName', 'gpt-4');

    await fixture.componentInstance['fetchModels']();
    expect(first(seen, 'modelFetch')['apiKeyId']).toBeUndefined();

    await fixture.componentInstance['testMessage']();
    const message = first(seen, 'connectionProfileTestMessage')['profile'] as Record<
      string,
      unknown
    >;
    expect(message['apiKeyId']).toBeUndefined();
  });

  it('leaves the api key untouched on a provider change (v4 `handleProviderChange`)', async () => {
    // Pinned: the value stays in form state so switching back restores it. A
    // spelling that cleared `apiKeyId` here would pass the outbound cases and
    // break this one.
    const fixture = await render({ providers: PROVIDERS, apiKeys: API_KEYS });
    selectProvider(fixture, 'ANTHROPIC');
    selectApiKey(fixture, 'key-anthropic');
    selectProvider(fixture, 'OLLAMA');
    expect(fixture.componentInstance['form']().apiKeyId).toBe('key-anthropic');
    selectProvider(fixture, 'OPENAI');
    expect(fixture.componentInstance['form']().apiKeyId).toBe('key-anthropic');
  });
});

/**
 * v4 bug 81 — a provider may take an api key without demanding one
 * (`__tests__/unit/components/settings/profile-modal-optional-api-key.test.tsx`,
 * five cases, mirrored here).
 *
 * `requiresApiKey` was answering two questions at once: "must this profile have
 * a key?" and "may it have one?". For OpenAI-Compatible the honest answers are
 * no and yes — the same provider serves an unauthenticated llama.cpp on
 * localhost and a hosted endpoint behind a bearer token — and the single flag
 * had to read `false`, which removed the key field from the form entirely.
 *
 * Driven like its Bug 76 twin above: the real modal, real dropdown gestures,
 * every dispatch captured, so the assertions are on the bytes that leave the
 * form.
 */
describe('ProfileModal optional api key (Bug 81)', () => {
  const ANTHROPIC = provider({
    id: 'ANTHROPIC',
    name: 'ANTHROPIC',
    displayName: 'Anthropic',
    configRequirements: { requiresApiKey: true, requiresBaseUrl: false },
  });
  const OAC = provider({
    id: 'OPENAI_COMPATIBLE',
    name: 'OPENAI_COMPATIBLE',
    displayName: 'OpenAI-Compatible',
    configRequirements: {
      requiresApiKey: false,
      acceptsApiKey: true,
      requiresBaseUrl: true,
      baseUrlDefault: 'http://localhost:8080/v1',
    },
  });
  const OLLAMA = provider({
    id: 'OLLAMA',
    name: 'OLLAMA',
    displayName: 'Ollama',
    configRequirements: {
      requiresApiKey: false,
      requiresBaseUrl: true,
      baseUrlDefault: 'http://localhost:11434',
    },
  });
  const PROVIDERS = [ANTHROPIC, OAC, OLLAMA];

  const API_KEYS: ApiKeyDto[] = [
    {
      id: 'key-anthropic',
      provider: 'ANTHROPIC',
      label: 'Anthropic key',
      isActive: true,
      lastUsed: null,
      createdAt: '2026-01-01T00:00:00.000Z',
      updatedAt: '2026-01-01T00:00:00.000Z',
      keyPreview: 'sk-…abcd',
    },
    {
      id: 'key-oac',
      provider: 'OPENAI_COMPATIBLE',
      label: 'Together key',
      isActive: true,
      lastUsed: null,
      createdAt: '2026-01-01T00:00:00.000Z',
      updatedAt: '2026-01-01T00:00:00.000Z',
      keyPreview: 'sk-…wxyz',
    },
  ];

  function recorder(): { client: Partial<CoreClient>; seen: CoreRequest[] } {
    const seen: CoreRequest[] = [];
    const client = stubClient({
      dispatchExpect: vi.fn(async (req: CoreRequest) => {
        seen.push(req);
        return req.type === 'modelFetch'
          ? { type: 'models', data: { models: [] } }
          : { type: 'connectionProfile', data: { profile: { id: 'new' } } };
      }) as unknown as CoreClient['dispatchExpect'],
      dispatchData: vi.fn(async (req: CoreRequest) => {
        seen.push(req);
        return { valid: true, message: 'ok', success: true };
      }) as unknown as CoreClient['dispatchData'],
    });
    return { client, seen };
  }

  function selectProvider(fixture: ComponentFixture<ProfileModal>, name: string): void {
    const select = (fixture.nativeElement as HTMLElement).querySelector<HTMLSelectElement>(
      '#qt-pf-provider',
    )!;
    select.value = name;
    select.dispatchEvent(new Event('change'));
    fixture.detectChanges();
  }

  function keySelect(fixture: ComponentFixture<ProfileModal>): HTMLSelectElement | null {
    return (fixture.nativeElement as HTMLElement).querySelector<HTMLSelectElement>('#qt-pf-key');
  }

  function keyLabel(fixture: ComponentFixture<ProfileModal>): string | null {
    const label = (fixture.nativeElement as HTMLElement).querySelector<HTMLLabelElement>(
      'label[for="qt-pf-key"]',
    );
    return label ? (label.textContent ?? '').trim() : null;
  }

  it('renders the key field unstarred for a provider that accepts but does not require one', async () => {
    const fixture = await render({ providers: PROVIDERS, apiKeys: API_KEYS });
    selectProvider(fixture, 'OPENAI_COMPATIBLE');

    expect(keySelect(fixture)).not.toBeNull();
    expect(keyLabel(fixture)).toBe('API Key');
    // …and the placeholder option says why it may be left alone.
    expect(keySelect(fixture)!.options[0].textContent?.trim()).toBe(
      'None — the endpoint needs no key',
    );
  });

  it('still stars the field, and its placeholder, for a provider that demands one', async () => {
    const fixture = await render({ providers: PROVIDERS, apiKeys: API_KEYS });
    selectProvider(fixture, 'ANTHROPIC');

    expect(keyLabel(fixture)).toBe('API Key *');
    expect(keySelect(fixture)!.options[0].textContent?.trim()).toBe('Select an API Key');
  });

  it('still hides the field for a provider that accepts no key at all', async () => {
    const fixture = await render({ providers: PROVIDERS, apiKeys: API_KEYS });
    selectProvider(fixture, 'OLLAMA');

    expect(keySelect(fixture)).toBeNull();
    expect(keyLabel(fixture)).toBeNull();
  });

  it('sends the attached key to a hosted OpenAI-compatible endpoint', async () => {
    const { client, seen } = recorder();
    const fixture = await render({ providers: PROVIDERS, apiKeys: API_KEYS }, client);
    selectProvider(fixture, 'OPENAI_COMPATIBLE');
    const select = keySelect(fixture)!;
    select.value = 'key-oac';
    select.dispatchEvent(new Event('change'));
    fixture.detectChanges();

    await fixture.componentInstance['connect']();
    const test = seen.find((r) => r.type === 'connectionProfileTest') as unknown as Record<
      string,
      unknown
    >;
    expect(test).toBeDefined();
    const profile = test['profile'] as Record<string, unknown>;
    expect(profile['provider']).toBe('OPENAI_COMPATIBLE');
    expect(profile['apiKeyId']).toBe('key-oac');
  });

  it('saves without a key, because the field is optional rather than merely present', async () => {
    const { client, seen } = recorder();
    const fixture = await render({ providers: PROVIDERS, apiKeys: API_KEYS }, client);
    selectProvider(fixture, 'OPENAI_COMPATIBLE');
    typeName(fixture, 'Local llama.cpp');
    fixture.componentInstance['setField']('modelName', 'qwen3.5-9b-q6');
    fixture.detectChanges();

    await fixture.componentInstance['submit']();
    const create = seen.find((r) => r.type === 'connectionProfileCreate') as unknown as Record<
      string,
      unknown
    >;
    const body = create['profile'] as Record<string, unknown>;
    expect(body['provider']).toBe('OPENAI_COMPATIBLE');
    expect(body['apiKeyId']).toBeNull();
  });

  it('does not carry an OpenAI-compatible key onto a provider that takes none', async () => {
    const { client, seen } = recorder();
    const fixture = await render({ providers: PROVIDERS, apiKeys: API_KEYS }, client);
    selectProvider(fixture, 'OPENAI_COMPATIBLE');
    const select = keySelect(fixture)!;
    select.value = 'key-oac';
    select.dispatchEvent(new Event('change'));
    fixture.detectChanges();

    selectProvider(fixture, 'OLLAMA');
    expect(keySelect(fixture)).toBeNull();

    typeName(fixture, 'Local');
    fixture.componentInstance['setField']('modelName', 'qwen3');
    fixture.detectChanges();

    await fixture.componentInstance['submit']();
    const create = seen.find((r) => r.type === 'connectionProfileCreate') as unknown as Record<
      string,
      unknown
    >;
    const body = create['profile'] as Record<string, unknown>;
    expect(body['provider']).toBe('OLLAMA');
    expect(body['apiKeyId']).toBeNull();
  });
});
