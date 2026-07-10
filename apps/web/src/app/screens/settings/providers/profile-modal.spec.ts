import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { CoreRequest, ProviderInfo } from '../../../core/core-contract';
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
  inputs: { takenNames?: Set<string>; providers?: ProviderInfo[] },
  client: Partial<CoreClient> = stubClient(),
): Promise<ComponentFixture<ProfileModal>> {
  TestBed.configureTestingModule({
    imports: [ProfileModal],
    providers: [{ provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(ProfileModal);
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
