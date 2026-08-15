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
    const label = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('label'),
    ).find((l) =>
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
      providers: [provider({}), provider({ id: 'ANTHROPIC', name: 'ANTHROPIC', displayName: 'Anthropic' })],
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
      providers: [provider({}), provider({ id: 'ANTHROPIC', name: 'ANTHROPIC', displayName: 'Anthropic' })],
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
 * The Ollama Enable Thinking checkbox (P4.D81 unit 3; v4 `d9c5a1c7`'s
 * `optionsSchema` field, rendered here as a hardcoded provider-gated row — the
 * recorded mechanism divergence on the component).
 */
describe('ProfileModal (Ollama Enable Thinking)', () => {
  const OLLAMA = provider({
    id: 'OLLAMA',
    name: 'OLLAMA',
    displayName: 'Ollama',
    configRequirements: { requiresApiKey: false, requiresBaseUrl: true },
  });

  function thinkingBox(fixture: ComponentFixture<ProfileModal>): HTMLInputElement | null {
    return (fixture.nativeElement as HTMLElement).querySelector<HTMLInputElement>(
      '#qt-pf-enable-thinking',
    );
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

  it('is hidden for a non-Ollama provider', async () => {
    const fixture = await render({});
    expect(thinkingBox(fixture)).toBeNull();
    expect(fixture.nativeElement.textContent).not.toContain('Enable Thinking');
  });

  it('renders unchecked by default on Ollama, with v4’s help text', async () => {
    const fixture = await render({ profile: ollamaProfile({}), providers: [OLLAMA] });
    expect(thinkingBox(fixture)!.checked).toBe(false);
    expect(fixture.nativeElement.textContent).toContain('Ollama Options');
    expect(fixture.nativeElement.textContent).toContain(
      'Let thinking-capable models (Qwen3, DeepSeek-R1, and kin) reason before answering.',
    );
    expect(fixture.nativeElement.textContent).toContain(
      'any <think> blocks that leak into the reply are routed to the thinking display',
    );
  });

  it('reads a stored boolean true', async () => {
    const fixture = await render({
      profile: ollamaProfile({ enable_thinking: true }),
      providers: [OLLAMA],
    });
    expect(thinkingBox(fixture)!.checked).toBe(true);
  });

  it('reads the STRING form the wire side also accepts', async () => {
    // Contract A: `value === true || value === "true"`. v4's generic
    // BooleanField would show this OFF (`value === true`) — v5's ordered
    // divergence, recorded on the component.
    const fixture = await render({
      profile: ollamaProfile({ enable_thinking: 'true' }),
      providers: [OLLAMA],
    });
    expect(thinkingBox(fixture)!.checked).toBe(true);
  });

  it('shows any OTHER string as off — the tolerance is exact, not truthy', async () => {
    const fixture = await render({
      profile: ollamaProfile({ enable_thinking: 'yes please' }),
      providers: [OLLAMA],
    });
    expect(thinkingBox(fixture)!.checked).toBe(false);
  });

  it('writes a BOOLEAN into the bag and leaves every other key alone', async () => {
    const dispatchExpect = vi.fn(async (_req: CoreRequest) => ({
      type: 'connectionProfile',
      data: { profile: { id: 'cp-ollama' } },
    }));
    const fixture = await render(
      {
        profile: ollamaProfile({ temperature: 0.4, num_ctx: 40960, enable_thinking: 'true' }),
        providers: [OLLAMA],
      },
      stubClient({ dispatchExpect: dispatchExpect as unknown as CoreClient['dispatchExpect'] }),
    );
    const box = thinkingBox(fixture)!;
    box.checked = false;
    box.dispatchEvent(new Event('change'));
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
      enable_thinking: false,
      num_ctx: 40960,
    });
  });
});
