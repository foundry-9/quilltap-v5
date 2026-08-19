import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { ToastService } from '../../../ui/toast.service';
import { ApiKeyModal } from './api-key-modal';

/**
 * dogfood-#6 regression (P4.6aa select-audit rider): the Provider `<select>`'s
 * options come from an async `providerList` query, so the selection binds
 * per-option with `[selected]` (no `[value]` on the `<select>`). A `[value]`
 * bound before the options render would leave the native control showing the
 * wrong option; `[selected]` re-evaluates as the options arrive.
 */

const PROVIDERS = [
  {
    name: 'ANTHROPIC',
    displayName: 'Anthropic',
    type: 'llm',
    configRequirements: { requiresApiKey: true },
  },
  {
    name: 'OPENAI',
    displayName: 'OpenAI',
    type: 'llm',
    configRequirements: { requiresApiKey: true },
  },
];

function stubClient(): Partial<CoreClient> {
  return {
    dispatchExpect: (async () => ({
      type: 'providers',
      data: { providers: PROVIDERS, count: PROVIDERS.length },
    })) as unknown as CoreClient['dispatchExpect'],
  };
}

async function render(): Promise<ComponentFixture<ApiKeyModal>> {
  TestBed.configureTestingModule({
    imports: [ApiKeyModal],
    providers: [
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: stubClient() },
    ],
  });
  const fixture = TestBed.createComponent(ApiKeyModal);
  fixture.detectChanges();
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

describe('ApiKeyModal — provider select (dogfood-#6)', () => {
  it('binds the selection per-option with [selected], not [value]', async () => {
    const fixture = await render();
    const select = fixture.nativeElement.querySelector('#qt-key-provider') as HTMLSelectElement;

    // The async options have rendered.
    const values = Array.from(select.options).map((o) => o.value);
    expect(values).toContain('ANTHROPIC');
    expect(values).toContain('OPENAI');

    // No [value] on the <select> — the fix is per-option.
    expect(select.getAttribute('ng-reflect-value')).toBeNull();

    // Initially the placeholder ('') is selected (provider() === '').
    expect(select.value).toBe('');

    // Choosing a provider is reflected by the native control (via [selected]).
    select.value = 'ANTHROPIC';
    select.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    expect(select.value).toBe('ANTHROPIC');
    expect(Array.from(select.options).find((o) => o.value === 'ANTHROPIC')!.selected).toBe(true);
  });
});

/** v4 `ApiKeyModal.tsx:104-112` — one 4000ms toast per auto-association. */
describe('ApiKeyModal auto-association toasts', () => {
  function toasts(): { type: string; message: string; duration: number }[] {
    return TestBed.inject(ToastService)
      .toasts()
      .map((t) => ({ type: t.type, message: t.message, duration: t.duration }));
  }

  function stubCreateClient(associations?: { profileId: string; profileName: string }[]) {
    return {
      dispatchExpect: (async (req: { type: string }) => {
        if (req.type === 'providerList') {
          return { type: 'providers', data: { providers: PROVIDERS, count: PROVIDERS.length } };
        }
        return { type: 'apiKey', data: { apiKey: { id: 'k1', label: 'My Key', associations } } };
      }) as unknown as CoreClient['dispatchExpect'],
    };
  }

  async function renderWith(client: Partial<CoreClient>): Promise<ComponentFixture<ApiKeyModal>> {
    TestBed.configureTestingModule({
      imports: [ApiKeyModal],
      providers: [
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: client },
      ],
    });
    const fixture = TestBed.createComponent(ApiKeyModal);
    fixture.detectChanges();
    for (let i = 0; i < 4; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    return fixture;
  }

  async function fillAndSubmit(fixture: ComponentFixture<ApiKeyModal>): Promise<void> {
    const label = fixture.nativeElement.querySelector('#qt-key-label') as HTMLInputElement;
    label.value = 'My Key';
    label.dispatchEvent(new Event('input'));
    const select = fixture.nativeElement.querySelector('#qt-key-provider') as HTMLSelectElement;
    select.value = 'ANTHROPIC';
    select.dispatchEvent(new Event('change'));
    const key = fixture.nativeElement.querySelector('#qt-key-value') as HTMLInputElement;
    key.value = 'sk-test';
    key.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    const submit = (
      Array.from(fixture.nativeElement.querySelectorAll('button')) as HTMLButtonElement[]
    ).find((b) => b.textContent?.trim() === 'Create API Key')!;
    submit.click();
    await fixture.whenStable();
    fixture.detectChanges();
  }

  it('raises no toast when the key carries no auto-associations', async () => {
    const fixture = await renderWith(stubCreateClient(undefined));
    await fillAndSubmit(fixture);
    expect(toasts()).toEqual([]);
  });

  it('toasts one 4000ms sentence per auto-association', async () => {
    const fixture = await renderWith(
      stubCreateClient([
        { profileId: 'p1', profileName: 'Claude Default' },
        { profileId: 'p2', profileName: 'Claude Vision' },
      ]),
    );
    await fillAndSubmit(fixture);
    expect(toasts()).toEqual([
      {
        type: 'success',
        message: 'Claude Default linked to API key "My Key"',
        duration: 4000,
      },
      {
        type: 'success',
        message: 'Claude Vision linked to API key "My Key"',
        duration: 4000,
      },
    ]);
  });
});

/**
 * v4 bug 81 (`__tests__/unit/components/settings/api-key-modal.test.tsx`'s added
 * case) — the list is filtered on whether a provider *may* hold a key, not
 * whether it demands one. OpenAI-Compatible requires none (a local llama.cpp has
 * nowhere to put one) and accepts one (a hosted endpoint demands a bearer
 * token); the stricter reading left no way to create such a key at all.
 */
describe('ApiKeyModal — the provider list is filtered on "may", not "must" (bug 81)', () => {
  const SPLIT_PROVIDERS = [
    {
      name: 'OPENAI_COMPATIBLE',
      displayName: 'OpenAI-Compatible',
      type: 'llm',
      configRequirements: { requiresApiKey: false, acceptsApiKey: true },
    },
    {
      name: 'OLLAMA',
      displayName: 'Ollama',
      type: 'llm',
      configRequirements: { requiresApiKey: false },
    },
    {
      name: 'ANTHROPIC',
      displayName: 'Anthropic',
      type: 'llm',
      configRequirements: { requiresApiKey: true },
    },
  ];

  it('offers a provider that accepts a key without requiring one, and still hides one that takes none', async () => {
    TestBed.configureTestingModule({
      imports: [ApiKeyModal],
      providers: [
        provideTanStackQuery(new QueryClient()),
        {
          provide: CoreClient,
          useValue: {
            dispatchExpect: (async () => ({
              type: 'providers',
              data: { providers: SPLIT_PROVIDERS, count: SPLIT_PROVIDERS.length },
            })) as unknown as CoreClient['dispatchExpect'],
          },
        },
      ],
    });
    const fixture = TestBed.createComponent(ApiKeyModal);
    fixture.detectChanges();
    for (let i = 0; i < 4; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }

    const select = fixture.nativeElement.querySelector('#qt-key-provider') as HTMLSelectElement;
    const values = Array.from(select.options).map((o) => o.value);
    expect(values).toContain('OPENAI_COMPATIBLE');
    expect(values).toContain('ANTHROPIC');
    // Ollama declares no `acceptsApiKey`, so the fallback is its `requiresApiKey:
    // false` — never a bare `true`.
    expect(values).not.toContain('OLLAMA');
  });
});
