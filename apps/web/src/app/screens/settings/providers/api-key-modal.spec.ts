import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { ApiKeyModal } from './api-key-modal';

/**
 * dogfood-#6 regression (P4.6aa select-audit rider): the Provider `<select>`'s
 * options come from an async `providerList` query, so the selection binds
 * per-option with `[selected]` (no `[value]` on the `<select>`). A `[value]`
 * bound before the options render would leave the native control showing the
 * wrong option; `[selected]` re-evaluates as the options arrive.
 */

const PROVIDERS = [
  { name: 'ANTHROPIC', displayName: 'Anthropic', type: 'llm', configRequirements: { requiresApiKey: true } },
  { name: 'OPENAI', displayName: 'OpenAI', type: 'llm', configRequirements: { requiresApiKey: true } },
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
